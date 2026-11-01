//! Format, colour-space and render-scale helpers shared by the D3D11 and D3D12 swap chains.
//!
//! Mirrors `ConvertSwapChainFormat`, `ConvertColorSpace`, `GetMonitorFormatBpp` and `RenderScale`
//! in `d3d11_swapchain.cpp` and `d3d12_swapchain.cpp`.

use winemetal_wire::{
    WMT_COLOR_SPACE_BT2020, WMT_COLOR_SPACE_HDR_PQ, WMT_COLOR_SPACE_HDR_SC_RGB,
    WMT_COLOR_SPACE_INVALID, WMT_COLOR_SPACE_SRGB, WMT_COLOR_SPACE_SRGB_LINEAR,
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB, WMT_PIXEL_FORMAT_INVALID,
    WMT_PIXEL_FORMAT_RGB10A2_UNORM, WMT_PIXEL_FORMAT_RGBA16_FLOAT,
};

pub const DXGI_FORMAT_R16G16B16A16_FLOAT: u32 = 10;
pub const DXGI_FORMAT_R10G10B10A2_UNORM: u32 = 24;
pub const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
pub const DXGI_FORMAT_R8G8B8A8_UNORM_SRGB: u32 = 29;
pub const DXGI_FORMAT_B8G8R8A8_UNORM: u32 = 87;
pub const DXGI_FORMAT_B8G8R8X8_UNORM: u32 = 88;
pub const DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM: u32 = 89;
pub const DXGI_FORMAT_B8G8R8A8_UNORM_SRGB: u32 = 91;
pub const DXGI_FORMAT_B8G8R8X8_UNORM_SRGB: u32 = 93;

pub const DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709: u32 = 0;
pub const DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709: u32 = 1;
pub const DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020: u32 = 10;
pub const DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020: u32 = 12;

/// Maps a DXGI swap-chain format to its `WMTPixelFormat`, or `WMTPixelFormatInvalid` (0).
///
/// Both `B8G8R8A8` and `R8G8B8A8` map to `BGRA8` in the Metal presenter.
#[must_use]
pub const fn convert_format(format: u32) -> u32 {
    match format {
        DXGI_FORMAT_B8G8R8A8_UNORM_SRGB | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => {
            WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB
        }
        DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM => WMT_PIXEL_FORMAT_BGRA8_UNORM,
        DXGI_FORMAT_R10G10B10A2_UNORM => WMT_PIXEL_FORMAT_RGB10A2_UNORM,
        DXGI_FORMAT_R16G16B16A16_FLOAT => WMT_PIXEL_FORMAT_RGBA16_FLOAT,
        _ => WMT_PIXEL_FORMAT_INVALID,
    }
}

/// Maps a `DXGI_COLOR_SPACE_TYPE` and HDR flag to its `WMTColorSpace`.
///
/// Linear G10/P709 selects `WMTColorSpaceHDR_scRGB` when `hdr` is true and
/// `WMTColorSpaceSRGBLinear` otherwise.
#[must_use]
pub const fn convert_color_space(color_space: u32, hdr: bool) -> u64 {
    match color_space {
        DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709 => WMT_COLOR_SPACE_SRGB,
        DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709 => {
            if hdr {
                WMT_COLOR_SPACE_HDR_SC_RGB
            } else {
                WMT_COLOR_SPACE_SRGB_LINEAR
            }
        }
        DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 => WMT_COLOR_SPACE_HDR_PQ,
        DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020 => WMT_COLOR_SPACE_BT2020,
        _ => WMT_COLOR_SPACE_INVALID,
    }
}

/// Bits per pixel for a display mode's `DXGI_FORMAT`: 64 for `R16G16B16A16_FLOAT`, 32 otherwise.
///
/// Logs `GetMonitorFormatBpp: Unknown format: <n>` when the format is outside the nine display
/// formats DXMT recognises.
#[must_use]
pub fn monitor_format_bpp(format: u32) -> u32 {
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM
        | DXGI_FORMAT_B8G8R8A8_UNORM
        | DXGI_FORMAT_B8G8R8X8_UNORM
        | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
        | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
        | DXGI_FORMAT_B8G8R8X8_UNORM_SRGB
        | DXGI_FORMAT_R10G10B10A2_UNORM
        | DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM => 32,
        DXGI_FORMAT_R16G16B16A16_FLOAT => 64,
        _ => {
            crate::log::warn(&format!("GetMonitorFormatBpp: Unknown format: {format}"));
            32
        }
    }
}

/// Parses a `DXMT_RENDER_SCALE` value, clamping positive numbers to `[0.25, 1.0]`.
///
/// Empty, non-positive or unparsable strings answer `1.0`.
#[must_use]
pub fn parse_render_scale(value: &str) -> f64 {
    match value.trim().parse::<f64>() {
        Ok(scale) if scale.is_finite() && scale > 0.0 => scale.clamp(0.25, 1.0),
        _ => 1.0,
    }
}

/// Reads `DXMT_RENDER_SCALE` from the environment and clamps it to `[0.25, 1.0]`.
///
/// Answers `1.0` when unset, non-positive or unparsable.
#[must_use]
pub fn render_scale() -> f64 {
    std::env::var("DXMT_RENDER_SCALE").map_or(1.0, |v| parse_render_scale(&v))
}

#[cfg(test)]
mod tests;
