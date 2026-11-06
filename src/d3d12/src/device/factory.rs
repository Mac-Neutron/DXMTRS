//! Device creation, `IUnknown`/`ID3D12Object`, and device-level stub methods (`d3d12_device.cpp`).

use core::ffi::c_void;
use std::{
    sync::{Mutex, OnceLock, PoisonError},
    time::Duration,
};

use dxmt_core::{
    device::adapter::{self, Failure},
    fence::EventListener,
    format,
    library::{self, Library},
    settings::{Flags, settings},
};
use dxmt_util::{com::ComObject, private_data, private_data::PrivateData, qpc};
use rustc_hash::FxHashMap;
use winemetal_sys::{
    MTLDevice_newCommandQueue, MTLDevice_newResidencySet, MTLDevice_registryID,
    MTLDevice_sampleTimestamps, MTLDevice_supportsBCTextureCompression,
    MTLDevice_supportsFXTemporalScaler, MTLDevice_supportsFamily, Obj,
};
use winemetal_wire::{WMT_COMPARE_FUNCTION_ALWAYS, WMTDepthStencilInfo, WMTStencilInfo};

use crate::{
    abi::ext::{IID_D3D12_DEVICE_DOWNLEVEL, IID_D3D12_DEVICE_EXT, IMTLDXGIAdapterVtbl},
    com::{
        self, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, Guid, Hresult, IID_ID3D10_DEVICE,
        IID_ID3D10_DEVICE1, IID_ID3D11_DEVICE, IID_ID3D11_DEVICE1, IID_ID3D11_DEVICE2,
        IID_ID3D11_DEVICE3, IID_ID3D11_DEVICE4, IID_ID3D11_DEVICE5, IID_ID3D12_DEVICE,
        IID_ID3D12_DEVICE1, IID_ID3D12_DEVICE2, IID_ID3D12_DEVICE3, IID_ID3D12_DEVICE4,
        IID_ID3D12_DEVICE5, IID_ID3D12_DEVICE6, IID_ID3D12_DEVICE7, IID_ID3D12_DEVICE8,
        IID_ID3D12_DEVICE9, IID_ID3D12_DEVICE10, IID_ID3D12_OBJECT, IID_IDXGI_DEVICE,
        IID_IDXGI_DEVICE1, IID_IDXGI_DEVICE2, IID_IDXGI_DEVICE3, IID_IDXGI_DEVICE4, IID_IUNKNOWN,
        S_FALSE, S_OK,
    },
    device::{
        Device, null, residency,
        vtbl::{DEVICE_EXT_VTBL, DEVICE_VTBL},
    },
};

const D3D12_DRIVER_MATCHING_IDENTIFIER_UNSUPPORTED_TYPE: u32 = 1;

impl Drop for Device {
    fn drop(&mut self) {
        // Release every Metal object held by a cache while the MTLDevice that owns it is alive.
        self.shader_functions.clear();
        self.graphics_psos
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.compute_psos
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.depth_stencil_states.clear();
        if !self.adapter.is_null() {
            // SAFETY: `self.adapter` holds one reference acquired during device creation.
            unsafe { com::release(self.adapter) };
        }
    }
}

