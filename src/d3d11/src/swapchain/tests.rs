use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use std::{sync::Arc, thread, time::Duration};

use dxmt_core::arena::{BlockPool, Commands};
use dxmt_util::sync::CpuFence;
use winemetal_sys::wire::{
    WMT_COLOR_SPACE_BT2020, WMT_COLOR_SPACE_HDR_PQ, WMT_COLOR_SPACE_HDR_SC_RGB,
    WMT_COLOR_SPACE_INVALID, WMT_COLOR_SPACE_SRGB, WMT_COLOR_SPACE_SRGB_LINEAR,
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB, WMT_PIXEL_FORMAT_INVALID,
    WMT_PIXEL_FORMAT_RGB10A2_UNORM, WMT_PIXEL_FORMAT_RGBA16_FLOAT,
};

use super::{
    DEFAULT_SWAPCHAIN_LATENCY, DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709,
    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020,
    DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, DXGI_ERROR_INVALID_CALL,
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT,
    DXGI_MAX_SWAP_CHAIN_BUFFERS, DXGI_MODE_ROTATION_IDENTITY, DXGI_PRESENT_TEST,
    DXGI_STATUS_OCCLUDED, DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
    DXGI_SWAP_EFFECT_DISCARD, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
    DXGI_SWAP_EFFECT_SEQUENTIAL, SwapChain, compute_vsync_duration, convert_color_space,
    convert_swapchain_format, create, evaluate_present_prelude, sync_frame,
};
use crate::{
    abi::{
        IID_IDXGI_SWAP_CHAIN_4,
        records::{
            DxgiFrameStatistics, DxgiRational, DxgiRgba, DxgiSampleDesc, DxgiSwapChainDesc,
            DxgiSwapChainDesc1,
        },
        vtbl::{IDXGISwapChain4Vtbl, IUnknownVtbl},
    },
    com::{E_INVALIDARG, Hresult, S_OK, vtbl},
};

unsafe fn release_ref(ptr: *mut c_void) -> u32 {
    // SAFETY: `ptr` is a live COM interface pointer.
    let u_vtbl = unsafe { vtbl::<IUnknownVtbl>(ptr) };
    // SAFETY: `ptr` is valid.
    unsafe { (u_vtbl.release)(ptr) }
}

const GOLDEN: &str = include_str!("golden/d11sc.txt");

fn golden_line(prefix: &str) -> &'static str {
    for line in GOLDEN.lines() {
        if line.starts_with(prefix) {
            return line;
        }
    }
    panic!("missing golden line starting with `{prefix}` in d11sc.txt");
}

fn parse_line_hex(line: &str, key: &str) -> Hresult {
    for token in line.split_whitespace() {
        if let Some(rest) = token.strip_prefix(key)
            && let Some(val_str) = rest.strip_prefix('=')
        {
            let hex = val_str
                .split('(')
                .next()
                .unwrap_or(val_str)
                .trim()
                .trim_start_matches("0x");
            let raw = u32::from_str_radix(hex, 16).expect("valid hex in d11sc.txt");
            return raw.cast_signed();
        }
    }
    panic!("missing golden key `{key}` in line `{line}`");
}

fn make_swapchain(width: u32, height: u32, swap_effect: u32, flags: u32) -> *mut c_void {
    let desc = DxgiSwapChainDesc1 {
        width,
        height,
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        stereo: 0,
        sample_desc: DxgiSampleDesc {
            count: 1,
            quality: 0,
        },
        buffer_usage: 0x20,
        buffer_count: 2,
        scaling: 0,
        swap_effect,
        alpha_mode: 0,
        flags,
    };
    let mut out = ptr::null_mut();
    let hwnd = ptr::without_provenance_mut(0x1234);
    assert_eq!(
        create(
            ptr::null_mut(),
            ptr::null_mut(),
            hwnd,
            &raw const desc,
            ptr::null(),
            ptr::null_mut(),
            &raw mut out,
        ),
        S_OK
    );
    assert!(!out.is_null());
    out
}

