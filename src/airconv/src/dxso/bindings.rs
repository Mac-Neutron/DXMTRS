//! D3D9's bindings: each resource at the slot D3D9's MSL binds it to (ruling C3-7).
//!
//! The vertex stage's `c#` at buffer 30, `PosFixup` 28 and `VsDraw` 27 always, `i#` 29 and `b#`
//! 26 for dynamic constants only, the LOD table 25 with it, textures and samplers at the D3D9
//! slot: `mtld3d-shared`'s `mtl.rs` slots and `emit.rs`'s conditions, so `draw.rs` binds the
//! AIR function as it binds the MSL one. The slots are restated here because airconv links no
//! D3D9 crate (C3-23); a change to either side is a change to both.
//!
//! The lowering reads each buffer as a constant buffer of 16-byte rows ([`range`]); a buffer
//! narrower than a row (`b#`, `PosFixup`'s second row, the `float2` LOD rows) is only ever
//! read one word at a time. Samplers carry no bias (the sampler metadata is 0), as the vertex
//! MSL applies none.

use std::collections::BTreeMap;

use airconv_ir::{
    binding::{
        AddressSpace, BufferBinding, FunctionInput, FunctionSignatureDesc, MemoryAccess as Access,
        MslScalar, MslTexture, MslType, MslValue, SamplerBinding, TextureBinding, TextureKind,
    },
    dxso::{
        TextureType,
        lower::{VsBindings, range},
    },
};

use crate::{
    air::builder::{AirBuilder, MemoryAccess, SampleType},
    api::Error,
    dxbc::converter::{
        BindingMap, BufferDescriptor, ConstantBufferDescriptor, CounterDescriptor,
        SamplerDescriptor, TextureDescriptor, invalid,
    },
    llvm::{Func, Val},
};

/// `VS_FLOAT_CONST_SLOT`.
pub const VS_FLOAT_CONST_SLOT: u32 = 30;
/// `VS_INT_CONST_SLOT`.
pub const VS_INT_CONST_SLOT: u32 = 29;
/// `VS_POS_FIXUP_SLOT`.
pub const VS_POS_FIXUP_SLOT: u32 = 28;
/// `VS_DRAW_SLOT`.
pub const VS_DRAW_SLOT: u32 = 27;
/// `VS_BOOL_CONST_SLOT`.
pub const VS_BOOL_CONST_SLOT: u32 = 26;
/// `VS_LOD_SLOT`.
pub const VS_LOD_SLOT: u32 = 25;

/// The constant address space.
const CONSTANT: u32 = 2;

/// A stage's bindings: each constant range's argument, each sampler slot's texture and sampler.
pub struct D3d9Bindings {
    buffers: BTreeMap<u32, u32>,
    textures: BTreeMap<u32, (u32, u32, TextureKind)>,
}

/// Declares the buffer of `range` at `slot` and records its argument.
fn buffer(
    desc: &mut FunctionSignatureDesc,
    out: &mut D3d9Bindings,
    range: u32,
    slot: u32,
    ty: MslValue,
    name: &str,
) {
    let index = desc.define_input(FunctionInput::Buffer(BufferBinding {
        buffer_size: None,
        location_index: slot,
        array_size: 1,
        memory_access: Access::Read,
        address_space: AddressSpace::Constant,
        ty: MslType::Value(ty),
        arg_name: name.to_owned(),
        raster_order_group: None,
    }));
    out.buffers.insert(range, index);
}

/// The texture kind a slot is bound as.
const fn kind(ty: TextureType) -> TextureKind {
    match ty {
        TextureType::TextureCube => TextureKind::TextureCube,
        TextureType::Texture3D => TextureKind::Texture3d,
        TextureType::Texture2D | TextureType::Unknown => TextureKind::Texture2d,
    }
}

