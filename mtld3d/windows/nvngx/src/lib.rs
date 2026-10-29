//! DXMT's `nvngx.dll`: DLSS super resolution answered by `MetalFX`, for Direct3D 11 and 12.

mod com;
mod d3d11;
mod d3d12;
mod entry;
mod float_cast;
mod log;
mod ngx;
mod parameter;

#[cfg(windows)]
/// The GUID of DXMT's NVIDIA extension, which DXGI answers only when `DXMT_ENABLE_NVEXT` is set.
static DXMT_NVEXT_GUID: [u8; 16] = [
    0x16, 0xf6, 0x0a, 0xba, 0x43, 0x4a, 0x59, 0x42, 0x81, 0x5c, 0xdb, 0x3b, 0x89, 0x82, 0x99, 0x05,
];

#[cfg(windows)]
unsafe extern "system" {
    fn DXGIGetDebugInterface1(flags: u32, riid: *const [u8; 16], debug: *mut *mut core::ffi::c_void) -> i32;
    fn DisableThreadLibraryCalls(module: *mut core::ffi::c_void) -> i32;
}

#[cfg(windows)]
/// Refuses to load unless DXMT's NVIDIA extension is on.
///
/// Games without DLSS enabled must not see NGX.
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(module: *mut core::ffi::c_void, reason: u32, _reserved: *mut core::ffi::c_void) -> i32 {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason != DLL_PROCESS_ATTACH {
        return 1;
    }
    // SAFETY: `module` is this DLL's handle, which the loader passes to DllMain.
    unsafe { DisableThreadLibraryCalls(module) };
    // SAFETY: a null output pointer asks DXGI only whether the interface exists; the GUID outlives the call.
    i32::from(unsafe { DXGIGetDebugInterface1(0, &raw const DXMT_NVEXT_GUID, core::ptr::null_mut()) } >= 0)
}
