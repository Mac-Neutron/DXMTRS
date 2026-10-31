use core::ffi::c_void;

use super::*;

/// The fake host's monitors: the displays it lists, in order.
const FIRST: u64 = 0x10001;
const SECOND: u64 = 0x10002;
const WIDE_NAMED: u64 = 0x10003;

/// A monitor handle, as a test passes one.
fn monitor(id: u64) -> Hmonitor {
    core::ptr::without_provenance_mut(usize::try_from(id).unwrap_or(0))
}

/// `text` as a display's UTF-16 name, NUL-padded.
fn utf16(text: &str) -> [u16; 32] {
    let mut name = [0; 32];
    for (unit, ch) in name.iter_mut().zip(text.encode_utf16()) {
        *unit = ch;
    }
    name
}

/// `text` as the native `WCHAR` name, NUL-padded.
fn wide(text: &str) -> [Wchar; 32] {
    utf16(text).map(Wchar::from)
}

/// The fake host's display `index`: two ordinary ones and one whose name fills all 32 units.
extern "C" fn fake_display(index: u32, out: *mut WsiDisplay) -> i32 {
    let display = match index {
        0 => WsiDisplay {
            monitor: FIRST,
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
            name: utf16(r"\\.\DISPLAY1"),
        },
        1 => WsiDisplay {
            monitor: SECOND,
            left: 1920,
            top: 0,
            right: 4480,
            bottom: 1440,
            name: utf16(r"\\.\DISPLAY2"),
        },
        2 => WsiDisplay {
            monitor: WIDE_NAMED,
            left: -10,
            top: -20,
            right: 30,
            bottom: 40,
            name: [0xffff; 32],
        },
        _ => return 0,
    };
    // SAFETY: the caller's `out` is a live `WsiDisplay`.
    unsafe { out.write(display) };
    1
}

/// The fake host's modes: the first display's three (index 0 the current one), the second's two.
extern "C" fn fake_mode(monitor: u64, index: u32, out: *mut WsiHookMode) -> i32 {
    let list: &[(u32, u32, u32, u32)] = match monitor {
        FIRST => &[(1920, 1080, 60, 1), (1280, 720, 60, 1), (1024, 768, 75, 1)],
        SECOND => &[(2560, 1440, 144, 1), (1600, 900, 59940, 1000)],
        _ => return 0,
    };
    let Some(&(width, height, refresh_num, refresh_den)) = usize::try_from(index)
        .ok()
        .and_then(|index| list.get(index))
    else {
        return 0;
    };
    // SAFETY: the caller's `out` is a live `WsiHookMode`.
    unsafe {
        out.write(WsiHookMode {
            width,
            height,
            refresh_num,
            refresh_den,
        });
    }
    1
}

/// The fake host's window: 800 by 600.
extern "C" fn fake_window(_: u64, out: *mut WsiWindow) -> i32 {
    // SAFETY: the caller's `out` is a live `WsiWindow`.
    unsafe {
        out.write(WsiWindow {
            monitor: FIRST,
            width: 800,
            height: 600,
            flags: 4,
        });
    }
    1
}

/// A window hook that knows no window and writes nothing.
extern "C" fn unknown_window(_: u64, _: *mut WsiWindow) -> i32 {
    0
}

/// A window hook that writes a size and then says it knows no window.
extern "C" fn half_window(_: u64, out: *mut WsiWindow) -> i32 {
    // SAFETY: the caller's `out` is a live `WsiWindow`.
    unsafe {
        out.write(WsiWindow {
            monitor: 0,
            width: 640,
            height: 480,
            flags: 0,
        });
    }
    0
}

/// A display hook whose one display has monitor 0.
extern "C" fn null_display(index: u32, out: *mut WsiDisplay) -> i32 {
    if index != 0 {
        return 0;
    }
    // SAFETY: the caller's `out` is a live `WsiDisplay`.
    unsafe {
        out.write(WsiDisplay {
            right: 640,
            bottom: 480,
            ..WsiDisplay::default()
        });
    }
    1
}

extern "C" fn no_action(_: *const c_void) {}

static FAKE: WsiHooks = WsiHooks {
    window: Some(fake_window),
    display: Some(fake_display),
    mode: Some(fake_mode),
    action: Some(no_action),
};

/// Hooks with every function missing.
static EMPTY: WsiHooks = WsiHooks {
    window: None,
    display: None,
    mode: None,
    action: None,
};

static UNKNOWN_WINDOW: WsiHooks = WsiHooks {
    window: Some(unknown_window),
    ..EMPTY
};

static HALF_WINDOW: WsiHooks = WsiHooks {
    window: Some(half_window),
    ..EMPTY
};

static NULL_MONITOR: WsiHooks = WsiHooks {
    display: Some(null_display),
    ..EMPTY
};

/// `mode`'s size and refresh as a tuple.
fn shape(mode: &WsiMode) -> (u32, u32, u32, u32) {
    (
        mode.width,
        mode.height,
        mode.refresh.numerator,
        mode.refresh.denominator,
    )
}

