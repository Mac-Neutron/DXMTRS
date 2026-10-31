//! The barrier structures, the legacy and the enhanced ones.
//!
//! `D3D12_RESOURCE_BARRIER` (what `ResourceBarrier` takes) and the enhanced barriers of
//! `ID3D12GraphicsCommandList7::Barrier`, with the enumerations their fields hold, from the
//! mingw-w64 `d3d12.h` the C++ builds against. Fields are the C's in snake case, the Hungarian
//! `p` of a pointer dropped; every enumeration is a `u32`. A struct's layout is the C's on every
//! target, asserted at the end against clang's record layouts (llvm-mingw's sysroot for the four
//! PE targets, `include/native` natively). The arms of `ResourceBarrier`'s union are structs of up
//! to 24 bytes that carry no `Copy`, so the union holds them in `ManuallyDrop`.

use core::{
    ffi::c_void,
    mem::{ManuallyDrop, offset_of},
};

// `D3D12_BARRIER_SYNC`.
pub const D3D12_BARRIER_SYNC_NONE: u32 = 0;
pub const D3D12_BARRIER_SYNC_ALL: u32 = 1;
pub const D3D12_BARRIER_SYNC_DRAW: u32 = 2;
pub const D3D12_BARRIER_SYNC_INDEX_INPUT: u32 = 4;
pub const D3D12_BARRIER_SYNC_VERTEX_SHADING: u32 = 8;
pub const D3D12_BARRIER_SYNC_PIXEL_SHADING: u32 = 0x10;
pub const D3D12_BARRIER_SYNC_DEPTH_STENCIL: u32 = 0x20;
pub const D3D12_BARRIER_SYNC_RENDER_TARGET: u32 = 0x40;
pub const D3D12_BARRIER_SYNC_COMPUTE_SHADING: u32 = 0x80;
pub const D3D12_BARRIER_SYNC_RAYTRACING: u32 = 0x100;
pub const D3D12_BARRIER_SYNC_COPY: u32 = 0x200;
pub const D3D12_BARRIER_SYNC_RESOLVE: u32 = 0x400;
pub const D3D12_BARRIER_SYNC_EXECUTE_INDIRECT: u32 = 0x800;
pub const D3D12_BARRIER_SYNC_PREDICATION: u32 = 0x800;
pub const D3D12_BARRIER_SYNC_ALL_SHADING: u32 = 0x1000;
pub const D3D12_BARRIER_SYNC_NON_PIXEL_SHADING: u32 = 0x2000;
pub const D3D12_BARRIER_SYNC_EMIT_RAYTRACING_ACCELERATION_STRUCTURE_POSTBUILD_INFO: u32 = 0x4000;
pub const D3D12_BARRIER_SYNC_CLEAR_UNORDERED_ACCESS_VIEW: u32 = 0x8000;
pub const D3D12_BARRIER_SYNC_VIDEO_DECODE: u32 = 0x10_0000;
pub const D3D12_BARRIER_SYNC_VIDEO_PROCESS: u32 = 0x20_0000;
pub const D3D12_BARRIER_SYNC_VIDEO_ENCODE: u32 = 0x40_0000;
pub const D3D12_BARRIER_SYNC_BUILD_RAYTRACING_ACCELERATION_STRUCTURE: u32 = 0x80_0000;
pub const D3D12_BARRIER_SYNC_COPY_RAYTRACING_ACCELERATION_STRUCTURE: u32 = 0x100_0000;
pub const D3D12_BARRIER_SYNC_SPLIT: u32 = 0x8000_0000;

