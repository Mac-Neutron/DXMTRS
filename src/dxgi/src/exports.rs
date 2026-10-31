//! dxgi.dll's exports (`dxgi.def`) and its `DllMain`; stubs for now.

use core::ffi::c_void;

use crate::abi::{E_NOTIMPL, Guid, Hresult};

/// `CreateDXGIFactory`.
#[unsafe(no_mangle)]
pub const extern "system" fn CreateDXGIFactory(
    _riid: *const Guid,
    _out: *mut *mut c_void,
) -> Hresult {
    E_NOTIMPL
}

/// `CreateDXGIFactory1`.
#[unsafe(no_mangle)]
pub const extern "system" fn CreateDXGIFactory1(
    _riid: *const Guid,
    _out: *mut *mut c_void,
) -> Hresult {
    E_NOTIMPL
}

/// `CreateDXGIFactory2`.
#[unsafe(no_mangle)]
pub const extern "system" fn CreateDXGIFactory2(
    _flags: u32,
    _riid: *const Guid,
    _out: *mut *mut c_void,
) -> Hresult {
    E_NOTIMPL
}

/// `DXGIDeclareAdapterRemovalSupport`.
#[unsafe(no_mangle)]
pub const extern "system" fn DXGIDeclareAdapterRemovalSupport() -> Hresult {
    E_NOTIMPL
}

/// `DXGIGetDebugInterface1`.
#[unsafe(no_mangle)]
pub const extern "system" fn DXGIGetDebugInterface1(
    _flags: u32,
    _riid: *const Guid,
    _out: *mut *mut c_void,
) -> Hresult {
    E_NOTIMPL
}

/// `DllMain`.
#[cfg(windows)]
#[unsafe(export_name = "DllMain")]
pub const extern "system" fn dll_main(
    _instance: *mut c_void,
    _reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    1
}
