//! Sampler descriptions as Metal samplers.
//!
//! D3D12 describes a sampler twice: `D3D12_SAMPLER_DESC` for `CreateSampler` and
//! `D3D12_STATIC_SAMPLER_DESC` in a root signature. Both fill winemetal's `WMTSamplerInfo` the way
//! DXMT's C++ `PopulateWMTSamplerInfo` does: into a description the caller owns, leaving a field
//! alone where the C++ does (an address mode outside 1 to 5, `gpu_resource_id`).
//!
//! What the C++ logs on the way is reported to the caller as a [`Warning`], so the front end
//! writes it through its own log.

use core::fmt;

use winemetal_wire::{
    WMT_COMPARE_FUNCTION_ALWAYS, WMT_COMPARE_FUNCTION_EQUAL, WMT_COMPARE_FUNCTION_GREATER,
    WMT_COMPARE_FUNCTION_GREATER_EQUAL, WMT_COMPARE_FUNCTION_LESS, WMT_COMPARE_FUNCTION_LESS_EQUAL,
    WMT_COMPARE_FUNCTION_NEVER, WMT_COMPARE_FUNCTION_NOT_EQUAL,
    WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR, WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
    WMT_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE, WMT_SAMPLER_ADDRESS_MODE_MIRROR_REPEAT,
    WMT_SAMPLER_ADDRESS_MODE_REPEAT, WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK,
    WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE, WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK,
    WMT_SAMPLER_MIN_MAG_FILTER_LINEAR, WMT_SAMPLER_MIN_MAG_FILTER_NEAREST,
    WMT_SAMPLER_MIP_FILTER_LINEAR, WMT_SAMPLER_MIP_FILTER_NEAREST, WMTSamplerInfo,
};

use crate::abi::sampler::{
    STATIC_BORDER_COLOR_OPAQUE_BLACK, STATIC_BORDER_COLOR_TRANSPARENT_BLACK, SamplerDesc,
    StaticSamplerDesc, TEXTURE_ADDRESS_MODE_BORDER, TEXTURE_ADDRESS_MODE_CLAMP,
    TEXTURE_ADDRESS_MODE_MIRROR, TEXTURE_ADDRESS_MODE_MIRROR_ONCE, TEXTURE_ADDRESS_MODE_WRAP,
    decode_is_anisotropic_filter, decode_is_comparison_filter, decode_mag_filter,
    decode_min_filter, decode_mip_filter,
};

/// Metal's compare function for each `D3D12_COMPARISON_FUNC`, from `NEVER` (1) to `ALWAYS` (8).
const COMPARE_FUNCTIONS: [u8; 8] = [
    WMT_COMPARE_FUNCTION_NEVER,
    WMT_COMPARE_FUNCTION_LESS,
    WMT_COMPARE_FUNCTION_EQUAL,
    WMT_COMPARE_FUNCTION_LESS_EQUAL,
    WMT_COMPARE_FUNCTION_GREATER,
    WMT_COMPARE_FUNCTION_NOT_EQUAL,
    WMT_COMPARE_FUNCTION_GREATER_EQUAL,
    WMT_COMPARE_FUNCTION_ALWAYS,
];

/// The largest anisotropy Metal takes.
const MAX_ANISOTROPY: u32 = 16;

/// A line the C++ logs as a warning while it fills a description.
#[derive(Debug, PartialEq)]
pub enum Warning {
    /// A comparison filter with a `ComparisonFunc` outside `NEVER` to `ALWAYS`.
    InvalidComparisonFunc,
    /// A dynamic sampler's border colour that is none of the three Metal has, with its components.
    UnsupportedBorderColor([f32; 4]),
}

impl fmt::Display for Warning {
    /// The C++'s message.
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidComparisonFunc => {
                out.write_str("CreateSamplerState: invalid ComparisonFunc")
            }
            Self::UnsupportedBorderColor([r, g, b, a]) => write!(
                out,
                "CreateSamplerState: Unsupported border color ({r}, {g}, {b}, {a})"
            ),
        }
    }
}

/// What both descriptions say alike.
struct Common {
    filter: u32,
    /// The U, V and W address modes.
    address: [u32; 3],
    comparison_func: u32,
    max_anisotropy: u32,
    min_lod: f32,
    max_lod: f32,
}

