//! The spatial and temporal scalers (`MTLFXSpatialScaler`, `MTLFXTemporalScaler`).
//!
//! A [`Scaler`] is a `MetalFX` scaler and the fence its passes wait on and update. The C++
//! `SpatialScaler` and `TemporalScaler` (`dxmt_scaler.hpp`) each own one through the
//! `dxmt_*_scaler_create` functions and keep their reference counts and the temporal scaler's
//! description themselves.

mod abi;

use core::ffi::c_void;

use winemetal_sys::{
    MTLDevice_newFence, MTLDevice_newSpatialScaler, MTLDevice_newTemporalScaler, Obj,
    wire::{WMTFXSpatialScalerInfo, WMTFXTemporalScalerInfo},
};

/// A `MetalFX` scaler and its fence, each `None` when Metal made none.
pub struct Scaler {
    // Fields drop in declaration order: the C++ released the fence first.
    fence: Option<Obj>,
    scaler: Option<Obj>,
}

impl Scaler {
    /// A spatial scaler for `info`, then its fence.
    ///
    /// # Safety
    ///
    /// `device` is a live `MTLDevice`.
    #[must_use]
    pub unsafe fn spatial(device: u64, info: &WMTFXSpatialScalerInfo) -> Self {
        // SAFETY: the caller's promise; the info is alive for the call.
        let scaler = unsafe { MTLDevice_newSpatialScaler(device, info) };
        // SAFETY: as above, and `scaler` is +1 or null.
        unsafe { Self::with_fence(device, scaler) }
    }

    /// A temporal scaler for `info`, then its fence.
    ///
    /// # Safety
    ///
    /// `device` is a live `MTLDevice`.
    #[must_use]
    pub unsafe fn temporal(device: u64, info: &WMTFXTemporalScalerInfo) -> Self {
        // SAFETY: the caller's promise; the info is alive for the call.
        let scaler = unsafe { MTLDevice_newTemporalScaler(device, info) };
        // SAFETY: as above, and `scaler` is +1 or null.
        unsafe { Self::with_fence(device, scaler) }
    }

    /// The scaler, or `None`.
    #[must_use]
    pub const fn scaler(&self) -> Option<&Obj> {
        self.scaler.as_ref()
    }

    /// The fence, or `None`.
    #[must_use]
    pub const fn fence(&self) -> Option<&Obj> {
        self.fence.as_ref()
    }

    /// # Safety
    ///
    /// `device` is live; `scaler` is +1 or null.
    unsafe fn with_fence(device: u64, scaler: u64) -> Self {
        // SAFETY: the caller's promise.
        let scaler = unsafe { Obj::adopt(scaler) };
        // SAFETY: a live device.
        let fence = unsafe { MTLDevice_newFence(device) };
        Self {
            // SAFETY: `newFence` answers +1 or null.
            fence: unsafe { Obj::adopt(fence) },
            scaler,
        }
    }
}

crate::seam_export! {
    /// [`Scaler::spatial`], freed by [`dxmt_scaler_destroy`].
    ///
    /// # Safety
    ///
    /// `device` is live; `info` is readable.
    unsafe fn dxmt_spatial_scaler_create(
        device: u64,
        info: *const WMTFXSpatialScalerInfo
    ) -> *mut c_void {
        // SAFETY: the caller's promise.
        let info = unsafe { &*info };
        // SAFETY: the caller's promise.
        let scaler = unsafe { Scaler::spatial(device, info) };
        Box::into_raw(Box::new(scaler)).cast::<c_void>()
    }
}

crate::seam_export! {
    /// [`Scaler::temporal`], freed by [`dxmt_scaler_destroy`].
    ///
    /// # Safety
    ///
    /// `device` is live; `info` is readable.
    unsafe fn dxmt_temporal_scaler_create(
        device: u64,
        info: *const WMTFXTemporalScalerInfo
    ) -> *mut c_void {
        // SAFETY: the caller's promise.
        let info = unsafe { &*info };
        // SAFETY: the caller's promise.
        let scaler = unsafe { Scaler::temporal(device, info) };
        Box::into_raw(Box::new(scaler)).cast::<c_void>()
    }
}

crate::seam_export! {
    /// Releases the fence, then the scaler, and frees it.
    ///
    /// # Safety
    ///
    /// `s` is a scaler a `dxmt_*_scaler_create` returned and nothing has destroyed.
    unsafe fn dxmt_scaler_destroy(s: *mut c_void) {
        // SAFETY: the caller's promise: a live box, given back here.
        drop(unsafe { Box::from_raw(s.cast::<Scaler>()) });
    }
}

crate::seam_export! {
    /// The scaler's handle (+0), or null.
    ///
    /// # Safety
    ///
    /// `s` is a live scaler.
    unsafe fn dxmt_scaler_object(s: *mut c_void) -> u64 {
        // SAFETY: the caller's promise.
        unsafe { &*s.cast::<Scaler>() }.scaler().map_or(0, Obj::handle)
    }
}

crate::seam_export! {
    /// The fence's handle (+0), or null.
    ///
    /// # Safety
    ///
    /// `s` is a live scaler.
    unsafe fn dxmt_scaler_fence(s: *mut c_void) -> u64 {
        // SAFETY: the caller's promise.
        unsafe { &*s.cast::<Scaler>() }.fence().map_or(0, Obj::handle)
    }
}
