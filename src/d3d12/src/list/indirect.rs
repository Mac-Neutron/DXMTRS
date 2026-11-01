//! `ExecuteIndirect` (`d3d12_command_list.cpp:2028`): fast direct path and ICB resolver path.

use core::ffi::c_void;

/// `ID3D12GraphicsCommandList::ExecuteIndirect` (`d3d12_command_list.cpp:2028`).
///
/// # Safety
///
/// `this`, `command_signature`, `argument_buffer`, and `count_buffer` are caller-supplied pointers.
pub unsafe fn execute_indirect(
    _this: *mut c_void,
    _command_signature: *mut c_void,
    _max_command_count: u32,
    _argument_buffer: *mut c_void,
    _argument_buffer_offset: u64,
    _count_buffer: *mut c_void,
    _count_buffer_offset: u64,
) {
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::ExecuteIndirect");
}
