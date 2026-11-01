//! Win32's window system, as `wsi_monitor_win32.cpp` and `wsi_window_win32.cpp` ask it.
//!
//! `user32`, `gdi32` and `kernel32` functions linked by meson. A monitor or window handle is
//! checked by `user32`, so a stale one fails the call rather than the process.

use core::{ffi::c_void, mem::offset_of, ptr};

use super::{
    ENUM_CURRENT_SETTINGS, ENUM_REGISTRY_SETTINGS, Hmonitor, Hwnd, Rational, Rect, Wchar,
    WindowState, WsiMode,
};

type MonitorEnumProc = extern "system" fn(Hmonitor, *mut c_void, *mut Rect, isize) -> i32;

const MONITOR_DEFAULTTOPRIMARY: u32 = 1;
const DM_BITSPERPEL: u32 = 0x0004_0000;
const DM_PELSWIDTH: u32 = 0x0008_0000;
const DM_PELSHEIGHT: u32 = 0x0010_0000;
const DM_DISPLAYFLAGS: u32 = 0x0020_0000;
const DM_DISPLAYFREQUENCY: u32 = 0x0040_0000;
const DM_INTERLACED: u32 = 2;
const CDS_FULLSCREEN: u32 = 4;
const DISP_CHANGE_SUCCESSFUL: i32 = 0;

const GWL_STYLE: i32 = -16;
const GWL_EXSTYLE: i32 = -20;
const WS_OVERLAPPEDWINDOW: i32 = 0x00cf_0000;
const WS_VISIBLE: i32 = 0x1000_0000;
const WS_EX_OVERLAPPEDWINDOW: i32 = 0x0000_0300;
const WS_EX_TOPMOST: i32 = 0x0000_0008;

const SW_MINIMIZE: i32 = 6;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;
const SWP_SHOWWINDOW: u32 = 0x0040;
const HWND_TOPMOST: Hwnd = ptr::without_provenance_mut(usize::MAX);
const HWND_NOTOPMOST: Hwnd = ptr::without_provenance_mut(usize::MAX - 1);

const D3DKMT_ESCAPE_SET_PRESENT_RECT_WINE: i32 = 0x8000_0001_u32.cast_signed();

/// `POINT`, which `MonitorFromPoint` takes by value.
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

/// `MONITORINFOEXW`: `MONITORINFO` and the display's device name.
#[derive(Default)]
#[repr(C)]
struct MonitorInfoExW {
    size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
    device: [Wchar; 32],
}

/// `DEVMODEW`, with its two unions' display arms.
#[derive(Default)]
#[repr(C)]
struct DevModeW {
    device_name: [Wchar; 32],
    spec_version: u16,
    driver_version: u16,
    size: u16,
    driver_extra: u16,
    fields: u32,
    position: Point,
    display_orientation: u32,
    display_fixed_output: u32,
    color: i16,
    duplex: i16,
    y_resolution: i16,
    tt_option: i16,
    collate: i16,
    form_name: [Wchar; 32],
    log_pixels: u16,
    bits_per_pel: u32,
    pels_width: u32,
    pub pels_height: u32,
    display_flags: u32,
    display_frequency: u32,
    icm_method: u32,
    icm_intent: u32,
    media_type: u32,
    dither_type: u32,
    reserved1: u32,
    reserved2: u32,
    panning_width: u32,
    panning_height: u32,
}

/// `D3DKMT_ESCAPE`.
#[repr(C)]
struct D3dkmtEscape {
    adapter: u32,
    device: u32,
    escape_type: i32,
    flags: u32,
    private_driver_data: *mut c_void,
    private_driver_data_size: u32,
    context: u32,
}

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
    fn ChangeDisplaySettingsExW(
        device_name: *const Wchar,
        dev_mode: *mut DevModeW,
        hwnd: Hwnd,
        flags: u32,
        param: *mut c_void,
    ) -> i32;
    fn GetClientRect(window: Hwnd, rect: *mut Rect) -> i32;
    fn GetWindowRect(window: Hwnd, rect: *mut Rect) -> i32;
    fn AdjustWindowRectEx(rect: *mut Rect, style: u32, menu: i32, exstyle: u32) -> i32;
    fn IsWindow(window: Hwnd) -> i32;
    fn IsIconic(window: Hwnd) -> i32;
    fn GetForegroundWindow() -> Hwnd;
    fn GetWindowThreadProcessId(window: Hwnd, process_id: *mut u32) -> u32;
    fn GetWindowLongW(window: Hwnd, index: i32) -> i32;
    fn SetWindowLongW(window: Hwnd, index: i32, new_long: i32) -> i32;
    fn MoveWindow(window: Hwnd, x: i32, y: i32, width: i32, height: i32, repaint: i32) -> i32;
    fn SetWindowPos(
        window: Hwnd,
        insert_after: Hwnd,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
    fn ShowWindowAsync(window: Hwnd, cmd_show: i32) -> i32;
    fn D3DKMTEscape(escape: *const D3dkmtEscape) -> i32;
    fn GetCurrentProcessId() -> u32;
}