/// Creates a `MTLD3D12Device` (`D3D12CreateDevice` / `CreateD3D12Device`, `d3d12.cpp:32`,
///
/// `d3d12_device.cpp:2040`).
///
/// # Safety
///
/// `adapter`, `riid`, and `out` are caller-supplied COM pointers (null or valid).
pub unsafe fn create(
    adapter: *mut c_void,
    min_feature_level: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `out` is forwarded from the COM caller.
    unsafe { com::init_out_ptr(out) };

    if min_feature_level < crate::D3D_FEATURE_LEVEL_11_0 {
        return E_INVALIDARG;
    }

    // SAFETY: `adapter` is null or a caller-supplied COM object.
    let mtl_adapter = match unsafe { adapter::resolve(adapter) } {
        Ok(mtl_adapter) => mtl_adapter,
        Err(Failure::Factory(hr)) => {
            crate::err!("D3D12CreateDevice: Failed to create a DXGI factory");
            return if hr < 0 { hr } else { E_FAIL };
        }
        Err(Failure::NoAdapter(hr)) => {
            crate::err!("D3D12CreateDevice: No default adapter available");
            return if hr < 0 { hr } else { E_FAIL };
        }
        Err(Failure::NotDxmt) => {
            crate::err!("D3D12CreateDevice: Not a DXMT adapter");
            return E_NOINTERFACE;
        }
    };

    if out.is_null() {
        // SAFETY: releases `mtl_adapter` before returning `S_FALSE`.
        unsafe { com::release(mtl_adapter) };
        return S_FALSE;
    }

    // SAFETY: `mtl_adapter` is a live `IMTLDXGIAdapter`.
    let adapter_vtbl = unsafe { com::vtbl::<IMTLDXGIAdapterVtbl>(mtl_adapter) };
    // SAFETY: `mtl_adapter` is live.
    let raw_mtl = unsafe { (adapter_vtbl.get_mtl_device)(mtl_adapter) };
    // SAFETY: `raw_mtl` is a borrowed `WMTDevice` handle from `mtl_adapter`.
    let Some(mtl) = (unsafe { Obj::retain(raw_mtl) }) else {
        // SAFETY: releases `mtl_adapter` on failure.
        unsafe { com::release(mtl_adapter) };
        return E_FAIL;
    };

    let mut err_out = 0u64;
    let h = mtl.handle();
    // SAFETY: `h` is a live `MTLDevice` handle and `err_out` is a valid out-pointer.
    let raw_set = unsafe { MTLDevice_newResidencySet(h, 0, &raw mut err_out) };
    // SAFETY: `err_out` is a `+1` retained error handle if non-zero.
    let _err_obj = unsafe { Obj::adopt(err_out) };
    // SAFETY: `raw_set` is a `+1` retained residency set handle if non-zero.
    let Some(residency_set) = (unsafe { Obj::adopt(raw_set) }) else {
        crate::err!("Failed to create MTLResidencySet");
        // SAFETY: releases `mtl_adapter` on failure.
        unsafe { com::release(mtl_adapter) };
        return E_FAIL;
    };

    // SAFETY: `h` is a live `MTLDevice` handle.
    let bc = unsafe { MTLDevice_supportsBCTextureCompression(h) };
    let formats: FxHashMap<u32, u32> = format::capabilities(
        |f| {
            // SAFETY: `h` is a live `MTLDevice` handle.
            unsafe { MTLDevice_supportsFamily(h, f) }
        },
        bc,
    )
    .unwrap_or_default()
    .into_iter()
    .collect();

    let empty_stencil = WMTStencilInfo {
        enabled: 0,
        depth_stencil_pass_op: 0,
        stencil_fail_op: 0,
        depth_fail_op: 0,
        stencil_compare_function: 0,
        write_mask: 0,
        read_mask: 0,
    };
    let ds_info = WMTDepthStencilInfo {
        depth_compare_function: WMT_COMPARE_FUNCTION_ALWAYS,
        depth_write_enabled: 0,
        front_stencil: empty_stencil,
        back_stencil: empty_stencil,
    };
    let default_depth_stencil_state = library::depth_stencil_state(&mtl, &ds_info);

    // SAFETY: `h` is a live `MTLDevice` handle.
    let raw_queue = unsafe { MTLDevice_newCommandQueue(h, 64) };
    // SAFETY: `raw_queue` is a `+1` retained command queue handle if non-zero.
    let Some(fence_helper) = (unsafe { Obj::adopt(raw_queue) }) else {
        crate::err!("Failed to create the fence helper queue");
        // SAFETY: releases `mtl_adapter` on failure.
        unsafe { com::release(mtl_adapter) };
        return E_FAIL;
    };

    let library = Library::new(&mtl).ok();
    // SAFETY: `h` is a live `MTLDevice` handle.
    let temporal_supported = unsafe { MTLDevice_supportsFXTemporalScaler(h) };
    let advertise_numa = settings().enabled(Flags::ADVERTISE_NUMA);

    let mut dev_residency = residency::Residency::new();
    dev_residency.set = Some(residency_set);

    let dev = Box::new(Device {
        vtbl: &DEVICE_VTBL,
        ext_vtbl: &DEVICE_EXT_VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        adapter: mtl_adapter,
        mtl: Some(mtl),
        min_feature_level,
        advertise_numa,
        temporal_supported,
        residency: dev_residency,
        null: null::NullResources::new(),
        library,
        formats,
        fence_helper: Some(fence_helper),
        default_depth_stencil_state,
        depth_stencil_states: dxmt_core::library::DepthStencilCache::default(),
        shader_functions: crate::shader_cache::ShaderFunctionCache::new(),
        graphics_psos: Mutex::new(FxHashMap::default()),
        compute_psos: Mutex::new(FxHashMap::default()),
        events: Some(EventListener::new()),
        upscalers: Mutex::new(Vec::new()),
        timestamp_frequency: OnceLock::new(),
    });
    let raw = Box::into_raw(dev);
    // SAFETY: `raw` is a live `Device` and `riid`/`out` are caller-supplied pointers.
    let hr = unsafe { query_interface(raw.cast(), riid.cast(), out) };
    // SAFETY: releases the initial reference from `with_initial_ref`; `query_interface` added one on success.
    let _ = unsafe { com::release_com(raw) };
    hr
}