#[test]
fn occluded_when_minimised_or_zero_size() {
    let hidden_line = golden_line("d11sc hidden ");
    let iconic_line = golden_line("d11sc iconic=1 ");

    let expected_normal = parse_line_hex(hidden_line, "p0");
    let expected_minimized_discard = parse_line_hex(iconic_line, "disc_p0");
    let expected_minimized_flip = parse_line_hex(iconic_line, "flip_p0");
    let expected_zero_rsz = parse_line_hex(iconic_line, "zero_rsz");
    let expected_zero_p0 = parse_line_hex(iconic_line, "zero_p0");

    assert_eq!(expected_normal, S_OK);
    assert_eq!(expected_minimized_discard, DXGI_STATUS_OCCLUDED);
    assert_eq!(expected_minimized_flip, S_OK);
    assert_eq!(expected_zero_rsz, S_OK);
    assert_eq!(expected_zero_p0, DXGI_STATUS_OCCLUDED);

    // Minimized window: DISCARD and SEQUENTIAL return DXGI_STATUS_OCCLUDED; FLIP_* return S_OK.
    assert_eq!(
        evaluate_present_prelude(
            0,
            0,
            true,
            (1280, 720),
            DXGI_SWAP_EFFECT_DISCARD,
            (false, 1, true),
        )
        .0,
        expected_minimized_discard
    );
    assert_eq!(
        evaluate_present_prelude(
            0,
            0,
            true,
            (1280, 720),
            DXGI_SWAP_EFFECT_SEQUENTIAL,
            (false, 1, true),
        )
        .0,
        expected_minimized_discard
    );
    assert_eq!(
        evaluate_present_prelude(
            0,
            0,
            true,
            (1280, 720),
            DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            (false, 1, true),
        )
        .0,
        expected_minimized_flip
    );
    assert_eq!(
        evaluate_present_prelude(
            0,
            0,
            true,
            (1280, 720),
            DXGI_SWAP_EFFECT_FLIP_DISCARD,
            (false, 1, true),
        )
        .0,
        expected_minimized_flip
    );

    // Zero width or height: DISCARD and SEQUENTIAL return DXGI_STATUS_OCCLUDED; FLIP_* return S_OK.
    assert_eq!(
        evaluate_present_prelude(
            0,
            0,
            false,
            (0, 720),
            DXGI_SWAP_EFFECT_DISCARD,
            (false, 1, true),
        )
        .0,
        expected_zero_p0
    );
    assert_eq!(
        evaluate_present_prelude(
            0,
            0,
            false,
            (1280, 0),
            DXGI_SWAP_EFFECT_SEQUENTIAL,
            (false, 1, true),
        )
        .0,
        expected_zero_p0
    );
    assert_eq!(
        evaluate_present_prelude(
            0,
            0,
            false,
            (0, 720),
            DXGI_SWAP_EFFECT_FLIP_DISCARD,
            (false, 1, true),
        )
        .0,
        expected_minimized_flip
    );
    assert_eq!(
        evaluate_present_prelude(
            0,
            0,
            false,
            (1280, 0),
            DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            (false, 1, true),
        )
        .0,
        expected_minimized_flip
    );

    // Normal non-minimized non-zero window: S_OK for all swap effects.
    assert_eq!(
        evaluate_present_prelude(
            0,
            0,
            false,
            (1280, 720),
            DXGI_SWAP_EFFECT_DISCARD,
            (false, 1, true),
        )
        .0,
        expected_normal
    );
    assert_eq!(
        evaluate_present_prelude(
            0,
            0,
            false,
            (1280, 720),
            DXGI_SWAP_EFFECT_FLIP_DISCARD,
            (false, 1, true),
        )
        .0,
        expected_normal
    );

    // Also verify via the COM Present and ResizeBuffers vtable on a SwapChain instance.
    let raw_discard = make_swapchain(64, 64, DXGI_SWAP_EFFECT_DISCARD, 0);
    // SAFETY: `raw_discard` is a live `IDXGISwapChain4`.
    let sc_vtbl = unsafe { vtbl::<IDXGISwapChain4Vtbl>(raw_discard) };
    assert_eq!(
        // SAFETY: `raw_discard` is valid.
        unsafe { (sc_vtbl.resize_buffers)(raw_discard, 0, 0, 0, DXGI_FORMAT_B8G8R8A8_UNORM, 0) },
        expected_zero_rsz
    );
    assert_eq!(
        // SAFETY: `raw_discard` is valid.
        unsafe { (sc_vtbl.present)(raw_discard, 0, 0) },
        expected_zero_p0
    );
    // SAFETY: `raw_discard` has refcount 1.
    assert_eq!(unsafe { release_ref(raw_discard) }, 0);

    let raw_flip = make_swapchain(0, 0, DXGI_SWAP_EFFECT_FLIP_DISCARD, 0);
    // SAFETY: `raw_flip` is a live `IDXGISwapChain4`.
    let flip_vtbl = unsafe { vtbl::<IDXGISwapChain4Vtbl>(raw_flip) };
    assert_eq!(
        // SAFETY: `raw_flip` is valid.
        unsafe { (flip_vtbl.present)(raw_flip, 0, 0) },
        expected_minimized_flip
    );
    // SAFETY: `raw_flip` has refcount 1.
    assert_eq!(unsafe { release_ref(raw_flip) }, 0);
}

