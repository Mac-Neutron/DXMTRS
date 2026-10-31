//! Bindings through a D3D12 root signature.
//!
//! Ported from `A/dxbc_binding_rootsig.cpp`: [`setup_binding_rootsig`] lays the root signature's
//! parameters out as the root argument buffer (descriptor tables, root constants and root
//! descriptors, then the heap and lock-table qwords this fork adds) and answers the
//! [`RootSignatureBindings`] that reads each declared resource through it. The DXIL front end
//! binds through the same map.

use std::collections::BTreeMap;

use airconv_ir::{
    binding::{
        AddressSpace, ArgumentTable, BufferBinding, FunctionInput, FunctionSignatureDesc,
        MemoryAccess as Access, MslScalar, MslType, MslValue, texture_kind,
    },
    container::Container,
    root_signature::{self, Payload, RootSignature},
    shader_info::{
        ConstantBufferInfo, ResourceRange, SamplerInfo, ShaderFlags, ShaderInfo,
        ShaderResourceViewInfo, UavFlags, UnorderedAccessViewInfo,
    },
    tokens::program_type,
};

use super::{
    binding_sm50::{get_argument, is_buffer, srv_texture, table_input, uav_texture},
    converter::{
        BindingMap, BufferDescriptor, ConstantBufferDescriptor, CounterDescriptor,
        SamplerDescriptor, TextureDescriptor, air_kind, lowered,
    },
};
use crate::{
    abi::Sm50ShaderRootSignatureData,
    air::{
        builder::AirBuilder,
        signature::{Handles, build_argument_buffer},
    },
    api::Error,
    llvm::{Func, Module, Ty, Val},
};

/// A resource read through a root descriptor (`~0u`): a buffer pointer, no table.
pub const ROOT_DESCRIPTOR: u32 = u32::MAX;
/// A constant buffer of root constants (`~0u - 1`).
pub const ROOT_CONSTANTS: u32 = u32::MAX - 1;
/// `D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND`.
const OFFSET_APPEND: u32 = u32::MAX;
/// The two `…_HEAP_DIRECTLY_INDEXED` root signature flags (CBV/SRV/UAV, sampler).
const HEAP_DIRECTLY_INDEXED: u32 = 0xC00;
/// The register space of a shader's heap-indexed ranges.
const HEAP_SPACE: u32 = u32::MAX;
/// `D3D12_DESCRIPTOR_RANGE_TYPE_*`.
const RANGE_SRV: u32 = 0;
const RANGE_UAV: u32 = 1;
const RANGE_CBV: u32 = 2;
/// `D3D12_SHADER_VISIBILITY_*`.
const VISIBILITY_ALL: u32 = 0;
const VISIBILITY_VERTEX: u32 = 1;
const VISIBILITY_HULL: u32 = 2;
const VISIBILITY_DOMAIN: u32 = 3;
const VISIBILITY_GEOMETRY: u32 = 4;
const VISIBILITY_PIXEL: u32 = 5;

/// A declared resource bound by the root signature.
///
/// `arg_index` is its root argument field; `offset` where its descriptor sits
/// ([`ROOT_DESCRIPTOR`], [`ROOT_CONSTANTS`], else a table's descriptor offset).
pub struct Bound<'s, T> {
    pub info: &'s T,
    pub arg_index: u32,
    pub offset: u32,
}

/// A shader's bindings through a root signature (`RootSignatureBindingMap`).
pub struct RootSignatureBindings<'s> {
    pub root_signature_argument_index: u32,
    pub static_sampler_argument_index: u32,
    pub constant_buffers: BTreeMap<u32, Bound<'s, ConstantBufferInfo>>,
    pub samplers: BTreeMap<u32, Bound<'s, SamplerInfo>>,
    pub srvs: BTreeMap<u32, Bound<'s, ShaderResourceViewInfo>>,
    pub uavs: BTreeMap<u32, Bound<'s, UnorderedAccessViewInfo>>,
    /// DXIL's 64-bit atomics: the root argument field of the lock table, `u32::MAX` for none.
    pub atomic64_locks: u32,
}

