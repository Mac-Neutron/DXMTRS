//! A fixed-function pipeline's key on DXMT's core, and what its pipeline is built from.
//!
//! The core runtime caches its fixed-function pipelines by [`FfPipelineKey`], which the front end
//! builds from the device's state on every draw. Only a miss calls [`ff_source`], which builds
//! both stages' DXSO tokens (`ff_build`) and the vertex layout (`fvf_to_elements`), so a draw
//! whose key is cached builds neither.

use std::sync::Arc;

use airconv_ir::dxso::spec::VariantKey;

use crate::{
    convert::{fvf_to_elements, resolve_attrs_for_ff},
    dxso::{FfPsKey, FfVsKey, MAX_LINKED_INPUTS},
    ff_build,
};

/// What a fixed-function pipeline of the core's back buffer depends on.
///
/// Today's arm draws only to the BGRA8 one-sample back buffer with blending refused, so these
/// fields are all of it; a target format, a sample count or a blend state joins the key when the
/// arm accepts them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FfPipelineKey {
    /// The vertex stage's key.
    pub vs: FfVsKey,
    /// The pixel stage's key.
    pub ps: FfPsKey,
    /// The pixel stage's specialization.
    pub variant: VariantKey,
    /// The FVF the vertex layout comes from.
    pub fvf: u32,
    /// Bytes from one vertex to the next.
    pub stride: u32,
    /// The colour target's `MTLColorWriteMask`.
    pub write_mask: u8,
}

/// What a fixed-function pipeline is built from: [`ff_source`] of its key.
#[derive(Debug, PartialEq, Eq)]
pub struct FfPipelineSource {
    /// The vertex stage, `ff_build::vertex`'s tokens.
    pub vs_tokens: Arc<[u32]>,
    /// The vertex inputs the vertex stage declares.
    pub provided_input_mask: u16,
    /// User clip planes the vertex stage writes.
    pub clip_plane_count: u8,
    /// The pixel stage, `ff_build::pixel`'s tokens.
    pub ps_tokens: Arc<[u32]>,
    /// The pixel stage's specialization.
    pub variant: VariantKey,
    /// Each vertex attribute: `(attribute index, MTLVertexFormat, byte offset)`, from buffer 0.
    pub attrs: Vec<(u32, u32, u32)>,
    /// Bytes from one vertex to the next.
    pub stride: u32,
    /// The colour target's `MTLColorWriteMask`.
    pub write_mask: u8,
}

/// What the pipeline of `key` is built from.
#[must_use]
pub fn ff_source(key: &FfPipelineKey) -> FfPipelineSource {
    let vs = ff_build::vertex(&key.vs);
    let ps = ff_build::pixel(&key.ps);
    let (elements, _) = fvf_to_elements(key.fvf);
    let attrs = resolve_attrs_for_ff(&elements, &[0; MAX_LINKED_INPUTS])
        .attrs
        .iter()
        .filter(|a| a.buffer_index == 0)
        .map(|a| (a.attr_index, a.format as u32, a.offset))
        .collect();
    FfPipelineSource {
        vs_tokens: vs.tokens,
        provided_input_mask: vs.provided_input_mask,
        clip_plane_count: key.vs.clip_plane_count,
        ps_tokens: ps.tokens,
        variant: key.variant,
        attrs,
        stride: key.stride,
        write_mask: key.write_mask,
    }
}

/// The bytes of `count` vertices `stride` bytes apart; `None` past `u32` or the address space.
#[must_use]
pub fn up_byte_count(stride: u32, count: u32) -> Option<usize> {
    usize::try_from(stride.checked_mul(count)?).ok()
}

#[cfg(test)]
mod tests;