// `D3D12_BARRIER_ACCESS`.
pub const D3D12_BARRIER_ACCESS_COMMON: u32 = 0;
pub const D3D12_BARRIER_ACCESS_VERTEX_BUFFER: u32 = 1;
pub const D3D12_BARRIER_ACCESS_CONSTANT_BUFFER: u32 = 2;
pub const D3D12_BARRIER_ACCESS_INDEX_BUFFER: u32 = 4;
pub const D3D12_BARRIER_ACCESS_RENDER_TARGET: u32 = 8;
pub const D3D12_BARRIER_ACCESS_UNORDERED_ACCESS: u32 = 0x10;
pub const D3D12_BARRIER_ACCESS_DEPTH_STENCIL_WRITE: u32 = 0x20;
pub const D3D12_BARRIER_ACCESS_DEPTH_STENCIL_READ: u32 = 0x40;
pub const D3D12_BARRIER_ACCESS_SHADER_RESOURCE: u32 = 0x80;
pub const D3D12_BARRIER_ACCESS_STREAM_OUTPUT: u32 = 0x100;
pub const D3D12_BARRIER_ACCESS_INDIRECT_ARGUMENT: u32 = 0x200;
pub const D3D12_BARRIER_ACCESS_PREDICATION: u32 = 0x200;
pub const D3D12_BARRIER_ACCESS_COPY_DEST: u32 = 0x400;
pub const D3D12_BARRIER_ACCESS_COPY_SOURCE: u32 = 0x800;
pub const D3D12_BARRIER_ACCESS_RESOLVE_DEST: u32 = 0x1000;
pub const D3D12_BARRIER_ACCESS_RESOLVE_SOURCE: u32 = 0x2000;
pub const D3D12_BARRIER_ACCESS_RAYTRACING_ACCELERATION_STRUCTURE_READ: u32 = 0x4000;
pub const D3D12_BARRIER_ACCESS_RAYTRACING_ACCELERATION_STRUCTURE_WRITE: u32 = 0x8000;
pub const D3D12_BARRIER_ACCESS_SHADING_RATE_SOURCE: u32 = 0x1_0000;
pub const D3D12_BARRIER_ACCESS_VIDEO_DECODE_READ: u32 = 0x2_0000;
pub const D3D12_BARRIER_ACCESS_VIDEO_DECODE_WRITE: u32 = 0x4_0000;
pub const D3D12_BARRIER_ACCESS_VIDEO_PROCESS_READ: u32 = 0x8_0000;
pub const D3D12_BARRIER_ACCESS_VIDEO_PROCESS_WRITE: u32 = 0x10_0000;
pub const D3D12_BARRIER_ACCESS_VIDEO_ENCODE_READ: u32 = 0x20_0000;
pub const D3D12_BARRIER_ACCESS_VIDEO_ENCODE_WRITE: u32 = 0x40_0000;
pub const D3D12_BARRIER_ACCESS_NO_ACCESS: u32 = 0x8000_0000;

