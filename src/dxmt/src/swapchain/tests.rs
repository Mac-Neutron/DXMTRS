use core::{cell::Cell, ffi::c_void, ptr};

use dxmt_util::{
    com::hresult::{E_INVALIDARG, E_POINTER, S_OK},
    dxgi_consts::DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
    ext::{
        FrameStatistics, Rational, SampleDesc, SwapChainDesc, SwapChainDesc1,
        SwapChainFullscreenDesc,
    },
    wsi::WindowState,
};
use winemetal_wire::{
    WMT_COLOR_SPACE_BT2020, WMT_COLOR_SPACE_HDR_PQ, WMT_COLOR_SPACE_HDR_SC_RGB,
    WMT_COLOR_SPACE_INVALID, WMT_COLOR_SPACE_SRGB, WMT_COLOR_SPACE_SRGB_LINEAR,
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB, WMT_PIXEL_FORMAT_INVALID,
    WMT_PIXEL_FORMAT_RGB10A2_UNORM, WMT_PIXEL_FORMAT_RGBA16_FLOAT,
};

use super::{
    Core, DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_B8G8R8X8_UNORM,
    DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
    DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM, DXGI_FORMAT_R10G10B10A2_UNORM,
    DXGI_FORMAT_R16G16B16A16_FLOAT, State, convert_color_space, convert_format, mode_refresh_rate,
    monitor_format_bpp, monitor_format_bpp_quiet, parse_render_scale, render_scale, vsync_duration,
};

#[test]
fn formats_as_cpp() {
    assert_eq!(
        convert_format(DXGI_FORMAT_B8G8R8A8_UNORM_SRGB),
        WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB
    );
    assert_eq!(
        convert_format(DXGI_FORMAT_R8G8B8A8_UNORM_SRGB),
        WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB
    );
    assert_eq!(
        convert_format(DXGI_FORMAT_B8G8R8A8_UNORM),
        WMT_PIXEL_FORMAT_BGRA8_UNORM
    );
    assert_eq!(
        convert_format(DXGI_FORMAT_R8G8B8A8_UNORM),
        WMT_PIXEL_FORMAT_BGRA8_UNORM
    );
    assert_eq!(
        convert_format(DXGI_FORMAT_R10G10B10A2_UNORM),
        WMT_PIXEL_FORMAT_RGB10A2_UNORM
    );
    assert_eq!(
        convert_format(DXGI_FORMAT_R16G16B16A16_FLOAT),
        WMT_PIXEL_FORMAT_RGBA16_FLOAT
    );
    assert_eq!(convert_format(0), WMT_PIXEL_FORMAT_INVALID);
    assert_eq!(
        convert_format(DXGI_FORMAT_B8G8R8X8_UNORM),
        WMT_PIXEL_FORMAT_INVALID
    );

    for fmt in [
        DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_B8G8R8A8_UNORM,
        DXGI_FORMAT_B8G8R8X8_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        DXGI_FORMAT_B8G8R8X8_UNORM_SRGB,
        DXGI_FORMAT_R10G10B10A2_UNORM,
        DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM,
    ] {
        assert_eq!(monitor_format_bpp(fmt), 32);
    }
    assert_eq!(monitor_format_bpp(DXGI_FORMAT_R16G16B16A16_FLOAT), 64);
    assert_eq!(monitor_format_bpp(0), 32);
}

#[test]
fn color_spaces_as_cpp() {
    assert_eq!(
        convert_color_space(DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, false),
        WMT_COLOR_SPACE_SRGB
    );
    assert_eq!(
        convert_color_space(DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, true),
        WMT_COLOR_SPACE_SRGB
    );
    assert_eq!(
        convert_color_space(DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, false),
        WMT_COLOR_SPACE_SRGB_LINEAR
    );
    assert_eq!(
        convert_color_space(DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, true),
        WMT_COLOR_SPACE_HDR_SC_RGB
    );
    assert_eq!(
        convert_color_space(DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, false),
        WMT_COLOR_SPACE_HDR_PQ
    );
    assert_eq!(
        convert_color_space(DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, true),
        WMT_COLOR_SPACE_HDR_PQ
    );
    assert_eq!(
        convert_color_space(DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020, false),
        WMT_COLOR_SPACE_BT2020
    );
    assert_eq!(convert_color_space(99, false), WMT_COLOR_SPACE_INVALID);
    assert_eq!(convert_color_space(99, true), WMT_COLOR_SPACE_INVALID);
}

#[test]
fn render_scale_parses_as_cpp() {
    for unscaled in ["", "0", "-1", "nope", "1"] {
        assert_eq!(parse_render_scale(unscaled), 1.0, "input {unscaled:?}");
    }
    assert_eq!(parse_render_scale("0.5"), 0.5);
    assert_eq!(parse_render_scale("0.1"), 0.25);
    assert_eq!(parse_render_scale("2.0"), 1.0);
    assert_eq!(render_scale(), 1.0);
}

/// The interval d3d11 and d3d12 each computed: refreshes over the rate, floored by the preferred.
#[test]
fn vsync_duration_as_cpp() {
    assert!((vsync_duration(0, 0, 60.0) - 0.0).abs() < 1e-12);
    assert!((vsync_duration(1, 0, 60.0) - (1.0 / 60.0)).abs() < 1e-12);
    assert!((vsync_duration(2, 0, 60.0) - (2.0 / 60.0)).abs() < 1e-12);
    assert!((vsync_duration(0, 120, 60.0) - (1.0 / 120.0)).abs() < 1e-12);
    assert!((vsync_duration(1, 30, 60.0) - (1.0 / 30.0)).abs() < 1e-12);
}

