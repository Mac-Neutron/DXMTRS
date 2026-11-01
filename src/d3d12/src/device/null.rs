//! Device-wide null resources and atomic lock table (`d3d12_device.cpp:1903-1996`).

use std::sync::{Arc, Mutex, OnceLock};

use dxmt_core::resource::{Buffer, BufferViewKey, Texture, TextureViewKey};
use rustc_hash::FxHashMap;

use crate::device::Device;

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

/// Returns the device's null texture and full-view key for `texture_type` (`NullTexture`,
///
/// `d3d12_device.cpp:1904`).
#[must_use]
pub fn null_texture(
    _device: &Device,
    _texture_type: u32,
) -> Option<(Arc<Texture>, TextureViewKey)> {
    crate::not_implemented!("device::null::null_texture")
}

/// Returns the device's 16-byte null texel buffer and its `RGBA32Float` view key
///
/// (`NullTexelBuffer`, `d3d12_device.cpp:1955`).
#[must_use]
pub fn null_texel_buffer(_device: &Device) -> Option<(Arc<Buffer>, BufferViewKey)> {
    crate::not_implemented!("device::null::null_texel_buffer")
}

/// Returns the GPU virtual address of the 4 KiB null vertex buffer (`NullVertexBuffer`,
///
/// `d3d12_device.cpp:1968`).
#[must_use]
pub fn null_vertex_buffer(_device: &Device) -> u64 {
    crate::not_implemented!("device::null::null_vertex_buffer")
}

/// Returns the GPU virtual address of the 256 KiB 64-bit atomic lock table (`Atomic64Locks`,
///
/// `d3d12_device.cpp:1981`).
#[must_use]
pub fn atomic64_locks(_device: &Device) -> u64 {
    crate::not_implemented!("device::null::atomic64_locks")
}

/// Zeros the 256 KiB 64-bit atomic lock table (`ResetAtomic64Locks`, `d3d12_device.cpp:1992`).
pub fn reset_atomic64_locks(_device: &Device) {
    let (): () = crate::not_implemented!("device::null::reset_atomic64_locks");
}
