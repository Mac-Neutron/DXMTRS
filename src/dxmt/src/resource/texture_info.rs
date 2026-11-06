//! The texture description both APIs reduce to before they ask Metal for a texture.
//!
//! Each API reads its own description (D3D11's bind and misc flags, D3D12's resource flags) and
//! validates it its own way; the rules they share are here: which Metal texture type a shape is,
//! how the type, mip count, sample count and usage pack into `WMTTextureInfo::bits`, which UAV
//! formats get atomics, and when lossless compression is allowed.

use winemetal_wire::{
    WMT_PIXEL_FORMAT_R32_FLOAT, WMT_PIXEL_FORMAT_R32_SINT, WMT_PIXEL_FORMAT_R32_UINT,
    WMT_PIXEL_FORMAT_RG32_UINT, WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE_CUBE_ARRAY,
    WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY, WMT_TEXTURE_TYPE2D_MULTISAMPLE,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D, WMTTextureInfo,
};

use crate::{format::texel_size, settings::Compression};

/// A resource description's dimension, as both APIs name it.
#[derive(Clone, Copy)]
pub enum Dimension {
    Texture1D,
    Texture2D,
    Texture3D,
}

/// The Metal texture type of a texture of `dimension` with `array_size` slices.
///
/// A 1D texture is a 2D one of height 1. `cube` (D3D11's misc flag, never set by D3D12) makes a
/// 2D texture a cube, an array of them past six slices; otherwise more than one sample makes it
/// multisampled.
#[must_use]
pub const fn texture_type(
    dimension: Dimension,
    array_size: u32,
    sample_count: u32,
    cube: bool,
) -> u32 {
    match dimension {
        Dimension::Texture3D => WMT_TEXTURE_TYPE3D,
        Dimension::Texture2D if cube => {
            if array_size / 6 > 1 {
                WMT_TEXTURE_TYPE_CUBE_ARRAY
            } else {
                WMT_TEXTURE_TYPE_CUBE
            }
        }
        Dimension::Texture2D if sample_count > 1 => {
            if array_size > 1 {
                WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY
            } else {
                WMT_TEXTURE_TYPE2D_MULTISAMPLE
            }
        }
        _ if array_size > 1 => WMT_TEXTURE_TYPE2D_ARRAY,
        _ => WMT_TEXTURE_TYPE2D,
    }
}

/// `WMTTextureInfo::bits`: the type, mip count, sample count and usage, a byte each.
#[must_use]
pub const fn info_bits(texture_type: u32, mip_count: u32, sample_count: u32, usage: u32) -> u32 {
    ((texture_type & 0xff) << WMTTextureInfo::TYPE_SHIFT)
        | ((mip_count & 0xff) << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
        | ((sample_count & 0xff) << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
        | ((usage & 0xff) << WMTTextureInfo::USAGE_SHIFT)
}

/// Whether a shader-writable texture of `pixel_format` gets atomic usage.
///
/// 32-bit integers do; `RG32Uint` (64-bit atomics) only where `supports_apple8` says the GPU is
/// Apple8, which it is asked only for that format.
pub fn atomic_ok(pixel_format: u32, supports_apple8: impl FnOnce() -> bool) -> bool {
    pixel_format == WMT_PIXEL_FORMAT_R32_UINT
        || pixel_format == WMT_PIXEL_FORMAT_R32_SINT
        || (pixel_format == WMT_PIXEL_FORMAT_RG32_UINT && supports_apple8())
}

/// Whether `policy` lets a texture of `pixel_format` be losslessly compressed.
///
/// A UAV texture is not under `RenderTargets`, nor, under the default policy, when it is a 32-bit
/// format other than `R32` that a shader may view as `R32`. Each API adds its own conditions.
#[must_use]
pub const fn lossless(policy: Compression, pixel_format: u32, uav: bool) -> bool {
    let r32_view = texel_size(pixel_format) == 4
        && pixel_format != WMT_PIXEL_FORMAT_R32_UINT
        && pixel_format != WMT_PIXEL_FORMAT_R32_SINT
        && pixel_format != WMT_PIXEL_FORMAT_R32_FLOAT;
    !matches!(policy, Compression::Off)
        && !(uav && (matches!(policy, Compression::RenderTargets) || r32_view))
}

#[cfg(test)]
mod tests;
