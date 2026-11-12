//! The core runtime's UP draws and shader creation (DXMTRS C4b-2): thin arms over `d3d9_rt`.
//!
//! A shader of a core device is parsed in-process and its tokens interned on the device, so a
//! draw's key compares programs by pointer. A draw resolves its [`PipelineKey`] and binds from the
//! device state with `mtld3d_core::pipeline_key`'s host-tested decisions ([`core_draw_kinds`],
//! [`core_ps_variant`], [`float_rows`], [`core_viewport`], [`pos_fixup`]); `d3d9_rt` builds the
//! pipeline on a miss and records the draw. A draw the core does not cover yet answers
//! `D3DERR_NOTAVAILABLE` with one warning per reason.

use core::{ffi::c_void, ops::Range};

use d3d9_rt::device::{Bind, DrawData, DrawRefused, Indices, Stage};
use mtld3d_core::{
    convert::{
        FanRewrite, d3d_to_metal_cull, d3d_to_metal_fill, d3d_to_metal_primitive,
        ff_vs_layout_from_elements, vertex_count,
    },
    dxso::{FfPsKey, SemanticSet},
    ff_state::FfState,
    pipeline_key::{
        DrawFacts, DrawFlags, PipelineKey, PsStageKey, ShaderCode, StageKind, VsStageKey,
        core_draw_kinds, core_ps_variant, core_viewport, float_rows, indexed_up_vertices,
        pos_fixup, up_byte_count,
    },
    shader_create::{CoreShaderMeta, ParsedShader, core_meta, parse_shader},
    vs_draw::clip_plane_count,
};
use mtld3d_shared::{mtl, shader_create::ShaderStage, slice_from_caller};
use mtld3d_types::{
    D3D_OK, D3DERR_INVALIDCALL, D3DERR_NOTAVAILABLE, D3DFMT_INDEX16, D3DFMT_INDEX32, D3DMATRIX,
    D3DPT_POINTLIST, D3DPT_TRIANGLEFAN, D3DRS_ALPHABLENDENABLE, D3DRS_COLORWRITEENABLE,
    D3DRS_CULLMODE, D3DRS_FILLMODE, D3DRS_SCISSORTESTENABLE,
};

use super::{DeviceInner, refuse_under_core};

/// The refusal of a draw whose layout crosses its stride (C4b2-8), one name for both entry points.
const CROSSING: &str = "IDirect3DDevice9::Draw*PrimitiveUP (an attribute crossing the stride)";

/// The shader `bytecode` of `stage`, parsed in-process for a core device (C4b2-3).
///
/// Its tokens are interned on the device, so a draw compares programs by pointer.
///
/// # Errors
///
/// `D3DERR_INVALIDCALL` for bytecode that does not parse or validate as a `stage` program.
pub fn create_core_shader(
    dev: &mut DeviceInner,
    bytecode: &[u32],
    stage: &ShaderStage,
) -> Result<(ParsedShader, CoreShaderMeta, ShaderCode), i32> {
    let parsed = parse_shader(stage, bytecode).map_err(|e| {
        log::debug!(target: crate::LOG_TARGET, "d3d9 core: shader creation: {e:?}");
        D3DERR_INVALIDCALL
    })?;
    let meta = core_meta(&parsed.program);
    let code = ShaderCode::intern(&mut dev.programs, bytecode);
    Ok((parsed, meta, code))
}

/// The bytes a core draw binds and the stage and slot of each, empty ones left out.
#[derive(Default)]
struct Binds {
    bytes: Vec<u8>,
    at: Vec<(Stage, u8, Range<usize>)>,
}

impl Binds {
    fn push(&mut self, stage: Stage, slot: u32, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let start = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        let slot = u8::try_from(slot).expect("a Metal buffer slot below 31");
        self.at.push((stage, slot, start..self.bytes.len()));
    }

