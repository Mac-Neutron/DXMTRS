//! `ID3D12Heap` (`d3d12_heap.cpp`): Metal heaps and native hooked CPU heaps.

use core::{ffi::c_void, ptr};
use std::sync::{Arc, LazyLock, Mutex};

use dxmt_core::resource::Buffer;
#[cfg(not(windows))]
use dxmt_core::resource::BufferAllocationFlags;
use dxmt_util::{
    com::ComObject,
    private_data::{self, HasPrivateData, PrivateData},
};
use rustc_hash::FxHashMap;
use winemetal_sys::Obj;
use winemetal_wire::{
    WMT_HEAP_TYPE_PLACEMENT, WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_RESOURCE_STORAGE_MODE_SHARED,
    WMT_SPARSE_PAGE_SIZE16, WMTHeapInfo,
};

use crate::{
    abi::{
        barrier::{
            D3D12_HEAP_TYPE_CUSTOM, D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_TYPE_READBACK,
            D3D12_HEAP_TYPE_UPLOAD,
        },
        objects::{D3D12_HEAP_FLAG_ALLOW_DISPLAY, HeapDesc},
        resource::{
            D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE, D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT,
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT, D3D12_HEAP_FLAG_ALLOW_WRITE_WATCH,
            D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER, D3D12_MEMORY_POOL_L1, D3D12_MEMORY_POOL_UNKNOWN,
            HeapProperties,
        },
        vtbl::{ID3D12HeapVtbl, IUnknownVtbl},
    },
    com::{
        self, ComRefCounted, E_INVALIDARG, E_NOTIMPL, E_OUTOFMEMORY, Hresult,
        IID_ID3D12_DEVICE_CHILD, IID_ID3D12_HEAP, IID_ID3D12_OBJECT, IID_ID3D12_PAGEABLE,
        IID_IUNKNOWN, S_FALSE, S_OK, init_out_ptr,
    },
    device::Device,
};

static HEAP_DESCS: LazyLock<Mutex<FxHashMap<usize, HeapDesc>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

/// `MTLD3D12HeapImpl` (`d3d12_heap.cpp:26`, `d3d12_device.hpp:167-177`).
#[repr(C)]
pub struct HeapObject {
    pub vtbl: &'static ID3D12HeapVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub heap: Option<Obj>,
    pub cpu_buffer: Option<Arc<Buffer>>,
    pub cpu_memory: *mut c_void,
}

impl ComRefCounted for HeapObject {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for HeapObject {
    fn drop(&mut self) {
        crate::count!("#heaps destroyed", 1);
        let addr = ptr::from_ref(self).addr();
        if let Ok(mut map) = HEAP_DESCS.lock() {
            map.remove(&addr);
        }
        if let Some(ref h) = self.heap
            && !self.device.is_null()
        {
            // SAFETY: `self.device` is retained for the lifetime of `self`.
            let dev_ref = unsafe { &*self.device };
            crate::device::residency::unregister_heap(dev_ref, h);
        }
        if !self.device.is_null() {
            // SAFETY: `self.device` was retained when `HeapObject` was created.
            unsafe { com::release_com(self.device) };
        }
    }
}

impl HasPrivateData for HeapObject {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `HeapObject`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

static HEAP_VTBL: ID3D12HeapVtbl = ID3D12HeapVtbl {
    query_interface: heap_query_interface,
    add_ref: heap_add_ref,
    release: heap_release,
    get_private_data: private_data::get_private_data::<HeapObject>,
    set_private_data: private_data::set_private_data::<HeapObject>,
    set_private_data_interface: private_data::set_private_data_interface::<HeapObject>,
    set_name: private_data::set_name::<HeapObject>,
    get_device: heap_get_device,
    get_desc: heap_get_desc,
};

unsafe extern "system-unwind" fn heap_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv: *mut *mut c_void,
) -> Hresult {
    crate::scope!("memheap.QueryInterface");
    if this.is_null() {
        // SAFETY: the out pointer is null or writable.
        return unsafe { com::qi_refuse(ppv, E_INVALIDARG) };
    }
    let qi = com::Qi {
        null_riid: E_INVALIDARG,
        silent: &[],
        log: &crate::LOG,
        miss: com::QiMiss::Quiet,
    };
    let add_ref = || {
        // SAFETY: `this` is a live `HeapObject`.
        unsafe { com::add_ref_com(this.cast::<HeapObject>()) };
    };
    // SAFETY: the application's `riid` and out pointer, null or valid; `this` is a live `HeapObject`.
    unsafe {
        com::qi_table(
            riid,
            ppv,
            &[(
                &[
                    IID_IUNKNOWN,
                    IID_ID3D12_OBJECT,
                    IID_ID3D12_DEVICE_CHILD,
                    IID_ID3D12_PAGEABLE,
                    IID_ID3D12_HEAP,
                ],
                this,
            )],
            add_ref,
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn heap_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `HeapObject` COM pointer.
    unsafe { com::add_ref_com(this.cast::<HeapObject>()) }
}

unsafe extern "system-unwind" fn heap_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `HeapObject` COM pointer allocated via `Box::into_raw`.
    unsafe { com::release_com(this.cast::<HeapObject>()) }
}

unsafe extern "system-unwind" fn heap_get_device(
    this: *mut c_void,
    riid: *const c_void,
    device: *mut *mut c_void,
) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    let obj = this.cast::<HeapObject>();
    // SAFETY: `obj` is a valid `HeapObject` COM pointer.
    let dev = unsafe { (*obj).device }.cast::<c_void>();
    if dev.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `dev` is a live `Device` whose vtable starts with `IUnknownVtbl`.
    let unk = unsafe { com::vtbl::<IUnknownVtbl>(dev) };
    // SAFETY: `dev`, `riid`, and `device` are forwarded to `QueryInterface`.
    unsafe { (unk.query_interface)(dev, riid, device) }
}

unsafe extern "system-unwind" fn heap_get_desc(this: *mut c_void, ret: *mut c_void) -> *mut c_void {
    crate::scope!("memheap.GetDesc");
    if !this.is_null() && !ret.is_null() {
        let addr = this.addr();
        let desc = HEAP_DESCS
            .lock()
            .ok()
            .and_then(|map| map.get(&addr).map(HeapDesc::copy))
            .unwrap_or_else(HeapDesc::empty);
        // SAFETY: `ret` is caller-provided storage for `D3D12_HEAP_DESC`.
        unsafe { *ret.cast::<HeapDesc>() = desc };
    }
    ret
}

const fn is_cpu_visible_heap(props: &HeapProperties) -> bool {
    match props.r#type {
        D3D12_HEAP_TYPE_UPLOAD | D3D12_HEAP_TYPE_READBACK => true,
        D3D12_HEAP_TYPE_CUSTOM => props.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE,
        _ => false,
    }
}

