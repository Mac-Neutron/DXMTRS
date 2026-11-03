//! `IDXGIDevice3`, `IMTLDXGIDevice` and `IMTLSwapChainFactory` companion on [`super::Device`].

use core::{ffi::c_void, ptr};

use dxmt_util::com::{
    E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, Guid, Hresult, IUnknownVtbl, S_OK,
    add_ref, write_out,
};

use crate::{
    LOG,
    abi::{
        ext::{Hwnd, SwapChainDesc1, SwapChainFullscreenDesc},
        vtbl::IDXGIDevice3Vtbl,
    },
    device::{
        DXGI_ERROR_INVALID_CALL, DxgiDevice, container_add_ref, container_query_interface,
        container_release,
    },
};

/// `IMTLDXGIDevice` vtable (`IDXGIDevice3` + `GetMTLDevice` + `GetLocalD3DKMT` + `CreateSwapChain`)
#[repr(C)]
pub struct IMTLDXGIDeviceVtbl {
    /// Base `IDXGIDevice3` vtable (18 slots).
    pub base: IDXGIDevice3Vtbl,
    /// `IMTLDXGIDevice::GetMTLDevice`.
    pub get_mtl_device: unsafe extern "system-unwind" fn(*mut c_void) -> u64,
    /// `IMTLDXGIDevice::GetLocalD3DKMT`.
    pub get_local_d3dkmt: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    /// `IMTLDXGIDevice::CreateSwapChain`.
    pub create_swap_chain: unsafe extern "system-unwind" fn(
        *mut c_void,
        *mut c_void,
        Hwnd,
        *const SwapChainDesc1,
        *const SwapChainFullscreenDesc,
        *mut *mut c_void,
    ) -> Hresult,
}

/// `IMTLDXGIDevice` vtable for [`DxgiDevice`].
pub static VTBL: IMTLDXGIDeviceVtbl = IMTLDXGIDeviceVtbl {
    base: IDXGIDevice3Vtbl {
        query_interface,
        add_ref: dxgi_add_ref,
        release: dxgi_release,
        set_private_data,
        set_private_data_interface,
        get_private_data,
        get_parent,
        get_adapter,
        create_surface,
        query_resource_residency,
        set_gpu_thread_priority,
        get_gpu_thread_priority,
        set_maximum_frame_latency,
        get_maximum_frame_latency,
        offer_resources,
        reclaim_resources,
        enqueue_set_event,
        trim,
    },
    get_mtl_device,
    get_local_d3dkmt,
    create_swap_chain,
};

unsafe extern "system-unwind" fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    unsafe { container_query_interface(this.cast::<DxgiDevice>(), riid, out) }
}

unsafe extern "system-unwind" fn dxgi_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    unsafe { container_add_ref(this.cast::<DxgiDevice>()) }
}

unsafe extern "system-unwind" fn dxgi_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    unsafe { container_release(this.cast::<DxgiDevice>()) }
}

unsafe extern "system-unwind" fn set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: `guid` is a `GUID` pointer passed by the COM caller.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    let dev = unsafe { &*this.cast::<DxgiDevice>() };
    // SAFETY: Forwarding COM caller pointers to `PrivateData::set_data`.
    unsafe { dev.private_data.set_data(g, size, data) }
}

unsafe extern "system-unwind" fn set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    unknown: *const c_void,
) -> Hresult {
    // SAFETY: `guid` is a `GUID` pointer passed by the COM caller.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    let dev = unsafe { &*this.cast::<DxgiDevice>() };
    // SAFETY: Forwarding COM caller pointers to `PrivateData::set_interface`.
    unsafe { dev.private_data.set_interface(g, unknown.cast_mut()) }
}

unsafe extern "system-unwind" fn get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: `guid` is a `GUID` pointer passed by the COM caller.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    let dev = unsafe { &*this.cast::<DxgiDevice>() };
    // SAFETY: Forwarding COM caller pointers to `PrivateData::get_data`.
    unsafe { dev.private_data.get_data(g, size, data) }
}

unsafe extern "system-unwind" fn get_parent(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    let dev = unsafe { &*this.cast::<DxgiDevice>() };
    if dev.adapter.is_null() {
        if !out.is_null() {
            // SAFETY: `out` is non-null.
            unsafe {
                write_out(out, ptr::null_mut());
            }
        }
        return E_NOINTERFACE;
    }
    // SAFETY: `dev.adapter` is a valid COM pointer with an `IUnknownVtbl` header.
    let vtbl = unsafe { *dev.adapter.cast::<*const IUnknownVtbl>() };
    // SAFETY: `vtbl` is a valid `IUnknownVtbl`.
    let qi = unsafe { (*vtbl).query_interface };
    // SAFETY: Forwarding `GetParent` to `adapter->QueryInterface(riid, out)`.
    unsafe { qi(dev.adapter, riid.cast::<Guid>(), out) }
}

