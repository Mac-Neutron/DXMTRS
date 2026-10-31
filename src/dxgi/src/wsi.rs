//! The window system dxgi asks about monitors, modes and windows: Win32 on PE, hooks natively.
//!
//! Natively the window system is the one a VM host answers for its guest through winemetal's hooks
//! (`WMTSetWsiHooks`); without hooks there is no monitor and no mode.

use crate::abi::{Hmonitor, Hwnd, Rational, Rect, Wchar};

#[cfg(not(windows))]
mod hooks;
#[cfg(windows)]
mod win32;

#[cfg(not(windows))]
use hooks as platform;
#[cfg(windows)]
use win32 as platform;

/// `ENUM_CURRENT_SETTINGS`: the mode number of the current mode.
pub const ENUM_CURRENT_SETTINGS: u32 = 0xffff_ffff;
/// `ENUM_REGISTRY_SETTINGS`: the mode number of the registry's (the desktop's) mode.
pub const ENUM_REGISTRY_SETTINGS: u32 = 0xffff_fffe;

/// A display mode, as `wsi::WsiMode` describes one.
#[derive(Default)]
pub struct WsiMode {
    pub width: u32,
    pub height: u32,
    pub refresh: Rational,
    pub bits_per_pixel: u32,
    pub interlaced: bool,
}

/// The `index`-th monitor, if there is one.
#[must_use]
pub fn enum_monitors(index: u32) -> Option<Hmonitor> {
    platform::enum_monitors(index)
}

/// The primary monitor.
#[must_use]
pub fn default_monitor() -> Option<Hmonitor> {
    platform::default_monitor()
}

/// `monitor`'s rectangle on the desktop.
#[must_use]
pub fn desktop_coordinates(monitor: Hmonitor) -> Option<Rect> {
    platform::desktop_coordinates(monitor)
}

/// `monitor`'s device name, NUL-terminated.
#[must_use]
pub fn display_name(monitor: Hmonitor) -> Option<[Wchar; 32]> {
    platform::display_name(monitor)
}

/// `monitor`'s mode `number`, or its current or registry mode (`ENUM_*_SETTINGS`).
#[must_use]
pub fn display_mode(monitor: Hmonitor, number: u32) -> Option<WsiMode> {
    platform::display_mode(monitor, number)
}

/// `monitor`'s current mode.
#[must_use]
pub fn current_display_mode(monitor: Hmonitor) -> Option<WsiMode> {
    platform::current_display_mode(monitor)
}

/// The size of `window`'s client area.
#[must_use]
pub fn window_size(window: Hwnd) -> (u32, u32) {
    platform::window_size(window)
}