    fn binds(&self) -> Vec<Bind<'_>> {
        self.at
            .iter()
            .map(|(stage, slot, range)| Bind {
                stage: *stage,
                slot: *slot,
                bytes: &self.bytes[range.clone()],
            })
            .collect()
    }
}

/// The pipeline key and binds of a UP draw of `primitive` with `stride` from the device state.
///
/// The caller checked that a declaration (or an FVF's) is bound.
///
/// # Errors
///
/// `D3DERR_NOTAVAILABLE` for a draw the core does not cover yet ([`core_draw_kinds`]), logged
/// once per reason.
fn resolve(
    dev: &mut DeviceInner,
    primitive: u32,
    stride: u32,
) -> Result<(PipelineKey, Binds), i32> {
    let rs = *dev.render_states();
    // SAFETY: bound (the caller checked), and its slot's refcount keeps it live for the call.
    let decl = unsafe { &*dev.vertex_decl() }.inner();
    // SAFETY: null or bound, and a bound shader's slot refcount keeps it live for the call.
    let vs = unsafe { dev.shader_bindings().vertex_shader().as_ref() };
    // SAFETY: as the vertex shader's.
    let ps = unsafe { dev.shader_bindings().pixel_shader().as_ref() };
    let layout = ff_vs_layout_from_elements(decl.elements());
    let vs_core = vs.and_then(|s| s.core());
    let ps_core = ps.and_then(|s| s.core());
    let ff_ps = ps.is_none().then(|| dev.ff_state().build_ps_key(&rs, 0));
    let mut flags = DrawFlags::empty();
    flags.set(DrawFlags::VS_BOUND, vs.is_some());
    flags.set(DrawFlags::VS_CORE, vs_core.is_some());
    flags.set(
        DrawFlags::VS_SAMPLES,
        vs_core.is_some_and(|(_, m)| m.samples()),
    );
    flags.set(DrawFlags::LAYOUT_RHW, layout.has_rhw());
    flags.set(DrawFlags::PS_BOUND, ps.is_some());
    flags.set(DrawFlags::PS_CORE, ps_core.is_some());
    flags.set(
        DrawFlags::PS_SAMPLES,
        ps_core.is_some_and(|(_, m)| m.samples()),
    );
    flags.set(
        DrawFlags::PS_BUMP_ENV,
        ps_core.is_some_and(|(_, m)| m.bump_env()),
    );
    flags.set(
        DrawFlags::BLENDING,
        rs[D3DRS_ALPHABLENDENABLE as usize] != 0,
    );
    flags.set(
        DrawFlags::SCISSOR,
        rs[D3DRS_SCISSORTESTENABLE as usize] != 0,
    );
    let facts = DrawFacts {
        flags,
        ff_ps_constant_rows: ff_ps.as_ref().map_or(0, FfPsKey::constant_rows),
    };
    let (vs_kind, ps_kind) = core_draw_kinds(facts).map_err(|refusal| {
        refuse_under_core(refusal.name());
        D3DERR_NOTAVAILABLE
    })?;
    let pp = dev.present_params();
    let target = (pp.back_buffer_width, pp.back_buffer_height);
    let viewport = dev.viewport();
    let mut binds = Binds::default();
    // A pre-transformed draw's outputs: the declaration elements its FF VS passes through.
    let mut ff_outputs = None;
    let vs_key = if let (StageKind::Prog, Some(vs), Some((code, _))) = (vs_kind, vs, vs_core) {
        let rows = float_rows(vs.max_const_used(), vs.uses_rel_const());
        let bindings = dev.shader_bindings();
        binds.push(
            Stage::Vertex,
            mtl::VS_FLOAT_CONST_SLOT,
            bindings.vs_constant_bytes(rows),
        );
        if vs.uses_int_const() {
            let ints = bindings.vs_constants_i_bytes();
            binds.push(Stage::Vertex, mtl::VS_INT_CONST_SLOT, &ints);
        }
        if vs.uses_bool_const() {
            let bools = bindings.vs_constants_b_bits().to_le_bytes();
            binds.push(Stage::Vertex, mtl::VS_BOOL_CONST_SLOT, &bools);
        }
        VsStageKey::Prog {
            code: code.clone(),
            clip_plane_count: clip_plane_count(&rs),
        }
    } else {
        // A pre-transformed layout: the fixed-function stage, a bound shader bypassed.
        let to_f32 = |n: u32| f32::from(u16::try_from(n).unwrap_or(u16::MAX));
        let mut row = [core::mem::MaybeUninit::uninit()];
        FfState::fill_xyzrhw_row(
            (
                to_f32(viewport.x),
                to_f32(viewport.y),
                to_f32(viewport.width),
                to_f32(viewport.height),
            ),
            &mut row,
        );
        // SAFETY: `fill_xyzrhw_row` wrote the one row.
        let row = unsafe { row[0].assume_init() }.map(f32::to_le_bytes);
        binds.push(Stage::Vertex, mtl::VS_FLOAT_CONST_SLOT, row.as_flattened());
        let key = dev
            .ff_state()
            .build_vs_key(&rs, layout, 0, decl.passthrough());
        ff_outputs = Some(SemanticSet::passthrough_outputs(&key.passthrough));
        VsStageKey::Ff(key)
    };
    let vs_outputs = ff_outputs
        .as_ref()
        .or_else(|| vs_core.map(|(_, meta)| &meta.link_outputs));
    let fixup = pos_fixup(&viewport, target, vs_kind == StageKind::Ff).map(f32::to_le_bytes);
    binds.push(Stage::Vertex, mtl::VS_POS_FIXUP_SLOT, fixup.as_flattened());
    let view = dev
        .ff_state()
        .transform(mtld3d_types::D3DTS_VIEW)
        .copied()
        .unwrap_or(D3DMATRIX::IDENTITY);
    let point_size = dev.point_size();
    let vs_draw = dev
        .vs_draw
        .build_bytes(&rs, point_size, &view, &dev.clip_planes);
    binds.push(Stage::Vertex, mtl::VS_DRAW_SLOT, &vs_draw);
    let (ps_key, linked) = match (ps_kind, ps, ps_core, ff_ps) {
        (StageKind::Prog, Some(ps), Some((code, meta)), _) => {
            let rows = float_rows(ps.max_const_used(), ps.uses_rel_const());
            let bindings = dev.shader_bindings();
            binds.push(
                Stage::Pixel,
                mtl::PS_FLOAT_CONST_SLOT,
                bindings.ps_constant_bytes(rows),
            );
            if ps.uses_int_const() {
                let ints = bindings.ps_constants_i_bytes();
                binds.push(Stage::Pixel, mtl::PS_INT_CONST_SLOT, &ints);
            }
            if ps.uses_bool_const() {
                let bools = bindings.ps_constants_b_bits().to_le_bytes();
                binds.push(Stage::Pixel, mtl::PS_BOOL_CONST_SLOT, &bools);
            }
            let linked = vs_outputs.map_or(0, |outputs| meta.link_inputs.mask_against(outputs));
            (PsStageKey::Prog(code.clone()), linked)
        }
        (_, _, _, ff_ps) => {
            let constants = dev.ff_state().build_ps_constants(&rs);
            binds.push(Stage::Pixel, mtl::PS_FLOAT_CONST_SLOT, &constants);
            let key = ff_ps.unwrap_or_else(|| dev.ff_state().build_ps_key(&rs, 0));
            (PsStageKey::Ff(key), 0)
        }
    };
    let mut base = dev
        .ff_state()
        .variant_key(&rs, layout.has_rhw(), dev.uses_automatic_fog());
    base.tt_projected_mask = dev.ff_state().tt_projected_mask();
    let variant = core_ps_variant(
        base,
        ps_kind == StageKind::Prog,
        primitive == D3DPT_POINTLIST,
        linked,
    );
    // Empty when the alpha test or fog is off: not bound.
    let (alpha_ref, alpha_len) =
        mtld3d_core::draw_data::build_alpha_ref_bytes(variant, dev.ff_state().alpha_ref(&rs));
    binds.push(
        Stage::Pixel,
        mtl::PS_ALPHA_REF_SLOT,
        &alpha_ref[..alpha_len],
    );
    let (fog, fog_len) = mtld3d_core::ff_state::build_fog_color_bytes(&rs, variant);
    binds.push(Stage::Pixel, mtl::PS_FOG_SLOT, &fog[..fog_len]);
    // D3D9's colour write bits (R 1, G 2, B 4, A 8) as Metal's (R 8, G 4, B 2, A 1).
    let enabled = rs[D3DRS_COLORWRITEENABLE as usize];
    let write_mask = [(1, 8), (2, 4), (4, 2), (8, 1)]
        .iter()
        .filter(|(d3d, _)| enabled & d3d != 0)
        .fold(0_u8, |mask, (_, metal)| mask | metal);
    let key = PipelineKey {
        vs: vs_key,
        ps: ps_key,
        variant,
        layout: decl.code().clone(),
        stride,
        write_mask,
    };
    Ok((key, binds))
}

