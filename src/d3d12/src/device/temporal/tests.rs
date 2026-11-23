use core::{ffi::c_void, ptr};

use dxmt_util::dxgi_consts::{
    DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R32_UINT, DXGI_FORMAT_UNKNOWN,
};
use winemetal_wire::WMT_PIXEL_FORMAT_INVALID;

use super::{
    create_temporal_denoised_scaler, denoised_info_eq, optional_colour_pixel_format,
    supports_temporal_denoised_scaler, zeroed_denoised_desc, zeroed_denoised_info,
};
use crate::{
    com::{DXGI_ERROR_UNSUPPORTED, E_INVALIDARG, release_com},
    device::Device,
};

#[test]
fn supports_and_create_validate_null_and_unsupported() {
    // SAFETY: null companion pointer returns 0.
    let supported = unsafe { supports_temporal_denoised_scaler(ptr::null_mut()) };
    assert_eq!(supported, 0);

    let dev = Box::into_raw(Device::new_dummy());
    // SAFETY: `Device::EXT_OFFSET` is within `Device`.
    let ext_this = unsafe { dev.cast::<c_void>().byte_add(Device::EXT_OFFSET) };

    // SAFETY: dummy device has `mtl = None` -> 0.
    assert_eq!(unsafe { supports_temporal_denoised_scaler(ext_this) }, 0);

    let mut out: *mut c_void = ptr::without_provenance_mut(0xdead);
    let desc = zeroed_denoised_desc();
    // SAFETY: null `desc` returns `E_INVALIDARG`.
    let hr = unsafe { create_temporal_denoised_scaler(ext_this, ptr::null(), &raw mut out) };
    assert_eq!(hr, E_INVALIDARG);
    // SAFETY: null `out` returns `E_INVALIDARG`.
    let hr = unsafe { create_temporal_denoised_scaler(ext_this, &raw const desc, ptr::null_mut()) };
    assert_eq!(hr, E_INVALIDARG);

    // SAFETY: unsupported dummy device zeroes `out` and returns `DXGI_ERROR_UNSUPPORTED`.
    let hr = unsafe { create_temporal_denoised_scaler(ext_this, &raw const desc, &raw mut out) };
    assert_eq!(hr, DXGI_ERROR_UNSUPPORTED);
    assert!(out.is_null());

    // SAFETY: releases initial reference on `dev`.
    assert_eq!(unsafe { release_com(dev) }, 0);
}

#[test]
fn optional_colour_format_and_info_eq_behave_as_expected() {
    assert_eq!(
        optional_colour_pixel_format(0, DXGI_FORMAT_UNKNOWN),
        Some((0, WMT_PIXEL_FORMAT_INVALID))
    );
    assert_eq!(
        optional_colour_pixel_format(0, DXGI_FORMAT_R16G16B16A16_FLOAT),
        Some((0, WMT_PIXEL_FORMAT_INVALID))
    );
    assert_eq!(optional_colour_pixel_format(1, DXGI_FORMAT_UNKNOWN), None);
    assert_eq!(optional_colour_pixel_format(1, DXGI_FORMAT_R32_UINT), None);
    let parsed = optional_colour_pixel_format(1, DXGI_FORMAT_R16G16B16A16_FLOAT);
    assert!(matches!(parsed, Some((1, fmt)) if fmt != WMT_PIXEL_FORMAT_INVALID));

    let a = zeroed_denoised_info();
    let mut b = zeroed_denoised_info();
    assert!(denoised_info_eq(&a, &b));
    b.transparency_overlay_enabled = 1;
    assert!(!denoised_info_eq(&a, &b));
}
