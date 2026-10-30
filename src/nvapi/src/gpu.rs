//! NVAPI's GPUs and their properties.
//!
//! A GPU's handle is its Metal device's registry ID.

use core::ffi::c_void;

use crate::{
    ffi::{
        MTLDevice_registryID, NSArray_count, NSArray_object, NSObject_release, ObjHandle,
        WMTCopyAllDevices,
    },
    nv::{
        GPU_ARCH_IMPLEMENTATION_AD102, GPU_ARCH_INFO_VER1, GPU_ARCH_INFO_VER2,
        GPU_ARCHITECTURE_AD100, GPU_CHIP_REV_UNKNOWN, GpuArchInfo, INCOMPATIBLE_STRUCT_VERSION,
        INVALID_ARGUMENT, LOGICAL_GPU_DATA_VER1, LogicalGpuData, MAX_GPUS, NO_IMPLEMENTATION,
        NOT_SUPPORTED, NVIDIA_DEVICE_NOT_FOUND, OK, SHORT_STRING_MAX, Status,
    },
    sys::short_string,
};

/// The registry IDs of the Metal devices, as `WMT::CopyAllDevices` lists them.
///
/// At most `MAX_GPUS`, as many as a handle array holds.
#[must_use]
pub fn registry_ids() -> Vec<u64> {
    // SAFETY: takes no arguments; the array it returns is ours, released below.
    let devices: ObjHandle = unsafe { WMTCopyAllDevices() };
    if devices == 0 {
        return Vec::new();
    }
    // SAFETY: `devices` is the live array just copied.
    let count = unsafe { NSArray_count(devices) }.min(MAX_GPUS as u64);
    let ids = (0..count)
        .map(|i| {
            // SAFETY: `i` is below the array's count.
            let device = unsafe { NSArray_object(devices, i) };
            // SAFETY: `device` is a live element of `devices`.
            unsafe { MTLDevice_registryID(device) }
        })
        .collect();
    // SAFETY: releases the copy `WMTCopyAllDevices` gave us; nothing uses `devices` after.
    unsafe { NSObject_release(devices) };
    ids
}

/// A GPU's LUID: its registry ID with the bytes swapped, as DXGI's adapter answers.
const fn luid(registry_id: u64) -> [u8; 8] {
    registry_id.swap_bytes().to_ne_bytes()
}

/// A handle's registry ID.
fn handle_id(handle: *mut c_void) -> u64 {
    handle as u64
}

/// A registry ID's handle.
#[must_use]
pub fn id_handle(registry_id: u64) -> *mut c_void {
    core::ptr::without_provenance_mut(usize::try_from(registry_id).unwrap_or(0))
}

/// `NvAPI_EnumPhysicalGPUs` and `NvAPI_EnumLogicalGPUs`: every device's handle.
fn enum_gpus(handles: *mut *mut c_void, count: *mut u32) -> Status {
    if handles.is_null() || count.is_null() {
        return INVALID_ARGUMENT;
    }
    let ids = registry_ids();
    if ids.is_empty() {
        return NVIDIA_DEVICE_NOT_FOUND;
    }
    // SAFETY: `count` is non-null and the caller's `NvU32`.
    unsafe { count.write(u32::try_from(ids.len()).unwrap_or(0)) };
    for (i, &id) in ids.iter().enumerate() {
        // SAFETY: the caller's array holds `NVAPI_MAX_*_GPUS` handles, and `i` is below that.
        let slot = unsafe { handles.add(i) };
        // SAFETY: `slot` is inside the caller's array.
        unsafe { slot.write(id_handle(id)) };
    }
    OK
}

extern "C" fn enum_physical_gpus(handles: *mut *mut c_void, count: *mut u32) -> Status {
    enum_gpus(handles, count)
}

extern "C" fn enum_logical_gpus(handles: *mut *mut c_void, count: *mut u32) -> Status {
    enum_gpus(handles, count)
}

/// `NvAPI_GPU_GetArchInfo`: an RTX 4090 for either version.
extern "C" fn get_arch_info(gpu: *mut c_void, info: *mut GpuArchInfo) -> Status {
    if gpu.is_null() || info.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: `info` is non-null and the caller's `NV_GPU_ARCH_INFO`.
    let version = unsafe { (*info).version };
    if version != GPU_ARCH_INFO_VER1 && version != GPU_ARCH_INFO_VER2 {
        return INCOMPATIBLE_STRUCT_VERSION;
    }
    // SAFETY: as above; V1 and V2 are the same three words.
    let info = unsafe { &mut *info };
    info.architecture = GPU_ARCHITECTURE_AD100;
    info.implementation = GPU_ARCH_IMPLEMENTATION_AD102;
    info.revision = GPU_CHIP_REV_UNKNOWN;
    OK
}