impl<'cx> BindingMap<'cx> for RootSignatureBindings<'_> {
    fn constant_buffer(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<ConstantBufferDescriptor<'cx>>, Error> {
        let root = self.root_signature_argument_index;
        if root == u32::MAX {
            return Ok(None);
        }
        let Some(cb) = self.constant_buffers.get(&range) else {
            return Ok(None);
        };
        let pointer = match cb.offset {
            ROOT_CONSTANTS => {
                // `GetRootConstantPointer`: the constants' field itself, as `uint4`s.
                let b = air.builder();
                let (arg, st) = table_arg(air, function, root)?;
                let field = b.struct_gep(st, arg, cb.arg_index);
                b.pointer_cast(field, b.cx().tcx().ptr(air.int_ty(4), 2))
            }
            ROOT_DESCRIPTOR => get_argument(air, function, root, cb.arg_index)?,
            offset => {
                let heap = get_argument(air, function, root, cb.arg_index)?;
                let tcx = air.builder().cx().tcx();
                let handle_ty = tcx.ptr(air.int_ty(4), 2);
                let (pointer, metadata) = buffer_descriptor(
                    air,
                    heap,
                    index,
                    handle_ty,
                    cb.info.range.lower_bound,
                    offset,
                );
                return Ok(Some(ConstantBufferDescriptor {
                    pointer,
                    metadata: Some(metadata),
                }));
            }
        };
        Ok(Some(ConstantBufferDescriptor {
            pointer,
            metadata: None,
        }))
    }

    fn sampler(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<SamplerDescriptor<'cx>>, Error> {
        let Some(s) = self.samplers.get(&range) else {
            return Ok(None);
        };
        if s.offset == ROOT_DESCRIPTOR {
            if self.static_sampler_argument_index == u32::MAX {
                return Ok(None);
            }
            let tcx = air.builder().cx().tcx();
            let samplers = function
                .param(tcx, self.static_sampler_argument_index)
                .ok_or_else(|| Error::new(b"invalid operation: no static samplers argument."))?;
            return Ok(Some(sampler_descriptor(
                air,
                samplers,
                index,
                s.info.range.lower_bound,
                s.arg_index,
            )));
        }
        if self.root_signature_argument_index == u32::MAX {
            return Ok(None);
        }
        let heap = get_argument(
            air,
            function,
            self.root_signature_argument_index,
            s.arg_index,
        )?;
        Ok(Some(sampler_descriptor(
            air,
            heap,
            index,
            s.info.range.lower_bound,
            s.offset,
        )))
    }

    fn srv_texture(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<TextureDescriptor<'cx>>, Error> {
        if self.root_signature_argument_index == u32::MAX {
            return Ok(None);
        }
        let Some(srv) = self.srvs.get(&range) else {
            return Ok(None);
        };
        if is_buffer(&srv.info.resource_type) {
            return Ok(None);
        }
        let heap = get_argument(
            air,
            function,
            self.root_signature_argument_index,
            srv.arg_index,
        )?;
        if srv.offset == ROOT_DESCRIPTOR {
            // A root descriptor can only be a buffer.
            return Ok(None);
        }
        let Some(logical) = texture_kind(&srv.info.resource_type, srv.info.compared) else {
            return Ok(None);
        };
        let handle_ty = air.texture_handle_type(air_kind(lowered(logical)));
        let (handle, metadata) = texture_descriptor(
            air,
            heap,
            index,
            handle_ty,
            srv.info.range.lower_bound,
            srv.offset,
            0,
        );
        Ok(srv_texture(srv.info, handle, metadata))
    }

    fn uav_texture(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<TextureDescriptor<'cx>>, Error> {
        if self.root_signature_argument_index == u32::MAX {
            return Ok(None);
        }
        let Some(uav) = self.uavs.get(&range) else {
            return Ok(None);
        };
        if is_buffer(&uav.info.resource_type) {
            return Ok(None);
        }
        let heap = get_argument(
            air,
            function,
            self.root_signature_argument_index,
            uav.arg_index,
        )?;
        if uav.offset == ROOT_DESCRIPTOR {
            // A root descriptor can only be a buffer.
            return Ok(None);
        }
        let Some(logical) = texture_kind(&uav.info.resource_type, false) else {
            return Ok(None);
        };
        let handle_ty = air.texture_handle_type(air_kind(lowered(logical)));
        let lower = uav.info.range.lower_bound;
        // MacNeutron: an atomic view's R32Uint view is the descriptor's third qword.
        let field = if uav.info.flags.contains(UavFlags::ATOMIC_VIEW) {
            2
        } else {
            0
        };
        let (handle, metadata) =
            texture_descriptor(air, heap, index, handle_ty, lower, uav.offset, field);
        // MacNeutron: an address64 view's buffer base, the descriptor's fourth qword.
        let buffer_address = uav.info.flags.contains(UavFlags::ADDRESS64).then(|| {
            let tcx = air.builder().cx().tcx();
            let byte_ptr = tcx.ptr(air.byte_ty(), 1);
            texture_descriptor(air, heap, index, byte_ptr, lower, uav.offset, 3).0
        });
        Ok(uav_texture(uav.info, handle, metadata, buffer_address))
    }

    fn srv_buffer(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<BufferDescriptor<'cx>>, Error> {
        if self.root_signature_argument_index == u32::MAX {
            return Ok(None);
        }
        let Some(srv) = self.srvs.get(&range) else {
            return Ok(None);
        };
        if !is_buffer(&srv.info.resource_type) {
            return Ok(None);
        }
        let heap = get_argument(
            air,
            function,
            self.root_signature_argument_index,
            srv.arg_index,
        )?;
        Ok(Some(buffer_of(
            air,
            heap,
            index,
            srv.info.range.lower_bound,
            srv.offset,
            srv.info.structure_stride,
            false,
        )))
    }

    fn uav_buffer(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<BufferDescriptor<'cx>>, Error> {
        if self.root_signature_argument_index == u32::MAX {
            return Ok(None);
        }
        let Some(uav) = self.uavs.get(&range) else {
            return Ok(None);
        };
        if !is_buffer(&uav.info.resource_type) {
            return Ok(None);
        }
        let heap = get_argument(
            air,
            function,
            self.root_signature_argument_index,
            uav.arg_index,
        )?;
        Ok(Some(buffer_of(
            air,
            heap,
            index,
            uav.info.range.lower_bound,
            uav.offset,
            uav.info.structure_stride,
            uav.info.flags.contains(UavFlags::GLOBAL_COHERENT),
        )))
    }

    fn uav_counter(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<CounterDescriptor<'cx>>, Error> {
        if self.root_signature_argument_index == u32::MAX {
            return Ok(None);
        }
        let Some(uav) = self.uavs.get(&range) else {
            return Ok(None);
        };
        if !is_buffer(&uav.info.resource_type) {
            return Ok(None);
        }
        let heap = get_argument(
            air,
            function,
            self.root_signature_argument_index,
            uav.arg_index,
        )?;
        if uav.offset == ROOT_DESCRIPTOR {
            // A root descriptor has no counter.
            return Ok(None);
        }
        let b = air.builder();
        let tcx = b.cx().tcx();
        let long = tcx.int(64);
        let counter_ty = tcx.ptr(air.int_ty(1), 1);
        let descriptor_ty = tcx.literal_struct(&[long, long, counter_ty, long], false);
        let idx = descriptor_index(air, index, uav.info.range.lower_bound, uav.offset);
        let cast = b.pointer_cast(heap, tcx.ptr(descriptor_ty, 2));
        let field = b.gep(descriptor_ty, cast, &[idx, air.get_int(2)]);
        Ok(Some(CounterDescriptor {
            pointer: b.load(counter_ty, field),
        }))
    }

    fn atomic64_locks(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
    ) -> Result<Option<Val<'cx>>, Error> {
        if self.atomic64_locks == u32::MAX {
            return Ok(None);
        }
        get_argument(
            air,
            function,
            self.root_signature_argument_index,
            self.atomic64_locks,
        )
        .map(Some)
    }
}