const fn refresh_from_hz(hz: u32) -> Rational {
    Rational {
        numerator: hz.wrapping_mul(1000),
        denominator: 1000,
    }
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
    monitor_info(monitor).map(|info| info.monitor)
}

pub fn display_name(monitor: Hmonitor) -> Option<[Wchar; 32]> {
    monitor_info(monitor).map(|info| info.device)
}

pub fn display_mode(monitor: Hmonitor, number: u32) -> Option<WsiMode> {
    let info = monitor_info(monitor)?;
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
        refresh: refresh_from_hz(mode.display_frequency),
        bits_per_pixel: mode.bits_per_pel,
        interlaced: mode.display_flags & DM_INTERLACED != 0,
    })
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

pub fn window_monitor(window: Hwnd) -> Hmonitor {
    let mut window_rect = Rect::default();
    // SAFETY: `window_rect` is a live `RECT` on the stack.
    unsafe { GetWindowRect(window, &raw mut window_rect) };
    let pt = Point {
        x: i32::midpoint(window_rect.left, window_rect.right),
        y: i32::midpoint(window_rect.top, window_rect.bottom),
    };
    // SAFETY: `pt` goes by value; `MONITOR_DEFAULTTOPRIMARY` falls back to the primary monitor.
    unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY) }
}

pub fn is_window(window: Hwnd) -> bool {
    // SAFETY: `IsWindow` checks the handle in `user32` without dereferencing it in user mode.
    (unsafe { IsWindow(window) }) != 0
}

pub fn is_minimized(window: Hwnd) -> bool {
    // SAFETY: `IsIconic` queries whether `window` is minimized in `user32`.
    (unsafe { IsIconic(window) }) != 0
}

pub fn is_foreground(window: Hwnd) -> bool {
    // SAFETY: `GetForegroundWindow` takes no arguments and returns a handle value.
    (unsafe { GetForegroundWindow() }) == window
}

pub fn is_process_foreground() -> bool {
    // SAFETY: `GetForegroundWindow` takes no arguments.
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() {
        return false;
    }
    let mut process_id = 0_u32;
    // SAFETY: `process_id` is a live `u32` on the stack.
    unsafe { GetWindowThreadProcessId(fg, &raw mut process_id) };
    // SAFETY: `GetCurrentProcessId` takes no arguments.
    let current = unsafe { GetCurrentProcessId() };
    process_id != 0 && process_id == current
}

pub fn minimize_window(window: Hwnd) {
    // SAFETY: `ShowWindowAsync` posts the show state to the window's thread.
    unsafe { ShowWindowAsync(window, SW_MINIMIZE) };
}

pub fn save_window_state(window: Hwnd, state: &mut WindowState) {
    // SAFETY: `GetWindowLongW` reads `window`'s style in `user32`.
    state.style = unsafe { GetWindowLongW(window, GWL_STYLE) };
    // SAFETY: `GetWindowLongW` reads `window`'s extended style in `user32`.
    state.exstyle = unsafe { GetWindowLongW(window, GWL_EXSTYLE) };
    // SAFETY: `state.rect` is a live `RECT` on the caller's `WindowState`.
    unsafe { GetWindowRect(window, &raw mut state.rect) };
}

