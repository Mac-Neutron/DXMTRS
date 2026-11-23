//! Unit tests for occlusion-query sample scaling behind `IDirect3DQuery9`.

use super::logical_samples;

#[test]
fn logical_samples_passes_equal_and_unknown_areas_through() {
    assert_eq!(logical_samples(0, 307_200, 307_200), 0);
    assert_eq!(logical_samples(307_200, 307_200, 307_200), 307_200);
    assert_eq!(logical_samples(u64::MAX, 307_200, 307_200), u64::MAX);
    assert_eq!(
        logical_samples(42, 0, 307_200),
        42,
        "no target bound at BEGIN"
    );
    assert_eq!(
        logical_samples(42, 76_800, 0),
        42,
        "no target bound at BEGIN"
    );
}

#[test]
fn logical_samples_scales_by_the_area_ratio_rounding_to_nearest() {
    // A 640x480 frame at 50% rasterizes 320x240 samples.
    assert_eq!(logical_samples(76_800, 320 * 240, 640 * 480), 307_200);
    // At 75% a 480x360 grid; 172_800 * 16 / 9.
    assert_eq!(logical_samples(172_800, 480 * 360, 640 * 480), 307_200);
    // A dimension the scale does not divide rounds to a whole texel on the
    // render grid: 3456x2234 at 75% is 2592x1676 (1675.5 rounds to nearest,
    // half up), and a full-frame count is exact only
    // through the actual ratio, where the nominal 16/9 would give 7_723_008.
    assert_eq!(
        logical_samples(2592 * 1676, 2592 * 1676, 3456 * 2234),
        3456 * 2234
    );
    // One sample at 75% is 1.78 reported pixels, rounded to 2; at 50% it is 4.
    assert_eq!(logical_samples(1, 480 * 360, 640 * 480), 2);
    assert_eq!(logical_samples(1, 320 * 240, 640 * 480), 4);
    // 5 samples at 75% are 8.89, rounded to 9.
    assert_eq!(logical_samples(5, 480 * 360, 640 * 480), 9);
}

#[test]
fn logical_samples_saturates_instead_of_wrapping() {
    assert_eq!(logical_samples(u64::MAX, 320 * 240, 640 * 480), u64::MAX);
}
