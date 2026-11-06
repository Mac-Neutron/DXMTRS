//! `D3D12_RESOURCE_DESC`, the heap properties, footprints and allocation information.
//!
//! The structures and constants as `d3d12.h` defines them. Every enumeration is its `u32` (the C
//! compilers give a header enumeration of non-negative values that type) and the fields are the
//! C's in snake case. A structure's layout is the C's on every target, asserted at the end against
//! clang's record layouts (llvm-mingw's sysroot for the four PE targets, `include/native`
//! natively). The heap types and the resource states are `abi::barrier`'s.

use core::mem::offset_of;

pub use dxmt_util::dxgi_consts::{
    DXGI_FORMAT_R8_TYPELESS, DXGI_FORMAT_R32_TYPELESS, DXGI_FORMAT_UNKNOWN,
};

pub use super::pipeline::SampleDesc;

/// `D3D12_RESOURCE_DIMENSION_BUFFER`.
pub const D3D12_RESOURCE_DIMENSION_BUFFER: u32 = 1;
/// `D3D12_RESOURCE_DIMENSION_TEXTURE1D`.
pub const D3D12_RESOURCE_DIMENSION_TEXTURE1D: u32 = 2;
/// `D3D12_RESOURCE_DIMENSION_TEXTURE2D`.
pub const D3D12_RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;
/// `D3D12_RESOURCE_DIMENSION_TEXTURE3D`.
pub const D3D12_RESOURCE_DIMENSION_TEXTURE3D: u32 = 4;

/// `D3D12_TEXTURE_LAYOUT_UNKNOWN`.
pub const D3D12_TEXTURE_LAYOUT_UNKNOWN: u32 = 0;
/// `D3D12_TEXTURE_LAYOUT_ROW_MAJOR`.
pub const D3D12_TEXTURE_LAYOUT_ROW_MAJOR: u32 = 1;

/// `D3D12_RESOURCE_FLAG_NONE`.
pub const D3D12_RESOURCE_FLAG_NONE: u32 = 0;
/// `D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET`.
pub const D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET: u32 = 0x1;
/// `D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL`.
pub const D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL: u32 = 0x2;
/// `D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS`.
pub const D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS: u32 = 0x4;
/// `D3D12_RESOURCE_FLAG_ALLOW_CROSS_ADAPTER`.
pub const D3D12_RESOURCE_FLAG_ALLOW_CROSS_ADAPTER: u32 = 0x10;
/// `D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS`.
pub const D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS: u32 = 0x20;

/// `D3D12_CPU_PAGE_PROPERTY_UNKNOWN`.
pub const D3D12_CPU_PAGE_PROPERTY_UNKNOWN: u32 = 0;
/// `D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE`.
pub const D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE: u32 = 1;
/// `D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE`.
pub const D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE: u32 = 2;
/// `D3D12_CPU_PAGE_PROPERTY_WRITE_BACK`.
pub const D3D12_CPU_PAGE_PROPERTY_WRITE_BACK: u32 = 3;

/// `D3D12_MEMORY_POOL_UNKNOWN`.
pub const D3D12_MEMORY_POOL_UNKNOWN: u32 = 0;
/// `D3D12_MEMORY_POOL_L0`.
pub const D3D12_MEMORY_POOL_L0: u32 = 1;
/// `D3D12_MEMORY_POOL_L1`.
pub const D3D12_MEMORY_POOL_L1: u32 = 2;

/// `D3D12_HEAP_FLAG_NONE`.
pub const D3D12_HEAP_FLAG_NONE: u32 = 0;
/// `D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER`.
pub const D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER: u32 = 0x20;
/// `D3D12_HEAP_FLAG_ALLOW_WRITE_WATCH`.
pub const D3D12_HEAP_FLAG_ALLOW_WRITE_WATCH: u32 = 0x200;

/// `D3D12_TEXTURE_DATA_PITCH_ALIGNMENT`.
pub const D3D12_TEXTURE_DATA_PITCH_ALIGNMENT: u32 = 256;
/// `D3D12_TEXTURE_DATA_PLACEMENT_ALIGNMENT`.
pub const D3D12_TEXTURE_DATA_PLACEMENT_ALIGNMENT: u64 = 512;
/// `D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT`.
pub const D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT: u64 = 65536;
/// `D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT`.
pub const D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT: u64 = 4_194_304;

/// `D3D12_RESOURCE_DESC`.
#[repr(C)]
pub struct ResourceDesc {
    pub dimension: u32,
    pub alignment: u64,
    pub width: u64,
    pub height: u32,
    pub depth_or_array_size: u16,
    pub mip_levels: u16,
    pub format: u32,
    pub sample_desc: SampleDesc,
    pub layout: u32,
    pub flags: u32,
}