fn set_monitor_display_mode(monitor: Hmonitor, mode: &WsiMode) -> bool {
    let Some(info) = monitor_info(monitor) else {
        return false;
    };
    let mut devmode = DevModeW {
        size: u16::try_from(size_of::<DevModeW>()).unwrap_or(0),
        fields: DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL,
        pels_width: mode.width,
        pels_height: mode.height,
        bits_per_pel: mode.bits_per_pixel,
        ..DevModeW::default()
    };
    if mode.refresh.numerator != 0 {
        devmode.fields |= DM_DISPLAYFREQUENCY;
        devmode.display_frequency = mode.refresh.numerator / mode.refresh.denominator.max(1);
    }
    let mut cur_mode = DevModeW {
        size: u16::try_from(size_of::<DevModeW>()).unwrap_or(0),
        ..DevModeW::default()
    };
    // SAFETY: `info.device` is NUL-terminated and `cur_mode` is a live `DEVMODEW` on the stack.
    if unsafe {
        EnumDisplaySettingsW(
            info.device.as_ptr(),
            ENUM_CURRENT_SETTINGS,
            &raw mut cur_mode,
        )
    } != 0
    {
        let mut eq = cur_mode.pels_width == devmode.pels_width
            && cur_mode.pels_height == devmode.pels_height
            && cur_mode.bits_per_pel == devmode.bits_per_pel;
        if (devmode.fields & DM_DISPLAYFREQUENCY) != 0 {
            eq &= cur_mode.display_frequency == devmode.display_frequency;
        }
        if eq {
            return true;
        }
    }
    // SAFETY: `info.device` and `devmode` are live on the stack.
    let mut status = unsafe {
        ChangeDisplaySettingsExW(
            info.device.as_ptr(),
            &raw mut devmode,
            ptr::null_mut(),
            CDS_FULLSCREEN,
            ptr::null_mut(),
        )
    };
    if status != DISP_CHANGE_SUCCESSFUL {
        devmode.fields &= !DM_DISPLAYFREQUENCY;
        // SAFETY: as above.
        status = unsafe {
            ChangeDisplaySettingsExW(
                info.device.as_ptr(),
                &raw mut devmode,
                ptr::null_mut(),
                CDS_FULLSCREEN,
                ptr::null_mut(),
            )
        };
    }
    status == DISP_CHANGE_SUCCESSFUL
}

fn send_present_rect_escape(window: Hwnd, mut rect: Rect) {
    let escape = D3dkmtEscape {
        adapter: 0,
        device: 0,
        escape_type: D3DKMT_ESCAPE_SET_PRESENT_RECT_WINE,
        flags: 0,
        private_driver_data: (&raw mut rect).cast(),
        private_driver_data_size: u32::try_from(size_of::<Rect>()).unwrap_or(0),
        context: u32::try_from(
            u64::try_from(window.expose_provenance()).unwrap_or(0) & u64::from(u32::MAX),
        )
        .unwrap_or(0),
    };
    // SAFETY: `escape` and `rect` are live on the stack for the duration of the call.
    unsafe { D3DKMTEscape(&raw const escape) };
}

pub fn set_window_mode(monitor: Hmonitor, _window: Hwnd, mode: &WsiMode) -> bool {
    set_monitor_display_mode(monitor, mode)
}

pub fn enter_fullscreen_mode(
    monitor: Hmonitor,
    window: Hwnd,
    state: &mut WindowState,
    _mode_switch: bool,
) -> bool {
    let rect = desktop_coordinates(monitor).unwrap_or_default();
    send_present_rect_escape(window, rect);
    if state.style == 0 && state.exstyle == 0 && state.rect == Rect::default() {
        save_window_state(window, state);
    }
    let style = state.style & !WS_OVERLAPPEDWINDOW;
    let exstyle = state.exstyle & !WS_EX_OVERLAPPEDWINDOW;
    // SAFETY: `SetWindowLongW` sets `window`'s style in `user32`.
    unsafe { SetWindowLongW(window, GWL_STYLE, style) };
    // SAFETY: `SetWindowLongW` sets `window`'s extended style in `user32`.
    unsafe { SetWindowLongW(window, GWL_EXSTYLE, exstyle) };
    // SAFETY: `SetWindowPos` repositions `window` in `user32`.
    unsafe {
        SetWindowPos(
            window,
            HWND_TOPMOST,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE,
        )
    };
    true
}

