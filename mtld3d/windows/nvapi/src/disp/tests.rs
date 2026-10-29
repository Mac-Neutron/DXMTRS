use super::{device_name_is, display_count, hdr_mode, low16, mastering_of, mastering_of_description, metadata_of, primaries};
use crate::ffi::{COLOR_SPACE_HDR_PQ, COLOR_SPACE_HDR_SCRGB, COLOR_SPACE_SRGB, DisplayDescription, HdrMetadata};
use crate::nv::{HDR_MODE_OFF, HDR_MODE_UHDA, HDR_MODE_UHDA_PASSTHROUGH};

/// A P3 display with headroom 4.0, as this Mac's built-in one reports.
const P3: DisplayDescription = DisplayDescription {
    red_primaries: [0.68, 0.32],
    blue_primaries: [0.15, 0.06],
    green_primaries: [0.265, 0.69],
    white_points: [0.3127, 0.329],
    maximum_edr_color_component_value: 1.0,
    maximum_potential_edr_color_component_value: 4.0,
    maximum_reference_edr_color_component_value: 1.0,
};

#[test]
fn primaries_scale_by_50000_and_luminance_by_100() {
    let s = primaries(&P3);
    // red, green, blue, white: 0.68f * 50000 = 34000.0 exactly in f32; 0.265f * 50000 truncates from below.
    assert_eq!(s.primary[0], 34000);
    assert_eq!(s.primary[1], 16000);
    assert_eq!(s.primary[3], 34500);
    assert_eq!(s.primary[4], 7500);
    assert_eq!(s.primary[5], 3000);
    assert_eq!(s.primary[7], 16450);
    assert_eq!(s.max_luminance, 400);
}

#[test]
fn a_scaled_value_truncates() {
    let mut d = P3;
    d.red_primaries[0] = 0.333_33;
    d.maximum_potential_edr_color_component_value = 1.239;
    let s = primaries(&d);
    assert_eq!(s.primary[0], 16666);
    assert_eq!(s.max_luminance, 123);
}

#[test]
fn display_count_is_zero_one_or_two() {
    assert_eq!(display_count(0, 0), 0);
    assert_eq!(display_count(0, 7), 0);
    assert_eq!(display_count(3, 0), 1);
    assert_eq!(display_count(3, 7), 2);
}

#[test]
fn hdr_mode_maps_scrgb_pq_and_the_rest() {
    assert_eq!(hdr_mode(COLOR_SPACE_HDR_SCRGB), HDR_MODE_UHDA);
    assert_eq!(hdr_mode(COLOR_SPACE_HDR_PQ), HDR_MODE_UHDA_PASSTHROUGH);
    assert_eq!(hdr_mode(COLOR_SPACE_SRGB), HDR_MODE_OFF);
    assert_eq!(hdr_mode(1), HDR_MODE_OFF);
}

#[test]
fn a_display_name_matches_up_to_its_nul() {
    let mut device = [0_u16; 32];
    for (slot, c) in device.iter_mut().zip(r"\\.\DISPLAY1".encode_utf16()) {
        *slot = c;
    }
    assert!(device_name_is(br"\\.\DISPLAY1", &device));
    assert!(!device_name_is(br"\\.\DISPLAY2", &device));
    assert!(!device_name_is(br"\\.\DISPLAY", &device));
    assert!(!device_name_is(b"", &device));
    assert!(device_name_is(b"", &[0; 32]));
}

#[test]
fn metadata_and_mastering_round_trip_and_narrow() {
    let metadata = HdrMetadata {
        red_primary: [1, 2],
        green_primary: [3, 4],
        blue_primary: [5, 6],
        white_point: [7, 8],
        max_mastering_luminance: 0x1_2345,
        min_mastering_luminance: 9,
        max_content_light_level: 10,
        max_frame_average_light_level: 11,
    };
    let mastering = mastering_of(&metadata);
    assert_eq!((mastering.primary_x0, mastering.white_point_y), (1, 8));
    assert_eq!(mastering.max_display_mastering_luminance, 0x2345);
    let back = metadata_of(&mastering);
    assert_eq!(back.max_mastering_luminance, 0x2345);
    assert_eq!(back.min_mastering_luminance, 9);
    assert_eq!(low16(0x1_0001), 1);
}

#[test]
fn a_display_without_a_setting_has_its_description_and_minimum_1() {
    let m = mastering_of_description(&primaries(&P3));
    assert_eq!((m.primary_x0, m.primary_y0, m.white_point_y), (34000, 16000, 16450));
    assert_eq!(m.max_display_mastering_luminance, 400);
    assert_eq!(m.min_display_mastering_luminance, 1);
    assert_eq!(m.max_content_light_level, 400);
    assert_eq!(m.max_frame_average_light_level, 400);
}
