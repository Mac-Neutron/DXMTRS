use core::cell::Cell;

use super::{leave_fullscreen_background, max_frame_rate_interval, minimised_sleep};
use crate::abi::swapchain::Hwnd;

#[test]
fn minimised_skip_sleeps_without_hooks() {
    let slept_without_hooks = Cell::new(None);
    minimised_sleep(true, |ms| slept_without_hooks.set(Some(ms)));
    assert_eq!(slept_without_hooks.get(), Some(16));

    let slept_with_hooks = Cell::new(None);
    minimised_sleep(false, |ms| slept_with_hooks.set(Some(ms)));
    assert_eq!(slept_with_hooks.get(), None);
}

#[test]
fn max_frame_rate_interval_as_cpp() {
    assert!((max_frame_rate_interval(0, 0, 60.0) - 0.0).abs() < 1e-12);
    assert!((max_frame_rate_interval(1, 0, 60.0) - (1.0 / 60.0)).abs() < 1e-12);
    assert!((max_frame_rate_interval(2, 0, 60.0) - (2.0 / 60.0)).abs() < 1e-12);
    assert!((max_frame_rate_interval(0, 120, 60.0) - (1.0 / 120.0)).abs() < 1e-12);
    assert!((max_frame_rate_interval(1, 120, 60.0) - (1.0 / 120.0)).abs() < 1e-12);
    assert!((max_frame_rate_interval(2, 120, 60.0) - (2.0 / 120.0)).abs() < 1e-12);
    assert!((max_frame_rate_interval(1, 0, f64::MAX) - (1.0 / f64::MAX)).abs() < 1e-300);
}

#[test]
fn background_leave_minimises() {
    let fake_hwnd: Hwnd = core::ptr::without_provenance_mut(0x1234);

    let logged = Cell::new(None);
    let minimised = Cell::new(None);
    leave_fullscreen_background(
        fake_hwnd,
        false,
        || false,
        |msg| logged.set(Some(msg)),
        |w| minimised.set(Some(w)),
    );
    assert_eq!(
        logged.get(),
        Some("Leaving fullscreen in the background: minimising")
    );
    assert_eq!(minimised.get(), Some(fake_hwnd));

    let logged_switching = Cell::new(None);
    let minimised_switching = Cell::new(None);
    leave_fullscreen_background(
        fake_hwnd,
        true,
        || false,
        |msg| logged_switching.set(Some(msg)),
        |w| minimised_switching.set(Some(w)),
    );
    assert_eq!(logged_switching.get(), None);
    assert_eq!(minimised_switching.get(), None);

    let logged_fg = Cell::new(None);
    let minimised_fg = Cell::new(None);
    leave_fullscreen_background(
        fake_hwnd,
        false,
        || true,
        |msg| logged_fg.set(Some(msg)),
        |w| minimised_fg.set(Some(w)),
    );
    assert_eq!(logged_fg.get(), None);
    assert_eq!(minimised_fg.get(), None);
}
