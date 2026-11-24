use super::{PS_DRAW_BYTES, PS_DRAW_MSL, build_ps_draw_bytes};
use crate::render_scale::RenderScale;

fn lanes(bytes: &[u8; PS_DRAW_BYTES]) -> [f32; 4] {
    let mut out = [0.0f32; 4];
    for (lane, chunk) in out.iter_mut().zip(bytes.as_chunks::<4>().0) {
        *lane = f32::from_le_bytes(*chunk);
    }
    out
}

/// The lanes as bit patterns, for an exact comparison of values that are exact.
fn bits(bytes: &[u8; PS_DRAW_BYTES]) -> [u32; 4] {
    lanes(bytes).map(f32::to_bits)
}

#[test]
fn the_identity_scale_maps_one_render_pixel_to_one_logical_pixel() {
    assert_eq!(
        bits(&build_ps_draw_bytes(RenderScale::IDENTITY)),
        [1.0f32, 1.0, 0.0, 0.0].map(f32::to_bits)
    );
}

#[test]
fn a_reduced_scale_maps_one_render_pixel_to_its_inverse_in_logical_pixels() {
    assert_eq!(
        bits(&build_ps_draw_bytes(RenderScale::from_percent(50))),
        [2.0f32, 2.0, 0.0, 0.0].map(f32::to_bits)
    );
    let [x, y, z, w] = lanes(&build_ps_draw_bytes(RenderScale::from_percent(75)));
    assert!(
        (x - 4.0 / 3.0).abs() < 1e-6,
        "75% is four thirds of a logical pixel per render pixel"
    );
    assert_eq!(
        [y, z, w].map(f32::to_bits),
        [x, 0.0, 0.0].map(f32::to_bits),
        "both axes carry the same factor and the spare lanes are zero"
    );
}

#[test]
fn a_per_axis_scale_carries_its_own_inverse_in_each_lane() {
    let fit =
        crate::render_scale::panel_fit(true, (2048, 1536), Some((1920, 1080)), Some((2048, 1536)));
    let [x, y, z, w] = lanes(&build_ps_draw_bytes(fit));
    assert!((x - 2048.0 / 1920.0).abs() < 1e-6);
    assert!((y - 1536.0 / 1080.0).abs() < 1e-6);
    assert_eq!([z, w].map(f32::to_bits), [0.0f32, 0.0].map(f32::to_bits));
}

#[test]
fn the_msl_view_names_the_one_row_the_bytes_carry() {
    assert!(PS_DRAW_MSL.contains("float4 vpos_scale;"));
    assert_eq!(PS_DRAW_BYTES, 16);
}