/// Declares the vertex stage's arguments in `emit_vs_function`'s order, under its conditions.
pub fn vertex(b: &VsBindings, desc: &mut FunctionSignatureDesc) -> D3d9Bindings {
    let mut out = D3d9Bindings {
        buffers: BTreeMap::new(),
        textures: BTreeMap::new(),
    };
    let float4 = MslValue::FLOAT4;
    buffer(
        desc,
        &mut out,
        range::FLOAT,
        VS_FLOAT_CONST_SLOT,
        float4,
        "vs_c",
    );
    buffer(
        desc,
        &mut out,
        range::POS_FIXUP,
        VS_POS_FIXUP_SLOT,
        float4,
        "pos_fixup",
    );
    buffer(
        desc,
        &mut out,
        range::VS_DRAW,
        VS_DRAW_SLOT,
        float4,
        "vs_draw",
    );
    if b.int_consts {
        let int4 = MslValue::INT4;
        buffer(desc, &mut out, range::INT, VS_INT_CONST_SLOT, int4, "vs_i");
    }
    if b.bool_consts {
        let uint = MslValue::UINT;
        buffer(
            desc,
            &mut out,
            range::BOOL,
            VS_BOOL_CONST_SLOT,
            uint,
            "vs_b",
        );
    }
    for (slot, ty) in &b.samplers {
        let slot = u32::from(*slot);
        let kind = kind(*ty);
        let texture = desc.define_input(FunctionInput::Texture(TextureBinding {
            location_index: slot,
            array_size: 1,
            memory_access: Access::Sample,
            ty: MslTexture {
                component: MslScalar::Float,
                access: Access::Sample,
                kind,
                kind_logical: kind,
            },
            arg_name: format!("s{slot}"),
            raster_order_group: None,
        }));
        let sampler = desc.define_input(FunctionInput::Sampler(SamplerBinding {
            location_index: slot,
            array_size: 1,
            arg_name: format!("samp{slot}"),
        }));
        out.textures.insert(slot, (texture, sampler, kind));
    }
    if b.lod_table {
        let float2 = MslValue::FLOAT2;
        buffer(desc, &mut out, range::VS_LOD, VS_LOD_SLOT, float2, "vs_lod");
    }
    out
}

/// Argument `index` of `function`.
fn param<'cx>(
    air: &AirBuilder<'_, 'cx>,
    function: Func<'cx>,
    index: u32,
) -> Result<Val<'cx>, Error> {
    function
        .param(air.builder().cx().tcx(), index)
        .ok_or_else(|| invalid("a D3D9 binding of no argument"))
}

/// The metadata of a texture or sampler that has none: a zero bias and level clamp.
fn no_metadata<'cx>(air: &AirBuilder<'_, 'cx>) -> Val<'cx> {
    let cx = air.builder().cx();
    cx.const_int(cx.tcx().int(64), 0, false)
}

impl<'cx> BindingMap<'cx> for D3d9Bindings {
    fn constant_buffer(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<ConstantBufferDescriptor<'cx>>, Error> {
        let Some(index) = self.buffers.get(&range) else {
            return Ok(None);
        };
        let arg = param(air, function, *index)?;
        let b = air.builder();
        let rows = b.cx().tcx().ptr(air.types().int4, CONSTANT);
        Ok(Some(ConstantBufferDescriptor {
            pointer: b.bitcast(arg, rows),
        }))
    }

    fn sampler(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<SamplerDescriptor<'cx>>, Error> {
        let Some((_, index, _)) = self.textures.get(&range) else {
            return Ok(None);
        };
        let sampler = param(air, function, *index)?;
        Ok(Some(SamplerDescriptor {
            sampler,
            cube_sampler: sampler,
            metadata: no_metadata(air),
        }))
    }

    fn srv_texture(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<TextureDescriptor<'cx>>, Error> {
        let Some((index, _, kind)) = self.textures.get(&range) else {
            return Ok(None);
        };
        Ok(Some(TextureDescriptor {
            handle: param(air, function, *index)?,
            metadata: no_metadata(air),
            global_coherent: false,
            kind: *kind,
            kind_logical: *kind,
            memory_access: MemoryAccess::Sample,
            sample_type: SampleType::Float,
            buffer_address: None,
        }))
    }

    fn uav_texture(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<TextureDescriptor<'cx>>, Error> {
        Ok(None)
    }

    fn srv_buffer(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<BufferDescriptor<'cx>>, Error> {
        Ok(None)
    }

    fn uav_buffer(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<BufferDescriptor<'cx>>, Error> {
        Ok(None)
    }

    fn uav_counter(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<CounterDescriptor<'cx>>, Error> {
        Ok(None)
    }
}
