use super::*;
use crate::abi::DXGI_ENUM_MODES_SCALING;

/// A window-system mode of 32 bits per pixel, progressive.
fn wsi(width: u32, height: u32, numerator: u32, denominator: u32) -> WsiMode {
    WsiMode {
        width,
        height,
        refresh: Rational {
            numerator,
            denominator,
        },
        bits_per_pixel: 32,
        interlaced: false,
    }
}

/// The probe's first native display: its current mode, 1920x1080 at 60 Hz, then its list.
fn display() -> Vec<WsiMode> {
    vec![
        wsi(1280, 720, 60, 1),
        wsi(1920, 1080, 60, 1),
        wsi(1920, 1080, 120_000, 1000),
        wsi(1024, 768, 75, 1),
        wsi(1600, 900, 59_940, 1000),
        wsi(1920, 1080, 50, 1),
    ]
}

/// The display's current mode, as the output converts it.
fn current() -> ModeDesc1 {
    from_wsi(&wsi(1920, 1080, 60, 1))
}

/// The display's modes as an output lists them for `R8G8B8A8_UNORM` to find a closest one.
fn listed() -> Vec<ModeDesc1> {
    mode_list(
        &display(),
        DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_ENUM_MODES_SCALING,
    )
}

/// A mode to match: a size, a refresh rate and `R8G8B8A8_UNORM`, the rest unspecified.
fn target(width: u32, height: u32, numerator: u32, denominator: u32) -> ModeDesc1 {
    ModeDesc1 {
        width,
        height,
        refresh_rate: Rational {
            numerator,
            denominator,
        },
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        ..ModeDesc1::default()
    }
}

/// Each mode's width, height and refresh rate.
fn sizes(modes: &[ModeDesc1]) -> Vec<(u32, u32, u32, u32)> {
    modes
        .iter()
        .map(|mode| {
            (
                mode.width,
                mode.height,
                mode.refresh_rate.numerator,
                mode.refresh_rate.denominator,
            )
        })
        .collect()
}

/// The closest mode's width, height and refresh rate, if there is one.
fn closest(modes: Vec<ModeDesc1>, target: &ModeDesc1) -> Option<(u32, u32, u32, u32)> {
    find_closest(modes, target, &current()).map(|mode| sizes(&[mode])[0])
}

#[test]
fn fp16_needs_64_bpp() {
    assert_eq!(monitor_format_bpp(DXGI_FORMAT_R16G16B16A16_FLOAT), 64);
    assert_eq!(monitor_format_bpp(DXGI_FORMAT_R8G8B8A8_UNORM), 32);
    let deep = WsiMode {
        bits_per_pixel: 64,
        ..wsi(800, 600, 60, 1)
    };
    let raw = [deep, wsi(640, 480, 60, 1)];
    let fp16 = mode_list(&raw, DXGI_FORMAT_R16G16B16A16_FLOAT, 0);
    assert_eq!(sizes(&fp16), [(800, 600, 60, 1)]);
    assert_eq!(fp16[0].format, DXGI_FORMAT_R16G16B16A16_FLOAT);
    assert_eq!(
        sizes(&mode_list(&raw, DXGI_FORMAT_B8G8R8A8_UNORM, 0)),
        [(640, 480, 60, 1)]
    );
}

#[test]
fn unknown_format_has_no_modes() {
    assert!(!is_back_buffer_format(DXGI_FORMAT_UNKNOWN));
    assert!(mode_list(&display(), DXGI_FORMAT_UNKNOWN, 0).is_empty());
}

#[test]
fn non_back_buffer_format_has_no_modes() {
    for format in [
        DXGI_FORMAT_B8G8R8X8_UNORM,
        DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM,
        71,
    ] {
        assert!(!is_back_buffer_format(format));
        assert!(mode_list(&display(), format, 0).is_empty());
    }
    for format in [
        DXGI_FORMAT_R16G16B16A16_FLOAT,
        DXGI_FORMAT_R10G10B10A2_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        DXGI_FORMAT_B8G8R8A8_UNORM,
        DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
    ] {
        assert!(is_back_buffer_format(format));
    }
}

#[test]
fn interlaced_only_when_asked() {
    let interlaced = WsiMode {
        interlaced: true,
        ..wsi(1920, 1080, 60, 1)
    };
    let raw = [interlaced, wsi(1280, 720, 60, 1)];
    for flags in [0, DXGI_ENUM_MODES_SCALING] {
        let list = mode_list(&raw, DXGI_FORMAT_R8G8B8A8_UNORM, flags);
        assert_eq!(sizes(&list), [(1280, 720, 60, 1)]);
    }
    let list = mode_list(&raw, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_ENUM_MODES_INTERLACED);
    assert_eq!(sizes(&list), [(1280, 720, 60, 1), (1920, 1080, 60, 1)]);
    assert_eq!(
        list[1].scanline_ordering,
        DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST
    );
    assert_eq!(
        list[0].scanline_ordering,
        DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE
    );
}

#[test]
fn sorted_by_width_height_rate() {
    let list = mode_list(&display(), DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, 0);
    assert_eq!(
        sizes(&list),
        [
            (1024, 768, 75, 1),
            (1280, 720, 60, 1),
            (1600, 900, 59_940, 1000),
            (1920, 1080, 50, 1),
            (1920, 1080, 60, 1),
            (1920, 1080, 120_000, 1000),
        ]
    );
    for mode in &list {
        assert_eq!(mode.format, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB);
        assert_eq!(mode.scanline_ordering, DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE);
        assert_eq!((mode.scaling, mode.stereo), (0, 0));
    }
}

