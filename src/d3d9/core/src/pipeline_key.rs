//! One pipeline key for D3D9's fixed-function and programmable stages on DXMT's core.
//!
//! The core runtime caches its pipelines by [`PipelineKey`], which the front end builds on every
//! draw: each stage is fixed-function (its FF key) or programmable (its tokens), and the layout is
//! the bound declaration's elements. A key is cheap to compare: a program's and a declaration's
//! codes are built once, at creation, and compare by pointer first. Only a miss calls
//! [`pipeline_source`], which turns a key into what its pipeline is built from. The draw decisions
//! the front end makes before it builds a key are here too, host-tested: which stages a draw takes
//! ([`core_draw_kinds`]), the pixel variant ([`core_ps_variant`]), the float rows a stage binds
//! ([`float_rows`]) and the viewport ([`core_viewport`]).

use std::{
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

use airconv_ir::dxso::spec::{StageSpec, VariantFlags, VariantKey, VsSamplerKinds, VsSpec};
use mtld3d_shared::shader_create::ShaderStage;
use mtld3d_types::{D3DRECT, D3DVERTEXELEMENT9, D3DVIEWPORT9};
use rustc_hash::FxHashMap;

use crate::{
    convert::{
        InputSemantic, hash_elements, is_decl_end, resolve_attrs_for_ff, resolve_attrs_for_vs,
    },
    dxso::{FfPsKey, FfVsKey},
    ff_build,
    ids::ProgramId,
    shader_create::{ShaderCreateError, parse_shader},
};

/// A program's tokens and their id, compared exactly.
///
/// It hashes by the id alone and is equal to another when both share the tokens' `Arc` or carry
/// the same id and the same tokens, so a forged or colliding id never binds another program's
/// pipeline. A device builds one per program at creation, so a draw's compare is a pointer's.
#[derive(Clone, Debug)]
pub struct ShaderCode {
    id: ProgramId,
    tokens: Arc<[u32]>,
}

impl ShaderCode {
    /// The program `tokens` whose id is `id`.
    #[must_use]
    pub const fn from_parts(id: ProgramId, tokens: Arc<[u32]>) -> Self {
        Self { id, tokens }
    }

    /// The program `tokens`, sharing the `Arc` of a live program of the same tokens in `programs`.
    ///
    /// Its id is the tokens' hash, so one `Arc` never carries two ids. An entry whose program
    /// is gone or holds other tokens under the same id is replaced.
    pub fn intern(programs: &mut FxHashMap<ProgramId, Weak<[u32]>>, tokens: &[u32]) -> Self {
        let id = ProgramId::from_tokens(tokens);
        let live = programs
            .get(&id)
            .and_then(Weak::upgrade)
            .filter(|kept| **kept == *tokens);
        let tokens = live.unwrap_or_else(|| {
            let fresh = Arc::<[u32]>::from(tokens);
            programs.insert(id, Arc::downgrade(&fresh));
            fresh
        });
        Self { id, tokens }
    }

    /// The program's id.
    #[must_use]
    pub const fn id(&self) -> ProgramId {
        self.id
    }

    /// The program's tokens.
    #[must_use]
    pub fn tokens(&self) -> &[u32] {
        &self.tokens
    }

    /// The program's tokens, shared.
    #[must_use]
    pub fn shared_tokens(&self) -> Arc<[u32]> {
        Arc::clone(&self.tokens)
    }
}

impl PartialEq for ShaderCode {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.tokens, &other.tokens)
            || (self.id == other.id && self.tokens == other.tokens)
    }
}

impl Eq for ShaderCode {}

impl Hash for ShaderCode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// A vertex declaration's elements and their hash, compared exactly.
///
/// Built once when the declaration is created; compared as [`ShaderCode`] is.
#[derive(Clone, Debug)]
pub struct DeclCode {
    hash: u64,
    with_end: Arc<[D3DVERTEXELEMENT9]>,
}

impl DeclCode {
    /// The declaration `with_end`, its `D3DDECL_END` last.
    #[must_use]
    pub fn new(with_end: Arc<[D3DVERTEXELEMENT9]>) -> Self {
        Self {
            hash: hash_elements(&with_end),
            with_end,
        }
    }

    /// The elements without `D3DDECL_END`.
    #[must_use]
    pub fn elements(&self) -> &[D3DVERTEXELEMENT9] {
        match self.with_end.split_last() {
            Some((end, elements)) if is_decl_end(end) => elements,
            _ => &self.with_end,
        }
    }
}