/// Metal's address mode for a `D3D12_TEXTURE_ADDRESS_MODE`, none for a value outside it.
const fn address_mode(mode: u32) -> Option<u8> {
    match mode {
        TEXTURE_ADDRESS_MODE_WRAP => Some(WMT_SAMPLER_ADDRESS_MODE_REPEAT),
        TEXTURE_ADDRESS_MODE_MIRROR => Some(WMT_SAMPLER_ADDRESS_MODE_MIRROR_REPEAT),
        TEXTURE_ADDRESS_MODE_CLAMP => Some(WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE),
        TEXTURE_ADDRESS_MODE_BORDER => Some(WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR),
        TEXTURE_ADDRESS_MODE_MIRROR_ONCE => Some(WMT_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE),
        _ => None,
    }
}

/// Metal's min or mag filter for a `D3D12_FILTER_TYPE` bit field, linear for any bit set.
const fn min_mag_filter(filter_type: u32) -> u8 {
    if filter_type == 0 {
        WMT_SAMPLER_MIN_MAG_FILTER_NEAREST
    } else {
        WMT_SAMPLER_MIN_MAG_FILTER_LINEAR
    }
}

/// Everything but the border colour, which the two descriptions give differently.
fn populate_common(info: &mut WMTSamplerInfo, common: &Common, warn: &mut impl FnMut(Warning)) {
    let filter = common.filter;
    info.lod_average = 0;
    info.min_filter = min_mag_filter(decode_min_filter(filter));
    info.mag_filter = min_mag_filter(decode_mag_filter(filter));
    info.mip_filter = if decode_mip_filter(filter) == 0 {
        WMT_SAMPLER_MIP_FILTER_NEAREST
    } else {
        WMT_SAMPLER_MIP_FILTER_LINEAR
    };
    info.lod_min_clamp = common.min_lod;
    info.lod_max_clamp = common.max_lod;
    info.max_anisotroy = if decode_is_anisotropic_filter(filter) {
        common.max_anisotropy.clamp(1, MAX_ANISOTROPY)
    } else {
        1
    };
    if let Some(mode) = address_mode(common.address[0]) {
        info.s_address_mode = mode;
    }
    if let Some(mode) = address_mode(common.address[1]) {
        info.t_address_mode = mode;
    }
    if let Some(mode) = address_mode(common.address[2]) {
        info.r_address_mode = mode;
    }
    info.compare_function = WMT_COMPARE_FUNCTION_NEVER;
    if decode_is_comparison_filter(filter) {
        let slot = common.comparison_func.wrapping_sub(1) as usize;
        match COMPARE_FUNCTIONS.get(slot) {
            Some(&function) => info.compare_function = function,
            None => warn(Warning::InvalidComparisonFunc),
        }
    }
    info.support_argument_buffers = 1;
    info.normalized_coords = 1;
}

/// Fills `info` from a root signature's static sampler.
///
/// The border colour is the enumeration's, opaque white for any value but the first two.
pub fn populate_static_sampler_info(
    info: &mut WMTSamplerInfo,
    desc: &StaticSamplerDesc,
    warn: &mut impl FnMut(Warning),
) {
    let common = Common {
        filter: desc.filter,
        address: [desc.address_u, desc.address_v, desc.address_w],
        comparison_func: desc.comparison_func,
        max_anisotropy: desc.max_anisotropy,
        min_lod: desc.min_lod,
        max_lod: desc.max_lod,
    };
    populate_common(info, &common, warn);
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
    let common = Common {
        filter: desc.filter,
        address: [desc.address_u, desc.address_v, desc.address_w],
        comparison_func: desc.comparison_func,
        max_anisotropy: desc.max_anisotropy,
        min_lod: desc.min_lod,
        max_lod: desc.max_lod,
    };
    populate_common(info, &common, warn);
    info.border_color = WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE;
    if common.address.contains(&TEXTURE_ADDRESS_MODE_BORDER) {
        match desc.border_color {
            [0.0, 0.0, 0.0, 0.0] => info.border_color = WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK,
            [0.0, 0.0, 0.0, 1.0] => info.border_color = WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK,
            [1.0, 1.0, 1.0, 1.0] => info.border_color = WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE,
            other => warn(Warning::UnsupportedBorderColor(other)),
        }
    }
}

#[cfg(test)]
mod tests;
