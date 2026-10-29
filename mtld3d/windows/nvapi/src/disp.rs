//! NVAPI's displays, their IDs and HDR.

use crate::ffi::{
    COLOR_SPACE_HDR_PQ, COLOR_SPACE_HDR_SCRGB, COLOR_SPACE_SRGB, DisplayDescription, EnumDisplayMonitors,
    GetMonitorInfoW, HdrMetadata, MONITORINFOF_PRIMARY, MonitorInfoExW, WMTGetDisplayDescription, WMTGetPrimaryDisplayId,
    WMTGetSecondaryDisplayId, WMTQueryDisplaySetting, WMTUpdateDisplaySetting,
};
use crate::nv::{
    BPC_10, COLOR_FORMAT_RGB, DISPLAYIDS_ACTIVE, DISPLAYIDS_CONNECTED, DISPLAYIDS_PHYSICALLY_CONNECTED,
    DYNAMIC_RANGE_VESA, GPU_DISPLAYIDS_VER1, GPU_DISPLAYIDS_VER2, GpuDisplayIds, HDR_CAP_ST2084_EOTF, HDR_CAPABILITIES_VER1,
    HDR_CAPABILITIES_VER2, HDR_CAPABILITIES_VER3, HDR_CMD_GET, HDR_COLOR_DATA_VER1, HDR_COLOR_DATA_VER2, HDR_MODE_OFF,
    HDR_MODE_UHDA, HDR_MODE_UHDA_PASSTHROUGH, HdrCapabilitiesV1, HdrCapabilitiesV2, HdrCapabilitiesV3, HdrColorDataV1,
    HdrColorDataV2, HdrMasteringData, INCOMPATIBLE_STRUCT_VERSION, INSUFFICIENT_BUFFER, INVALID_ARGUMENT,
    END_ENUMERATION, MONITOR_CONN_TYPE_UNKNOWN, NOT_SUPPORTED, NVIDIA_DEVICE_NOT_FOUND, OK, Status,
};
use crate::gpu::{id_handle, registry_ids};
use core::ffi::{CStr, c_char, c_void};
use dxmt_util::float_cast::f32_to_u16;

/// A display's primaries and luminance as NVAPI scales them, from a `WMTDisplayDescription`.
pub struct Scaled {
    /// Red, green and blue x and y, then the white point's, in units of 0.00002.
    pub primary: [u16; 8],
    /// The largest EDR value, times 100.
    pub max_luminance: u16,
}

/// A `WMTDisplayDescription` scaled for NVAPI: chromaticities by 50000, luminance by 100.
#[must_use]
pub fn primaries(desc: &DisplayDescription) -> Scaled {
    let scale = |v: f32| f32_to_u16(v * 50000.0);
    Scaled {
        primary: [
            scale(desc.red_primaries[0]),
            scale(desc.red_primaries[1]),
            scale(desc.green_primaries[0]),
            scale(desc.green_primaries[1]),
            scale(desc.blue_primaries[0]),
            scale(desc.blue_primaries[1]),
            scale(desc.white_points[0]),
            scale(desc.white_points[1]),
        ],
        max_luminance: f32_to_u16(desc.maximum_potential_edr_color_component_value * 100.0),
    }
}

/// How many displays there are, from the primary's and the secondary's IDs (0 for none).
#[must_use]
pub const fn display_count(primary: u32, secondary: u32) -> u32 {
    if primary == 0 {
        0
    } else if secondary == 0 {
        1
    } else {
        2
    }
}

/// The HDR mode a color space is: scRGB is UHDA, PQ is UHDA passthrough, anything else is off.
#[must_use]
pub const fn hdr_mode(colorspace: u64) -> u32 {
    if colorspace == COLOR_SPACE_HDR_SCRGB {
        HDR_MODE_UHDA
    } else if colorspace == COLOR_SPACE_HDR_PQ {
        HDR_MODE_UHDA_PASSTHROUGH
    } else {
        HDR_MODE_OFF
    }
}

