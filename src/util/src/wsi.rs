//! What DXGI and the swap chains ask the window system: displays, modes and windows.
//!
//! Two backends: `win32` on PE (`user32` and `gdi32`) and `hooks` natively (`WMTGetWsiHooks()`, or
//! the 1920x1080@60 headless fallback when no VM host installed hooks).

use core::{ffi::c_void, mem::offset_of};

#[cfg(not(windows))]
mod hooks;
#[cfg(windows)]
mod win32;

/// `WCHAR`: UTF-16 on PE.
#[cfg(windows)]
pub type Wchar = u16;
/// `WCHAR`: natively `wchar_t`, UTF-32.
#[cfg(not(windows))]
pub type Wchar = u32;
/// `HWND`.
pub type Hwnd = *mut c_void;
/// `HMONITOR`.
pub type Hmonitor = *mut c_void;

/// `ENUM_CURRENT_SETTINGS`: `(DWORD)-1`, the display's active mode.
pub const ENUM_CURRENT_SETTINGS: u32 = u32::MAX;
/// `ENUM_REGISTRY_SETTINGS`: `(DWORD)-2`, the display's registry desktop mode.
pub const ENUM_REGISTRY_SETTINGS: u32 = u32::MAX - 1;

/// `RECT`.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// `DXGI_RATIONAL`.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32,
}

/// `WsiMode` in `wsi_monitor.hpp`.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct WsiMode {
    pub width: u32,
    pub height: u32,
    pub refresh: Rational,
    pub bits_per_pixel: u32,
    pub interlaced: bool,
}

/// `DXMTWindowState` in `wsi_platform_win32.hpp`: a window's saved windowed styles and rectangle.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct WindowState {
    pub style: i32,
    pub exstyle: i32,
    pub rect: Rect,
}

/// `wsi::enumMonitors(index)`.
#[must_use]
pub fn enum_monitors(index: u32) -> Option<Hmonitor> {
    #[cfg(windows)]
    {
        win32::enum_monitors(index)
    }
    #[cfg(not(windows))]
    {
        hooks::enum_monitors(index)
    }
}

/// `wsi::getDefaultMonitor()`.
#[must_use]
pub fn default_monitor() -> Option<Hmonitor> {
    #[cfg(windows)]
    {
        win32::default_monitor()
    }
    #[cfg(not(windows))]
    {
        hooks::default_monitor()
    }
}

/// `wsi::getDesktopCoordinates(hMonitor, pRect)`.
#[must_use]
pub fn desktop_coordinates(monitor: Hmonitor) -> Option<Rect> {
    #[cfg(windows)]
    {
        win32::desktop_coordinates(monitor)
    }
    #[cfg(not(windows))]
    {
        hooks::desktop_coordinates(monitor)
    }
}

/// `wsi::getDisplaySize(hMonitor, pWidth, pHeight)`.
#[must_use]
pub fn display_size(monitor: Hmonitor) -> (u32, u32) {
    desktop_coordinates(monitor).map_or((0, 0), |r| {
        (
            (r.right - r.left).cast_unsigned(),
            (r.bottom - r.top).cast_unsigned(),
        )
    })
}

/// `wsi::getDisplayName(hMonitor, Name)`: the 32-`WCHAR` `CCHDEVICENAME` buffer.
#[must_use]
pub fn display_name(monitor: Hmonitor) -> Option<[Wchar; 32]> {
    #[cfg(windows)]
    {
        win32::display_name(monitor)
    }
    #[cfg(not(windows))]
    {
        hooks::display_name(monitor)
    }
}

/// `wsi::getDisplayMode(hMonitor, modeNumber, pMode)`.
#[must_use]
pub fn display_mode(monitor: Hmonitor, number: u32) -> Option<WsiMode> {
    #[cfg(windows)]
    {
        win32::display_mode(monitor, number)
    }
    #[cfg(not(windows))]
    {
        hooks::display_mode(monitor, number)
    }
}

/// `wsi::getCurrentDisplayMode(hMonitor, pMode)`.
#[must_use]
pub fn current_display_mode(monitor: Hmonitor) -> Option<WsiMode> {
    display_mode(monitor, ENUM_CURRENT_SETTINGS)
}

/// `wsi::getDesktopDisplayMode(hMonitor, pMode)`.
#[must_use]
pub fn desktop_display_mode(monitor: Hmonitor) -> Option<WsiMode> {
    display_mode(monitor, ENUM_REGISTRY_SETTINGS)
}

/// `wsi::getWindowSize(hWindow, pWidth, pHeight)`.
#[must_use]
pub fn window_size(window: Hwnd) -> (u32, u32) {
    #[cfg(windows)]
    {
        win32::window_size(window)
    }
    #[cfg(not(windows))]
    {
        hooks::window_size(window)
    }
}

/// `wsi::getWindowMonitor(hWindow)`.
#[must_use]
pub fn window_monitor(window: Hwnd) -> Hmonitor {
    #[cfg(windows)]
    {
        win32::window_monitor(window)
    }
    #[cfg(not(windows))]
    {
        hooks::window_monitor(window)
    }
}

/// `wsi::isWindow(hWindow)`.
#[must_use]
pub fn is_window(window: Hwnd) -> bool {
    #[cfg(windows)]
    {
        win32::is_window(window)
    }
    #[cfg(not(windows))]
    {
        hooks::is_window(window)
    }
}

