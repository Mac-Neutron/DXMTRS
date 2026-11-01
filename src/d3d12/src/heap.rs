//! `ID3D12Heap` (`d3d12_heap.cpp`): Metal heaps and native hooked CPU heaps.

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::resource::Buffer;
use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::Obj;

use crate::{
    abi::vtbl::ID3D12HeapVtbl,
    com::{ComRefCounted, Hresult},
    device::Device,
};

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

/// Casts a non-null `ID3D12Heap` pointer if its vtable matches [`HeapObject`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_heap<'a>(raw: *mut c_void) -> Option<&'a HeapObject> {
    let _ = raw;
    crate::not_implemented!("heap::cast_heap")
}

/// `ID3D12Device::CreateHeap` (`d3d12_device.cpp:1192`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `heap` are caller-supplied COM pointers.
pub unsafe fn create_heap(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateHeap");
    crate::not_implemented!("ID3D12Device::CreateHeap")
}

/// `ID3D12Device3::OpenExistingHeapFromAddress` (`d3d12_device.cpp:1536`).
///
/// # Safety
///
/// `this`, `address`, `riid`, and `heap` are caller-supplied pointers.
pub unsafe fn open_existing_heap_from_address(
    _this: *mut c_void,
    _address: *const c_void,
    _riid: *const c_void,
    _heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.OpenExistingHeapFromAddress");
    crate::not_implemented!("ID3D12Device::OpenExistingHeapFromAddress")
}

/// `ID3D12Device3::OpenExistingHeapFromFileMapping` (`d3d12_device.cpp:1545`).
///
/// # Safety
///
/// `this`, `file_mapping`, `riid`, and `heap` are caller-supplied pointers.
pub unsafe fn open_existing_heap_from_file_mapping(
    _this: *mut c_void,
    _file_mapping: *mut c_void,
    _riid: *const c_void,
    _heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.OpenExistingHeapFromFileMapping");
    crate::not_implemented!("ID3D12Device::OpenExistingHeapFromFileMapping")
}

/// `ID3D12Device4::CreateHeap1` (`d3d12_device.cpp:1612`).
///
/// # Safety
///
/// `this`, `desc`, `protected_session`, `riid`, and `heap` are caller-supplied COM pointers.
pub unsafe fn create_heap1(
    _this: *mut c_void,
    _desc: *const c_void,
    _protected_session: *mut c_void,
    _riid: *const c_void,
    _heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateHeap1");
    crate::not_implemented!("ID3D12Device::CreateHeap1")
}
