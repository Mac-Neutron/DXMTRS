//! `ID3D12Fence1` (`d3d12_fence.cpp`): GPU/CPU timeline fences over [`dxmt_core::fence::Fence`].

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::fence::Fence;
use dxmt_util::{com::ComObject, private_data::PrivateData};

use crate::{
    abi::vtbl::ID3D12Fence1Vtbl,
    com::{ComRefCounted, Hresult},
    device::Device,
};

/// `MTLD3D12FenceImpl` (`d3d12_fence.cpp:27`).
#[repr(C)]
pub struct FenceObject {
    pub vtbl: &'static ID3D12Fence1Vtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub fence: Arc<Fence>,
    pub flags: u32,
}

impl ComRefCounted for FenceObject {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

/// Casts a non-null `ID3D12Fence` pointer if its vtable matches [`FenceObject`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_fence<'a>(raw: *mut c_void) -> Option<&'a FenceObject> {
    let _ = raw;
    crate::not_implemented!("fence::cast_fence")
}

/// Creates a [`FenceObject`] wrapping an existing core [`Fence`] (`OpenSharedHandle`).
///
/// # Safety
///
/// `device`, `riid`, and `out` are caller-supplied COM pointers.
pub unsafe fn create_with_fence(
    _device: *mut Device,
    _fence: Arc<Fence>,
    _flags: u32,
    _riid: *const c_void,
    _out: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("fence::create_with_fence")
}

/// `ID3D12Device::CreateFence` (`d3d12_device.cpp:1277`).
///
/// # Safety
///
/// `this`, `riid`, and `fence` are caller-supplied COM pointers.
pub unsafe fn create_fence(
    _this: *mut c_void,
    _initial_value: u64,
    _flags: u32,
    _riid: *const c_void,
    _fence: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateFence");
    crate::not_implemented!("ID3D12Device::CreateFence")
}

/// `ID3D12Device1::SetEventOnMultipleFenceCompletion` (`d3d12_device.cpp:1487`).
///
/// # Safety
///
/// `this`, `fences`, `values`, and `event` are caller-supplied pointers.
pub unsafe fn set_event_on_multiple_fence_completion(
    _this: *mut c_void,
    _fences: *const *mut c_void,
    _values: *const u64,
    _num_fences: u32,
    _flags: u32,
    _event: *mut c_void,
) -> Hresult {
    crate::scope!("device.SetEventOnMultipleFenceCompletion");
    crate::not_implemented!("ID3D12Device::SetEventOnMultipleFenceCompletion")
}
