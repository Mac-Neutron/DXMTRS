//! Bindings through D3D11's constant-buffer and argument tables.
//!
//! Ported from `A/dxbc_binding_sm50.cpp`: [`setup_binding_table2`] defines the two table
//! arguments a shader's declarations need and answers the [`Sm50Bindings`] that reads its
//! resources from them.

use std::collections::BTreeMap;

use airconv_ir::{
    binding::{
        AddressSpace, FunctionInput, FunctionSignatureDesc, IndirectBufferBinding,
        MemoryAccess as Access, MslScalar, msl_scalar, texture_kind,
    },
    initialize::DxbcShader,
    shader_info::{
        ConstantBufferInfo, ResourceType, SamplerInfo, ShaderResourceViewInfo, UavFlags,
        UnorderedAccessViewInfo,
    },
};

use super::converter::{
    BindingMap, BufferDescriptor, ConstantBufferDescriptor, CounterDescriptor, SamplerDescriptor,
    TextureDescriptor, lowered,
};
use crate::{
    abi::{SM50_BINDING_INDEX_ARGUMENT_TABLE, SM50_BINDING_INDEX_CONSTANT_BUFFER},
    air::{
        builder::{AirBuilder, MemoryAccess, SampleType},
        signature::{Handles, build_argument_buffer},
    },
    api::Error,
    llvm::{Func, Module, Val},
};

/// A D3D11 shader's bindings (`SM50BindingMap`): each resource at its argument-table field.
///
/// A table the shader does not use is `u32::MAX`.
pub struct Sm50Bindings<'s> {
    pub constant_buffer_table_index: u32,
    pub binding_table_index: u32,
    pub constant_buffers: &'s BTreeMap<u32, ConstantBufferInfo>,
    pub samplers: &'s BTreeMap<u32, SamplerInfo>,
    pub srvs: &'s BTreeMap<u32, ShaderResourceViewInfo>,
    pub uavs: &'s BTreeMap<u32, UnorderedAccessViewInfo>,
}

