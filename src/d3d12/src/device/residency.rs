//! Device residency set and GPU virtual-address lookup (`d3d12_device.cpp:1264-1275, 1888-2038`).

use core::ffi::c_void;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use dxmt_core::resource::{Buffer, BufferAllocationFlags, Texture, TextureAllocationFlags};
use winemetal_sys::{
    MTLResidencySet_addAllocations, MTLResidencySet_commit, MTLResidencySet_removeAllocations, Obj,
};

use crate::{
    abi::vtbl::ID3D12FenceVtbl,
    com::{self, E_INVALIDARG, Hresult, S_OK},
    device::Device,
};

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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn add_allocation(set: Option<&Obj>, allocation: u64) {
    if let Some(set) = set
        && allocation != 0
    {
        let h = set.handle();
        // SAFETY: `h` is a live `MTLResidencySet` handle and `allocation` is a live `MTLAllocation` handle.
        unsafe { MTLResidencySet_addAllocations(h, &raw const allocation, 1) };
        // SAFETY: `h` is a live `MTLResidencySet` handle.
        unsafe { MTLResidencySet_commit(h) };
    }
}

fn remove_allocation(set: Option<&Obj>, allocation: u64) {
    if let Some(set) = set
        && allocation != 0
    {
        let h = set.handle();
        // SAFETY: `h` is a live `MTLResidencySet` handle and `allocation` is a live `MTLAllocation` handle.
        unsafe { MTLResidencySet_removeAllocations(h, &raw const allocation, 1) };
        // SAFETY: `h` is a live `MTLResidencySet` handle.
        unsafe { MTLResidencySet_commit(h) };
    }
}

fn insert_va<T>(map: &mut BTreeMap<u64, Vec<Arc<T>>>, gpu_va: u64, item: Arc<T>) {
    if gpu_va != 0 {
        map.entry(gpu_va).or_default().push(item);
    }
}

fn remove_va<T>(map: &mut BTreeMap<u64, Vec<Arc<T>>>, gpu_va: u64, item: &Arc<T>) {
    if gpu_va == 0 {
        return;
    }
    if let Some(list) = map.get_mut(&gpu_va) {
        if let Some(pos) = list.iter().position(|x| Arc::ptr_eq(x, item)) {
            list.remove(pos);
        }
        if list.is_empty() {
            map.remove(&gpu_va);
        }
    }
}

fn lookup_va<T>(map: &BTreeMap<u64, Vec<Arc<T>>>, address: u64) -> Option<(Arc<T>, u64)> {
    let (&base_va, list) = map.range(..=address).next_back()?;
    let item = list.last()?;
    Some((Arc::clone(item), address - base_va))
}

/// Registers `buffer` in the device's Metal residency set (`d3d12_device.cpp:1998`).
pub fn register(device: &Device, buffer: &Arc<Buffer>) {
    let _guard = lock(&device.residency.va_map);
    if let Some(alloc) = buffer.current()
        && !alloc
            .flags()
            .contains(BufferAllocationFlags::ALLOCATED_ON_HEAP)
    {
        add_allocation(device.residency.set.as_ref(), alloc.buffer());
    }
}

/// Registers `buffer` in the residency set and GPU VA lookup table (`d3d12_device.cpp:2009`).
pub fn register_with_va(device: &Device, buffer: &Arc<Buffer>) {
    let mut map = lock(&device.residency.va_map);
    if let Some(alloc) = buffer.current() {
        if !alloc
            .flags()
            .contains(BufferAllocationFlags::ALLOCATED_ON_HEAP)
        {
            add_allocation(device.residency.set.as_ref(), alloc.buffer());
        }
        insert_va(&mut map, alloc.gpu_address(), Arc::clone(buffer));
    }
}

/// Unregisters `buffer` from the residency set and GPU VA lookup table (`d3d12_device.cpp:2017`).
pub fn unregister(device: &Device, buffer: &Arc<Buffer>, gpu_va: u64) {
    let mut map = lock(&device.residency.va_map);
    if let Some(alloc) = buffer.current()
        && !alloc
            .flags()
            .contains(BufferAllocationFlags::ALLOCATED_ON_HEAP)
    {
        remove_allocation(device.residency.set.as_ref(), alloc.buffer());
    }
    remove_va(&mut map, gpu_va, buffer);
}

