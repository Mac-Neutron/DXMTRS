//! `ID3D12QueryHeap` (`d3d12_query_heap.cpp`): occlusion, timestamp, and statistics query heaps.

use core::{ffi::c_void, mem::size_of, ptr};
use std::sync::{LazyLock, Mutex, PoisonError};

use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::{
    MTLCounterSampleBuffer_newTimestampBuffer, MTLDevice_newBuffer, Obj,
    wire::{MemPtr, WMT_RESOURCE_STORAGE_MODE_SHARED, WMTBufferInfo},
};

use crate::{
    abi::{
        objects::{
            D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP, D3D12_QUERY_HEAP_TYPE_OCCLUSION,
            D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS, D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS1,
            D3D12_QUERY_HEAP_TYPE_SO_STATISTICS, D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
            D3D12_QUERY_HEAP_TYPE_VIDEO_DECODE_STATISTICS, QueryDataPipelineStatistics,
            QueryDataSoStatistics, QueryHeapDesc,
        },
        vtbl::{ID3D12QueryHeapVtbl, IUnknownVtbl},
    },
    com::{
        self, ComRefCounted, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_OUTOFMEMORY, E_POINTER, Guid,
        Hresult, IID_ID3D12_DEVICE_CHILD, IID_ID3D12_OBJECT, IID_ID3D12_PAGEABLE,
        IID_ID3D12_QUERY_HEAP, IID_IUNKNOWN, S_FALSE, S_OK,
    },
    device::Device,
    slot,
};

/// Metal counter sample buffer capacity (`kTimestampsPerBuffer`, `d3d12_device.hpp:238`).
pub const TIMESTAMPS_PER_BUFFER: u32 = 4096;

/// `MTLD3D12QueryHeapImpl` (`d3d12_query_heap.cpp:28`, `d3d12_device.hpp:222-236`).
#[repr(C)]
pub struct QueryHeap {
    pub vtbl: &'static ID3D12QueryHeapVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub results: Option<Obj>,
    pub stride: u32,
    pub counters: Vec<Obj>,
    pub aliases: Mutex<Vec<u32>>,
    pub values: Vec<u64>,
}