/// Records a UP draw of `vertices` (and `indices`, if any) with the device state.
///
/// A crossing layout answers `D3DERR_NOTAVAILABLE` (logged once); a draw `d3d9_rt` drops for
/// another reason answers `D3D_OK`, as under C4a.
fn draw(
    dev: &mut DeviceInner,
    primitive: u32,
    stride: u32,
    vertices: &[u8],
    vertex_count: u32,
    indices: Option<Indices<'_>>,
) -> i32 {
    let metal_primitive = if primitive == D3DPT_TRIANGLEFAN {
        // A fan arrives as its triangle list.
        Some(mtld3d_shared::mtl::PrimitiveType::Triangle)
    } else {
        d3d_to_metal_primitive(primitive)
    };
    let Some(metal_primitive) = metal_primitive else {
        return D3DERR_INVALIDCALL;
    };
    let (key, binds) = match resolve(dev, primitive, stride) {
        Ok(resolved) => resolved,
        Err(hr) => return hr,
    };
    let binds = binds.binds();
    let pp = dev.present_params();
    let rs = dev.render_states();
    let draw = DrawData {
        primitive: metal_primitive as u8,
        vertices,
        vertex_count,
        indices,
        viewport: core_viewport(
            &dev.viewport(),
            (pp.back_buffer_width, pp.back_buffer_height),
        ),
        fill_mode: d3d_to_metal_fill(rs[D3DRS_FILLMODE as usize]) as u8,
        cull_mode: d3d_to_metal_cull(rs[D3DRS_CULLMODE as usize]) as u8,
        binds: &binds,
    };
    let Some(core) = dev.core.as_deref_mut() else {
        return D3D_OK;
    };
    if core.draw_up(&key, &draw) == Err(DrawRefused::Crossing) {
        refuse_under_core(CROSSING);
        return D3DERR_NOTAVAILABLE;
    }
    D3D_OK
}

