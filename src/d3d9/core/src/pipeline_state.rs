//! Single source of truth for D3D9 → Metal pipeline-state translation.

// Re-exported for the core runtime, which maps them to Metal enums.
pub use mtld3d_shared::mtl::{BlendFactor, BlendOperation};
use mtld3d_shared::mtl::{ColorWriteMask, PixelFormat, VertexStepFunction};
use mtld3d_types::{
    D3DBLEND_BOTHINVSRCALPHA, D3DBLEND_BOTHSRCALPHA, D3DBLEND_INVSRCALPHA, D3DBLEND_ONE,
    D3DBLEND_SRCALPHA, D3DBLEND_ZERO, D3DBLENDOP_ADD, D3DBLENDOP_MAX, D3DBLENDOP_MIN,
    D3DRS_ALPHABLENDENABLE, D3DRS_BLENDOP, D3DRS_BLENDOPALPHA, D3DRS_DESTBLEND,
    D3DRS_DESTBLENDALPHA, D3DRS_SEPARATEALPHABLENDENABLE, D3DRS_SRCBLEND, D3DRS_SRCBLENDALPHA,
    RENDER_STATE_COUNT,
};

use crate::{
    convert::{d3d_to_metal_blend_op, d3d_to_metal_blend_rt, d3d_to_metal_write_mask},
    render_state::enum_value,
};

bitflags::bitflags! {
    /// Boolean RS bits that affect pipeline identity.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    #[repr(transparent)]
    pub struct PipelineRsFlags: u8 {
        const BLEND_ENABLE = 1 << 0;
        const SEPARATE_ALPHA_BLEND = 1 << 1;
        /// ATOC plus alpha test or the A2M latch; effective only on MSAA targets.
        const ALPHA_TO_COVERAGE = 1 << 2;
    }
}

/// Pipeline-identity-affecting render-state bits.
///
/// Carries the RS baked into the pipeline: blend state, color-write mask,
/// alpha-to-coverage. Depth state, cull, scissor, blend-factor, and depth-bias
/// are dynamic state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct PipelineRsBits {
    pub flags: PipelineRsFlags,
    /// `D3DRS_SRCBLEND` (raw D3DBLEND value, fits u8: 1..=19).
    pub src_blend: u8,
    /// `D3DRS_DESTBLEND` (raw D3DBLEND value).
    pub dst_blend: u8,
    /// `D3DRS_BLENDOP` (raw D3DBLENDOP value, 1..=5).
    pub blend_op: u8,
    /// `D3DRS_SRCBLENDALPHA`.
    ///
    /// Only active when `flags.contains(SEPARATE_ALPHA_BLEND)`; otherwise
    /// alpha mirrors `src_blend` per D3D9 spec.
    pub src_blend_alpha: u8,
    /// `D3DRS_DESTBLENDALPHA`. Same activation rule.
    pub dst_blend_alpha: u8,
    /// `D3DRS_BLENDOPALPHA`. Same activation rule.
    pub blend_op_alpha: u8,
    /// `D3DRS_COLORWRITEENABLE` (4 D3DCOLORWRITEENABLE_* bits).
    pub color_write_mask: u8,
    /// `D3DRS_COLORWRITEENABLE1..3`, the write masks of render targets 1..3.
    ///
    /// Index `i` holds the mask for target `i + 1`. Only consulted for
    /// targets present in the pass; an absent target contributes a zero
    /// mask to the key so single-target draws never fragment the cache on
    /// these states.
    pub color_write_mask_ext: [u8; 3],
}

/// Colour attachments 1..3 of the render pass a draw lands in.
///
/// `present_mask` bit `i` (0..3) says slot `i + 1` is bound in the pass,
/// `formats[i]` is its Metal format (ignored when absent), and
/// `has_alpha_mask` bit `i` mirrors `COLOR_HAS_ALPHA` for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExtraColorAttachments {
    pub formats: [PixelFormat; 3],
    pub present_mask: u8,
    pub has_alpha_mask: u8,
}

impl ExtraColorAttachments {
    /// No extra attachment: the single-render-target shape.
    pub const NONE: Self = Self {
        formats: [PixelFormat::Bgra8Unorm; 3],
        present_mask: 0,
        has_alpha_mask: 0,
    };

    #[inline]
    #[must_use]
    pub const fn is_present(&self, extra_index: usize) -> bool {
        self.present_mask & (1 << extra_index) != 0
    }

    #[inline]
    #[must_use]
    pub const fn has_alpha(&self, extra_index: usize) -> bool {
        self.has_alpha_mask & (1 << extra_index) != 0
    }
}

impl Default for ExtraColorAttachments {
    fn default() -> Self {
        Self::NONE
    }
}

