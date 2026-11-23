use dxmt_util::dxgi_consts::{
    DXGI_FORMAT_R8_TYPELESS, DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R16_TYPELESS, DXGI_FORMAT_R16_UNORM,
    DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R16G16B16A16_TYPELESS,
};

use super::*;
use crate::parameter::{Parameters, Value};

struct ParamBag(*mut Parameters);

impl ParamBag {
    fn new() -> Self {
        Self(Parameters::allocate())
    }

    fn get(&mut self) -> &mut Parameters {
        // SAFETY: `self.0` came from `Parameters::allocate` and is uniquely owned by `self`.
        unsafe { &mut *self.0 }
    }
}

impl Drop for ParamBag {
    fn drop(&mut self) {
        Parameters::destroy(self.0);
    }
}

#[test]
fn resolves_typeless_reactive_formats() {
    assert_eq!(
        resolve_reactive_format(DXGI_FORMAT_R8_TYPELESS),
        DXGI_FORMAT_R8_UNORM
    );
    assert_eq!(
        resolve_reactive_format(DXGI_FORMAT_R16_TYPELESS),
        DXGI_FORMAT_R16_UNORM
    );
    assert_eq!(
        resolve_reactive_format(DXGI_FORMAT_R16G16B16A16_TYPELESS),
        DXGI_FORMAT_R16G16B16A16_FLOAT
    );
    assert_eq!(
        resolve_reactive_format(DXGI_FORMAT_R8_UNORM),
        DXGI_FORMAT_R8_UNORM
    );
}

#[test]
fn effective_pre_exposure_combines_scale_and_guards_invalid() {
    let mut bag = ParamBag::new();
    let p = bag.get();
    assert_eq!(effective_pre_exposure(p).to_bits(), 1.0_f32.to_bits());

    p.set(PARAM_EXPOSURE_SCALE, Value::F32(1.5));
    assert_eq!(effective_pre_exposure(p).to_bits(), 1.5_f32.to_bits());

    p.set(c"DLSS.Pre.Exposure", Value::F32(2.0));
    assert_eq!(effective_pre_exposure(p).to_bits(), 3.0_f32.to_bits());

    p.set(c"DLSS.Pre.Exposure", Value::F32(-1.0));
    assert_eq!(effective_pre_exposure(p).to_bits(), 1.5_f32.to_bits());

    p.set(PARAM_EXPOSURE_SCALE, Value::F32(f32::NAN));
    assert_eq!(effective_pre_exposure(p).to_bits(), 1.0_f32.to_bits());
}

#[test]
fn extract_camera_params_filters_non_positive_and_nan() {
    let mut bag = ParamBag::new();
    let p = bag.get();
    assert!(extract_camera_params(p).is_none());

    p.set(PARAM_DLSSG_CAMERA_NEAR, Value::F32(0.0));
    p.set(PARAM_DLSSG_CAMERA_FAR, Value::F32(-5.0));
    p.set(PARAM_DLSSG_CAMERA_FOV, Value::F32(f32::NAN));
    p.set(PARAM_DLSSG_CAMERA_ASPECT, Value::F32(f32::INFINITY));
    assert!(extract_camera_params(p).is_none());

    p.set(PARAM_DLSSG_CAMERA_NEAR, Value::F32(0.25));
    p.set(PARAM_DLSSG_CAMERA_FOV, Value::F32(60.0));
    let cam = extract_camera_params(p).expect("valid camera params");
    assert_eq!(cam.near_plane.to_bits(), 0.25_f32.to_bits());
    assert_eq!(cam.far_plane.to_bits(), 0.0_f32.to_bits());
    assert_eq!(cam.field_of_view.to_bits(), 60.0_f32.to_bits());
    assert_eq!(cam.aspect_ratio.to_bits(), 0.0_f32.to_bits());
}

#[test]
fn parse_framegen_env_handles_truthy_and_falsy() {
    assert!(!crate::entry::parse_framegen_env(None));
    assert!(!crate::entry::parse_framegen_env(Some("")));
    assert!(!crate::entry::parse_framegen_env(Some("   ")));
    assert!(!crate::entry::parse_framegen_env(Some("0")));
    assert!(!crate::entry::parse_framegen_env(Some("off")));
    assert!(!crate::entry::parse_framegen_env(Some("OFF")));
    assert!(!crate::entry::parse_framegen_env(Some("false")));
    assert!(!crate::entry::parse_framegen_env(Some("False")));
    assert!(crate::entry::parse_framegen_env(Some("1")));
    assert!(crate::entry::parse_framegen_env(Some("always")));
    assert!(crate::entry::parse_framegen_env(Some("true")));
}

