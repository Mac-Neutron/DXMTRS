//! Default Metal adapter info queried directly through `winemetal-sys` (`7b` off `mtld3d.so`).

use core::ffi::{CStr, c_char};

use mtld3d_core::gpu_caps::DeviceCapsFlags;
use winemetal_sys::{
    MTLDevice_name, MTLDevice_registryID, MTLDevice_supportsFamily,
    MTLDevice_supportsTextureSampleCount, NSArray_count, NSArray_object, NSString_getCString,
    NSString_lengthOfBytesUsingEncoding, Obj, Pool, WMTCopyAllDevices,
    wire::{
        WMTGPU_FAMILY_APPLE2, WMTGPU_FAMILY_APPLE7, WMTGPU_FAMILY_MAC2, WMTUTF8_STRING_ENCODING,
    },
};

/// Metal device name, registry ID, and caps returned by [`query_default_device_info`].
pub struct DeviceInfo {
    /// UTF-8 device name bytes, NUL-terminated when shorter than 256 bytes.
    pub name: [u8; 256],
    /// Byte length of the valid UTF-8 prefix in [`Self::name`].
    pub name_len: usize,
    /// Metal `registryID` of the default device.
    pub registry_id: u64,
    /// D3D9 GPU capability bits derived from Metal family and MSAA support.
    pub caps: DeviceCapsFlags,
}

/// Computes [`DeviceCapsFlags`] from device name, GPU families, and MSAA `[2, 4, 8]` bits.
///
/// Mirrors the Unix adapter query parity rules without ever setting `PAGES_FROM_UNIX`. Note that
/// `FLOAT32_FILTERING` uses `apple7 || mac2` (matching `mtld3d-mtl`, agreeing on Apple Silicon and
/// modern AMD GPUs, whereas legacy Mac2 GPUs without 32-bit float filtering can differ from
/// `mtld3d-unix`'s `supports32BitFloatFiltering`).
#[must_use]
pub fn device_caps_from(
    name: &str,
    mac2: bool,
    apple2: bool,
    apple7: bool,
    msaa: [bool; 3],
) -> DeviceCapsFlags {
    let paravirtual = name.contains("Paravirtual");
    let mut caps = DeviceCapsFlags::empty();
    if mac2 && !paravirtual {
        caps |= DeviceCapsFlags::SAMPLER_BORDER;
    }
    if apple2 {
        caps |= DeviceCapsFlags::NATIVE_PACKED16;
    }
    if apple7 || mac2 {
        caps |= DeviceCapsFlags::FLOAT32_FILTERING;
    }
    if msaa[0] {
        caps |= DeviceCapsFlags::SAMPLE_COUNT_2;
    }
    if msaa[1] {
        caps |= DeviceCapsFlags::SAMPLE_COUNT_4;
    }
    if msaa[2] {
        caps |= DeviceCapsFlags::SAMPLE_COUNT_8;
    }
    if paravirtual {
        caps |= DeviceCapsFlags::RESOLVE_NEEDS_RETIRE;
    }
    caps
}

/// Copies the longest valid UTF-8 prefix of `src` that fits in 255 bytes plus a trailing NUL.
fn copy_utf8_prefix(src: &[u8], dst: &mut [u8; 256]) -> usize {
    let valid = core::str::from_utf8(src).unwrap_or_else(|err| {
        // SAFETY: `Utf8Error::valid_up_to` guarantees `src[..err.valid_up_to()]` is valid UTF-8.
        unsafe { core::str::from_utf8_unchecked(&src[..err.valid_up_to()]) }
    });
    let limit = dst.len() - 1;
    let end = if valid.len() <= limit {
        valid.len()
    } else {
        let mut i = limit;
        while !valid.is_char_boundary(i) {
            i -= 1;
        }
        i
    };
    dst[..end].copy_from_slice(&valid.as_bytes()[..end]);
    dst[end] = 0;
    end
}