#[test]
fn no_hooks_no_monitors() {
    for hooks in [None, Some(&EMPTY)] {
        set_hooks(hooks);
        assert!(enum_monitors(0).is_none());
        assert!(default_monitor().is_none());
        assert!(desktop_coordinates(monitor(FIRST)).is_none());
        assert!(display_name(monitor(FIRST)).is_none());
        assert!(display_mode(monitor(FIRST), 0).is_none());
        assert!(current_display_mode(monitor(FIRST)).is_none());
    }
}

#[test]
fn displays_are_found_by_index_and_monitor() {
    set_hooks(Some(&FAKE));
    assert_eq!(enum_monitors(0), Some(monitor(FIRST)));
    assert_eq!(enum_monitors(1), Some(monitor(SECOND)));
    assert_eq!(enum_monitors(2), Some(monitor(WIDE_NAMED)));
    assert!(enum_monitors(3).is_none());
    assert_eq!(default_monitor(), Some(monitor(FIRST)));
    let rect = desktop_coordinates(monitor(SECOND)).expect("the second display");
    assert_eq!(
        (rect.left, rect.top, rect.right, rect.bottom),
        (1920, 0, 4480, 1440)
    );
    let rect = desktop_coordinates(monitor(WIDE_NAMED)).expect("the third display");
    assert_eq!(
        (rect.left, rect.top, rect.right, rect.bottom),
        (-10, -20, 30, 40)
    );
    assert!(desktop_coordinates(monitor(0x20000)).is_none());
    assert!(display_name(monitor(0x20000)).is_none());
}

#[test]
fn a_null_monitor_is_none() {
    set_hooks(Some(&NULL_MONITOR));
    assert!(enum_monitors(0).is_none());
    assert!(default_monitor().is_none());
    // It is still a display a monitor of 0 finds, as the C++ matches monitor numbers.
    let rect = desktop_coordinates(core::ptr::null_mut()).expect("the display of monitor 0");
    assert_eq!((rect.right, rect.bottom), (640, 480));
}

#[test]
fn name_widens_to_u32() {
    set_hooks(Some(&FAKE));
    assert_eq!(
        display_name(monitor(FIRST)),
        Some(wide(r"\\.\DISPLAY1")),
        "UTF-16 units widen to the native WCHAR"
    );
    assert_eq!(display_name(monitor(SECOND)), Some(wide(r"\\.\DISPLAY2")));
    let mut cut = [0xffff_u16; 32];
    cut[31] = 0;
    assert_eq!(
        display_name(monitor(WIDE_NAMED)),
        Some(cut.map(Wchar::from)),
        "zero-extended, never sign-extended, and the last unit is 0"
    );
}

#[test]
fn mode_zero_is_current() {
    set_hooks(Some(&FAKE));
    let current = current_display_mode(monitor(SECOND)).expect("a current mode");
    assert_eq!(shape(&current), (2560, 1440, 144, 1));
    assert_eq!(current.bits_per_pixel, 32);
    assert!(!current.interlaced);
    for number in [ENUM_CURRENT_SETTINGS, ENUM_REGISTRY_SETTINGS] {
        let mode = display_mode(monitor(SECOND), number).expect("hook mode 0");
        assert_eq!(shape(&mode), (2560, 1440, 144, 1));
        assert_eq!((mode.bits_per_pixel, mode.interlaced), (32, false));
    }
}

#[test]
fn list_index_shifts_by_one() {
    set_hooks(Some(&FAKE));
    let first = display_mode(monitor(FIRST), 0).expect("hook mode 1");
    assert_eq!(shape(&first), (1280, 720, 60, 1));
    let last = display_mode(monitor(FIRST), 1).expect("hook mode 2");
    assert_eq!(shape(&last), (1024, 768, 75, 1));
    assert!(display_mode(monitor(FIRST), 2).is_none());
    assert!(display_mode(monitor(FIRST), u32::MAX - 2).is_none());
    // The refresh stays the hook's numerator and denominator.
    let rate = display_mode(monitor(SECOND), 0).expect("hook mode 1");
    assert_eq!(shape(&rate), (1600, 900, 59940, 1000));
    assert!(display_mode(monitor(0x20000), 0).is_none());
}

#[test]
fn window_falls_back_to_1024() {
    let window = core::ptr::without_provenance_mut(0x1234);
    set_hooks(None);
    assert_eq!(window_size(window), (1024, 1024));
    set_hooks(Some(&EMPTY));
    assert_eq!(window_size(window), (1024, 1024));
    set_hooks(Some(&UNKNOWN_WINDOW));
    assert_eq!(window_size(window), (1024, 1024));
    set_hooks(Some(&FAKE));
    assert_eq!(window_size(window), (800, 600));
}

#[test]
fn a_window_hook_that_wrote_before_saying_no_still_counts() {
    // The C++ reads the structure whatever the hook answered.
    set_hooks(Some(&HALF_WINDOW));
    assert_eq!(window_size(core::ptr::null_mut()), (640, 480));
}