/// C++'s narrowing of a 32-bit value to 16 bits.
fn low16(v: u32) -> u16 {
    u16::try_from(v & 0xffff).unwrap_or(0)
}

/// The mastering data of a display's current metadata.
fn mastering_of(m: &HdrMetadata) -> HdrMasteringData {
    HdrMasteringData {
        primary_x0: m.red_primary[0],
        primary_y0: m.red_primary[1],
        primary_x1: m.green_primary[0],
        primary_y1: m.green_primary[1],
        primary_x2: m.blue_primary[0],
        primary_y2: m.blue_primary[1],
        white_point_x: m.white_point[0],
        white_point_y: m.white_point[1],
        max_display_mastering_luminance: low16(m.max_mastering_luminance),
        min_display_mastering_luminance: low16(m.min_mastering_luminance),
        max_content_light_level: m.max_content_light_level,
        max_frame_average_light_level: m.max_frame_average_light_level,
    }
}

/// The mastering data of a display with no HDR setting: its description, with minimum luminance 1.
const fn mastering_of_description(s: &Scaled) -> HdrMasteringData {
    HdrMasteringData {
        primary_x0: s.primary[0],
        primary_y0: s.primary[1],
        primary_x1: s.primary[2],
        primary_y1: s.primary[3],
        primary_x2: s.primary[4],
        primary_y2: s.primary[5],
        white_point_x: s.primary[6],
        white_point_y: s.primary[7],
        max_display_mastering_luminance: s.max_luminance,
        min_display_mastering_luminance: 1,
        max_content_light_level: s.max_luminance,
        max_frame_average_light_level: s.max_luminance,
    }
}

/// The metadata a game's mastering data sets.
fn metadata_of(d: &HdrMasteringData) -> HdrMetadata {
    HdrMetadata {
        red_primary: [d.primary_x0, d.primary_y0],
        green_primary: [d.primary_x1, d.primary_y1],
        blue_primary: [d.primary_x2, d.primary_y2],
        white_point: [d.white_point_x, d.white_point_y],
        max_mastering_luminance: u32::from(d.max_display_mastering_luminance),
        min_mastering_luminance: u32::from(d.min_display_mastering_luminance),
        max_content_light_level: d.max_content_light_level,
        max_frame_average_light_level: d.max_frame_average_light_level,
    }
}

/// Whether a monitor's device name (UTF-16, NUL-terminated) is the name a game asked for.
#[must_use]
pub fn device_name_is(name: &[u8], device: &[u16]) -> bool {
    let end = device.iter().position(|&c| c == 0).unwrap_or(device.len());
    String::from_utf16_lossy(&device[..end]).as_bytes() == name
}

/// A description with nothing in it, for the call to fill.
const fn empty_description() -> DisplayDescription {
    DisplayDescription {
        red_primaries: [0.0; 2],
        blue_primaries: [0.0; 2],
        green_primaries: [0.0; 2],
        white_points: [0.0; 2],
        maximum_edr_color_component_value: 0.0,
        maximum_potential_edr_color_component_value: 0.0,
        maximum_reference_edr_color_component_value: 0.0,
    }
}

/// The scaled description of display `id`.
fn scaled_description(id: u32) -> (Scaled, f32) {
    let mut desc = empty_description();
    // SAFETY: winemetal fills the local description.
    unsafe { WMTGetDisplayDescription(id, &raw mut desc) };
    (primaries(&desc), desc.maximum_potential_edr_color_component_value)
}

/// The primary display's ID, 0 for none.
fn primary_display_id() -> u32 {
    // SAFETY: winemetal's display ID takes no arguments.
    unsafe { WMTGetPrimaryDisplayId() }
}

/// The secondary display's ID, 0 for none.
fn secondary_display_id() -> u32 {
    // SAFETY: winemetal's display ID takes no arguments.
    unsafe { WMTGetSecondaryDisplayId() }
}