#[test]
fn closest_prefers_resolution_then_rate() {
    assert_eq!(
        closest(listed(), &target(1900, 1000, 100, 1)),
        Some((1920, 1080, 120_000, 1000))
    );
    assert_eq!(
        closest(listed(), &target(1610, 905, 0, 0)),
        Some((1600, 900, 59_940, 1000))
    );
    assert_eq!(
        closest(listed(), &target(1920, 1080, 55_000, 1000)),
        Some((1920, 1080, 50, 1))
    );
}

#[test]
fn empty_rate_diff_is_u32_max() {
    assert_eq!(min_rate_diff(&[], &target(0, 0, 60, 1)), u32::MAX);
    // The u64 minimum's low 32 bits: (2^32 - 1)^2 - 1 is 2^33 away from 2^64, low bits 0.
    let fast = target(1, 1, u32::MAX, 1);
    let slow = target(1, 1, 1, u32::MAX);
    assert_eq!(min_rate_diff(&[fast], &slow), 0);
    assert_eq!(closest(vec![target(1, 1, u32::MAX, 1)], &slow), None);
}

#[test]
fn zero_denominator_sorts_last_and_is_skipped() {
    let mut raw = vec![
        wsi(2560, 1440, 144, 1),
        wsi(2560, 1440, 60, 1),
        wsi(1920, 1080, 60, 1),
    ];
    raw.push(wsi(1280, 720, 0, 0));
    let list = mode_list(&raw, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_ENUM_MODES_SCALING);
    assert_eq!(
        sizes(&list),
        [
            (1920, 1080, 60, 1),
            (2560, 1440, 60, 1),
            (2560, 1440, 144, 1),
            (1280, 720, 0, 0),
        ]
    );
    let again = mode_list(&raw, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_ENUM_MODES_SCALING);
    assert_eq!(closest(list, &target(1280, 720, 60, 1)), None);
    assert_eq!(
        closest(again, &target(1920, 1080, 60, 1)),
        Some((1920, 1080, 60, 1))
    );
}

#[test]
fn unknown_format_takes_srgb() {
    let current = current();
    assert_eq!(current.format, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB);
    let list = mode_list(
        &display(),
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        DXGI_ENUM_MODES_SCALING,
    );
    let unknown = ModeDesc1 {
        format: DXGI_FORMAT_UNKNOWN,
        ..ModeDesc1::default()
    };
    let found = find_closest(list, &unknown, &current);
    assert_eq!(
        found.map(|mode| (mode.width, mode.height, mode.format)),
        Some((1920, 1080, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB))
    );
}

#[test]
fn zero_size_takes_the_current_size() {
    assert_eq!(
        closest(listed(), &target(0, 0, 0, 0)),
        Some((1920, 1080, 60, 1))
    );
}

#[test]
fn zero_size_at_61_keeps_the_current_size() {
    assert_eq!(
        closest(listed(), &target(0, 0, 61, 1)),
        Some((1920, 1080, 60, 1))
    );
}

#[test]
fn stereo_finds_nothing() {
    let stereo = ModeDesc1 {
        stereo: 1,
        ..target(1920, 1080, 60, 1)
    };
    assert_eq!(closest(listed(), &stereo), None);
}

#[test]
fn progressive_scanline_filters() {
    let interlaced = WsiMode {
        interlaced: true,
        ..wsi(1920, 1080, 60, 1)
    };
    let raw = [interlaced, wsi(1920, 1080, 60, 1)];
    let list = || mode_list(&raw, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_ENUM_MODES_INTERLACED);
    for order in [
        DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE,
        DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST,
    ] {
        let wanted = ModeDesc1 {
            scanline_ordering: order,
            ..target(1920, 1080, 0, 0)
        };
        let found = find_closest(list(), &wanted, &current());
        assert_eq!(found.map(|mode| mode.scanline_ordering), Some(order));
    }
}

#[test]
fn win32_refresh_is_milli() {
    let rate = refresh_from_hz(60);
    assert_eq!((rate.numerator, rate.denominator), (60_000, 1000));
    let wrapped = refresh_from_hz(0x0100_0000);
    assert_eq!(
        (wrapped.numerator, wrapped.denominator),
        (3_892_314_112, 1000)
    );
}

#[test]
fn short_buffer_is_more_data_count_kept() {
    let mut buffer = [0; 2];
    let answer = copy_out(listed(), 2, |i, mode| buffer[i] = mode.width);
    assert_eq!(answer, None);
    assert_eq!(buffer, [1024, 1280]);
    let mut calls = 0;
    assert_eq!(copy_out(listed(), 0, |_, _| calls += 1), None);
    assert_eq!(calls, 0);
}

#[test]
fn big_buffer_sets_the_count_tail_untouched() {
    let mut buffer = [7; 9];
    let answer = copy_out(listed(), 9, |i, mode| buffer[i] = mode.width);
    assert_eq!(answer, Some(6));
    assert_eq!(buffer, [1024, 1280, 1600, 1920, 1920, 1920, 7, 7, 7]);
    assert_eq!(copy_out(listed(), 6, |_, _| {}), Some(6));
    assert_eq!(copy_out(Vec::new(), 0, |_, _| {}), Some(0));
}