// `D3D12_BARRIER_LAYOUT`.
pub const D3D12_BARRIER_LAYOUT_UNDEFINED: u32 = 0xffff_ffff;
pub const D3D12_BARRIER_LAYOUT_COMMON: u32 = 0;
pub const D3D12_BARRIER_LAYOUT_PRESENT: u32 = 0;
pub const D3D12_BARRIER_LAYOUT_GENERIC_READ: u32 = 1;
pub const D3D12_BARRIER_LAYOUT_RENDER_TARGET: u32 = 2;
pub const D3D12_BARRIER_LAYOUT_UNORDERED_ACCESS: u32 = 3;
pub const D3D12_BARRIER_LAYOUT_DEPTH_STENCIL_WRITE: u32 = 4;
pub const D3D12_BARRIER_LAYOUT_DEPTH_STENCIL_READ: u32 = 5;
pub const D3D12_BARRIER_LAYOUT_SHADER_RESOURCE: u32 = 6;
pub const D3D12_BARRIER_LAYOUT_COPY_SOURCE: u32 = 7;
pub const D3D12_BARRIER_LAYOUT_COPY_DEST: u32 = 8;
pub const D3D12_BARRIER_LAYOUT_RESOLVE_SOURCE: u32 = 9;
pub const D3D12_BARRIER_LAYOUT_RESOLVE_DEST: u32 = 0xa;
pub const D3D12_BARRIER_LAYOUT_SHADING_RATE_SOURCE: u32 = 0xb;
pub const D3D12_BARRIER_LAYOUT_VIDEO_DECODE_READ: u32 = 0xc;
pub const D3D12_BARRIER_LAYOUT_VIDEO_DECODE_WRITE: u32 = 0xd;
pub const D3D12_BARRIER_LAYOUT_VIDEO_PROCESS_READ: u32 = 0xe;
pub const D3D12_BARRIER_LAYOUT_VIDEO_PROCESS_WRITE: u32 = 0xf;
pub const D3D12_BARRIER_LAYOUT_VIDEO_ENCODE_READ: u32 = 0x10;
pub const D3D12_BARRIER_LAYOUT_VIDEO_ENCODE_WRITE: u32 = 0x11;
pub const D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_COMMON: u32 = 0x12;
pub const D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_GENERIC_READ: u32 = 0x13;
pub const D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_UNORDERED_ACCESS: u32 = 0x14;
pub const D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_SHADER_RESOURCE: u32 = 0x15;
pub const D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_COPY_SOURCE: u32 = 0x16;
pub const D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_COPY_DEST: u32 = 0x17;
pub const D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_COMMON: u32 = 0x18;
pub const D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_GENERIC_READ: u32 = 0x19;
pub const D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_UNORDERED_ACCESS: u32 = 0x1a;
pub const D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_SHADER_RESOURCE: u32 = 0x1b;
pub const D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_COPY_SOURCE: u32 = 0x1c;
pub const D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_COPY_DEST: u32 = 0x1d;
pub const D3D12_BARRIER_LAYOUT_VIDEO_QUEUE_COMMON: u32 = 0x1e;

// `D3D12_BARRIER_TYPE`.
pub const D3D12_BARRIER_TYPE_GLOBAL: u32 = 0;
pub const D3D12_BARRIER_TYPE_TEXTURE: u32 = 1;
pub const D3D12_BARRIER_TYPE_BUFFER: u32 = 2;

// `D3D12_TEXTURE_BARRIER_FLAGS`.
pub const D3D12_TEXTURE_BARRIER_FLAG_NONE: u32 = 0;
pub const D3D12_TEXTURE_BARRIER_FLAG_DISCARD: u32 = 1;

// `D3D12_RESOURCE_STATES`.
pub const D3D12_RESOURCE_STATE_COMMON: u32 = 0;
pub const D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER: u32 = 1;
pub const D3D12_RESOURCE_STATE_INDEX_BUFFER: u32 = 2;
pub const D3D12_RESOURCE_STATE_RENDER_TARGET: u32 = 4;
pub const D3D12_RESOURCE_STATE_UNORDERED_ACCESS: u32 = 8;
pub const D3D12_RESOURCE_STATE_DEPTH_WRITE: u32 = 0x10;
pub const D3D12_RESOURCE_STATE_DEPTH_READ: u32 = 0x20;
pub const D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE: u32 = 0x40;
pub const D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE: u32 = 0x80;
pub const D3D12_RESOURCE_STATE_STREAM_OUT: u32 = 0x100;
pub const D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT: u32 = 0x200;
pub const D3D12_RESOURCE_STATE_COPY_DEST: u32 = 0x400;
pub const D3D12_RESOURCE_STATE_COPY_SOURCE: u32 = 0x800;
pub const D3D12_RESOURCE_STATE_RESOLVE_DEST: u32 = 0x1000;
pub const D3D12_RESOURCE_STATE_RESOLVE_SOURCE: u32 = 0x2000;
pub const D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE: u32 = 0x40_0000;
pub const D3D12_RESOURCE_STATE_SHADING_RATE_SOURCE: u32 = 0x100_0000;
pub const D3D12_RESOURCE_STATE_GENERIC_READ: u32 = D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER
    | D3D12_RESOURCE_STATE_INDEX_BUFFER
    | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE
    | D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE
    | D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT
    | D3D12_RESOURCE_STATE_COPY_SOURCE;
