//! Mip, subresource and linear-layout arithmetic that D3D11 and D3D12 do alike.
//!
//! Plain numbers in and out, so each API feeds it from its own description. Where the two APIs
//! disagree the module has a function for each rule and names it: a caller picks the one it
//! always used.

use winemetal_wire::WMT_PIXEL_FORMAT_INVALID;

use crate::format::{
    MTL_DXGI_FORMAT_BC, MTL_DXGI_FORMAT_DEPTH_PLANER, MTL_DXGI_FORMAT_STENCIL_PLANER,
    MtlDxgiFormatDesc,
};

/// The size of a dimension of `base` at `level`: halved per level, never below 1.
///
/// A level of 32 or more is 1. D3D11's rule (`GetMipmapSize`).
#[must_use]
pub const fn mip_extent(base: u32, level: u32) -> u32 {
    match base.checked_shr(level) {
        Some(shifted) if shifted > 1 => shifted,
        _ => 1,
    }
}

/// The size of a dimension of `base` at `level`, shifted as the hardware does.
///
/// Only the low five bits of `level` count, so level 32 gives `base` again. D3D12's rule
/// (`GetResourceExtent`).
#[must_use]
pub const fn mip_extent_wrapping(base: u32, level: u32) -> u32 {
    let shifted = base.wrapping_shr(level);
    if shifted > 1 { shifted } else { 1 }
}

/// How many levels run from `extent_or`'s largest extent down to 1: its bit length, 0 for 0.
#[must_use]
pub const fn full_mip_count(extent_or: u32) -> u32 {
    extent_or.bit_width()
}

/// A texture's full mip count from its width and height: D3D11's rule.
///
/// A 3D texture gets it there too, so its depth does not count.
#[must_use]
pub const fn mip_count_wh(width: u32, height: u32) -> u32 {
    full_mip_count(width | height)
}

/// A texture's full mip count from all three extents, 3D depth included. D3D12's rule.
#[must_use]
pub const fn mip_count_whd(width: u32, height: u32, depth: u32) -> u32 {
    full_mip_count(width | height | depth)
}

/// D3D12's mip count for a resource.
///
/// 1 for an empty extent, and at most `requested` when that is not 0 (0 asks for the full chain).
#[must_use]
pub const fn clamped_mip_count(width: u32, height: u32, depth: u32, requested: u32) -> u32 {
    let full = if width | height | depth == 0 {
        1
    } else {
        mip_count_whd(width, height, depth)
    };
    if requested != 0 && requested < full {
        requested
    } else {
        full
    }
}

/// `D3D11CalcSubresource`, and `D3D12CalcSubresource` without the plane.
///
/// Wrapping, as C++'s unsigned arithmetic.
#[must_use]
pub const fn calc_subresource(mip_slice: u32, array_slice: u32, mip_levels: u32) -> u32 {
    mip_slice.wrapping_add(array_slice.wrapping_mul(mip_levels))
}

/// A subresource index as its mip, array and plane slices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decomposed {
    /// The mip level.
    pub mip_slice: u32,
    /// The array slice.
    pub array_slice: u32,
    /// The plane slice.
    pub plane_slice: u32,
}

/// `DecomposeSubresource`: `subresource` split over `mip_levels` and `array_size`.
///
/// The C++ divides by zero for a texture with no mip levels or no array slices; every slice is 0
/// here.
#[must_use]
pub const fn decompose_subresource(
    subresource: u32,
    mip_levels: u32,
    array_size: u32,
) -> Decomposed {
    let array_and_plane = match subresource.checked_div(mip_levels) {
        Some(quotient) => quotient,
        None => 0,
    };
    Decomposed {
        mip_slice: match subresource.checked_rem(mip_levels) {
            Some(rest) => rest,
            None => 0,
        },
        array_slice: match array_and_plane.checked_rem(array_size) {
            Some(rest) => rest,
            None => 0,
        },
        plane_slice: match subresource.checked_div(mip_levels.wrapping_mul(array_size)) {
            Some(plane) => plane,
            None => 0,
        },
    }
}

/// A linear texture's byte counts at one mip level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinearLayout {
    /// Bytes from one row to the next.
    pub bytes_per_row: u32,
    /// Bytes of one 2D image.
    pub bytes_per_image: u32,
    /// Bytes of the whole slice (all depth images).
    pub bytes_per_slice: u32,
}

/// The row pitch of `width` texels of `format`, or none when the format has no linear byte layout.
///
/// Rows align to the texel size, or to 16 bytes when `aligned` and the format is not a depth or
/// stencil plane. This is a 1D texture's layout; block-compressed formats take [`linear_layout`].
#[must_use]
pub fn linear_row_pitch(format: &MtlDxgiFormatDesc, width: u32, aligned: bool) -> Option<u32> {
    if format.pixel_format == WMT_PIXEL_FORMAT_INVALID || format.bytes_per_texel == 0 {
        return None;
    }
    let planar = format.flag & (MTL_DXGI_FORMAT_DEPTH_PLANER | MTL_DXGI_FORMAT_STENCIL_PLANER);
    let alignment = if aligned && planar == 0 {
        16
    } else {
        format.bytes_per_texel.max(1)
    };
    Some((format.bytes_per_texel * width).div_ceil(alignment) * alignment)
}

/// A 2D or 3D texture's layout at a level whose extent is `width` by `height` by `depth`.
///
/// A block-compressed format counts whole 4 by 4 blocks. None when the format has no linear byte
/// layout (D3D11's `E_FAIL`).
#[must_use]
pub fn linear_layout(
    format: &MtlDxgiFormatDesc,
    width: u32,
    height: u32,
    depth: u32,
    aligned: bool,
) -> Option<LinearLayout> {
    if format.pixel_format == WMT_PIXEL_FORMAT_INVALID || format.bytes_per_texel == 0 {
        return None;
    }
    if format.flag & MTL_DXGI_FORMAT_BC != 0 {
        let width_physical = width.div_ceil(4) * 4;
        let height_physical = height.div_ceil(4) * 4;
        let bytes_per_row = (format.bytes_per_texel * width_physical) >> 2;
        return Some(LinearLayout {
            bytes_per_row,
            bytes_per_image: (bytes_per_row * height_physical) >> 2,
            bytes_per_slice: (bytes_per_row * height_physical * depth) >> 2,
        });
    }
    let bytes_per_row = linear_row_pitch(format, width, aligned)?;
    let bytes_per_image = bytes_per_row * height;
    Some(LinearLayout {
        bytes_per_row,
        bytes_per_image,
        bytes_per_slice: bytes_per_image * depth,
    })
}

#[cfg(test)]
mod tests;
