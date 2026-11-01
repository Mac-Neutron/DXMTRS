//! The native window system: winemetal's hooks (`WMTSetWsiHooks`).
//!
//! A VM host answers for its guest, from what the guest last told it, and sets its hooks before
//! any device. Without hooks (or with a hook missing) there is no monitor and no mode, and a
//! window is 1024 by 1024.

#[cfg(test)]
use core::cell::Cell;
use core::mem::offset_of;

use super::{
    ENUM_CURRENT_SETTINGS, ENUM_REGISTRY_SETTINGS, Hmonitor, Hwnd, Rational, Rect, Wchar,
    WindowState, WsiMode,
};

#[cfg(not(test))]
unsafe extern "C" {
    /// The hooks a VM host set, or null.
    fn WMTGetWsiHooks() -> *const WsiHooks;
}

/// The size of a window the hooks do not know.
const DEFAULT_WINDOW_SIZE: u32 = 1024;

pub const WMT_WSI_WIN_MINIMIZED: u32 = 1;
pub const WMT_WSI_WIN_FOCUSED: u32 = 2;
pub const WMT_WSI_WIN_KNOWN: u32 = 4;
pub const WMT_WSI_WIN_PROCESS_FG: u32 = 8;

pub const WMT_WSI_SET_MODE: u32 = 1;
pub const WMT_WSI_ENTER_FULLSCREEN: u32 = 2;
pub const WMT_WSI_LEAVE_FULLSCREEN: u32 = 3;
pub const WMT_WSI_RESTORE_DISPLAY: u32 = 4;
pub const WMT_WSI_MINIMIZE_WINDOW: u32 = 5;
pub const WMT_WSI_RESIZE_WINDOW: u32 = 6;
pub const WMT_WSI_UPDATE_FULLSCREEN: u32 = 7;
pub const WMT_WSI_SAVE_WINDOW: u32 = 8;

/// winemetal's `WMTWsiWindow`: a window's state as a VM host's guest last told it.
#[derive(Default, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct WsiWindow {
    pub monitor: u64,
    pub width: u32,
    pub height: u32,
    pub flags: u32,
}

/// winemetal's `WMTWsiDisplay`: a display, its rectangle and its UTF-16 name.
#[derive(Default)]
#[repr(C)]
pub struct WsiDisplay {
    pub monitor: u64,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub name: [u16; 32],
}

/// winemetal's `WMTWsiMode`: a display mode.
#[derive(Default)]
#[repr(C)]
pub struct WsiHookMode {
    pub width: u32,
    pub height: u32,
    pub refresh_num: u32,
    pub refresh_den: u32,
}

/// winemetal's `WMTWsiAction`: a window or display action sent back to the VM guest.
#[derive(Default, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct WsiAction {
    pub kind: u32,
    pub window: u64,
    pub monitor: u64,
    pub width: u32,
    pub height: u32,
    pub refresh_num: u32,
    pub refresh_den: u32,
    pub flags: u32,
}

/// winemetal's `WMTWsiHooks`: the window system a VM host answers for its guest.
///
/// Each query hook answers 1 with its out-structure filled, or 0. `action` forwards a
/// `WMTWsiAction` to the guest. A null hook is `None`.
#[repr(C)]
pub struct WsiHooks {
    pub window: Option<unsafe extern "C" fn(u64, *mut WsiWindow) -> i32>,
    pub display: Option<unsafe extern "C" fn(u32, *mut WsiDisplay) -> i32>,
    pub mode: Option<unsafe extern "C" fn(u64, u32, *mut WsiHookMode) -> i32>,
    pub action: Option<unsafe extern "C" fn(*const WsiAction)>,
}

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
pub fn set_hooks(hooks: Option<&'static WsiHooks>) {
    TEST_HOOKS.set(hooks.map_or(core::ptr::null(), core::ptr::from_ref));
}

/// The hooks table the host set, or null.
#[cfg(not(test))]
fn installed() -> *const WsiHooks {
    // SAFETY: winemetal's getter reads a pointer it keeps; it takes and touches nothing else.
    unsafe { WMTGetWsiHooks() }
}

/// The hooks table the calling test installed, or null.
#[cfg(test)]
fn installed() -> *const WsiHooks {
    TEST_HOOKS.get()
}