impl ComRefCounted for QueryHeap {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for QueryHeap {
    fn drop(&mut self) {
        if !self.device.is_null() {
            // SAFETY: `self.device` was retained with `add_ref_com` when `self` was created.
            unsafe { com::release_com(self.device) };
        }
    }
}

impl QueryHeap {
    /// Allocates a timestamp sample slot for `target` (`AllocateTimestamp`,
    ///
    /// `d3d12_query_heap.cpp:117`).
    #[must_use]
    pub fn allocate_timestamp(&self, target: u32) -> (Option<Obj>, u32) {
        let mut aliases = self.aliases.lock().unwrap_or_else(PoisonError::into_inner);
        let t = target as usize;
        if let Some(slot) = aliases.get_mut(t) {
            *slot += 1;
            if *slot == 2 {
                crate::warn!("D3D12 QueryHeap: Timestamp slot {target} overwritten before resolve");
            }
        }
        let index = u32::try_from(aliases.len()).unwrap_or(0);
        aliases.push(target);
        drop(aliases);
        let buf_idx = (index / TIMESTAMPS_PER_BUFFER) as usize;
        let counter = self
            .counters
            .get(buf_idx)
            // SAFETY: `o` is a live `MTLCounterSampleBuffer` owned by `self.counters`.
            .and_then(|o| unsafe { Obj::retain(o.handle()) });
        (counter, index % TIMESTAMPS_PER_BUFFER)
    }
}

const PIPELINE_STATS_STRIDE: u32 = 88;
const SO_STATS_STRIDE: u32 = 16;
const U64_STRIDE: u32 = 8;
const _: [(); size_of::<QueryDataPipelineStatistics>()] = [(); PIPELINE_STATS_STRIDE as usize];
const _: [(); size_of::<QueryDataSoStatistics>()] = [(); SO_STATS_STRIDE as usize];
const _: [(); size_of::<u64>()] = [(); U64_STRIDE as usize];

/// Returns the byte stride per query element for `heap_type`, or `None` if unsupported.
#[must_use]
pub const fn query_stride(heap_type: u32) -> Option<u32> {
    match heap_type {
        D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS | D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS1 => {
            Some(PIPELINE_STATS_STRIDE)
        }
        D3D12_QUERY_HEAP_TYPE_SO_STATISTICS => Some(SO_STATS_STRIDE),
        D3D12_QUERY_HEAP_TYPE_OCCLUSION
        | D3D12_QUERY_HEAP_TYPE_TIMESTAMP
        | D3D12_QUERY_HEAP_TYPE_VIDEO_DECODE_STATISTICS
        | D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP => Some(U64_STRIDE),
        _ => None,
    }
}

/// Returns whether `DXMT_D3D12_TIMESTAMP_BLITS=1` is set (`TimestampBlits`,
///
/// `d3d12_device.hpp:242`).
#[must_use]
pub fn timestamp_blits() -> bool {
    static ON: LazyLock<bool> =
        LazyLock::new(|| std::env::var("DXMT_D3D12_TIMESTAMP_BLITS").as_deref() == Ok("1"));
    *ON
}

static QUERY_HEAP_VTBL: ID3D12QueryHeapVtbl = ID3D12QueryHeapVtbl {
    query_interface: slot!((this: *mut c_void, riid: *const c_void, ppv_object: *mut *mut c_void) -> Hresult => {
        if ppv_object.is_null() {
            return E_POINTER;
        }
        // SAFETY: `ppv_object` is non-null.
        unsafe { com::init_out_ptr(ppv_object) };
        // SAFETY: `riid` is a caller-supplied GUID pointer.
        let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        if iid == IID_IUNKNOWN
            || iid == IID_ID3D12_OBJECT
            || iid == IID_ID3D12_DEVICE_CHILD
            || iid == IID_ID3D12_PAGEABLE
            || iid == IID_ID3D12_QUERY_HEAP
        {
            // SAFETY: `this` is a live `QueryHeap`.
            unsafe { com::add_ref_com(this.cast::<QueryHeap>()) };
            // SAFETY: `ppv_object` is non-null and writable.
            unsafe { ppv_object.write_unaligned(this) };
            return S_OK;
        }
        if com::qi_log(&IID_ID3D12_QUERY_HEAP, &iid) {
            crate::warn!("ID3D12QueryHeap::QueryInterface: Unknown interface query {iid}");
        }
        E_NOINTERFACE
    }),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `QueryHeap`.
        unsafe { com::add_ref_com(this.cast::<QueryHeap>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `QueryHeap`.
        unsafe { com::release_com(this.cast::<QueryHeap>()) }
    }),
    get_private_data: slot!((this: *mut c_void, guid: *const c_void, data_size: *mut u32, data: *mut c_void) -> Hresult => {
        // SAFETY: `guid` is a caller-supplied GUID pointer.
        let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: `this` is a live `QueryHeap`.
        let qh = unsafe { &*this.cast::<QueryHeap>() };
        // SAFETY: `data_size` and `data` are caller-supplied.
        unsafe { qh.private_data.get_data(guid, data_size, data) }
    }),
    set_private_data: slot!((this: *mut c_void, guid: *const c_void, data_size: u32, data: *const c_void) -> Hresult => {
        // SAFETY: `guid` is a caller-supplied GUID pointer.
        let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: `this` is a live `QueryHeap`.
        let qh = unsafe { &*this.cast::<QueryHeap>() };
        // SAFETY: `data` is caller-supplied.
        unsafe { qh.private_data.set_data(guid, data_size, data) }
    }),
    set_private_data_interface: slot!((this: *mut c_void, guid: *const c_void, data: *const c_void) -> Hresult => {
        // SAFETY: `guid` is a caller-supplied GUID pointer.
        let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: `this` is a live `QueryHeap`.
        let qh = unsafe { &*this.cast::<QueryHeap>() };
        // SAFETY: `data` is caller-supplied.
        unsafe { qh.private_data.set_interface(guid, data.cast_mut()) }
    }),
    set_name: slot!((this: *mut c_void, name: *const c_void) -> Hresult => {
        // SAFETY: `this` is a live `QueryHeap`.
        let qh = unsafe { &*this.cast::<QueryHeap>() };
        // SAFETY: `name` is null or a NUL-terminated UTF-16 string.
        unsafe { com::set_name_private_data(&qh.private_data, name.cast::<u16>()) }
    }),
    get_device: slot!((this: *mut c_void, riid: *const c_void, ppv_device: *mut *mut c_void) -> Hresult => {
        // SAFETY: `this` is a live `QueryHeap`.
        let dev = unsafe { (*this.cast::<QueryHeap>()).device }.cast::<c_void>();
        if dev.is_null() {
            return E_INVALIDARG;
        }
        // SAFETY: `dev` is a live `Device` whose vtable starts with `IUnknownVtbl`.
        let unk = unsafe { com::vtbl::<IUnknownVtbl>(dev) };
        // SAFETY: `dev` is live and `riid`/`ppv_device` are forwarded from the caller.
        unsafe { (unk.query_interface)(dev, riid, ppv_device) }
    }),
};

/// Casts a non-null `ID3D12QueryHeap` pointer if its vtable matches [`QueryHeap`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_query_heap<'a>(raw: *mut c_void) -> Option<&'a QueryHeap> {
    // SAFETY: forwarded from caller; `QUERY_HEAP_VTBL` uniquely identifies `QueryHeap`.
    unsafe { com::cast_com::<QueryHeap, ID3D12QueryHeapVtbl>(raw, &QUERY_HEAP_VTBL) }
}

