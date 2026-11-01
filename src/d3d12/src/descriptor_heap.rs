//! `ID3D12DescriptorHeap` (`d3d12_descriptor_heap.{hpp,cpp}`): CBV/SRV/UAV, sampler, RTV, and DSV
//!
//! heaps.

use core::ffi::c_void;
use std::sync::{Arc, Mutex};

use dxmt_core::resource::{Buffer, BufferViewKey, Texture, TextureViewKey};
use dxmt_util::{com::ComObject, private_data::PrivateData};

use crate::{
    abi::vtbl::{D3d12CpuDescriptorHandle, ID3D12DescriptorHeapVtbl},
    com::{ComRefCounted, Hresult},
    device::Device,
};

/// Buffer subrange descriptor (`dxmt::BufferSlice`, `dxmt_buffer.hpp:44-50`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferSlice {
    pub byte_offset: u64,
    pub byte_length: u64,
    pub first_element: u64,
    pub element_count: u32,
    pub byte_stride: u32,
}

/// Packed CPU descriptor handle (`EMBEDDED_DESCRIPTOR_HANDLE`, `d3d12_descriptor_heap.hpp:32-77`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct EmbeddedDescriptorHandle(pub usize);

/// CPU-side storage for a shader-visible descriptor (`ShaderVisibleDescriptorCPUStorage`,
///
/// `d3d12_descriptor_heap.hpp:79-130`).
#[derive(Clone)]
pub enum ShaderVisibleDescriptor {
    Null,
    SrvTexture {
        texture: Option<Arc<Texture>>,
        view: TextureViewKey,
    },
    ConstantBuffer {
        address: u64,
        size: u64,
    },
    UavTexture {
        texture: Option<Arc<Texture>>,
        view: TextureViewKey,
    },
    UavTexelBuffer {
        buffer: Option<Arc<Buffer>>,
        view: BufferViewKey,
        slice: BufferSlice,
    },
    UavBuffer {
        buffer: Option<Arc<Buffer>>,
        slice: BufferSlice,
    },
    SrvTexelBuffer {
        buffer: Option<Arc<Buffer>>,
        view: BufferViewKey,
        slice: BufferSlice,
    },
    SrvBuffer {
        buffer: Option<Arc<Buffer>>,
        slice: BufferSlice,
    },
}

/// CPU-side RTV or DSV descriptor (`MTL_RENDER_TARGET_DESC`, `d3d12_descriptor_heap.hpp:172-180`).
#[derive(Clone)]
pub struct RenderTargetDesc {
    pub texture: Option<Arc<Texture>>,
    pub view: TextureViewKey,
    pub width: u32,
    pub height: u32,
    pub depth_plane: u32,
    pub render_target_array_length: u32,
    pub flags: u32,
}

/// `ID3D12DescriptorHeap` object (`d3d12_descriptor_heap.cpp`).
#[repr(C)]
pub struct Heap {
    pub vtbl: &'static ID3D12DescriptorHeapVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub heap_type: u32,
    pub num_descriptors: u32,
    pub flags: u32,
    pub gpu_buffer: Option<Arc<Buffer>>,
    pub cpu_descriptors: Mutex<Vec<ShaderVisibleDescriptor>>,
    pub samplers: Mutex<Vec<[u64; 4]>>,
    pub render_targets: Mutex<Vec<RenderTargetDesc>>,
}

impl ComRefCounted for Heap {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Heap {
    /// Returns the CPU-side descriptor metadata at `index` (`GetDescriptor`,
    ///
    /// `d3d12_descriptor_heap.hpp:160`).
    #[must_use]
    pub fn descriptor(&self, _index: u32) -> ShaderVisibleDescriptor {
        let (): () = crate::not_implemented!("descriptor_heap::Heap::descriptor");
        ShaderVisibleDescriptor::Null
    }