/// Whether a VM host installed a hooks table.
pub fn hooked() -> bool {
    !installed().is_null()
}

/// The hooks a VM host set, if it did.
fn hooks() -> Option<&'static WsiHooks> {
    // SAFETY: the host sets a table that stays valid for the process before any device exists, or
    // none; winemetal keeps only the pointer.
    unsafe { installed().as_ref() }
}

/// Sends `act` to the VM guest if an action hook is installed.
fn send_action(act: &WsiAction) {
    if let Some(action) = hooks().and_then(|hooks| hooks.action) {
        // SAFETY: `act` is a live `WsiAction` on the stack for the duration of the call.
        unsafe { action(core::ptr::from_ref(act)) };
    }
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

/// `window`'s monitor, or the primary monitor.
pub fn window_monitor(window: Hwnd) -> Hmonitor {
    if let Some(hook) = hooks().and_then(|hooks| hooks.window) {
        let mut win = WsiWindow::default();
        // SAFETY: the hook writes the window state it is given; `win` is live.
        let found = unsafe { hook(window as u64, &raw mut win) };
        if found != 0
            && let Some(mon) = handle(win.monitor)
        {
            return mon;
        }
    }
    default_monitor().unwrap_or(core::ptr::null_mut())
}

/// Whether `window` is a valid window.
pub fn is_window(window: Hwnd) -> bool {
    if window.is_null() {
        return false;
    }
    if let Some(hook) = hooks().and_then(|hooks| hooks.window) {
        let mut win = WsiWindow::default();
        // SAFETY: the hook writes the window state it is given; `win` is live.
        let found = unsafe { hook(window as u64, &raw mut win) };
        if found == 0 {
            return true;
        }
        return (win.flags & WMT_WSI_WIN_KNOWN) != 0;
    }
    true
}

/// Whether `window` is minimised.
pub fn is_minimized(window: Hwnd) -> bool {
    if let Some(hook) = hooks().and_then(|hooks| hooks.window) {
        let mut win = WsiWindow::default();
        // SAFETY: the hook writes the window state it is given; `win` is live.
        unsafe { hook(window as u64, &raw mut win) };
        return (win.flags & WMT_WSI_WIN_MINIMIZED) != 0;
    }
    false
}

/// Whether `window` is the foreground window.
pub fn is_foreground(window: Hwnd) -> bool {
    if let Some(hook) = hooks().and_then(|hooks| hooks.window) {
        let mut win = WsiWindow::default();
        // SAFETY: the hook writes the window state it is given; `win` is live.
        let found = unsafe { hook(window as u64, &raw mut win) };
        if found == 0 {
            return true;
        }
        return (win.flags & WMT_WSI_WIN_FOCUSED) != 0;
    }
    true
}

/// Whether the process owns the foreground window.
pub fn is_process_foreground() -> bool {
    if let Some(hook) = hooks().and_then(|hooks| hooks.window) {
        let mut win = WsiWindow::default();
        // SAFETY: the hook writes the window state it is given; `win` is live.
        let found = unsafe { hook(0, &raw mut win) };
        if found == 0 {
            return true;
        }
        return (win.flags & WMT_WSI_WIN_PROCESS_FG) != 0;
    }
    true
}

/// Minimises `window`.
pub fn minimize_window(window: Hwnd) {
    send_action(&WsiAction {
        kind: WMT_WSI_MINIMIZE_WINDOW,
        window: window as u64,
        ..WsiAction::default()
    });
}

/// Saves `window`'s windowed state.
pub fn save_window_state(window: Hwnd, _state: &mut WindowState) {
    send_action(&WsiAction {
        kind: WMT_WSI_SAVE_WINDOW,
        window: window as u64,
        ..WsiAction::default()
    });
}

/// Changes `monitor`'s mode for `window`.
pub fn set_window_mode(monitor: Hmonitor, window: Hwnd, mode: &WsiMode) -> bool {
    send_action(&WsiAction {
        kind: WMT_WSI_SET_MODE,
        window: window as u64,
        monitor: monitor_id(monitor),
        width: mode.width,
        height: mode.height,
        refresh_num: mode.refresh.numerator,
        refresh_den: mode.refresh.denominator,
        flags: u32::from(mode.interlaced),
    });
    true
}

/// Moves `window` to fullscreen on `monitor`.
pub fn enter_fullscreen_mode(
    monitor: Hmonitor,
    window: Hwnd,
    _state: &mut WindowState,
    mode_switch: bool,
) -> bool {
    send_action(&WsiAction {
        kind: WMT_WSI_ENTER_FULLSCREEN,
        window: window as u64,
        monitor: monitor_id(monitor),
        flags: u32::from(mode_switch),
        ..WsiAction::default()
    });
    true
}

/// Restores `window` from fullscreen.
pub fn leave_fullscreen_mode(
    window: Hwnd,
    _state: &WindowState,
    restore_coordinates: bool,
) -> bool {
    send_action(&WsiAction {
        kind: WMT_WSI_LEAVE_FULLSCREEN,
        window: window as u64,
        flags: u32::from(restore_coordinates),
        ..WsiAction::default()
    });
    true
}

/// Restores the desktop display mode.
pub fn restore_display_mode() -> bool {
    send_action(&WsiAction {
        kind: WMT_WSI_RESTORE_DISPLAY,
        ..WsiAction::default()
    });
    true
}

/// Resizes `window`'s client area to `width` by `height`.
pub fn resize_window(window: Hwnd, _state: &WindowState, width: u32, height: u32) {
    send_action(&WsiAction {
        kind: WMT_WSI_RESIZE_WINDOW,
        window: window as u64,
        width,
        height,
        ..WsiAction::default()
    });
}

/// Updates a fullscreen `window`'s bounds on `monitor`.
pub fn update_fullscreen_window(monitor: Hmonitor, window: Hwnd, force_topmost: bool) {
    send_action(&WsiAction {
        kind: WMT_WSI_UPDATE_FULLSCREEN,
        window: window as u64,
        monitor: monitor_id(monitor),
        flags: u32::from(force_topmost),
        ..WsiAction::default()
    });
}

const _: () = {
    assert!(size_of::<WsiAction>() == 48 && align_of::<WsiAction>() == 8);
    assert!(offset_of!(WsiAction, kind) == 0);
    assert!(offset_of!(WsiAction, window) == 8);
    assert!(offset_of!(WsiAction, monitor) == 16);
    assert!(offset_of!(WsiAction, width) == 24);
    assert!(offset_of!(WsiAction, height) == 28);
    assert!(offset_of!(WsiAction, refresh_num) == 32);
    assert!(offset_of!(WsiAction, refresh_den) == 36);
    assert!(offset_of!(WsiAction, flags) == 40);
    assert!(size_of::<WsiDisplay>() == 88 && align_of::<WsiDisplay>() == 8);
    assert!(offset_of!(WsiDisplay, monitor) == 0);
    assert!(offset_of!(WsiDisplay, left) == 8);
    assert!(offset_of!(WsiDisplay, top) == 12);
    assert!(offset_of!(WsiDisplay, right) == 16);
    assert!(offset_of!(WsiDisplay, bottom) == 20);
    assert!(offset_of!(WsiDisplay, name) == 24);
    assert!(size_of::<WsiHookMode>() == 16 && align_of::<WsiHookMode>() == 4);
    assert!(offset_of!(WsiHookMode, width) == 0);
    assert!(offset_of!(WsiHookMode, height) == 4);
    assert!(offset_of!(WsiHookMode, refresh_num) == 8);
    assert!(offset_of!(WsiHookMode, refresh_den) == 12);
    assert!(size_of::<WsiHooks>() == 32 && align_of::<WsiHooks>() == 8);
    assert!(offset_of!(WsiHooks, window) == 0);
    assert!(offset_of!(WsiHooks, display) == 8);
    assert!(offset_of!(WsiHooks, mode) == 16);
    assert!(offset_of!(WsiHooks, action) == 24);
    assert!(size_of::<WsiWindow>() == 24 && align_of::<WsiWindow>() == 8);
    assert!(offset_of!(WsiWindow, monitor) == 0);
    assert!(offset_of!(WsiWindow, width) == 8);
    assert!(offset_of!(WsiWindow, height) == 12);
    assert!(offset_of!(WsiWindow, flags) == 16);
};