/// `wsi::isMinimized(hWindow)`.
#[must_use]
pub fn is_minimized(window: Hwnd) -> bool {
    #[cfg(windows)]
    {
        win32::is_minimized(window)
    }
    #[cfg(not(windows))]
    {
        hooks::is_minimized(window)
    }
}

/// `wsi::isForeground(hWindow)`.
#[must_use]
pub fn is_foreground(window: Hwnd) -> bool {
    #[cfg(windows)]
    {
        win32::is_foreground(window)
    }
    #[cfg(not(windows))]
    {
        hooks::is_foreground(window)
    }
}

/// `wsi::isProcessForeground()`.
#[must_use]
pub fn is_process_foreground() -> bool {
    #[cfg(windows)]
    {
        win32::is_process_foreground()
    }
    #[cfg(not(windows))]
    {
        hooks::is_process_foreground()
    }
}

/// `wsi::isOccluded(hWindow)`: always `false` in DXMT (`FIXME: not implemented`).
#[must_use]
pub const fn is_occluded(_window: Hwnd) -> bool {
    false
}

/// `wsi::minimizeWindow(hWindow)`.
pub fn minimize_window(window: Hwnd) {
    #[cfg(windows)]
    {
        win32::minimize_window(window);
    }
    #[cfg(not(windows))]
    {
        hooks::minimize_window(window);
    }
}

/// `wsi::saveWindowState(hWindow, pState)`.
pub fn save_window_state(window: Hwnd, state: &mut WindowState) {
    #[cfg(windows)]
    {
        win32::save_window_state(window, state);
    }
    #[cfg(not(windows))]
    {
        hooks::save_window_state(window, state);
    }
}

/// `wsi::setWindowMode(hMonitor, hWindow, mode)`.
#[must_use]
pub fn set_window_mode(monitor: Hmonitor, window: Hwnd, mode: &WsiMode) -> bool {
    #[cfg(windows)]
    {
        win32::set_window_mode(monitor, window, mode)
    }
    #[cfg(not(windows))]
    {
        hooks::set_window_mode(monitor, window, mode)
    }
}

/// `wsi::enterFullscreenMode(hMonitor, hWindow, pState, modeSwitch)`.
#[must_use]
pub fn enter_fullscreen_mode(
    monitor: Hmonitor,
    window: Hwnd,
    state: &mut WindowState,
    mode_switch: bool,
) -> bool {
    #[cfg(windows)]
    {
        win32::enter_fullscreen_mode(monitor, window, state, mode_switch)
    }
    #[cfg(not(windows))]
    {
        hooks::enter_fullscreen_mode(monitor, window, state, mode_switch)
    }
}

/// `wsi::leaveFullscreenMode(hWindow, pState, restoreCoordinates)`.
#[must_use]
pub fn leave_fullscreen_mode(window: Hwnd, state: &WindowState, restore_coordinates: bool) -> bool {
    #[cfg(windows)]
    {
        win32::leave_fullscreen_mode(window, state, restore_coordinates)
    }
    #[cfg(not(windows))]
    {
        hooks::leave_fullscreen_mode(window, state, restore_coordinates)
    }
}

/// `wsi::restoreDisplayMode(hMonitor)`.
#[must_use]
pub fn restore_display_mode(monitor: Hmonitor) -> bool {
    #[cfg(windows)]
    {
        win32::restore_display_mode(monitor)
    }
    #[cfg(not(windows))]
    {
        hooks::restore_display_mode(monitor)
    }
}

/// `wsi::resizeWindow(hWindow, pState, width, height)`.
pub fn resize_window(window: Hwnd, state: &WindowState, width: u32, height: u32) {
    #[cfg(windows)]
    {
        win32::resize_window(window, state, width, height);
    }
    #[cfg(not(windows))]
    {
        hooks::resize_window(window, state, width, height);
    }
}

/// `wsi::updateFullscreenWindow(hMonitor, hWindow, forceTopmost)`.
pub fn update_fullscreen_window(monitor: Hmonitor, window: Hwnd, force_topmost: bool) {
    #[cfg(windows)]
    {
        win32::update_fullscreen_window(monitor, window, force_topmost);
    }
    #[cfg(not(windows))]
    {
        hooks::update_fullscreen_window(monitor, window, force_topmost);
    }
}

/// Whether a VM host installed `WMTGetWsiHooks()` natively (`false` on PE).
///
/// When `true`, the guest already paces its frames and sleeps when minimised, so the host skips
/// its own latency wait and minimised `Sleep(16)`.
#[cfg(windows)]
#[must_use]
pub const fn hooked() -> bool {
    false
}

/// Whether a VM host installed `WMTGetWsiHooks()` natively (`false` on PE).
///
/// When `true`, the guest already paces its frames and sleeps when minimised, so the host skips
/// its own latency wait and minimised `Sleep(16)`.
#[cfg(not(windows))]
#[must_use]
pub fn hooked() -> bool {
    hooks::hooked()
}

const _: () = {
    assert!(size_of::<Rational>() == 8 && align_of::<Rational>() == 4);
    assert!(offset_of!(Rational, numerator) == 0);
    assert!(offset_of!(Rational, denominator) == 4);
    assert!(size_of::<Rect>() == 16 && align_of::<Rect>() == 4);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
};

#[cfg(test)]
mod tests;
