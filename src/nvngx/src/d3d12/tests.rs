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