pub fn leave_fullscreen_mode(window: Hwnd, state: &WindowState, restore_coordinates: bool) -> bool {
    // SAFETY: `GetWindowLongW` queries `window`'s current style in `user32`.
    let cur_style = (unsafe { GetWindowLongW(window, GWL_STYLE) }) & !WS_VISIBLE;
    // SAFETY: `GetWindowLongW` queries `window`'s current extended style in `user32`.
    let cur_exstyle = (unsafe { GetWindowLongW(window, GWL_EXSTYLE) }) & !WS_EX_TOPMOST;
    if cur_style == (state.style & !(WS_VISIBLE | WS_OVERLAPPEDWINDOW))
        && cur_exstyle == (state.exstyle & !(WS_EX_TOPMOST | WS_EX_OVERLAPPEDWINDOW))
    {
        // SAFETY: `SetWindowLongW` restores `window`'s style in `user32`.
        unsafe { SetWindowLongW(window, GWL_STYLE, state.style) };
        // SAFETY: `SetWindowLongW` restores `window`'s extended style in `user32`.
        unsafe { SetWindowLongW(window, GWL_EXSTYLE, state.exstyle) };
    }
    send_present_rect_escape(window, Rect::default());
    let mut flags = SWP_FRAMECHANGED | SWP_NOACTIVATE;
    if !restore_coordinates {
        flags |= SWP_NOSIZE | SWP_NOMOVE;
    }
    let insert_after = if (state.exstyle & WS_EX_TOPMOST) != 0 {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    // SAFETY: `SetWindowPos` restores `window`'s position and Z-order in `user32`.
    unsafe {
        SetWindowPos(
            window,
            insert_after,
            state.rect.left,
            state.rect.top,
            state.rect.right - state.rect.left,
            state.rect.bottom - state.rect.top,
            flags,
        )
    };
    true
}

pub fn restore_display_mode() -> bool {
    let mut data = Success(true);
    let param = (&raw mut data).expose_provenance().cast_signed();
    // SAFETY: `restore` reaches `data` through `param`, which outlives the enumeration.
    let result = unsafe { EnumDisplayMonitors(ptr::null_mut(), ptr::null(), restore, param) };
    result != 0 && data.0
}

pub fn resize_window(window: Hwnd, _state: &WindowState, width: u32, height: u32) {
    let mut old_rect = Rect::default();
    // SAFETY: `old_rect` is a live `RECT` on the stack.
    unsafe { GetWindowRect(window, &raw mut old_rect) };
    let mut new_rect = Rect {
        left: 0,
        top: 0,
        right: width.cast_signed(),
        bottom: height.cast_signed(),
    };
    // SAFETY: `GetWindowLongW` reads `window`'s current style in `user32`.
    let style = unsafe { GetWindowLongW(window, GWL_STYLE) };
    // SAFETY: `GetWindowLongW` reads `window`'s current extended style in `user32`.
    let exstyle = unsafe { GetWindowLongW(window, GWL_EXSTYLE) };
    // SAFETY: `new_rect` is a live `RECT` on the stack.
    unsafe {
        AdjustWindowRectEx(
            &raw mut new_rect,
            style.cast_unsigned(),
            0,
            exstyle.cast_unsigned(),
        )
    };
    // SAFETY: `MoveWindow` resizes `window` at its current position in `user32`.
    unsafe {
        MoveWindow(
            window,
            old_rect.left,
            old_rect.top,
            new_rect.right - new_rect.left,
            new_rect.bottom - new_rect.top,
            1,
        )
    };
}

pub fn update_fullscreen_window(monitor: Hmonitor, window: Hwnd, force_topmost: bool) {
    let bounds = desktop_coordinates(monitor).unwrap_or_default();
    send_present_rect_escape(window, bounds);
    if force_topmost {
        // SAFETY: `SetWindowPos` brings the fullscreen window to the top of the Z-order.
        unsafe {
            SetWindowPos(
                window,
                HWND_TOPMOST,
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE,
            )
        };
    } else {
        // SAFETY: `MoveWindow` updates `window`'s fullscreen bounds in `user32`.
        unsafe {
            MoveWindow(
                window,
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                1,
            )
        };
    }
}

/// What `enum_monitors` looks for: the monitor `left` monitors further on.
struct Search {
    left: u32,
    found: Hmonitor,
}

struct Success(bool);

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

/// `EnumDisplayMonitors`' callback for `restore_display_mode`.
extern "system" fn restore(
    monitor: Hmonitor,
    _dc: *mut c_void,
    _rect: *mut Rect,
    data: isize,
) -> i32 {
    let success = ptr::with_exposed_provenance_mut::<Success>(data.cast_unsigned());
    // SAFETY: `data` is the `Success` `restore_display_mode` passed, alive during enumeration.
    let Some(success) = (unsafe { success.as_mut() }) else {
        return 0;
    };
    let Some(info) = monitor_info(monitor) else {
        success.0 = false;
        return 0;
    };
    let mut current_mode = DevModeW {
        size: u16::try_from(size_of::<DevModeW>()).unwrap_or(0),
        ..DevModeW::default()
    };
    let mut registry_mode = DevModeW {
        size: u16::try_from(size_of::<DevModeW>()).unwrap_or(0),
        ..DevModeW::default()
    };
    // SAFETY: `info.device` is NUL-terminated and `current_mode` is a live `DEVMODEW`.
    if unsafe {
        EnumDisplaySettingsW(
            info.device.as_ptr(),
            ENUM_CURRENT_SETTINGS,
            &raw mut current_mode,
        )
    } == 0
    {
        success.0 = false;
        return 0;
    }
    // SAFETY: `info.device` is NUL-terminated and `registry_mode` is a live `DEVMODEW`.
    if unsafe {
        EnumDisplaySettingsW(
            info.device.as_ptr(),
            ENUM_REGISTRY_SETTINGS,
            &raw mut registry_mode,
        )
    } == 0
    {
        success.0 = false;
        return 0;
    }
    let freq_diff = (current_mode.fields & registry_mode.fields & DM_DISPLAYFREQUENCY) != 0
        && current_mode.display_frequency != registry_mode.display_frequency;
    if current_mode.pels_width == registry_mode.pels_width
        && current_mode.pels_height == registry_mode.pels_height
        && current_mode.bits_per_pel == registry_mode.bits_per_pel
        && !freq_diff
    {
        return 1;
    }
    // SAFETY: `info.device` is NUL-terminated; null `DEVMODEW` restores the registry mode.
    let ret = unsafe {
        ChangeDisplaySettingsExW(
            info.device.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
        )
    };
    if ret != DISP_CHANGE_SUCCESSFUL {
        success.0 = false;
        return 0;
    }
    1
}

/// `monitor`'s `MONITORINFOEXW`, or none.
fn monitor_info(monitor: Hmonitor) -> Option<MonitorInfoExW> {
    let mut info = MonitorInfoExW {
        size: u32::try_from(size_of::<MonitorInfoExW>()).unwrap_or(0),
        ..MonitorInfoExW::default()
    };
    // SAFETY: `info` is a local `MONITORINFOEXW` with its size set.
    if unsafe { GetMonitorInfoW(monitor, &raw mut info) } == 0 {
        return None;
    }
    Some(info)
}

const _: () = {
    let _ = DM_DISPLAYFLAGS;
    assert!(size_of::<DevModeW>() == 220 && align_of::<DevModeW>() == 4);
    assert!(offset_of!(DevModeW, device_name) == 0);
    assert!(offset_of!(DevModeW, spec_version) == 64);
    assert!(offset_of!(DevModeW, driver_version) == 66);
    assert!(offset_of!(DevModeW, size) == 68);
    assert!(offset_of!(DevModeW, driver_extra) == 70);
    assert!(offset_of!(DevModeW, fields) == 72);
    assert!(offset_of!(DevModeW, position) == 76);
    assert!(offset_of!(DevModeW, display_orientation) == 84);
    assert!(offset_of!(DevModeW, display_fixed_output) == 88);
    assert!(offset_of!(DevModeW, color) == 92);
    assert!(offset_of!(DevModeW, duplex) == 94);
    assert!(offset_of!(DevModeW, y_resolution) == 96);
    assert!(offset_of!(DevModeW, tt_option) == 98);
    assert!(offset_of!(DevModeW, collate) == 100);
    assert!(offset_of!(DevModeW, form_name) == 102);
    assert!(offset_of!(DevModeW, log_pixels) == 166);
    assert!(offset_of!(DevModeW, bits_per_pel) == 168);
    assert!(offset_of!(DevModeW, pels_width) == 172);
    assert!(offset_of!(DevModeW, pels_height) == 176);
    assert!(offset_of!(DevModeW, display_flags) == 180);
    assert!(offset_of!(DevModeW, display_frequency) == 184);
    assert!(offset_of!(DevModeW, icm_method) == 188);
    assert!(offset_of!(DevModeW, icm_intent) == 192);
    assert!(offset_of!(DevModeW, media_type) == 196);
    assert!(offset_of!(DevModeW, dither_type) == 200);
    assert!(offset_of!(DevModeW, reserved1) == 204);
    assert!(offset_of!(DevModeW, reserved2) == 208);
    assert!(offset_of!(DevModeW, panning_width) == 212);
    assert!(offset_of!(DevModeW, panning_height) == 216);
    assert!(size_of::<MonitorInfoExW>() == 104 && align_of::<MonitorInfoExW>() == 4);
    assert!(offset_of!(MonitorInfoExW, size) == 0);
    assert!(offset_of!(MonitorInfoExW, monitor) == 4);
    assert!(offset_of!(MonitorInfoExW, work) == 20);
    assert!(offset_of!(MonitorInfoExW, flags) == 36);
    assert!(offset_of!(MonitorInfoExW, device) == 40);
    assert!(size_of::<Point>() == 8 && align_of::<Point>() == 4);
    assert!(offset_of!(Point, x) == 0);
    assert!(offset_of!(Point, y) == 4);
};
