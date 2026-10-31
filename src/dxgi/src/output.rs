//! The output, `IMTLDXGIOutput`; a stub until the port.

use core::ffi::c_void;

use crate::abi::Hmonitor;

/// A new output with one reference on `monitor`; it takes a public reference to `adapter`.
///
/// `force_sdr` is the adapter's `dxgi.forceSDR`.
///
/// # Safety
///
/// `adapter` is a live adapter of this crate.
#[must_use]
pub const unsafe fn create(
    _adapter: *mut c_void,
    _monitor: Hmonitor,
    _force_sdr: bool,
) -> *mut c_void {
    core::ptr::null_mut()
}