impl PipelineRsBits {
    #[inline]
    #[must_use]
    pub const fn blend_enable(&self) -> bool {
        self.flags.contains(PipelineRsFlags::BLEND_ENABLE)
    }
    #[inline]
    #[must_use]
    pub const fn alpha_to_coverage(&self, sample_count: u8) -> bool {
        sample_count > 1 && self.flags.contains(PipelineRsFlags::ALPHA_TO_COVERAGE)
    }
    #[inline]
    #[must_use]
    pub const fn separate_alpha_blend_enable(&self) -> bool {
        self.flags.contains(PipelineRsFlags::SEPARATE_ALPHA_BLEND)
    }

    /// Effective Metal write mask of extra target `extra_index` (slot `extra_index + 1`).
    ///
    /// Empty when the target is absent from the pass or the pixel shader,
    /// whose `oCn` outputs `ps_color_out_mask` lists, does not write it;
    /// otherwise the D3D9 `COLORWRITEENABLEn` mask.
    #[must_use]
    pub fn extra_write_mask(
        &self,
        extra: &ExtraColorAttachments,
        ps_color_out_mask: u8,
        extra_index: usize,
    ) -> ColorWriteMask {
        let written = ps_color_out_mask & (1 << (extra_index + 1)) != 0;
        if extra.is_present(extra_index) && written {
            d3d_to_metal_write_mask(u32::from(self.color_write_mask_ext[extra_index]))
        } else {
            ColorWriteMask::empty()
        }
    }

    /// `true` when render target 0 receives a write under these states.
    ///
    /// The write mask must be non-zero and the pixel shader, whose `oCn`
    /// outputs `ps_color_out_mask` lists, must write `oC0`.
    #[must_use]
    pub const fn writes_rt0(&self, ps_color_out_mask: u8) -> bool {
        self.color_write_mask != 0 && ps_color_out_mask & 1 != 0
    }

    /// `true` when no colour target of the pass receives a write under these states.
    ///
    /// Render target 0 by its D3D9 mask, targets 1..3 by their effective
    /// mask (present, written by the shader, non-zero `COLORWRITEENABLEn`).
    #[must_use]
    pub fn writes_no_color(&self, extra: &ExtraColorAttachments, ps_color_out_mask: u8) -> bool {
        self.color_write_mask == 0
            && (0..3).all(|i| {
                self.extra_write_mask(extra, ps_color_out_mask, i)
                    .is_empty()
            })
    }
}

/// One vertex buffer layout of a pipeline: how Metal steps through a D3D9 stream.
///
/// Part of the pipeline identity: a stream bound with a different stride or
/// a different `SetStreamSourceFreq` needs a different vertex descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StreamLayout {
    /// Bytes per step; never 0 for a used stream (Metal rejects it).
    pub stride: u32,
    pub step: VertexStepFunction,
    /// Instances per advance (`PerInstance`); 1 for `PerVertex`, 0 for `Constant`.
    pub step_rate: u32,
}

impl StreamLayout {
    /// The canonical value of a stream the draw does not read.
    pub const UNUSED: Self = Self {
        stride: 0,
        step: VertexStepFunction::PerVertex,
        step_rate: 0,
    };

    #[inline]
    #[must_use]
    pub const fn is_used(&self) -> bool {
        self.stride != 0
    }

    /// Whether the attributes the shader consumes, `extent` bytes of an element, pass the stride.
    ///
    /// The draw then fetches those through a [`crate::streams::CrossingFetch`].
    #[inline]
    #[must_use]
    pub const fn crosses(&self, extent: u32) -> bool {
        self.stride < extent
    }
}

/// The blend factors and operations a compiled pipeline sees, as D3D9 enum values.
struct EffectiveBlend {
    /// `D3DRS_ALPHABLENDENABLE`, cleared when the pipeline has no colour output.
    enable: bool,
    src: u32,
    dst: u32,
    op: u32,
    src_alpha: u32,
    dst_alpha: u32,
    op_alpha: u32,
}

/// Effective blend of the render-state bits `rs` alone.
///
/// `has_color_output` is whether the pass has a colour output to blend into.
fn effective_blend_rs(rs: &PipelineRsBits, has_color_output: bool) -> EffectiveBlend {
    if !rs.blend_enable() || !has_color_output {
        return EffectiveBlend {
            enable: false,
            src: D3DBLEND_ONE,
            dst: D3DBLEND_ZERO,
            op: D3DBLENDOP_ADD,
            src_alpha: D3DBLEND_ONE,
            dst_alpha: D3DBLEND_ZERO,
            op_alpha: D3DBLENDOP_ADD,
        };
    }
    let equation = |src: u8, dst: u8, op: u8| {
        let (src, dst) = both_src_alpha(u32::from(src), u32::from(dst));
        let op = u32::from(op);
        let (src, dst) = min_max_factors(src, dst, op);
        (src, dst, op)
    };
    let (src, dst, op) = equation(rs.src_blend, rs.dst_blend, rs.blend_op);
    let (src_alpha, dst_alpha, op_alpha) = if rs.separate_alpha_blend_enable() {
        equation(rs.src_blend_alpha, rs.dst_blend_alpha, rs.blend_op_alpha)
    } else {
        (src, dst, op)
    };
    EffectiveBlend {
        enable: true,
        src,
        dst,
        op,
        src_alpha,
        dst_alpha,
        op_alpha,
    }
}