pub const D3D12_RESOURCE_STATE_ALL_SHADER_RESOURCE: u32 = 0xc0;
pub const D3D12_RESOURCE_STATE_PRESENT: u32 = 0;
pub const D3D12_RESOURCE_STATE_PREDICATION: u32 = 0x200;
pub const D3D12_RESOURCE_STATE_VIDEO_DECODE_READ: u32 = 0x1_0000;
pub const D3D12_RESOURCE_STATE_VIDEO_DECODE_WRITE: u32 = 0x2_0000;
pub const D3D12_RESOURCE_STATE_VIDEO_PROCESS_READ: u32 = 0x4_0000;
pub const D3D12_RESOURCE_STATE_VIDEO_PROCESS_WRITE: u32 = 0x8_0000;
pub const D3D12_RESOURCE_STATE_VIDEO_ENCODE_READ: u32 = 0x20_0000;
pub const D3D12_RESOURCE_STATE_VIDEO_ENCODE_WRITE: u32 = 0x80_0000;

// `D3D12_RESOURCE_BARRIER_TYPE`.
pub const D3D12_RESOURCE_BARRIER_TYPE_TRANSITION: u32 = 0;
pub const D3D12_RESOURCE_BARRIER_TYPE_ALIASING: u32 = 1;
pub const D3D12_RESOURCE_BARRIER_TYPE_UAV: u32 = 2;

// `D3D12_RESOURCE_BARRIER_FLAGS`.
pub const D3D12_RESOURCE_BARRIER_FLAG_NONE: u32 = 0;
pub const D3D12_RESOURCE_BARRIER_FLAG_BEGIN_ONLY: u32 = 1;
pub const D3D12_RESOURCE_BARRIER_FLAG_END_ONLY: u32 = 2;

// `D3D12_HEAP_TYPE`.
pub const D3D12_HEAP_TYPE_DEFAULT: u32 = 1;
pub const D3D12_HEAP_TYPE_UPLOAD: u32 = 2;
pub const D3D12_HEAP_TYPE_READBACK: u32 = 3;
pub const D3D12_HEAP_TYPE_CUSTOM: u32 = 4;

// `D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES`.
pub const D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES: u32 = 0xffff_ffff;

/// `D3D12_BARRIER_SUBRESOURCE_RANGE`.
#[repr(C)]
pub struct BarrierSubresourceRange {
    pub index_or_first_mip_level: u32,
    pub num_mip_levels: u32,
    pub first_array_slice: u32,
    pub num_array_slices: u32,
    pub first_plane: u32,
    pub num_planes: u32,
}

/// `D3D12_GLOBAL_BARRIER`.
#[repr(C)]
pub struct GlobalBarrier {
    pub sync_before: u32,
    pub sync_after: u32,
    pub access_before: u32,
    pub access_after: u32,
}

/// `D3D12_TEXTURE_BARRIER`.
#[repr(C)]
pub struct TextureBarrier {
    pub sync_before: u32,
    pub sync_after: u32,
    pub access_before: u32,
    pub access_after: u32,
    pub layout_before: u32,
    pub layout_after: u32,
    /// `ID3D12Resource *`.
    pub resource: *mut c_void,
    pub subresources: BarrierSubresourceRange,
    pub flags: u32,
}

/// `D3D12_BUFFER_BARRIER`.
#[repr(C)]
pub struct BufferBarrier {
    pub sync_before: u32,
    pub sync_after: u32,
    pub access_before: u32,
    pub access_after: u32,
    /// `ID3D12Resource *`.
    pub resource: *mut c_void,
    pub offset: u64,
    pub size: u64,
}

/// The anonymous union of `D3D12_BARRIER_GROUP`: the barriers' array, as the group's type names it.
#[repr(C)]
pub union BarrierGroupBarriers {
    pub global_barriers: *const GlobalBarrier,
    pub texture_barriers: *const TextureBarrier,
    pub buffer_barriers: *const BufferBarrier,
}