const fn zero_mem_ptr() -> MemPtr {
    #[cfg(target_pointer_width = "64")]
    {
        MemPtr { ptr: 0 }
    }
    #[cfg(target_pointer_width = "32")]
    {
        MemPtr { ptr: 0, high: 0 }
    }
}

fn mem_ptr_to_mut(mem: MemPtr) -> *mut u8 {
    usize::try_from(mem.ptr).map_or(ptr::null_mut(), ptr::with_exposed_provenance_mut)
}

/// `ID3D12Device::CreateQueryHeap` (`d3d12_device.cpp:1443`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `heap` are caller-supplied COM pointers.
pub unsafe fn create_query_heap(
    this: *mut c_void,
    desc: *const c_void,
    riid: *const c_void,
    heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateQueryHeap");
    // SAFETY: `heap` is null or writable.
    unsafe { com::init_out_ptr(heap) };
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null and points to a caller-supplied `D3D12_QUERY_HEAP_DESC`.
    let d = unsafe { desc.cast::<QueryHeapDesc>().read_unaligned() };
    let Some(stride) = query_stride(d.r#type) else {
        return E_INVALIDARG;
    };
    if heap.is_null() {
        return S_FALSE;
    }

    let device = this.cast::<Device>();
    // SAFETY: if `device` is non-null, it points to a live `Device`.
    let mtl = unsafe { device.as_ref().and_then(|dev| dev.mtl.as_ref()) };

    let mut results = None;
    let mut counters = Vec::new();
    let mut aliases = Vec::new();
    let mut values = Vec::new();

    if let Some(mtl_dev) = mtl {
        let byte_len = u64::from(d.count.max(1)) * u64::from(stride);
        let mut info = WMTBufferInfo {
            length: byte_len,
            options: WMT_RESOURCE_STORAGE_MODE_SHARED,
            memory: zero_mem_ptr(),
            gpu_address: 0,
        };
        // SAFETY: `mtl_dev` is a live `MTLDevice` and `info` is a valid buffer descriptor.
        let buf_handle = unsafe { MTLDevice_newBuffer(mtl_dev.handle(), &raw mut info) };
        // SAFETY: `newBuffer` returns a +1 object handle or 0.
        let Some(buf_obj) = (unsafe { Obj::adopt(buf_handle) }) else {
            return E_OUTOFMEMORY;
        };
        let mapped = mem_ptr_to_mut(info.memory);
        if !mapped.is_null()
            && let Ok(len) = usize::try_from(byte_len)
        {
            // SAFETY: `mapped` points to `len` writable bytes owned by `buf_obj`.
            unsafe { ptr::write_bytes(mapped, 0, len) };
        }
        results = Some(buf_obj);

        let is_timestamp = d.r#type == D3D12_QUERY_HEAP_TYPE_TIMESTAMP
            || d.r#type == D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP;
        if is_timestamp {
            if timestamp_blits() {
                aliases = vec![0u32; d.count as usize];
                let num_buffers = d.count.div_ceil(TIMESTAMPS_PER_BUFFER).max(1);
                counters.reserve(num_buffers as usize);
                for _ in 0..num_buffers {
                    // SAFETY: `mtl_dev` is a live `MTLDevice`.
                    let csb_handle = unsafe {
                        MTLCounterSampleBuffer_newTimestampBuffer(
                            mtl_dev.handle(),
                            TIMESTAMPS_PER_BUFFER,
                            true,
                        )
                    };
                    // SAFETY: `newTimestampBuffer` returns a +1 handle or 0.
                    let Some(csb_obj) = (unsafe { Obj::adopt(csb_handle) }) else {
                        crate::err!("CreateQueryHeap: Failed to create MTLCounterSampleBuffer");
                        return E_FAIL;
                    };
                    counters.push(csb_obj);
                }
            } else {
                values = vec![0u64; d.count as usize];
            }
        }
    }

    if !device.is_null() {
        // SAFETY: `device` is a live `Device`.
        unsafe { com::add_ref_com(device) };
    }
    let obj = Box::into_raw(Box::new(QueryHeap {
        vtbl: &QUERY_HEAP_VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device,
        results,
        stride,
        counters,
        aliases: Mutex::new(aliases),
        values,
    }));
    // SAFETY: `obj` is a newly allocated `QueryHeap`; `riid` and `heap` are caller-supplied.
    let hr = unsafe { (QUERY_HEAP_VTBL.query_interface)(obj.cast(), riid, heap) };
    // SAFETY: Release the initial creation reference (`query_interface` added one on `S_OK`).
    unsafe { com::release_com(obj) };
    hr
}

#[cfg(test)]
mod tests;
