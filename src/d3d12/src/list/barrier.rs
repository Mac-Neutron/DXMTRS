//! `ResourceBarrier` and enhanced `Barrier` classification (`d3d12_command_list.cpp:1330, 2483`).

use core::ffi::c_void;

/// `ID3D12GraphicsCommandList::ResourceBarrier` (`d3d12_command_list.cpp:1330`).
///
/// # Safety
///
/// `this` and `barriers` are caller-supplied pointers.
pub unsafe fn resource_barrier(_this: *mut c_void, _num_barriers: u32, _barriers: *const c_void) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ResourceBarrier");
}

/// `ID3D12GraphicsCommandList7::Barrier` (`d3d12_command_list.cpp:2483`).
///
/// # Safety
///
/// `this` and `barrier_groups` are caller-supplied pointers.
pub unsafe fn barrier(
    _this: *mut c_void,
    _num_barrier_groups: u32,
    _barrier_groups: *const c_void,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::Barrier");
}
