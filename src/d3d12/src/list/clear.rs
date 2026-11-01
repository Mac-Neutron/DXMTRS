//! RTV/DSV/UAV clears, `DiscardResource`, and `BeginRenderPass`/`EndRenderPass`
//!
//! (`d3d12_command_list.cpp`).

use core::ffi::c_void;

use crate::abi::vtbl::{D3d12CpuDescriptorHandle, D3d12GpuDescriptorHandle};

/// `ID3D12GraphicsCommandList::ClearDepthStencilView` (`d3d12_command_list.cpp:1732`).
///
/// # Safety
///
/// `this` and `rects` are caller-supplied pointers.
pub unsafe fn clear_depth_stencil_view(
    _this: *mut c_void,
    _depth_stencil_view: D3d12CpuDescriptorHandle,
    _clear_flags: u32,
    _depth: f32,
    _stencil: u8,
    _num_rects: u32,
    _rects: *const c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ClearDepthStencilView");
}

/// `ID3D12GraphicsCommandList::ClearRenderTargetView` (`d3d12_command_list.cpp:1759`).
///
/// # Safety
///
/// `this`, `color_rgba`, and `rects` are caller-supplied pointers.
pub unsafe fn clear_render_target_view(
    _this: *mut c_void,
    _render_target_view: D3d12CpuDescriptorHandle,
    _color_rgba: *const f32,
    _num_rects: u32,
    _rects: *const c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ClearRenderTargetView");
}

/// `ID3D12GraphicsCommandList::ClearUnorderedAccessViewUint` (`d3d12_command_list.cpp:1783`).
///
/// # Safety
///
/// `this`, `resource`, `values`, and `rects` are caller-supplied pointers.
pub unsafe fn clear_unordered_access_view_uint(
    _this: *mut c_void,
    _view_gpu_handle_in_current_heap: D3d12GpuDescriptorHandle,
    _view_cpu_handle: D3d12CpuDescriptorHandle,
    _resource: *mut c_void,
    _values: *const u32,
    _num_rects: u32,
    _rects: *const c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ClearUnorderedAccessViewUint");
}

/// `ID3D12GraphicsCommandList::ClearUnorderedAccessViewFloat` (`d3d12_command_list.cpp:1815`).
///
/// # Safety
///
/// `this`, `resource`, `values`, and `rects` are caller-supplied pointers.
pub unsafe fn clear_unordered_access_view_float(
    _this: *mut c_void,
    _view_gpu_handle_in_current_heap: D3d12GpuDescriptorHandle,
    _view_cpu_handle: D3d12CpuDescriptorHandle,
    _resource: *mut c_void,
    _values: *const f32,
    _num_rects: u32,
    _rects: *const c_void,
) {
    let (): () =
        crate::not_implemented!("ID3D12GraphicsCommandList::ClearUnorderedAccessViewFloat");
}

/// `ID3D12GraphicsCommandList::DiscardResource` (`d3d12_command_list.cpp:1842`).
///
/// # Safety
///
/// `this`, `resource`, and `region` are caller-supplied pointers.
pub unsafe fn discard_resource(_this: *mut c_void, _resource: *mut c_void, _region: *const c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::DiscardResource");
}

/// `ID3D12GraphicsCommandList4::BeginRenderPass` (`d3d12_command_list.cpp:2291`).
///
/// # Safety
///
/// `this`, `render_targets`, and `depth_stencil` are caller-supplied pointers.
pub unsafe fn begin_render_pass(
    _this: *mut c_void,
    _num_render_targets: u32,
    _render_targets: *const c_void,
    _depth_stencil: *const c_void,
    _flags: u32,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::BeginRenderPass");
}

/// `ID3D12GraphicsCommandList4::EndRenderPass` (`d3d12_command_list.cpp:2399`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn end_render_pass(_this: *mut c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::EndRenderPass");
}