#[test]
fn extract_matrices_reads_both_unaligned_or_returns_zero() {
    let mut bag = ParamBag::new();
    let p = bag.get();

    // Unaligned backing buffer (+1 byte offset) holding two 4x4 matrices.
    let mut raw = [0u8; 1 + 2 * 64];
    let mut w2v = [0.0f32; 16];
    let mut v2c = [0.0f32; 16];
    for i in 0..16 {
        w2v[i] = u32::try_from(i + 1).map_or(0.0, dxmt_util::float_cast::u32_to_f32);
        v2c[i] = u32::try_from(i + 17).map_or(0.0, dxmt_util::float_cast::u32_to_f32);
    }
    for (i, val) in w2v.iter().enumerate() {
        let start = 1 + i * 4;
        raw[start..start + 4].copy_from_slice(&val.to_le_bytes());
    }
    for (i, val) in v2c.iter().enumerate() {
        raw[65 + i * 4..65 + (i + 1) * 4].copy_from_slice(&val.to_le_bytes());
    }

    let w2v_ptr = raw.as_mut_ptr().wrapping_add(1).cast::<c_void>();
    let v2c_ptr = raw.as_mut_ptr().wrapping_add(65).cast::<c_void>();

    // Only one matrix set -> has_matrices stays 0.
    p.set(PARAM_DLSSD_WORLD_TO_VIEW_MATRIX, Value::Ptr(w2v_ptr));
    let (has, _, _) = extract_matrices(
        p,
        &[PARAM_WORLD_TO_VIEW_MATRIX, PARAM_DLSSD_WORLD_TO_VIEW_MATRIX],
        &[PARAM_VIEW_TO_CLIP_MATRIX, PARAM_DLSSD_VIEW_TO_CLIP_MATRIX],
    );
    assert_eq!(has, 0);

    // Both matrices set via DLSSD.* keys -> reads unaligned floats cleanly.
    p.set(PARAM_DLSSD_VIEW_TO_CLIP_MATRIX, Value::Ptr(v2c_ptr));
    let (has, got_w2v, got_v2c) = extract_matrices(
        p,
        &[PARAM_WORLD_TO_VIEW_MATRIX, PARAM_DLSSD_WORLD_TO_VIEW_MATRIX],
        &[PARAM_VIEW_TO_CLIP_MATRIX, PARAM_DLSSD_VIEW_TO_CLIP_MATRIX],
    );
    assert_eq!(has, 1);
    assert_eq!(got_w2v[0].to_bits(), 1.0f32.to_bits());
    assert_eq!(got_w2v[15].to_bits(), 16.0f32.to_bits());
    assert_eq!(got_v2c[0].to_bits(), 17.0f32.to_bits());
    assert_eq!(got_v2c[15].to_bits(), 32.0f32.to_bits());

    // Non-finite entry -> rejected.
    raw[1..5].copy_from_slice(&f32::NAN.to_le_bytes());
    let (has, _, _) = extract_matrices(
        p,
        &[PARAM_WORLD_TO_VIEW_MATRIX, PARAM_DLSSD_WORLD_TO_VIEW_MATRIX],
        &[PARAM_VIEW_TO_CLIP_MATRIX, PARAM_DLSSD_VIEW_TO_CLIP_MATRIX],
    );
    assert_eq!(has, 0);
}

#[test]
fn build_frame_gen_ui_desc_handles_ui_hudless_and_subrects() {
    let mut bag = ParamBag::new();
    let p = bag.get();
    assert!(!has_frame_gen_ui_params(p));

    let hudless = core::ptr::without_provenance_mut::<c_void>(0x1000);
    let ui = core::ptr::without_provenance_mut::<c_void>(0x2000);
    p.set(PARAM_DLSSG_HUDLESS, Value::D3D12(hudless));
    assert!(has_frame_gen_ui_params(p));
    let desc_hudless = build_frame_gen_ui_desc(p, 640, 360);
    assert_eq!(desc_hudless.ui_texture, hudless);
    assert_eq!(desc_hudless.content_width, 640);
    assert_eq!(desc_hudless.content_height, 360);

    // Explicit DLSSG.UI takes priority over DLSSG.HUDLess.
    p.set(PARAM_DLSSG_UI, Value::D3D12(ui));
    p.set(PARAM_DLSSG_UI_COMPOSITED, Value::I32(1));
    p.set(PARAM_DLSSG_CONTENT_WIDTH, Value::U32(1280));
    p.set(PARAM_DLSSG_CONTENT_HEIGHT, Value::U32(720));
    p.set(PARAM_DLSSG_DEPTH_SUBRECT_BASE_X, Value::U32(4));
    p.set(PARAM_DLSSG_DEPTH_SUBRECT_BASE_Y, Value::U32(8));
    p.set(PARAM_DLSSG_MVEC_SUBRECT_BASE_X, Value::U32(12));
    p.set(PARAM_DLSSG_MVEC_SUBRECT_BASE_Y, Value::U32(16));
    p.set(PARAM_DLSSG_BACKBUFFER_SUBRECT_BASE_X, Value::U32(20));
    p.set(PARAM_DLSSG_BACKBUFFER_SUBRECT_BASE_Y, Value::U32(24));

    let desc_ui = build_frame_gen_ui_desc(p, 640, 360);
    assert_eq!(desc_ui.ui_texture, ui);
    assert_eq!(desc_ui.ui_composited, 1);
    assert_eq!(desc_ui.content_width, 1280);
    assert_eq!(desc_ui.content_height, 720);
    assert_eq!(desc_ui.depth_offset, [4, 8]);
    assert_eq!(desc_ui.motion_offset, [12, 16]);
    assert_eq!(desc_ui.output_offset, [20, 24]);
}

#[test]
fn denoised_scaler_desc_eq_detects_field_changes() {
    let a = zero_denoised_scaler_desc();
    let mut b = zero_denoised_scaler_desc();
    assert!(denoised_scaler_desc_eq(&a, &b));

    b.roughness_format = DXGI_FORMAT_R16_TYPELESS;
    assert!(!denoised_scaler_desc_eq(&a, &b));

    b.roughness_format = 0;
    b.transparency_overlay_enabled = 1;
    assert!(!denoised_scaler_desc_eq(&a, &b));
}