#[test]
fn test_flag_returns_before_present() {
    let hidden_line = golden_line("d11sc hidden ");
    let iconic_line = golden_line("d11sc iconic=1 ");

    let expected_test_ready = parse_line_hex(hidden_line, "ptest");
    let expected_p0_ready = parse_line_hex(hidden_line, "p0");
    let expected_disc_test = parse_line_hex(iconic_line, "disc_test");
    let expected_zero_test = parse_line_hex(iconic_line, "zero_test");

    assert_eq!(expected_test_ready, S_OK);
    assert_eq!(expected_p0_ready, S_OK);
    assert_eq!(expected_disc_test, DXGI_STATUS_OCCLUDED);
    assert_eq!(expected_zero_test, DXGI_STATUS_OCCLUDED);

    // Occluded swapchain with DXGI_PRESENT_TEST returns DXGI_STATUS_OCCLUDED without incrementing present count.
    let raw_occ = make_swapchain(0, 0, DXGI_SWAP_EFFECT_DISCARD, 0);
    // SAFETY: `raw_occ` is a live `IDXGISwapChain4`.
    let occ_vtbl = unsafe { vtbl::<IDXGISwapChain4Vtbl>(raw_occ) };
    assert_eq!(
        // SAFETY: `raw_occ` is valid.
        unsafe { (occ_vtbl.present)(raw_occ, 0, DXGI_PRESENT_TEST) },
        expected_zero_test
    );
    let mut count = 99_u32;
    assert_eq!(
        // SAFETY: `raw_occ` and `count` are valid.
        unsafe { (occ_vtbl.get_last_present_count)(raw_occ, &raw mut count) },
        S_OK
    );
    assert_eq!(count, 0);
    // SAFETY: `raw_occ` has refcount 1.
    assert_eq!(unsafe { release_ref(raw_occ) }, 0);

    // Ready swapchain with DXGI_PRESENT_TEST returns S_OK without incrementing present count.
    let raw_ok = make_swapchain(1280, 720, DXGI_SWAP_EFFECT_FLIP_DISCARD, 0);
    // SAFETY: `raw_ok` is a live `IDXGISwapChain4`.
    let ok_vtbl = unsafe { vtbl::<IDXGISwapChain4Vtbl>(raw_ok) };
    assert_eq!(
        // SAFETY: `raw_ok` is valid.
        unsafe { (ok_vtbl.present)(raw_ok, 0, DXGI_PRESENT_TEST) },
        expected_test_ready
    );
    assert_eq!(
        // SAFETY: `raw_ok` and `count` are valid.
        unsafe { (ok_vtbl.get_last_present_count)(raw_ok, &raw mut count) },
        S_OK
    );
    assert_eq!(count, 0);

    // Normal Present (without DXGI_PRESENT_TEST) increments presentation_count.
    assert_eq!(
        // SAFETY: `raw_ok` is valid.
        unsafe { (ok_vtbl.present)(raw_ok, 0, 0) },
        expected_p0_ready
    );
    assert_eq!(
        // SAFETY: `raw_ok` and `count` are valid.
        unsafe { (ok_vtbl.get_last_present_count)(raw_ok, &raw mut count) },
        S_OK
    );
    assert_eq!(count, 1);
    // SAFETY: `raw_ok` has refcount 1.
    assert_eq!(unsafe { release_ref(raw_ok) }, 0);
}

