//! `D3D12_SAMPLER_DESC` and its filter and address modes.
//!
//! The structures and constants as `d3d12.h` defines them. Every enumeration is its `u32` (the C
//! compilers give a header enumeration of non-negative values that type) and the fields are the
//! C's, in snake case. A structure's layout is the C's on every target, asserted at the end against
//! clang's record layouts (llvm-mingw's sysroot for the four PE targets, `include/native`
//! natively).

use core::mem::offset_of;

/// `D3D12_MIP_FILTER_SHIFT`.
pub const MIP_FILTER_SHIFT: u32 = 0;
/// `D3D12_MAG_FILTER_SHIFT`.
pub const MAG_FILTER_SHIFT: u32 = 2;
/// `D3D12_MIN_FILTER_SHIFT`.
pub const MIN_FILTER_SHIFT: u32 = 4;
/// `D3D12_FILTER_TYPE_MASK`.
pub const FILTER_TYPE_MASK: u32 = 0x3;
/// `D3D12_ANISOTROPIC_FILTERING_BIT`.
pub const ANISOTROPIC_FILTERING_BIT: u32 = 0x40;
/// `D3D12_FILTER_REDUCTION_TYPE_SHIFT`.
pub const FILTER_REDUCTION_TYPE_SHIFT: u32 = 7;
/// `D3D12_FILTER_REDUCTION_TYPE_MASK`.
pub const FILTER_REDUCTION_TYPE_MASK: u32 = 0x3;
/// `D3D12_FILTER_TYPE_LINEAR`.
pub const FILTER_TYPE_LINEAR: u32 = 1;
/// `D3D12_FILTER_REDUCTION_TYPE_COMPARISON`.
pub const FILTER_REDUCTION_TYPE_COMPARISON: u32 = 1;

/// `D3D12_TEXTURE_ADDRESS_MODE_WRAP`.
pub const TEXTURE_ADDRESS_MODE_WRAP: u32 = 1;
/// `D3D12_TEXTURE_ADDRESS_MODE_MIRROR`.
pub const TEXTURE_ADDRESS_MODE_MIRROR: u32 = 2;
/// `D3D12_TEXTURE_ADDRESS_MODE_CLAMP`.
pub const TEXTURE_ADDRESS_MODE_CLAMP: u32 = 3;
/// `D3D12_TEXTURE_ADDRESS_MODE_BORDER`.
pub const TEXTURE_ADDRESS_MODE_BORDER: u32 = 4;
/// `D3D12_TEXTURE_ADDRESS_MODE_MIRROR_ONCE`.
pub const TEXTURE_ADDRESS_MODE_MIRROR_ONCE: u32 = 5;

/// `D3D12_COMPARISON_FUNC_NEVER`.
pub const COMPARISON_FUNC_NEVER: u32 = 1;
/// `D3D12_COMPARISON_FUNC_ALWAYS`.
pub const COMPARISON_FUNC_ALWAYS: u32 = 8;

/// `D3D12_STATIC_BORDER_COLOR_TRANSPARENT_BLACK`.
pub const STATIC_BORDER_COLOR_TRANSPARENT_BLACK: u32 = 0;
/// `D3D12_STATIC_BORDER_COLOR_OPAQUE_BLACK`.
pub const STATIC_BORDER_COLOR_OPAQUE_BLACK: u32 = 1;
/// `D3D12_STATIC_BORDER_COLOR_OPAQUE_WHITE`.
pub const STATIC_BORDER_COLOR_OPAQUE_WHITE: u32 = 2;

/// `D3D12_DECODE_MIN_FILTER`: the minification filter type of `filter`.
#[must_use]
pub const fn decode_min_filter(filter: u32) -> u32 {
    (filter >> MIN_FILTER_SHIFT) & FILTER_TYPE_MASK
}

/// `D3D12_DECODE_MAG_FILTER`: the magnification filter type of `filter`.
#[must_use]
pub const fn decode_mag_filter(filter: u32) -> u32 {
    (filter >> MAG_FILTER_SHIFT) & FILTER_TYPE_MASK
}

