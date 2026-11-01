//! Buffer resources (`d3d12_buffer.cpp`): committed and placed buffers, `Map`/`Unmap`, and
//!
//! buffer SRV/UAV creation.

use core::ffi::c_void;

use crate::{
    abi::{
        resource::{HeapProperties, ResourceDesc},
        vtbl::D3d12CpuDescriptorHandle,
    },
    com::Hresult,
    device::Device,
    heap::HeapObject,
    resource::Resource,
};

/// Creates a buffer [`Resource`] (`CreateBufferResource`, `d3d12_buffer.cpp`).
///
/// # Safety
///
/// `device`, `placed`, `riid`, and `out` are caller-supplied COM pointers.
pub unsafe fn create(
    _device: *mut Device,
    _heap_props: &HeapProperties,
    _heap_flags: u32,
    _desc: &ResourceDesc,
    _placed: Option<(*mut HeapObject, u64)>,
    _riid: *const c_void,
    _out: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("resource::buffer::create")
}

/// Creates a buffer SRV in `descriptor` (`d3d12_buffer.cpp:193`).
///
/// # Safety
///
/// `desc` is null or points to a valid `D3D12_SHADER_RESOURCE_VIEW_DESC`.
pub unsafe fn create_srv(
    _resource: &Resource,
    _desc: *const c_void,
    _descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::not_implemented!("resource::buffer::create_srv")
}

/// Creates a buffer UAV in `descriptor` (`d3d12_buffer.cpp:259`).
///
/// # Safety
///
/// `counter` and `desc` are caller-supplied pointers.
pub unsafe fn create_uav(
    _resource: &Resource,
    _counter: *mut c_void,
    _desc: *const c_void,
    _descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::not_implemented!("resource::buffer::create_uav")
}
