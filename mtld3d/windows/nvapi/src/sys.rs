//! NVAPI's system information, driver versions and error messages.

use core::ffi::c_void;

/// The function `id` names, or null if this module has none.
#[must_use]
pub const fn query(_id: u32) -> *mut c_void {
    core::ptr::null_mut()
}
