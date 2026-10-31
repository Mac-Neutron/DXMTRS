//! The native window system: winemetal's hooks (`WMTSetWsiHooks`).
//!
//! A VM host answers for its guest, from what the guest last told it, and sets its hooks before
//! any device. Without hooks (or with a hook missing) there is no monitor and no mode, and a
//! window is 1024 by 1024.

#[cfg(test)]
use core::cell::Cell;

use super::{ENUM_CURRENT_SETTINGS, ENUM_REGISTRY_SETTINGS, WsiMode};
use crate::abi::{
    Hmonitor, Hwnd, Rational, Rect, Wchar, WsiDisplay, WsiHookMode, WsiHooks, WsiWindow,
};

/// The size of a window the hooks do not know.
const DEFAULT_WINDOW_SIZE: u32 = 1024;

#[cfg(test)]
thread_local! {
    /// The hooks a test installed.
    ///
    /// Stands in for winemetal's process-wide table (`WMTSetWsiHooks`), which a host test does
    /// not link. Per thread, since the tests run in parallel and each installs its own. Test
    /// only.
    static TEST_HOOKS: Cell<*const WsiHooks> = const { Cell::new(core::ptr::null()) };
}

/// Installs `hooks` (or none) for the calling test.
#[cfg(test)]
fn set_hooks(hooks: Option<&'static WsiHooks>) {
    TEST_HOOKS.set(hooks.map_or(core::ptr::null(), core::ptr::from_ref));
}

/// The hooks table the host set, or null.
#[cfg(not(test))]
fn installed() -> *const WsiHooks {
    // SAFETY: winemetal's getter reads a pointer it keeps; it takes and touches nothing else.
    unsafe { crate::ffi::WMTGetWsiHooks() }
}

/// The hooks table the calling test installed, or null.
#[cfg(test)]
fn installed() -> *const WsiHooks {
    TEST_HOOKS.get()
}

/// The hooks a VM host set, if it did.
fn hooks() -> Option<&'static WsiHooks> {
    // SAFETY: the host sets a table that stays valid for the process before any device exists, or
    // none; winemetal keeps only the pointer.
    unsafe { installed().as_ref() }
}

/// The hooks' display `index`, if they have one.
fn display(index: u32) -> Option<WsiDisplay> {
    let hook = hooks()?.display?;
    let mut display = WsiDisplay::default();
    // SAFETY: the hook fills the display it is given and answers 1 if it did; `display` is live.
    let found = unsafe { hook(index, &raw mut display) };
    (found != 0).then_some(display)
}

/// The first display the hooks list with `monitor`'s number.
fn display_of(monitor: Hmonitor) -> Option<WsiDisplay> {
    let id = monitor_id(monitor);
    (0..u32::MAX)
        .map_while(display)
        .find(|display| display.monitor == id)
}

/// A monitor handle's number in the hooks.
fn monitor_id(monitor: Hmonitor) -> u64 {
    monitor as u64
}

/// The monitor handle for a hooks' monitor number; 0 is no monitor.
fn handle(id: u64) -> Option<Hmonitor> {
    let monitor: Hmonitor = core::ptr::without_provenance_mut(usize::try_from(id).unwrap_or(0));
    (!monitor.is_null()).then_some(monitor)
}

/// The `index`-th monitor, if there is one.
pub fn enum_monitors(index: u32) -> Option<Hmonitor> {
    display(index).and_then(|display| handle(display.monitor))
}

/// The primary monitor: the hooks' first display.
pub fn default_monitor() -> Option<Hmonitor> {
    enum_monitors(0)
}

/// `monitor`'s rectangle on the desktop.
pub fn desktop_coordinates(monitor: Hmonitor) -> Option<Rect> {
    display_of(monitor).map(|display| Rect {
        left: display.left,
        top: display.top,
        right: display.right,
        bottom: display.bottom,
    })
}

/// `monitor`'s name: the hooks' UTF-16 units, each a native `WCHAR`, the last one 0.
pub fn display_name(monitor: Hmonitor) -> Option<[Wchar; 32]> {
    display_of(monitor).map(|display| {
        let mut name = display.name.map(Wchar::from);
        name[31] = 0;
        name
    })
}

/// `monitor`'s mode `number`.
///
/// The hooks list the current mode first, then the modes by number, so mode `n` is the hooks'
/// `n + 1`; the current and the registry modes are both the first.
pub fn display_mode(monitor: Hmonitor, number: u32) -> Option<WsiMode> {
    let index = if number == ENUM_CURRENT_SETTINGS || number == ENUM_REGISTRY_SETTINGS {
        0
    } else {
        number.wrapping_add(1)
    };
    let hook = hooks()?.mode?;
    let mut mode = WsiHookMode::default();
    // SAFETY: the hook fills the mode it is given and answers 1 if it did; `mode` is live.
    let found = unsafe { hook(monitor_id(monitor), index, &raw mut mode) };
    (found != 0).then_some(WsiMode {
        width: mode.width,
        height: mode.height,
        refresh: Rational {
            numerator: mode.refresh_num,
            denominator: mode.refresh_den,
        },
        bits_per_pixel: 32,
        interlaced: false,
    })
}

/// `monitor`'s current mode.
pub fn current_display_mode(monitor: Hmonitor) -> Option<WsiMode> {
    display_mode(monitor, ENUM_CURRENT_SETTINGS)
}

/// The size of `window`'s client area; 1024 by 1024 for a window the hooks do not know.
///
/// Whatever a hook leaves in the structure counts, whether or not it answered 1.
pub fn window_size(window: Hwnd) -> (u32, u32) {
    let mut state = WsiWindow {
        width: DEFAULT_WINDOW_SIZE,
        height: DEFAULT_WINDOW_SIZE,
        ..WsiWindow::default()
    };
    if let Some(hook) = hooks().and_then(|hooks| hooks.window) {
        // SAFETY: the hook writes the window state it is given; `state` is live.
        unsafe { hook(window as u64, &raw mut state) };
    }
    (state.width, state.height)
}

#[cfg(test)]
mod tests;
