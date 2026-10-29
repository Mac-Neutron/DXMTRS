//! DXMT's NVIDIA extension, which DXGI answers only when `DXMT_ENABLE_NVEXT` is set.

#[cfg(windows)]
/// The GUID of DXMT's NVIDIA extension.
static DXMT_NVEXT_GUID: [u8; 16] = [
    0x16, 0xf6, 0x0a, 0xba, 0x43, 0x4a, 0x59, 0x42, 0x81, 0x5c, 0xdb, 0x3b, 0x89, 0x82, 0x99, 0x05,
];

#[cfg(windows)]
unsafe extern "system" {
    fn DXGIGetDebugInterface1(flags: u32, riid: *const [u8; 16], debug: *mut *mut core::ffi::c_void) -> i32;
}

#[cfg(windows)]
/// Whether DXMT's NVIDIA extension is on.
#[must_use]
pub fn enabled() -> bool {
    // SAFETY: a null output pointer asks DXGI only whether the interface exists; the GUID outlives the call.
    unsafe { DXGIGetDebugInterface1(0, &raw const DXMT_NVEXT_GUID, core::ptr::null_mut()) >= 0 }
}
