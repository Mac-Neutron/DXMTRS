//! What `nvapi64.dll` imports: winemetal's Metal and display calls, and user32's monitors.
//!
//! meson links winemetal and user32. Declared without `cfg(windows)`: a host test binary
//! still links, because the linker strips what no test reaches. A function that calls one of
//! these is out of the host tests' reach.

use core::ffi::c_void;

/// A Metal object, as winemetal hands it across (`obj_handle_t`).
pub type ObjHandle = u64;

/// `WMTColorSpace`, a 64-bit enum: sRGB, HDR PQ and HDR scRGB.
pub const COLOR_SPACE_SRGB: u64 = 0;
pub const COLOR_SPACE_HDR_PQ: u64 = 0b100;
pub const COLOR_SPACE_HDR_SCRGB: u64 = 0b101;

/// `WMTDisplayDescription`.
#[repr(C)]
pub struct DisplayDescription {
    pub red_primaries: [f32; 2],
    pub blue_primaries: [f32; 2],
    pub green_primaries: [f32; 2],
    pub white_points: [f32; 2],
    pub maximum_edr_color_component_value: f32,
    pub maximum_potential_edr_color_component_value: f32,
    pub maximum_reference_edr_color_component_value: f32,
}
const _: () = assert!(size_of::<DisplayDescription>() == 44);

/// `WMTHDRMetadata`.
#[repr(C)]
pub struct HdrMetadata {
    pub red_primary: [u16; 2],
    pub green_primary: [u16; 2],
    pub blue_primary: [u16; 2],
    pub white_point: [u16; 2],
    pub max_mastering_luminance: u32,
    pub min_mastering_luminance: u32,
    pub max_content_light_level: u16,
    pub max_frame_average_light_level: u16,
}
const _: () = assert!(size_of::<HdrMetadata>() == 28);

unsafe extern "C" {
    pub fn WMTCopyAllDevices() -> ObjHandle;
    pub fn NSArray_count(array: ObjHandle) -> u64;
    pub fn NSArray_object(array: ObjHandle, index: u64) -> ObjHandle;
    pub fn MTLDevice_registryID(device: ObjHandle) -> u64;
    pub fn NSObject_release(object: ObjHandle);
    pub fn WMTGetPrimaryDisplayId() -> u32;
    pub fn WMTGetSecondaryDisplayId() -> u32;
    pub fn WMTGetDisplayDescription(display_id: u32, desc: *mut DisplayDescription);
    pub fn WMTQueryDisplaySetting(
        display_id: u32,
        colorspace: *mut u64,
        metadata: *mut HdrMetadata,
    ) -> bool;
    pub fn WMTUpdateDisplaySetting(display_id: u32, colorspace: u64, metadata: *const HdrMetadata);
}

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
