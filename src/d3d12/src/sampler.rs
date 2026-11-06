//! Sampler descriptions as Metal samplers.
//!
//! D3D12 describes a sampler twice: `D3D12_SAMPLER_DESC` for `CreateSampler` and
//! `D3D12_STATIC_SAMPLER_DESC` in a root signature. Both fill winemetal's `WMTSamplerInfo` through
//! [`dxmt_core::sampler::populate`], into a description the caller owns; a static sampler's border
//! colour is its enumeration's.
//!
//! What the C++ logs on the way is reported to the caller as a [`Warning`], so the front end
//! writes it through its own log.

pub use dxmt_core::sampler::Warning;
use dxmt_core::sampler::{self as core_sampler, populate};
use winemetal_wire::{
    WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK, WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE,
    WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK, WMTSamplerInfo,
};

use crate::abi::sampler::{
    STATIC_BORDER_COLOR_OPAQUE_BLACK, STATIC_BORDER_COLOR_TRANSPARENT_BLACK, SamplerDesc,
    StaticSamplerDesc, decode_is_comparison_filter,
};

/// Fills `info` from a root signature's static sampler.
///
/// The border colour is the enumeration's, opaque white for any value but the first two.
pub fn populate_static_sampler_info(
    info: &mut WMTSamplerInfo,
    desc: &StaticSamplerDesc,
    warn: &mut impl FnMut(Warning),
) {
    let common = core_sampler::SamplerDesc {
        filter: desc.filter,
        address: [desc.address_u, desc.address_v, desc.address_w],
        comparison: decode_is_comparison_filter(desc.filter),
        comparison_func: desc.comparison_func,
        max_anisotropy: desc.max_anisotropy,
        min_lod: desc.min_lod,
        max_lod: desc.max_lod,
        border_color: None,
    };
    populate(info, &common, warn);
    info.border_color = match desc.border_color {
        STATIC_BORDER_COLOR_TRANSPARENT_BLACK => WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK,
        STATIC_BORDER_COLOR_OPAQUE_BLACK => WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK,
        _ => WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE,
    };
}

/// Fills `info` from a sampler description of `CreateSampler`.
///
/// The border colour matters only when an address mode is `BORDER`, and Metal has transparent
/// black, opaque black and opaque white: any other colour is a warning and opaque white.
pub fn populate_sampler_info(
    info: &mut WMTSamplerInfo,
    desc: &SamplerDesc,
    warn: &mut impl FnMut(Warning),
) {
    let common = core_sampler::SamplerDesc {
        filter: desc.filter,
        address: [desc.address_u, desc.address_v, desc.address_w],
        comparison: decode_is_comparison_filter(desc.filter),
        comparison_func: desc.comparison_func,
        max_anisotropy: desc.max_anisotropy,
        min_lod: desc.min_lod,
        max_lod: desc.max_lod,
        border_color: Some(desc.border_color),
    };
    populate(info, &common, warn);
}

#[cfg(test)]
mod tests;
