//! `IMTLD3D12DeviceExt` and `MTLD3D12TemporalScalerImpl` (`d3d12_device.cpp:267-321, 1858-1884`).

use core::{ffi::c_void, sync::atomic::AtomicBool};
use std::sync::{Arc, Mutex};

use dxmt_core::{presenter::framegen::TemporalScaler, resource::Texture};
use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_wire::WMTFXTemporalScalerInfo;

use crate::{
    abi::ext::{D3D12ScalerDesc, D3D12TemporalScalerVtbl},
    com::{ComRefCounted, Hresult},
    device::Device,
};

/// Which scratch texture a temporal upscale needs (`MTLD3D12TemporalScaler::Scratch`,
///
/// `d3d12_device.hpp:163`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scratch {
    Depth,
    Motion,
    Output,
}

/// `MTLD3D12TemporalScalerImpl` (`d3d12_device.hpp:156-165`, `d3d12_device.cpp:267-321`).
#[repr(C)]
pub struct TemporalScalerObject {
    pub vtbl: &'static D3D12TemporalScalerVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub desc: D3D12ScalerDesc,
    pub info: WMTFXTemporalScalerInfo,
    pub scaler: Arc<TemporalScaler>,
    pub fresh: AtomicBool,
    pub depth_scratch: Mutex<Option<Arc<Texture>>>,
    pub motion_scratch: Mutex<Option<Arc<Texture>>>,
    pub output_scratch: Mutex<Option<Arc<Texture>>>,
}

impl ComRefCounted for TemporalScalerObject {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl TemporalScalerObject {
    /// Returns (creating on first use) the scratch texture for `which` (`GetScratch`,
    ///
    /// `d3d12_device.cpp:289`).
    #[must_use]
    pub fn get_scratch(&self, _which: Scratch) -> Option<Arc<Texture>> {
        crate::not_implemented!("device::temporal::TemporalScalerObject::get_scratch")
    }
}

/// Casts a non-null `IMTLD3D12TemporalScaler` pointer if its vtable matches
///
/// [`TemporalScalerObject`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer (a valid vtable pointer at
/// offset 0).
#[must_use]
pub unsafe fn cast_temporal_scaler<'a>(raw: *mut c_void) -> Option<&'a TemporalScalerObject> {
    let _ = raw;
    crate::not_implemented!("device::temporal::cast_temporal_scaler")
}

/// `IMTLD3D12DeviceExt::GetTemporalScalerScaleRange` (`d3d12_device.cpp:1858`).
///
/// # Safety
///
/// `this`, `min_scale`, and `max_scale` are caller-supplied pointers.
pub unsafe fn get_temporal_scaler_scale_range(
    _this: *mut c_void,
    _min_scale: *mut f32,
    _max_scale: *mut f32,
) -> Hresult {
    crate::scope!("device.GetTemporalScalerScaleRange");
    crate::not_implemented!("IMTLD3D12DeviceExt::GetTemporalScalerScaleRange")
}

/// `IMTLD3D12DeviceExt::CreateTemporalScaler` (`d3d12_device.cpp:1866`).
///
/// # Safety
///
/// `this`, `desc`, and `scaler` are caller-supplied pointers.
pub unsafe fn create_temporal_scaler(
    _this: *mut c_void,
    _desc: *const D3D12ScalerDesc,
    _scaler: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateTemporalScaler");
    crate::not_implemented!("IMTLD3D12DeviceExt::CreateTemporalScaler")
}

/// Acquires or creates a `MetalFX` temporal scaler matching `info` (`d3d12_device.cpp:1866`).
#[must_use]
pub fn acquire_temporal_scaler(
    _device: &Device,
    _info: &WMTFXTemporalScalerInfo,
) -> Option<Arc<TemporalScaler>> {
    crate::not_implemented!("device::temporal::acquire_temporal_scaler")
}

/// Returns a `MetalFX` temporal scaler to the device's pool (`d3d12_device.cpp:283`).
pub fn release_temporal_scaler(
    _device: &Device,
    _info: WMTFXTemporalScalerInfo,
    _scaler: Arc<TemporalScaler>,
) {
    let (): () = crate::not_implemented!("device::temporal::release_temporal_scaler");
}