const fn is_private_heap(props: &HeapProperties, private_on: bool) -> bool {
    private_on && !is_cpu_visible_heap(props)
}

const fn align_up(value: u64, alignment: u64) -> u64 {
    if value == 0 {
        0
    } else {
        value.wrapping_add(alignment - 1) & !(alignment - 1)
    }
}

const fn validate_heap_properties(
    props: &HeapProperties,
    flags: u32,
    advertise_numa: bool,
) -> Hresult {
    match props.r#type {
        D3D12_HEAP_TYPE_DEFAULT | D3D12_HEAP_TYPE_READBACK => {
            if (flags & D3D12_HEAP_FLAG_ALLOW_WRITE_WATCH) != 0 {
                return E_INVALIDARG;
            }
            if props.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_UNKNOWN
                || props.memory_pool_preference != D3D12_MEMORY_POOL_UNKNOWN
            {
                return E_INVALIDARG;
            }
        }
        D3D12_HEAP_TYPE_UPLOAD => {
            if props.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_UNKNOWN
                || props.memory_pool_preference != D3D12_MEMORY_POOL_UNKNOWN
            {
                return E_INVALIDARG;
            }
        }
        D3D12_HEAP_TYPE_CUSTOM => {
            if props.cpu_page_property == D3D12_CPU_PAGE_PROPERTY_UNKNOWN
                || props.memory_pool_preference == D3D12_MEMORY_POOL_UNKNOWN
            {
                return E_INVALIDARG;
            }
            if props.memory_pool_preference == D3D12_MEMORY_POOL_L1
                && advertise_numa
                && props.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE
            {
                return E_INVALIDARG;
            }
        }
        _ => return E_INVALIDARG,
    }

    if props.memory_pool_preference == D3D12_MEMORY_POOL_L1
        && (!advertise_numa || props.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE)
    {
        return E_INVALIDARG;
    }

    if (flags & D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER) != 0 && is_cpu_visible_heap(props) {
        return E_INVALIDARG;
    }

    S_OK
}

const fn normalize_heap_desc(desc: &HeapDesc) -> Result<(HeapDesc, u64), Hresult> {
    if (desc.flags & D3D12_HEAP_FLAG_ALLOW_DISPLAY) != 0 {
        return Err(E_INVALIDARG);
    }
    let alignment = match desc.alignment {
        0 | D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT => {
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT
        }
        D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT => {
            D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT
        }
        _ => return Err(E_INVALIDARG),
    };
    let size_aligned = align_up(desc.size_in_bytes, alignment);
    if size_aligned == 0 {
        return Err(E_INVALIDARG);
    }
    let mut normalized = desc.copy();
    normalized.properties.creation_node_mask = 1;
    normalized.properties.visible_node_mask = 1;
    normalized.alignment = alignment;
    Ok((normalized, size_aligned))
}

