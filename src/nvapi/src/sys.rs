//! NVAPI's system information, driver versions and error messages.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::nv::{
    API_NOT_INITIALIZED, DISPLAY_DRIVER_VERSION_VER, DisplayDriverVersion,
    INCOMPATIBLE_STRUCT_VERSION, INVALID_ARGUMENT, MOSAIC_NOT_ACTIVE, NOT_SUPPORTED,
    NVAPI_SDK_VERSION, NVIDIA_DEVICE_NOT_FOUND, OK, SHORT_STRING_MAX, STEREO_NOT_INITIALIZED,
    Status,
};

/// How many `NvAPI_Initialize` calls `NvAPI_Unload` has yet to match.
///
/// Process-wide resource: the API passes no object to hold it, and it is the C++'s
/// `s_initialization_count`.
static INIT_COUNT: AtomicU32 = AtomicU32::new(0);

/// The driver-branch string the version calls answer with.
fn build_branch() -> String {
    format!("r{NVAPI_SDK_VERSION}_000")
}

/// Copies `s` and a NUL into the start of `dst`.
///
/// As the C++'s `memcpy(dst, s, len + 1)`: the bytes after the NUL stay as they were.
pub fn short_string(dst: &mut [u8], s: &str) {
    dst[..s.len()].copy_from_slice(s.as_bytes());
    dst[s.len()] = 0;
}

/// Whether DXMT's NVIDIA extension is on (never, off Windows).
#[cfg(windows)]
fn nvext_enabled() -> bool {
    dxmt_util::nvext::enabled()
}

/// Whether DXMT's NVIDIA extension is on (never, off Windows).
#[cfg(not(windows))]
const fn nvext_enabled() -> bool {
    false
}

/// `NvAPI_Initialize`: counts the call, if DXMT's NVIDIA extension is on.
extern "C" fn initialize() -> Status {
    if !nvext_enabled() {
        return NVIDIA_DEVICE_NOT_FOUND;
    }
    INIT_COUNT.fetch_add(1, Ordering::SeqCst);
    OK
}

/// `NvAPI_Unload`: matches an `Initialize`.
extern "C" fn unload() -> Status {
    let mut n = INIT_COUNT.load(Ordering::SeqCst);
    while n != 0 {
        match INIT_COUNT.compare_exchange(n, n - 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return OK,
            Err(now) => n = now,
        }
    }
    API_NOT_INITIALIZED
}

/// `NvAPI_SYS_GetDriverAndBranchVersion`.
extern "C" fn get_driver_and_branch_version(version: *mut u32, branch: *mut u8) -> Status {
    if version.is_null() || branch.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: the caller passes an `NvAPI_ShortString`, 64 bytes.
    let branch = unsafe { core::slice::from_raw_parts_mut(branch, SHORT_STRING_MAX) };
    short_string(branch, &build_branch());
    // SAFETY: `version` is non-null and the caller's `NvU32`.
    unsafe { version.write(99999) };
    OK
}

/// `NvAPI_GetDisplayDriverVersion`.
extern "C" fn get_display_driver_version(
    _display: *mut c_void,
    version: *mut DisplayDriverVersion,
) -> Status {
    if version.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: `version` is non-null and the caller's `NV_DISPLAY_DRIVER_VERSION`.
    if unsafe { (*version).version } != DISPLAY_DRIVER_VERSION_VER {
        return INCOMPATIBLE_STRUCT_VERSION;
    }
    // SAFETY: as above, and the version check says the struct has all its fields.
    let version = unsafe { &mut *version };
    version.drv_version = 99999;
    version.bld_change_list_num = 0;
    short_string(&mut version.build_branch_string, &build_branch());
    short_string(&mut version.adapter_string, "NVIDIA GeForce RTX");
    OK
}

/// `NvAPI_GetInterfaceVersionString`.
extern "C" fn get_interface_version_string(desc: *mut u8) -> Status {
    if desc.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: the caller passes an `NvAPI_ShortString`, 64 bytes.
    let desc = unsafe { core::slice::from_raw_parts_mut(desc, SHORT_STRING_MAX) };
    short_string(desc, "NVAPI Open Source Interface (DXMT-NVAPI)");
    OK
}

/// `NvAPI_GetErrorMessage`: an empty message for every status.
const extern "C" fn get_error_message(_status: Status, desc: *mut u8) -> Status {
    if desc.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: `desc` is non-null and the caller's `NvAPI_ShortString`.
    unsafe { desc.write(0) };
    OK
}

/// `NvAPI_Mosaic_GetDisplayViewportsByResolution`.
const extern "C" fn mosaic_get_display_viewports_by_resolution(
    _display_id: u32,
    _width: u32,
    _height: u32,
    _viewports: *mut c_void,
    _bezel_corrected: *mut u8,
) -> Status {
    MOSAIC_NOT_ACTIVE
}

/// `NvAPI_Stereo_IsEnabled`.
const extern "C" fn stereo_is_enabled(enabled: *mut u8) -> Status {
    if enabled.is_null() {
        return INVALID_ARGUMENT;
    }
    STEREO_NOT_INITIALIZED
}

/// `NvAPI_Stereo_SetDriverMode`.
const extern "C" fn stereo_set_driver_mode(_mode: u32) -> Status {
    STEREO_NOT_INITIALIZED
}

/// `NvAPI_DRS_CreateSession`.
const extern "C" fn drs_create_session(session: *mut c_void) -> Status {
    if session.is_null() {
        return INVALID_ARGUMENT;
    }
    NOT_SUPPORTED
}

/// `NvAPI_DRS_LoadSettings`.
const extern "C" fn drs_load_settings(_session: *mut c_void) -> Status {
    NOT_SUPPORTED
}

/// The function `id` names, or null if this module has none.
#[must_use]
pub fn query(id: u32) -> *mut c_void {
    match id {
        0x0150_e828 => initialize as *mut c_void,
        0xd22b_dd7e => unload as *mut c_void,
        0x2926_aaad => get_driver_and_branch_version as *mut c_void,
        0xf951_a4d1 => get_display_driver_version as *mut c_void,
        0x0105_3fa5 => get_interface_version_string as *mut c_void,
        0x6c2d_048c => get_error_message as *mut c_void,
        0xdc6d_c8d3 => mosaic_get_display_viewports_by_resolution as *mut c_void,
        0x348f_f8e1 => stereo_is_enabled as *mut c_void,
        0x5e8f_0bec => stereo_set_driver_mode as *mut c_void,
        0x0694_d52e => drs_create_session as *mut c_void,
        0x375d_bd6b => drs_load_settings as *mut c_void,
        _ => core::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests;