impl<'cx> BindingMap<'cx> for Sm50Bindings<'_> {
    fn constant_buffer(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<ConstantBufferDescriptor<'cx>>, Error> {
        if self.constant_buffer_table_index == u32::MAX {
            return Ok(None);
        }
        let Some(cb) = self.constant_buffers.get(&range) else {
            return Ok(None);
        };
        let pointer = get_argument(
            air,
            function,
            self.constant_buffer_table_index,
            cb.arg_index,
        )?;
        Ok(Some(ConstantBufferDescriptor { pointer }))
    }

    fn sampler(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<SamplerDescriptor<'cx>>, Error> {
        if self.binding_table_index == u32::MAX {
            return Ok(None);
        }
        let Some(s) = self.samplers.get(&range) else {
            return Ok(None);
        };
        let table = self.binding_table_index;
        let sampler = get_argument(air, function, table, s.arg_index)?;
        let cube_sampler = get_argument(air, function, table, s.arg_cube_index)?;
        let metadata = get_argument(air, function, table, s.arg_metadata_index)?;
        Ok(Some(SamplerDescriptor {
            sampler,
            cube_sampler,
            metadata,
        }))
    }

    fn srv_texture(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<TextureDescriptor<'cx>>, Error> {
        if self.binding_table_index == u32::MAX {
            return Ok(None);
        }
        let Some(srv) = self.srvs.get(&range) else {
            return Ok(None);
        };
        if is_buffer(&srv.resource_type) {
            return Ok(None);
        }
        let table = self.binding_table_index;
        let handle = get_argument(air, function, table, srv.arg_index)?;
        let metadata = get_argument(air, function, table, srv.arg_metadata_index)?;
        Ok(srv_texture(srv, handle, metadata))
    }

    fn uav_texture(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<TextureDescriptor<'cx>>, Error> {
        if self.binding_table_index == u32::MAX {
            return Ok(None);
        }
        let Some(uav) = self.uavs.get(&range) else {
            return Ok(None);
        };
        if is_buffer(&uav.resource_type) {
            return Ok(None);
        }
        let table = self.binding_table_index;
        let handle = get_argument(air, function, table, uav.arg_index)?;
        let metadata = get_argument(air, function, table, uav.arg_metadata_index)?;
        Ok(uav_texture(uav, handle, metadata, None))
    }

    fn srv_buffer(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<BufferDescriptor<'cx>>, Error> {
        if self.binding_table_index == u32::MAX {
            return Ok(None);
        }
        let Some(srv) = self.srvs.get(&range) else {
            return Ok(None);
        };
        if !is_buffer(&srv.resource_type) {
            return Ok(None);
        }
        let table = self.binding_table_index;
        let pointer = get_argument(air, function, table, srv.arg_index)?;
        let metadata = get_argument(air, function, table, srv.arg_metadata_index)?;
        Ok(Some(BufferDescriptor {
            pointer,
            metadata: Some(metadata),
            structure_stride: srv.structure_stride,
            global_coherent: false,
        }))
    }

    fn uav_buffer(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<BufferDescriptor<'cx>>, Error> {
        if self.binding_table_index == u32::MAX {
            return Ok(None);
        }
        let Some(uav) = self.uavs.get(&range) else {
            return Ok(None);
        };
        if !is_buffer(&uav.resource_type) {
            return Ok(None);
        }
        let table = self.binding_table_index;
        let pointer = get_argument(air, function, table, uav.arg_index)?;
        let metadata = get_argument(air, function, table, uav.arg_metadata_index)?;
        Ok(Some(BufferDescriptor {
            pointer,
            metadata: Some(metadata),
            structure_stride: uav.structure_stride,
            global_coherent: uav.flags.contains(UavFlags::GLOBAL_COHERENT),
        }))
    }

    fn uav_counter(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<CounterDescriptor<'cx>>, Error> {
        if self.binding_table_index == u32::MAX {
            return Ok(None);
        }
        let Some(uav) = self.uavs.get(&range) else {
            return Ok(None);
        };
        if !is_buffer(&uav.resource_type) {
            return Ok(None);
        }
        let pointer = get_argument(
            air,
            function,
            self.binding_table_index,
            uav.arg_counter_index,
        )?;
        Ok(Some(CounterDescriptor { pointer }))
    }
}

/// Field `index` of the structure argument `table` points to, loaded (`GetArgument`).
///
/// # Errors
///
/// An argument that is not a pointer to a structure with that field.
pub fn get_argument<'cx>(
    air: &AirBuilder<'_, 'cx>,
    function: Func<'cx>,
    table: u32,
    index: u32,
) -> Result<Val<'cx>, Error> {
    let b = air.builder();
    let tcx = b.cx().tcx();
    let not_a_table = || Error::new(b"invalid operation: a table argument is not a structure.");
    let arg = function.param(tcx, table).ok_or_else(not_a_table)?;
    let st = tcx.pointee(arg.ty()).ok_or_else(not_a_table)?;
    let field = tcx.field(st, index).ok_or_else(not_a_table)?;
    Ok(b.load(field, b.struct_gep(st, arg, index)))
}

/// A view's texel type for the AIR builder (`to_air_scaler_type`, doubles read as float).
#[must_use]
pub const fn sample_type(scalar: &airconv_ir::shader_info::ScalerDataType) -> SampleType {
    match msl_scalar(scalar) {
        Some(MslScalar::Int) => SampleType::Int,
        Some(MslScalar::Uint) => SampleType::Uint,
        _ => SampleType::Float,
    }
}

/// A shader resource view's texture description, or `None` for a buffer.
#[must_use]
pub fn srv_texture<'cx>(
    srv: &ShaderResourceViewInfo,
    handle: Val<'cx>,
    metadata: Val<'cx>,
) -> Option<TextureDescriptor<'cx>> {
    let logical = texture_kind(&srv.resource_type, srv.compared)?;
    Some(TextureDescriptor {
        handle,
        metadata,
        global_coherent: false,
        kind: lowered(logical),
        kind_logical: logical,
        memory_access: if srv.sampled {
            MemoryAccess::Sample
        } else {
            MemoryAccess::Read
        },
        sample_type: sample_type(&srv.scaler_type),
        buffer_address: None,
    })
}

