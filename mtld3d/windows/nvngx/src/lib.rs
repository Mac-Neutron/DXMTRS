//! DXMT's `nvngx.dll`: DLSS super resolution answered by `MetalFX`, for Direct3D 11 and 12.

mod com;
mod d3d11;
mod d3d12;
mod entry;
mod ngx;
mod parameter;

use dxmt_util::log::Logger;

/// This DLL's log file.
///
/// Process-wide resource: one log file per DLL, opened by the first message that passes the level, and
/// the exports have no object to hold it. It is DXMT's `Logger::s_instance`.
static LOG: Logger = Logger::new("nvngx.log");

#[cfg(windows)]
unsafe extern "system" {
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
    i32::from(dxmt_util::nvext::enabled())
}