/// `ID3D12Device::QueryInterface` (`d3d12_device.cpp:424`).
///
/// # Safety
///
/// `this`, `riid`, and `ppv_object` are caller-supplied COM pointers.
pub unsafe fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.QueryInterface");
    if this.is_null() {
        // SAFETY: `ppv_object` is null or writable.
        return unsafe { com::qi_refuse(ppv_object, E_INVALIDARG) };
    }
    // SAFETY: `this` is a live `Device`.
    let dev = unsafe { &*this.cast::<Device>() };
    // SAFETY: `Device::EXT_OFFSET` is within `Device`.
    let ext_ptr = unsafe { com::to_companion(this.cast::<Device>(), Device::EXT_OFFSET) };
    // Refused without a line; `IID_D3D12_DEVICE_EXT`, last, only while temporal scaling is off.
    let silent = [
        IID_IDXGI_DEVICE,
        IID_IDXGI_DEVICE1,
        IID_IDXGI_DEVICE2,
        IID_IDXGI_DEVICE3,
        IID_IDXGI_DEVICE4,
        IID_ID3D10_DEVICE,
        IID_ID3D10_DEVICE1,
        IID_ID3D11_DEVICE,
        IID_ID3D11_DEVICE1,
        IID_ID3D11_DEVICE2,
        IID_ID3D11_DEVICE3,
        IID_ID3D11_DEVICE4,
        IID_ID3D11_DEVICE5,
        IID_D3D12_DEVICE_DOWNLEVEL,
        IID_D3D12_DEVICE_EXT,
    ];
    let qi = com::Qi {
        null_riid: E_INVALIDARG,
        silent: &silent[..silent.len() - usize::from(dev.temporal_supported)],
        log: &crate::LOG,
        miss: com::QiMiss::Once {
            object: &IID_ID3D12_DEVICE1,
            name: "D3D12Device",
        },
    };
    let ext: &[Guid] = if dev.temporal_supported {
        &[IID_D3D12_DEVICE_EXT]
    } else {
        &[]
    };
    let add_ref = || {
        // SAFETY: `this` is a live `Device`.
        unsafe { com::add_ref_com(this.cast::<Device>()) };
    };
    // SAFETY: the application's `riid` and `ppv_object`, null or valid; `this` is a live `Device`.
    unsafe {
        com::qi_table(
            riid,
            ppv_object,
            &[
                (
                    &[
                        IID_IUNKNOWN,
                        IID_ID3D12_OBJECT,
                        IID_ID3D12_DEVICE,
                        IID_ID3D12_DEVICE1,
                        IID_ID3D12_DEVICE2,
                        IID_ID3D12_DEVICE3,
                        IID_ID3D12_DEVICE4,
                        IID_ID3D12_DEVICE5,
                        IID_ID3D12_DEVICE6,
                        IID_ID3D12_DEVICE7,
                        IID_ID3D12_DEVICE8,
                        IID_ID3D12_DEVICE9,
                        IID_ID3D12_DEVICE10,
                    ],
                    this,
                ),
                (ext, ext_ptr),
            ],
            add_ref,
            &qi,
        )
    }
}

/// `IMTLD3D12DeviceExt::QueryInterface` (`d3d12_device.cpp:337`).
///
/// # Safety
///
/// `this`, `riid`, and `object` are caller-supplied COM pointers (`this` at `Device::EXT_OFFSET`).
pub unsafe fn ext_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    object: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is the `ext_vtbl` companion pointer at `Device::EXT_OFFSET`.
    let dev = unsafe { this.byte_sub(Device::EXT_OFFSET) };
    // SAFETY: `dev` is the enclosing `Device` pointer.
    unsafe { query_interface(dev, riid, object) }
}

