//! C-layout mirrors for command-list operations (copies, clears, render passes, immediate writes).
//!
//! The structures and constants as `d3d12.h` defines them. Every enumeration is its `u32` and the
//! fields are the C's in snake case. A structure's layout is the C's on every target, asserted at
//! the end against clang's `-fdump-record-layouts`.

use core::{
    ffi::c_void,
    mem::{ManuallyDrop, offset_of},
};

pub use super::{
    barrier::{
        BarrierGroup, BarrierGroupBarriers, BarrierSubresourceRange, BufferBarrier, GlobalBarrier,
        ResourceAliasingBarrier, ResourceBarrier, ResourceBarrierBody, ResourceTransitionBarrier,
        ResourceUavBarrier, TextureBarrier,
    },
    resource::{D3d12Box, PlacedSubresourceFootprint, SubresourceFootprint},
    vtbl::D3d12CpuDescriptorHandle,
};

// `D3D12_TEXTURE_COPY_TYPE`.
pub const D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX: u32 = 0;
pub const D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT: u32 = 1;

// `D3D12_CLEAR_FLAGS`.
pub const D3D12_CLEAR_FLAG_DEPTH: u32 = 0x1;
pub const D3D12_CLEAR_FLAG_STENCIL: u32 = 0x2;

// `D3D12_QUERY_TYPE`.
pub const D3D12_QUERY_TYPE_OCCLUSION: u32 = 0;
pub const D3D12_QUERY_TYPE_BINARY_OCCLUSION: u32 = 1;
pub const D3D12_QUERY_TYPE_TIMESTAMP: u32 = 2;
pub const D3D12_QUERY_TYPE_PIPELINE_STATISTICS: u32 = 3;
pub const D3D12_QUERY_TYPE_SO_STATISTICS_STREAM0: u32 = 4;
pub const D3D12_QUERY_TYPE_SO_STATISTICS_STREAM1: u32 = 5;
pub const D3D12_QUERY_TYPE_SO_STATISTICS_STREAM2: u32 = 6;
pub const D3D12_QUERY_TYPE_SO_STATISTICS_STREAM3: u32 = 7;
pub const D3D12_QUERY_TYPE_VIDEO_DECODE_STATISTICS: u32 = 8;
pub const D3D12_QUERY_TYPE_PIPELINE_STATISTICS1: u32 = 10;

// `D3D12_RESOLVE_MODE`.
pub const D3D12_RESOLVE_MODE_DECOMPRESS: u32 = 0;
pub const D3D12_RESOLVE_MODE_MIN: u32 = 1;
pub const D3D12_RESOLVE_MODE_MAX: u32 = 2;
pub const D3D12_RESOLVE_MODE_AVERAGE: u32 = 3;
pub const D3D12_RESOLVE_MODE_ENCODE_SAMPLER_FEEDBACK: u32 = 4;
pub const D3D12_RESOLVE_MODE_DECODE_SAMPLER_FEEDBACK: u32 = 5;

// `D3D12_WRITEBUFFERIMMEDIATE_MODE`.
pub const D3D12_WRITEBUFFERIMMEDIATE_MODE_DEFAULT: u32 = 0;
pub const D3D12_WRITEBUFFERIMMEDIATE_MODE_MARKER_IN: u32 = 1;
pub const D3D12_WRITEBUFFERIMMEDIATE_MODE_MARKER_OUT: u32 = 2;

// `D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE`.
pub const D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_DISCARD: u32 = 0;
pub const D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_PRESERVE: u32 = 1;
pub const D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_CLEAR: u32 = 2;
pub const D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_NO_ACCESS: u32 = 3;

// `D3D12_RENDER_PASS_ENDING_ACCESS_TYPE`.
pub const D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_DISCARD: u32 = 0;
pub const D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_PRESERVE: u32 = 1;
pub const D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_RESOLVE: u32 = 2;
pub const D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_NO_ACCESS: u32 = 3;