/// `DrawPrimitiveUP` on the core, after the entry point's shared checks (C4b2-7).
///
/// `None` for a device of mtld3d's runtime. A fan draws as an indexed triangle list.
pub fn draw_primitive_up(
    dev: &mut DeviceInner,
    primitive: u32,
    primitive_count: u32,
    data: *const c_void,
    stride: u32,
) -> Option<i32> {
    if !dev.is_core() {
        return None;
    }
    let count = vertex_count(primitive, primitive_count);
    let Some(byte_count) = up_byte_count(stride, count) else {
        return Some(D3DERR_INVALIDCALL);
    };
    if data.is_null() || byte_count == 0 {
        return Some(D3DERR_INVALIDCALL);
    }
    // SAFETY: per the DrawPrimitiveUP ABI, `data` holds `count` vertices `stride` bytes apart,
    // read here; alignment is not assumed.
    let vertices = unsafe { slice_from_caller(data.cast::<u8>(), byte_count) };
    if primitive != D3DPT_TRIANGLEFAN {
        return Some(draw(dev, primitive, stride, &vertices, count, None));
    }
    let Some(fan) = FanRewrite::sequential(0, primitive_count) else {
        return Some(D3DERR_INVALIDCALL);
    };
    let mut list = vec![0; fan.byte_len()];
    fan.write(&mut list);
    let indices = Indices {
        bytes: &list,
        index_type: fan.index_type() as u8,
        count: fan.index_count(),
    };
    Some(draw(
        dev,
        primitive,
        stride,
        &vertices,
        count,
        Some(indices),
    ))
}