/// An unordered access view's texture description, or `None` for a buffer.
#[must_use]
pub fn uav_texture<'cx>(
    uav: &UnorderedAccessViewInfo,
    handle: Val<'cx>,
    metadata: Val<'cx>,
    buffer_address: Option<Val<'cx>>,
) -> Option<TextureDescriptor<'cx>> {
    let logical = texture_kind(&uav.resource_type, false)?;
    let memory_access = match (
        uav.flags.contains(UavFlags::WRITTEN),
        uav.flags.contains(UavFlags::READ),
    ) {
        (true, true) => MemoryAccess::ReadWrite,
        (true, false) => MemoryAccess::Write,
        (false, _) => MemoryAccess::Read,
    };
    Some(TextureDescriptor {
        handle,
        metadata,
        global_coherent: uav.flags.contains(UavFlags::GLOBAL_COHERENT),
        kind: lowered(logical),
        kind_logical: logical,
        memory_access,
        sample_type: sample_type(&uav.scaler_type),
        buffer_address,
    })
}

/// Whether a view is a buffer (`NonApplicable`), not a texture.
#[must_use]
pub const fn is_buffer(ty: &ResourceType) -> bool {
    matches!(ty, ResourceType::NonApplicable)
}

/// `setup_binding_table2`: the argument table's and the constant-buffer table's inputs.
///
/// Each is defined when the shader declares something in it, at `argbuffer_slot` and
/// `argbuffer_constant_slot` (named with the slot when it is not the default).
///
/// # Errors
///
/// [`build_argument_buffer`]'s.
pub fn setup_binding_table2<'s>(
    shader: &'s DxbcShader,
    desc: &mut FunctionSignatureDesc,
    m: &Module<'_>,
    handles: &mut Handles,
    argbuffer_constant_slot: u32,
    argbuffer_slot: u32,
) -> Result<Sm50Bindings<'s>, Error> {
    let info = &shader.cfg.info;
    let mut bindings = Sm50Bindings {
        constant_buffer_table_index: u32::MAX,
        binding_table_index: u32::MAX,
        constant_buffers: &info.cbuffer_map,
        samplers: &info.sampler_map,
        srvs: &info.srv_map,
        uavs: &info.uav_map,
    };
    if !shader.binding_table.is_empty() {
        let (struct_type, struct_type_info) =
            build_argument_buffer(m, &shader.binding_table, handles)?;
        let mut arg_name = String::from("binding_table");
        if argbuffer_slot != SM50_BINDING_INDEX_ARGUMENT_TABLE {
            arg_name.push_str(&argbuffer_slot.to_string());
        }
        bindings.binding_table_index = desc.define_input(table_input(
            argbuffer_slot,
            struct_type,
            struct_type_info,
            arg_name,
        ));
    }
    if !shader.binding_table_cbuffer.is_empty() {
        let (struct_type, struct_type_info) =
            build_argument_buffer(m, &shader.binding_table_cbuffer, handles)?;
        let mut arg_name = String::from("cbuffer_table");
        if argbuffer_constant_slot != SM50_BINDING_INDEX_CONSTANT_BUFFER {
            arg_name.push_str(&argbuffer_constant_slot.to_string());
        }
        bindings.constant_buffer_table_index = desc.define_input(table_input(
            argbuffer_constant_slot,
            struct_type,
            struct_type_info,
            arg_name,
        ));
    }
    Ok(bindings)
}

/// An argument buffer's input (`ArgumentBindingIndirectBuffer`), read in the constant space.
#[must_use]
pub const fn table_input(
    location_index: u32,
    struct_type: u32,
    struct_type_info: u32,
    arg_name: String,
) -> FunctionInput {
    FunctionInput::IndirectBuffer(IndirectBufferBinding {
        location_index,
        array_size: 1,
        memory_access: Access::Read,
        address_space: AddressSpace::Constant,
        struct_type,
        struct_type_info,
        arg_name,
    })
}