impl PartialEq for DeclCode {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.with_end, &other.with_end)
            || (self.hash == other.hash && self.with_end == other.with_end)
    }
}

impl Eq for DeclCode {}

impl Hash for DeclCode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

/// The vertex stage of a pipeline.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum VsStageKey {
    /// The fixed-function vertex stage of this key.
    Ff(FfVsKey),
    /// The application's program.
    ///
    /// With the user clip planes it writes and the kinds of the textures at its vertex samplers.
    Prog {
        code: ShaderCode,
        clip_plane_count: u8,
        sampler_kinds: VsSamplerKinds,
    },
}

/// The pixel stage of a pipeline.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PsStageKey {
    /// The fixed-function pixel stage of this key.
    Ff(FfPsKey),
    /// The application's program.
    Prog(ShaderCode),
}

/// What a pipeline of the core's draws depends on.
///
/// The arm draws to one one-sample colour target and at most one depth-stencil, with blending
/// refused, so these fields are all of it. The stride is here because D3D9 fetches through a
/// Metal vertex descriptor.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    pub vs: VsStageKey,
    pub ps: PsStageKey,
    /// The pixel stage's specialization.
    pub variant: VariantKey,
    /// The bound declaration (the FVF's for an FVF draw).
    pub layout: DeclCode,
    /// Bytes from one vertex to the next.
    pub stride: u32,
    /// The colour target's `MTLColorWriteMask`.
    pub write_mask: u8,
    /// The colour target's `WMTPixelFormat`.
    pub color_format: u32,
    /// The attached depth-stencil's `WMTPixelFormat`; 0 for a draw without one.
    pub depth_format: u32,
}

/// One stage as it is translated: its program and its specialization.
#[derive(Debug, PartialEq, Eq)]
pub struct StageSource {
    pub code: ShaderCode,
    pub spec: StageSpec,
}

/// What a pipeline is built from: [`pipeline_source`] of its key.
#[derive(Debug, PartialEq, Eq)]
pub struct PipelineSource {
    pub vs: StageSource,
    pub ps: StageSource,
    /// Each vertex attribute: `(attribute index, MTLVertexFormat, byte offset)`, from buffer 0.
    pub attrs: Vec<(u32, u32, u32)>,
    /// The end of the furthest attribute on stream 0.
    pub stream0_extent: u32,
    /// Bytes from one vertex to the next.
    pub stride: u32,
    /// The colour target's `MTLColorWriteMask`.
    pub write_mask: u8,
    /// The colour target's `WMTPixelFormat`.
    pub color_format: u32,
    /// The attached depth-stencil's `WMTPixelFormat`; 0 for a draw without one.
    pub depth_format: u32,
}

impl PipelineSource {
    /// Whether an attribute ends past the stride, reading into the next vertex.
    ///
    /// Such a layout needs mtld3d's slot remap, which the core does not have yet.
    #[must_use]
    pub const fn crosses_stride(&self) -> bool {
        self.stream0_extent > self.stride
    }
}

/// The fixed-function program `tokens`.
fn ff_code(tokens: Arc<[u32]>) -> ShaderCode {
    ShaderCode::from_parts(ProgramId::from_tokens(&tokens), tokens)
}

