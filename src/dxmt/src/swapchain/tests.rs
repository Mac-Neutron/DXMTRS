use winemetal_wire::{
    WMT_COLOR_SPACE_BT2020, WMT_COLOR_SPACE_HDR_PQ, WMT_COLOR_SPACE_HDR_SC_RGB,
    WMT_COLOR_SPACE_INVALID, WMT_COLOR_SPACE_SRGB, WMT_COLOR_SPACE_SRGB_LINEAR,
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB, WMT_PIXEL_FORMAT_INVALID,
    WMT_PIXEL_FORMAT_RGB10A2_UNORM, WMT_PIXEL_FORMAT_RGBA16_FLOAT,
};

use super::{
    DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_B8G8R8X8_UNORM,
    DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
    DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM, DXGI_FORMAT_R10G10B10A2_UNORM,
    DXGI_FORMAT_R16G16B16A16_FLOAT, convert_color_space, convert_format, monitor_format_bpp,
    parse_render_scale, render_scale, vsync_duration,
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