/// `D3D12_DECODE_MIP_FILTER`: the mip filter type of `filter`.
#[must_use]
pub const fn decode_mip_filter(filter: u32) -> u32 {
    (filter >> MIP_FILTER_SHIFT) & FILTER_TYPE_MASK
}

/// `D3D12_DECODE_IS_ANISOTROPIC_FILTER`: the anisotropic bit with all three filters linear.
#[must_use]
pub const fn decode_is_anisotropic_filter(filter: u32) -> bool {
    filter & ANISOTROPIC_FILTERING_BIT != 0
        && decode_min_filter(filter) == FILTER_TYPE_LINEAR
        && decode_mag_filter(filter) == FILTER_TYPE_LINEAR
        && decode_mip_filter(filter) == FILTER_TYPE_LINEAR
}

/// `D3D12_DECODE_IS_COMPARISON_FILTER`: the reduction type is a comparison.
#[must_use]
pub const fn decode_is_comparison_filter(filter: u32) -> bool {
    (filter >> FILTER_REDUCTION_TYPE_SHIFT) & FILTER_REDUCTION_TYPE_MASK
        == FILTER_REDUCTION_TYPE_COMPARISON
}

/// `D3D12_SAMPLER_DESC`.
#[repr(C)]
pub struct SamplerDesc {
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub address_w: u32,
    pub mip_lod_bias: f32,
    pub max_anisotropy: u32,
    pub comparison_func: u32,
    pub border_color: [f32; 4],
    pub min_lod: f32,
    pub max_lod: f32,
}

/// `D3D12_STATIC_SAMPLER_DESC`.
#[repr(C)]
pub struct StaticSamplerDesc {
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub address_w: u32,
    pub mip_lod_bias: f32,
    pub max_anisotropy: u32,
    pub comparison_func: u32,
    pub border_color: u32,
    pub min_lod: f32,
    pub max_lod: f32,
    pub shader_register: u32,
    pub register_space: u32,
    pub shader_visibility: u32,
}

// The layouts of clang's `-fdump-record-layouts` over the headers named above: every mirrored
// structure's size, alignment and field offsets; the same on every target.
const _: () = {
    assert!(size_of::<SamplerDesc>() == 52 && align_of::<SamplerDesc>() == 4);
    assert!(offset_of!(SamplerDesc, filter) == 0);
    assert!(offset_of!(SamplerDesc, address_u) == 4);
    assert!(offset_of!(SamplerDesc, address_v) == 8);
    assert!(offset_of!(SamplerDesc, address_w) == 12);
    assert!(offset_of!(SamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(SamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(SamplerDesc, comparison_func) == 24);
    assert!(offset_of!(SamplerDesc, border_color) == 28);
    assert!(offset_of!(SamplerDesc, min_lod) == 44);
    assert!(offset_of!(SamplerDesc, max_lod) == 48);
    assert!(size_of::<StaticSamplerDesc>() == 52 && align_of::<StaticSamplerDesc>() == 4);
    assert!(offset_of!(StaticSamplerDesc, filter) == 0);
    assert!(offset_of!(StaticSamplerDesc, address_u) == 4);
    assert!(offset_of!(StaticSamplerDesc, address_v) == 8);
    assert!(offset_of!(StaticSamplerDesc, address_w) == 12);
    assert!(offset_of!(StaticSamplerDesc, mip_lod_bias) == 16);
    assert!(offset_of!(StaticSamplerDesc, max_anisotropy) == 20);
    assert!(offset_of!(StaticSamplerDesc, comparison_func) == 24);
    assert!(offset_of!(StaticSamplerDesc, border_color) == 28);
    assert!(offset_of!(StaticSamplerDesc, min_lod) == 32);
    assert!(offset_of!(StaticSamplerDesc, max_lod) == 36);
    assert!(offset_of!(StaticSamplerDesc, shader_register) == 40);
    assert!(offset_of!(StaticSamplerDesc, register_space) == 44);
    assert!(offset_of!(StaticSamplerDesc, shader_visibility) == 48);
};
