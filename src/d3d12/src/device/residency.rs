//! Device residency set and GPU virtual-address lookup (`d3d12_device.cpp:1264-1275, 1888-2038`).

use core::ffi::c_void;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use dxmt_core::resource::{Buffer, Texture};
use winemetal_sys::Obj;

use crate::{com::Hresult, device::Device};

/// Device residency set and GPU virtual-address map (`d3d12_device.cpp:365-366`).
pub struct Residency {
    pub set: Option<Obj>,
    pub va_map: Mutex<BTreeMap<u64, Vec<Arc<Buffer>>>>,
}

impl Residency {
    /// Creates an empty residency table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            set: None,
            va_map: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Default for Residency {
    fn default() -> Self {
        Self::new()
    }
}

/// Registers `buffer` in the device's Metal residency set (`d3d12_device.cpp:1998`).
pub fn register(_device: &Device, _buffer: &Arc<Buffer>) {
    let (): () = crate::not_implemented!("device::residency::register");
}

/// Registers `buffer` in the residency set and GPU VA lookup table (`d3d12_device.cpp:2009`).
pub fn register_with_va(_device: &Device, _buffer: &Arc<Buffer>) {
    let (): () = crate::not_implemented!("device::residency::register_with_va");
}

/// Unregisters `buffer` from the residency set and GPU VA lookup table (`d3d12_device.cpp:2017`).
pub fn unregister(_device: &Device, _buffer: &Arc<Buffer>, _gpu_va: u64) {
    let (): () = crate::not_implemented!("device::residency::unregister");
}

/// Looks up the newest buffer whose GPU VA range contains `address` (`d3d12_device.cpp:2025`).
///
/// Returns `(buffer, byte_offset)`.
#[must_use]
pub fn lookup_buffer_by_va(_device: &Device, _address: u64) -> Option<(Arc<Buffer>, u64)> {
    crate::not_implemented!("device::residency::lookup_buffer_by_va")
}

/// Registers `texture` in the device's Metal residency set (`d3d12_device.cpp:1998`).
pub fn register_texture(_device: &Device, _texture: &Arc<Texture>) {
    let (): () = crate::not_implemented!("device::residency::register_texture");
}

/// Unregisters `texture` from the device's Metal residency set (`d3d12_device.cpp:2004`).
pub fn unregister_texture(_device: &Device, _texture: &Arc<Texture>) {
    let (): () = crate::not_implemented!("device::residency::unregister_texture");
}

/// Registers a `MTLHeap` in the device's Metal residency set (`d3d12_device.cpp:1998`).
pub fn register_heap(_device: &Device, _heap: &Obj) {
    let (): () = crate::not_implemented!("device::residency::register_heap");
}

/// Unregisters a `MTLHeap` from the device's Metal residency set (`d3d12_device.cpp:2004`).
pub fn unregister_heap(_device: &Device, _heap: &Obj) {
    let (): () = crate::not_implemented!("device::residency::unregister_heap");
}

/// `ID3D12Device::MakeResident` (`d3d12_device.cpp:1264`).
///
/// # Safety
///
/// `this` and `objects` are caller-supplied COM pointers.
pub unsafe fn make_resident(
    _this: *mut c_void,
    _num_objects: u32,
    _objects: *const *mut c_void,
) -> Hresult {
    crate::scope!("device.MakeResident");
    crate::not_implemented!("ID3D12Device::MakeResident")
}

/// `ID3D12Device::Evict` (`d3d12_device.cpp:1271`).
///
/// # Safety
///
/// `this` and `objects` are caller-supplied COM pointers.
pub unsafe fn evict(
    _this: *mut c_void,
    _num_objects: u32,
    _objects: *const *mut c_void,
) -> Hresult {
    crate::scope!("device.Evict");
    crate::not_implemented!("ID3D12Device::Evict")
}

/// `ID3D12Device1::SetResidencyPriority` (`d3d12_device.cpp:1495`).
///
/// # Safety
///
/// `this`, `objects`, and `priorities` are caller-supplied pointers.
pub unsafe fn set_residency_priority(
    _this: *mut c_void,
    _num_objects: u32,
    _objects: *const *mut c_void,
    _priorities: *const u32,
) -> Hresult {
    crate::scope!("device.SetResidencyPriority");
    crate::not_implemented!("ID3D12Device::SetResidencyPriority")
}

/// `ID3D12Device3::EnqueueMakeResident` (`d3d12_device.cpp:1554`).
///
/// # Safety
///
/// `this`, `objects`, and `fence` are caller-supplied COM pointers.
pub unsafe fn enqueue_make_resident(
    _this: *mut c_void,
    _flags: u32,
    _num_objects: u32,
    _objects: *const *mut c_void,
    _fence: *mut c_void,
    _fence_value: u64,
) -> Hresult {
    crate::scope!("device.EnqueueMakeResident");
    crate::not_implemented!("ID3D12Device::EnqueueMakeResident")
}
