//! The core runtime's draws and shader creation (DXMTRS C4b-2, C4c-1): thin arms over `d3d9_rt`.
//!
//! A shader of a core device is parsed in-process and its tokens interned on the device, so a
//! draw's key compares programs by pointer. A draw resolves its [`PipelineKey`] and binds from the
//! device state with `mtld3d_core::pipeline_key`'s host-tested decisions ([`core_draw_kinds`],
//! [`core_ps_variant`], [`float_rows`], [`core_viewport`], [`pos_fixup`]); `d3d9_rt` builds the
//! pipeline on a miss and records the draw. A UP draw feeds the call's bytes; a draw from the
//! bound buffers first uploads what each buffer it reads owes its core form, and answers in
//! mtld3d's order. A draw the core does not cover yet answers `D3DERR_NOTAVAILABLE` with one
//! warning per reason.

use core::{ffi::c_void, ops::Range};

use d3d9_rt::device::{
    Bind, CoreDevice, DrawData, DrawRefused, IndexData, Indices, Stage, StreamData, TextureBind,
    view_format,
};
use mtld3d_core::{
    buffer_rename::BufferMapMode,
    convert::{
        FanRewrite, d3d_depth_bias_to_clip, d3d_slope_scale_to_metal, d3d_to_metal_cull,
        d3d_to_metal_fill, d3d_to_metal_primitive, d3dcolor_to_rgba_f32, fan_fits_pattern,
        ff_vs_layout_from_elements, index_format, vertex_count,
    },
    depth_stencil_state::{DepthStencilSnapshot, snapshot_from_state as depth_snapshot},
    dxso::{SemanticSet, VsSamplerKinds},
    ff_state::FfState,
    format::{FormatMapping, map_d3d_format},
    multisample::{alpha_to_coverage_requested, effective_sample_mask},
    pipeline_key::{
        DrawFacts, DrawFlags, PipelineKey, PsStageKey, SampledMasks, ShaderCode, StageKind,
        StreamKey, VsStageKey, core_coverage_variant, core_draw_kinds, core_extras, core_key_depth,
        core_ps_variant, core_scissor, core_srgb_variant, core_texture_variant, core_viewport,
        extra_present, float_rows, indexed_up_vertices, pos_fixup, rhw_depth_clamp, up_byte_count,
        viewport_depth_range,
    },
    pipeline_state::{PipelineRsBits, PipelineRsFlags, core_blend},
    ps_draw::build_ps_draw_bytes,
    render_scale::TargetExtent,
    sampler_state::{
        TEXTURE_LOD_SLOT, VertexLodTable, description_from_snapshot, key_from_snapshot, lod_table,
        sample_unfiltered, snapshot_from_state, srgb_texture_enabled, vertex_lod_table,
    },
    shader_create::{CoreShaderMeta, ParsedShader, core_meta, parse_shader},
    streams::{is_instance_data, offset_shift, stream_step},
    vs_draw::clip_plane_count,
};
use mtld3d_shared::{
    mtl::{self, PixelFormat},
    shader_create::ShaderStage,
    slice_from_caller,
};
use mtld3d_types::{
    D3D_OK, D3DERR_INVALIDCALL, D3DERR_NOTAVAILABLE, D3DMATRIX, D3DPT_POINTLIST, D3DPT_TRIANGLEFAN,
    D3DRS_ALPHABLENDENABLE, D3DRS_BLENDFACTOR, D3DRS_COLORWRITEENABLE, D3DRS_CULLMODE,
    D3DRS_DEPTHBIAS, D3DRS_FILLMODE, D3DRS_SCISSORTESTENABLE, D3DRS_SLOPESCALEDEPTHBIAS,
    D3DRS_SRGBWRITEENABLE, D3DRS_STENCILREF, D3DRS_ZENABLE, D3DSURFACE_DESC, MAX_STREAMS,
    RENDER_STATE_COUNT, SAMPLER_STATE_COUNT,
};

use super::{DeviceInner, refuse_under_core};
use crate::{
    com_ref::ComChild, index_buffer::Direct3DIndexBuffer9, vertex_buffer::Direct3DVertexBuffer9,
};

/// The UP draws' refusal of a layout no binding can fetch (DXMTRS C4c-2a).
///
/// An attribute wider than its stride, or more than 16 attributes under a fetch.
const UP_FETCH: &str = "IDirect3DDevice9::Draw*PrimitiveUP (a layout no binding can fetch)";
/// The buffer draws' refusal of a layout no binding can fetch (DXMTRS C4c-2a).
///
/// An attribute wider than its stride, more than 16 attributes under a fetch, or a binding
/// advanced past a crossing attribute's offset that starts past its buffer.
const BUFFER_FETCH: &str = "IDirect3DDevice9::Draw*Primitive (a layout no binding can fetch)";
/// The refusal of a draw that reads a buffer another device made (DXMTRS C4c-1).
///
/// The dirty range is one a buffer and the core's store one a device: a second device would draw
/// stale bytes and keep a store entry nothing forgets.
const OTHER_DEVICE: &str = "IDirect3DDevice9::Draw*Primitive (a buffer of another device)";

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

/// What a core draw binds.
///
/// The bytes and the stage and slot of each, empty ones left out; and its sampled slots' textures
/// and samplers, pixel and vertex.
#[derive(Default)]
struct Binds {
    bytes: Vec<u8>,
    at: Vec<(Stage, u8, Range<usize>)>,
    textures: Vec<TextureBind>,
}

/// Cached resolved pipeline key, constant bytes, texture binds, and static `DrawData` fields.
struct ResolvedCoreDraw {
    point_list: bool,
    streams: [StreamKey; MAX_STREAMS as usize],
    target_gen: u64,
    key: PipelineKey,
    binds: Binds,
    sampled_pixel_mask: u16,
    sampled_vertex_mask: u8,
    cached_pixel_lods: [u32; 16],
    cached_vertex_lods: [u32; 4],
    viewport: [f64; 6],
    scissor: [u32; 4],
    fill_mode: u8,
    cull_mode: u8,
    depth_stencil: DepthStencilSnapshot,
    stencil_ref: u8,
    slope_scale: f32,
    blend_color: Option<[f32; 4]>,
}

/// Device-owned front-end cache of the most recent core draw's resolved state.
///
/// Sits alongside `CoreDevice` on `DeviceInner`. `dirty` is invalidated on every state/target/
/// constant/sampler/texture mutator and cleared (dropping cached `TextureBind` handles without
/// resetting `key_gen`) on device reset, texture bind/evict, finalize, and release.
pub struct DrawCache {
    dirty: bool,
    key_gen: u64,
    cached: Option<ResolvedCoreDraw>,
}

impl DrawCache {
    /// Creates an empty, initially dirty draw cache with generation starting at 1.
    pub const fn new() -> Self {
        Self {
            dirty: true,
            key_gen: 1,
            cached: None,
        }
    }

    /// Marks the cache dirty so the next core draw re-resolves state.
    pub const fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Drops the cached draw entry and marks the cache dirty while preserving `key_gen`.
    pub fn clear(&mut self) {
        self.dirty = true;
        self.cached = None;
    }
}

