//! `ID3D12QueryHeap` (`d3d12_query_heap.cpp`): occlusion, timestamp, and statistics query heaps.

use core::ffi::c_void;
use std::sync::Mutex;

use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::Obj;

use crate::{
    abi::vtbl::ID3D12QueryHeapVtbl,
    com::{ComRefCounted, Hresult},
    device::Device,
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

/// Returns whether `DXMT_D3D12_TIMESTAMP_BLITS=1` is set (`TimestampBlits`,
///
/// `d3d12_device.hpp:242`).
#[must_use]
pub fn timestamp_blits() -> bool {
    crate::not_implemented!("query_heap::timestamp_blits")
}

/// Casts a non-null `ID3D12QueryHeap` pointer if its vtable matches [`QueryHeap`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_query_heap<'a>(raw: *mut c_void) -> Option<&'a QueryHeap> {
    let _ = raw;
    crate::not_implemented!("query_heap::cast_query_heap")
}

/// `ID3D12Device::CreateQueryHeap` (`d3d12_device.cpp:1443`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `heap` are caller-supplied COM pointers.
pub unsafe fn create_query_heap(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateQueryHeap");
    crate::not_implemented!("ID3D12Device::CreateQueryHeap")
}