/// The bytes of a compile argument's root signature container.
///
/// # Safety
///
/// `data.bytecode` points to `data.bytecode_length` readable bytes that live as long as `data`
/// (`SM50Compile`'s contract for its argument chain), or is null.
#[must_use]
pub const unsafe fn root_signature_bytes(data: &Sm50ShaderRootSignatureData) -> &[u8] {
    if data.bytecode.is_null() {
        return &[];
    }
    // SAFETY: the caller's contract.
    unsafe { core::slice::from_raw_parts(data.bytecode.cast(), data.bytecode_length) }
}

/// A root signature blob's `D3D12_ROOT_SIGNATURE_FLAGS` (`RootSignatureFlags`).
///
/// 0 when the blob cannot be read.
#[must_use]
pub fn root_signature_flags(bytecode: &[u8]) -> u32 {
    read_root_signature(bytecode).map_or(0, |rs| rs.v1_1.flags)
}

/// `setup_binding_rootsig`: the root argument buffer of a root signature.
///
/// `bytecode` is a container holding an `RTS0` part; the buffer is defined as the `rootsig`
/// input at `slots.0`, and the static samplers' buffer at `slots.1` when it has some.
///
/// # Errors
///
/// A container without a readable root signature (the C++ asserted and crashed on the null map
/// it answered), a descriptor range of an unknown type, [`build_argument_buffer`]'s.
pub fn setup_binding_rootsig<'s>(
    info: &'s ShaderInfo,
    desc: &mut FunctionSignatureDesc,
    m: &Module<'_>,
    handles: &mut Handles,
    shader_type: u32,
    bytecode: &[u8],
    slots: (u32, u32),
) -> Result<RootSignatureBindings<'s>, Error> {
    let (root_sig_slot, static_sampler_slot) = slots;
    let rs = read_root_signature(bytecode)?;
    let root_sig = &rs.v1_1;
    let mut map = RootSignatureBindings {
        root_signature_argument_index: u32::MAX,
        static_sampler_argument_index: u32::MAX,
        constant_buffers: BTreeMap::new(),
        samplers: BTreeMap::new(),
        srvs: BTreeMap::new(),
        uavs: BTreeMap::new(),
        atomic64_locks: u32::MAX,
    };
    let mut table = ArgumentTable::default();
    let mut next_attribute: u32 = 0;
    let mut attribute = || {
        let index = next_attribute;
        next_attribute = next_attribute.wrapping_add(1);
        index
    };
    let visible = |visibility: u32| visibility_matches(visibility, shader_type);

    for (i, parameter) in (0_u32..).zip(&root_sig.parameters) {
        match &parameter.payload {
            Payload::Table(ranges) => {
                let table_index = buffer(
                    &mut table,
                    format!("table{i}"),
                    AddressSpace::Constant,
                    Access::Read,
                    MslValue::Scalar(MslScalar::Int),
                    attribute(),
                );
                if !visible(parameter.visibility) {
                    continue;
                }
                let mut descriptor_offset: u32 = 0;
                for range in ranges {
                    let offset = if range.offset == OFFSET_APPEND {
                        descriptor_offset
                    } else {
                        range.offset
                    };
                    descriptor_offset = offset.wrapping_add(range.num_descriptors);
                    let bound = |r: &ResourceRange| {
                        (r.space == range.space && range_contained(r, range)).then(|| {
                            offset.wrapping_add(r.lower_bound.wrapping_sub(range.base_register))
                        })
                    };
                    match range.range_type {
                        RANGE_SRV => bind_range(
                            &mut map.srvs,
                            &info.srv_map,
                            table_index,
                            |s| bound(&s.range),
                            |s| s.range.range_id,
                        ),
                        RANGE_UAV => bind_range(
                            &mut map.uavs,
                            &info.uav_map,
                            table_index,
                            |s| bound(&s.range),
                            |s| s.range.range_id,
                        ),
                        RANGE_CBV => bind_range(
                            &mut map.constant_buffers,
                            &info.cbuffer_map,
                            table_index,
                            |s| bound(&s.range),
                            |s| s.range.range_id,
                        ),
                        root_signature::RANGE_TYPE_SAMPLER => bind_range(
                            &mut map.samplers,
                            &info.sampler_map,
                            table_index,
                            |s| bound(&s.range),
                            |s| s.range.range_id,
                        ),
                        _ => {
                            return Err(Error::new(
                                b"Invalid DXBC bytecode: a descriptor range of an unknown type",
                            ));
                        }
                    }
                }
            }
            Payload::Constants(constants) => {
                let qwords = (constants.values >> 1) + (constants.values & 1);
                let first = table.define_integer64(format!("root_constant{i}"), attribute());
                for x in 1..qwords {
                    table.define_integer64(format!("root_constant{i}_{x}"), attribute());
                }
                if !visible(parameter.visibility) {
                    continue;
                }
                bind_range(
                    &mut map.constant_buffers,
                    &info.cbuffer_map,
                    first,
                    |c| {
                        (c.range.lower_bound == constants.register
                            && c.range.space == constants.space)
                            .then_some(ROOT_CONSTANTS)
                    },
                    |c| c.range.range_id,
                );
            }
            Payload::Cbv(descriptor) => {
                let index = buffer(
                    &mut table,
                    format!("cb{i}"),
                    AddressSpace::Constant,
                    Access::Read,
                    MslValue::UINT4,
                    attribute(),
                );
                if !visible(parameter.visibility) {
                    continue;
                }
                bind_range(
                    &mut map.constant_buffers,
                    &info.cbuffer_map,
                    index,
                    |c| root_bound(&c.range, descriptor.register, descriptor.space),
                    |c| c.range.range_id,
                );
            }
            Payload::Srv(descriptor) => {
                let index = buffer(
                    &mut table,
                    format!("t{i}"),
                    AddressSpace::Device,
                    Access::Read,
                    MslValue::UINT,
                    attribute(),
                );
                if !visible(parameter.visibility) {
                    continue;
                }
                bind_range(
                    &mut map.srvs,
                    &info.srv_map,
                    index,
                    |s| root_bound(&s.range, descriptor.register, descriptor.space),
                    |s| s.range.range_id,
                );
            }
            Payload::Uav(descriptor) => {
                let index = buffer(
                    &mut table,
                    format!("u{i}"),
                    AddressSpace::Device,
                    Access::ReadWrite,
                    MslValue::UINT,
                    attribute(),
                );
                if !visible(parameter.visibility) {
                    continue;
                }
                bind_range(
                    &mut map.uavs,
                    &info.uav_map,
                    index,
                    |u| root_bound(&u.range, descriptor.register, descriptor.space),
                    |u| u.range.range_id,
                );
            }
            Payload::Unknown(_) => {}
        }
    }

    // Dynamic resources (MacNeutron): with CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED or
    // SAMPLER_HEAP_DIRECTLY_INDEXED, two qwords follow the last parameter's: the CBV/SRV/UAV
    // heap's GPU descriptor base, then the sampler heap's. Only a shader that indexes a heap
    // declares them (its heap ranges are in register space ~0u, lower bound 0); each heap range
    // reads its heap from descriptor 0, so a heap index is a descriptor index. DXIL 64-bit
    // atomics: the lock table's address is the qword after those (declared then even in a shader
    // that indexes no heap), or after the last parameter's without the flags.
    let atomic64 = info.flags.contains(ShaderFlags::USE_ATOMIC64_LOCKS);
    let in_heap = info.srv_map.values().any(|s| s.range.space == HEAP_SPACE)
        || info.uav_map.values().any(|s| s.range.space == HEAP_SPACE)
        || info
            .cbuffer_map
            .values()
            .any(|s| s.range.space == HEAP_SPACE)
        || info
            .sampler_map
            .values()
            .any(|s| s.range.space == HEAP_SPACE);
    if root_sig.flags & HEAP_DIRECTLY_INDEXED != 0 && (in_heap || atomic64) {
        let heap = buffer(
            &mut table,
            "heap".into(),
            AddressSpace::Constant,
            Access::Read,
            MslValue::Scalar(MslScalar::Int),
            attribute(),
        );
        let sampler_heap = buffer(
            &mut table,
            "sampler_heap".into(),
            AddressSpace::Constant,
            Access::Read,
            MslValue::Scalar(MslScalar::Int),
            attribute(),
        );
        let heap_bound = |r: &ResourceRange| (r.space == HEAP_SPACE).then_some(0);
        bind_range(
            &mut map.srvs,
            &info.srv_map,
            heap,
            |s| heap_bound(&s.range),
            |s| s.range.range_id,
        );
        bind_range(
            &mut map.uavs,
            &info.uav_map,
            heap,
            |s| heap_bound(&s.range),
            |s| s.range.range_id,
        );
        bind_range(
            &mut map.constant_buffers,
            &info.cbuffer_map,
            heap,
            |s| heap_bound(&s.range),
            |s| s.range.range_id,
        );
        bind_range(
            &mut map.samplers,
            &info.sampler_map,
            sampler_heap,
            |s| heap_bound(&s.range),
            |s| s.range.range_id,
        );
    }
    if atomic64 {
        map.atomic64_locks = buffer(
            &mut table,
            "atomic64_locks".into(),
            AddressSpace::Device,
            Access::ReadWrite,
            MslValue::UINT,
            attribute(),
        );
    }

    let (struct_type, struct_type_info) = build_argument_buffer(m, &table, handles)?;
    map.root_signature_argument_index = desc.define_input(table_input(
        root_sig_slot,
        struct_type,
        struct_type_info,
        "rootsig".into(),
    ));

    if rs.static_samplers.is_empty() {
        return Ok(map);
    }
    for (i, state) in (0_u32..).zip(&rs.static_samplers) {
        for sampler in info.sampler_map.values() {
            if sampler.range.space != state.space || sampler.range.lower_bound != state.register {
                continue;
            }
            if !visible(state.visibility) {
                break;
            }
            map.samplers.insert(
                sampler.range.range_id,
                Bound {
                    info: sampler,
                    arg_index: i,
                    offset: ROOT_DESCRIPTOR,
                },
            );
        }
    }
    map.static_sampler_argument_index = desc.define_input(FunctionInput::Buffer(BufferBinding {
        buffer_size: None,
        location_index: static_sampler_slot,
        array_size: 0,
        memory_access: Access::Read,
        address_space: AddressSpace::Constant,
        ty: MslType::Value(MslValue::ULONG),
        arg_name: "static_samplers".into(),
        raster_order_group: None,
    }));
    Ok(map)
}

