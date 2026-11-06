//! `d3d11.dll` / `d3d11.dylib` exported entry points (`d3d11.def`, `d3d11.cpp`).

use core::{ffi::c_void, ptr, slice};

use dxmt_util::{
    com::{
        E_INVALIDARG, E_NOTIMPL, Guid, Hresult, IUnknownVtbl, S_FALSE, S_OK,
        iid::IID_IDXGIFACTORY1, release, write_out,
    },
    dxgi_vtbl::{IDXGIFactory1Vtbl, IDXGIObjectVtbl},
};

use crate::{
    LOG,
    abi::{
        IID_ID3D11_DEVICE, ext::IID_IMTL_DXGI_ADAPTER, records::DxgiSwapChainDesc,
        vtbl::ID3D11Device5Vtbl,
    },
    device::{
        D3D11_CREATE_DEVICE_D3D10_INTERNAL, DEFAULT_FEATURE_LEVELS, adapter_supports_apple7,
        create_d3d11_device, max_feature_level_from_config, select_feature_level,
    },
    names::feature_level_name,
};

const D3D_DRIVER_TYPE_UNKNOWN: u32 = 0;
const D3D_DRIVER_TYPE_HARDWARE: u32 = 1;

/// `D3D11CoreCreateDevice`.
///
/// # Safety
///
/// `adapter`, `feature_levels`, and `out_device` must obey the `D3D11CoreCreateDevice` contract.
#[must_use]
#[cfg_attr(feature = "front-end", unsafe(export_name = "D3D11CoreCreateDevice"))]
pub unsafe extern "system" fn d3d11_core_create_device(
    _factory: *mut c_void,
    adapter: *mut c_void,
    flags: u32,
    feature_levels: *const u32,
    levels: u32,
    out_device: *mut *mut c_void,
) -> Hresult {
    if !out_device.is_null() {
        // SAFETY: `out_device` is non-null.
        unsafe {
            write_out(out_device, ptr::null_mut());
        }
    }
    if adapter.is_null() || out_device.is_null() {
        return E_INVALIDARG;
    }

    let mut mtl_adapter: *mut c_void = ptr::null_mut();
    // SAFETY: `adapter` is a valid COM pointer passed by the caller.
    let adapter_vtbl = unsafe { *adapter.cast::<*const IUnknownVtbl>() };
    // SAFETY: `adapter_vtbl` is a valid `IUnknownVtbl`.
    let adapter_qi = unsafe { (*adapter_vtbl).query_interface };
    // SAFETY: Querying `adapter` for `IID_IMTL_DXGI_ADAPTER`.
    let hr_adapter = unsafe {
        adapter_qi(
            adapter,
            ptr::from_ref(&IID_IMTL_DXGI_ADAPTER),
            &raw mut mtl_adapter,
        )
    };
    if hr_adapter < 0 || mtl_adapter.is_null() {
        LOG.err("Not a DXMT adapter");
        return E_INVALIDARG;
    }

    let levels_slice: &[u32] = if feature_levels.is_null() || levels == 0 {
        &DEFAULT_FEATURE_LEVELS
    } else {
        // SAFETY: `feature_levels` is non-null with `levels` elements.
        unsafe { slice::from_raw_parts(feature_levels, levels as usize) }
    };

    // SAFETY: `mtl_adapter` is a valid `IMTLDXGIAdapter*` queried above.
    let supports_apple7 = unsafe { adapter_supports_apple7(mtl_adapter) };
    let cfg_fl = crate::device::CONFIG.get_str("d3d11.maxFeatureLevel");
    let max_fl = max_feature_level_from_config(&cfg_fl, supports_apple7);

    LOG.info(&format!(
        "Maximum supported feature level: {}",
        feature_level_name(max_fl)
    ));

    let Some(selected_fl) = select_feature_level(levels_slice, max_fl) else {
        let min_fl = levels_slice.last().copied().unwrap_or(0);
        LOG.err(&format!(
            "Minimum required feature level {} not supported",
            feature_level_name(min_fl)
        ));
        // SAFETY: Releasing `mtl_adapter` before returning.
        unsafe {
            release(mtl_adapter);
        }
        return E_INVALIDARG;
    };

    LOG.info(&format!(
        "Using feature level {}",
        feature_level_name(selected_fl)
    ));

    let mut dxgi_device: *mut c_void = ptr::null_mut();
    // SAFETY: `mtl_adapter` is a valid `IMTLDXGIAdapter*` and `dxgi_device` is a local out-ptr.
    let hr_create =
        unsafe { create_d3d11_device(mtl_adapter, selected_fl, flags, &raw mut dxgi_device) };
    // SAFETY: Releasing our temporary reference to `mtl_adapter`.
    unsafe {
        release(mtl_adapter);
    }
    if hr_create < 0 || dxgi_device.is_null() {
        return hr_create;
    }

    // SAFETY: `dxgi_device` is a valid COM object with an `IUnknownVtbl` header.
    let dxgi_vtbl = unsafe { *dxgi_device.cast::<*const IUnknownVtbl>() };
    // SAFETY: `dxgi_vtbl` is a valid `IUnknownVtbl`.
    let dxgi_qi = unsafe { (*dxgi_vtbl).query_interface };
    // SAFETY: Querying `dxgi_device` for `IID_ID3D11_DEVICE` into `out_device`.
    let hr_qi = unsafe { dxgi_qi(dxgi_device, ptr::from_ref(&IID_ID3D11_DEVICE), out_device) };
    // SAFETY: Releasing the initial `IMTLDXGIDevice` reference.
    unsafe {
        release(dxgi_device);
    }
    hr_qi
}