unsafe extern "system-unwind" fn get_adapter(this: *mut c_void, out: *mut *mut c_void) -> Hresult {
    if out.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    let dev = unsafe { &*this.cast::<DxgiDevice>() };
    if !dev.adapter.is_null() {
        // SAFETY: `dev.adapter` is a valid COM pointer.
        unsafe {
            add_ref(dev.adapter);
        }
    }
    // SAFETY: `out` was checked non-null above.
    unsafe {
        write_out(out, dev.adapter);
    }
    S_OK
}

unsafe extern "system-unwind" fn create_surface(
    _this: *mut c_void,
    _desc: *const c_void,
    _num_surfaces: u32,
    _usage: u32,
    _shared_resource: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, ptr::null_mut());
        }
    }
    LOG.err("CreateSurface: not implemented");
    #[cfg(all(not(windows), not(test)))]
    {
        // SAFETY: Recording not-implemented call for native probe diagnostic.
        unsafe {
            winemetal_sys::wmt_native_not_implemented();
        }
    }
    E_NOTIMPL
}

unsafe extern "system-unwind" fn query_resource_residency(
    _this: *mut c_void,
    resources: *const *mut c_void,
    residency: *mut u32,
    count: u32,
) -> Hresult {
    if resources.is_null() || residency.is_null() {
        return E_INVALIDARG;
    }
    for i in 0..(count as usize) {
        // SAFETY: `residency` is non-null and has `count` elements.
        let slot = unsafe { residency.add(i) };
        // SAFETY: `slot` points to a valid `u32` element in `residency`.
        unsafe {
            *slot = 1;
        }
    }
    S_OK
}

unsafe extern "system-unwind" fn set_gpu_thread_priority(
    _this: *mut c_void,
    priority: i32,
) -> Hresult {
    if !(-7..=7).contains(&priority) {
        return E_INVALIDARG;
    }
    S_OK
}

unsafe extern "system-unwind" fn get_gpu_thread_priority(
    _this: *mut c_void,
    out: *mut i32,
) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `out` was checked non-null above.
    unsafe {
        *out = 0;
    }
    S_OK
}

unsafe extern "system-unwind" fn set_maximum_frame_latency(
    this: *mut c_void,
    max_latency: u32,
) -> Hresult {
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    let dev = unsafe { &*this.cast::<DxgiDevice>() };
    dev.d3d11.core().queue().set_max_latency(max_latency);
    S_OK
}

unsafe extern "system-unwind" fn get_maximum_frame_latency(
    this: *mut c_void,
    out: *mut u32,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `this` is a valid `DxgiDevice` pointer.
        let dev = unsafe { &*this.cast::<DxgiDevice>() };
        // SAFETY: `out` was checked non-null above.
        unsafe {
            *out = dev.d3d11.core().queue().max_latency();
        }
    }
    S_OK
}

const unsafe extern "system-unwind" fn offer_resources(
    _this: *mut c_void,
    num_resources: u32,
    resources: *const *mut c_void,
    _priority: u32,
) -> Hresult {
    if num_resources > 0 && resources.is_null() {
        return E_INVALIDARG;
    }
    S_OK
}

unsafe extern "system-unwind" fn reclaim_resources(
    _this: *mut c_void,
    num_resources: u32,
    resources: *const *mut c_void,
    discarded: *mut i32,
) -> Hresult {
    if num_resources > 0 && resources.is_null() {
        return E_INVALIDARG;
    }
    if !discarded.is_null() {
        for i in 0..(num_resources as usize) {
            // SAFETY: `discarded` is non-null with `num_resources` elements.
            let slot = unsafe { discarded.add(i) };
            // SAFETY: `slot` points to a valid `i32` element in `discarded`.
            unsafe {
                *slot = 0;
            }
        }
    }
    S_OK
}

const unsafe extern "system-unwind" fn enqueue_set_event(
    _this: *mut c_void,
    _event: *mut c_void,
) -> Hresult {
    E_FAIL
}

const unsafe extern "system-unwind" fn trim(_this: *mut c_void) {}

unsafe extern "system-unwind" fn get_mtl_device(this: *mut c_void) -> u64 {
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    let dev = unsafe { &*this.cast::<DxgiDevice>() };
    dev.d3d11.core().device().handle()
}

unsafe extern "system-unwind" fn get_local_d3dkmt(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `DxgiDevice` pointer.
    let dev = unsafe { &*this.cast::<DxgiDevice>() };
    dev.local_kmt
}

unsafe extern "system-unwind" fn create_swap_chain(
    this: *mut c_void,
    factory: *mut c_void,
    hwnd: Hwnd,
    desc: *const SwapChainDesc1,
    fullscreen_desc: *const SwapChainFullscreenDesc,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a valid `DxgiDevice` pointer; project its `d3d11` subobject.
    let d3d11_ptr = unsafe { (&raw mut (*this.cast::<DxgiDevice>()).d3d11).cast::<c_void>() };
    crate::swapchain::create_swap_chain(
        d3d11_ptr,
        factory,
        hwnd,
        desc.cast(),
        fullscreen_desc.cast(),
        out,
    )
}
