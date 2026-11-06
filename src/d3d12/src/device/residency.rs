//! Device residency set and GPU virtual-address lookup (`d3d12_device.cpp:1264-1275, 1888-2038`).

use core::ffi::c_void;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, RwLock},
};

use dxmt_core::resource::{Buffer, BufferAllocationFlags, Texture, TextureAllocationFlags};
use dxmt_util::sync::{lock, read, write};
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
    /// Additions batched until the next command buffer commit, and whether one is owed.
    pub pending: Mutex<PendingResidency>,
    pub va_map: RwLock<BTreeMap<u64, Vec<Arc<Buffer>>>>,
}

/// The additions the next commit sends, and whether the set changed since the last commit.
///
/// Removals are never batched: the caller releases the allocation right after unregistering it,
/// so a removal queued for the next flush would reach `removeAllocations` as a dead handle.
#[derive(Default)]
pub struct PendingResidency {
    add: Vec<u64>,
    dirty: bool,
}

impl PendingResidency {
    /// Queues `allocation` for the next flush.
    fn add(&mut self, allocation: u64) {
        if !self.add.contains(&allocation) {
            self.add.push(allocation);
        }
        self.dirty = true;
    }

    /// Forgets a queued `allocation`.
    ///
    /// `true` when a flush already sent it, so it must leave the Metal set now.
    fn remove(&mut self, allocation: u64) -> bool {
        if let Some(at) = self.add.iter().position(|&a| a == allocation) {
            self.add.swap_remove(at);
            return false;
        }
        self.dirty = true;
        true
    }

    /// The queued additions, which the caller sends now.
    fn take_adds(&mut self) -> Vec<u64> {
        core::mem::take(&mut self.add)
    }
}

impl Residency {
    /// Creates an empty residency table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            set: None,
            pending: Mutex::new(PendingResidency {
                add: Vec::new(),
                dirty: false,
            }),
            va_map: RwLock::new(BTreeMap::new()),
        }
    }
}

impl Default for Residency {
    fn default() -> Self {
        Self::new()
    }
}

fn add_allocation(residency: &Residency, allocation: u64) {
    if residency.set.is_some() && allocation != 0 {
        lock(&residency.pending).add(allocation);
    }
}

fn remove_allocation(residency: &Residency, allocation: u64) {
    if let Some(ref set) = residency.set
        && allocation != 0
    {
        let mut pending = lock(&residency.pending);
        if pending.remove(allocation) {
            // SAFETY: `set` is a live `MTLResidencySet`; `allocation` is live until our caller
            // releases it, after this returns.
            unsafe { MTLResidencySet_removeAllocations(set.handle(), &raw const allocation, 1) };
        }
    }
}

/// Commits any pending residency set additions or removals before command buffer execution.
pub fn flush(device: &Device) {
    let Some(ref set) = device.residency.set else {
        return;
    };
    let h = set.handle();
    let mut pending = lock(&device.residency.pending);
    if !pending.dirty {
        return;
    }
    pending.dirty = false;
    let add = pending.take_adds();
    if !add.is_empty() {
        // Under the lock, so a removal of one of these cannot run before it is added.
        // SAFETY: `h` is a live `MTLResidencySet` and `add` holds live allocation handles
        // (a released one was taken out by its removal).
        unsafe { MTLResidencySet_addAllocations(h, add.as_ptr(), add.len() as u64) };
    }
    drop(pending);
    // SAFETY: `h` is a live `MTLResidencySet` handle.
    unsafe { MTLResidencySet_commit(h) };
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
    if let Some(alloc) = buffer.current()
        && !alloc
            .flags()
            .contains(BufferAllocationFlags::ALLOCATED_ON_HEAP)
    {
        add_allocation(&device.residency, alloc.buffer());
    }
}

/// Registers `buffer` in the residency set and GPU VA lookup table (`d3d12_device.cpp:2009`).
pub fn register_with_va(device: &Device, buffer: &Arc<Buffer>) {
    if let Some(alloc) = buffer.current() {
        let va = alloc.gpu_address();
        let buf_handle = if alloc
            .flags()
            .contains(BufferAllocationFlags::ALLOCATED_ON_HEAP)
        {
            0
        } else {
            alloc.buffer()
        };
        drop(alloc);
        if buf_handle != 0 {
            add_allocation(&device.residency, buf_handle);
        }
        let mut map = write(&device.residency.va_map);
        insert_va(&mut map, va, Arc::clone(buffer));
    }
}

/// Unregisters `buffer` from the residency set and GPU VA lookup table (`d3d12_device.cpp:2017`).
pub fn unregister(device: &Device, buffer: &Arc<Buffer>, gpu_va: u64) {
    if let Some(alloc) = buffer.current()
        && !alloc
            .flags()
            .contains(BufferAllocationFlags::ALLOCATED_ON_HEAP)
    {
        remove_allocation(&device.residency, alloc.buffer());
    }
    let mut map = write(&device.residency.va_map);
    remove_va(&mut map, gpu_va, buffer);
}

/// Looks up the newest buffer whose GPU VA range contains `address` (`d3d12_device.cpp:2025`).
///
/// Returns `(buffer, byte_offset)`.
#[must_use]
pub fn lookup_buffer_by_va(device: &Device, address: u64) -> Option<(Arc<Buffer>, u64)> {
    let map = read(&device.residency.va_map);
    lookup_va(&map, address)
}

/// Registers `texture` in the device's Metal residency set (`d3d12_device.cpp:1998`).
pub fn register_texture(device: &Device, texture: &Arc<Texture>) {
    if let Some(alloc) = texture.current()
        && !alloc
            .flags()
            .contains(TextureAllocationFlags::ALLOCATED_ON_HEAP)
    {
        add_allocation(&device.residency, alloc.texture());
    }
}

/// Unregisters `texture` from the device's Metal residency set (`d3d12_device.cpp:2004`).
pub fn unregister_texture(device: &Device, texture: &Arc<Texture>) {
    if let Some(alloc) = texture.current()
        && !alloc
            .flags()
            .contains(TextureAllocationFlags::ALLOCATED_ON_HEAP)
    {
        remove_allocation(&device.residency, alloc.texture());
    }
}

/// Registers a `MTLHeap` in the device's Metal residency set (`d3d12_device.cpp:1998`).
pub fn register_heap(device: &Device, heap: &Obj) {
    add_allocation(&device.residency, heap.handle());
}

/// Unregisters a `MTLHeap` from the device's Metal residency set (`d3d12_device.cpp:2004`).
pub fn unregister_heap(device: &Device, heap: &Obj) {
    remove_allocation(&device.residency, heap.handle());
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