/// `IMTLD3D12DeviceExt::AddRef` (`d3d12_device.cpp:337`).
///
/// # Safety
///
/// `this` is a valid `Device` companion pointer at [`Device::EXT_OFFSET`].
pub unsafe fn ext_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is at `Device::EXT_OFFSET` inside a live `Device`.
    unsafe { com::companion_add_ref::<Device, { Device::EXT_OFFSET }>(this) }
}

/// `IMTLD3D12DeviceExt::Release` (`d3d12_device.cpp:337`).
///
/// # Safety
///
/// `this` is a valid `Device` companion pointer at [`Device::EXT_OFFSET`].
pub unsafe fn ext_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is at `Device::EXT_OFFSET` inside a live `Device`.
    unsafe { com::companion_release::<Device, { Device::EXT_OFFSET }>(this) }
}

/// `ID3D12Object::GetPrivateData` (`d3d12_base.hpp`).
///
/// # Safety
///
/// `this`, `guid`, `data_size`, and `data` are caller-supplied COM pointers.
pub unsafe fn get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    data_size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `Device`.
    let dev = unsafe { &*this.cast::<Device>() };
    // SAFETY: `guid` is null or 16 readable bytes; the rest is forwarded from the COM caller.
    unsafe { dev.private_data.get_ffi(guid, data_size, data) }
}

/// `ID3D12Object::SetPrivateData` (`d3d12_base.hpp`).
///
/// # Safety
///
/// `this`, `guid`, and `data` are caller-supplied COM pointers.
pub unsafe fn set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    data_size: u32,
    data: *const c_void,
) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `Device`.
    let dev = unsafe { &*this.cast::<Device>() };
    // SAFETY: `guid` is null or 16 readable bytes; the rest is forwarded from the COM caller.
    unsafe { dev.private_data.set_ffi(guid, data_size, data) }
}

/// `ID3D12Object::SetPrivateDataInterface` (`d3d12_base.hpp`).
///
/// # Safety
///
/// `this`, `guid`, and `data` are caller-supplied COM pointers.
pub unsafe fn set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    data: *const c_void,
) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `Device`.
    let dev = unsafe { &*this.cast::<Device>() };
    // SAFETY: `guid` is null or 16 readable bytes; the rest is forwarded from the COM caller.
    unsafe { dev.private_data.set_interface_ffi(guid, data) }
}

/// `ID3D12Object::SetName` (`d3d12_base.hpp`).
///
/// # Safety
///
/// `this` and `name` are caller-supplied COM pointers.
pub unsafe fn set_name(this: *mut c_void, name: *const c_void) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `Device`.
    let dev = unsafe { &*this.cast::<Device>() };
    // SAFETY: `name` is null or a NUL-terminated UTF-16 string.
    unsafe { private_data::set_debug_name(&dev.private_data, name.cast::<u16>()) }
}

/// `ID3D12Device::GetNodeCount` (`d3d12_device.cpp:417`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn get_node_count(this: *mut c_void) -> u32 {
    crate::scope!("device.GetNodeCount");
    let _ = Option::from(this);
    1
}

/// `ID3D12Device::GetDeviceRemovedReason` (`d3d12_device.cpp:1330`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn get_device_removed_reason(this: *mut c_void) -> Hresult {
    crate::scope!("device.GetDeviceRemovedReason");
    let _ = Option::from(this);
    S_OK
}

/// `ID3D12Device::SetStablePowerState` (`d3d12_device.cpp:1453`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn set_stable_power_state(this: *mut c_void, _enable: i32) -> Hresult {
    crate::scope!("device.SetStablePowerState");
    let _ = Option::from(this);
    E_NOTIMPL
}

/// `ID3D12Device::GetAdapterLuid` (`d3d12_device.cpp:1476`).
///
/// # Safety
///
/// `this` and `ret` are caller-supplied pointers.
pub unsafe fn get_adapter_luid(this: *mut c_void, ret: *mut c_void) -> *mut c_void {
    crate::scope!("device.GetAdapterLuid");
    if ret.is_null() {
        return ret;
    }
    let luid_bits = if this.is_null() {
        0u64
    } else {
        // SAFETY: `this` is a live `Device`.
        let dev = unsafe { &*this.cast::<Device>() };
        dev.mtl.as_ref().map_or(0u64, |m| {
            let h = m.handle();
            // SAFETY: `h` is a live `MTLDevice` handle.
            let reg_id = unsafe { MTLDevice_registryID(h) };
            reg_id.swap_bytes()
        })
    };
    // SAFETY: `ret` points to a caller-supplied 8-byte `LUID`.
    unsafe { ret.cast::<u64>().write_unaligned(luid_bits) };
    ret
}