/// The root signature of a container's `RTS0` part.
fn read_root_signature(bytecode: &[u8]) -> Result<RootSignature, Error> {
    let container = Container::parse(bytecode)?;
    let blob = root_signature::from_container(&container)?;
    Ok(RootSignature::deserialize(blob)?)
}

/// Whether a parameter of `visibility` reaches a shader of `shader_type`.
const fn visibility_matches(visibility: u32, shader_type: u32) -> bool {
    match visibility {
        VISIBILITY_ALL => true,
        VISIBILITY_VERTEX => shader_type == program_type::VERTEX,
        VISIBILITY_HULL => shader_type == program_type::HULL,
        VISIBILITY_DOMAIN => shader_type == program_type::DOMAIN,
        VISIBILITY_GEOMETRY => shader_type == program_type::GEOMETRY,
        VISIBILITY_PIXEL => shader_type == program_type::PIXEL,
        _ => false,
    }
}

/// Whether a declared range lies in a descriptor range (`range_contained`).
const fn range_contained(r: &ResourceRange, range: &root_signature::Range) -> bool {
    if r.lower_bound < range.base_register {
        return false;
    }
    if range.num_descriptors == u32::MAX || r.size == u32::MAX {
        return true;
    }
    r.lower_bound.wrapping_add(r.size) <= range.base_register.wrapping_add(range.num_descriptors)
}