extern "C" fn get_physical_gpus_from_display(display: *mut c_void, gpus: *mut *mut c_void, count: *mut u32) -> Status {
    if display.is_null() || gpus.is_null() || count.is_null() {
        return INVALID_ARGUMENT;
    }
    let ids = registry_ids();
    if ids.is_empty() {
        return NVIDIA_DEVICE_NOT_FOUND;
    }
    for (i, id) in ids.iter().enumerate() {
        // SAFETY: `registry_ids` returns at most `MAX_GPUS` IDs, and the game's array holds
        // NVAPI_MAX_PHYSICAL_GPUS handles.
        unsafe { gpus.wrapping_add(i).write(id_handle(*id)) };
    }
    // SAFETY: the game's out pointer, checked non-null.
    unsafe { count.write(u32::try_from(ids.len()).unwrap_or(0)) };
    OK
}

/// What `EnumDisplayMonitors` finds for `DISP_GetDisplayIdByDisplayName`.
struct NameSearch<'a> {
    name: &'a [u8],
    monitor: *mut c_void,
    is_primary: bool,
}

unsafe extern "system" fn name_proc(monitor: *mut c_void, _dc: *mut c_void, _rect: *mut [i32; 4], data: isize) -> i32 {
    // SAFETY: `data` is the `NameSearch` the caller passed, alive for the enumeration.
    let Some(search) = (unsafe { core::ptr::with_exposed_provenance_mut::<NameSearch>(usize::try_from(data).unwrap_or(0)).as_mut() })
    else {
        return 0;
    };
    let mut info = MonitorInfoExW { size: 104, monitor: [0; 4], work: [0; 4], flags: 0, device: [0; 32] };
    // SAFETY: `info` is a local `MONITORINFOEXW` with its size set.
    if unsafe { GetMonitorInfoW(monitor, &raw mut info) } == 0 || !device_name_is(search.name, &info.device) {
        return 1;
    }
    search.monitor = monitor;
    search.is_primary = info.flags & MONITORINFOF_PRIMARY != 0;
    0
}

extern "C" fn get_display_id_by_display_name(name: *const c_char, id: *mut u32) -> Status {
    if name.is_null() || id.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: the game's NUL-terminated string.
    let name = unsafe { CStr::from_ptr(name) };
    let mut search = NameSearch { name: name.to_bytes(), monitor: core::ptr::null_mut(), is_primary: false };
    let data = (&raw mut search).expose_provenance();
    // SAFETY: `name_proc` reads the `NameSearch` through `data` while `search` lives.
    unsafe { EnumDisplayMonitors(core::ptr::null_mut(), core::ptr::null(), name_proc, isize::try_from(data).unwrap_or(0)) };
    if search.monitor.is_null() {
        return NVIDIA_DEVICE_NOT_FOUND;
    }
    let display_id = if search.is_primary { primary_display_id() } else { secondary_display_id() };
    // SAFETY: the game's out pointer, checked non-null.
    unsafe { id.write(display_id) };
    OK
}

extern "C" fn get_hdr_capabilities(display_id: u32, caps: *mut HdrCapabilitiesV3) -> Status {
    if caps.is_null() {
        return INVALID_ARGUMENT;
    }
    let (scaled, max_potential) = scaled_description(display_id);
    // SAFETY: every version of the struct starts with its version word.
    let version = unsafe { caps.cast::<u32>().read() };
    let size = match version {
        HDR_CAPABILITIES_VER1 => size_of::<HdrCapabilitiesV1>(),
        HDR_CAPABILITIES_VER2 => size_of::<HdrCapabilitiesV2>(),
        HDR_CAPABILITIES_VER3 => size_of::<HdrCapabilitiesV3>(),
        _ => return INCOMPATIBLE_STRUCT_VERSION,
    };
    // SAFETY: the game allocated the size its version says.
    unsafe { caps.cast::<u8>().write_bytes(0, size) };
    // SAFETY: V2 and V3 start with V1, and the struct holds at least V1.
    let Some(out) = (unsafe { caps.cast::<HdrCapabilitiesV1>().as_mut() }) else { return INVALID_ARGUMENT };
    out.version = version;
    out.flags = u32::from(max_potential > 1.0) * HDR_CAP_ST2084_EOTF;
    let data = &mut out.display_data;
    [
        data.primary_x0,
        data.primary_y0,
        data.primary_x1,
        data.primary_y1,
        data.primary_x2,
        data.primary_y2,
        data.white_point_x,
        data.white_point_y,
    ] = scaled.primary;
    data.desired_content_max_luminance = scaled.max_luminance;
    data.desired_content_min_luminance = 1;
    data.desired_content_max_frame_average_luminance = scaled.max_luminance;
    OK
}

