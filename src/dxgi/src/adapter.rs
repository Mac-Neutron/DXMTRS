//! The adapter, `IMTLDXGIAdapter`; a stub until the port.

use core::ffi::c_void;

/// A new adapter with one reference, on the Metal `device`.
///
/// It takes a reference to `device` and a public one to `factory`.
///
/// # Safety
///
/// `factory` is a live factory of this crate.
#[must_use]
pub const unsafe fn create(_device: u64, _factory: *mut c_void) -> *mut c_void {
    core::ptr::null_mut()
}