/// What a draw feeds its vertex stage and draws: each stream, its vertices or its indices.
///
/// A UP draw's are the call's bytes at stream 0; a buffer draw's the bound buffers (DXMTRS C4c-1).
struct Feed<'a> {
    keys: [StreamKey; MAX_STREAMS as usize],
    streams: [StreamData<'a>; MAX_STREAMS as usize],
    vertex_count: u32,
    vertex_start: u32,
    base_vertex: i32,
    /// Stream 0's frequency word and its instance-data streams (bit `n`), if the draw instances.
    instancing: Option<(u32, u16)>,
    indices: Option<Indices<'a>>,
    /// The name a layout no binding can fetch is refused under: the UP draws' or the buffer draws'.
    fetch: &'static str,
}

/// The declared streams' bound vertex buffers, as a buffer draw reads them (DXMTRS C4c-1).
struct BoundStreams {
    keys: [StreamKey; MAX_STREAMS as usize],
    streams: [StreamData<'static>; MAX_STREAMS as usize],
    /// Each declared stream's buffer, null where none is bound.
    buffers: [*mut Direct3DVertexBuffer9; MAX_STREAMS as usize],
    /// Stream 0's frequency word, and the bound streams whose word is instance data (bit `n`).
    instancing: (u32, u16),
    /// Bitmask of streams holding a non-null vertex buffer pointer in `buffers`.
    present_mask: u16,
}

/// One sampled slot of a draw, pixel or vertex: its texture (null for none) and its sampler states.
struct SampledSlot {
    slot: u16,
    texture: *mut crate::texture::Direct3DTexture9,
    states: [u32; SAMPLER_STATE_COUNT],
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
}

/// Whether any sampled pixel or vertex slot holds a dirty texture or a changed LOD setting.
fn any_bound_texture_dirty_or_lod_changed(
    dev: &DeviceInner,
    sampled_pixel_mask: u16,
    sampled_vertex_mask: u8,
    cached_pixel_lods: &[u32; 16],
    cached_vertex_lods: &[u32; 4],
) -> bool {
    let bindings = dev.stage_bindings();
    let mut px_rem = sampled_pixel_mask;
    while px_rem != 0 {
        let slot = px_rem.trailing_zeros() as usize;
        px_rem &= px_rem - 1;
        let tex = bindings.texture(slot);
        if !tex.is_null() {
            // SAFETY: a bound texture, live while its stage holds it.
            let inner = unsafe { &*tex }.inner();
            if inner.has_any_dirty() || inner.lod() != cached_pixel_lods[slot] {
                return true;
            }
        }
    }
    let mut vx_rem = sampled_vertex_mask;
    while vx_rem != 0 {
        let slot = vx_rem.trailing_zeros() as usize;
        vx_rem &= vx_rem - 1;
        let tex = dev.vertex_texture(slot);
        if !tex.is_null() {
            // SAFETY: a bound texture, live while its vertex slot holds it.
            let inner = unsafe { &*tex }.inner();
            if inner.has_any_dirty() || inner.lod() != cached_vertex_lods[slot] {
                return true;
            }
        }
    }
    false
}

/// Resolves a draw's pipeline key, binds, texture snapshots, and static `DrawData` fields.
///
/// The draw has `point_list` and its streams fed as `streams` says; the caller checked that a
/// declaration (or an FVF's) is bound.
///
/// # Errors
///
/// `D3DERR_NOTAVAILABLE` for a draw the core does not cover yet ([`core_draw_kinds`]), logged
/// once per reason.
fn resolve(
    dev: &mut DeviceInner,
    streams: &[StreamKey; MAX_STREAMS as usize],
    target_gen: u64,
    point_list: bool,
) -> Result<ResolvedCoreDraw, i32> {
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
    // The pixel stages' bound textures (`cached_bound_texture_mask` is mtld3d's snapshot's, 0 here).
    let bound_mask = dev.stage_bindings().bound_mask();
    let bound = bound_mask.to_le_bytes()[0];
    let ff_ps = ps
        .is_none()
        .then(|| dev.ff_state().build_ps_key(&rs, bound));
    // The slots this draw samples: the PS's declared samplers, or the cascade's sampled stages.
    let sampled_mask = match (ps_core, &ff_ps) {
        (Some((_, meta)), _) => meta.sampler_mask,
        (None, Some(key)) => key.sampled_stage_mask(),
        (None, None) => 0,
    };
    let bindings = dev.stage_bindings();
    // The sampled pixel slots by what they hold, as mtld3d's draw folds them into its variant.
    let (fetch4, fetch4_alpha) = bindings.fetch4().masks();
    let masks = SampledMasks {
        volume: bindings.volume_sampler_mask() & sampled_mask,
        cube: bindings.cube_sampler_mask() & sampled_mask,
        depth: bindings.depth_sampler_mask() & sampled_mask,
        depth_fetch: bindings.depth_fetch_mask() & sampled_mask,
        raw_depth_red: bindings.fetch4().raw_red_mask() & sampled_mask,
        fetch4: fetch4 & sampled_mask,
        fetch4_alpha: fetch4_alpha & sampled_mask,
    };
    // Whether a sampled pixel slot holds the bound depth-stencil's texture, at any level: when
    // `samples_bound && !uses_plane` the pass omits the depth-stencil so no snapshot copy is
    // needed, and when `samples_bound && uses_plane` `d3d9-rt` blits an up-to-date snapshot of
    // the bound depth attachment for the sampler so Metal never samples an active pass attachment.
    let bound_depth = dev.core.as_deref().and_then(CoreDevice::depth);
    let samples_bound = bound_depth.is_some_and(|depth| {
        (0..16usize)
            .filter(|slot| sampled_mask & (1 << slot) != 0)
            .map(|slot| bindings.texture(slot))
            // SAFETY: a bound texture, live while its stage holds it.
            .any(|tex| !tex.is_null() && unsafe { &*tex }.texture_id() == depth.id)
    });
    // The vertex slots (s0-s3) a core vertex shader samples; none where a pre-transformed layout
    // bypasses the shader.
    let vertex_sampled = vs_core
        .filter(|_| !layout.has_rhw())
        .map_or(0, |(_, meta)| meta.sampler_mask & 0xF);
    // A depth texture by its format, as the pixel slots' mask has it (one made without the
    // DEPTHSTENCIL usage is a depth texture too).
    let vertex_depth = (0..4usize)
        .filter(|slot| vertex_sampled & (1 << slot) != 0)
        .map(|slot| dev.vertex_texture(slot))
        // SAFETY: a bound texture, live while its vertex slot holds it.
        .any(|tex| !tex.is_null() && unsafe { &*tex }.is_depth_format());
    let mut flags = DrawFlags::empty();
    flags.set(DrawFlags::VS_BOUND, vs.is_some());
    flags.set(DrawFlags::VS_CORE, vs_core.is_some());
    flags.set(DrawFlags::LAYOUT_RHW, layout.has_rhw());
    flags.set(DrawFlags::PS_BOUND, ps.is_some());
    flags.set(DrawFlags::PS_CORE, ps_core.is_some());
    flags.set(DrawFlags::VERTEX_DEPTH_TEXTURE, vertex_depth);
    let facts = DrawFacts { flags };
    let (vs_kind, ps_kind) = core_draw_kinds(facts).map_err(|refusal| {
        refuse_under_core(refusal.name());
        D3DERR_NOTAVAILABLE
    })?;
    // The bound colour targets' size and the format the draw attaches each slot of the pass set
    // in (DXMTRS C4b-3d1, C4b-3d4): the sRGB twins' under sRGB write when every member has one,
    // which then encode (C4b-3d3). The present parameters' for a device with no back buffer,
    // whose draws record nothing.
    let (target, colors, hardware_srgb) =
        core_target_of(dev, rs[D3DRS_SRGBWRITEENABLE as usize] != 0);
    let target0_extent = core_target0_extent(dev, target);
    let target_scale = target0_extent.scale();
    let [color_format, f1, f2, f3] = colors;
    // Target 0's sample count, which the pass, its pipeline and its coverage follow (DXMTRS
    // C4b-3d5).
    let sample_count = dev.core.as_deref().map_or(1, CoreDevice::target_samples);
    // The depth-stencil the draw has (DXMTRS C4b-3d2): the bound one, whatever its size, when it
    // has target 0's count; none for a draw that samples it and uses no plane of it.
    let (depth_bound, depth_format) = key_depth(dev, &rs, samples_bound, sample_count);
    let has_depth = depth_format != 0;
    let viewport = dev.viewport();
    let (px, py, pw, ph) = if viewport.width != 0 && viewport.height != 0 {
        if target_scale.is_identity() {
            (viewport.x, viewport.y, viewport.width, viewport.height)
        } else {
            target0_extent.rect(viewport.x, viewport.y, viewport.width, viewport.height)
        }
    } else {
        (0, 0, target.0, target.1)
    };
    let vp_collapsed = pw == 0 || ph == 0;
    let physical_vp = mtld3d_types::D3DVIEWPORT9 {
        x: px,
        y: py,
        width: pw.max(1),
        height: ph.max(1),
        min_z: viewport.min_z,
        max_z: viewport.max_z,
    };
    let mut binds = Binds::default();
    // A pre-transformed draw's outputs: the declaration elements its FF VS passes through.
    let mut ff_outputs = None;
    // The vertex slots this draw samples, apart from the pixel slots: they share numbers 0-3.
    let mut vertex_slots = Vec::new();
    let mut cached_vertex_lods = [0_u32; 4];
    let vs_key = if let (StageKind::Prog, Some(vs), Some((code, meta))) = (vs_kind, vs, vs_core) {
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
        // The kinds of the textures at the vertex slots, as mtld3d keys them, and the LOD table
        // where a `texldl` samples a slot whose state moves its level.
        let mut sampler_kinds = VsSamplerKinds::default();
        if vertex_sampled != 0 {
            let mut levels = VertexLodTable::new();
            for slot in (0..4usize).filter(|slot| vertex_sampled & (1 << slot) != 0) {
                let texture = dev.vertex_texture(slot);
                let mut states = dev.vertex_sampler_states[slot];
                // The spare state 0 carries the bound texture's SetLOD, none unbound.
                states[TEXTURE_LOD_SLOT] = 0;
                if !texture.is_null() {
                    // SAFETY: a bound texture, live while its vertex slot holds it.
                    let tex = unsafe { &*texture };
                    if dev.samples_unfiltered(tex) {
                        sample_unfiltered(&mut states);
                    }
                    let lod = tex.inner().lod();
                    states[TEXTURE_LOD_SLOT] = lod;
                    cached_vertex_lods[slot] = lod;
                }
                levels.set_slot(slot, &states);
                vertex_slots.push(SampledSlot {
                    slot: u16::try_from(slot).unwrap_or(0),
                    texture,
                    states,
                });
            }
            sampler_kinds = dev.vertex_texture_kinds();
            sampler_kinds.lod_table = vertex_lod_table(meta.explicit_lod_mask, levels.mask());
            if sampler_kinds.lod_table {
                binds.push(Stage::Vertex, mtl::VS_LOD_SLOT, levels.bytes());
            }
        }
        VsStageKey::Prog {
            code: code.clone(),
            clip_plane_count: clip_plane_count(&rs),
            sampler_kinds,
        }
    } else {
        // The fixed-function vertex stage: no shader bound, or a pre-transformed layout, which
        // bypasses one.
        let key = dev
            .ff_state()
            .build_vs_key(&rs, layout, bound, decl.passthrough());
        if layout.has_rhw() {
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
        } else {
            // The transforms, lights, material and texture transforms, rebuilt whole each draw
            // (C4b3c-2: dedup against the last-bound rows is C4d's).
            let rows = dev
                .ff_state()
                .ff_vs_rows(&rs, &key)
                .into_iter()
                .map(|row| row.map(f32::to_le_bytes))
                .collect::<Vec<_>>();
            binds.push(
                Stage::Vertex,
                mtl::VS_FLOAT_CONST_SLOT,
                rows.as_flattened().as_flattened(),
            );
        }
        ff_outputs = Some(SemanticSet::passthrough_outputs(&key.passthrough));
        VsStageKey::Ff(key)
    };
    let vs_outputs = ff_outputs
        .as_ref()
        .or_else(|| vs_core.map(|(_, meta)| &meta.link_outputs));
    // The depth clamp is a pre-transformed draw's alone: an untransformed fixed-function draw
    // clips, as a shader's does. It follows the bound depth-stencil, which mtld3d's draw has
    // attached also when it samples it (the key then names none here).
    let clamp_z = rhw_depth_clamp(
        layout.has_rhw(),
        depth_bound,
        rs[D3DRS_ZENABLE as usize] != 0,
    );
    let (znear, zfar) = viewport_depth_range(viewport.min_z, viewport.max_z);
    let depth_bias = d3d_depth_bias_to_clip(rs[D3DRS_DEPTHBIAS as usize], znear, zfar);
    let fixup = pos_fixup(
        &physical_vp,
        target,
        clamp_z,
        target_scale.factor(),
        depth_bias,
    )
    .map(f32::to_le_bytes);
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
    // The pixel stage's key, its linked inputs and the `oC<i>` it writes (bit `i`): the
    // fixed-function stage writes colour 0 alone.
    let (ps_key, linked, written) = match (ps_kind, ps, ps_core, ff_ps) {
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
            if meta.bump_env() {
                let bump_env = dev.ff_state().build_bump_env_bytes();
                binds.push(Stage::Pixel, mtl::PS_BUMP_ENV_SLOT, &bump_env);
            }
            let linked = vs_outputs.map_or(0, |outputs| meta.link_inputs.mask_against(outputs));
            (PsStageKey::Prog(code.clone()), linked, ps.color_out_mask())
        }
        (_, _, _, ff_ps) => {
            let key = ff_ps.unwrap_or_else(|| dev.ff_state().build_ps_key(&rs, bound));
            // TFACTOR and the stages' constants, as many rows as the cascade reads (C4b-3b).
            let mut rows = [[0u8; 16]; 9];
            let count = usize::from(key.constant_rows()).clamp(1, rows.len());
            dev.ff_state()
                .fill_ps_stage_constants(&rs, &mut rows[..count]);
            binds.push(
                Stage::Pixel,
                mtl::PS_FLOAT_CONST_SLOT,
                rows[..count].as_flattened(),
            );
            // Read only by a BUMPENVMAP stage; bound unread otherwise, as mtld3d's draws do.
            let bump_env = dev.ff_state().build_bump_env_bytes();
            binds.push(Stage::Pixel, mtl::PS_BUMP_ENV_SLOT, &bump_env);
            (PsStageKey::Ff(key), 0, 1)
        }
    };
    let mut base = dev
        .ff_state()
        .variant_key(&rs, layout.has_rhw(), dev.uses_automatic_fog());
    base.tt_projected_mask = dev.ff_state().tt_projected_mask();
    let vpos_scale = ps_kind == StageKind::Prog
        && !target_scale.is_identity()
        && ps_core.is_some_and(|(_, meta)| meta.reads_vpos);
    if vpos_scale {
        let ps_draw = build_ps_draw_bytes(target_scale);
        binds.push(Stage::Pixel, mtl::PS_DRAW_SLOT, &ps_draw);
    }
    let variant = core_ps_variant(
        base,
        ps_kind,
        point_list,
        linked,
        has_depth,
        extra_present([f1, f2, f3]),
        vpos_scale,
    );
    // The sampled pixel slots: each one's texture (flushed to the core), view and sampler, or the
    // black.
    let mut slots = Vec::new();
    let mut cached_pixel_lods = [0_u32; 16];
    let mut remaining = sampled_mask;
    while remaining != 0 {
        let slot = u16::try_from(remaining.trailing_zeros()).unwrap_or(0);
        remaining &= remaining - 1;
        let slot_idx = usize::from(slot);
        let texture = dev.stage_bindings().texture(slot_idx);
        let mut states = dev.stage_bindings().sampler_states(slot_idx);
        if !texture.is_null() {
            // SAFETY: a bound texture, live while its stage holds it.
            let tex = unsafe { &*texture };
            if dev.samples_unfiltered(tex) {
                sample_unfiltered(&mut states);
            }
            let lod = tex.inner().lod();
            states[TEXTURE_LOD_SLOT] = lod;
            cached_pixel_lods[slot_idx] = lod;
        }
        slots.push(SampledSlot {
            slot,
            texture,
            states,
        });
    }
    let texldl = ps_core.map_or(0, |(_, meta)| meta.explicit_lod_mask);
    let lod_bias = if dev.config().render_lod_bias && !target_scale.is_identity() {
        target_scale.lod_bias()
    } else {
        0.0
    };
    let table = lod_table(
        sampled_mask,
        masks.fetch4 | masks.fetch4_alpha,
        texldl,
        masks.depth,
        slots.iter().map(|s| (u32::from(s.slot), &s.states)),
        lod_bias,
    );
    if let Some(table) = &table {
        binds.push(Stage::Pixel, mtl::PS_LOD_BIAS_SLOT, &table.bytes());
    }
    let variant = core_srgb_variant(
        core_texture_variant(variant, &masks, table.is_some()),
        hardware_srgb,
    );
    // Coverage follows target 0, not the device (DXMTRS C4b-3d5): mtld3d's own predicates. The
    // sample mask narrowed against target 0's count and type (a non-maskable or single-sample
    // target ignores it); alpha-to-coverage, requested by `D3DRS_ADAPTIVETESS_Y` or the A2M
    // latch, on a multisampled target only, where it replaces the alpha test.
    let mut requested = PipelineRsBits::default();
    requested.flags.set(
        PipelineRsFlags::ALPHA_TO_COVERAGE,
        alpha_to_coverage_requested(&rs, dev.a2m_enabled()),
    );
    let alpha_to_coverage = requested.alpha_to_coverage(sample_count);
    let variant = core_coverage_variant(
        variant,
        effective_sample_mask(
            rs[mtld3d_types::D3DRS_MULTISAMPLEMASK as usize],
            sample_count,
            // A null slot 0 is the device's own back buffer, never set: its type, as mtld3d reads it.
            // SAFETY: null, or the surface bound at slot 0, which its slot's refcount keeps live.
            unsafe { dev.bound_rt().render_target(0).as_ref() }.map_or_else(
                || dev.backbuffer_multi_sample_type(),
                |surface| surface.multi_sample().multi_sample_type,
            ),
        ),
        alpha_to_coverage,
    );
    if let Some(core) = dev.core.as_deref_mut() {
        let every_slot = slots
            .iter()
            .map(|s| (Stage::Pixel, s))
            .chain(vertex_slots.iter().map(|s| (Stage::Vertex, s)));
        for (stage, s) in every_slot {
            // A pixel slot's depth texture: a shadow sample takes a compare sampler with the
            // application's filters; a raw one (INTZ, DF16, DF24) a plain point sampler.
            let bit = if matches!(stage, Stage::Pixel) {
                1u16 << s.slot
            } else {
                0
            };
            let raw_depth = masks.depth_fetch & bit != 0;
            let mut snapshot = snapshot_from_state(&s.states, masks.depth & bit != 0 && !raw_depth);
            if raw_depth {
                snapshot.force_point_filter();
            }
            let skey = key_from_snapshot(&snapshot);
            let sampler = core.sampler(skey, &description_from_snapshot(&snapshot, skey));
            let slot = u8::try_from(s.slot).unwrap_or(0);
            let bind = if s.texture.is_null() {
                None
            } else {
                // SAFETY: a bound texture, live while its stage holds it; distinct from the device.
                let tex = unsafe { &mut *s.texture };
                let id = tex.texture_id();
                if crate::texture::core_flush_dirty(tex.inner_mut(), core) {
                    core.texture_format(id).and_then(|stored| {
                        // The low bit, the predicate the sampler key reads on both runtimes.
                        let srgb = srgb_texture_enabled(&s.states);
                        let format = mtl::PixelFormat::from_repr(stored)
                            .and_then(|f| srgb.then(|| f.srgb_twin()).flatten())
                            .map_or(stored, |twin| twin as u32);
                        let swizzle = tex.inner().swizzle().map(|s| s.map(|c| c as u8));
                        core.texture_bind(
                            stage,
                            slot,
                            id,
                            view_format(format, swizzle),
                            sampler.clone(),
                        )
                    })
                } else {
                    None
                }
            };
            // The black is a colour texture: a depth slot's shader could not take it. Every depth
            // texture has a core form, so none falls back.
            debug_assert!(
                bind.is_some() || masks.depth & bit == 0,
                "a depth slot binds its depth texture"
            );
            binds
                .textures
                .push(bind.unwrap_or_else(|| core.black_bind(stage, slot, sampler)));
        }
    }
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
    // Only a blended draw asks what its targets store: each slot of the set at its own bit.
    let blending = rs[D3DRS_ALPHABLENDENABLE as usize] != 0;
    let alpha = if blending {
        (1..colors.len())
            .filter(|&slot| colors[slot] != 0 && slot_has_alpha(dev, slot))
            .fold(0_u8, |mask, slot| mask | (1 << slot))
    } else {
        0
    };
    let key = PipelineKey {
        vs: vs_key,
        ps: ps_key,
        variant,
        layout: decl.code().clone(),
        streams: *streams,
        write_mask,
        color_format,
        depth_format,
        blend: blending
            .then(|| core_blend(&rs, slot_has_alpha(dev, 0)))
            .flatten(),
        extras: core_extras([f1, f2, f3], &rs, written, alpha),
        samples: sample_count,
        alpha_to_coverage,
    };
    let blend_color = key
        .blend
        .as_ref()
        .filter(|blend| blend.reads_blend_color())
        .map(|_| d3dcolor_to_rgba_f32(rs[D3DRS_BLENDFACTOR as usize]));
    let scissor_rect = dev.scissor_rect();
    let scissor_test = rs[D3DRS_SCISSORTESTENABLE as usize] != 0;
    let scissor = if vp_collapsed {
        [0, 0, 0, 0]
    } else if scissor_test {
        let [sx, sy, sw, sh] = core_scissor(true, scissor_rect, &physical_vp, target);
        if sw == 0 || sh == 0 || target_scale.is_identity() {
            [sx, sy, sw, sh]
        } else {
            target0_extent.rect(sx, sy, sw, sh).into()
        }
    } else {
        core_scissor(false, scissor_rect, &physical_vp, target)
    };
    Ok(ResolvedCoreDraw {
        point_list,
        streams: *streams,
        target_gen,
        key,
        binds,
        sampled_pixel_mask: sampled_mask,
        sampled_vertex_mask: u8::try_from(vertex_sampled & 0xF).unwrap_or(0),
        cached_pixel_lods,
        cached_vertex_lods,
        viewport: core_viewport(&physical_vp, target),
        scissor,
        fill_mode: d3d_to_metal_fill(rs[D3DRS_FILLMODE as usize]) as u8,
        cull_mode: d3d_to_metal_cull(rs[D3DRS_CULLMODE as usize]) as u8,
        depth_stencil: depth_snapshot(&rs),
        stencil_ref: rs[D3DRS_STENCILREF as usize].to_le_bytes()[0],
        slope_scale: d3d_slope_scale_to_metal(
            rs[D3DRS_SLOPESCALEDEPTHBIAS as usize],
            target_scale.factor(),
        ),
        blend_color,
    })
}