/// A root descriptor's binding of a declared range at its register.
fn root_bound(r: &ResourceRange, register: u32, space: u32) -> Option<u32> {
    (r.lower_bound == register && r.space == space).then_some(ROOT_DESCRIPTOR)
}

/// Binds each declaration `offset` places, at field `arg_index`, under its range id.
///
/// A later binding of one range replaces an earlier one, as the C++'s `operator[]` assignment.
fn bind_range<'s, T>(
    map: &mut BTreeMap<u32, Bound<'s, T>>,
    declared: &'s BTreeMap<u32, T>,
    arg_index: u32,
    offset: impl Fn(&T) -> Option<u32>,
    range_id: impl Fn(&T) -> u32,
) {
    for info in declared.values() {
        if let Some(offset) = offset(info) {
            map.insert(
                range_id(info),
                Bound {
                    info,
                    arg_index,
                    offset,
                },
            );
        }
    }
}

/// `ArgumentBufferBuilder::DefineBuffer` of a root argument.
fn buffer(
    table: &mut ArgumentTable,
    name: String,
    address_space: AddressSpace,
    access: Access,
    value: MslValue,
    location_index: u32,
) -> u32 {
    table.define_buffer(
        name,
        address_space,
        access,
        MslType::Value(value),
        location_index,
        None,
    )
}

