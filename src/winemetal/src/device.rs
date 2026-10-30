//! Device discovery and queries.

use winemetal_wire::{
    UnixcallGenericObjObjRet, UnixcallGenericObjRet, UnixcallGenericObjUint64Noret, UnixcallGenericObjUint64Ret,
    UnixcallGenericObjUint64Uint64Ret, UnixcallGetOsVersion, slot,
};

use crate::transport::{call, write_if};

exports! {
    fn WMTCopyAllDevices() -> u64 {
        call(slot::WMT_COPY_ALL_DEVICES, |_: &mut UnixcallGenericObjRet| ()).ret
    }

    fn MTLDevice_recommendedMaxWorkingSetSize(device: u64) -> u64 {
        call(slot::MTL_DEVICE_RECOMMENDED_MAX_WORKING_SET_SIZE, |p: &mut UnixcallGenericObjUint64Ret| {
            p.handle = device;
        })
        .ret
    }

    fn MTLDevice_currentAllocatedSize(device: u64) -> u64 {
        call(slot::MTL_DEVICE_CURRENT_ALLOCATED_SIZE, |p: &mut UnixcallGenericObjUint64Ret| p.handle = device).ret
    }

    fn MTLDevice_name(device: u64) -> u64 {
        call(slot::MTL_DEVICE_NAME, |p: &mut UnixcallGenericObjObjRet| p.handle = device).ret
    }

    fn MTLDevice_minimumLinearTextureAlignmentForPixelFormat(device: u64, format: u32) -> u64 {
        call(
            slot::MTL_DEVICE_MINIMUM_LINEAR_TEXTURE_ALIGNMENT_FOR_PIXEL_FORMAT,
            |p: &mut UnixcallGenericObjUint64Uint64Ret| {
                p.handle = device;
                p.arg = u64::from(format);
            },
        )
        .ret
    }

    fn MTLDevice_supportsFamily(device: u64, gpu_family: u32) -> bool {
        call(slot::MTL_DEVICE_SUPPORTS_FAMILY, |p: &mut UnixcallGenericObjUint64Uint64Ret| {
            p.handle = device;
            p.arg = u64::from(gpu_family);
        })
        .ret
            != 0
    }

    fn MTLDevice_supportsBCTextureCompression(device: u64) -> bool {
        call(slot::MTL_DEVICE_SUPPORTS_BC_TEXTURE_COMPRESSION, |p: &mut UnixcallGenericObjUint64Ret| {
            p.handle = device;
        })
        .ret
            != 0
    }

    fn MTLDevice_supportsTextureSampleCount(device: u64, sample_count: u8) -> bool {
        call(slot::MTL_DEVICE_SUPPORTS_TEXTURE_SAMPLE_COUNT, |p: &mut UnixcallGenericObjUint64Uint64Ret| {
            p.handle = device;
            p.arg = u64::from(sample_count);
        })
        .ret
            != 0
    }

    fn MTLDevice_hasUnifiedMemory(device: u64) -> bool {
        call(slot::MTL_DEVICE_HAS_UNIFIED_MEMORY, |p: &mut UnixcallGenericObjUint64Ret| p.handle = device).ret != 0
    }

    fn MTLDevice_setShouldMaximizeConcurrentCompilation(device: u64, value: bool) {
        call(slot::MTL_DEVICE_SET_SHOULD_MAXIMIZE_CONCURRENT_COMPILATION, |p: &mut UnixcallGenericObjUint64Noret| {
            p.handle = device;
            p.arg = u64::from(value);
        });
    }

    fn WMTGetOSVersion(major: *mut u64, minor: *mut u64, patch: *mut u64) {
        let p = call(slot::WMT_GET_OS_VERSION, |_: &mut UnixcallGetOsVersion| ());
        // SAFETY: each is null or valid for a write (the function's contract).
        unsafe { write_if(major, p.ret_major) };
        // SAFETY: as above.
        unsafe { write_if(minor, p.ret_minor) };
        // SAFETY: as above.
        unsafe { write_if(patch, p.ret_patch) };
    }

    fn MTLDevice_registryID(device: u64) -> u64 {
        call(slot::MTL_DEVICE_REGISTRY_ID, |p: &mut UnixcallGenericObjUint64Ret| p.handle = device).ret
    }
}