/// Resolves bound colour target 0's `RenderScale` and logical-vs-physical extent mapping.
fn core_target0_extent(dev: &DeviceInner, physical: (u32, u32)) -> TargetExtent {
    let bound = dev.bound_rt().render_target(0);
    // SAFETY: null, or the bound surface kept live by its slot reference for this call.
    let Some(surface) = (unsafe { bound.as_ref() }) else {
        return TargetExtent::new(
            dev.render_scale(),
            (dev.backbuffer_width(), dev.backbuffer_height()),
            physical,
        );
    };
    if surface.is_back_buffer() {
        return TargetExtent::new(
            dev.render_scale(),
            (dev.backbuffer_width(), dev.backbuffer_height()),
            physical,
        );
    }
    let parent = surface.parent_texture();
    if !parent.is_null() {
        // SAFETY: `parent` is live while `surface` holds its parent reference.
        let tex = unsafe { &*parent };
        let lvl = surface.mip_level() as usize;
        return TargetExtent::new(
            tex.inner().render_scale(),
            (tex.inner().mip_width(lvl), tex.inner().mip_height(lvl)),
            physical,
        );
    }
    TargetExtent::new(
        surface.render_scale(),
        (surface.standalone_width(), surface.standalone_height()),
        physical,
    )
}

/// The bound colour targets' size, the format a draw attaches each in, and whether that encodes.
///
/// The `WMTPixelFormat`s are [`CoreDevice::target_desc`]'s, one a slot, 0 for a slot not in the
/// pass set: each target's own, or under `srgb_write` (`D3DRS_SRGBWRITEENABLE`) every one's sRGB
/// twin's when every one has one; the flag says colour 0's is the twin's (DXMTRS C4b-3d1,
/// C4b-3d3, C4b-3d4). The back buffer's from the present parameters when target 0 does not
/// resolve (a device with no back buffer, whose draws record nothing).
fn core_target_of(dev: &DeviceInner, srgb_write: bool) -> ((u32, u32), [u32; 4], bool) {
    let core = dev.core.as_deref();
    core.and_then(|core| core.target_desc(srgb_write))
        .map_or_else(
            || {
                let pp = dev.present_params();
                (
                    (pp.back_buffer_width, pp.back_buffer_height),
                    [mtl::PixelFormat::Bgra8Unorm as u32, 0, 0, 0],
                    false,
                )
            },
            |(size, formats)| {
                let stored = core.and_then(CoreDevice::target_format);
                (size, formats, srgb_write && stored != Some(formats[0]))
            },
        )
}

