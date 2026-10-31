//! `dxmt_presenter.hpp`'s `dxmt-abi` block, asserted against the Rust exports.
//!
//! Written by `tools/winemetal-layouts.sh --seam-abi` from clang's AST on each target the
//! crates build for: regenerate it, never edit it.

#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, u64, f32, u8) -> *mut core::ffi::c_void = crate::presenter::dxmt_presenter_create;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::presenter::dxmt_presenter_destroy;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u32, u64, f64, f64, u8) -> bool = crate::presenter::dxmt_presenter_change_layer_properties;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64) -> bool = crate::presenter::dxmt_presenter_change_layer_color_space;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const winemetal_wire::WMTHDRMetadata) = crate::presenter::dxmt_presenter_change_hdr_metadata;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const crate::presenter::DXMTGammaRamp) = crate::presenter::dxmt_presenter_change_gamma_ramp;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *mut crate::presenter::DXMTPresentMetadata) -> u64 = crate::presenter::dxmt_presenter_synchronize;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64) = crate::presenter::dxmt_presenter_frame_done;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64, u64, *const crate::presenter::DXMTPresentMetadata, *const crate::presenter::DxmtPresenterCallbacks) -> u64 = crate::presenter::dxmt_presenter_encode;

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, u64, f32, u8) -> *mut core::ffi::c_void = crate::presenter::dxmt_presenter_create;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::presenter::dxmt_presenter_destroy;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u32, u64, f64, f64, u8) -> bool = crate::presenter::dxmt_presenter_change_layer_properties;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64) -> bool = crate::presenter::dxmt_presenter_change_layer_color_space;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const winemetal_wire::WMTHDRMetadata) = crate::presenter::dxmt_presenter_change_hdr_metadata;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const crate::presenter::DXMTGammaRamp) = crate::presenter::dxmt_presenter_change_gamma_ramp;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *mut crate::presenter::DXMTPresentMetadata) -> u64 = crate::presenter::dxmt_presenter_synchronize;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64) = crate::presenter::dxmt_presenter_frame_done;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64, u64, *const crate::presenter::DXMTPresentMetadata, *const crate::presenter::DxmtPresenterCallbacks) -> u64 = crate::presenter::dxmt_presenter_encode;

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, u64, f32, u8) -> *mut core::ffi::c_void = crate::presenter::dxmt_presenter_create;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::presenter::dxmt_presenter_destroy;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u32, u64, f64, f64, u8) -> bool = crate::presenter::dxmt_presenter_change_layer_properties;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64) -> bool = crate::presenter::dxmt_presenter_change_layer_color_space;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const winemetal_wire::WMTHDRMetadata) = crate::presenter::dxmt_presenter_change_hdr_metadata;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const crate::presenter::DXMTGammaRamp) = crate::presenter::dxmt_presenter_change_gamma_ramp;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *mut crate::presenter::DXMTPresentMetadata) -> u64 = crate::presenter::dxmt_presenter_synchronize;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64) = crate::presenter::dxmt_presenter_frame_done;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64, u64, *const crate::presenter::DXMTPresentMetadata, *const crate::presenter::DxmtPresenterCallbacks) -> u64 = crate::presenter::dxmt_presenter_encode;

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, u64, f32, u8) -> *mut core::ffi::c_void = crate::presenter::dxmt_presenter_create;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::presenter::dxmt_presenter_destroy;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u32, u64, f64, f64, u8) -> bool = crate::presenter::dxmt_presenter_change_layer_properties;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64) -> bool = crate::presenter::dxmt_presenter_change_layer_color_space;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const winemetal_wire::WMTHDRMetadata) = crate::presenter::dxmt_presenter_change_hdr_metadata;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const crate::presenter::DXMTGammaRamp) = crate::presenter::dxmt_presenter_change_gamma_ramp;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *mut crate::presenter::DXMTPresentMetadata) -> u64 = crate::presenter::dxmt_presenter_synchronize;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64) = crate::presenter::dxmt_presenter_frame_done;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u64, u64, *const crate::presenter::DXMTPresentMetadata, *const crate::presenter::DxmtPresenterCallbacks) -> u64 = crate::presenter::dxmt_presenter_encode;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, u64, u64, f32, u8) -> *mut core::ffi::c_void = crate::presenter::dxmt_presenter_create;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) = crate::presenter::dxmt_presenter_destroy;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, u32, u64, f64, f64, u8) -> bool = crate::presenter::dxmt_presenter_change_layer_properties;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, u64) -> bool = crate::presenter::dxmt_presenter_change_layer_color_space;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, *const winemetal_wire::WMTHDRMetadata) = crate::presenter::dxmt_presenter_change_hdr_metadata;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, *const crate::presenter::DXMTGammaRamp) = crate::presenter::dxmt_presenter_change_gamma_ramp;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, *mut crate::presenter::DXMTPresentMetadata) -> u64 = crate::presenter::dxmt_presenter_synchronize;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, u64) = crate::presenter::dxmt_presenter_frame_done;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, u64, u64, *const crate::presenter::DXMTPresentMetadata, *const crate::presenter::DxmtPresenterCallbacks) -> u64 = crate::presenter::dxmt_presenter_encode;