extern "C" fn hdr_color_control(display_id: u32, data: *mut HdrColorDataV2) -> Status {
    if data.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: every version of the struct starts with its version word.
    let version = unsafe { data.cast::<u32>().read() };
    if version != HDR_COLOR_DATA_VER1 && version != HDR_COLOR_DATA_VER2 {
        return INCOMPATIBLE_STRUCT_VERSION;
    }
    // SAFETY: V2 starts with V1, and the struct holds at least V1.
    let Some(v1) = (unsafe { data.cast::<HdrColorDataV1>().as_mut() }) else { return INVALID_ARGUMENT };
    let mut metadata = metadata_of(&v1.mastering_display_data);
    let mut colorspace = COLOR_SPACE_SRGB;
    if v1.cmd == HDR_CMD_GET {
        // SAFETY: winemetal fills the two locals.
        if unsafe { WMTQueryDisplaySetting(display_id, &raw mut colorspace, &raw mut metadata) } {
            v1.hdr_mode = hdr_mode(colorspace);
            v1.mastering_display_data = mastering_of(&metadata);
        } else {
            v1.hdr_mode = HDR_MODE_OFF;
            v1.mastering_display_data = mastering_of_description(&scaled_description(display_id).0);
        }
        if version == HDR_COLOR_DATA_VER2 {
            // SAFETY: the game's struct is a V2.
            let Some(v2) = (unsafe { data.as_mut() }) else { return INVALID_ARGUMENT };
            v2.hdr_color_format = COLOR_FORMAT_RGB;
            v2.hdr_dynamic_range = DYNAMIC_RANGE_VESA;
            v2.hdr_bpc = BPC_10;
        }
    } else if v1.hdr_mode == HDR_MODE_OFF {
        // SAFETY: a null metadata is the call's "off".
        unsafe { WMTUpdateDisplaySetting(display_id, COLOR_SPACE_SRGB, core::ptr::null()) };
    } else {
        let space = if v1.hdr_mode == HDR_MODE_UHDA { COLOR_SPACE_HDR_SCRGB } else { COLOR_SPACE_HDR_PQ };
        // SAFETY: winemetal reads the local metadata.
        unsafe { WMTUpdateDisplaySetting(display_id, space, &raw const metadata) };
    }
    OK
}

const extern "C" fn get_monitor_capabilities(_display_id: u32, caps: *mut c_void) -> Status {
    if caps.is_null() { INVALID_ARGUMENT } else { NOT_SUPPORTED }
}

/// What `EnumDisplayMonitors` finds for `EnumNvidiaDisplayHandle`: the monitor at `index`.
struct IndexSearch {
    index: u32,
    monitor: *mut c_void,
}

unsafe extern "system" fn index_proc(monitor: *mut c_void, _dc: *mut c_void, _rect: *mut [i32; 4], data: isize) -> i32 {
    // SAFETY: `data` is the `IndexSearch` the caller passed, alive for the enumeration.
    let Some(search) = (unsafe { core::ptr::with_exposed_provenance_mut::<IndexSearch>(usize::try_from(data).unwrap_or(0)).as_mut() })
    else {
        return 0;
    };
    if search.index > 0 {
        search.index -= 1;
        return 1;
    }
    search.monitor = monitor;
    0
}