/// Whether the colour target at `slot` stores alpha, as mtld3d's blend factors see it.
///
/// A back buffer does, whatever its format (mtld3d binds it so), and so does an empty slot;
/// another target by its format, one the format table does not know counting as alpha-bearing
/// (DXMTRS C4b-3d3, C4b-3d4).
fn slot_has_alpha(dev: &DeviceInner, slot: usize) -> bool {
    let bound = dev.bound_rt().render_target(slot);
    // SAFETY: null, or the bound surface, which its slot's refcount keeps live for the call.
    let Some(surface) = (unsafe { bound.as_ref() }) else {
        return true;
    };
    if surface.is_back_buffer() {
        return true;
    }
    let mut desc = D3DSURFACE_DESC {
        format: 0,
        resource_type: 0,
        usage: 0,
        pool: 0,
        multi_sample_type: 0,
        multi_sample_quality: 0,
        width: 0,
        height: 0,
    };
    // SAFETY: the surface's own `GetDesc` thunk, with the surface as `this` and a writable
    // description.
    if unsafe { (surface.vtbl().get_desc)(bound.cast(), &raw mut desc) } != 0 {
        return true;
    }
    map_d3d_format(desc.format)
        .as_ref()
        .is_none_or(FormatMapping::has_alpha)
}