/// `D3D11CreateDevice`.
///
/// # Safety
///
/// All pointer arguments must obey the `D3D11CreateDevice` Win32/COM contract.
#[must_use]
#[cfg_attr(feature = "front-end", unsafe(export_name = "D3D11CreateDevice"))]
pub unsafe extern "system" fn d3d11_create_device(
    adapter: *mut c_void,
    driver_type: u32,
    software: *mut c_void,
    flags: u32,
    feature_levels: *const u32,
    levels: u32,
    sdk_version: u32,
    out_device: *mut *mut c_void,
    out_feature_level: *mut u32,
    out_immediate_context: *mut *mut c_void,
) -> Hresult {
    // SAFETY: Forwarding to `d3d11_create_device_and_swap_chain` with null swap chain pointers.
    unsafe {
        d3d11_create_device_and_swap_chain(
            adapter,
            driver_type,
            software,
            flags,
            feature_levels,
            levels,
            sdk_version,
            ptr::null(),
            ptr::null_mut(),
            out_device,
            out_feature_level,
            out_immediate_context,
        )
    }
}

/// `D3D11CreateDeviceAndSwapChain`.
///
/// # Safety
///
/// All pointer arguments must obey the `D3D11CreateDeviceAndSwapChain` Win32/COM contract.
#[must_use]
#[cfg_attr(
    feature = "front-end",
    unsafe(export_name = "D3D11CreateDeviceAndSwapChain")
)]
pub unsafe extern "system" fn d3d11_create_device_and_swap_chain(
    adapter: *mut c_void,
    driver_type: u32,
    software: *mut c_void,
    flags: u32,
    feature_levels: *const u32,
    levels: u32,
    _sdk_version: u32,
    swap_chain_desc: *const DxgiSwapChainDesc,
    out_swap_chain: *mut *mut c_void,
    out_device: *mut *mut c_void,
    out_feature_level: *mut u32,
    out_immediate_context: *mut *mut c_void,
) -> Hresult {
    if !out_device.is_null() {
        // SAFETY: `out_device` is non-null.
        unsafe {
            write_out(out_device, ptr::null_mut());
        }
    }
    if !out_swap_chain.is_null() {
        // SAFETY: `out_swap_chain` is non-null.
        unsafe {
            write_out(out_swap_chain, ptr::null_mut());
        }
    }
    if !out_immediate_context.is_null() {
        // SAFETY: `out_immediate_context` is non-null.
        unsafe {
            write_out(out_immediate_context, ptr::null_mut());
        }
    }
    if !out_feature_level.is_null() {
        // SAFETY: `out_feature_level` is non-null.
        unsafe {
            *out_feature_level = 0;
        }
    }
    if !out_swap_chain.is_null() && swap_chain_desc.is_null() {
        return E_INVALIDARG;
    }

    let mut dxgi_factory: *mut c_void = ptr::null_mut();
    let mut owned_adapter: *mut c_void = ptr::null_mut();
    let active_adapter = if adapter.is_null() {
        if driver_type != D3D_DRIVER_TYPE_HARDWARE {
            LOG.warn(&format!(
                "D3D11CreateDevice: Unsupported driver type {driver_type}"
            ));
        }
        let hr_fac = create_default_dxgi_factory(&raw mut dxgi_factory);
        if hr_fac < 0 || dxgi_factory.is_null() {
            LOG.err("D3D11CreateDevice: Failed to create a DXGI factory");
            return hr_fac;
        }
        // SAFETY: `dxgi_factory` is a valid `IDXGIFactory1*`.
        let fac_vtbl = unsafe { *dxgi_factory.cast::<*const IDXGIFactory1Vtbl>() };
        // SAFETY: `fac_vtbl` is a valid `IDXGIFactory1Vtbl`.
        let enum_adapters = unsafe { (*fac_vtbl).enum_adapters };
        // SAFETY: Enumerating adapter 0 from `dxgi_factory`.
        let hr_adp = unsafe { enum_adapters(dxgi_factory, 0, &raw mut owned_adapter) };
        if hr_adp < 0 || owned_adapter.is_null() {
            LOG.err("D3D11CreateDevice: No default adapter available");
            // SAFETY: Releasing `dxgi_factory`.
            unsafe {
                release(dxgi_factory);
            }
            return hr_adp;
        }
        owned_adapter
    } else {
        // SAFETY: `adapter` is a valid `IDXGIAdapter*` (`IDXGIObject*`).
        let obj_vtbl = unsafe { *adapter.cast::<*const IDXGIObjectVtbl>() };
        // SAFETY: `obj_vtbl` is a valid `IDXGIObjectVtbl`.
        let get_parent = unsafe { (*obj_vtbl).get_parent };
        // SAFETY: Querying parent `IDXGIFactory1` from `adapter`.
        let hr_parent = unsafe {
            get_parent(
                adapter,
                ptr::from_ref(&IID_IDXGIFACTORY1).cast(),
                &raw mut dxgi_factory,
            )
        };
        if hr_parent < 0 || dxgi_factory.is_null() {
            LOG.err("D3D11CreateDevice: Failed to query DXGI factory from DXGI adapter");
            return E_INVALIDARG;
        }
        if driver_type != D3D_DRIVER_TYPE_UNKNOWN || !software.is_null() {
            // SAFETY: Releasing `dxgi_factory`.
            unsafe {
                release(dxgi_factory);
            }
            return E_INVALIDARG;
        }
        adapter
    };

    let mut device: *mut c_void = ptr::null_mut();
    // SAFETY: Creating the D3D11 device on `active_adapter`.
    let hr = unsafe {
        d3d11_core_create_device(
            dxgi_factory,
            active_adapter,
            flags,
            feature_levels,
            levels,
            &raw mut device,
        )
    };
    if !owned_adapter.is_null() {
        // SAFETY: Releasing `owned_adapter`.
        unsafe {
            release(owned_adapter);
        }
    }
    if hr < 0 || device.is_null() {
        // SAFETY: Releasing `dxgi_factory`.
        unsafe {
            release(dxgi_factory);
        }
        return hr;
    }

    if !out_swap_chain.is_null() {
        // SAFETY: `dxgi_factory` is a valid `IDXGIFactory1*`.
        let fac_vtbl = unsafe { *dxgi_factory.cast::<*const IDXGIFactory1Vtbl>() };
        // SAFETY: `fac_vtbl` is a valid `IDXGIFactory1Vtbl`.
        let create_sc = unsafe { (*fac_vtbl).create_swap_chain };
        // SAFETY: Calling `IDXGIFactory::CreateSwapChain`.
        let hr_sc = unsafe {
            create_sc(
                dxgi_factory,
                device,
                swap_chain_desc.cast_mut().cast(),
                out_swap_chain,
            )
        };
        if hr_sc < 0 {
            LOG.err("D3D11CreateDevice: Failed to create swap chain");
            // SAFETY: Releasing `device` and `dxgi_factory`.
            unsafe {
                release(device);
            }
            // SAFETY: Releasing `dxgi_factory`.
            unsafe {
                release(dxgi_factory);
            }
            return hr_sc;
        }
    }

    // SAFETY: Releasing `dxgi_factory`.
    unsafe {
        release(dxgi_factory);
    }

    // SAFETY: `device` is a valid `ID3D11Device*`.
    let dev_vtbl = unsafe { *device.cast::<*const ID3D11Device5Vtbl>() };
    if !out_feature_level.is_null() {
        // SAFETY: `dev_vtbl` is a valid `ID3D11Device5Vtbl`.
        let get_fl = unsafe { (*dev_vtbl).get_feature_level };
        // SAFETY: Calling `GetFeatureLevel` on `device`.
        let fl = unsafe { get_fl(device) };
        // SAFETY: `out_feature_level` is non-null.
        unsafe {
            *out_feature_level = fl;
        }
    }

    if !out_immediate_context.is_null() {
        // SAFETY: `dev_vtbl` is a valid `ID3D11Device5Vtbl`.
        let get_imm = unsafe { (*dev_vtbl).get_immediate_context };
        // SAFETY: Calling `GetImmediateContext` on `device`.
        unsafe {
            get_imm(device, out_immediate_context);
        }
    }

    if out_device.is_null() {
        // SAFETY: Caller did not request `out_device`; release our reference.
        unsafe {
            release(device);
        }
    } else {
        // SAFETY: `out_device` is non-null; transferring `device` reference.
        unsafe {
            write_out(out_device, device);
        }
    }

    if out_device.is_null() && out_immediate_context.is_null() && out_swap_chain.is_null() {
        return S_FALSE;
    }

    S_OK
}