#[test]
fn sync_interval_over_4_is_invalid() {
    let hidden_line = golden_line("d11sc hidden ");
    let iconic_line = golden_line("d11sc iconic=1 ");

    let expected_p0 = parse_line_hex(hidden_line, "p0");
    let expected_p5 = parse_line_hex(hidden_line, "p5");
    let expected_disc_p5 = parse_line_hex(iconic_line, "disc_p5");
    let expected_flip_p5 = parse_line_hex(iconic_line, "flip_p5");

    assert_eq!(expected_p0, S_OK);
    assert_eq!(expected_p5, DXGI_ERROR_INVALID_CALL);
    assert_eq!(expected_disc_p5, DXGI_ERROR_INVALID_CALL);
    assert_eq!(expected_flip_p5, DXGI_ERROR_INVALID_CALL);

    let raw = make_swapchain(1280, 720, DXGI_SWAP_EFFECT_FLIP_DISCARD, 0);
    // SAFETY: `raw` is a live `IDXGISwapChain4`.
    let sc_vtbl = unsafe { vtbl::<IDXGISwapChain4Vtbl>(raw) };
    for interval in 0..=4 {
        assert_eq!(
            // SAFETY: `raw` is valid.
            unsafe { (sc_vtbl.present)(raw, interval, DXGI_PRESENT_TEST) },
            expected_p0
        );
    }
    // SAFETY: `raw` is valid.
    assert_eq!(unsafe { (sc_vtbl.present)(raw, 5, 0) }, expected_p5);
    // SAFETY: `raw` is valid.
    assert_eq!(unsafe { (sc_vtbl.present)(raw, u32::MAX, 0) }, expected_p5);
    // SAFETY: `raw` has refcount 1.
    assert_eq!(unsafe { release_ref(raw) }, 0);

    // Even when minimized, sync_interval > 4 returns DXGI_ERROR_INVALID_CALL (`disc_p5`, `flip_p5`).
    assert_eq!(
        evaluate_present_prelude(
            5,
            0,
            true,
            (1280, 720),
            DXGI_SWAP_EFFECT_DISCARD,
            (false, 1, true),
        )
        .0,
        expected_disc_p5
    );
    assert_eq!(
        evaluate_present_prelude(
            5,
            0,
            true,
            (1280, 720),
            DXGI_SWAP_EFFECT_FLIP_DISCARD,
            (false, 1, true),
        )
        .0,
        expected_flip_p5
    );
}

#[test]
fn alt_tab_leaves_fullscreen() {
    let hidden_line = golden_line("d11sc hidden ");
    let iconic_line = golden_line("d11sc iconic=1 ");

    let expected_unfocused = parse_line_hex(iconic_line, "disc_p0");
    let expected_focused = parse_line_hex(hidden_line, "p0");
    let expected_disabled = parse_line_hex(hidden_line, "p0");
    let expected_windowed = parse_line_hex(hidden_line, "p0");

    assert_eq!(expected_unfocused, DXGI_STATUS_OCCLUDED);
    assert_eq!(expected_focused, S_OK);
    assert_eq!(expected_disabled, S_OK);
    assert_eq!(expected_windowed, S_OK);

    // handle_alt_tab=true, windowed=0 (fullscreen), minimized=false, is_foreground=false -> occluded & exits fullscreen.
    let (hr, exit_fs, early) = evaluate_present_prelude(
        0,
        0,
        false,
        (1920, 1080),
        DXGI_SWAP_EFFECT_FLIP_DISCARD,
        (true, 0, false),
    );
    assert_eq!(hr, expected_unfocused);
    assert!(exit_fs);
    assert!(!early);

    // Focused fullscreen does not exit fullscreen.
    let (hr_fg, exit_focused, _) = evaluate_present_prelude(
        0,
        0,
        false,
        (1920, 1080),
        DXGI_SWAP_EFFECT_FLIP_DISCARD,
        (true, 0, true),
    );
    assert_eq!(hr_fg, expected_focused);
    assert!(!exit_focused);

    // handle_alt_tab=false does not exit fullscreen when unfocused.
    let (hr_dis, exit_dis, _) = evaluate_present_prelude(
        0,
        0,
        false,
        (1920, 1080),
        DXGI_SWAP_EFFECT_FLIP_DISCARD,
        (false, 0, false),
    );
    assert_eq!(hr_dis, expected_disabled);
    assert!(!exit_dis);

    // Windowed unfocused does not exit fullscreen or occlude.
    let (hr_win, exit_win, _) = evaluate_present_prelude(
        0,
        0,
        false,
        (1920, 1080),
        DXGI_SWAP_EFFECT_FLIP_DISCARD,
        (true, 1, false),
    );
    assert_eq!(hr_win, expected_windowed);
    assert!(!exit_win);

    // Verify on a SwapChain instance that Present transitions fullscreen -> windowed when handle_alt_tab=true and unfocused.
    let raw = make_swapchain(1920, 1080, DXGI_SWAP_EFFECT_FLIP_DISCARD, 0);
    let sc = SwapChain::from_iface(raw).expect("valid SwapChain");
    {
        let mut st = sc.lock_state();
        st.handle_alt_tab = true;
        st.fullscreen_desc.windowed = 0;
    }
    // SAFETY: `raw` is a live `IDXGISwapChain4`.
    let sc_vtbl = unsafe { vtbl::<IDXGISwapChain4Vtbl>(raw) };
    // SAFETY: `raw` is valid; stubbed `wsi_is_foreground` returns `false`.
    let present_hr = unsafe { (sc_vtbl.present)(raw, 0, 0) };
    assert_eq!(present_hr, expected_unfocused);
    assert_eq!(sc.lock_state().fullscreen_desc.windowed, 1);
    // SAFETY: `raw` has refcount 1.
    assert_eq!(unsafe { release_ref(raw) }, 0);
}