/// `D3D12_BARRIER_GROUP`.
#[repr(C)]
pub struct BarrierGroup {
    /// `D3D12_BARRIER_TYPE`.
    pub r#type: u32,
    pub num_barriers: u32,
    pub barriers: BarrierGroupBarriers,
}

/// `D3D12_RESOURCE_TRANSITION_BARRIER`.
#[repr(C)]
pub struct ResourceTransitionBarrier {
    /// `ID3D12Resource *`.
    pub resource: *mut c_void,
    pub subresource: u32,
    /// `D3D12_RESOURCE_STATES`.
    pub state_before: u32,
    /// `D3D12_RESOURCE_STATES`.
    pub state_after: u32,
}

/// `D3D12_RESOURCE_ALIASING_BARRIER`.
#[repr(C)]
pub struct ResourceAliasingBarrier {
    /// `ID3D12Resource *`.
    pub resource_before: *mut c_void,
    /// `ID3D12Resource *`.
    pub resource_after: *mut c_void,
}

/// `D3D12_RESOURCE_UAV_BARRIER`.
#[repr(C)]
pub struct ResourceUavBarrier {
    /// `ID3D12Resource *`.
    pub resource: *mut c_void,
}

/// The anonymous union of `D3D12_RESOURCE_BARRIER`: the arm its type names.
#[repr(C)]
pub union ResourceBarrierBody {
    pub transition: ManuallyDrop<ResourceTransitionBarrier>,
    pub aliasing: ManuallyDrop<ResourceAliasingBarrier>,
    pub uav: ManuallyDrop<ResourceUavBarrier>,
}

/// `D3D12_RESOURCE_BARRIER`.
#[repr(C)]
pub struct ResourceBarrier {
    /// `D3D12_RESOURCE_BARRIER_TYPE`.
    pub r#type: u32,
    /// `D3D12_RESOURCE_BARRIER_FLAGS`.
    pub flags: u32,
    pub barrier: ResourceBarrierBody,
}

