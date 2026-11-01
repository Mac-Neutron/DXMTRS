//! 64-bit resource subset state and subresource enumeration (`dxmt_subresource.hpp`).

use dxmt_core::resource::TextureViewDescriptor;
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_DEPTH24_UNORM_STENCIL8, WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
    WMT_PIXEL_FORMAT_R32X8X32, WMT_PIXEL_FORMAT_X24_STENCIL8, WMT_PIXEL_FORMAT_X32_STENCIL8,
    WMT_PIXEL_FORMAT_X32G8X32,
};

use crate::abi::records::{D3d11Texture1dDesc, D3d11Texture2dDesc1, D3d11Texture3dDesc1};

#[cfg(test)]
mod tests;

/// Number of planes for a `WMTPixelFormat` (`getPlanarCount` in `dxmt_subresource.cpp`).
#[must_use]
pub const fn planar_count(format: u32) -> u32 {
    match format {
        WMT_PIXEL_FORMAT_X32G8X32
        | WMT_PIXEL_FORMAT_R32X8X32
        | WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8
        | WMT_PIXEL_FORMAT_DEPTH24_UNORM_STENCIL8
        | WMT_PIXEL_FORMAT_X32_STENCIL8
        | WMT_PIXEL_FORMAT_X24_STENCIL8 => 2,
        _ => 1,
    }
}

/// Active plane mask for a `WMTPixelFormat` (`getPlanarMask` in `dxmt_subresource.cpp`).
#[must_use]
pub const fn planar_mask(format: u32) -> u32 {
    match format {
        WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 | WMT_PIXEL_FORMAT_DEPTH24_UNORM_STENCIL8 => 0b11,
        WMT_PIXEL_FORMAT_X32G8X32
        | WMT_PIXEL_FORMAT_X32_STENCIL8
        | WMT_PIXEL_FORMAT_X24_STENCIL8 => 0b10,
        _ => 0b01,
    }
}

/// 64-bit subresource subset encoding (`dxmt::ResourceSubsetState`).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceSubset(pub u64);

impl core::fmt::Debug for ResourceSubset {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ResourceSubset").field(&self.0).finish()
    }
}

impl ResourceSubset {
    /// Whole-resource subset (`encoded_tag == 0`).
    pub const WHOLE: Self = Self(0);

    /// Creates a subset covering a buffer byte range `[offset, offset + length)`.
    #[must_use]
    pub const fn buffer(offset: u32, length: u32) -> Self {
        let off = (offset as u64) & 0x7fff_ffff;
        let len = (length as u64) & 0x7fff_ffff;
        Self(0b01 | (off << 2) | (len << 33))
    }

    /// Creates a subset covering the subresources referenced by a texture view descriptor.
    #[must_use]
    pub const fn from_texture_view(
        desc: &TextureViewDescriptor,
        total_mip_count: u32,
        total_array_size: u32,
        ignore_planar_mask: u32,
    ) -> Self {
        let total_planar = planar_count(desc.format);
        let pmask = planar_mask(desc.format) & !ignore_planar_mask;
        if total_planar
            .saturating_mul(total_mip_count)
            .saturating_mul(total_array_size)
            <= 62
        {
            let mut bits = 0u64;
            let mut planar = 0u32;
            while planar < total_planar {
                let mut slice = desc.first_slice;
                let slice_end = desc.first_slice.wrapping_add(desc.array_size);
                while slice < slice_end {
                    let mut level = desc.first_mip;
                    let level_end = desc.first_mip.wrapping_add(desc.mip_count);
                    while level < level_end {
                        if ((1u32 << planar) & pmask) != 0 {
                            let idx = planar
                                .wrapping_mul(total_array_size)
                                .wrapping_mul(total_mip_count)
                                .wrapping_add(slice.wrapping_mul(total_mip_count))
                                .wrapping_add(level);
                            bits |= 1u64 << idx;
                        }
                        level += 1;
                    }
                    slice += 1;
                }
                planar += 1;
            }
            Self(0b11 | ((bits & ((1u64 << 62) - 1)) << 2))
        } else {
            let pm = (pmask as u64) & 0x3fff_ffff;
            let mip_start = (desc.first_mip as u64) & 0xf;
            let array_start = (desc.first_slice as u64) & 0xfff;
            let mip_end = (desc.first_mip.wrapping_add(desc.mip_count) as u64) & 0xf;
            let array_end = (desc.first_slice.wrapping_add(desc.array_size) as u64) & 0xfff;
            Self(
                0b10 | (pm << 2)
                    | (mip_start << 32)
                    | (array_start << 36)
                    | (mip_end << 48)
                    | (array_end << 52),
            )
        }
    }