#[test]
fn vsync_duration_as_cpp() {
    let cases: &[(u32, i32, f64, f64)] = &[
        (0, 0, 60.0, 0.0),
        (1, 0, 60.0, 1.0 / 60.0),
        (2, 0, 60.0, 2.0 / 60.0),
        (0, 120, 60.0, 1.0 / 120.0),
        (1, 120, 60.0, 1.0 / 120.0),
        (2, 120, 60.0, 2.0 / 120.0),
        (1, 0, f64::MAX, 1.0 / f64::MAX),
    ];

    for &(sync_interval, pref_rate, init_rate, expected) in cases {
        let got = compute_vsync_duration(sync_interval, pref_rate, init_rate);
        assert!(
            (got - expected).abs() <= 1e-15,
            "mismatch for ({sync_interval}, {pref_rate}, {init_rate}): got {got}, expected {expected}"
        );
    }
}

#[test]
fn guest_paces_skips_latency_wait() {
    let fence = Arc::new(CpuFence::new(0));
    // With guest_paces = true, sync_frame at frame 5 (with latency 1 and fence at 0) returns immediately without waiting.
    let state = sync_frame(Some(&fence), 5, 1, true);
    // Still unsignaled while `state` is alive.
    assert_eq!(fence.signaled(), 0);
    drop(state);
    assert_eq!(fence.signaled(), 5);
}

