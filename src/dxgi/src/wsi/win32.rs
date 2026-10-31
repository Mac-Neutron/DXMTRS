//! Win32's window system, as `wsi_monitor_win32.cpp` and `wsi_window_win32.cpp` ask it.
//!
//! user32's functions, which meson links. A monitor or window handle is checked by user32, so a
//! stale one fails the call rather than the process.

use core::{ffi::c_void, ptr};

use super::{ENUM_CURRENT_SETTINGS, WsiMode};
use crate::{
    LOG,
    abi::{
        DM_INTERLACED, DevModeW, Hmonitor, Hwnd, MONITOR_DEFAULTTOPRIMARY, MonitorEnumProc,
        MonitorInfoExW, Point, Rect, Wchar,
    },
    modes,
};

unsafe extern "system" {
    fn MonitorFromPoint(point: Point, flags: u32) -> Hmonitor;
    fn EnumDisplayMonitors(
        dc: *mut c_void,
        clip: *const Rect,
        callback: MonitorEnumProc,
        data: isize,
    ) -> i32;
    fn GetMonitorInfoW(monitor: Hmonitor, info: *mut MonitorInfoExW) -> i32;
    fn EnumDisplaySettingsW(device: *const Wchar, mode: u32, devmode: *mut DevModeW) -> i32;
    fn GetClientRect(window: Hwnd, rect: *mut Rect) -> i32;
}

pub fn enum_monitors(index: u32) -> Option<Hmonitor> {
    let mut search = Search {
        left: index,
        found: ptr::null_mut(),
    };
    let data = (&raw mut search).expose_provenance().cast_signed();
    // SAFETY: `countdown` reaches `search` through `data`, which outlives the enumeration.
    unsafe { EnumDisplayMonitors(ptr::null_mut(), ptr::null(), countdown, data) };
    (!search.found.is_null()).then_some(search.found)
}

pub fn default_monitor() -> Option<Hmonitor> {
    // SAFETY: the point goes by value; the flag makes user32 answer the primary monitor.
    let monitor = unsafe { MonitorFromPoint(Point::default(), MONITOR_DEFAULTTOPRIMARY) };
    (!monitor.is_null()).then_some(monitor)
}

pub fn desktop_coordinates(monitor: Hmonitor) -> Option<Rect> {
    // The C++ names `getDisplayName` in this function's log line too.
    monitor_info(monitor, "getDisplayName").map(|info| info.monitor)
}

pub fn display_name(monitor: Hmonitor) -> Option<[Wchar; 32]> {
    monitor_info(monitor, "getDisplayName").map(|info| info.device)
}

pub fn display_mode(monitor: Hmonitor, number: u32) -> Option<WsiMode> {
    let info = monitor_info(monitor, "retrieveDisplayMode")?;
    let mut mode = DevModeW {
        size: u16::try_from(size_of::<DevModeW>()).unwrap_or(0),
        ..DevModeW::default()
    };
    // SAFETY: the device name is the NUL-terminated one user32 wrote; `mode` is a local
    // `DEVMODEW` with its size set.
    if unsafe { EnumDisplaySettingsW(info.device.as_ptr(), number, &raw mut mode) } == 0 {
        return None;
    }
    Some(WsiMode {
        width: mode.pels_width,
        height: mode.pels_height,
        refresh: modes::refresh_from_hz(mode.display_frequency),
        bits_per_pixel: mode.bits_per_pel,
        interlaced: mode.display_flags & DM_INTERLACED != 0,
    })
}

pub fn current_display_mode(monitor: Hmonitor) -> Option<WsiMode> {
    display_mode(monitor, ENUM_CURRENT_SETTINGS)
}

pub fn window_size(window: Hwnd) -> (u32, u32) {
    let mut rect = Rect::default();
    // SAFETY: `rect` is a local the call writes; for a bad window it stays zero.
    unsafe { GetClientRect(window, &raw mut rect) };
    (
        rect.right.wrapping_sub(rect.left).cast_unsigned(),
        rect.bottom.wrapping_sub(rect.top).cast_unsigned(),
    )
}

/// What `enum_monitors` looks for: the monitor `left` monitors further on.
struct Search {
    left: u32,
    found: Hmonitor,
}

/// `EnumDisplayMonitors`' callback: counts `data`'s `Search` down to its monitor, then stops.
///
/// One word for a name, so that the i686 callee-pop check, which pairs `<class>_<method>` names
/// with the C++'s methods, does not look for a counterpart it has none of.
const extern "system" fn countdown(
    monitor: Hmonitor,
    _dc: *mut c_void,
    _rect: *mut Rect,
    data: isize,
) -> i32 {
    let search = ptr::with_exposed_provenance_mut::<Search>(data.cast_unsigned());
    // SAFETY: `data` is the `Search` `enum_monitors` passed, alive and not otherwise borrowed
    // while the enumeration runs.
    let Some(search) = (unsafe { search.as_mut() }) else {
        return 0;
    };
    if search.left > 0 {
        search.left -= 1;
        return 1;
    }
    search.found = monitor;
    0
}

/// `monitor`'s `MONITORINFOEXW`, or none, logged under the C++ function `what`.
fn monitor_info(monitor: Hmonitor, what: &str) -> Option<MonitorInfoExW> {
    let mut info = MonitorInfoExW {
        size: u32::try_from(size_of::<MonitorInfoExW>()).unwrap_or(0),
        ..MonitorInfoExW::default()
    };
    // SAFETY: `info` is a local `MONITORINFOEXW` with its size set.
    if unsafe { GetMonitorInfoW(monitor, &raw mut info) } == 0 {
        LOG.err(&format!("Win32 WSI: {what}: Failed to query monitor info"));
        return None;
    }
    Some(info)
}
