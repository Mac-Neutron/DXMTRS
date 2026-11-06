//! Device-wide null resources and atomic lock table (`d3d12_device.cpp:1903-1996`).

use std::sync::{Arc, Mutex, OnceLock};

use dxmt_core::resource::{
    Buffer, BufferAllocationFlags, BufferViewDescriptor, BufferViewKey, Texture,
    TextureAllocationFlags, TextureViewKey,
};
use dxmt_util::sync::lock;
use rustc_hash::FxHashMap;
use winemetal_sys::MTLTexture_replaceRegion;
use winemetal_wire::{
    MemPtr, WMT_PIXEL_FORMAT_RGBA32_FLOAT, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
    WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_RESOURCE_STORAGE_MODE_SHARED, WMT_TEXTURE_TYPE_CUBE,
    WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE2D_MULTISAMPLE,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW,
    WMT_TEXTURE_USAGE_SHADER_READ, WMTOrigin, WMTSize, WMTTextureInfo,
};

use crate::device::{Device, residency};

/// Number of 32-bit spinlocks in the 64-bit atomic lock table (`kAtomic64Locks`,
///
/// `d3d12_device.hpp:208`).
pub const ATOMIC64_LOCKS: usize = 65536;

/// Device-wide null resources and 64-bit atomic lock table (`d3d12_device.cpp:354-360`).
pub struct NullResources {
    pub textures: Mutex<FxHashMap<u32, Arc<Texture>>>,
    pub texel_buffer: Mutex<Option<(Arc<Buffer>, BufferViewKey)>>,
    pub vertex_buffer: Mutex<Option<Arc<Buffer>>>,
    pub atomic64_locks: OnceLock<Arc<Buffer>>,
}

impl NullResources {
    /// Creates an empty null-resource set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            textures: Mutex::new(FxHashMap::default()),
            texel_buffer: Mutex::new(None),
            vertex_buffer: Mutex::new(None),
            atomic64_locks: OnceLock::new(),
        }
    }
}

impl Default for NullResources {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_pointer_width = "64")]
const fn mem_ptr(addr: usize) -> MemPtr {
    MemPtr { ptr: addr as u64 }
}

#[cfg(target_pointer_width = "32")]
fn mem_ptr(addr: usize) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(addr).unwrap_or(0),
        high: 0,
    }
}