fn alloc_heap_box(
    device: *mut Device,
    normalized: HeapDesc,
    heap: Option<Obj>,
    cpu_buffer: Option<Arc<Buffer>>,
    cpu_memory: *mut c_void,
) -> *mut HeapObject {
    if !device.is_null() {
        // SAFETY: `device` is a valid `Device` pointer; retain it for the heap's lifetime.
        let _ = unsafe { (*device).com.add_ref() };
    }
    let raw = Box::into_raw(Box::new(HeapObject {
        vtbl: &HEAP_VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device,
        heap,
        cpu_buffer,
        cpu_memory,
    }));
    if let Ok(mut map) = HEAP_DESCS.lock() {
        map.insert(raw.addr(), normalized);
    }
    raw
}

/// Casts a non-null `ID3D12Heap` pointer if its vtable matches [`HeapObject`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_heap<'a>(raw: *mut c_void) -> Option<&'a HeapObject> {
    if raw.is_null() {
        return None;
    }
    let obj = raw.cast::<HeapObject>();
    // SAFETY: `raw` is non-null and points to a COM object whose first word is its vtable pointer.
    let vtbl = unsafe { (*obj).vtbl };
    if ptr::eq(vtbl, &raw const HEAP_VTBL) {
        // SAFETY: Vtable identity confirms `obj` is a live `HeapObject`.
        Some(unsafe { &*obj })
    } else {
        None
    }
}