// `D3D12_RENDER_PASS_FLAGS`.
pub const D3D12_RENDER_PASS_FLAG_NONE: u32 = 0;
pub const D3D12_RENDER_PASS_FLAG_ALLOW_UAV_WRITES: u32 = 0x1;
pub const D3D12_RENDER_PASS_FLAG_SUSPENDING_PASS: u32 = 0x2;
pub const D3D12_RENDER_PASS_FLAG_RESUMING_PASS: u32 = 0x4;
pub const D3D12_RENDER_PASS_FLAG_BIND_READ_ONLY_DEPTH: u32 = 0x8;
pub const D3D12_RENDER_PASS_FLAG_BIND_READ_ONLY_STENCIL: u32 = 0x10;

pub const D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT: u32 = 8;

// `D3D12_INDIRECT_ARGUMENT_TYPE`.
pub const D3D12_INDIRECT_ARGUMENT_TYPE_DRAW: u32 = 0;
pub const D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED: u32 = 1;
pub const D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH: u32 = 2;

/// `D3D12_RECT` (`RECT`).
#[repr(C)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The anonymous union of `D3D12_TEXTURE_COPY_LOCATION`.
#[repr(C)]
pub union TextureCopyLocationBody {
    pub placed_footprint: ManuallyDrop<PlacedSubresourceFootprint>,
    pub subresource_index: u32,
}

/// `D3D12_TEXTURE_COPY_LOCATION`.
#[repr(C)]
pub struct TextureCopyLocation {
    /// `ID3D12Resource *`.
    pub resource: *mut c_void,
    /// `D3D12_TEXTURE_COPY_TYPE`.
    pub r#type: u32,
    pub location: TextureCopyLocationBody,
}

/// `D3D12_TILED_RESOURCE_COORDINATE`.
#[repr(C)]
pub struct TiledResourceCoordinate {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub subresource: u32,
}

/// `D3D12_TILE_REGION_SIZE`.
#[repr(C)]
pub struct TileRegionSize {
    pub num_tiles: u32,
    pub use_box: i32,
    pub width: u32,
    pub height: u16,
    pub depth: u16,
}

/// `D3D12_DISCARD_REGION`.
#[repr(C)]
pub struct DiscardRegion {
    pub num_rects: u32,
    pub rects: *const Rect,
    pub first_subresource: u32,
    pub num_subresources: u32,
}

/// `D3D12_WRITEBUFFERIMMEDIATE_PARAMETER`.
#[repr(C)]
pub struct WriteBufferImmediateParameter {
    pub dest: u64,
    pub value: u32,
}

/// `D3D12_DEPTH_STENCIL_VALUE`.
#[repr(C)]
pub struct DepthStencilValue {
    pub depth: f32,
    pub stencil: u8,
}

/// The anonymous union of `D3D12_CLEAR_VALUE`.
#[repr(C)]
pub union ClearValueBody {
    pub color: [f32; 4],
    pub depth_stencil: ManuallyDrop<DepthStencilValue>,
}

/// `D3D12_CLEAR_VALUE`.
#[repr(C)]
pub struct ClearValue {
    /// `DXGI_FORMAT`.
    pub format: u32,
    pub value: ClearValueBody,
}

/// `D3D12_RENDER_PASS_BEGINNING_ACCESS_CLEAR_PARAMETERS`.
#[repr(C)]
pub struct RenderPassBeginningAccessClearParameters {
    pub clear_value: ClearValue,
}

/// The anonymous union of `D3D12_RENDER_PASS_BEGINNING_ACCESS`.
#[repr(C)]
pub union RenderPassBeginningAccessBody {
    pub clear: ManuallyDrop<RenderPassBeginningAccessClearParameters>,
}

/// `D3D12_RENDER_PASS_BEGINNING_ACCESS`.
#[repr(C)]
pub struct RenderPassBeginningAccess {
    /// `D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE`.
    pub r#type: u32,
    pub access: RenderPassBeginningAccessBody,
}

