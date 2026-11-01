//! Device creation, `IUnknown`/`ID3D12Object`, and device-level stub methods (`d3d12_device.cpp`).

use core::ffi::c_void;

use crate::{
    com::{self, Guid, Hresult},
    device::Device,
};

/// Creates a `MTLD3D12Device` (`CreateD3D12Device`, `d3d12_device.cpp:2040`).
///
/// # Safety
///
/// `adapter`, `riid`, and `out` are caller-supplied COM pointers (null or valid).
pub unsafe fn create(
    _adapter: *mut c_void,
    _min_feature_level: u32,
    _riid: *const Guid,
    _out: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("D3D12CreateDevice")
}

/// `ID3D12Device::QueryInterface` (`d3d12_device.cpp:424`).
///
/// # Safety
///
/// `this`, `riid`, and `ppv_object` are caller-supplied COM pointers.
pub unsafe fn query_interface(
    _this: *mut c_void,
    _riid: *const c_void,
    _ppv_object: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.QueryInterface");
    crate::not_implemented!("ID3D12Device::QueryInterface")
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
    _this: *mut c_void,
    _guid: *const c_void,
    _data_size: *mut u32,
    _data: *mut c_void,
) -> Hresult {
    crate::not_implemented!("ID3D12Device::GetPrivateData")
}

/// `ID3D12Object::SetPrivateData` (`d3d12_base.hpp`).
///
/// # Safety
///
/// `this`, `guid`, and `data` are caller-supplied COM pointers.
pub unsafe fn set_private_data(
    _this: *mut c_void,
    _guid: *const c_void,
    _data_size: u32,
    _data: *const c_void,
) -> Hresult {
    crate::not_implemented!("ID3D12Device::SetPrivateData")
}

/// `ID3D12Object::SetPrivateDataInterface` (`d3d12_base.hpp`).
///
/// # Safety
///
/// `this`, `guid`, and `data` are caller-supplied COM pointers.
pub unsafe fn set_private_data_interface(
    _this: *mut c_void,
    _guid: *const c_void,
    _data: *const c_void,
) -> Hresult {
    crate::not_implemented!("ID3D12Device::SetPrivateDataInterface")
}

/// `ID3D12Object::SetName` (`d3d12_base.hpp`).
///
/// # Safety
///
/// `this` and `name` are caller-supplied COM pointers.
pub unsafe fn set_name(_this: *mut c_void, _name: *const c_void) -> Hresult {
    crate::not_implemented!("ID3D12Device::SetName")
}

/// `ID3D12Device::GetNodeCount` (`d3d12_device.cpp:471`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn get_node_count(_this: *mut c_void) -> u32 {
    crate::scope!("device.GetNodeCount");
    crate::not_implemented!("ID3D12Device::GetNodeCount")
}

/// `ID3D12Device::GetDeviceRemovedReason` (`d3d12_device.cpp:1285`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn get_device_removed_reason(_this: *mut c_void) -> Hresult {
    crate::scope!("device.GetDeviceRemovedReason");
    crate::not_implemented!("ID3D12Device::GetDeviceRemovedReason")
}

/// `ID3D12Device::SetStablePowerState` (`d3d12_device.cpp:1453`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn set_stable_power_state(_this: *mut c_void, _enable: i32) -> Hresult {
    crate::scope!("device.SetStablePowerState");
    crate::not_implemented!("ID3D12Device::SetStablePowerState")
}

/// `ID3D12Device::GetAdapterLuid` (`d3d12_device.cpp:1476`).
///
/// # Safety
///
/// `this` and `ret` are caller-supplied pointers.
pub unsafe fn get_adapter_luid(_this: *mut c_void, ret: *mut c_void) -> *mut c_void {
    crate::scope!("device.GetAdapterLuid");
    let (): () = crate::not_implemented!("ID3D12Device::GetAdapterLuid");
    ret
}

/// `ID3D12Device4::CreateProtectedResourceSession` (`d3d12_device.cpp:1590`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `session` are caller-supplied COM pointers.
pub unsafe fn create_protected_resource_session(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _session: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateProtectedResourceSession");
    crate::not_implemented!("ID3D12Device::CreateProtectedResourceSession")
}

/// `ID3D12Device5::CreateLifetimeTracker` (`d3d12_device.cpp:1666`).
///
/// # Safety
///
/// `this`, `owner`, `riid`, and `tracker` are caller-supplied COM pointers.
pub unsafe fn create_lifetime_tracker(
    _this: *mut c_void,
    _owner: *mut c_void,
    _riid: *const c_void,
    _tracker: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateLifetimeTracker");
    crate::not_implemented!("ID3D12Device::CreateLifetimeTracker")
}

/// `ID3D12Device5::RemoveDevice` (`d3d12_device.cpp:1674`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn remove_device(_this: *mut c_void) {
    crate::scope!("device.RemoveDevice");
    let (): () = crate::not_implemented!("ID3D12Device::RemoveDevice");
}

/// `ID3D12Device5::EnumerateMetaCommands` (`d3d12_device.cpp:1680`).
///
/// # Safety
///
/// `this`, `num_meta_commands`, and `descs` are caller-supplied pointers.
pub unsafe fn enumerate_meta_commands(
    _this: *mut c_void,
    _num_meta_commands: *mut u32,
    _descs: *mut c_void,
) -> Hresult {
    crate::scope!("device.EnumerateMetaCommands");
    crate::not_implemented!("ID3D12Device::EnumerateMetaCommands")
}