fn create_default_dxgi_factory(out_factory: *mut *mut c_void) -> Hresult {
    unsafe extern "system" {
        fn CreateDXGIFactory1(riid: *const Guid, out: *mut *mut c_void) -> Hresult;
    }
    let iid = IID_IDXGIFACTORY1;
    // SAFETY: Calling `CreateDXGIFactory1` with `IID_IDXGIFACTORY1`.
    unsafe { CreateDXGIFactory1(&raw const iid, out_factory) }
}

/// `D3D11On12CreateDevice`: zeroes out-parameters and returns `E_NOTIMPL` (`d3d11.cpp:219-228`).
///
/// # Safety
///
/// `out_device` and `out_immediate_context` must be null or writable pointers.
#[must_use]
#[cfg_attr(feature = "front-end", unsafe(export_name = "D3D11On12CreateDevice"))]
pub const unsafe extern "system" fn d3d11_on12_create_device(
    _device: *mut c_void,
    _flags: u32,
    _feature_levels: *const u32,
    _levels: u32,
    _command_queues: *const *mut c_void,
    _num_queues: u32,
    _node_mask: u32,
    out_device: *mut *mut c_void,
    out_immediate_context: *mut *mut c_void,
    _out_chosen_feature_level: *mut u32,
) -> Hresult {
    if !out_device.is_null() {
        // SAFETY: `out_device` is non-null.
        unsafe {
            write_out(out_device, ptr::null_mut());
        }
    }
    if !out_immediate_context.is_null() {
        // SAFETY: `out_immediate_context` is non-null.
        unsafe {
            write_out(out_immediate_context, ptr::null_mut());
        }
    }
    E_NOTIMPL
}

