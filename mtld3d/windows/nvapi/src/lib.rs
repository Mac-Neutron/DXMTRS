//! DXMT's `nvapi64.dll`: NVIDIA's NVAPI, answered for Metal GPUs.

use core::ffi::c_void;

/// NVAPI's one export: the address of the function an ID names, or null.
#[unsafe(no_mangle)]
pub extern "C" fn nvapi_QueryInterface(_id: u32) -> *mut c_void {
    core::ptr::null_mut()
}