/// What the pipeline of `key` is built from.
///
/// # Errors
///
/// When a programmable vertex stage's tokens do not parse as a vertex shader.
pub fn pipeline_source(key: &PipelineKey) -> Result<PipelineSource, ShaderCreateError> {
    // Every field by name: a field added to the key is a compile error here, the one place a key
    // becomes a pipeline, until it is read.
    let PipelineKey {
        vs,
        ps,
        variant,
        layout,
        stride,
        write_mask,
        color_format,
        depth_format,
    } = key;
    let elements = layout.elements();
    let (vs_code, declared, clip_plane_count, sampler_kinds, resolved) = match vs {
        VsStageKey::Ff(vs) => {
            let built = ff_build::vertex(vs);
            let resolved = resolve_attrs_for_ff(elements, &vs.passthrough);
            (
                ff_code(built.tokens),
                built.provided_input_mask,
                vs.clip_plane_count,
                // The fixed-function vertex stage samples nothing.
                VsSamplerKinds::default(),
                resolved,
            )
        }
        VsStageKey::Prog {
            code,
            clip_plane_count,
            sampler_kinds,
        } => {
            let parsed = parse_shader(&ShaderStage::Vertex, code.tokens())?;
            let semantics = parsed
                .input_semantics
                .iter()
                .map(|s| InputSemantic {
                    usage: s.usage,
                    usage_index: s.usage_index,
                    register_index: s.register_index,
                })
                .collect::<Vec<_>>();
            let resolved = resolve_attrs_for_vs(elements, &semantics);
            // A program declares what it reads; the layout alone decides what is provided.
            (
                code.clone(),
                u16::MAX,
                *clip_plane_count,
                *sampler_kinds,
                resolved,
            )
        }
    };
    // A UP draw feeds buffer 0 alone: elements on other streams back no input, which then reads
    // the translator's zero fill, as mtld3d's unbound stream reads zeros.
    let kept = resolved
        .attrs
        .iter()
        .filter(|a| a.buffer_index == 0)
        .collect::<Vec<_>>();
    let kept_mask = kept
        .iter()
        .filter_map(|a| 1_u16.checked_shl(a.attr_index))
        .fold(0, |mask, bit| mask | bit);
    let ps_code = match ps {
        PsStageKey::Ff(ps) => ff_code(ff_build::pixel(ps).tokens),
        PsStageKey::Prog(code) => code.clone(),
    };
    Ok(PipelineSource {
        vs: StageSource {
            code: vs_code,
            spec: StageSpec::Vs(VsSpec {
                provided_input_mask: declared & kept_mask,
                clip_plane_count,
                sampler_kinds,
            }),
        },
        ps: StageSource {
            code: ps_code,
            spec: StageSpec::Ps(*variant),
        },
        attrs: kept
            .iter()
            .map(|a| (a.attr_index, a.format as u32, a.offset))
            .collect(),
        stream0_extent: resolved.extents[0],
        stride: *stride,
        write_mask: *write_mask,
        color_format: *color_format,
        depth_format: *depth_format,
    })
}

/// The pixel variant of a core draw from the API thread's `base`.
///
/// What mtld3d's encoder patches per draw, for the core's one colour target, one sample, no
/// render scale and no samplers: a programmable stage exports colour 0 alone, and its depth
/// exactly when the draw has a depth-stencil attached (`has_depth`); `POINT_SPRITE` holds on
/// point lists only, and the stage reads the extra inputs of `linked_input_mask`. `SRGB_WRITE`
/// stays: the target has no sRGB view, so the shader encodes.
#[must_use]
pub fn core_ps_variant(
    base: VariantKey,
    ps_is_prog: bool,
    point_list: bool,
    linked_input_mask: u8,
    has_depth: bool,
) -> VariantKey {
    let mut variant = VariantKey {
        linked_input_mask,
        sample_mask: 0,
        ..base
    };
    variant
        .flags
        .remove(VariantFlags::LOD_BIAS | VariantFlags::VPOS_SCALE | VariantFlags::SAMPLE_MASK);
    if !point_list {
        variant.flags.remove(VariantFlags::POINT_SPRITE);
    }
    if ps_is_prog {
        variant.color_out_mask = 0;
        variant
            .flags
            .set(VariantFlags::NO_DEPTH_ATTACHMENT, !has_depth);
    }
    variant
}

/// The sampled pixel slots of a core draw by what they hold, one bit a slot (C4b-3b, C4b-3d2).
///
/// What mtld3d's draw folds into its variant from the stage bindings.
#[derive(Debug, Default)]
pub struct SampledMasks {
    /// Volume textures.
    pub volume: u16,
    /// Cube textures.
    pub cube: u16,
    /// Depth-format textures: a shadow sample, unless the slot is in `depth_fetch`.
    pub depth: u16,
    /// The depth formats sampled raw (INTZ, DF16, DF24), a subset of `depth`.
    pub depth_fetch: u16,
    /// The raw depth formats that read as red alone (DF16, DF24).
    pub raw_depth_red: u16,
    /// Slots whose sample is a fetch4 gather.
    pub fetch4: u16,
    /// The fetch4 slots that gather alpha (A8).
    pub fetch4_alpha: u16,
}

/// A textured core draw's variant (C4b-3b).
///
/// Its sampled slots typed by `masks`, `LOD_BIAS` when it binds its LOD table.
#[must_use]
pub fn core_texture_variant(
    mut variant: VariantKey,
    masks: &SampledMasks,
    lod_table: bool,
) -> VariantKey {
    variant.volume_sampler_mask = masks.volume;
    variant.cube_sampler_mask = masks.cube;
    variant.depth_sampler_mask = masks.depth;
    variant.depth_fetch_mask = masks.depth_fetch;
    variant.raw_depth_red_mask = masks.raw_depth_red;
    variant.fetch4_mask = masks.fetch4;
    variant.fetch4_alpha_mask = masks.fetch4_alpha;
    variant.flags.set(VariantFlags::LOD_BIAS, lod_table);
    variant
}