    /// Returns the GPU virtual address of the heap's backing buffer (`0` when not shader-visible).
    #[must_use]
    pub fn gpu_address(&self) -> u64 {
        self.gpu_buffer
            .as_ref()
            .and_then(|b| b.current())
            .map_or(0, |a| a.gpu_address())
    }
}

/// Casts a non-null `ID3D12DescriptorHeap` pointer if its vtable matches [`Heap`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_heap<'a>(raw: *mut c_void) -> Option<&'a Heap> {
    let _ = raw;
    crate::not_implemented!("descriptor_heap::cast_heap")
}

/// Decodes a CBV/SRV/UAV `D3D12_CPU_DESCRIPTOR_HANDLE` into `(heap, index)`.
///
/// # Safety
///
/// `handle` must have been produced by a live `Heap`.
#[must_use]
pub unsafe fn shader_visible<'a>(handle: D3d12CpuDescriptorHandle) -> Option<(&'a Heap, u32)> {
    let _ = handle;
    crate::not_implemented!("descriptor_heap::shader_visible")
}

/// Decodes an RTV or DSV `D3D12_CPU_DESCRIPTOR_HANDLE` into its [`RenderTargetDesc`].
///
/// # Safety
///
/// `handle` must have been produced by a live RTV or DSV `Heap`.
#[must_use]
pub unsafe fn render_target(handle: D3d12CpuDescriptorHandle) -> Option<RenderTargetDesc> {
    let _ = handle;
    crate::not_implemented!("descriptor_heap::render_target")
}

/// Decodes a sampler `D3D12_CPU_DESCRIPTOR_HANDLE` into `(heap, index)`.
///
/// # Safety
///
/// `handle` must have been produced by a live sampler `Heap`.
#[must_use]
pub unsafe fn sampler<'a>(handle: D3d12CpuDescriptorHandle) -> Option<(&'a Heap, u32)> {
    let _ = handle;
    crate::not_implemented!("descriptor_heap::sampler")
}

/// Writes a CBV descriptor at `handle` (`AddConstantBufferView`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_cbv(
    _handle: D3d12CpuDescriptorHandle,
    _va: u64,
    _size_in_bytes: u32,
) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_cbv")
}

/// Writes a texture SRV descriptor at `handle` (`AddShaderResourceView`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_srv_texture(
    _handle: D3d12CpuDescriptorHandle,
    _texture: Option<Arc<Texture>>,
    _view: TextureViewKey,
    _min_lod_clamp: f32,
) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_srv_texture")
}

/// Writes a typed buffer SRV descriptor at `handle` (`AddShaderResourceView`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_srv_texel_buffer(
    _handle: D3d12CpuDescriptorHandle,
    _buffer: Option<Arc<Buffer>>,
    _view: BufferViewKey,
    _slice: BufferSlice,
) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_srv_texel_buffer")
}

/// Writes a raw/structured buffer SRV descriptor at `handle` (`AddShaderResourceView`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_srv_buffer(
    _handle: D3d12CpuDescriptorHandle,
    _buffer: Option<Arc<Buffer>>,
    _slice: BufferSlice,
) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_srv_buffer")
}

/// Writes a null SRV descriptor at `handle` (`AddShaderResourceView`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`; `desc` is caller-supplied.
#[must_use]
pub unsafe fn write_srv_null(_handle: D3d12CpuDescriptorHandle, _desc: *const c_void) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_srv_null")
}

/// Writes a texture UAV descriptor at `handle` (`AddUnorderedAccessView`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_uav_texture(
    _handle: D3d12CpuDescriptorHandle,
    _texture: Option<Arc<Texture>>,
    _view: TextureViewKey,
    _atomic_view: Option<TextureViewKey>,
) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_uav_texture")
}

/// Writes a typed buffer UAV descriptor at `handle` (`AddUnorderedAccessView`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_uav_texel_buffer(
    _handle: D3d12CpuDescriptorHandle,
    _buffer: Option<Arc<Buffer>>,
    _view: BufferViewKey,
    _slice: BufferSlice,
    _atomic_view: Option<BufferViewKey>,
) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_uav_texel_buffer")
}