/// The root argument structure pointer and its structure type.
fn table_arg<'cx>(
    air: &AirBuilder<'_, 'cx>,
    function: Func<'cx>,
    table: u32,
) -> Result<(Val<'cx>, Ty), Error> {
    let tcx = air.builder().cx().tcx();
    let not_a_table = || Error::new(b"invalid operation: a table argument is not a structure.");
    let arg = function.param(tcx, table).ok_or_else(not_a_table)?;
    let st = tcx.pointee(arg.ty()).ok_or_else(not_a_table)?;
    Ok((arg, st))
}

/// The descriptor's index in its heap: `index - range + offset`.
fn descriptor_index<'cx>(
    air: &AirBuilder<'_, 'cx>,
    index: Val<'cx>,
    range: u32,
    offset: u32,
) -> Val<'cx> {
    let b = air.builder();
    b.add(b.sub(index, air.get_int(range)), air.get_int(offset))
}

/// A buffer descriptor's pointer of `handle_ty` and its metadata (`GetBufferDescriptor`).
fn buffer_descriptor<'cx>(
    air: &AirBuilder<'_, 'cx>,
    heap: Val<'cx>,
    index: Val<'cx>,
    handle_ty: Ty,
    range: u32,
    offset: u32,
) -> (Val<'cx>, Val<'cx>) {
    let b = air.builder();
    let tcx = b.cx().tcx();
    let long = tcx.int(64);
    let descriptor_ty = tcx.literal_struct(&[handle_ty, long, long, long], false);
    let idx = descriptor_index(air, index, range, offset);
    let field = |i: u32, ty: Ty| {
        let cast = b.pointer_cast(heap, tcx.ptr(descriptor_ty, 2));
        b.load(ty, b.gep(descriptor_ty, cast, &[idx, air.get_int(i)]))
    };
    let pointer = field(0, handle_ty);
    (pointer, field(1, long))
}