// The layouts of clang's `-fdump-record-layouts` over `d3d12.h`: every struct's size and
// alignment, its field offsets and the offset of each union arm, per target. The three 64-bit PE
// targets' dumps are the same.
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: () = {
    assert!(
        size_of::<BarrierSubresourceRange>() == 24 && align_of::<BarrierSubresourceRange>() == 4
    );
    assert!(offset_of!(BarrierSubresourceRange, index_or_first_mip_level) == 0);
    assert!(offset_of!(BarrierSubresourceRange, num_mip_levels) == 4);
    assert!(offset_of!(BarrierSubresourceRange, first_array_slice) == 8);
    assert!(offset_of!(BarrierSubresourceRange, num_array_slices) == 12);
    assert!(offset_of!(BarrierSubresourceRange, first_plane) == 16);
    assert!(offset_of!(BarrierSubresourceRange, num_planes) == 20);
    assert!(size_of::<GlobalBarrier>() == 16 && align_of::<GlobalBarrier>() == 4);
    assert!(offset_of!(GlobalBarrier, sync_before) == 0);
    assert!(offset_of!(GlobalBarrier, sync_after) == 4);
    assert!(offset_of!(GlobalBarrier, access_before) == 8);
    assert!(offset_of!(GlobalBarrier, access_after) == 12);
    assert!(size_of::<TextureBarrier>() == 56 && align_of::<TextureBarrier>() == 4);
    assert!(offset_of!(TextureBarrier, sync_before) == 0);
    assert!(offset_of!(TextureBarrier, sync_after) == 4);
    assert!(offset_of!(TextureBarrier, access_before) == 8);
    assert!(offset_of!(TextureBarrier, access_after) == 12);
    assert!(offset_of!(TextureBarrier, layout_before) == 16);
    assert!(offset_of!(TextureBarrier, layout_after) == 20);
    assert!(offset_of!(TextureBarrier, resource) == 24);
    assert!(offset_of!(TextureBarrier, subresources) == 28);
    assert!(offset_of!(TextureBarrier, flags) == 52);
    assert!(size_of::<BufferBarrier>() == 40 && align_of::<BufferBarrier>() == 8);
    assert!(offset_of!(BufferBarrier, sync_before) == 0);
    assert!(offset_of!(BufferBarrier, sync_after) == 4);
    assert!(offset_of!(BufferBarrier, access_before) == 8);
    assert!(offset_of!(BufferBarrier, access_after) == 12);
    assert!(offset_of!(BufferBarrier, resource) == 16);
    assert!(offset_of!(BufferBarrier, offset) == 24);
    assert!(offset_of!(BufferBarrier, size) == 32);
    assert!(size_of::<BarrierGroup>() == 12 && align_of::<BarrierGroup>() == 4);
    assert!(offset_of!(BarrierGroup, r#type) == 0);
    assert!(offset_of!(BarrierGroup, num_barriers) == 4);
    assert!(offset_of!(BarrierGroup, barriers.global_barriers) == 8);
    assert!(offset_of!(BarrierGroup, barriers.texture_barriers) == 8);
    assert!(offset_of!(BarrierGroup, barriers.buffer_barriers) == 8);
    assert!(
        size_of::<ResourceTransitionBarrier>() == 16
            && align_of::<ResourceTransitionBarrier>() == 4
    );
    assert!(offset_of!(ResourceTransitionBarrier, resource) == 0);
    assert!(offset_of!(ResourceTransitionBarrier, subresource) == 4);
    assert!(offset_of!(ResourceTransitionBarrier, state_before) == 8);
    assert!(offset_of!(ResourceTransitionBarrier, state_after) == 12);
    assert!(
        size_of::<ResourceAliasingBarrier>() == 8 && align_of::<ResourceAliasingBarrier>() == 4
    );
    assert!(offset_of!(ResourceAliasingBarrier, resource_before) == 0);
    assert!(offset_of!(ResourceAliasingBarrier, resource_after) == 4);
    assert!(size_of::<ResourceUavBarrier>() == 4 && align_of::<ResourceUavBarrier>() == 4);
    assert!(offset_of!(ResourceUavBarrier, resource) == 0);
    assert!(size_of::<ResourceBarrier>() == 24 && align_of::<ResourceBarrier>() == 4);
    assert!(offset_of!(ResourceBarrier, r#type) == 0);
    assert!(offset_of!(ResourceBarrier, flags) == 4);
    assert!(offset_of!(ResourceBarrier, barrier.transition) == 8);
    assert!(offset_of!(ResourceBarrier, barrier.aliasing) == 8);
    assert!(offset_of!(ResourceBarrier, barrier.uav) == 8);
};
#[cfg(all(target_os = "windows", target_pointer_width = "64"))]
const _: () = {
    assert!(
        size_of::<BarrierSubresourceRange>() == 24 && align_of::<BarrierSubresourceRange>() == 4
    );
    assert!(offset_of!(BarrierSubresourceRange, index_or_first_mip_level) == 0);
    assert!(offset_of!(BarrierSubresourceRange, num_mip_levels) == 4);
    assert!(offset_of!(BarrierSubresourceRange, first_array_slice) == 8);
    assert!(offset_of!(BarrierSubresourceRange, num_array_slices) == 12);
    assert!(offset_of!(BarrierSubresourceRange, first_plane) == 16);
    assert!(offset_of!(BarrierSubresourceRange, num_planes) == 20);
    assert!(size_of::<GlobalBarrier>() == 16 && align_of::<GlobalBarrier>() == 4);
    assert!(offset_of!(GlobalBarrier, sync_before) == 0);
    assert!(offset_of!(GlobalBarrier, sync_after) == 4);
    assert!(offset_of!(GlobalBarrier, access_before) == 8);
    assert!(offset_of!(GlobalBarrier, access_after) == 12);
    assert!(size_of::<TextureBarrier>() == 64 && align_of::<TextureBarrier>() == 8);
    assert!(offset_of!(TextureBarrier, sync_before) == 0);
    assert!(offset_of!(TextureBarrier, sync_after) == 4);
    assert!(offset_of!(TextureBarrier, access_before) == 8);
    assert!(offset_of!(TextureBarrier, access_after) == 12);
    assert!(offset_of!(TextureBarrier, layout_before) == 16);
    assert!(offset_of!(TextureBarrier, layout_after) == 20);
    assert!(offset_of!(TextureBarrier, resource) == 24);
    assert!(offset_of!(TextureBarrier, subresources) == 32);
    assert!(offset_of!(TextureBarrier, flags) == 56);
    assert!(size_of::<BufferBarrier>() == 40 && align_of::<BufferBarrier>() == 8);
    assert!(offset_of!(BufferBarrier, sync_before) == 0);
    assert!(offset_of!(BufferBarrier, sync_after) == 4);
    assert!(offset_of!(BufferBarrier, access_before) == 8);
    assert!(offset_of!(BufferBarrier, access_after) == 12);
    assert!(offset_of!(BufferBarrier, resource) == 16);
    assert!(offset_of!(BufferBarrier, offset) == 24);
    assert!(offset_of!(BufferBarrier, size) == 32);
    assert!(size_of::<BarrierGroup>() == 16 && align_of::<BarrierGroup>() == 8);
    assert!(offset_of!(BarrierGroup, r#type) == 0);
    assert!(offset_of!(BarrierGroup, num_barriers) == 4);
    assert!(offset_of!(BarrierGroup, barriers.global_barriers) == 8);
    assert!(offset_of!(BarrierGroup, barriers.texture_barriers) == 8);
    assert!(offset_of!(BarrierGroup, barriers.buffer_barriers) == 8);
    assert!(
        size_of::<ResourceTransitionBarrier>() == 24
            && align_of::<ResourceTransitionBarrier>() == 8
    );
    assert!(offset_of!(ResourceTransitionBarrier, resource) == 0);
    assert!(offset_of!(ResourceTransitionBarrier, subresource) == 8);
    assert!(offset_of!(ResourceTransitionBarrier, state_before) == 12);
    assert!(offset_of!(ResourceTransitionBarrier, state_after) == 16);
    assert!(
        size_of::<ResourceAliasingBarrier>() == 16 && align_of::<ResourceAliasingBarrier>() == 8
    );
    assert!(offset_of!(ResourceAliasingBarrier, resource_before) == 0);
    assert!(offset_of!(ResourceAliasingBarrier, resource_after) == 8);
    assert!(size_of::<ResourceUavBarrier>() == 8 && align_of::<ResourceUavBarrier>() == 8);
    assert!(offset_of!(ResourceUavBarrier, resource) == 0);
    assert!(size_of::<ResourceBarrier>() == 32 && align_of::<ResourceBarrier>() == 8);
    assert!(offset_of!(ResourceBarrier, r#type) == 0);
    assert!(offset_of!(ResourceBarrier, flags) == 4);
    assert!(offset_of!(ResourceBarrier, barrier.transition) == 8);
    assert!(offset_of!(ResourceBarrier, barrier.aliasing) == 8);
    assert!(offset_of!(ResourceBarrier, barrier.uav) == 8);
};
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: () = {
    assert!(
        size_of::<BarrierSubresourceRange>() == 24 && align_of::<BarrierSubresourceRange>() == 4
    );
    assert!(offset_of!(BarrierSubresourceRange, index_or_first_mip_level) == 0);
    assert!(offset_of!(BarrierSubresourceRange, num_mip_levels) == 4);
    assert!(offset_of!(BarrierSubresourceRange, first_array_slice) == 8);
    assert!(offset_of!(BarrierSubresourceRange, num_array_slices) == 12);
    assert!(offset_of!(BarrierSubresourceRange, first_plane) == 16);
    assert!(offset_of!(BarrierSubresourceRange, num_planes) == 20);
    assert!(size_of::<GlobalBarrier>() == 16 && align_of::<GlobalBarrier>() == 4);
    assert!(offset_of!(GlobalBarrier, sync_before) == 0);
    assert!(offset_of!(GlobalBarrier, sync_after) == 4);
    assert!(offset_of!(GlobalBarrier, access_before) == 8);
    assert!(offset_of!(GlobalBarrier, access_after) == 12);
    assert!(size_of::<TextureBarrier>() == 64 && align_of::<TextureBarrier>() == 8);
    assert!(offset_of!(TextureBarrier, sync_before) == 0);
    assert!(offset_of!(TextureBarrier, sync_after) == 4);
    assert!(offset_of!(TextureBarrier, access_before) == 8);
    assert!(offset_of!(TextureBarrier, access_after) == 12);
    assert!(offset_of!(TextureBarrier, layout_before) == 16);
    assert!(offset_of!(TextureBarrier, layout_after) == 20);
    assert!(offset_of!(TextureBarrier, resource) == 24);
    assert!(offset_of!(TextureBarrier, subresources) == 32);
    assert!(offset_of!(TextureBarrier, flags) == 56);
    assert!(size_of::<BufferBarrier>() == 40 && align_of::<BufferBarrier>() == 8);
    assert!(offset_of!(BufferBarrier, sync_before) == 0);
    assert!(offset_of!(BufferBarrier, sync_after) == 4);
    assert!(offset_of!(BufferBarrier, access_before) == 8);
    assert!(offset_of!(BufferBarrier, access_after) == 12);
    assert!(offset_of!(BufferBarrier, resource) == 16);
    assert!(offset_of!(BufferBarrier, offset) == 24);
    assert!(offset_of!(BufferBarrier, size) == 32);
    assert!(size_of::<BarrierGroup>() == 16 && align_of::<BarrierGroup>() == 8);
    assert!(offset_of!(BarrierGroup, r#type) == 0);
    assert!(offset_of!(BarrierGroup, num_barriers) == 4);
    assert!(offset_of!(BarrierGroup, barriers.global_barriers) == 8);
    assert!(offset_of!(BarrierGroup, barriers.texture_barriers) == 8);
    assert!(offset_of!(BarrierGroup, barriers.buffer_barriers) == 8);
    assert!(
        size_of::<ResourceTransitionBarrier>() == 24
            && align_of::<ResourceTransitionBarrier>() == 8
    );
    assert!(offset_of!(ResourceTransitionBarrier, resource) == 0);
    assert!(offset_of!(ResourceTransitionBarrier, subresource) == 8);
    assert!(offset_of!(ResourceTransitionBarrier, state_before) == 12);
    assert!(offset_of!(ResourceTransitionBarrier, state_after) == 16);
    assert!(
        size_of::<ResourceAliasingBarrier>() == 16 && align_of::<ResourceAliasingBarrier>() == 8
    );
    assert!(offset_of!(ResourceAliasingBarrier, resource_before) == 0);
    assert!(offset_of!(ResourceAliasingBarrier, resource_after) == 8);
    assert!(size_of::<ResourceUavBarrier>() == 8 && align_of::<ResourceUavBarrier>() == 8);
    assert!(offset_of!(ResourceUavBarrier, resource) == 0);
    assert!(size_of::<ResourceBarrier>() == 32 && align_of::<ResourceBarrier>() == 8);
    assert!(offset_of!(ResourceBarrier, r#type) == 0);
    assert!(offset_of!(ResourceBarrier, flags) == 4);
    assert!(offset_of!(ResourceBarrier, barrier.transition) == 8);
    assert!(offset_of!(ResourceBarrier, barrier.aliasing) == 8);
    assert!(offset_of!(ResourceBarrier, barrier.uav) == 8);
};
