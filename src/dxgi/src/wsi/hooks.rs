//! The native window system: winemetal's hooks; stubs for now.

use super::WsiMode;
use crate::abi::{Hmonitor, Hwnd, Rect, Wchar};

pub fn enum_monitors(_index: u32) -> Option<Hmonitor> {
    not_ported("enum_monitors");
    None
}

pub fn default_monitor() -> Option<Hmonitor> {
    not_ported("default_monitor");
    None
}

pub fn desktop_coordinates(_monitor: Hmonitor) -> Option<Rect> {
    not_ported("desktop_coordinates");
    None
}

pub fn display_name(_monitor: Hmonitor) -> Option<[Wchar; 32]> {
    not_ported("display_name");
    None
}

pub fn display_mode(_monitor: Hmonitor, _number: u32) -> Option<WsiMode> {
    not_ported("display_mode");
    None
}

pub fn current_display_mode(_monitor: Hmonitor) -> Option<WsiMode> {
    not_ported("current_display_mode");
    None
}

pub fn window_size(_window: Hwnd) -> (u32, u32) {
    not_ported("window_size");
    (0, 0)
}

/// A stub's log line: the function is not ported yet.
fn not_ported(name: &str) {
    crate::LOG.err(&format!("wsi::{name}: not ported yet"));
}