const fn null_texture_info(texture_type: u32) -> WMTTextureInfo {
    let multisample = texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE
        || texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY;
    let sample_count: u32 = if multisample { 2 } else { 1 };
    let usage = WMT_TEXTURE_USAGE_SHADER_READ | WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW;
    let storage = if multisample {
        WMT_RESOURCE_STORAGE_MODE_PRIVATE
    } else {
        WMT_RESOURCE_STORAGE_MODE_SHARED
    };
    let bits = (texture_type & 0xff)
        | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
        | ((sample_count & 0xff) << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
        | ((usage & 0xff) << WMTTextureInfo::USAGE_SHIFT);
    WMTTextureInfo {
        pixel_format: WMT_PIXEL_FORMAT_RGBA32_FLOAT,
        width: 1,
        height: 1,
        depth: 1,
        array_length: 1,
        bits,
        options: storage | WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    }
}

const fn null_texel_buffer_view() -> BufferViewKey {
    BufferViewDescriptor {
        format: WMT_PIXEL_FORMAT_RGBA32_FLOAT,
        texel_size: 16,
    }
    .key()
}

/// Returns the device's null texture and full-view key for `texture_type` (`NullTexture`,
///
/// `d3d12_device.cpp:1904`).
#[must_use]
pub fn null_texture(device: &Device, texture_type: u32) -> Option<(Arc<Texture>, TextureViewKey)> {
    let mtl = device.mtl.as_ref()?;
    let mut map = lock(&device.null.textures);
    if let Some(tex) = map.get(&texture_type) {
        return Some((Arc::clone(tex), tex.full_view()));
    }
    let info = null_texture_info(texture_type);
    let multisample = texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE
        || texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY;
    let tex = Arc::new(Texture::new(info, mtl));
    let flags = if multisample {
        TextureAllocationFlags::GPU_PRIVATE
    } else {
        TextureAllocationFlags::empty()
    };
    let alloc = tex.allocate(flags);
    let view = tex.full_view();
    let _ = tex.rename(Arc::clone(&alloc));
    residency::register_texture(device, &tex);

    if !multisample && alloc.texture() != 0 {
        let slices: u64 = if texture_type == WMT_TEXTURE_TYPE_CUBE
            || texture_type == WMT_TEXTURE_TYPE_CUBE_ARRAY
        {
            6
        } else {
            1
        };
        let zero = [0u8; 16];
        let origin = WMTOrigin { x: 0, y: 0, z: 0 };
        let size = WMTSize {
            width: 1,
            height: 1,
            depth: 1,
        };
        let data = mem_ptr(zero.as_ptr().expose_provenance());
        for slice in 0..slices {
            // SAFETY: `alloc.texture()` is a live 1x1 `RGBA32Float` shared `MTLTexture` and `zero` is 16 bytes.
            unsafe {
                MTLTexture_replaceRegion(alloc.texture(), origin, size, 0, slice, data, 16, 0);
            };
        }
    }

    map.insert(texture_type, Arc::clone(&tex));
    drop(map);
    Some((tex, view))
}

/// Returns the device's 16-byte null texel buffer and its `RGBA32Float` view key
///
/// (`NullTexelBuffer`, `d3d12_device.cpp:1955`).
#[must_use]
pub fn null_texel_buffer(device: &Device) -> Option<(Arc<Buffer>, BufferViewKey)> {
    let mtl = device.mtl.as_ref()?;
    let mut slot = lock(&device.null.texel_buffer);
    if let Some((buf, key)) = slot.as_ref() {
        return Some((Arc::clone(buf), *key));
    }
    let buf = Arc::new(Buffer::new(16, mtl));
    let alloc = buf.allocate(BufferAllocationFlags::empty());
    let ptr = alloc.mapped_memory(0);
    if !ptr.is_null() {
        // SAFETY: `ptr` points to 16 writable bytes of the newly allocated buffer.
        unsafe { core::ptr::write_bytes(ptr, 0, 16) };
    }
    let _ = buf.rename(Arc::clone(&alloc));
    residency::register_with_va(device, &buf);
    let key = null_texel_buffer_view();
    let _ = buf.view(&alloc, key);
    *slot = Some((Arc::clone(&buf), key));
    drop(slot);
    Some((buf, key))
}

/// Returns the GPU virtual address of the 4 KiB null vertex buffer (`NullVertexBuffer`,
///
/// `d3d12_device.cpp:1968`).
#[must_use]
pub fn null_vertex_buffer(device: &Device) -> u64 {
    let Some(mtl) = device.mtl.as_ref() else {
        return 0;
    };
    let mut slot = lock(&device.null.vertex_buffer);
    if let Some(buf) = slot.as_ref() {
        return buf.current().map_or(0, |a| a.gpu_address());
    }
    let buf = Arc::new(Buffer::new(4096, mtl));
    let alloc = buf.allocate(BufferAllocationFlags::empty());
    let ptr = alloc.mapped_memory(0);
    if !ptr.is_null() {
        // SAFETY: `ptr` points to 4096 writable bytes of the newly allocated buffer.
        unsafe { core::ptr::write_bytes(ptr, 0, 4096) };
    }
    let gpu_va = alloc.gpu_address();
    let _ = buf.rename(alloc);
    residency::register_with_va(device, &buf);
    *slot = Some(buf);
    gpu_va
}

/// Returns the GPU virtual address of the 256 KiB 64-bit atomic lock table (`Atomic64Locks`,
///
/// `d3d12_device.cpp:1981`).
#[must_use]
pub fn atomic64_locks(device: &Device) -> u64 {
    let Some(mtl) = device.mtl.as_ref() else {
        return 0;
    };
    let buf = device.null.atomic64_locks.get_or_init(|| {
        let bytes = ATOMIC64_LOCKS * 4;
        let buf = Arc::new(Buffer::new(bytes as u64, mtl));
        let alloc = buf.allocate(BufferAllocationFlags::empty());
        let ptr = alloc.mapped_memory(0);
        if !ptr.is_null() {
            // SAFETY: `ptr` points to `bytes` writable bytes of the newly allocated buffer.
            unsafe { core::ptr::write_bytes(ptr, 0, bytes) };
        }
        let _ = buf.rename(alloc);
        residency::register_with_va(device, &buf);
        buf
    });
    buf.current().map_or(0, |a| a.gpu_address())
}

/// Zeros the 256 KiB 64-bit atomic lock table (`ResetAtomic64Locks`, `d3d12_device.cpp:1992`).
pub fn reset_atomic64_locks(device: &Device) {
    let _ = atomic64_locks(device);
    if let Some(buf) = device.null.atomic64_locks.get()
        && let Some(alloc) = buf.current()
    {
        let ptr = alloc.mapped_memory(0);
        if !ptr.is_null() {
            // SAFETY: `ptr` points to `ATOMIC64_LOCKS * 4` writable bytes of the lock buffer.
            unsafe { core::ptr::write_bytes(ptr, 0, ATOMIC64_LOCKS * 4) };
        }
    }
}

#[cfg(test)]
mod tests;