#[test]
fn frame_latency_fence_signals_at_retire() {
    let raw = make_swapchain(
        1280,
        720,
        DXGI_SWAP_EFFECT_FLIP_DISCARD,
        DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
    );
    let sc = SwapChain::from_iface(raw).expect("valid SwapChain");
    let fence = Arc::clone(
        sc.frame_latency_fence
            .as_ref()
            .expect("waitable swapchain has CpuFence"),
    );
    assert_eq!(fence.signaled(), 0);

    // Record Present #1 into a chunk's Commands arena: SyncFrame(1) with latency=1 does not wait (1 <= 1),
    // and its SyncFrameState is captured by the present closure in the chunk arena.
    let mut chunk_commands = Commands::new(Arc::new(BlockPool::new(1024)));
    let sync_1 = sync_frame(Some(&fence), 1, DEFAULT_SWAPCHAIN_LATENCY, false);
    let ran_present_1 = Arc::new(AtomicBool::new(false));
    let ran_clone = Arc::clone(&ran_present_1);
    chunk_commands.emit(move |_enc| {
        let _keep_alive = &sync_1;
        ran_clone.store(true, Ordering::Release);
    });
    // Still unsignaled before chunk reset (P8: signaled when chunk resets, not when recorded or run).
    assert_eq!(fence.signaled(), 0);

    // Spawn a thread that runs SyncFrame(2) with latency=1: it must block waiting for fence >= 1.
    let fence_waiter = Arc::clone(&fence);
    let waiter_unblocked = Arc::new(AtomicBool::new(false));
    let waiter_flag = Arc::clone(&waiter_unblocked);
    let handle = thread::spawn(move || {
        let sync_2 = sync_frame(Some(&fence_waiter), 2, DEFAULT_SWAPCHAIN_LATENCY, false);
        waiter_flag.store(true, Ordering::Release);
        sync_2
    });

    thread::sleep(Duration::from_millis(15));
    assert!(
        !waiter_unblocked.load(Ordering::Acquire),
        "SyncFrame(2) must wait until Present #1's chunk resets"
    );
    assert_eq!(fence.signaled(), 0);

    // Resetting the chunk drops the recorded command closure and its SyncFrameState, signaling frame 1.
    chunk_commands.reset();
    assert_eq!(fence.signaled(), 1);

    let sync_2 = handle
        .join()
        .expect("waiter thread completes once chunk resets");
    assert!(waiter_unblocked.load(Ordering::Acquire));
    assert_eq!(fence.signaled(), 1);

    // Dropping Present #2's SyncFrameState signals frame 2.
    drop(sync_2);
    assert_eq!(fence.signaled(), 2);

    // SAFETY: `raw` has refcount 1.
    assert_eq!(unsafe { release_ref(raw) }, 0);
}