bitflags::bitflags! {
    /// What a draw's state is, for [`core_draw_kinds`].
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct DrawFlags: u16 {
        /// A vertex shader is bound.
        const VS_BOUND = 1 << 0;
        /// The bound vertex shader was created by a core device.
        const VS_CORE = 1 << 1;
        /// The layout is pre-transformed (POSITIONT).
        const LAYOUT_RHW = 1 << 3;
        /// A pixel shader is bound.
        const PS_BOUND = 1 << 4;
        /// The bound pixel shader was created by a core device.
        const PS_CORE = 1 << 5;
        /// Blending is enabled.
        const BLENDING = 1 << 8;
        /// A sampled vertex slot holds a depth-format texture.
        const VERTEX_DEPTH_TEXTURE = 1 << 10;
    }
}

/// What [`core_draw_kinds`] decides on.
#[derive(Clone, Copy, Default)]
pub struct DrawFacts {
    pub flags: DrawFlags,
}

/// A stage of a core draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageKind {
    Ff,
    Prog,
}

/// Why the core refuses a draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreRefusal {
    /// A bound shader was not created by a core device.
    NotCoreShader,
    /// Blending is enabled.
    Blending,
    /// A sampled vertex slot holds a depth-format texture (C4c).
    VertexDepthTexture,
    /// The draw tests or writes the depth-stencil it samples (C4c).
    BoundDepthSampled,
}

impl CoreRefusal {
    /// The refusal's static name, which the front end logs once.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NotCoreShader => {
                "IDirect3DDevice9::Draw*PrimitiveUP (a shader no core device created)"
            }
            Self::Blending => "IDirect3DDevice9::Draw*PrimitiveUP (blending)",
            Self::VertexDepthTexture => {
                "IDirect3DDevice9::Draw*PrimitiveUP (a depth texture at a vertex sampler)"
            }
            Self::BoundDepthSampled => {
                "IDirect3DDevice9::Draw*PrimitiveUP (the depth-stencil sampled while bound)"
            }
        }
    }
}

/// The stages a core draw of `f` takes, or why the core refuses it.
///
/// A pre-transformed layout takes the fixed-function vertex stage whether a vertex shader is
/// bound or not, as D3D9 bypasses it; so does any layout with no vertex shader, whose transforms,
/// lights and texture coordinates the fixed-function vertex stage computes.
///
/// # Errors
///
/// The first reason the core does not draw `f`.
pub const fn core_draw_kinds(f: DrawFacts) -> Result<(StageKind, StageKind), CoreRefusal> {
    let flags = f.flags;
    if flags.contains(DrawFlags::BLENDING) {
        return Err(CoreRefusal::Blending);
    }
    if flags.contains(DrawFlags::VERTEX_DEPTH_TEXTURE) {
        return Err(CoreRefusal::VertexDepthTexture);
    }
    let vs = if flags.contains(DrawFlags::LAYOUT_RHW) || !flags.contains(DrawFlags::VS_BOUND) {
        StageKind::Ff
    } else if !flags.contains(DrawFlags::VS_CORE) {
        return Err(CoreRefusal::NotCoreShader);
    } else {
        StageKind::Prog
    };
    let ps = if !flags.contains(DrawFlags::PS_BOUND) {
        StageKind::Ff
    } else if !flags.contains(DrawFlags::PS_CORE) {
        return Err(CoreRefusal::NotCoreShader);
    } else {
        StageKind::Prog
    };
    Ok((vs, ps))
}

/// The depth format a core draw's key names, or why the core refuses the draw (C4b-3d2).
///
/// `bound` is the bound depth-stencil's `WMTPixelFormat` (none: the key names none),
/// `samples_bound` whether the draw samples it, and `uses_plane` whether the draw's state tests
/// or writes one of its planes. A depth-stencil the draw does not sample is the key's, whatever
/// its size (C4b-3d3). A draw that samples it goes without it, which only a draw that uses no
/// plane may.
///
/// # Errors
///
/// A draw that uses a plane of the depth-stencil it samples.
pub const fn core_key_depth(
    bound: Option<u32>,
    samples_bound: bool,
    uses_plane: bool,
) -> Result<u32, CoreRefusal> {
    let Some(format) = bound else {
        return Ok(0);
    };
    if !samples_bound {
        return Ok(format);
    }
    if uses_plane {
        Err(CoreRefusal::BoundDepthSampled)
    } else {
        Ok(0)
    }
}