/// `ID3D12Device5::EnumerateMetaCommandParameters` (`d3d12_device.cpp:1691`).
///
/// # Safety
///
/// `this`, `command_id`, `total_size`, `param_count`, and `param_descs` are caller-supplied
/// pointers.
pub unsafe fn enumerate_meta_command_parameters(
    _this: *mut c_void,
    _command_id: *const c_void,
    _stage: u32,
    _total_size: *mut u32,
    _param_count: *mut u32,
    _param_descs: *mut c_void,
) -> Hresult {
    crate::scope!("device.EnumerateMetaCommandParameters");
    crate::not_implemented!("ID3D12Device::EnumerateMetaCommandParameters")
}

/// `ID3D12Device5::CreateMetaCommand` (`d3d12_device.cpp:1703`).
///
/// # Safety
///
/// `this`, `command_id`, `params`, `riid`, and `meta_command` are caller-supplied pointers.
pub unsafe fn create_meta_command(
    _this: *mut c_void,
    _command_id: *const c_void,
    _node_mask: u32,
    _params: *const c_void,
    _params_size: usize,
    _riid: *const c_void,
    _meta_command: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateMetaCommand");
    crate::not_implemented!("ID3D12Device::CreateMetaCommand")
}

/// `ID3D12Device5::CreateStateObject` (`d3d12_device.cpp:1716`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `state_object` are caller-supplied pointers.
pub unsafe fn create_state_object(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _state_object: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateStateObject");
    crate::not_implemented!("ID3D12Device::CreateStateObject")
}

/// `ID3D12Device5::GetRaytracingAccelerationStructurePrebuildInfo` (`d3d12_device.cpp:1726`).
///
/// # Safety
///
/// `this`, `desc`, and `info` are caller-supplied pointers.
pub unsafe fn get_raytracing_acceleration_structure_prebuild_info(
    _this: *mut c_void,
    _desc: *const c_void,
    _info: *mut c_void,
) {
    crate::scope!("device.GetRaytracingAccelerationStructurePrebuildInfo");
    let (): () =
        crate::not_implemented!("ID3D12Device::GetRaytracingAccelerationStructurePrebuildInfo");
}

/// `ID3D12Device5::CheckDriverMatchingIdentifier` (`d3d12_device.cpp:1735`).
///
/// # Safety
///
/// `this` and `identifier` are caller-supplied pointers.
pub unsafe fn check_driver_matching_identifier(
    _this: *mut c_void,
    _data_type: u32,
    _identifier: *const c_void,
) -> u32 {
    crate::scope!("device.CheckDriverMatchingIdentifier");
    crate::not_implemented!("ID3D12Device::CheckDriverMatchingIdentifier")
}

/// `ID3D12Device6::SetBackgroundProcessingMode` (`d3d12_device.cpp:1744`).
///
/// # Safety
///
/// `this`, `event`, and `further_desired` are caller-supplied pointers.
pub unsafe fn set_background_processing_mode(
    _this: *mut c_void,
    _mode: u32,
    _action: u32,
    _event: *mut c_void,
    _further_desired: *mut i32,
) -> Hresult {
    crate::scope!("device.SetBackgroundProcessingMode");
    crate::not_implemented!("ID3D12Device::SetBackgroundProcessingMode")
}

/// `ID3D12Device7::AddToStateObject` (`d3d12_device.cpp:1756`).
///
/// # Safety
///
/// `this`, `addition`, `grow_from`, `riid`, and `new_state_object` are caller-supplied pointers.
pub unsafe fn add_to_state_object(
    _this: *mut c_void,
    _addition: *const c_void,
    _grow_from: *mut c_void,
    _riid: *const c_void,
    _new_state_object: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.AddToStateObject");
    crate::not_implemented!("ID3D12Device::AddToStateObject")
}

/// `ID3D12Device7::CreateProtectedResourceSession1` (`d3d12_device.cpp:1767`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `session` are caller-supplied pointers.
pub unsafe fn create_protected_resource_session1(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _session: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateProtectedResourceSession1");
    crate::not_implemented!("ID3D12Device::CreateProtectedResourceSession1")
}

/// `ID3D12Device9::CreateShaderCacheSession` (`d3d12_device.cpp:1815`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `session` are caller-supplied pointers.
pub unsafe fn create_shader_cache_session(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _session: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateShaderCacheSession");
    crate::not_implemented!("ID3D12Device::CreateShaderCacheSession")
}

/// `ID3D12Device9::ShaderCacheControl` (`d3d12_device.cpp:1825`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn shader_cache_control(_this: *mut c_void, _kinds: u32, _flags: u32) -> Hresult {
    crate::scope!("device.ShaderCacheControl");
    crate::not_implemented!("ID3D12Device::ShaderCacheControl")
}

/// Calibrates and caches the GPU timestamp frequency (`TimestampFrequency`,
///
/// `d3d12_device.cpp:1934`).
#[must_use]
pub fn timestamp_frequency(_device: &Device) -> u64 {
    crate::not_implemented!("device::factory::timestamp_frequency")
}