    /// Returns `true` if `self` and `other` overlap (`ResourceSubsetState::overlapWith`).
    #[must_use]
    pub const fn overlaps(self, other: Self) -> bool {
        let tag_a = self.0 & 0b11;
        let tag_b = other.0 & 0b11;
        if tag_a != 0 && tag_b != 0 && tag_a != tag_b {
            return false;
        }
        if tag_a == 0 || tag_b == 0 {
            return true;
        }
        match tag_a {
            0b01 => {
                let off_a = (self.0 >> 2) & 0x7fff_ffff;
                let len_a = (self.0 >> 33) & 0x7fff_ffff;
                let off_b = (other.0 >> 2) & 0x7fff_ffff;
                let len_b = (other.0 >> 33) & 0x7fff_ffff;
                (off_a < off_b + len_b) && (off_b < off_a + len_a)
            }
            0b11 => ((self.0 >> 2) & (other.0 >> 2)) != 0,
            0b10 => {
                let pm_a = (self.0 >> 2) & 0x3fff_ffff;
                let pm_b = (other.0 >> 2) & 0x3fff_ffff;
                let mip_lo_a = (self.0 >> 32) & 0xf;
                let mip_lo_b = (other.0 >> 32) & 0xf;
                let arr_lo_a = (self.0 >> 36) & 0xfff;
                let arr_lo_b = (other.0 >> 36) & 0xfff;
                let mip_hi_a = (self.0 >> 48) & 0xf;
                let mip_hi_b = (other.0 >> 48) & 0xf;
                let arr_hi_a = (self.0 >> 52) & 0xfff;
                let arr_hi_b = (other.0 >> 52) & 0xfff;
                (pm_a & pm_b) != 0
                    && (mip_lo_a < mip_hi_b)
                    && (mip_lo_b < mip_hi_a)
                    && (arr_lo_a < arr_hi_b)
                    && (arr_lo_b < arr_hi_a)
            }
            _ => true,
        }
    }
}

/// Computes `D3D11CalcSubresource(mip_slice, array_slice, mip_levels)`.
#[must_use]
pub const fn calc_subresource(mip_slice: u32, array_slice: u32, mip_levels: u32) -> u32 {
    mip_slice.wrapping_add(array_slice.wrapping_mul(mip_levels))
}

/// Coordinates of one subresource (`SUBRESOURCE_ENTRY` in `d3d11_enumerable.hpp`).
#[derive(Debug, PartialEq, Eq)]
pub struct SubresourceEntry {
    /// Subresource index (`D3D11CalcSubresource`).
    pub subresource_id: u32,
    /// Mip level (`0..mip_levels`).
    pub mip_level: u32,
    /// Array slice (`0..array_size`).
    pub array_slice: u32,
}

/// Enumerates all subresources of a 1D texture (`EnumerateSubresources` in `d3d11_enumerable.cpp`).
#[must_use]
pub fn enumerate_subresources_1d(desc: &D3d11Texture1dDesc) -> Vec<SubresourceEntry> {
    let cap = usize::try_from(desc.array_size.saturating_mul(desc.mip_levels)).unwrap_or(0);
    let mut out = Vec::with_capacity(cap);
    for slice in 0..desc.array_size {
        for level in 0..desc.mip_levels {
            out.push(SubresourceEntry {
                subresource_id: calc_subresource(level, slice, desc.mip_levels),
                mip_level: level,
                array_slice: slice,
            });
        }
    }
    out
}

/// Enumerates all subresources of a 2D texture (`EnumerateSubresources` in `d3d11_enumerable.cpp`).
#[must_use]
pub fn enumerate_subresources_2d(desc: &D3d11Texture2dDesc1) -> Vec<SubresourceEntry> {
    let cap = usize::try_from(desc.array_size.saturating_mul(desc.mip_levels)).unwrap_or(0);
    let mut out = Vec::with_capacity(cap);
    for slice in 0..desc.array_size {
        for level in 0..desc.mip_levels {
            out.push(SubresourceEntry {
                subresource_id: calc_subresource(level, slice, desc.mip_levels),
                mip_level: level,
                array_slice: slice,
            });
        }
    }
    out
}

/// Enumerates all subresources of a 3D texture (`EnumerateSubresources` in `d3d11_enumerable.cpp`).
#[must_use]
pub fn enumerate_subresources_3d(desc: &D3d11Texture3dDesc1) -> Vec<SubresourceEntry> {
    let cap = usize::try_from(desc.mip_levels).unwrap_or(0);
    let mut out = Vec::with_capacity(cap);
    for level in 0..desc.mip_levels {
        out.push(SubresourceEntry {
            subresource_id: calc_subresource(level, 0, desc.mip_levels),
            mip_level: level,
            array_slice: 0,
        });
    }
    out
}