/// Looks up the newest buffer whose GPU VA range contains `address` (`d3d12_device.cpp:2025`).
///
/// Returns `(buffer, byte_offset)`.
#[must_use]
pub fn lookup_buffer_by_va(device: &Device, address: u64) -> Option<(Arc<Buffer>, u64)> {
    let map = lock(&device.residency.va_map);
    lookup_va(&map, address)
}

/// Registers `texture` in the device's Metal residency set (`d3d12_device.cpp:1998`).
pub fn register_texture(device: &Device, texture: &Arc<Texture>) {
    let _guard = lock(&device.residency.va_map);
    if let Some(alloc) = texture.current()
        && !alloc
            .flags()
            .contains(TextureAllocationFlags::ALLOCATED_ON_HEAP)
    {
        add_allocation(device.residency.set.as_ref(), alloc.texture());
    }
}

/// Unregisters `texture` from the device's Metal residency set (`d3d12_device.cpp:2004`).
pub fn unregister_texture(device: &Device, texture: &Arc<Texture>) {
    let _guard = lock(&device.residency.va_map);
    if let Some(alloc) = texture.current()
        && !alloc
            .flags()
            .contains(TextureAllocationFlags::ALLOCATED_ON_HEAP)
    {
        remove_allocation(device.residency.set.as_ref(), alloc.texture());
    }
}

/// Registers a `MTLHeap` in the device's Metal residency set (`d3d12_device.cpp:1998`).
pub fn register_heap(device: &Device, heap: &Obj) {
    let _guard = lock(&device.residency.va_map);
    add_allocation(device.residency.set.as_ref(), heap.handle());
}

/// Unregisters a `MTLHeap` from the device's Metal residency set (`d3d12_device.cpp:2004`).
pub fn unregister_heap(device: &Device, heap: &Obj) {
    let _guard = lock(&device.residency.va_map);
    remove_allocation(device.residency.set.as_ref(), heap.handle());
}

/// `ID3D12Device::MakeResident` (`d3d12_device.cpp:1264`).
///
/// # Safety
///
/// `this` and `objects` are caller-supplied COM pointers.
pub unsafe fn make_resident(
    this: *mut c_void,
    _num_objects: u32,
    _objects: *const *mut c_void,
) -> Hresult {
    crate::scope!("device.MakeResident");
    let _ = Option::from(this);
    S_OK
}

/// `ID3D12Device::Evict` (`d3d12_device.cpp:1271`).
///
/// # Safety
///
/// `this` and `objects` are caller-supplied COM pointers.
pub unsafe fn evict(this: *mut c_void, _num_objects: u32, _objects: *const *mut c_void) -> Hresult {
    crate::scope!("device.Evict");
    let _ = Option::from(this);
    S_OK
}

/// `ID3D12Device1::SetResidencyPriority` (`d3d12_device.cpp:1495`).
///
/// # Safety
///
/// `this`, `objects`, and `priorities` are caller-supplied pointers.
pub unsafe fn set_residency_priority(
    this: *mut c_void,
    _num_objects: u32,
    _objects: *const *mut c_void,
    _priorities: *const u32,
) -> Hresult {
    crate::scope!("device.SetResidencyPriority");
    let _ = Option::from(this);
    S_OK
}

/// `ID3D12Device3::EnqueueMakeResident` (`d3d12_device.cpp:1554`).
///
/// # Safety
///
/// `this`, `objects`, and `fence` are caller-supplied COM pointers.
pub unsafe fn enqueue_make_resident(
    this: *mut c_void,
    _flags: u32,
    _num_objects: u32,
    _objects: *const *mut c_void,
    fence: *mut c_void,
    fence_value: u64,
) -> Hresult {
    crate::scope!("device.EnqueueMakeResident");
    let _ = this;
    if fence.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `fence` is a non-null `ID3D12Fence` COM interface pointer.
    let vtbl = unsafe { com::vtbl::<ID3D12FenceVtbl>(fence) };
    // SAFETY: `vtbl.signal` is the `ID3D12Fence::Signal` method for `fence`.
    unsafe { (vtbl.signal)(fence, fence_value) }
}

#[cfg(test)]
mod tests;
