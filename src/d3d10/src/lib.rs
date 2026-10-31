//! DXMT's `d3d10core.dll` and `d3d10core.dylib`: Direct3D 10 devices made over d3d11.

mod abi;

use core::{ffi::c_void, ptr};

use dxmt_util::{
    com::{self, Hresult, IUnknownVtbl},
    log::Logger,
};

use crate::abi::{
    AdapterCheckVtbl, D3D10_CREATE_DEVICE_SINGLETHREADED, DXMT_D3D10_DEVICE, E_INVALIDARG,
    E_NOTIMPL, IID_ID3D10_DEVICE, IID_ID3D10_MULTITHREAD, MultithreadVtbl,
};

/// This DLL's log file.
///
/// Process-wide resource: one log file per DLL, opened by the first message that passes the
/// level, and the exports have no object to hold it. It is DXMT's `Logger::s_instance`.
static LOG: Logger = Logger::new("d3d10core.log");

unsafe extern "system" {
    /// DXMT's d3d11 device constructor, which `d3d11.dll` and `d3d11.dylib` export.
    fn D3D11CoreCreateDevice(
        factory: *mut c_void,
        adapter: *mut c_void,
        flags: u32,
        feature_levels: *const u32,
        feature_level_count: u32,
        device: *mut *mut c_void,
    ) -> Hresult;
}

/// Makes a Direct3D 10 device: a d3d11 device made as a D3D10 one, answered as `ID3D10Device`.
///
/// The adapter is asked about `ID3D10Device` first, and its failure returned. Multithread
/// protection is on unless `D3D10_CREATE_DEVICE_SINGLETHREADED` is set. A null adapter answers
/// `E_INVALIDARG`; a null `device` reaches d3d11's `QueryInterface`, which answers `E_POINTER`.
///
/// Not `pub`, as neither export is: `no_mangle` exports them, and no Rust caller may hand them a
/// raw pointer (clippy's `not_unsafe_ptr_arg_deref` lints a public safe function that does).
#[unsafe(no_mangle)]
extern "system" fn D3D10CoreCreateDevice(
    factory: *mut c_void,
    adapter: *mut c_void,
    flags: u32,
    feature_level: u32,
    device: *mut *mut c_void,
) -> Hresult {
    if !device.is_null() {
        // SAFETY: the application's out slot, non-null; the write assumes no alignment.
        unsafe { device.write_unaligned(ptr::null_mut()) };
    }
    if adapter.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: a non-null adapter is the application's live `IDXGIAdapter`.
    let slots = unsafe { com::vtbl::<AdapterCheckVtbl>(adapter) };
    // SAFETY: the adapter is live; a null version pointer asks only whether it is supported.
    let hr = unsafe {
        (slots.check_interface_support)(adapter, &raw const IID_ID3D10_DEVICE, ptr::null_mut())
    };
    if hr < 0 {
        return hr;
    }
    let mut d3d11 = ptr::null_mut();
    // SAFETY: the application's factory and adapter go to d3d11 as they came; the one feature
    // level and the device slot are locals.
    let hr = unsafe {
        D3D11CoreCreateDevice(
            factory,
            adapter,
            flags | DXMT_D3D10_DEVICE,
            &raw const feature_level,
            1,
            &raw mut d3d11,
        )
    };
    if hr < 0 {
        return hr;
    }
    // SAFETY: d3d11's new device, whose reference this function holds.
    if let Some(multithread) = unsafe { com::query(d3d11, &IID_ID3D10_MULTITHREAD) } {
        // SAFETY: the query answered an `ID3D10Multithread`.
        let slots = unsafe { com::vtbl::<MultithreadVtbl>(multithread) };
        let protect = i32::from(flags & D3D10_CREATE_DEVICE_SINGLETHREADED == 0);
        // SAFETY: the interface is live; its answer, the old setting, is not needed.
        unsafe { (slots.set_multithread_protected)(multithread, protect) };
        // SAFETY: the reference the query gave.
        unsafe { com::release(multithread) };
    }
    // SAFETY: the device is live and starts with `IUnknown`'s slots.
    let unknown = unsafe { com::vtbl::<IUnknownVtbl>(d3d11) };
    // SAFETY: the device is live; `device` is the application's slot, or null, which d3d11's
    // `QueryInterface` refuses.
    let hr = unsafe { (unknown.query_interface)(d3d11, &raw const IID_ID3D10_DEVICE, device) };
    // SAFETY: the reference D3D11CoreCreateDevice gave; the answer holds its own.
    unsafe { com::release(d3d11) };
    hr
}

/// Registers no layers: a stub, as DXMT's.
#[unsafe(no_mangle)]
extern "system" fn D3D10CoreRegisterLayers() -> Hresult {
    LOG.err("D3D10CoreRegisterLayers: stub");
    E_NOTIMPL
}