/// `ID3D12Device4::CreateProtectedResourceSession` (`d3d12_device.cpp:1568`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `session` are caller-supplied COM pointers.
pub unsafe fn create_protected_resource_session(
    this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    session: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateProtectedResourceSession");
    let _ = Option::from(this);
    // SAFETY: `session` is forwarded from the COM caller.
    unsafe { com::init_out_ptr(session) };
    E_NOTIMPL
}

/// `ID3D12Device5::CreateLifetimeTracker` (`d3d12_device.cpp:1594`).
///
/// # Safety
///
/// `this`, `owner`, `riid`, and `tracker` are caller-supplied COM pointers.
pub unsafe fn create_lifetime_tracker(
    this: *mut c_void,
    _owner: *mut c_void,
    _riid: *const c_void,
    tracker: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateLifetimeTracker");
    let _ = Option::from(this);
    // SAFETY: `tracker` is forwarded from the COM caller.
    unsafe { com::init_out_ptr(tracker) };
    E_NOTIMPL
}

/// `ID3D12Device5::RemoveDevice` (`d3d12_device.cpp:1601`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn remove_device(this: *mut c_void) {
    crate::scope!("device.RemoveDevice");
    let _ = Option::from(this);
}

/// `ID3D12Device5::EnumerateMetaCommands` (`d3d12_device.cpp:1605`).
///
/// # Safety
///
/// `this`, `num_meta_commands`, and `descs` are caller-supplied pointers.
pub unsafe fn enumerate_meta_commands(
    this: *mut c_void,
    num_meta_commands: *mut u32,
    _descs: *mut c_void,
) -> Hresult {
    crate::scope!("device.EnumerateMetaCommands");
    let _ = Option::from(this);
    if num_meta_commands.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `num_meta_commands` is non-null.
    unsafe { num_meta_commands.write_unaligned(0) };
    S_OK
}

/// `ID3D12Device5::EnumerateMetaCommandParameters` (`d3d12_device.cpp:1614`).
///
/// # Safety
///
/// `this`, `command_id`, `total_size`, `param_count`, and `param_descs` are caller-supplied
/// pointers.
pub unsafe fn enumerate_meta_command_parameters(
    this: *mut c_void,
    _command_id: *const c_void,
    _stage: u32,
    _total_size: *mut u32,
    _param_count: *mut u32,
    _param_descs: *mut c_void,
) -> Hresult {
    crate::scope!("device.EnumerateMetaCommandParameters");
    let _ = Option::from(this);
    E_INVALIDARG
}

/// `ID3D12Device5::CreateMetaCommand` (`d3d12_device.cpp:1623`).
///
/// # Safety
///
/// `this`, `command_id`, `params`, `riid`, and `meta_command` are caller-supplied pointers.
pub unsafe fn create_meta_command(
    this: *mut c_void,
    _command_id: *const c_void,
    _node_mask: u32,
    _params: *const c_void,
    _params_size: usize,
    _riid: *const c_void,
    meta_command: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateMetaCommand");
    let _ = Option::from(this);
    // SAFETY: `meta_command` is forwarded from the COM caller.
    unsafe { com::init_out_ptr(meta_command) };
    E_INVALIDARG
}

/// `ID3D12Device5::CreateStateObject` (`d3d12_device.cpp:1630`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `state_object` are caller-supplied pointers.
pub unsafe fn create_state_object(
    this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    state_object: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateStateObject");
    let _ = Option::from(this);
    // SAFETY: `state_object` is forwarded from the COM caller.
    unsafe { com::init_out_ptr(state_object) };
    E_NOTIMPL
}

/// `ID3D12Device5::GetRaytracingAccelerationStructurePrebuildInfo` (`d3d12_device.cpp:1637`).
///
/// # Safety
///
/// `this`, `desc`, and `info` are caller-supplied pointers.
pub unsafe fn get_raytracing_acceleration_structure_prebuild_info(
    this: *mut c_void,
    _desc: *const c_void,
    info: *mut c_void,
) {
    crate::scope!("device.GetRaytracingAccelerationStructurePrebuildInfo");
    let _ = Option::from(this);
    if !info.is_null() {
        // SAFETY: `info` points to `D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO` (24 bytes).
        unsafe { info.cast::<[u64; 3]>().write_unaligned([0; 3]) };
    }
}

