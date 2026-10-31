//! What `nvapi64.dll` imports: winemetal's Metal and display calls, and user32's monitors.
//!
//! winemetal's are winemetal-sys's declarations; meson links winemetal and user32. Declared
//! without `cfg(windows)`: a host test binary still links, because the linker strips what no
//! test reaches. A function that calls one of these is out of the host tests' reach.

use core::ffi::c_void;

/// A Metal object, as winemetal hands it across (`obj_handle_t`).
pub type ObjHandle = u64;

/// `WMTColorSpace`, a 64-bit enum: sRGB, HDR PQ and HDR scRGB.
pub const COLOR_SPACE_SRGB: u64 = 0;
pub const COLOR_SPACE_HDR_PQ: u64 = 0b100;
pub const COLOR_SPACE_HDR_SCRGB: u64 = 0b101;

pub use winemetal_sys::{
    MTLDevice_registryID, NSArray_count, NSArray_object, NSObject_release, WMTCopyAllDevices,
    WMTGetDisplayDescription, WMTGetPrimaryDisplayId, WMTGetSecondaryDisplayId,
    WMTQueryDisplaySetting, WMTUpdateDisplaySetting,
    wire::{WMTDisplayDescription as DisplayDescription, WMTHDRMetadata as HdrMetadata},
};

/// `MONITORINFOEXW`: `MONITORINFO` with the device name.
#[repr(C)]
pub struct MonitorInfoExW {
    pub size: u32,
    pub monitor: [i32; 4],
    pub work: [i32; 4],
    pub flags: u32,
    pub device: [u16; 32],
}
const _: () = assert!(size_of::<MonitorInfoExW>() == 104);

/// `MONITORINFOF_PRIMARY`.
pub const MONITORINFOF_PRIMARY: u32 = 1;

/// `MONITORENUMPROC`.
pub type MonitorEnumProc =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *mut [i32; 4], isize) -> i32;

unsafe extern "system" {
    pub fn EnumDisplayMonitors(
        hdc: *mut c_void,
        clip: *const [i32; 4],
        callback: MonitorEnumProc,
        data: isize,
    ) -> i32;
    pub fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfoExW) -> i32;
}