extern "C" fn enum_nvidia_display_handle(index: u32, handle: *mut *mut c_void) -> Status {
    if handle.is_null() {
        return INVALID_ARGUMENT;
    }
    let mut search = IndexSearch { index, monitor: core::ptr::null_mut() };
    let data = (&raw mut search).expose_provenance();
    // SAFETY: `index_proc` reads the `IndexSearch` through `data` while `search` lives.
    unsafe { EnumDisplayMonitors(core::ptr::null_mut(), core::ptr::null(), index_proc, isize::try_from(data).unwrap_or(0)) };
    if search.monitor.is_null() {
        return END_ENUMERATION;
    }
    // SAFETY: the game's out pointer, checked non-null.
    unsafe { handle.write(search.monitor) };
    OK
}

extern "C" fn get_connected_display_ids(_gpu: *mut c_void, ids: *mut GpuDisplayIds, count: *mut u32, _flags: u32) -> Status {
    if count.is_null() {
        return INVALID_ARGUMENT;
    }
    let (primary, secondary) = (primary_display_id(), secondary_display_id());
    let wanted = display_count(primary, secondary);
    if ids.is_null() {
        // SAFETY: the game's out pointer, checked non-null.
        unsafe { count.write(wanted) };
        return OK;
    }
    // SAFETY: the game's out pointer, checked non-null.
    if unsafe { count.read() } < wanted {
        // SAFETY: as above.
        unsafe { count.write(wanted) };
        return INSUFFICIENT_BUFFER;
    }
    for i in 0..wanted {
        // SAFETY: the game's array holds `*count >= wanted` entries.
        let entry = unsafe { ids.add(usize::try_from(i).unwrap_or(0)) };
        // SAFETY: as above; every version starts with the version word.
        let version = unsafe { entry.cast::<u32>().read() };
        if version != GPU_DISPLAYIDS_VER1 && version != GPU_DISPLAYIDS_VER2 {
            return INCOMPATIBLE_STRUCT_VERSION;
        }
        let display_id = if i == 0 { primary } else { secondary };
        let flags = DISPLAYIDS_ACTIVE | DISPLAYIDS_CONNECTED | DISPLAYIDS_PHYSICALLY_CONNECTED;
        // SAFETY: the entry is the game's.
        unsafe { entry.write(GpuDisplayIds { version, connector_type: MONITOR_CONN_TYPE_UNKNOWN, display_id, flags }) };
    }
    OK
}

extern "C" fn get_gdi_primary_display_id(id: *mut u32) -> Status {
    if id.is_null() {
        return INVALID_ARGUMENT;
    }
    let primary = primary_display_id();
    // SAFETY: the game's out pointer, checked non-null.
    unsafe { id.write(primary) };
    OK
}

/// The address of an `NvAPI_*` function, for `nvapi_QueryInterface`.
const fn addr(function: *const ()) -> *mut c_void {
    function.cast_mut().cast()
}

/// The function `id` names, or null if this module has none.
#[must_use]
pub fn query(id: u32) -> *mut c_void {
    match id {
        0x34ef_9506 => addr(get_physical_gpus_from_display as *const ()),
        0xae45_7190 => addr(get_display_id_by_display_name as *const ()),
        0x84f2_a8df => addr(get_hdr_capabilities as *const ()),
        0x351d_a224 => addr(hdr_color_control as *const ()),
        0x3b05_c7e1 => addr(get_monitor_capabilities as *const ()),
        0x9abd_d40d => addr(enum_nvidia_display_handle as *const ()),
        0x0078_dba2 => addr(get_connected_display_ids as *const ()),
        0x1e9d_8a31 => addr(get_gdi_primary_display_id as *const ()),
        _ => core::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests;
