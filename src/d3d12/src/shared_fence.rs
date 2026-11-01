//! Process-wide shared fence handle table (`d3d12_shared_fence.hpp`, `d3d12_device.cpp:1229-1262`).

use core::ffi::c_void;

use crate::com::Hresult;

/// `ID3D12Device::CreateSharedHandle` (`d3d12_device.cpp:1229`).
///
/// # Safety
///
/// `this`, `object`, `attributes`, `name`, and `handle` are caller-supplied pointers.
pub unsafe fn create_shared_handle(
    _this: *mut c_void,
    _object: *mut c_void,
    _attributes: *const c_void,
    _access: u32,
    _name: *const c_void,
    _handle: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateSharedHandle");
    crate::not_implemented!("ID3D12Device::CreateSharedHandle")
}

/// `ID3D12Device::OpenSharedHandle` (`d3d12_device.cpp:1244`).
///
/// # Safety
///
/// `this`, `nt_handle`, `riid`, and `obj` are caller-supplied pointers.
pub unsafe fn open_shared_handle(
    _this: *mut c_void,
    _nt_handle: *mut c_void,
    _riid: *const c_void,
    _obj: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.OpenSharedHandle");
    crate::not_implemented!("ID3D12Device::OpenSharedHandle")
}

/// `ID3D12Device::OpenSharedHandleByName` (`d3d12_device.cpp:1255`).
///
/// # Safety
///
/// `this`, `name`, and `nt_handle` are caller-supplied pointers.
pub unsafe fn open_shared_handle_by_name(
    _this: *mut c_void,
    _name: *const c_void,
    _access: u32,
    _nt_handle: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.OpenSharedHandleByName");
    crate::not_implemented!("ID3D12Device::OpenSharedHandleByName")
}
