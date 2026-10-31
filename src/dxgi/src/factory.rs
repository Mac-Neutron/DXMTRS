//! The factory, `IDXGIFactory6`; a stub until the port.

use core::ffi::c_void;

/// A new factory with one reference, created with `flags` (`CreateDXGIFactory2`'s).
#[must_use]
pub const fn create(_flags: u32) -> *mut c_void {
    core::ptr::null_mut()
}