#[test]
fn swapchain_com_methods_and_format_helpers() {
    assert_eq!(
        convert_swapchain_format(DXGI_FORMAT_B8G8R8A8_UNORM_SRGB),
        WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB
    );
    assert_eq!(
        convert_swapchain_format(DXGI_FORMAT_R8G8B8A8_UNORM_SRGB),
        WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB
    );
    assert_eq!(
        convert_swapchain_format(DXGI_FORMAT_B8G8R8A8_UNORM),
        WMT_PIXEL_FORMAT_BGRA8_UNORM
    );
    assert_eq!(
        convert_swapchain_format(DXGI_FORMAT_R8G8B8A8_UNORM),
        WMT_PIXEL_FORMAT_BGRA8_UNORM
    );
    assert_eq!(
        convert_swapchain_format(DXGI_FORMAT_R10G10B10A2_UNORM),
        WMT_PIXEL_FORMAT_RGB10A2_UNORM
    );
    assert_eq!(
        convert_swapchain_format(DXGI_FORMAT_R16G16B16A16_FLOAT),
        WMT_PIXEL_FORMAT_RGBA16_FLOAT
    );
    assert_eq!(convert_swapchain_format(999), WMT_PIXEL_FORMAT_INVALID);

    assert_eq!(
        convert_color_space(DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, false),
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
        convert_color_space(DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020, false),
        WMT_COLOR_SPACE_BT2020
    );
    assert_eq!(convert_color_space(999, false), WMT_COLOR_SPACE_INVALID);

    let raw = make_swapchain(800, 600, DXGI_SWAP_EFFECT_FLIP_DISCARD, 0);
    // SAFETY: `raw` is a live `IDXGISwapChain4`.
    let sc_vtbl = unsafe { vtbl::<IDXGISwapChain4Vtbl>(raw) };

    let mut desc = DxgiSwapChainDesc {
        buffer_desc: crate::abi::records::DxgiModeDesc {
            width: 0,
            height: 0,
            refresh_rate: DxgiRational {
                numerator: 0,
                denominator: 0,
            },
            format: 0,
            scanline_ordering: 0,
            scaling: 0,
        },
        sample_desc: DxgiSampleDesc {
            count: 0,
            quality: 0,
        },
        buffer_usage: 0,
        buffer_count: 0,
        output_window: ptr::null_mut(),
        windowed: 0,
        swap_effect: 0,
        flags: 0,
    };
    assert_eq!(
        // SAFETY: `raw` and `desc` are valid.
        unsafe { (sc_vtbl.get_desc)(raw, (&raw mut desc).cast::<c_void>()) },
        S_OK
    );
    assert_eq!(desc.buffer_desc.width, 800);
    assert_eq!(desc.buffer_desc.height, 600);
    assert_eq!(desc.windowed, 1);

    let mut w = 0_u32;
    let mut h = 0_u32;
    assert_eq!(
        // SAFETY: `raw`, `w`, and `h` are valid.
        unsafe { (sc_vtbl.get_source_size)(raw, &raw mut w, &raw mut h) },
        S_OK
    );
    assert_eq!((w, h), (800, 600));

    let mut rot = 0_u32;
    // SAFETY: `raw` and `rot` are valid.
    assert_eq!(unsafe { (sc_vtbl.get_rotation)(raw, &raw mut rot) }, S_OK);
    assert_eq!(rot, DXGI_MODE_ROTATION_IDENTITY);

    let mut bg = DxgiRgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    assert_eq!(
        // SAFETY: `raw` and `bg` are valid.
        unsafe { (sc_vtbl.get_background_color)(raw, (&raw mut bg).cast::<c_void>()) },
        S_OK
    );
    assert!(bg.r == 0.0 && bg.a == 0.0);

    assert_eq!(
        // SAFETY: `raw` is valid.
        unsafe { (sc_vtbl.set_maximum_frame_latency)(raw, 0) },
        E_INVALIDARG
    );
    assert_eq!(
        // SAFETY: `raw` is valid.
        unsafe { (sc_vtbl.set_maximum_frame_latency)(raw, DXGI_MAX_SWAP_CHAIN_BUFFERS + 1) },
        E_INVALIDARG
    );
    // SAFETY: `raw` is valid.
    assert_eq!(unsafe { (sc_vtbl.set_maximum_frame_latency)(raw, 3) }, S_OK);
    let mut max_lat = 0_u32;
    assert_eq!(
        // SAFETY: `raw` and `max_lat` are valid.
        unsafe { (sc_vtbl.get_maximum_frame_latency)(raw, &raw mut max_lat) },
        S_OK
    );
    assert_eq!(max_lat, 3);

    // SAFETY: `raw` is valid.
    assert_eq!(unsafe { (sc_vtbl.get_current_back_buffer_index)(raw) }, 0);

    let mut stats = DxgiFrameStatistics {
        present_count: 99,
        present_refresh_count: 99,
        sync_refresh_count: 99,
        sync_qpc_time: 1,
        sync_gpu_time: 1,
    };
    assert_eq!(
        // SAFETY: `raw` and `stats` are valid.
        unsafe { (sc_vtbl.get_frame_statistics)(raw, (&raw mut stats).cast::<c_void>()) },
        S_OK
    );
    assert_eq!(stats.present_count, 0);

    let mut sc4 = ptr::null_mut();
    let validate_line = golden_line("d11sc validate ");
    assert_eq!(
        create(
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            &raw mut sc4,
        ),
        parse_line_hex(validate_line, "null_desc")
    );
    let dummy_desc = DxgiSwapChainDesc1 {
        width: 64,
        height: 64,
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        stereo: 0,
        sample_desc: DxgiSampleDesc {
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
    assert_eq!(
        create(
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &raw const dummy_desc,
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
        ),
        parse_line_hex(validate_line, "null_out")
    );

    let qi_null_line = golden_line("d11sc sc_qi_null ");
    let iid = IID_IDXGI_SWAP_CHAIN_4;
    assert_eq!(
        // SAFETY: `raw` is valid.
        unsafe {
            (sc_vtbl.query_interface)(raw, (&raw const iid).cast::<c_void>(), ptr::null_mut())
        },
        parse_line_hex(qi_null_line, "hr")
    );

    assert_eq!(
        // SAFETY: `raw` is valid.
        unsafe { (sc_vtbl.query_interface)(raw, (&raw const iid).cast::<c_void>(), &raw mut sc4) },
        parse_line_hex(golden_line("d11sc sc_qi iid=IDXGISwapChain4 "), "hr")
    );
    // SAFETY: `sc4` was retained by `query_interface`.
    assert_eq!(unsafe { release_ref(sc4) }, 1);
    // SAFETY: `raw` has 1 remaining reference.
    assert_eq!(unsafe { release_ref(raw) }, 0);
}