/// `ID3D12Device5::CheckDriverMatchingIdentifier` (`d3d12_device.cpp:1647`).
///
/// # Safety
///
/// `this` and `identifier` are caller-supplied pointers.
pub unsafe fn check_driver_matching_identifier(
    this: *mut c_void,
    _data_type: u32,
    _identifier: *const c_void,
) -> u32 {
    crate::scope!("device.CheckDriverMatchingIdentifier");
    let _ = Option::from(this);
    D3D12_DRIVER_MATCHING_IDENTIFIER_UNSUPPORTED_TYPE
}

/// `ID3D12Device6::SetBackgroundProcessingMode` (`d3d12_device.cpp:1653`).
///
/// # Safety
///
/// `this`, `event`, and `further_desired` are caller-supplied pointers.
pub unsafe fn set_background_processing_mode(
    this: *mut c_void,
    _mode: u32,
    _action: u32,
    _event: *mut c_void,
    further_desired: *mut i32,
) -> Hresult {
    crate::scope!("device.SetBackgroundProcessingMode");
    let _ = Option::from(this);
    if !further_desired.is_null() {
        // SAFETY: `further_desired` is non-null.
        unsafe { further_desired.write_unaligned(0) };
    }
    S_OK
}

/// `ID3D12Device7::AddToStateObject` (`d3d12_device.cpp:1663`).
///
/// # Safety
///
/// `this`, `addition`, `grow_from`, `riid`, and `new_state_object` are caller-supplied pointers.
pub unsafe fn add_to_state_object(
    this: *mut c_void,
    _addition: *const c_void,
    _grow_from: *mut c_void,
    _riid: *const c_void,
    new_state_object: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.AddToStateObject");
    let _ = Option::from(this);
    // SAFETY: `new_state_object` is forwarded from the COM caller.
    unsafe { com::init_out_ptr(new_state_object) };
    E_NOTIMPL
}

/// `ID3D12Device7::CreateProtectedResourceSession1` (`d3d12_device.cpp:1670`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `session` are caller-supplied pointers.
pub unsafe fn create_protected_resource_session1(
    this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    session: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateProtectedResourceSession1");
    let _ = Option::from(this);
    // SAFETY: `session` is forwarded from the COM caller.
    unsafe { com::init_out_ptr(session) };
    E_NOTIMPL
}

/// `ID3D12Device9::CreateShaderCacheSession` (`d3d12_device.cpp:1750`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `session` are caller-supplied pointers.
pub unsafe fn create_shader_cache_session(
    this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    session: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateShaderCacheSession");
    let _ = Option::from(this);
    // SAFETY: `session` is forwarded from the COM caller.
    unsafe { com::init_out_ptr(session) };
    E_NOTIMPL
}

/// `ID3D12Device9::ShaderCacheControl` (`d3d12_device.cpp:1757`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn shader_cache_control(this: *mut c_void, _kinds: u32, _flags: u32) -> Hresult {
    crate::scope!("device.ShaderCacheControl");
    let _ = Option::from(this);
    E_NOTIMPL
}

/// Calibrates and caches the GPU timestamp frequency (`TimestampFrequency`,
///
/// `d3d12_device.cpp:1934`).
#[must_use]
pub fn timestamp_frequency(device: &Device) -> u64 {
    *device.timestamp_frequency.get_or_init(|| {
        let Some(mtl) = device.mtl.as_ref() else {
            return 1;
        };
        let mut cpu0 = 0u64;
        let mut gpu0 = 0u64;
        let mut cpu1 = 0u64;
        let mut gpu1 = 0u64;
        let h = mtl.handle();
        let (q0, qf) = qpc::qpc();
        // SAFETY: `h` is a live `MTLDevice` handle and `cpu0`/`gpu0` are valid out-pointers.
        unsafe { MTLDevice_sampleTimestamps(h, &raw mut cpu0, &raw mut gpu0) };
        std::thread::sleep(Duration::from_millis(10));
        let (q1, _) = qpc::qpc();
        // SAFETY: `h` is a live `MTLDevice` handle and `cpu1`/`gpu1` are valid out-pointers.
        unsafe { MTLDevice_sampleTimestamps(h, &raw mut cpu1, &raw mut gpu1) };
        if gpu1 > gpu0 && q1 > q0 {
            let dq = q1 - q0;
            let num = (gpu1 - gpu0).saturating_mul(qf);
            num.saturating_add(dq / 2) / dq
        } else {
            1
        }
    })
}

#[cfg(test)]
mod tests;