/// The float rows a stage binds: the whole 256-row file under relative addressing.
///
/// The translator clamps a relative index to 0-255, so any row may be read.
#[must_use]
pub const fn float_rows(max_const_used: u32, relative: bool) -> u32 {
    if relative { 256 } else { max_const_used }
}

/// The bytes of `count` vertices `stride` bytes apart; `None` past `i32::MAX`.
///
/// The bound holds on every host: a caller's slice may not exceed `isize::MAX`, 2 GiB on i686,
/// and no D3D9 UP draw passes that much.
#[must_use]
pub fn up_byte_count(stride: u32, count: u32) -> Option<usize> {
    usize::try_from(i32::try_from(stride.checked_mul(count)?).ok()?).ok()
}

/// The vertices an indexed UP draw reads: `[0, min_vertex_index + num_vertices)`.
///
/// `None` past `u32`.
#[must_use]
pub const fn indexed_up_vertices(min_vertex_index: u32, num_vertices: u32) -> Option<u32> {
    min_vertex_index.checked_add(num_vertices)
}

/// D3D9's depth range: one narrower than 0.001 (or inverted) becomes `min_z .. min_z + 0.001`.
#[must_use]
pub const fn viewport_depth_range(min_z: f32, max_z: f32) -> (f32, f32) {
    (min_z, max_z.max(min_z + 0.001))
}

/// The Metal viewport of `v` on a `target` of `(width, height)`.
///
/// `(origin x, origin y, width, height, znear, zfar)`: a zero width or height covers the
/// target, as mtld3d's pass does, and the depth range is [`viewport_depth_range`]'s.
#[must_use]
pub fn core_viewport(v: &D3DVIEWPORT9, target: (u32, u32)) -> [f64; 6] {
    let (x, y, width, height) = drawn_rect(v, target);
    let (znear, zfar) = viewport_depth_range(v.min_z, v.max_z);
    [
        f64::from(x),
        f64::from(y),
        f64::from(width),
        f64::from(height),
        f64::from(znear),
        f64::from(zfar),
    ]
}

/// The rectangle `v` draws to on a `target` of `(width, height)`: all of it when unset.
///
/// `(x, y, width, height)`.
#[must_use]
pub const fn drawn_rect(v: &D3DVIEWPORT9, target: (u32, u32)) -> (u32, u32, u32, u32) {
    if v.width != 0 && v.height != 0 {
        (v.x, v.y, v.width, v.height)
    } else {
        (0, 0, target.0, target.1)
    }
}

/// The scissor a core draw is cut to, before the pass's extent clamps it (C4b-3d3).
///
/// `[x, y, width, height]`, as mtld3d resolves it: the scissor `rect` while `test`
/// (`D3DRS_SCISSORTESTENABLE`) is on, an inverted or empty one staying empty; the drawn viewport
/// with it off.
#[must_use]
pub fn core_scissor(test: bool, rect: D3DRECT, v: &D3DVIEWPORT9, target: (u32, u32)) -> [u32; 4] {
    if test {
        crate::render_state::scissor_snapshot_rect(rect).map(u32::from)
    } else {
        drawn_rect(v, target).into()
    }
}

/// The vertex stages' `PosFixup` of a draw through `v` on `target`.
///
/// `(1 / width, -1 / height, depth clamp, render scale, depth bias)` of the drawn viewport, as
/// mtld3d's draw binds it: the half-pixel offset; `clamp_z` ([`rhw_depth_clamp`]); no render
/// scale; `depth_bias` in clip depth (`d3d_depth_bias_to_clip`), which the vertex stage adds.
#[must_use]
pub fn pos_fixup(v: &D3DVIEWPORT9, target: (u32, u32), clamp_z: bool, depth_bias: f32) -> [f32; 5] {
    let (_, _, width, height) = drawn_rect(v, target);
    let pixels = |n: u32| f32::from(u16::try_from(n.max(1)).unwrap_or(u16::MAX));
    [
        1.0 / pixels(width),
        -1.0 / pixels(height),
        f32::from(u8::from(clamp_z)),
        1.0,
        depth_bias,
    ]
}

/// Whether a draw clamps its depth in the vertex stage, as mtld3d's draw decides it.
///
/// A pre-transformed (`rhw`) draw does, unless a depth test is live: `has_depth` is whether the
/// draw has a depth-stencil attached, `depth_enable` whether `D3DRS_ZENABLE` is on.
#[must_use]
pub const fn rhw_depth_clamp(rhw: bool, has_depth: bool, depth_enable: bool) -> bool {
    rhw && !(has_depth && depth_enable)
}

#[cfg(test)]
mod tests;
