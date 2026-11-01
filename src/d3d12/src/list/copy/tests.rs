use core::ptr;

use dxmt_util::{com::ComObject, private_data::PrivateData};

use super::{begin_query, end_query};
use crate::{
    abi::{
        list_ops::{
            D3D12_QUERY_TYPE_BINARY_OCCLUSION, D3D12_QUERY_TYPE_OCCLUSION,
            D3D12_QUERY_TYPE_PIPELINE_STATISTICS,
        },
        vtbl::ID3D12QueryHeapVtbl,
    },
    com::{Hresult, S_OK},
    list::{DirtyFlags, List},
    query_heap::QueryHeap,
    slot,
};

static DUMMY_QUERY_HEAP_VTBL: ID3D12QueryHeapVtbl = ID3D12QueryHeapVtbl {
    query_interface: slot!((_this: *mut core::ffi::c_void, _riid: *const core::ffi::c_void, _ppv: *mut *mut core::ffi::c_void) -> Hresult => S_OK),
    add_ref: slot!((_this: *mut core::ffi::c_void) -> u32 => 1),
    release: slot!((_this: *mut core::ffi::c_void) -> u32 => 0),
    get_private_data: slot!((_this: *mut core::ffi::c_void, _guid: *const core::ffi::c_void, _size: *mut u32, _data: *mut core::ffi::c_void) -> Hresult => S_OK),
    set_private_data: slot!((_this: *mut core::ffi::c_void, _guid: *const core::ffi::c_void, _size: u32, _data: *const core::ffi::c_void) -> Hresult => S_OK),
    set_private_data_interface: slot!((_this: *mut core::ffi::c_void, _guid: *const core::ffi::c_void, _data: *const core::ffi::c_void) -> Hresult => S_OK),
    set_name: slot!((_this: *mut core::ffi::c_void, _name: *const core::ffi::c_void) -> Hresult => S_OK),
    get_device: slot!((_this: *mut core::ffi::c_void, _riid: *const core::ffi::c_void, _dev: *mut *mut core::ffi::c_void) -> Hresult => S_OK),
};

#[test]
fn query_begin_end_as_cpp() {
    let mut list = List::new_closed(ptr::null_mut());
    let mut heap = QueryHeap {
        vtbl: &DUMMY_QUERY_HEAP_VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        results: None,
        stride: 8,
        counters: Vec::new(),
        aliases: std::sync::Mutex::new(Vec::new()),
        values: Vec::new(),
    };
    let list_ptr = (&raw mut *list).cast::<core::ffi::c_void>();
    let heap_ptr = (&raw mut heap).cast::<core::ffi::c_void>();

    // Non-occlusion queries do not touch visibility state.
    // SAFETY: `list_ptr` and `heap_ptr` point to valid local objects.
    unsafe { begin_query(list_ptr, heap_ptr, D3D12_QUERY_TYPE_PIPELINE_STATISTICS, 5) };
    assert_eq!(list.active_visibility_offset, u64::MAX);
    assert!(!list.dirty_state.contains(DirtyFlags::VISIBILITY_BUFFER));

    // Counting occlusion query at index 3 -> byte offset 24, bit 0 = 0.
    // SAFETY: `list_ptr` and `heap_ptr` point to valid local objects.
    unsafe { begin_query(list_ptr, heap_ptr, D3D12_QUERY_TYPE_OCCLUSION, 3) };
    assert_eq!(list.active_visibility_offset, 24);
    assert!(list.dirty_state.contains(DirtyFlags::VISIBILITY_BUFFER));

    list.dirty_state.remove(DirtyFlags::VISIBILITY_BUFFER);

    // Binary occlusion query at index 4 -> byte offset 32 | 1.
    // SAFETY: `list_ptr` and `heap_ptr` point to valid local objects.
    unsafe { begin_query(list_ptr, heap_ptr, D3D12_QUERY_TYPE_BINARY_OCCLUSION, 4) };
    assert_eq!(list.active_visibility_offset, 33);
    assert!(list.dirty_state.contains(DirtyFlags::VISIBILITY_BUFFER));

    list.dirty_state.remove(DirtyFlags::VISIBILITY_BUFFER);

    // EndQuery resets active_visibility_offset to u64::MAX and marks VISIBILITY_BUFFER dirty.
    // SAFETY: `list_ptr` and `heap_ptr` point to valid local objects.
    unsafe { end_query(list_ptr, heap_ptr, D3D12_QUERY_TYPE_BINARY_OCCLUSION, 4) };
    assert_eq!(list.active_visibility_offset, u64::MAX);
    assert!(list.dirty_state.contains(DirtyFlags::VISIBILITY_BUFFER));
}
