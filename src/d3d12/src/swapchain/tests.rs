use core::cell::Cell;

use dxmt_core::swapchain::vsync_duration;

use super::{leave_fullscreen_background, minimised_sleep};
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
fn vsync_duration_as_cpp() {
    assert!((vsync_duration(0, 0, 60.0) - 0.0).abs() < 1e-12);
    assert!((vsync_duration(1, 0, 60.0) - (1.0 / 60.0)).abs() < 1e-12);
    assert!((vsync_duration(2, 0, 60.0) - (2.0 / 60.0)).abs() < 1e-12);
    assert!((vsync_duration(0, 120, 60.0) - (1.0 / 120.0)).abs() < 1e-12);
    assert!((vsync_duration(1, 120, 60.0) - (1.0 / 120.0)).abs() < 1e-12);
    assert!((vsync_duration(2, 120, 60.0) - (2.0 / 120.0)).abs() < 1e-12);
    assert!((vsync_duration(1, 0, f64::MAX) - (1.0 / f64::MAX)).abs() < 1e-300);
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

#[test]
fn invalid_pixel_format_returns_invalid_arg() {
    use core::{ffi::c_void, ptr};

    use dxmt_util::{
        dxgi_consts::{
            DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT,
            DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_EFFECT_FLIP_DISCARD,
        },
        ext::{SampleDesc, SwapChainDesc1},
    };

    use super::{create, validate_create_desc};
    use crate::{com::E_INVALIDARG, device::Device, queue::Queue};

    // SAFETY: null `desc` is explicitly handled by `validate_create_desc`.
    let null_res = unsafe { validate_create_desc(ptr::null()) };
    assert_eq!(null_res.err(), Some(E_INVALIDARG));

    let make_desc = |format| SwapChainDesc1 {
        width: 64,
        height: 64,
        format,
        stereo: 0,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        buffer_usage: 0x20,
        buffer_count: 2,
        scaling: 0,
        swap_effect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        alpha_mode: 0,
        flags: 0,
    };

    let mut dev = Device::new_dummy();
    let queue = Queue::new_for_test(&raw mut *dev);

    for bad_format in [DXGI_FORMAT_UNKNOWN, DXGI_FORMAT_R32G32B32A32_FLOAT, 999] {
        let desc = make_desc(bad_format);
        // SAFETY: `desc` is a valid local `SwapChainDesc1`.
        let bad_res = unsafe { validate_create_desc((&raw const desc).cast()) };
        assert_eq!(bad_res.err(), Some(E_INVALIDARG));

        let mut out: *mut c_void = ptr::without_provenance_mut(0xDEAD);
        // SAFETY: `desc` is a valid local `SwapChainDesc1` and `out` is a writable local slot.
        let hr = unsafe {
            create(
                ptr::null_mut(),
                &dev,
                &queue,
                ptr::null_mut(),
                (&raw const desc).cast(),
                ptr::null(),
                &raw mut out,
            )
        };
        assert_eq!(hr, E_INVALIDARG);
        assert!(out.is_null());
    }

    for good_format in [
        DXGI_FORMAT_B8G8R8A8_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        DXGI_FORMAT_R10G10B10A2_UNORM,
        DXGI_FORMAT_R16G16B16A16_FLOAT,
    ] {
        let desc = make_desc(good_format);
        // SAFETY: `desc` is a valid local `SwapChainDesc1`.
        let good_res = unsafe { validate_create_desc((&raw const desc).cast()) };
        assert!(good_res.is_ok());
    }
}