/// Whether a depth-stencil the pass can attach is bound, and the depth format a draw's key names.
///
/// From the device's depth-stencil and render states `rs`; `samples_bound` is whether the draw
/// samples the bound depth-stencil's texture, `samples` colour target 0's sample count. A bound
/// depth-stencil of another count is left out of the pass, as mtld3d's draw leaves it out: the
/// key names no format and the clamp sees none bound (DXMTRS C4b-3d5).
fn key_depth(
    dev: &DeviceInner,
    rs: &[u32; RENDER_STATE_COUNT],
    samples_bound: bool,
    samples: u8,
) -> (bool, u32) {
    let core = dev.core.as_deref();
    let bound = core.and_then(CoreDevice::depth_format);
    let same_samples = core.and_then(CoreDevice::depth_samples) == Some(samples);
    // What the draw's state tests or writes of the planes that depth-stencil has.
    let uses_plane = bound.is_some_and(|format| {
        let has_stencil = format == PixelFormat::Depth32FloatStencil8 as u32;
        let gated = depth_snapshot(rs).gated_on_stencil_attachment(has_stencil);
        gated.uses_depth() || gated.stencil_enable != 0
    });
    let format = core_key_depth(bound, samples_bound, uses_plane, same_samples);
    (bound.is_some() && same_samples, format)
}

