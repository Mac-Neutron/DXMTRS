//! Buffer/texture copies, MSAA resolves, and query recording (`d3d12_command_list.cpp`).

use core::ffi::c_void;

/// `ID3D12GraphicsCommandList::CopyBufferRegion` (`d3d12_command_list.cpp:547`).
///
/// # Safety
///
/// `this`, `dst_buffer`, and `src_buffer` are caller-supplied pointers.
pub unsafe fn copy_buffer_region(
    _this: *mut c_void,
    _dst_buffer: *mut c_void,
    _dst_offset: u64,
    _src_buffer: *mut c_void,
    _src_offset: u64,
    _num_bytes: u64,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::CopyBufferRegion");
}

/// `ID3D12GraphicsCommandList::CopyTextureRegion` (`d3d12_command_list.cpp:587`).
///
/// # Safety
///
/// `this`, `dst`, `src`, and `src_box` are caller-supplied pointers.
pub unsafe fn copy_texture_region(
    _this: *mut c_void,
    _dst: *const c_void,
    _dst_x: u32,
    _dst_y: u32,
    _dst_z: u32,
    _src: *const c_void,
    _src_box: *const c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::CopyTextureRegion");
}

/// `ID3D12GraphicsCommandList::CopyResource` (`d3d12_command_list.cpp:1132`).
///
/// # Safety
///
/// `this`, `dst_resource`, and `src_resource` are caller-supplied pointers.
pub unsafe fn copy_resource(
    _this: *mut c_void,
    _dst_resource: *mut c_void,
    _src_resource: *mut c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::CopyResource");
}

/// `ID3D12GraphicsCommandList::CopyTiles` (`d3d12_command_list.cpp:1197`).
///
/// # Safety
///
/// `this` and the copy arguments are caller-supplied pointers.
pub unsafe fn copy_tiles(
    _this: *mut c_void,
    _tiled_resource: *mut c_void,
    _tile_region_start_coord: *const c_void,
    _tile_region_size: *const c_void,
    _buffer: *mut c_void,
    _buffer_start_offset_in_bytes: u64,
    _flags: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::CopyTiles");
}

/// `ID3D12GraphicsCommandList::ResolveSubresource` (`d3d12_command_list.cpp:1209`).
///
/// # Safety
///
/// `this`, `dst_resource`, and `src_resource` are caller-supplied pointers.
pub unsafe fn resolve_subresource(
    _this: *mut c_void,
    _dst_resource: *mut c_void,
    _dst_subresource: u32,
    _src_resource: *mut c_void,
    _src_subresource: u32,
    _format: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ResolveSubresource");
}

/// `ID3D12GraphicsCommandList::BeginQuery` (`d3d12_command_list.cpp:1848`).
///
/// # Safety
///
/// `this` and `query_heap` are caller-supplied pointers.
pub unsafe fn begin_query(
    _this: *mut c_void,
    _query_heap: *mut c_void,
    _query_type: u32,
    _index: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::BeginQuery");
}

/// `ID3D12GraphicsCommandList::EndQuery` (`d3d12_command_list.cpp:1873`).
///
/// # Safety
///
/// `this` and `query_heap` are caller-supplied pointers.
pub unsafe fn end_query(
    _this: *mut c_void,
    _query_heap: *mut c_void,
    _query_type: u32,
    _index: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::EndQuery");
}

/// `ID3D12GraphicsCommandList::ResolveQueryData` (`d3d12_command_list.cpp:1915`).
///
/// # Safety
///
/// `this`, `query_heap`, and `destination_buffer` are caller-supplied pointers.
pub unsafe fn resolve_query_data(
    _this: *mut c_void,
    _query_heap: *mut c_void,
    _query_type: u32,
    _start_index: u32,
    _num_queries: u32,
    _destination_buffer: *mut c_void,
    _aligned_destination_buffer_offset: u64,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ResolveQueryData");
}

/// `ID3D12GraphicsCommandList1::AtomicCopyBufferUINT` (`d3d12_command_list.cpp:2088`).
///
/// # Safety
///
/// `this` and the copy arguments are caller-supplied pointers.
pub unsafe extern "system-unwind" fn atomic_copy_buffer_uint(
    _this: *mut c_void,
    _dst_buffer: *mut c_void,
    _dst_offset: u64,
    _src_buffer: *mut c_void,
    _src_offset: u64,
    _dependencies: u32,
    _dependent_resources: *const *mut c_void,
    _dependent_subresource_ranges: *const c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::AtomicCopyBufferUINT");
}

/// `ID3D12GraphicsCommandList1::AtomicCopyBufferUINT64` (`d3d12_command_list.cpp:2101`).
///
/// # Safety
///
/// `this` and the copy arguments are caller-supplied pointers.
pub unsafe extern "system-unwind" fn atomic_copy_buffer_uint64(
    _this: *mut c_void,
    _dst_buffer: *mut c_void,
    _dst_offset: u64,
    _src_buffer: *mut c_void,
    _src_offset: u64,
    _dependencies: u32,
    _dependent_resources: *const *mut c_void,
    _dependent_subresource_ranges: *const c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::AtomicCopyBufferUINT64");
}

/// `ID3D12GraphicsCommandList1::ResolveSubresourceRegion` (`d3d12_command_list.cpp:2128`).
///
/// # Safety
///
/// `this`, `dst_resource`, `src_resource`, and `src_rect` are caller-supplied pointers.
pub unsafe extern "system-unwind" fn resolve_subresource_region(
    _this: *mut c_void,
    _dst_resource: *mut c_void,
    _dst_subresource: u32,
    _dst_x: u32,
    _dst_y: u32,
    _src_resource: *mut c_void,
    _src_subresource: u32,
    _src_rect: *mut c_void,
    _format: u32,
    _resolve_mode: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ResolveSubresourceRegion");
}

/// `ID3D12GraphicsCommandList2::WriteBufferImmediate` (`d3d12_command_list.cpp:2275`).
///
/// # Safety
///
/// `this`, `params`, and `modes` are caller-supplied pointers.
pub unsafe fn write_buffer_immediate(
    _this: *mut c_void,
    _count: u32,
    _params: *const c_void,
    _modes: *const u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::WriteBufferImmediate");
}