/// The blend a pipeline of the core runtime is built with.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CoreBlend {
    pub src: BlendFactor,
    pub dst: BlendFactor,
    pub op: BlendOperation,
    pub src_alpha: BlendFactor,
    pub dst_alpha: BlendFactor,
    pub op_alpha: BlendOperation,
}

impl CoreBlend {
    /// Whether an equation reads the blend colour (`D3DRS_BLENDFACTOR`).
    #[must_use]
    pub const fn reads_blend_color(&self) -> bool {
        const fn reads(factor: BlendFactor) -> bool {
            matches!(
                factor,
                BlendFactor::BlendColor | BlendFactor::OneMinusBlendColor
            )
        }
        reads(self.src) || reads(self.dst) || reads(self.src_alpha) || reads(self.dst_alpha)
    }
}

/// The blend of a core draw from the render states `rs`; `None` with blending off.
///
/// `target_has_alpha` is whether the target stores alpha (destination-alpha factors
/// of one that does not read 1).
#[must_use]
pub fn core_blend(rs: &[u32; RENDER_STATE_COUNT], target_has_alpha: bool) -> Option<CoreBlend> {
    let mut flags = PipelineRsFlags::empty();
    flags.set(
        PipelineRsFlags::BLEND_ENABLE,
        rs[D3DRS_ALPHABLENDENABLE as usize] != 0,
    );
    flags.set(
        PipelineRsFlags::SEPARATE_ALPHA_BLEND,
        rs[D3DRS_SEPARATEALPHABLENDENABLE as usize] != 0,
    );
    let bits = PipelineRsBits {
        flags,
        src_blend: enum_value(rs, D3DRS_SRCBLEND),
        dst_blend: enum_value(rs, D3DRS_DESTBLEND),
        blend_op: enum_value(rs, D3DRS_BLENDOP),
        src_blend_alpha: enum_value(rs, D3DRS_SRCBLENDALPHA),
        dst_blend_alpha: enum_value(rs, D3DRS_DESTBLENDALPHA),
        blend_op_alpha: enum_value(rs, D3DRS_BLENDOPALPHA),
        ..PipelineRsBits::default()
    };
    let blend = effective_blend_rs(&bits, true);
    let factor = |d3d: u32| d3d_to_metal_blend_rt(d3d, target_has_alpha);
    blend.enable.then(|| CoreBlend {
        src: factor(blend.src),
        dst: factor(blend.dst),
        op: d3d_to_metal_blend_op(blend.op),
        src_alpha: factor(blend.src_alpha),
        dst_alpha: factor(blend.dst_alpha),
        op_alpha: d3d_to_metal_blend_op(blend.op_alpha),
    })
}

/// Resolve the `D3DBLEND_BOTH*` source factors into the pair they stand for.
///
/// `D3DBLEND_BOTHSRCALPHA` as a source factor means source `SRCALPHA` and
/// destination `INVSRCALPHA`, and `D3DBLEND_BOTHINVSRCALPHA` the reverse; the
/// destination state is overridden either way (D3D9 spec). Any other pair
/// passes through. A `BOTH*` value written as the destination factor is left
/// to [`crate::convert::d3d_to_metal_blend`], which reads it as its source half.
const fn both_src_alpha(src: u32, dst: u32) -> (u32, u32) {
    match src {
        D3DBLEND_BOTHSRCALPHA => (D3DBLEND_SRCALPHA, D3DBLEND_INVSRCALPHA),
        D3DBLEND_BOTHINVSRCALPHA => (D3DBLEND_INVSRCALPHA, D3DBLEND_SRCALPHA),
        _ => (src, dst),
    }
}

/// The factors of one blend equation, read as `ONE` under `D3DBLENDOP_MIN` or `MAX`.
///
/// Both operations combine the unweighted source and destination (D3D9
/// spec), and Metal ignores the factors of a `Min` or `Max` equation as
/// well, so any pair is the same pipeline.
const fn min_max_factors(src: u32, dst: u32, op: u32) -> (u32, u32) {
    match op {
        D3DBLENDOP_MIN | D3DBLENDOP_MAX => (D3DBLEND_ONE, D3DBLEND_ONE),
        _ => (src, dst),
    }
}

#[cfg(test)]
mod tests;