fn core(flags: u32) -> Core<()> {
    let desc = SwapChainDesc1 {
        width: 640,
        height: 480,
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        stereo: 0,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        buffer_usage: 0x20,
        buffer_count: 2,
        scaling: 0,
        swap_effect: 3,
        alpha_mode: 0,
        flags,
    };
    let state = State {
        desc,
        fullscreen_desc: SwapChainFullscreenDesc {
            refresh_rate: Rational {
                numerator: 60,
                denominator: 1,
            },
            scanline_ordering: 0,
            scaling: 0,
            windowed: 1,
        },
        monitor: ptr::null_mut(),
        target: ptr::null_mut(),
        window_state: WindowState::default(),
        presentation_count: 7,
        frame_latency: 1,
        color_space: 0,
        init_refresh_rate: f64::MAX,
        api: (),
    };
    Core::new(ptr::without_provenance_mut(0x1234), 0, None, state)
}

#[test]
fn getters_answer_from_the_state() {
    let core = core(0);
    // SAFETY: null pointers, each.
    let nulls = [
        unsafe { core.get_desc(ptr::null_mut()) },
        // SAFETY: as above.
        unsafe { core.get_desc1(ptr::null_mut()) },
        // SAFETY: as above.
        unsafe { core.get_fullscreen_desc(ptr::null_mut()) },
        // SAFETY: as above.
        unsafe { core.get_hwnd(ptr::null_mut()) },
        // SAFETY: as above.
        unsafe { core.get_last_present_count(ptr::null_mut()) },
        // SAFETY: as above.
        unsafe { core.get_maximum_frame_latency(ptr::null_mut()) },
        // SAFETY: as above.
        unsafe { core.get_fullscreen_state(ptr::null_mut(), ptr::null_mut()) },
    ];
    assert_eq!(
        nulls,
        [
            E_INVALIDARG,
            E_POINTER,
            E_POINTER,
            E_POINTER,
            E_POINTER,
            S_OK,
            S_OK
        ]
    );

    let mut desc = core::mem::MaybeUninit::<SwapChainDesc>::uninit();
    // SAFETY: a local.
    let hr = unsafe { core.get_desc(desc.as_mut_ptr().cast()) };
    assert_eq!(hr, S_OK);
    // SAFETY: written above.
    let desc = unsafe { desc.assume_init() };
    assert_eq!(
        (desc.buffer_desc.width, desc.buffer_desc.height),
        (640, 480)
    );
    assert_eq!(desc.buffer_desc.refresh_rate.numerator, 60);
    assert_eq!((desc.windowed, desc.output_window), (1, core.hwnd));

    let mut count = 0;
    // SAFETY: a local.
    let hr = unsafe { core.get_last_present_count(&raw mut count) };
    assert_eq!(hr, S_OK);
    assert_eq!(count, 7);
    let mut stats = FrameStatistics::default();
    // SAFETY: a local.
    unsafe { core.write_frame_statistics((&raw mut stats).cast::<c_void>()) };
    assert_eq!((stats.present_count, stats.sync_refresh_count), (7, 7));

    let mut fullscreen = -1;
    let mut target = ptr::without_provenance_mut(1);
    // SAFETY: locals.
    let hr = unsafe { core.get_fullscreen_state(&raw mut fullscreen, &raw mut target) };
    assert_eq!(hr, S_OK);
    assert_eq!((fullscreen, target), (0, ptr::null_mut()));
}

#[test]
fn frame_latency_and_its_waitable_object() {
    let core = core(0);
    let released = Cell::new(None);
    assert_eq!(
        core.set_maximum_frame_latency(0, |n| released.set(Some(n))),
        E_INVALIDARG
    );
    assert_eq!(
        core.set_maximum_frame_latency(17, |n| released.set(Some(n))),
        E_INVALIDARG
    );
    assert_eq!(
        core.set_maximum_frame_latency(3, |n| released.set(Some(n))),
        S_OK
    );
    assert_eq!(released.take(), Some(2), "a raise releases the difference");
    assert_eq!(
        core.set_maximum_frame_latency(2, |n| released.set(Some(n))),
        S_OK
    );
    assert_eq!(released.get(), None, "a lowering releases nothing");
    let mut latency = 0;
    // SAFETY: a local.
    let hr = unsafe { core.get_maximum_frame_latency(&raw mut latency) };
    assert_eq!(hr, S_OK);
    assert_eq!(latency, 2);

    let handle = ptr::without_provenance_mut(9);
    assert!(core.frame_latency_waitable_object(|| handle).is_null());
    let waitable = self::core(DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT);
    assert_eq!(waitable.frame_latency_waitable_object(|| handle), handle);

    assert!(!core.set_color_space1(12), "no presenter: no layer change");
    assert_eq!(core.lock().color_space, 12);
}

#[test]
fn quiet_bpp_and_refresh_rates() {
    assert_eq!(monitor_format_bpp_quiet(DXGI_FORMAT_R16G16B16A16_FLOAT), 64);
    assert_eq!(monitor_format_bpp_quiet(12_345), 32);
    let mode = |numerator, denominator| dxmt_util::wsi::WsiMode {
        refresh: Rational {
            numerator,
            denominator,
        },
        ..dxmt_util::wsi::WsiMode::default()
    };
    assert_eq!(mode_refresh_rate(Some(mode(120_000, 1000))), Some(120.0));
    assert_eq!(mode_refresh_rate(Some(mode(0, 1))), None);
    assert_eq!(mode_refresh_rate(Some(mode(60, 0))), None);
    assert_eq!(mode_refresh_rate(None), None);
}