/// Records a UP draw of `vertices` (and `indices`, if any) with the device state.
///
/// Its bytes go inline at the call's stride on stream 0, zeros at each other stream the
/// declaration names, as mtld3d feeds them; [`record`] does the rest.
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
    let declared = super::declared_stream_mask(dev);
    let mut keys = [StreamKey::Unused; MAX_STREAMS as usize];
    let mut streams = [StreamData::None; MAX_STREAMS as usize];
    for (stream, (key, data)) in keys.iter_mut().zip(&mut streams).enumerate() {
        if declared & (1 << stream) == 0 {
            continue;
        }
        (*key, *data) = if stream == 0 {
            (StreamKey::Inline { stride }, StreamData::Bytes(vertices))
        } else {
            (StreamKey::Null, StreamData::Zeros)
        };
    }
    let feed = Feed {
        keys,
        streams,
        vertex_count,
        vertex_start: 0,
        base_vertex: 0,
        instancing: None,
        indices,
        fetch: UP_FETCH,
    };
    record(dev, primitive, metal_primitive, feed)
}

/// Records a draw of `primitive`, as Metal's `metal_primitive`, from `feed` with the device state.
///
/// A layout no binding can fetch answers `D3DERR_NOTAVAILABLE` under `feed`'s name (logged once);
/// a draw `d3d9_rt` drops for another reason answers `D3D_OK`, as under C4a, one whose colour
/// targets, depth-stencil or buffer are not its key's or the store's logged once (unreachable
/// while the key's colours, count and depth format are `target_desc`'s, `target_samples`' and
/// `key_depth`'s, and the arms flush every buffer they read).
fn record(
    dev: &mut DeviceInner,
    primitive: u32,
    metal_primitive: mtl::PrimitiveType,
    feed: Feed<'_>,
) -> i32 {
    let Feed {
        keys,
        streams,
        vertex_count,
        vertex_start,
        base_vertex,
        instancing,
        indices,
        fetch,
    } = feed;
    let point_list = primitive == D3DPT_POINTLIST;
    let target_gen = dev.core.as_deref().map_or(0, CoreDevice::target_gen);
    let cache_hit = !dev.core_draw_cache.dirty
        && dev.core_draw_cache.cached.as_ref().is_some_and(|cached| {
            cached.point_list == point_list
                && cached.target_gen == target_gen
                && cached.streams == keys
                && !any_bound_texture_dirty_or_lod_changed(
                    dev,
                    cached.sampled_pixel_mask,
                    cached.sampled_vertex_mask,
                    &cached.cached_pixel_lods,
                    &cached.cached_vertex_lods,
                )
        });
    if !cache_hit {
        dev.core_draw_cache.clear();
        match resolve(dev, &keys, target_gen, point_list) {
            Ok(resolved) => {
                let next_gen = dev.core_draw_cache.key_gen.wrapping_add(1).max(1);
                dev.core_draw_cache.key_gen = next_gen;
                dev.core_draw_cache.cached = Some(resolved);
                dev.core_draw_cache.dirty = false;
            }
            Err(hr) => return hr,
        }
    }
    let DeviceInner {
        core_draw_cache,
        core,
        ..
    } = dev;
    let Some(core) = core.as_deref_mut() else {
        return D3D_OK;
    };
    let result = {
        let Some(resolved) = core_draw_cache.cached.as_ref() else {
            return D3D_OK;
        };
        let mut binds_buf = [const {
            Bind {
                stage: Stage::Vertex,
                slot: 0,
                bytes: &[],
            }
        }; 16];
        debug_assert!(
            resolved.binds.at.len() <= binds_buf.len(),
            "resolved constant binds fit in 16 slots"
        );
        let bind_count = resolved.binds.at.len().min(binds_buf.len());
        for (dst, (stage, slot, range)) in
            binds_buf[..bind_count].iter_mut().zip(&resolved.binds.at)
        {
            *dst = Bind {
                stage: *stage,
                slot: *slot,
                bytes: &resolved.binds.bytes[range.clone()],
            };
        }
        let draw = DrawData {
            key_gen: core_draw_cache.key_gen,
            primitive: metal_primitive as u8,
            streams,
            vertex_count,
            vertex_start,
            base_vertex,
            instancing,
            indices,
            viewport: resolved.viewport,
            scissor: resolved.scissor,
            fill_mode: resolved.fill_mode,
            cull_mode: resolved.cull_mode,
            binds: &binds_buf[..bind_count],
            textures: &resolved.binds.textures,
            depth_stencil: resolved.depth_stencil,
            stencil_ref: resolved.stencil_ref,
            slope_scale: resolved.slope_scale,
            blend_color: resolved.blend_color,
        };
        core.draw(&resolved.key, &draw)
    };
    if result.is_err() {
        core_draw_cache.clear();
    }
    match result {
        Err(DrawRefused::Fetch) => {
            refuse_under_core(fetch);
            D3DERR_NOTAVAILABLE
        }
        Err(DrawRefused::Buffer) => dropped(),
        Err(DrawRefused::Target) => {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "d3d9 core: a draw's colour targets are not its key's; the draw is dropped"
            );
            D3D_OK
        }
        Err(DrawRefused::Depth) => {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "d3d9 core: a draw's depth-stencil is not its key's; the draw is dropped"
            );
            D3D_OK
        }
        _ => D3D_OK,
    }
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
        if let Err(hr) = dev.encoder_status() {
            return Some(hr);
        }
        return Some(draw(dev, primitive, stride, &vertices, count, None));
    }
    let Some(fan) = FanRewrite::sequential(0, primitive_count) else {
        return Some(D3DERR_INVALIDCALL);
    };
    if let Err(hr) = dev.encoder_status() {
        return Some(hr);
    }
    let mut list = vec![0; fan.byte_len()];
    fan.write(&mut list);
    let indices = Indices {
        data: IndexData::Bytes(&list),
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
    let Some((index_type, index_size)) = index_format(up.index_format) else {
        return Some(D3DERR_INVALIDCALL);
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
        if let Err(hr) = dev.encoder_status() {
            return Some(hr);
        }
        let indices = Indices {
            data: IndexData::Bytes(&source),
            index_type: index_type as u8,
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
    if let Err(hr) = dev.encoder_status() {
        return Some(hr);
    }
    let mut list = vec![0; fan.byte_len()];
    fan.write(&mut list);
    let indices = Indices {
        data: IndexData::Bytes(&list),
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

/// `DrawPrimitive` on the core, after the entry point's shared checks (DXMTRS C4c-1).
///
/// `None` for a device of mtld3d's runtime. Answers in mtld3d's order: a fan first
/// ([`sequential_fan`]); then the primitive type, the vertex count and no declared stream with a
/// buffer, each `D3DERR_INVALIDCALL`. A draw that is no fan never instances; a fan off mtld3d's
/// shared pattern does ([`sequential_fan`]).
#[must_use]
pub fn draw_primitive(
    dev: &mut DeviceInner,
    primitive: u32,
    start_vertex: u32,
    primitive_count: u32,
) -> Option<i32> {
    if !dev.is_core() {
        return None;
    }
    if primitive == D3DPT_TRIANGLEFAN {
        return Some(sequential_fan(dev, start_vertex, primitive_count));
    }
    let Some(metal_primitive) = d3d_to_metal_primitive(primitive) else {
        return Some(D3DERR_INVALIDCALL);
    };
    let count = vertex_count(primitive, primitive_count);
    if count == 0 {
        return Some(D3DERR_INVALIDCALL);
    }
    if let Err(hr) = dev.latched_encoder_status() {
        return Some(hr);
    }
    let bound = match bound_streams(dev) {
        Ok(Some(bound)) => bound,
        Ok(None) => return Some(invalid("DrawPrimitive: no vertex buffer bound")),
        Err(hr) => return Some(hr),
    };
    if !flush(dev, &bound, None) {
        return Some(dropped());
    }
    let feed = Feed {
        keys: bound.keys,
        streams: bound.streams,
        vertex_count: count,
        vertex_start: start_vertex,
        base_vertex: 0,
        instancing: None,
        indices: None,
        fetch: BUFFER_FETCH,
    };
    Some(record(dev, primitive, metal_primitive, feed))
}

/// `DrawIndexedPrimitive` on the core, after the entry point's shared checks (DXMTRS C4c-1).
///
/// `None` for a device of mtld3d's runtime. Answers in mtld3d's order: a fan first
/// ([`indexed_fan`]); then the primitive type and the index count (`D3DERR_INVALIDCALL`), no
/// declared stream with a buffer (`D3D_OK`, nothing drawn, the index buffer not looked at), no
/// index buffer (`D3DERR_INVALIDCALL`). The indices are the index buffer's from `start_index`,
/// `base_vertex` added to each; the draw instances by stream 0's frequency word.
#[must_use]
pub fn draw_indexed_primitive(
    dev: &mut DeviceInner,
    primitive: u32,
    base_vertex: i32,
    start_index: u32,
    primitive_count: u32,
) -> Option<i32> {
    if !dev.is_core() {
        return None;
    }
    if primitive == D3DPT_TRIANGLEFAN {
        return Some(indexed_fan(dev, base_vertex, start_index, primitive_count));
    }
    let Some(metal_primitive) = d3d_to_metal_primitive(primitive) else {
        return Some(D3DERR_INVALIDCALL);
    };
    let count = vertex_count(primitive, primitive_count);
    if count == 0 {
        return Some(D3DERR_INVALIDCALL);
    }
    if let Err(hr) = dev.latched_encoder_status() {
        return Some(hr);
    }
    let bound = match bound_streams(dev) {
        Ok(Some(bound)) => bound,
        Ok(None) => return Some(D3D_OK),
        Err(hr) => return Some(hr),
    };
    let index = match bound_index(dev) {
        Ok(Some(index)) => index,
        Ok(None) => return Some(invalid("DrawIndexedPrimitive: no index buffer bound")),
        Err(hr) => return Some(hr),
    };
    let (format, id) = {
        // SAFETY: the bound index buffer, live while bound.
        let inner = unsafe { &*index }.inner();
        (inner.format(), inner.buffer_id())
    };
    let Some((index_type, size)) = index_format(format) else {
        return Some(invalid("DrawIndexedPrimitive: unsupported index format"));
    };
    if !flush(dev, &bound, Some(index)) {
        return Some(dropped());
    }
    let offset = u64::from(start_index) * u64::from(size);
    let feed = Feed {
        keys: bound.keys,
        streams: bound.streams,
        vertex_count: count,
        vertex_start: 0,
        base_vertex,
        instancing: Some(bound.instancing),
        indices: Some(Indices {
            data: IndexData::Buffer { id, offset },
            index_type: index_type as u8,
            count,
        }),
        fetch: BUFFER_FETCH,
    };
    Some(record(dev, primitive, metal_primitive, feed))
}

/// A `DrawPrimitive` fan, its answers in mtld3d's order.
///
/// A rewrite that cannot address it, then no declared stream with a buffer, each
/// `D3DERR_INVALIDCALL`. The list holds the fan's absolute vertex numbers, so it draws from vertex
/// 0 at base 0. A fan mtld3d's shared pattern cannot address ([`fan_fits_pattern`]) instances, as
/// mtld3d's generated list does.
fn sequential_fan(dev: &mut DeviceInner, start_vertex: u32, primitive_count: u32) -> i32 {
    let Some(fan) = FanRewrite::sequential(start_vertex, primitive_count) else {
        return super::reject_unaddressable_fan("DrawPrimitive", primitive_count);
    };
    if let Err(hr) = dev.known_encoder_status() {
        return hr;
    }
    let bound = match bound_streams(dev) {
        Ok(Some(bound)) => bound,
        Ok(None) => return invalid("triangle fan: no vertex buffer bound"),
        Err(hr) => return hr,
    };
    let generated = !fan_fits_pattern(start_vertex, primitive_count);
    fan_draw(dev, &fan, &bound, generated.then_some(bound.instancing))
}

/// A `DrawIndexedPrimitive` fan, in mtld3d's order (its `bound_index_fan`, then its draw).
///
/// No index buffer, a fan that reads past the index buffer's pages, or a rewrite that cannot
/// address it answers `D3DERR_INVALIDCALL`; then no declared stream with a buffer `D3D_OK`. The
/// rewrite folds `base_vertex` into its indices, so the list draws at base 0. The index buffer is
/// read on the CPU alone: its core form is not flushed. A buffer the core maps directly is read
/// where the last lock wrote it, its logical length the bound (DXMTRS C4c-2b).
fn indexed_fan(
    dev: &mut DeviceInner,
    base_vertex: i32,
    start_index: u32,
    primitive_count: u32,
) -> i32 {
    let index = match bound_index(dev) {
        Ok(Some(index)) => index,
        Ok(None) => return invalid("DrawIndexedPrimitive: no index buffer bound"),
        Err(hr) => return hr,
    };
    // SAFETY: the bound index buffer, live while bound; nothing writes its backing or its core
    // memory until the fan's list is written (its own flush is the vertex buffers').
    let inner = unsafe { &*index }.inner();
    let Some((_, size)) = index_format(inner.format()) else {
        return invalid("DrawIndexedPrimitive: unsupported index format");
    };
    // The fan's `primitive_count + 2` indices from `start_index`, inside the backing's pages.
    let first = usize::try_from(u64::from(start_index) * u64::from(size));
    let len = usize::try_from((u64::from(primitive_count) + 2) * u64::from(size));
    let window = first.ok().zip(len.ok());
    let copied;
    let src = if matches!(inner.map_mode(), BufferMapMode::Direct) {
        // Copied out: the draw below borrows the device the core's memory is reached through.
        copied = window.and_then(|(first, len)| {
            dev.core
                .as_deref()?
                .dynamic_copy(inner.buffer_id(), first, len)
        });
        copied.as_deref()
    } else {
        window.and_then(|(first, len)| inner.backing().get(first..)?.get(..len))
    };
    let Some(src) = src else {
        return invalid("DrawIndexedPrimitive: triangle fan reads past the index buffer");
    };
    let Some(fan) = FanRewrite::indexed(src, usize::from(size), base_vertex, primitive_count)
    else {
        return D3DERR_INVALIDCALL;
    };
    if let Err(hr) = dev.known_encoder_status() {
        return hr;
    }
    let bound = match bound_streams(dev) {
        Ok(Some(bound)) => bound,
        Ok(None) => return D3D_OK,
        Err(hr) => return hr,
    };
    fan_draw(dev, &fan, &bound, Some(bound.instancing))
}

/// Draws `fan`'s triangle list over the bound streams `bound`, once each is flushed.
fn fan_draw(
    dev: &mut DeviceInner,
    fan: &FanRewrite<'_>,
    bound: &BoundStreams,
    instancing: Option<(u32, u16)>,
) -> i32 {
    if !flush(dev, bound, None) {
        return dropped();
    }
    let mut list = vec![0; fan.byte_len()];
    fan.write(&mut list);
    let feed = Feed {
        keys: bound.keys,
        streams: bound.streams,
        vertex_count: fan.index_count(),
        vertex_start: 0,
        base_vertex: 0,
        instancing,
        indices: Some(Indices {
            data: IndexData::Bytes(&list),
            index_type: fan.index_type() as u8,
            count: fan.index_count(),
        }),
        fetch: BUFFER_FETCH,
    };
    record(dev, D3DPT_TRIANGLEFAN, mtl::PrimitiveType::Triangle, feed)
}

/// The declared streams' bound vertex buffers, keyed and fed as a buffer draw reads them.
///
/// A declared stream with none bound reads zeros; `Ok(None)` when no declared stream has one.
///
/// # Errors
///
/// `D3DERR_NOTAVAILABLE` for a buffer of another device ([`of_this_device`]).
fn bound_streams(dev: &DeviceInner) -> Result<Option<BoundStreams>, i32> {
    let declared = super::declared_stream_mask(dev);
    let bound = dev.bound_buffers();
    let mut streams = BoundStreams {
        keys: [StreamKey::Unused; MAX_STREAMS as usize],
        streams: [StreamData::None; MAX_STREAMS as usize],
        buffers: [core::ptr::null_mut(); MAX_STREAMS as usize],
        instancing: (bound.stream_freq(0), 0),
        present_mask: 0,
    };
    let mut remaining = declared;
    while remaining != 0 {
        let stream = remaining.trailing_zeros() as usize;
        remaining &= remaining - 1;
        let vb = bound.stream_vertex_buffer(stream);
        // SAFETY: null, or the stream's bound vertex buffer, live while bound.
        let Some(buffer) = (unsafe { vb.as_ref() }) else {
            streams.keys[stream] = StreamKey::Null;
            streams.streams[stream] = StreamData::Zeros;
            continue;
        };
        of_this_device(buffer, dev)?;
        let freq = bound.stream_freq(stream);
        let (step, rate) = stream_step(freq);
        let offset = bound.stream_offset(stream);
        streams.keys[stream] = StreamKey::Bound {
            stride: bound.stream_stride(stream),
            step,
            rate,
            shift: u8::try_from(offset_shift(offset)).expect("a shift is below 4"),
        };
        streams.streams[stream] = StreamData::Buffer {
            id: buffer.inner().buffer_id(),
            offset,
        };
        streams.buffers[stream] = vb;
        streams.present_mask |= 1 << stream;
        if is_instance_data(freq) {
            streams.instancing.1 |= 1 << stream;
        }
    }
    Ok((streams.present_mask != 0).then_some(streams))
}

/// The bound index buffer, `Ok(None)` for none.
///
/// # Errors
///
/// `D3DERR_NOTAVAILABLE` for one of another device ([`of_this_device`]), before its bytes are
/// read.
fn bound_index(dev: &DeviceInner) -> Result<Option<*mut Direct3DIndexBuffer9>, i32> {
    let index = dev.bound_buffers().index_buffer();
    // SAFETY: null, or the bound index buffer, live while bound.
    let Some(buffer) = (unsafe { index.as_ref() }) else {
        return Ok(None);
    };
    of_this_device(buffer, dev)?;
    Ok(Some(index))
}

/// Whether the drawing device `dev` made `buffer`.
///
/// # Errors
///
/// `D3DERR_NOTAVAILABLE` under [`OTHER_DEVICE`], logged once, for a buffer of another device:
/// before any buffer of the draw is flushed or read.
fn of_this_device(buffer: &impl ComChild, dev: &DeviceInner) -> Result<(), i32> {
    if core::ptr::eq(buffer.owning_device(), dev.device_wrapper()) {
        return Ok(());
    }
    refuse_under_core(OTHER_DEVICE);
    Err(D3DERR_NOTAVAILABLE)
}

/// Uploads what each buffer a draw reads owes its core form; false when one has none.
///
/// `bound`'s vertex buffers, and `index` for an indexed draw that reads it. A buffer the core
/// maps directly owes nothing and is skipped: the CPU wrote the memory the GPU reads (DXMTRS
/// C4c-2b).
fn flush(
    dev: &mut DeviceInner,
    bound: &BoundStreams,
    index: Option<*mut Direct3DIndexBuffer9>,
) -> bool {
    // Read before the core is borrowed: a vertex buffer's release decision takes it.
    let ignore_lock_bounds = dev.config().buffer_ignore_lock_bounds;
    let Some(core) = dev.core.as_deref_mut() else {
        return false;
    };
    let mut remaining = bound.present_mask;
    while remaining != 0 {
        let stream = remaining.trailing_zeros() as usize;
        remaining &= remaining - 1;
        let vb = bound.buffers[stream];
        // SAFETY: bit set in `present_mask` iff `vb` is a non-null bound vertex buffer,
        // live while bound and distinct from the device.
        let inner = unsafe { &mut *vb }.inner_mut();
        if !matches!(inner.map_mode(), BufferMapMode::Direct)
            && !inner.core_flush(core, ignore_lock_bounds)
        {
            return false;
        }
    }
    index.is_none_or(|ib| {
        // SAFETY: the bound index buffer, live while bound, distinct from the device.
        let inner = unsafe { &mut *ib }.inner_mut();
        matches!(inner.map_mode(), BufferMapMode::Direct) || inner.core_flush(core)
    })
}

/// The `D3D_OK` of a draw dropped because a buffer it reads has no core form, logged once.
fn dropped() -> i32 {
    mtld3d_shared::log_once_warn!(
        target: crate::LOG_TARGET,
        "d3d9 core: a draw's buffer has no core form; the draw is dropped"
    );
    D3D_OK
}

/// The `D3DERR_INVALIDCALL` of a buffer draw, logged once a reason, as mtld3d's draws log it.
#[cold]
fn invalid(reason: &'static str) -> i32 {
    mtld3d_shared::log_once_warn_by!(
        target: crate::LOG_TARGET,
        key: reason.as_ptr() as u64,
        "{reason}"
    );
    D3DERR_INVALIDCALL
}