/// Writes a raw/structured buffer UAV descriptor at `handle` (`AddUnorderedAccessView`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_uav_buffer(
    _handle: D3d12CpuDescriptorHandle,
    _buffer: Option<Arc<Buffer>>,
    _slice: BufferSlice,
    _counter: Option<Arc<Buffer>>,
    _counter_offset: u32,
) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_uav_buffer")
}

/// Writes a null UAV descriptor at `handle` (`AddUnorderedAccessView`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`; `desc` is caller-supplied.
#[must_use]
pub unsafe fn write_uav_null(_handle: D3d12CpuDescriptorHandle, _desc: *const c_void) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_uav_null")
}

/// Writes an RTV descriptor at `handle` (`AddRenderTarget`).
///
/// # Safety
///
/// `handle` must have been produced by a live RTV `Heap`.
#[must_use]
pub unsafe fn write_rtv(_handle: D3d12CpuDescriptorHandle, _desc: RenderTargetDesc) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_rtv")
}

/// Writes a DSV descriptor at `handle` (`AddRenderTarget`).
///
/// # Safety
///
/// `handle` must have been produced by a live DSV `Heap`.
#[must_use]
pub unsafe fn write_dsv(_handle: D3d12CpuDescriptorHandle, _desc: RenderTargetDesc) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_dsv")
}

/// Writes a sampler descriptor at `handle` (`AddSampler`).
///
/// # Safety
///
/// `handle` must have been produced by a live sampler `Heap`; `desc` is caller-supplied.
#[must_use]
pub unsafe fn write_sampler(_handle: D3d12CpuDescriptorHandle, _desc: *const c_void) -> Hresult {
    crate::not_implemented!("descriptor_heap::write_sampler")
}

/// `ID3D12Device::CreateDescriptorHeap` (`d3d12_device.cpp:808`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `heap` are caller-supplied COM pointers.
pub unsafe fn create_descriptor_heap(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateDescriptorHeap");
    crate::not_implemented!("ID3D12Device::CreateDescriptorHeap")
}

/// `ID3D12Device::GetDescriptorHandleIncrementSize` (`d3d12_device.cpp:817`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn get_descriptor_handle_increment_size(_this: *mut c_void, _heap_type: u32) -> u32 {
    crate::scope!("device.GetDescriptorHandleIncrementSize");
    crate::not_implemented!("ID3D12Device::GetDescriptorHandleIncrementSize")
}

/// `ID3D12Device::CreateSampler` (`d3d12_device.cpp:876`).
///
/// # Safety
///
/// `this` and `desc` are caller-supplied pointers.
pub unsafe fn create_sampler(
    _this: *mut c_void,
    _desc: *const c_void,
    _dest: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateSampler");
    let (): () = crate::not_implemented!("ID3D12Device::CreateSampler");
}

/// `ID3D12Device::CopyDescriptors` (`d3d12_device.cpp:888`).
///
/// # Safety
///
/// `this`, `dest_starts`, `dest_sizes`, `src_starts`, and `src_sizes` are caller-supplied pointers.
pub unsafe extern "system-unwind" fn copy_descriptors(
    _this: *mut c_void,
    _num_dest: u32,
    _dest_starts: *const c_void,
    _dest_sizes: *const u32,
    _num_src: u32,
    _src_starts: *const c_void,
    _src_sizes: *const u32,
    _heap_type: u32,
) {
    crate::scope!("device.CopyDescriptors");
    let (): () = crate::not_implemented!("ID3D12Device::CopyDescriptors");
}

/// `ID3D12Device::CopyDescriptorsSimple` (`d3d12_device.cpp:945`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn copy_descriptors_simple(
    _this: *mut c_void,
    _num_descriptors: u32,
    _dest_start: D3d12CpuDescriptorHandle,
    _src_start: D3d12CpuDescriptorHandle,
    _heap_type: u32,
) {
    crate::scope!("device.CopyDescriptorsSimple");
    let (): () = crate::not_implemented!("ID3D12Device::CopyDescriptorsSimple");
}