/// `ID3D12Device::CreateHeap` (`d3d12_device.cpp:1192`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `heap` are caller-supplied COM pointers.
pub unsafe fn create_heap(
    this: *mut c_void,
    desc: *const c_void,
    riid: *const c_void,
    heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateHeap");
    // SAFETY: `heap` is null or a valid out-pointer.
    unsafe { init_out_ptr(heap) };
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` was checked non-null above.
    let desc_ref = unsafe { &*desc.cast::<HeapDesc>() };
    let advertise_numa = if this.is_null() {
        false
    } else {
        // SAFETY: `this` is a valid `Device` pointer.
        unsafe { (*this.cast::<Device>()).advertise_numa }
    };
    let hr_props = validate_heap_properties(&desc_ref.properties, desc_ref.flags, advertise_numa);
    if hr_props < 0 {
        return hr_props;
    }
    let (normalized, size_aligned) = match normalize_heap_desc(desc_ref) {
        Ok(v) => v,
        Err(hr) => return hr,
    };
    if this.is_null() {
        return if heap.is_null() {
            S_FALSE
        } else {
            E_INVALIDARG
        };
    }
    let dev_ptr = this.cast::<Device>();
    // SAFETY: `dev_ptr` is a valid `Device` pointer.
    let dev_ref = unsafe { &*dev_ptr };
    let Some(ref mtl) = dev_ref.mtl else {
        return if heap.is_null() {
            S_FALSE
        } else {
            E_OUTOFMEMORY
        };
    };
    let is_private = is_private_heap(
        &normalized.properties,
        crate::resource::private_heap_enabled(),
    );

    #[cfg(not(windows))]
    {
        // SAFETY: `WMTMemoryHooked` has no preconditions.
        let hooked = unsafe { winemetal_sys::WMTMemoryHooked() } != 0;
        if !is_private && hooked {
            let buf = Arc::new(Buffer::new(size_aligned, mtl));
            let alloc = buf.allocate(BufferAllocationFlags::empty());
            let mapped = alloc.mapped_memory(0);
            let _ = buf.rename(alloc);
            if mapped.is_null() {
                return E_OUTOFMEMORY;
            }
            let raw = alloc_heap_box(dev_ptr, normalized, None, Some(buf), mapped.cast());
            if heap.is_null() {
                // SAFETY: Drop the temporary heap when probing with a null out-pointer.
                let _ = unsafe { heap_release(raw.cast()) };
                return S_FALSE;
            }
            // SAFETY: `raw`, `riid`, and `heap` are valid for `QueryInterface`.
            let hr = unsafe { heap_query_interface(raw.cast(), riid, heap) };
            // SAFETY: Release the initial reference from `with_initial_ref`.
            let _ = unsafe { heap_release(raw.cast()) };
            return hr;
        }
    }

    let mut info = WMTHeapInfo {
        size: size_aligned,
        options: if is_private {
            WMT_RESOURCE_STORAGE_MODE_PRIVATE
        } else {
            WMT_RESOURCE_STORAGE_MODE_SHARED
        },
        r#type: WMT_HEAP_TYPE_PLACEMENT,
        sparse_page_size: WMT_SPARSE_PAGE_SIZE16,
    };
    // SAFETY: `mtl.handle()` and `&raw mut info` are valid for `MTLDevice_newHeap`.
    let handle = unsafe { winemetal_sys::MTLDevice_newHeap(mtl.handle(), &raw mut info) };
    // SAFETY: `newHeap` returns a +1 object handle or 0.
    let Some(mtl_heap) = (unsafe { Obj::adopt(handle) }) else {
        return E_OUTOFMEMORY;
    };
    crate::device::residency::register_heap(dev_ref, &mtl_heap);
    let raw = alloc_heap_box(dev_ptr, normalized, Some(mtl_heap), None, ptr::null_mut());
    if heap.is_null() {
        // SAFETY: Drop the temporary heap when probing with a null out-pointer.
        let _ = unsafe { heap_release(raw.cast()) };
        return S_FALSE;
    }
    // SAFETY: `raw`, `riid`, and `heap` are valid for `QueryInterface`.
    let hr = unsafe { heap_query_interface(raw.cast(), riid, heap) };
    // SAFETY: Release the initial reference from `with_initial_ref`.
    let _ = unsafe { heap_release(raw.cast()) };
    hr
}

/// `ID3D12Device3::OpenExistingHeapFromAddress` (`d3d12_device.cpp:1536`).
///
/// # Safety
///
/// `this`, `address`, `riid`, and `heap` are caller-supplied pointers.
pub unsafe fn open_existing_heap_from_address(
    this: *mut c_void,
    _address: *const c_void,
    _riid: *const c_void,
    heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.OpenExistingHeapFromAddress");
    let _ = this.expose_provenance();
    // SAFETY: `heap` is null or a valid out-pointer.
    unsafe { init_out_ptr(heap) };
    E_NOTIMPL
}

/// `ID3D12Device3::OpenExistingHeapFromFileMapping` (`d3d12_device.cpp:1545`).
///
/// # Safety
///
/// `this`, `file_mapping`, `riid`, and `heap` are caller-supplied pointers.
pub unsafe fn open_existing_heap_from_file_mapping(
    this: *mut c_void,
    _file_mapping: *mut c_void,
    _riid: *const c_void,
    heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.OpenExistingHeapFromFileMapping");
    let _ = this.expose_provenance();
    // SAFETY: `heap` is null or a valid out-pointer.
    unsafe { init_out_ptr(heap) };
    E_NOTIMPL
}

/// `ID3D12Device4::CreateHeap1` (`d3d12_device.cpp:1612`).
///
/// # Safety
///
/// `this`, `desc`, `protected_session`, `riid`, and `heap` are caller-supplied COM pointers.
pub unsafe fn create_heap1(
    this: *mut c_void,
    desc: *const c_void,
    protected_session: *mut c_void,
    riid: *const c_void,
    heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateHeap1");
    if !protected_session.is_null() {
        // SAFETY: `heap` is null or a valid out-pointer.
        unsafe { init_out_ptr(heap) };
        return E_NOTIMPL;
    }
    // SAFETY: Caller pointers are forwarded to `create_heap`.
    unsafe { create_heap(this, desc, riid, heap) }
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum HeapAllocChoice {
    NativeCpuBuffer { aligned_size: u64 },
    MetalHeap { size: u64, options: u64 },
}

#[cfg(test)]
fn plan_heap_allocation(desc: &HeapDesc, private_on: bool, memory_hooked: bool) -> HeapAllocChoice {
    let (normalized, size_aligned) =
        normalize_heap_desc(desc).expect("valid heap descriptor for plan_heap_allocation");
    let is_private = is_private_heap(&normalized.properties, private_on);
    if !is_private && memory_hooked {
        HeapAllocChoice::NativeCpuBuffer {
            aligned_size: size_aligned,
        }
    } else {
        HeapAllocChoice::MetalHeap {
            size: size_aligned,
            options: if is_private {
                WMT_RESOURCE_STORAGE_MODE_PRIVATE
            } else {
                WMT_RESOURCE_STORAGE_MODE_SHARED
            },
        }
    }
}

#[cfg(test)]
fn create_cpu_heap_with_hook<F>(desc: &HeapDesc, alloc_cpu: F) -> Result<*mut HeapObject, Hresult>
where
    F: FnOnce(u64) -> (Option<Arc<Buffer>>, *mut c_void),
{
    let hr_props = validate_heap_properties(&desc.properties, desc.flags, false);
    if hr_props < 0 {
        return Err(hr_props);
    }
    let (normalized, size_aligned) = normalize_heap_desc(desc)?;
    let (cpu_buffer, cpu_memory) = alloc_cpu(size_aligned);
    if cpu_memory.is_null() {
        return Err(E_OUTOFMEMORY);
    }
    Ok(alloc_heap_box(
        ptr::null_mut(),
        normalized,
        None,
        cpu_buffer,
        cpu_memory,
    ))
}

#[cfg(test)]
mod tests;