/// `D3D12_RENDER_PASS_ENDING_ACCESS_RESOLVE_SUBRESOURCE_PARAMETERS`.
#[repr(C)]
pub struct RenderPassEndingAccessResolveSubresourceParameters {
    pub src_subresource: u32,
    pub dst_subresource: u32,
    pub dst_x: u32,
    pub dst_y: u32,
    pub src_rect: Rect,
}

/// `D3D12_RENDER_PASS_ENDING_ACCESS_RESOLVE_PARAMETERS`.
#[repr(C)]
pub struct RenderPassEndingAccessResolveParameters {
    /// `ID3D12Resource *`.
    pub src_resource: *mut c_void,
    /// `ID3D12Resource *`.
    pub dst_resource: *mut c_void,
    pub subresource_count: u32,
    pub subresource_parameters: *const RenderPassEndingAccessResolveSubresourceParameters,
    /// `DXGI_FORMAT`.
    pub format: u32,
    /// `D3D12_RESOLVE_MODE`.
    pub resolve_mode: u32,
    pub preserve_resolve_source: i32,
}

/// The anonymous union of `D3D12_RENDER_PASS_ENDING_ACCESS`.
#[repr(C)]
pub union RenderPassEndingAccessBody {
    pub resolve: ManuallyDrop<RenderPassEndingAccessResolveParameters>,
}

/// `D3D12_RENDER_PASS_ENDING_ACCESS`.
#[repr(C)]
pub struct RenderPassEndingAccess {
    /// `D3D12_RENDER_PASS_ENDING_ACCESS_TYPE`.
    pub r#type: u32,
    pub access: RenderPassEndingAccessBody,
}

/// `D3D12_RENDER_PASS_RENDER_TARGET_DESC`.
#[repr(C)]
pub struct RenderPassRenderTargetDesc {
    pub cpu_descriptor: D3d12CpuDescriptorHandle,
    pub beginning_access: RenderPassBeginningAccess,
    pub ending_access: RenderPassEndingAccess,
}

/// `D3D12_RENDER_PASS_DEPTH_STENCIL_DESC`.
#[repr(C)]
pub struct RenderPassDepthStencilDesc {
    pub cpu_descriptor: D3d12CpuDescriptorHandle,
    pub depth_beginning_access: RenderPassBeginningAccess,
    pub stencil_beginning_access: RenderPassBeginningAccess,
    pub depth_ending_access: RenderPassEndingAccess,
    pub stencil_ending_access: RenderPassEndingAccess,
}

