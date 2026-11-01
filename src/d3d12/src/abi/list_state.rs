//! C-layout mirrors for command-list state (viewports, scissors, vertex/index/stream-output views).

use core::mem::size_of;

pub const D3D12_COMMAND_LIST_TYPE_DIRECT: u32 = 0;
pub const D3D12_COMMAND_LIST_TYPE_BUNDLE: u32 = 1;
pub const D3D12_COMMAND_LIST_TYPE_COMPUTE: u32 = 2;
pub const D3D12_COMMAND_LIST_TYPE_COPY: u32 = 3;

pub const D3D_PRIMITIVE_TOPOLOGY_UNDEFINED: u32 = 0;
pub const D3D_PRIMITIVE_TOPOLOGY_POINTLIST: u32 = 1;
pub const D3D_PRIMITIVE_TOPOLOGY_LINELIST: u32 = 2;
pub const D3D_PRIMITIVE_TOPOLOGY_LINESTRIP: u32 = 3;
pub const D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST: u32 = 4;
pub const D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP: u32 = 5;
pub const D3D_PRIMITIVE_TOPOLOGY_LINELIST_ADJ: u32 = 10;
pub const D3D_PRIMITIVE_TOPOLOGY_LINESTRIP_ADJ: u32 = 11;
pub const D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST_ADJ: u32 = 12;
pub const D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP_ADJ: u32 = 13;
pub const D3D_PRIMITIVE_TOPOLOGY_1_CONTROL_POINT_PATCHLIST: u32 = 33;
pub const D3D_PRIMITIVE_TOPOLOGY_32_CONTROL_POINT_PATCHLIST: u32 = 64;

pub const DXGI_FORMAT_R32_UINT: u32 = 42;
pub const DXGI_FORMAT_R16_UINT: u32 = 57;

pub const D3D12_SHADING_RATE_1X1: u32 = 0;

pub const D3D12_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE: usize = 16;
pub const D3D12_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT: usize = 32;
pub const D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT: usize = 8;
pub const MAX_SCISSOR_EXTENT: i32 = 16384;

/// `D3D12_VIEWPORT` layout (`d3d12.idl`).
#[repr(C)]
pub struct D3d12Viewport {
    pub top_left_x: f32,
    pub top_left_y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

/// `D3D12_RECT` / `RECT` layout (`windef.h`).
#[repr(C)]
pub struct D3d12Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// `D3D12_VERTEX_BUFFER_VIEW` layout (`d3d12.idl`).
#[repr(C)]
pub struct D3d12VertexBufferView {
    pub buffer_location: u64,
    pub size_in_bytes: u32,
    pub stride_in_bytes: u32,
}

/// `D3D12_INDEX_BUFFER_VIEW` layout (`d3d12.idl`).
#[repr(C)]
pub struct D3d12IndexBufferView {
    pub buffer_location: u64,
    pub size_in_bytes: u32,
    pub format: u32,
}

/// `D3D12_STREAM_OUTPUT_BUFFER_VIEW` layout (`d3d12.idl`).
#[repr(C)]
pub struct D3d12StreamOutputBufferView {
    pub buffer_location: u64,
    pub size_in_bytes: u64,
    pub buffer_filled_size_location: u64,
}

/// `D3D12_DRAW_ARGUMENTS` layout (`d3d12.idl`).
#[repr(C)]
pub struct D3d12DrawArguments {
    pub vertex_count_per_instance: u32,
    pub instance_count: u32,
    pub start_vertex_location: u32,
    pub start_instance_location: u32,
}

/// `D3D12_DRAW_INDEXED_ARGUMENTS` layout (`d3d12.idl`).
#[repr(C)]
pub struct D3d12DrawIndexedArguments {
    pub index_count_per_instance: u32,
    pub instance_count: u32,
    pub start_index_location: u32,
    pub base_vertex_location: i32,
    pub start_instance_location: u32,
}

/// `D3D12_DISPATCH_ARGUMENTS` layout (`d3d12.idl`).
#[repr(C)]
pub struct D3d12DispatchArguments {
    pub thread_group_count_x: u32,
    pub thread_group_count_y: u32,
    pub thread_group_count_z: u32,
}

/// GPU-heap vertex buffer table entry consumed by the vertex shader (`DXMT_VERTEX_BUFFER_ENTRY`).
#[repr(C)]
pub struct VertexBufferEntry {
    pub buffer_location: u64,
    pub stride: u32,
    pub size: u32,
}

const _: [(); 24] = [(); size_of::<D3d12Viewport>()];
const _: [(); 16] = [(); size_of::<D3d12Rect>()];
const _: [(); 16] = [(); size_of::<D3d12VertexBufferView>()];
const _: [(); 16] = [(); size_of::<D3d12IndexBufferView>()];
const _: [(); 24] = [(); size_of::<D3d12StreamOutputBufferView>()];
const _: [(); 16] = [(); size_of::<D3d12DrawArguments>()];
const _: [(); 20] = [(); size_of::<D3d12DrawIndexedArguments>()];
const _: [(); 12] = [(); size_of::<D3d12DispatchArguments>()];
const _: [(); 16] = [(); size_of::<VertexBufferEntry>()];