/// A `DrawIndexedPrimitiveUP`'s arguments, as the entry point received them.
pub struct IndexedUp {
    pub primitive: u32,
    pub min_vertex_index: u32,
    pub num_vertices: u32,
    pub primitive_count: u32,
    pub index_data: *const c_void,
    pub index_format: u32,
    pub vertex_data: *const c_void,
    pub stride: u32,
}

/// `DrawIndexedPrimitiveUP` on the core, after the entry point's shared checks (C4b2-7).
///
/// `None` for a device of mtld3d's runtime. The vertices are `[0, min_vertex_index +
/// num_vertices)`, the indices absolute; a fan draws as an indexed triangle list.
pub fn draw_indexed_primitive_up(dev: &mut DeviceInner, up: &IndexedUp) -> Option<i32> {
    if !dev.is_core() {
        return None;
    }
    let (index_type, index_size) = match up.index_format {
        D3DFMT_INDEX16 => (0_u8, 2_u8),
        D3DFMT_INDEX32 => (1, 4),
        _ => return Some(D3DERR_INVALIDCALL),
    };
    let vertex_total = indexed_up_vertices(up.min_vertex_index, up.num_vertices);
    let Some(byte_count) = vertex_total.and_then(|total| up_byte_count(up.stride, total)) else {
        return Some(D3DERR_INVALIDCALL);
    };
    let count = vertex_count(up.primitive, up.primitive_count);
    let Some(index_bytes) = up_byte_count(u32::from(index_size), count) else {
        return Some(D3DERR_INVALIDCALL);
    };
    if byte_count == 0 || index_bytes == 0 {
        return Some(D3DERR_INVALIDCALL);
    }
    let vertex_total = vertex_total.unwrap_or_default();
    // SAFETY: per the DrawIndexedPrimitiveUP ABI, `vertex_data` holds the vertices up to
    // `min_vertex_index + num_vertices`, `stride` bytes apart; alignment is not assumed.
    let vertices = unsafe { slice_from_caller(up.vertex_data.cast::<u8>(), byte_count) };
    // SAFETY: per the same ABI, `index_data` holds the draw's `count` indices of `index_size`
    // bytes; alignment is not assumed.
    let source = unsafe { slice_from_caller(up.index_data.cast::<u8>(), index_bytes) };
    let (primitive, stride) = (up.primitive, up.stride);
    if primitive != D3DPT_TRIANGLEFAN {
        let indices = Indices {
            bytes: &source,
            index_type,
            count,
        };
        return Some(draw(
            dev,
            primitive,
            stride,
            &vertices,
            vertex_total,
            Some(indices),
        ));
    }
    let Some(fan) = FanRewrite::indexed(&source, usize::from(index_size), 0, up.primitive_count)
    else {
        return Some(D3DERR_INVALIDCALL);
    };
    let mut list = vec![0; fan.byte_len()];
    fan.write(&mut list);
    let indices = Indices {
        bytes: &list,
        index_type: fan.index_type() as u8,
        count: fan.index_count(),
    };
    Some(draw(
        dev,
        primitive,
        stride,
        &vertices,
        vertex_total,
        Some(indices),
    ))
}