/// A root descriptor's buffer (no metadata but its full length), or a table's descriptor.
fn buffer_of<'cx>(
    air: &AirBuilder<'_, 'cx>,
    heap: Val<'cx>,
    index: Val<'cx>,
    range: u32,
    offset: u32,
    structure_stride: u32,
    global_coherent: bool,
) -> BufferDescriptor<'cx> {
    let b = air.builder();
    let tcx = b.cx().tcx();
    if offset == ROOT_DESCRIPTOR {
        return BufferDescriptor {
            pointer: heap,
            metadata: Some(b.cx().const_int(tcx.int(64), 0xffff_ffff, false)),
            structure_stride,
            global_coherent,
        };
    }
    let handle_ty = tcx.ptr(air.int_ty(1), 1);
    let (pointer, metadata) = buffer_descriptor(air, heap, index, handle_ty, range, offset);
    BufferDescriptor {
        pointer,
        metadata: Some(metadata),
        structure_stride,
        global_coherent,
    }
}

/// A sampler descriptor: the sampler, the cube sampler and the metadata (`GetSamplerDescriptor`).
fn sampler_descriptor<'cx>(
    air: &AirBuilder<'_, 'cx>,
    heap: Val<'cx>,
    index: Val<'cx>,
    range: u32,
    offset: u32,
) -> SamplerDescriptor<'cx> {
    let b = air.builder();
    let tcx = b.cx().tcx();
    let long = tcx.int(64);
    let sampler_ty = air.sampler_handle_type();
    let descriptor_ty = tcx.literal_struct(&[sampler_ty, sampler_ty, long, long], false);
    let idx = descriptor_index(air, index, range, offset);
    let field = |i: u32, ty: Ty| {
        let cast = b.pointer_cast(heap, tcx.ptr(descriptor_ty, 2));
        b.load(ty, b.gep(descriptor_ty, cast, &[idx, air.get_int(i)]))
    };
    let sampler = field(0, sampler_ty);
    let cube_sampler = field(1, sampler_ty);
    SamplerDescriptor {
        sampler,
        cube_sampler,
        metadata: field(2, long),
    }
}

/// A texture descriptor's handle and metadata (`GetTextureDescriptor`).
///
/// The handle is field `handle_field`, read as `handle_ty`.
fn texture_descriptor<'cx>(
    air: &AirBuilder<'_, 'cx>,
    heap: Val<'cx>,
    index: Val<'cx>,
    handle_ty: Ty,
    range: u32,
    offset: u32,
    handle_field: u32,
) -> (Val<'cx>, Val<'cx>) {
    let b = air.builder();
    let tcx = b.cx().tcx();
    let long = tcx.int(64);
    let descriptor_ty = tcx.literal_struct(&[handle_ty, long, long, long], false);
    let idx = descriptor_index(air, index, range, offset);
    let cast = b.pointer_cast(heap, tcx.ptr(descriptor_ty, 2));
    let field = b.gep(descriptor_ty, cast, &[idx, air.get_int(handle_field)]);
    let handle = b.load(handle_ty, b.pointer_cast(field, tcx.ptr(handle_ty, 2)));
    let cast = b.pointer_cast(heap, tcx.ptr(descriptor_ty, 2));
    let metadata = b.load(long, b.gep(descriptor_ty, cast, &[idx, air.get_int(1)]));
    (handle, metadata)
}
