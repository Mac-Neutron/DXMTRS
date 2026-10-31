//! dxgi.dll's exports (`dxgi.def`) and its `DllMain`, as `dxgi.cpp` and `dxgi_factory.cpp`.
//!
//! Not `pub`: `no_mangle` exports them, and no Rust caller may hand them a raw pointer.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
};

use dxmt_util::com::{IUnknownVtbl, release, vtbl};

use crate::{
    LOG,
    abi::{
        DXGI_ERROR_ALREADY_EXISTS, DXMT_NVEXT_GUID, E_NOINTERFACE, E_POINTER, Guid, Hresult, S_OK,
    },
    factory, nvext,
};
#[cfg(windows)]
use crate::{abi::DLL_PROCESS_ATTACH, ffi};

#[cfg(windows)]
unsafe extern "system" {
    fn DisableThreadLibraryCalls(module: *mut c_void) -> i32;
}

/// `CreateDXGIFactory`: `CreateDXGIFactory2` with no flags.
#[unsafe(no_mangle)]
extern "system" fn CreateDXGIFactory(riid: *const Guid, out: *mut *mut c_void) -> Hresult {
    CreateDXGIFactory2(0, riid, out)
}

/// `CreateDXGIFactory1`: `CreateDXGIFactory2` with no flags.
#[unsafe(no_mangle)]
extern "system" fn CreateDXGIFactory1(riid: *const Guid, out: *mut *mut c_void) -> Hresult {
    CreateDXGIFactory2(0, riid, out)
}

/// `CreateDXGIFactory2`: a new factory with `flags`, answered as `riid`.
#[unsafe(no_mangle)]
extern "system" fn CreateDXGIFactory2(
    flags: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let factory = factory::create(flags);
    // SAFETY: the new factory, live while this function holds its first reference.
    let unknown = unsafe { vtbl::<IUnknownVtbl>(factory) };
    // SAFETY: the factory is live; `riid` and `out` are the application's, null or valid, which
    // its `QueryInterface` checks and reaches unaligned.
    let hr = unsafe { (unknown.query_interface)(factory, riid, out) };
    // SAFETY: the creator's reference; a successful query holds its own.
    unsafe { release(factory) };
    hr
}

/// `DXGIDeclareAdapterRemovalSupport`: a stub that accepts the first declaration only.
#[unsafe(no_mangle)]
extern "system" fn DXGIDeclareAdapterRemovalSupport() -> Hresult {
    /// Whether the process declared it.
    ///
    /// The caller has no object to reach: an export with no object, declared by this one function
    /// (the C++'s function-local `static bool`).
    static DECLARED: AtomicBool = AtomicBool::new(false);
    if DECLARED.swap(true, Ordering::Relaxed) {
        return DXGI_ERROR_ALREADY_EXISTS;
    }
    LOG.warn("DXGIDeclareAdapterRemovalSupport: stub");
    S_OK
}

/// `DXGIGetDebugInterface1`: only DXMT's NVIDIA extension, answered while it is on.
///
/// `out` is never written: nvngx asks with a null one.
#[unsafe(no_mangle)]
extern "system" fn DXGIGetDebugInterface1(
    _flags: u32,
    riid: *const Guid,
    _out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's IID, null or 16 bytes; the read assumes no alignment.
    let Some(riid) = (unsafe { Guid::read(riid) }) else {
        return E_POINTER;
    };
    if riid == DXMT_NVEXT_GUID && nvext::enabled() {
        S_OK
    } else {
        E_NOINTERFACE
    }
}

/// `DllMain`: on process attach, no thread notifications, Metal's cache path, then the extension.
///
/// With Rust's TLS the loader still calls it per thread, which it ignores.
#[cfg(windows)]
#[unsafe(export_name = "DllMain")]
extern "system" fn dll_main(instance: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason != DLL_PROCESS_ATTACH {
        return 1;
    }
    // SAFETY: `instance` is this DLL's handle, which the loader passes to `DllMain`.
    unsafe { DisableThreadLibraryCalls(instance) };
    set_metal_cache_path();
    let _ = nvext::enabled();
    1
}

/// Points Metal's framework cache (pipelines, not DXMT's) at `dxmt/<cache exe name>`.
///
/// `DXMT_USE_DEFAULT_METAL_CACHE=1` keeps the system's.
#[cfg(windows)]
fn set_metal_cache_path() {
    if std::env::var("DXMT_USE_DEFAULT_METAL_CACHE").as_deref() == Ok("1") {
        return;
    }
    let mut path =
        format!("dxmt/{}/com.apple.metal", dxmt_util::env::cache_exe_name()).into_bytes();
    path.push(0);
    // SAFETY: `path` is NUL-terminated (winemetal reads up to the first NUL, as the C++'s
    // `c_str()`) and outlives the call.
    if !unsafe { ffi::WMTSetMetalShaderCachePath(path.as_ptr().cast()) } {
        LOG.info("Failed to set Metal cache path, fallback to system default");
    }
}