/// `D3D12_MIP_REGION`.
#[repr(C)]
pub struct MipRegion {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

/// `D3D12_RESOURCE_DESC1`: a [`ResourceDesc`] and the sampler feedback's mip region.
#[repr(C)]
pub struct ResourceDesc1 {
    pub dimension: u32,
    pub alignment: u64,
    pub width: u64,
    pub height: u32,
    pub depth_or_array_size: u16,
    pub mip_levels: u16,
    pub format: u32,
    pub sample_desc: SampleDesc,
    pub layout: u32,
    pub flags: u32,
    pub sampler_feedback_mip_region: MipRegion,
}

/// `D3D12_HEAP_PROPERTIES`.
#[repr(C)]
pub struct HeapProperties {
    pub r#type: u32,
    pub cpu_page_property: u32,
    pub memory_pool_preference: u32,
    pub creation_node_mask: u32,
    pub visible_node_mask: u32,
}

/// `D3D12_BOX`.
#[repr(C)]
pub struct D3d12Box {
    pub left: u32,
    pub top: u32,
    pub front: u32,
    pub right: u32,
    pub bottom: u32,
    pub back: u32,
}

/// `D3D12_SUBRESOURCE_FOOTPRINT`.
#[repr(C)]
pub struct SubresourceFootprint {
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub row_pitch: u32,
}

/// `D3D12_PLACED_SUBRESOURCE_FOOTPRINT`.
#[repr(C)]
pub struct PlacedSubresourceFootprint {
    pub offset: u64,
    pub footprint: SubresourceFootprint,
}

/// `D3D12_RESOURCE_ALLOCATION_INFO`.
#[repr(C)]
pub struct ResourceAllocationInfo {
    pub size_in_bytes: u64,
    pub alignment: u64,
}

/// `D3D12_RESOURCE_ALLOCATION_INFO1`.
#[repr(C)]
pub struct ResourceAllocationInfo1 {
    pub offset: u64,
    pub alignment: u64,
    pub size_in_bytes: u64,
}

// The layouts of clang's `-fdump-record-layouts` over the headers named above: every mirrored
// structure's size, alignment and field offsets; the same on every target.
const _: () = {
    assert!(size_of::<ResourceDesc>() == 56 && align_of::<ResourceDesc>() == 8);
    assert!(offset_of!(ResourceDesc, dimension) == 0);
    assert!(offset_of!(ResourceDesc, alignment) == 8);
    assert!(offset_of!(ResourceDesc, width) == 16);
    assert!(offset_of!(ResourceDesc, height) == 24);
    assert!(offset_of!(ResourceDesc, depth_or_array_size) == 28);
    assert!(offset_of!(ResourceDesc, mip_levels) == 30);
    assert!(offset_of!(ResourceDesc, format) == 32);
    assert!(offset_of!(ResourceDesc, sample_desc) == 36);
    assert!(offset_of!(ResourceDesc, layout) == 44);
    assert!(offset_of!(ResourceDesc, flags) == 48);
    assert!(size_of::<ResourceDesc1>() == 64 && align_of::<ResourceDesc1>() == 8);
    assert!(offset_of!(ResourceDesc1, dimension) == 0);
    assert!(offset_of!(ResourceDesc1, alignment) == 8);
    assert!(offset_of!(ResourceDesc1, width) == 16);
    assert!(offset_of!(ResourceDesc1, height) == 24);
    assert!(offset_of!(ResourceDesc1, depth_or_array_size) == 28);
    assert!(offset_of!(ResourceDesc1, mip_levels) == 30);
    assert!(offset_of!(ResourceDesc1, format) == 32);
    assert!(offset_of!(ResourceDesc1, sample_desc) == 36);
    assert!(offset_of!(ResourceDesc1, layout) == 44);
    assert!(offset_of!(ResourceDesc1, flags) == 48);
    assert!(offset_of!(ResourceDesc1, sampler_feedback_mip_region) == 52);
    assert!(size_of::<MipRegion>() == 12 && align_of::<MipRegion>() == 4);
    assert!(offset_of!(MipRegion, width) == 0);
    assert!(offset_of!(MipRegion, height) == 4);
    assert!(offset_of!(MipRegion, depth) == 8);
    assert!(size_of::<HeapProperties>() == 20 && align_of::<HeapProperties>() == 4);
    assert!(offset_of!(HeapProperties, r#type) == 0);
    assert!(offset_of!(HeapProperties, cpu_page_property) == 4);
    assert!(offset_of!(HeapProperties, memory_pool_preference) == 8);
    assert!(offset_of!(HeapProperties, creation_node_mask) == 12);
    assert!(offset_of!(HeapProperties, visible_node_mask) == 16);
    assert!(size_of::<D3d12Box>() == 24 && align_of::<D3d12Box>() == 4);
    assert!(offset_of!(D3d12Box, left) == 0);
    assert!(offset_of!(D3d12Box, top) == 4);
    assert!(offset_of!(D3d12Box, front) == 8);
    assert!(offset_of!(D3d12Box, right) == 12);
    assert!(offset_of!(D3d12Box, bottom) == 16);
    assert!(offset_of!(D3d12Box, back) == 20);
    assert!(size_of::<SubresourceFootprint>() == 20 && align_of::<SubresourceFootprint>() == 4);
    assert!(offset_of!(SubresourceFootprint, format) == 0);
    assert!(offset_of!(SubresourceFootprint, width) == 4);
    assert!(offset_of!(SubresourceFootprint, height) == 8);
    assert!(offset_of!(SubresourceFootprint, depth) == 12);
    assert!(offset_of!(SubresourceFootprint, row_pitch) == 16);
    assert!(
        size_of::<PlacedSubresourceFootprint>() == 32
            && align_of::<PlacedSubresourceFootprint>() == 8
    );
    assert!(offset_of!(PlacedSubresourceFootprint, offset) == 0);
    assert!(offset_of!(PlacedSubresourceFootprint, footprint) == 8);
    assert!(size_of::<ResourceAllocationInfo>() == 16 && align_of::<ResourceAllocationInfo>() == 8);
    assert!(offset_of!(ResourceAllocationInfo, size_in_bytes) == 0);
    assert!(offset_of!(ResourceAllocationInfo, alignment) == 8);
    assert!(
        size_of::<ResourceAllocationInfo1>() == 24 && align_of::<ResourceAllocationInfo1>() == 8
    );
    assert!(offset_of!(ResourceAllocationInfo1, offset) == 0);
    assert!(offset_of!(ResourceAllocationInfo1, alignment) == 8);
    assert!(offset_of!(ResourceAllocationInfo1, size_in_bytes) == 16);
};