/// `NvAPI_GPU_GetFullName`.
extern "C" fn get_full_name(_gpu: *mut c_void, name: *mut u8) -> Status {
    if name.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: the caller passes an `NvAPI_ShortString`, 64 bytes.
    let name = unsafe { core::slice::from_raw_parts_mut(name, SHORT_STRING_MAX) };
    short_string(name, "NVIDIA GeForce RTX 4090");
    OK
}

/// `NvAPI_GPU_GetAdapterIdFromPhysicalGpu`: the device's LUID.
extern "C" fn get_adapter_id_from_physical_gpu(
    gpu: *mut c_void,
    adapter_id: *mut [u8; 8],
) -> Status {
    if adapter_id.is_null() {
        return INVALID_ARGUMENT;
    }
    if !registry_ids().contains(&handle_id(gpu)) {
        return INVALID_ARGUMENT;
    }
    // SAFETY: `adapter_id` is non-null and the caller's `LUID`.
    unsafe { adapter_id.write(luid(handle_id(gpu))) };
    OK
}

/// `NvAPI_GPU_GetLogicalGpuInfo`: one physical GPU, the same device.
extern "C" fn get_logical_gpu_info(gpu: *mut c_void, data: *mut LogicalGpuData) -> Status {
    if data.is_null() {
        return INVALID_ARGUMENT;
    }
    if !registry_ids().contains(&handle_id(gpu)) {
        return NVIDIA_DEVICE_NOT_FOUND;
    }
    // SAFETY: `data` is non-null and the caller's `NV_LOGICAL_GPU_DATA`.
    if unsafe { (*data).version } != LOGICAL_GPU_DATA_VER1 {
        return INCOMPATIBLE_STRUCT_VERSION;
    }
    // SAFETY: as above, and the version check says the struct has all its fields.
    let data = unsafe { &mut *data };
    // The C++ writes through `pOSAdapterId` unchecked: a null one is an invalid argument here.
    if data.os_adapter_id.is_null() {
        return INVALID_ARGUMENT;
    }
    data.physical_gpu_count = 1;
    data.physical_gpu_handles = [core::ptr::null_mut(); MAX_GPUS];
    data.physical_gpu_handles[0] = gpu;
    // SAFETY: `os_adapter_id` is non-null and the caller's `LUID`.
    unsafe {
        data.os_adapter_id
            .cast::<[u8; 8]>()
            .write(luid(handle_id(gpu)));
    };
    OK
}

/// A GPU query that needs a handle and an output, and answers `status` for both.
const fn unsupported(gpu: *mut c_void, out: *mut c_void, status: Status) -> Status {
    if gpu.is_null() || out.is_null() {
        return INVALID_ARGUMENT;
    }
    status
}

const extern "C" fn get_all_clock_frequencies(_gpu: *mut c_void, freqs: *mut c_void) -> Status {
    // The C++ checks only the struct.
    if freqs.is_null() {
        return INVALID_ARGUMENT;
    }
    NOT_SUPPORTED
}

const extern "C" fn get_gpu_core_count(gpu: *mut c_void, count: *mut c_void) -> Status {
    unsupported(gpu, count, NOT_SUPPORTED)
}

const extern "C" fn get_bus_id(gpu: *mut c_void, bus_id: *mut c_void) -> Status {
    unsupported(gpu, bus_id, NOT_SUPPORTED)
}

const extern "C" fn get_bus_type(gpu: *mut c_void, bus_type: *mut c_void) -> Status {
    unsupported(gpu, bus_type, NOT_SUPPORTED)
}

const extern "C" fn get_memory_info(gpu: *mut c_void, info: *mut c_void) -> Status {
    unsupported(gpu, info, NOT_SUPPORTED)
}

const extern "C" fn get_memory_info_ex(gpu: *mut c_void, info: *mut c_void) -> Status {
    unsupported(gpu, info, NOT_SUPPORTED)
}

const extern "C" fn get_pstates20(gpu: *mut c_void, info: *mut c_void) -> Status {
    unsupported(gpu, info, NO_IMPLEMENTATION)
}

/// The function `id` names, or null if this module has none.
#[must_use]
pub fn query(id: u32) -> *mut c_void {
    match id {
        0xe5ac_921f => enum_physical_gpus as *mut c_void,
        0x48b3_ea59 => enum_logical_gpus as *mut c_void,
        0xd826_5d24 => get_arch_info as *mut c_void,
        0xdcb6_16c3 => get_all_clock_frequencies as *mut c_void,
        0xc702_6a87 => get_gpu_core_count as *mut c_void,
        0x1be0_b8e5 => get_bus_id as *mut c_void,
        0x1bb1_8724 => get_bus_type as *mut c_void,
        0x07f9_b368 => get_memory_info as *mut c_void,
        0xc059_9498 => get_memory_info_ex as *mut c_void,
        0xceee_8e9f => get_full_name as *mut c_void,
        0x6ff8_1213 => get_pstates20 as *mut c_void,
        0x0ff0_7fde => get_adapter_id_from_physical_gpu as *mut c_void,
        0x842b_066e => get_logical_gpu_info as *mut c_void,
        _ => core::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests;
