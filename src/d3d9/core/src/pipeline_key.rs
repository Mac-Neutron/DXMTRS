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
    sync::Arc,
};

use airconv_ir::dxso::spec::{StageSpec, VariantFlags, VariantKey, VsSamplerKinds, VsSpec};
use mtld3d_shared::shader_create::ShaderStage;
use mtld3d_types::{D3DVERTEXELEMENT9, D3DVIEWPORT9};

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
    /// The application's program, with the user clip planes it writes.
    Prog {
        code: ShaderCode,
        clip_plane_count: u8,
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

/// What a pipeline of the core's back buffer depends on.
///
/// The arm draws only to the BGRA8 one-sample back buffer with blending refused, so these fields
/// are all of it. The stride is here because D3D9 fetches through a Metal vertex descriptor.
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
    } = key;
    let elements = layout.elements();
    let (vs_code, declared, clip_plane_count, resolved) = match vs {
        VsStageKey::Ff(vs) => {
            let built = ff_build::vertex(vs);
            let resolved = resolve_attrs_for_ff(elements, &vs.passthrough);
            (
                ff_code(built.tokens),
                built.provided_input_mask,
                vs.clip_plane_count,
                resolved,
            )
        }
        VsStageKey::Prog {
            code,
            clip_plane_count,
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
            (code.clone(), u16::MAX, *clip_plane_count, resolved)
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
                sampler_kinds: VsSamplerKinds::default(),
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
    })
}

/// The pixel variant of a core draw from the API thread's `base`.
///
/// What mtld3d's encoder patches per draw, for the core's one BGRA8 back buffer with no depth,
/// one sample, no render scale and no samplers: a programmable stage exports colour 0 alone and
/// no depth, `POINT_SPRITE` holds on point lists only, and the stage reads the extra inputs of
/// `linked_input_mask`. `SRGB_WRITE` stays: the back buffer has no sRGB view, so the shader
/// encodes.
#[must_use]
pub fn core_ps_variant(
    base: VariantKey,
    ps_is_prog: bool,
    point_list: bool,
    linked_input_mask: u8,
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
        variant.flags.insert(VariantFlags::NO_DEPTH_ATTACHMENT);
    }
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
        /// The bound vertex shader declares a sampler.
        const VS_SAMPLES = 1 << 2;
        /// The layout is pre-transformed (POSITIONT).
        const LAYOUT_RHW = 1 << 3;
        /// A pixel shader is bound.
        const PS_BOUND = 1 << 4;
        /// The bound pixel shader was created by a core device.
        const PS_CORE = 1 << 5;
        /// The bound pixel shader declares a sampler.
        const PS_SAMPLES = 1 << 6;
        /// The bound pixel shader reads the bump-env rows.
        const PS_BUMP_ENV = 1 << 7;
        /// Blending is enabled.
        const BLENDING = 1 << 8;
        /// The scissor test is enabled.
        const SCISSOR = 1 << 9;
    }
}

/// What [`core_draw_kinds`] decides on.
#[derive(Clone, Copy, Default)]
pub struct DrawFacts {
    pub flags: DrawFlags,
    /// The constant rows the fixed-function pixel cascade reads (`FfPsKey::constant_rows`).
    pub ff_ps_constant_rows: u8,
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
    /// A bound shader declares a sampler (textures are C4b-3's).
    Sampler,
    /// The pixel shader reads the bump-env rows.
    BumpEnv,
    /// No vertex shader over a layout that is not pre-transformed (the FF VS's transforms).
    TransformedFf,
    /// The fixed-function pixel cascade reads more than one constant row.
    PsStageConstants,
    /// Blending is enabled.
    Blending,
    /// The scissor test is enabled.
    Scissor,
}

impl CoreRefusal {
    /// The refusal's static name, which the front end logs once.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NotCoreShader => {
                "IDirect3DDevice9::Draw*PrimitiveUP (a shader no core device created)"
            }
            Self::Sampler => "IDirect3DDevice9::Draw*PrimitiveUP (a shader declaring a sampler)",
            Self::BumpEnv => "IDirect3DDevice9::Draw*PrimitiveUP (a pixel shader reading bump-env)",
            Self::TransformedFf => {
                "IDirect3DDevice9::Draw*PrimitiveUP (fixed-function transform of untransformed \
                 vertices)"
            }
            Self::PsStageConstants => {
                "IDirect3DDevice9::Draw*PrimitiveUP (texture-stage constants)"
            }
            Self::Blending => "IDirect3DDevice9::Draw*PrimitiveUP (blending)",
            Self::Scissor => "IDirect3DDevice9::Draw*PrimitiveUP (scissor test)",
        }
    }
}

/// The stages a core draw of `f` takes, or why the core refuses it.
///
/// A pre-transformed layout takes the fixed-function vertex stage whether a vertex shader is
/// bound or not, as D3D9 bypasses it; no vertex shader over any other layout needs the
/// fixed-function transforms, which the core does not have yet.
///
/// # Errors
///
/// The first reason the core does not draw `f`.
pub const fn core_draw_kinds(f: DrawFacts) -> Result<(StageKind, StageKind), CoreRefusal> {
    let flags = f.flags;
    if flags.contains(DrawFlags::BLENDING) {
        return Err(CoreRefusal::Blending);
    }
    if flags.contains(DrawFlags::SCISSOR) {
        return Err(CoreRefusal::Scissor);
    }
    let vs = if flags.contains(DrawFlags::LAYOUT_RHW) {
        StageKind::Ff
    } else if !flags.contains(DrawFlags::VS_BOUND) {
        return Err(CoreRefusal::TransformedFf);
    } else if !flags.contains(DrawFlags::VS_CORE) {
        return Err(CoreRefusal::NotCoreShader);
    } else if flags.contains(DrawFlags::VS_SAMPLES) {
        return Err(CoreRefusal::Sampler);
    } else {
        StageKind::Prog
    };
    let ps = if !flags.contains(DrawFlags::PS_BOUND) {
        if f.ff_ps_constant_rows > 1 {
            return Err(CoreRefusal::PsStageConstants);
        }
        StageKind::Ff
    } else if !flags.contains(DrawFlags::PS_CORE) {
        return Err(CoreRefusal::NotCoreShader);
    } else if flags.contains(DrawFlags::PS_SAMPLES) {
        return Err(CoreRefusal::Sampler);
    } else if flags.contains(DrawFlags::PS_BUMP_ENV) {
        return Err(CoreRefusal::BumpEnv);
    } else {
        StageKind::Prog
    };
    Ok((vs, ps))
}

/// The float rows a stage binds: the whole 256-row file under relative addressing.
///
/// The translator clamps a relative index to 0-255, so any row may be read.
#[must_use]
pub const fn float_rows(max_const_used: u32, relative: bool) -> u32 {
    if relative { 256 } else { max_const_used }
}

/// The bytes of `count` vertices `stride` bytes apart; `None` past `u32` or the address space.
#[must_use]
pub fn up_byte_count(stride: u32, count: u32) -> Option<usize> {
    usize::try_from(stride.checked_mul(count)?).ok()
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
    let (x, y, width, height) = if v.width != 0 && v.height != 0 {
        (v.x, v.y, v.width, v.height)
    } else {
        (0, 0, target.0, target.1)
    };
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

#[cfg(test)]
mod tests;
