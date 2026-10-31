//! `dxmt_scaler.hpp`'s `dxmt-abi` block, asserted against the Rust exports.
//!
//! Written by `tools/winemetal-layouts.sh --seam-abi` from clang's AST on each target the
//! crates build for: regenerate it, never edit it.

#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *const winemetal_wire::WMTFXSpatialScalerInfo) -> *mut core::ffi::c_void = crate::scaler::dxmt_spatial_scaler_create;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *const winemetal_wire::WMTFXTemporalScalerInfo) -> *mut core::ffi::c_void = crate::scaler::dxmt_temporal_scaler_create;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::scaler::dxmt_scaler_destroy;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u64 = crate::scaler::dxmt_scaler_object;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u64 = crate::scaler::dxmt_scaler_fence;

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *const winemetal_wire::WMTFXSpatialScalerInfo) -> *mut core::ffi::c_void = crate::scaler::dxmt_spatial_scaler_create;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *const winemetal_wire::WMTFXTemporalScalerInfo) -> *mut core::ffi::c_void = crate::scaler::dxmt_temporal_scaler_create;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::scaler::dxmt_scaler_destroy;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u64 = crate::scaler::dxmt_scaler_object;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u64 = crate::scaler::dxmt_scaler_fence;

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *const winemetal_wire::WMTFXSpatialScalerInfo) -> *mut core::ffi::c_void = crate::scaler::dxmt_spatial_scaler_create;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *const winemetal_wire::WMTFXTemporalScalerInfo) -> *mut core::ffi::c_void = crate::scaler::dxmt_temporal_scaler_create;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::scaler::dxmt_scaler_destroy;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u64 = crate::scaler::dxmt_scaler_object;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u64 = crate::scaler::dxmt_scaler_fence;

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *const winemetal_wire::WMTFXSpatialScalerInfo) -> *mut core::ffi::c_void = crate::scaler::dxmt_spatial_scaler_create;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *const winemetal_wire::WMTFXTemporalScalerInfo) -> *mut core::ffi::c_void = crate::scaler::dxmt_temporal_scaler_create;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::scaler::dxmt_scaler_destroy;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u64 = crate::scaler::dxmt_scaler_object;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u64 = crate::scaler::dxmt_scaler_fence;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, *const winemetal_wire::WMTFXSpatialScalerInfo) -> *mut core::ffi::c_void = crate::scaler::dxmt_spatial_scaler_create;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, *const winemetal_wire::WMTFXTemporalScalerInfo) -> *mut core::ffi::c_void = crate::scaler::dxmt_temporal_scaler_create;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) = crate::scaler::dxmt_scaler_destroy;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) -> u64 = crate::scaler::dxmt_scaler_object;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) -> u64 = crate::scaler::dxmt_scaler_fence;