/// Copies a Metal `NSString` device name into a 256-byte buffer, truncating safely at UTF-8 bounds.
fn read_device_name(name_ns: u64) -> ([u8; 256], usize) {
    let mut buf = [0_u8; 256];
    if name_ns == 0 {
        return (buf, 0);
    }
    // SAFETY: `name_ns` is a live `NSString` and `buf` is a writable 256-byte C buffer.
    if unsafe {
        NSString_getCString(
            name_ns,
            buf.as_mut_ptr().cast::<c_char>(),
            buf.len() as u64,
            WMTUTF8_STRING_ENCODING,
        )
    } != 0
    {
        let len = CStr::from_bytes_until_nul(&buf).map_or(0, |s| s.to_bytes().len());
        return (buf, len);
    }

    // Fallback when the UTF-8 device name exceeds 255 bytes: allocate exact length and truncate.
    // SAFETY: `name_ns` is a live `NSString`.
    let utf8_len = unsafe { NSString_lengthOfBytesUsingEncoding(name_ns, WMTUTF8_STRING_ENCODING) };
    let Ok(len) = usize::try_from(utf8_len) else {
        return (buf, 0);
    };
    let Some(cap) = len.checked_add(1).filter(|&c| c <= 64 * 1024) else {
        return (buf, 0);
    };
    let mut full = vec![0_u8; cap];
    // SAFETY: `name_ns` is a live `NSString` and `full` has `cap` writable bytes.
    if unsafe {
        NSString_getCString(
            name_ns,
            full.as_mut_ptr().cast::<c_char>(),
            full.len() as u64,
            WMTUTF8_STRING_ENCODING,
        )
    } == 0
    {
        return (buf, 0);
    }
    let bytes = CStr::from_bytes_until_nul(&full).map_or(&full[..], CStr::to_bytes);
    let copied = copy_utf8_prefix(bytes, &mut buf);
    (buf, copied)
}

/// Queries the default Metal adapter (`WMTCopyAllDevices()[0]`) directly from `winemetal-sys`.
///
/// Returns `None` if Metal lists no devices. `NSArray_object` and `MTLDevice_name` return `+0`
/// handles scoped to an internal `Pool`, so neither is manually released.
#[must_use]
pub fn query_default_device_info() -> Option<DeviceInfo> {
    let _pool = Pool::new();
    // SAFETY: `WMTCopyAllDevices` returns `0` or an owned `+1` `NSArray` handle.
    let raw_devices = unsafe { WMTCopyAllDevices() };
    // SAFETY: `raw_devices` is `0` or a `+1` retained handle from `WMTCopyAllDevices`.
    let devices = unsafe { Obj::adopt(raw_devices) }?;
    // SAFETY: `devices` is a live `NSArray`.
    let count = unsafe { NSArray_count(devices.handle()) };
    if count == 0 {
        return None;
    }
    // SAFETY: index `0` is in bounds (`count > 0`); returned `+0` device stays valid with `devices`.
    let device = unsafe { NSArray_object(devices.handle(), 0) };
    if device == 0 {
        return None;
    }
    // SAFETY: `device` is a live `MTLDevice`; returned `+0` `NSString` is autoreleased in `_pool`.
    let name_ns = unsafe { MTLDevice_name(device) };
    let (name, name_len) = read_device_name(name_ns);
    let name_str = core::str::from_utf8(&name[..name_len]).unwrap_or("");
    // SAFETY: `device` is a live `MTLDevice`.
    let registry_id = unsafe { MTLDevice_registryID(device) };
    // SAFETY: `device` is a live `MTLDevice`.
    let mac2 = unsafe { MTLDevice_supportsFamily(device, WMTGPU_FAMILY_MAC2) };
    // SAFETY: `device` is a live `MTLDevice`.
    let apple2 = unsafe { MTLDevice_supportsFamily(device, WMTGPU_FAMILY_APPLE2) };
    // SAFETY: `device` is a live `MTLDevice`.
    let apple7 = unsafe { MTLDevice_supportsFamily(device, WMTGPU_FAMILY_APPLE7) };
    let msaa = [2_u8, 4, 8].map(|sample_count| {
        // SAFETY: `device` is a live `MTLDevice`.
        unsafe { MTLDevice_supportsTextureSampleCount(device, sample_count) }
    });
    let caps = device_caps_from(name_str, mac2, apple2, apple7, msaa);
    Some(DeviceInfo {
        name,
        name_len,
        registry_id,
        caps,
    })
}

#[cfg(test)]
mod tests;