/// `D3D10CoreCreateDevice` internal helper (`MTLD3D10CreateDeviceAndSwapChain` path).
///
/// # Safety
///
/// `adapter`, `riid`, and `out` must obey the `D3D10CoreCreateDevice` COM contract.
#[must_use]
pub unsafe fn create_d3d10_device(
    factory: *mut c_void,
    adapter: *mut c_void,
    flags: u32,
    feature_level: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let mut d3d11_device: *mut c_void = ptr::null_mut();
    // SAFETY: Creating a D3D11 device tagged with `D3D11_CREATE_DEVICE_D3D10_INTERNAL`.
    let hr = unsafe {
        d3d11_core_create_device(
            factory,
            adapter,
            flags | D3D11_CREATE_DEVICE_D3D10_INTERNAL,
            &raw const feature_level,
            1,
            &raw mut d3d11_device,
        )
    };
    if hr < 0 || d3d11_device.is_null() {
        return hr;
    }
    // SAFETY: `d3d11_device` is a valid COM object with an `IUnknownVtbl` header.
    let vtbl = unsafe { *d3d11_device.cast::<*const IUnknownVtbl>() };
    // SAFETY: `vtbl` is a valid `IUnknownVtbl`.
    let qi = unsafe { (*vtbl).query_interface };
    // SAFETY: Querying `d3d11_device` for `riid` into `out`.
    let hr_qi = unsafe { qi(d3d11_device, riid, out) };
    // SAFETY: Releasing the temporary `ID3D11Device` reference.
    unsafe {
        release(d3d11_device);
    }
    hr_qi
}

#[cfg(windows)]
const DLL_PROCESS_ATTACH: u32 = 1;

#[cfg(windows)]
unsafe extern "system" {
    fn DisableThreadLibraryCalls(module: *mut c_void) -> i32;
}

/// `DllMain`: disables per-thread notifications on process attach (`d3d11.cpp:234-240`).
///
/// # Safety
///
/// `instance` must be the module handle passed by the Windows loader.
#[cfg(windows)]
#[must_use]
#[cfg_attr(feature = "front-end", unsafe(export_name = "DllMain"))]
pub unsafe extern "system" fn dll_main(
    instance: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    if reason != DLL_PROCESS_ATTACH {
        return 1;
    }
    // SAFETY: `instance` is this DLL's handle passed by the loader to `DllMain`.
    unsafe { DisableThreadLibraryCalls(instance) };
    1
}

/// Native export `MTLD3D11WaitIdle` (`d3d11.cpp:244-251`).
#[cfg(not(target_os = "windows"))]
#[must_use]
#[cfg_attr(feature = "front-end", unsafe(export_name = "MTLD3D11WaitIdle"))]
pub extern "C-unwind" fn mtld3d11_wait_idle() -> i32 {
    i32::from(crate::scheduler::wait_idle(10_000))
}