// Target-independent record layouts asserted against clang's `-fdump-record-layouts`.
const _: () = {
    assert!(size_of::<Rect>() == 16 && align_of::<Rect>() == 4);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);

    assert!(
        size_of::<TiledResourceCoordinate>() == 16 && align_of::<TiledResourceCoordinate>() == 4
    );
    assert!(offset_of!(TiledResourceCoordinate, x) == 0);
    assert!(offset_of!(TiledResourceCoordinate, y) == 4);
    assert!(offset_of!(TiledResourceCoordinate, z) == 8);
    assert!(offset_of!(TiledResourceCoordinate, subresource) == 12);

    assert!(size_of::<TileRegionSize>() == 16 && align_of::<TileRegionSize>() == 4);
    assert!(offset_of!(TileRegionSize, num_tiles) == 0);
    assert!(offset_of!(TileRegionSize, use_box) == 4);
    assert!(offset_of!(TileRegionSize, width) == 8);
    assert!(offset_of!(TileRegionSize, height) == 12);
    assert!(offset_of!(TileRegionSize, depth) == 14);

    assert!(
        size_of::<WriteBufferImmediateParameter>() == 16
            && align_of::<WriteBufferImmediateParameter>() == 8
    );
    assert!(offset_of!(WriteBufferImmediateParameter, dest) == 0);
    assert!(offset_of!(WriteBufferImmediateParameter, value) == 8);

    assert!(size_of::<DepthStencilValue>() == 8 && align_of::<DepthStencilValue>() == 4);
    assert!(offset_of!(DepthStencilValue, depth) == 0);
    assert!(offset_of!(DepthStencilValue, stencil) == 4);

    assert!(size_of::<ClearValue>() == 20 && align_of::<ClearValue>() == 4);
    assert!(offset_of!(ClearValue, format) == 0);
    assert!(offset_of!(ClearValue, value.color) == 4);
    assert!(offset_of!(ClearValue, value.depth_stencil) == 4);

    assert!(
        size_of::<RenderPassBeginningAccessClearParameters>() == 20
            && align_of::<RenderPassBeginningAccessClearParameters>() == 4
    );
    assert!(offset_of!(RenderPassBeginningAccessClearParameters, clear_value) == 0);

    assert!(
        size_of::<RenderPassBeginningAccess>() == 24
            && align_of::<RenderPassBeginningAccess>() == 4
    );
    assert!(offset_of!(RenderPassBeginningAccess, r#type) == 0);
    assert!(offset_of!(RenderPassBeginningAccess, access.clear) == 4);

    assert!(
        size_of::<RenderPassEndingAccessResolveSubresourceParameters>() == 32
            && align_of::<RenderPassEndingAccessResolveSubresourceParameters>() == 4
    );
    assert!(
        offset_of!(
            RenderPassEndingAccessResolveSubresourceParameters,
            src_subresource
        ) == 0
    );
    assert!(
        offset_of!(
            RenderPassEndingAccessResolveSubresourceParameters,
            dst_subresource
        ) == 4
    );
    assert!(offset_of!(RenderPassEndingAccessResolveSubresourceParameters, dst_x) == 8);
    assert!(offset_of!(RenderPassEndingAccessResolveSubresourceParameters, dst_y) == 12);
    assert!(offset_of!(RenderPassEndingAccessResolveSubresourceParameters, src_rect) == 16);
};

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(size_of::<TextureCopyLocation>() == 40 && align_of::<TextureCopyLocation>() == 8);
    assert!(offset_of!(TextureCopyLocation, resource) == 0);
    assert!(offset_of!(TextureCopyLocation, r#type) == 4);
    assert!(offset_of!(TextureCopyLocation, location.placed_footprint) == 8);
    assert!(offset_of!(TextureCopyLocation, location.subresource_index) == 8);

    assert!(size_of::<DiscardRegion>() == 16 && align_of::<DiscardRegion>() == 4);
    assert!(offset_of!(DiscardRegion, num_rects) == 0);
    assert!(offset_of!(DiscardRegion, rects) == 4);
    assert!(offset_of!(DiscardRegion, first_subresource) == 8);
    assert!(offset_of!(DiscardRegion, num_subresources) == 12);

    assert!(
        size_of::<RenderPassEndingAccessResolveParameters>() == 28
            && align_of::<RenderPassEndingAccessResolveParameters>() == 4
    );
    assert!(offset_of!(RenderPassEndingAccessResolveParameters, src_resource) == 0);
    assert!(offset_of!(RenderPassEndingAccessResolveParameters, dst_resource) == 4);
    assert!(offset_of!(RenderPassEndingAccessResolveParameters, subresource_count) == 8);
    assert!(
        offset_of!(
            RenderPassEndingAccessResolveParameters,
            subresource_parameters
        ) == 12
    );
    assert!(offset_of!(RenderPassEndingAccessResolveParameters, format) == 16);
    assert!(offset_of!(RenderPassEndingAccessResolveParameters, resolve_mode) == 20);
    assert!(
        offset_of!(
            RenderPassEndingAccessResolveParameters,
            preserve_resolve_source
        ) == 24
    );

    assert!(size_of::<RenderPassEndingAccess>() == 32 && align_of::<RenderPassEndingAccess>() == 4);
    assert!(offset_of!(RenderPassEndingAccess, r#type) == 0);
    assert!(offset_of!(RenderPassEndingAccess, access.resolve) == 4);

    assert!(
        size_of::<RenderPassRenderTargetDesc>() == 60
            && align_of::<RenderPassRenderTargetDesc>() == 4
    );
    assert!(offset_of!(RenderPassRenderTargetDesc, cpu_descriptor) == 0);
    assert!(offset_of!(RenderPassRenderTargetDesc, beginning_access) == 4);
    assert!(offset_of!(RenderPassRenderTargetDesc, ending_access) == 28);

    assert!(
        size_of::<RenderPassDepthStencilDesc>() == 116
            && align_of::<RenderPassDepthStencilDesc>() == 4
    );
    assert!(offset_of!(RenderPassDepthStencilDesc, cpu_descriptor) == 0);
    assert!(offset_of!(RenderPassDepthStencilDesc, depth_beginning_access) == 4);
    assert!(offset_of!(RenderPassDepthStencilDesc, stencil_beginning_access) == 28);
    assert!(offset_of!(RenderPassDepthStencilDesc, depth_ending_access) == 52);
    assert!(offset_of!(RenderPassDepthStencilDesc, stencil_ending_access) == 84);
};

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<TextureCopyLocation>() == 48 && align_of::<TextureCopyLocation>() == 8);
    assert!(offset_of!(TextureCopyLocation, resource) == 0);
    assert!(offset_of!(TextureCopyLocation, r#type) == 8);
    assert!(offset_of!(TextureCopyLocation, location.placed_footprint) == 16);
    assert!(offset_of!(TextureCopyLocation, location.subresource_index) == 16);

    assert!(size_of::<DiscardRegion>() == 24 && align_of::<DiscardRegion>() == 8);
    assert!(offset_of!(DiscardRegion, num_rects) == 0);
    assert!(offset_of!(DiscardRegion, rects) == 8);
    assert!(offset_of!(DiscardRegion, first_subresource) == 16);
    assert!(offset_of!(DiscardRegion, num_subresources) == 20);

    assert!(
        size_of::<RenderPassEndingAccessResolveParameters>() == 48
            && align_of::<RenderPassEndingAccessResolveParameters>() == 8
    );
    assert!(offset_of!(RenderPassEndingAccessResolveParameters, src_resource) == 0);
    assert!(offset_of!(RenderPassEndingAccessResolveParameters, dst_resource) == 8);
    assert!(offset_of!(RenderPassEndingAccessResolveParameters, subresource_count) == 16);
    assert!(
        offset_of!(
            RenderPassEndingAccessResolveParameters,
            subresource_parameters
        ) == 24
    );
    assert!(offset_of!(RenderPassEndingAccessResolveParameters, format) == 32);
    assert!(offset_of!(RenderPassEndingAccessResolveParameters, resolve_mode) == 36);
    assert!(
        offset_of!(
            RenderPassEndingAccessResolveParameters,
            preserve_resolve_source
        ) == 40
    );

    assert!(size_of::<RenderPassEndingAccess>() == 56 && align_of::<RenderPassEndingAccess>() == 8);
    assert!(offset_of!(RenderPassEndingAccess, r#type) == 0);
    assert!(offset_of!(RenderPassEndingAccess, access.resolve) == 8);

    assert!(
        size_of::<RenderPassRenderTargetDesc>() == 88
            && align_of::<RenderPassRenderTargetDesc>() == 8
    );
    assert!(offset_of!(RenderPassRenderTargetDesc, cpu_descriptor) == 0);
    assert!(offset_of!(RenderPassRenderTargetDesc, beginning_access) == 8);
    assert!(offset_of!(RenderPassRenderTargetDesc, ending_access) == 32);

    assert!(
        size_of::<RenderPassDepthStencilDesc>() == 168
            && align_of::<RenderPassDepthStencilDesc>() == 8
    );
    assert!(offset_of!(RenderPassDepthStencilDesc, cpu_descriptor) == 0);
    assert!(offset_of!(RenderPassDepthStencilDesc, depth_beginning_access) == 8);
    assert!(offset_of!(RenderPassDepthStencilDesc, stencil_beginning_access) == 32);
    assert!(offset_of!(RenderPassDepthStencilDesc, depth_ending_access) == 56);
    assert!(offset_of!(RenderPassDepthStencilDesc, stencil_ending_access) == 112);
};
