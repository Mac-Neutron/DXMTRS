//! `ID3D12DescriptorHeap` (`d3d12_descriptor_heap.{hpp,cpp}`): CBV/SRV/UAV, sampler, RTV, and DSV
//!
//! heaps.

use core::{
    ffi::c_void,
    ptr::{self, read_unaligned, write_unaligned},
    sync::atomic::{AtomicBool, Ordering},
};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use dxmt_core::{
    resource::{Buffer, BufferAllocationFlags, BufferViewKey, Texture, TextureViewKey},
    sampler::Sampler,
};
use dxmt_util::{com::ComObject, dxgi_consts::DXGI_FORMAT_UNKNOWN, private_data::PrivateData};
use rustc_hash::FxHashMap;
use winemetal_sys::wire::{
    WMT_COMPARE_FUNCTION_NEVER, WMT_SAMPLER_ADDRESS_MODE_REPEAT,
    WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE, WMT_SAMPLER_MIN_MAG_FILTER_NEAREST,
    WMT_SAMPLER_MIP_FILTER_NEAREST, WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE_CUBE_ARRAY,
    WMT_TEXTURE_TYPE_TEXTURE_BUFFER, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D,
    WMTSamplerInfo,
};

use crate::{
    abi::{
        objects::{
            D3D12_BUFFER_SRV_FLAG_RAW, D3D12_BUFFER_UAV_FLAG_RAW,
            D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            D3D12_DESCRIPTOR_HEAP_TYPE_DSV, D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER, D3D12_SRV_DIMENSION_BUFFER,
            D3D12_SRV_DIMENSION_TEXTURE1D, D3D12_SRV_DIMENSION_TEXTURE1DARRAY,
            D3D12_SRV_DIMENSION_TEXTURE2D, D3D12_SRV_DIMENSION_TEXTURE2DARRAY,
            D3D12_SRV_DIMENSION_TEXTURE2DMS, D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY,
            D3D12_SRV_DIMENSION_TEXTURE3D, D3D12_SRV_DIMENSION_TEXTURECUBE,
            D3D12_SRV_DIMENSION_TEXTURECUBEARRAY, D3D12_UAV_DIMENSION_BUFFER, DescriptorHeapDesc,
            ShaderResourceViewDescPrefix, UnorderedAccessViewDescPrefix,
        },
        sampler::SamplerDesc,
        vtbl::{
            D3d12CpuDescriptorHandle, D3d12GpuDescriptorHandle, ID3D12DescriptorHeapVtbl,
            IUnknownVtbl,
        },
    },
    com::{
        self, ComRefCounted, E_INVALIDARG, E_NOINTERFACE, E_OUTOFMEMORY, E_POINTER, Guid, Hresult,
        IID_ID3D12_DESCRIPTOR_HEAP, IID_ID3D12_DEVICE_CHILD, IID_ID3D12_OBJECT,
        IID_ID3D12_PAGEABLE, IID_IUNKNOWN, S_OK,
    },
    device::Device,
};

type SamplerMap = FxHashMap<(usize, u32), Arc<Sampler>>;

/// Retains live `Arc<Sampler>` handles indexed by `(heap_addr, descriptor_index)`.
///
/// Keeps underlying `MTLSamplerState` objects valid while stored in a sampler descriptor heap.
static SAMPLER_OBJECTS: LazyLock<Mutex<SamplerMap>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

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

impl EmbeddedDescriptorHandle {
    /// Encodes `(heap, index)` into a `D3D12_CPU_DESCRIPTOR_HANDLE`.
    ///
    /// Uses the 64-bit bitfield layout from `EMBEDDED_DESCRIPTOR_HANDLE`
    /// (`d3d12_descriptor_heap.hpp:43-65`).
    #[must_use]
    pub fn pack(heap: *const Heap, index: u32) -> D3d12CpuDescriptorHandle {
        #[cfg(target_pointer_width = "64")]
        {
            let addr = heap.expose_provenance();
            let tag = (addr >> 3) & 0x1f;
            let desc = (index as usize) & 0x000f_ffff;
            let heap_bits = (addr >> 8) & ((1_usize << 39) - 1);
            D3d12CpuDescriptorHandle {
                ptr: tag | (desc << 5) | (heap_bits << 25),
            }
        }
        #[cfg(target_pointer_width = "32")]
        {
            let _ = (heap, index);
            let (): () = crate::not_implemented!("DescriptorHeap::GetShaderVisibleDescriptor");
            D3d12CpuDescriptorHandle { ptr: 0 }
        }
    }

    /// Decodes a `D3D12_CPU_DESCRIPTOR_HANDLE` into `(heap, index)`.
    ///
    /// Matches `EMBEDDED_DESCRIPTOR_HANDLE` (`d3d12_descriptor_heap.hpp:66-76`).
    #[must_use]
    pub fn unpack(handle: D3d12CpuDescriptorHandle) -> (*const Heap, u32) {
        #[cfg(target_pointer_width = "64")]
        {
            let raw = handle.ptr;
            let tag = raw & 0x1f;
            let index = u32::try_from((raw >> 5) & 0x000f_ffff).unwrap_or(0);
            let heap_bits = raw >> 25;
            let addr = (heap_bits << 8) | (tag << 3);
            (ptr::with_exposed_provenance(addr), index)
        }
        #[cfg(target_pointer_width = "32")]
        {
            let _ = handle;
            let (): () = crate::not_implemented!("DescriptorHeap::GetShaderVisibleDescriptorHeap");
            (ptr::null(), 0)
        }
    }
}

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
    pub gpu_mapped_ptr: *mut [u64; 4],
    pub gpu_base_address: u64,
    pub cpu_words_ptr: *mut [u64; 4],
    pub has_cpu_descriptors: AtomicBool,
    pub cpu_descriptors: Mutex<Vec<ShaderVisibleDescriptor>>,
    pub samplers: Mutex<Vec<[u64; 4]>>,
    pub render_targets: Mutex<Vec<RenderTargetDesc>>,
}

impl ComRefCounted for Heap {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        let heap_addr = ptr::from_ref(self).expose_provenance();
        if let Ok(mut map) = SAMPLER_OBJECTS.lock() {
            map.retain(|&(addr, _), _| addr != heap_addr);
        }
        if let Some(ref buf) = self.gpu_buffer
            && !self.device.is_null()
        {
            let va = self.gpu_address();
            // SAFETY: `self.device` is retained by this heap until released below.
            let dev = unsafe { &*self.device };
            crate::device::residency::unregister(dev, buf, va);
        }
        if !self.device.is_null() {
            // SAFETY: `self.device` was `AddRef`d at construction.
            unsafe { com::release_com(self.device) };
        }
    }
}

static VTBL: ID3D12DescriptorHeapVtbl = ID3D12DescriptorHeapVtbl {
    query_interface,
    add_ref: heap_add_ref,
    release: heap_release,
    get_private_data: get_private_data_fn,
    set_private_data: set_private_data_fn,
    set_private_data_interface: set_private_data_interface_fn,
    set_name: set_name_fn,
    get_device: get_device_fn,
    get_desc: get_desc_fn,
    get_cpu_descriptor_handle_for_heap_start,
    get_gpu_descriptor_handle_for_heap_start,
};

unsafe extern "system-unwind" fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> Hresult {
    crate::scope!("heap.QueryInterface");
    if ppv_object.is_null() {
        return E_POINTER;
    }
    // SAFETY: Caller passed a non-null output pointer.
    unsafe { write_unaligned(ppv_object, ptr::null_mut()) };
    // SAFETY: Reading caller-supplied IID.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    if iid == IID_IUNKNOWN
        || iid == IID_ID3D12_OBJECT
        || iid == IID_ID3D12_DEVICE_CHILD
        || iid == IID_ID3D12_PAGEABLE
        || iid == IID_ID3D12_DESCRIPTOR_HEAP
    {
        // SAFETY: `this` is a valid `Heap` COM pointer.
        unsafe { com::add_ref_com(this.cast::<Heap>()) };
        // SAFETY: `ppv_object` is non-null.
        unsafe { write_unaligned(ppv_object, this) };
        return S_OK;
    }
    crate::warn!("DescriptorHeap::QueryInterface: Unknown interface query {iid}");
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn heap_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `Heap` COM pointer.
    unsafe { com::add_ref_com(this.cast::<Heap>()) }
}

unsafe extern "system-unwind" fn heap_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `Heap` COM pointer.
    unsafe { com::release_com(this.cast::<Heap>()) }
}

unsafe extern "system-unwind" fn get_private_data_fn(
    this: *mut c_void,
    guid: *const c_void,
    data_size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: `guid` is a caller-supplied GUID pointer.
    let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `Heap` pointer.
    let heap = unsafe { &*this.cast::<Heap>() };
    // SAFETY: caller arguments are forwarded.
    unsafe { heap.private_data.get_data(guid, data_size, data) }
}

unsafe extern "system-unwind" fn set_private_data_fn(
    this: *mut c_void,
    guid: *const c_void,
    data_size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: `guid` is a caller-supplied GUID pointer.
    let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `Heap` pointer.
    let heap = unsafe { &*this.cast::<Heap>() };
    // SAFETY: caller arguments are forwarded.
    unsafe { heap.private_data.set_data(guid, data_size, data) }
}

unsafe extern "system-unwind" fn set_private_data_interface_fn(
    this: *mut c_void,
    guid: *const c_void,
    data: *const c_void,
) -> Hresult {
    // SAFETY: `guid` is a caller-supplied GUID pointer.
    let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `Heap` pointer.
    let heap = unsafe { &*this.cast::<Heap>() };
    // SAFETY: caller arguments are forwarded.
    unsafe { heap.private_data.set_interface(guid, data.cast_mut()) }
}

unsafe extern "system-unwind" fn set_name_fn(this: *mut c_void, name: *const c_void) -> Hresult {
    // SAFETY: `this` is a valid `Heap` pointer.
    let heap = unsafe { &*this.cast::<Heap>() };
    // SAFETY: `name` is null or a NUL-terminated UTF-16 string.
    unsafe { com::set_name_private_data(&heap.private_data, name.cast()) }
}

unsafe extern "system-unwind" fn get_device_fn(
    this: *mut c_void,
    riid: *const c_void,
    ppv_device: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a valid `Heap` pointer.
    let dev = unsafe { (*this.cast::<Heap>()).device }.cast::<c_void>();
    if dev.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `dev` is a live `Device` whose vtable starts with `IUnknownVtbl`.
    let unk = unsafe { com::vtbl::<IUnknownVtbl>(dev) };
    // SAFETY: Delegating to parent `Device::QueryInterface`.
    unsafe { (unk.query_interface)(dev, riid, ppv_device) }
}

/// Returns `heap`'s `D3D12_DESCRIPTOR_HEAP_DESC` while recording `heap.GetDesc`.
#[inline(never)]
#[must_use]
pub fn desc_of(heap: &Heap) -> DescriptorHeapDesc {
    crate::scope!("heap.GetDesc");
    DescriptorHeapDesc {
        r#type: heap.heap_type,
        num_descriptors: heap.num_descriptors,
        flags: heap.flags,
        node_mask: 0,
    }
}

unsafe extern "system-unwind" fn get_desc_fn(this: *mut c_void, out: *mut c_void) -> *mut c_void {
    // SAFETY: `this` is a valid `Heap` pointer.
    let heap = unsafe { &*this.cast::<Heap>() };
    let desc = desc_of(heap);
    if !out.is_null() {
        // SAFETY: `out` is non-null caller output storage.
        unsafe { write_unaligned(out.cast::<DescriptorHeapDesc>(), desc) };
    }
    out
}

unsafe extern "system-unwind" fn get_cpu_descriptor_handle_for_heap_start(
    this: *mut c_void,
    out: *mut c_void,
) -> *mut c_void {
    crate::scope!("heap.GetCPUDescriptorHandleForHeapStart");
    let heap_ptr = this.cast::<Heap>().cast_const();
    let handle = EmbeddedDescriptorHandle::pack(heap_ptr, 0);
    if !out.is_null() {
        // SAFETY: `out` is non-null caller output storage.
        unsafe { write_unaligned(out.cast::<D3d12CpuDescriptorHandle>(), handle) };
    }
    out
}

unsafe extern "system-unwind" fn get_gpu_descriptor_handle_for_heap_start(
    this: *mut c_void,
    out: *mut c_void,
) -> *mut c_void {
    crate::scope!("heap.GetGPUDescriptorHandleForHeapStart");
    #[cfg(target_pointer_width = "32")]
    let (): () = crate::not_implemented!("DescriptorHeap::GetGPUDescriptorHandleForHeapStart");
    // SAFETY: `this` is a valid `Heap` pointer.
    let heap = unsafe { &*this.cast::<Heap>() };
    let handle = D3d12GpuDescriptorHandle {
        ptr: heap.gpu_address(),
    };
    if !out.is_null() {
        // SAFETY: `out` is non-null caller output storage.
        unsafe { write_unaligned(out.cast::<D3d12GpuDescriptorHandle>(), handle) };
    }
    out
}

impl Heap {
    /// Returns the CPU-side descriptor metadata at `index` (`GetDescriptor`,
    ///
    /// `d3d12_descriptor_heap.hpp:160`).
    #[must_use]
    pub fn descriptor(&self, index: u32) -> ShaderVisibleDescriptor {
        let guard = self
            .cpu_descriptors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        guard
            .get(index as usize)
            .cloned()
            .unwrap_or(ShaderVisibleDescriptor::Null)
    }

    /// Returns the GPU virtual address of the heap's backing buffer (`0` when not shader-visible).
    #[inline]
    #[must_use]
    pub const fn gpu_address(&self) -> u64 {
        self.gpu_base_address
    }

    /// Returns the base pointer to the heap's 32-byte-per-descriptor `[u64; 4]` backing array.
    ///
    /// Returns the mapped Metal buffer pointer when shader-visible, or CPU `samplers` storage.
    #[inline]
    #[must_use]
    pub const fn words_ptr(&self) -> *mut [u64; 4] {
        if self.gpu_mapped_ptr.is_null() {
            self.cpu_words_ptr
        } else {
            self.gpu_mapped_ptr
        }
    }
}

/// Writes a 32-byte (`[u64; 4]`) GPU descriptor slot in CPU mirror storage or mapped Metal buffer.
#[inline]
const fn write_gpu_entry(heap: &Heap, index: u32, words: [u64; 4]) {
    let base = heap.words_ptr();
    if !base.is_null() && index < heap.num_descriptors {
        // SAFETY: `base` points to `heap.num_descriptors` `[u64; 4]` slots.
        let dst = unsafe { base.add(index as usize) };
        // SAFETY: `dst` points to a 32-byte slot in the descriptor buffer.
        unsafe { write_unaligned(dst, words) };
    }
}

/// Casts a non-null `ID3D12DescriptorHeap` pointer if its vtable matches [`Heap`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_heap<'a>(raw: *mut c_void) -> Option<&'a Heap> {
    if raw.is_null() {
        return None;
    }
    let heap = raw.cast::<Heap>();
    // SAFETY: Caller guarantees `raw` is a readable COM object pointer.
    let vtbl = unsafe { (*heap).vtbl };
    if !ptr::eq(vtbl, &raw const VTBL) {
        return None;
    }
    // SAFETY: Vtable identity confirmed `raw` is a `Heap`.
    Some(unsafe { &*heap })
}

/// Decodes a CBV/SRV/UAV `D3D12_CPU_DESCRIPTOR_HANDLE` into `(heap, index)`.
///
/// # Safety
///
/// `handle` must have been produced by a live `Heap`.
#[must_use]
pub unsafe fn shader_visible<'a>(handle: D3d12CpuDescriptorHandle) -> Option<(&'a Heap, u32)> {
    let (heap_ptr, index) = EmbeddedDescriptorHandle::unpack(handle);
    if heap_ptr.is_null() {
        return None;
    }
    // SAFETY: Caller guarantees `handle` was produced by a live `Heap`.
    let heap = unsafe { &*heap_ptr };
    if heap.heap_type != D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV || index >= heap.num_descriptors {
        return None;
    }
    Some((heap, index))
}

/// Decodes an RTV or DSV `D3D12_CPU_DESCRIPTOR_HANDLE` into its [`RenderTargetDesc`].
///
/// # Safety
///
/// `handle` must have been produced by a live RTV or DSV `Heap`.
#[must_use]
pub unsafe fn render_target(handle: D3d12CpuDescriptorHandle) -> Option<RenderTargetDesc> {
    #[cfg(target_pointer_width = "32")]
    {
        let (): () = crate::not_implemented!("DescriptorHeap::GetRenderTargetDescriptor");
        let (): () = crate::not_implemented!("DescriptorHeap::GetRenderTargetHeap");
    }
    let (heap_ptr, index) = EmbeddedDescriptorHandle::unpack(handle);
    if heap_ptr.is_null() {
        return None;
    }
    // SAFETY: Caller guarantees `handle` was produced by a live RTV or DSV `Heap`.
    let heap = unsafe { &*heap_ptr };
    if (heap.heap_type != D3D12_DESCRIPTOR_HEAP_TYPE_RTV
        && heap.heap_type != D3D12_DESCRIPTOR_HEAP_TYPE_DSV)
        || index >= heap.num_descriptors
    {
        return None;
    }
    let guard = heap
        .render_targets
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    guard.get(index as usize).cloned()
}

/// Decodes a sampler `D3D12_CPU_DESCRIPTOR_HANDLE` into `(heap, index)`.
///
/// # Safety
///
/// `handle` must have been produced by a live sampler `Heap`.
#[must_use]
pub unsafe fn sampler<'a>(handle: D3d12CpuDescriptorHandle) -> Option<(&'a Heap, u32)> {
    #[cfg(target_pointer_width = "32")]
    {
        let (): () = crate::not_implemented!("DescriptorHeap::GetSamplerDescriptor");
        let (): () = crate::not_implemented!("DescriptorHeap::GetSamplerDescriptorHeap");
    }
    let (heap_ptr, index) = EmbeddedDescriptorHandle::unpack(handle);
    if heap_ptr.is_null() {
        return None;
    }
    // SAFETY: Caller guarantees `handle` was produced by a live sampler `Heap`.
    let heap = unsafe { &*heap_ptr };
    if heap.heap_type != D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER || index >= heap.num_descriptors {
        return None;
    }
    Some((heap, index))
}

/// Writes a CBV descriptor at `handle` (`AddConstantBufferView`, `d3d12_descriptor_heap.cpp:283`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_cbv(handle: D3d12CpuDescriptorHandle, va: u64, size_in_bytes: u32) -> Hresult {
    // SAFETY: Caller guarantees `handle` comes from a live CBV/SRV/UAV `Heap`.
    let Some((heap, index)) = (unsafe { shader_visible(handle) }) else {
        return E_INVALIDARG;
    };
    let size = u64::from(size_in_bytes);
    if heap.gpu_mapped_ptr.is_null() {
        let mut cpu = heap
            .cpu_descriptors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = cpu.get_mut(index as usize) {
            *slot = ShaderVisibleDescriptor::ConstantBuffer { address: va, size };
            heap.has_cpu_descriptors.store(true, Ordering::Relaxed);
        }
    }
    write_gpu_entry(heap, index, [va, size, 0, 0]);
    S_OK
}

/// Writes a texture SRV descriptor at `handle` (`AddShaderResourceView`,
///
/// `d3d12_descriptor_heap.cpp:299`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_srv_texture(
    handle: D3d12CpuDescriptorHandle,
    texture: Option<&Texture>,
    view: TextureViewKey,
    min_lod_clamp: f32,
) -> Hresult {
    // SAFETY: Caller guarantees `handle` comes from a live CBV/SRV/UAV `Heap`.
    let Some((heap, index)) = (unsafe { shader_visible(handle) }) else {
        return E_INVALIDARG;
    };
    let (res_id, meta) = texture.map_or((0, 0), |tex| {
        let gpu_id = tex.current().map_or(0, |a| a.view(view).gpu_resource_id);
        let arr_len = u64::from(view.array_size());
        (gpu_id, (arr_len << 32) | u64::from(min_lod_clamp.to_bits()))
    });
    if heap.has_cpu_descriptors.load(Ordering::Relaxed) {
        let mut cpu = heap
            .cpu_descriptors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = cpu.get_mut(index as usize) {
            *slot = ShaderVisibleDescriptor::Null;
        }
    }
    write_gpu_entry(heap, index, [res_id, meta, 0, 0]);
    S_OK
}

/// Writes a typed buffer SRV descriptor at `handle` (`AddShaderResourceView`,
///
/// `d3d12_descriptor_heap.cpp:319`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_srv_texel_buffer(
    handle: D3d12CpuDescriptorHandle,
    buffer: Option<&Buffer>,
    view: BufferViewKey,
    slice: BufferSlice,
) -> Hresult {
    // SAFETY: Caller guarantees `handle` comes from a live CBV/SRV/UAV `Heap`.
    let Some((heap, index)) = (unsafe { shader_visible(handle) }) else {
        return E_INVALIDARG;
    };
    let (res_id, meta) = buffer.map_or((0, 0), |buf| {
        let gpu_id = buf
            .current()
            .map_or(0, |a| buf.view(&a, view).gpu_resource_id);
        let packed = (u64::from(slice.element_count) << 32) | (slice.first_element & 0xffff_ffff);
        (gpu_id, packed)
    });
    if heap.has_cpu_descriptors.load(Ordering::Relaxed) {
        let mut cpu = heap
            .cpu_descriptors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = cpu.get_mut(index as usize) {
            *slot = ShaderVisibleDescriptor::Null;
        }
    }
    write_gpu_entry(heap, index, [res_id, meta, 0, 0]);
    S_OK
}

/// Writes a raw/structured buffer SRV descriptor at `handle` (`AddShaderResourceView`,
///
/// `d3d12_descriptor_heap.cpp:340`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_srv_buffer(
    handle: D3d12CpuDescriptorHandle,
    buffer: Option<&Buffer>,
    slice: BufferSlice,
) -> Hresult {
    // SAFETY: Caller guarantees `handle` comes from a live CBV/SRV/UAV `Heap`.
    let Some((heap, index)) = (unsafe { shader_visible(handle) }) else {
        return E_INVALIDARG;
    };
    let (ptr_val, meta) = buffer.map_or((0, 0), |buf| {
        let va = buf
            .current()
            .map_or(0, |a| a.gpu_address().wrapping_add(slice.byte_offset));
        (va, slice.byte_length)
    });
    if heap.has_cpu_descriptors.load(Ordering::Relaxed) {
        let mut cpu = heap
            .cpu_descriptors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = cpu.get_mut(index as usize) {
            *slot = ShaderVisibleDescriptor::Null;
        }
    }
    write_gpu_entry(heap, index, [ptr_val, meta, 0, 0]);
    S_OK
}

/// Maps `D3D12_SRV_DIMENSION` to the corresponding `WMTTextureType` for null SRV descriptors.
///
/// Matches `NullViewType` (`d3d12_descriptor_heap.cpp:173-197`).
const fn null_view_type(dimension: u32) -> u32 {
    match dimension {
        D3D12_SRV_DIMENSION_TEXTURE1D | D3D12_SRV_DIMENSION_TEXTURE2D => WMT_TEXTURE_TYPE2D,
        D3D12_SRV_DIMENSION_TEXTURE1DARRAY | D3D12_SRV_DIMENSION_TEXTURE2DARRAY => {
            WMT_TEXTURE_TYPE2D_ARRAY
        }
        D3D12_SRV_DIMENSION_TEXTURE2DMS => WMT_TEXTURE_TYPE2D_MULTISAMPLE,
        D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY => WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
        D3D12_SRV_DIMENSION_TEXTURE3D => WMT_TEXTURE_TYPE3D,
        D3D12_SRV_DIMENSION_TEXTURECUBE => WMT_TEXTURE_TYPE_CUBE,
        D3D12_SRV_DIMENSION_TEXTURECUBEARRAY => WMT_TEXTURE_TYPE_CUBE_ARRAY,
        _ => WMT_TEXTURE_TYPE_TEXTURE_BUFFER,
    }
}

/// Writes a null SRV descriptor at `handle` (`AddShaderResourceView`,
///
/// `d3d12_descriptor_heap.cpp:360`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`; `desc` is caller-supplied.
#[must_use]
pub unsafe fn write_srv_null(handle: D3d12CpuDescriptorHandle, desc: *const c_void) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: Caller guarantees `handle` comes from a live CBV/SRV/UAV `Heap`.
    let Some((heap, index)) = (unsafe { shader_visible(handle) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `desc` was checked non-null; reading the `D3D12_SHADER_RESOURCE_VIEW_DESC` prefix.
    let srv_desc = unsafe { read_unaligned(desc.cast::<ShaderResourceViewDescPrefix>()) };
    if srv_desc.view_dimension == D3D12_SRV_DIMENSION_BUFFER {
        if srv_desc.format == DXGI_FORMAT_UNKNOWN
            || (srv_desc.buffer.flags & D3D12_BUFFER_SRV_FLAG_RAW) != 0
        {
            // SAFETY: `handle` is valid.
            return unsafe { write_srv_buffer(handle, None, BufferSlice::default()) };
        }
        if !heap.device.is_null() {
            // SAFETY: `heap.device` is retained by `heap`.
            let dev = unsafe { &*heap.device };
            if let Some((buf, view)) = crate::device::null::null_texel_buffer(dev) {
                // SAFETY: `handle` is valid.
                return unsafe {
                    write_srv_texel_buffer(handle, Some(&buf), view, BufferSlice::default())
                };
            }
        }
    } else {
        let tex_type = null_view_type(srv_desc.view_dimension);
        if tex_type != WMT_TEXTURE_TYPE_TEXTURE_BUFFER && !heap.device.is_null() {
            // SAFETY: `heap.device` is retained by `heap`.
            let dev = unsafe { &*heap.device };
            if let Some((tex, view)) = crate::device::null::null_texture(dev, tex_type) {
                // SAFETY: `handle` is valid.
                return unsafe { write_srv_texture(handle, Some(&tex), view, 0.0) };
            }
        }
    }

    if heap.has_cpu_descriptors.load(Ordering::Relaxed) {
        let mut cpu = heap
            .cpu_descriptors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = cpu.get_mut(index as usize) {
            *slot = ShaderVisibleDescriptor::Null;
        }
    }
    write_gpu_entry(heap, index, [0; 4]);
    S_OK
}

/// Writes a texture UAV descriptor at `handle` (`AddUnorderedAccessView`,
///
/// `d3d12_descriptor_heap.cpp:395`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_uav_texture(
    handle: D3d12CpuDescriptorHandle,
    texture: Option<Arc<Texture>>,
    view: TextureViewKey,
    atomic_view: Option<TextureViewKey>,
) -> Hresult {
    // SAFETY: Caller guarantees `handle` comes from a live CBV/SRV/UAV `Heap`.
    let Some((heap, index)) = (unsafe { shader_visible(handle) }) else {
        return E_INVALIDARG;
    };
    let (res_id, meta, atomic_id) = texture.as_ref().map_or((0, 0, 0), |tex| {
        let alloc = tex.current();
        let gpu_id = alloc.as_ref().map_or(0, |a| a.view(view).gpu_resource_id);
        let arr_len = u64::from(view.array_size()) << 32;
        let atom = match (alloc.as_ref(), atomic_view) {
            (Some(a), Some(av)) => a.view(av).gpu_resource_id,
            _ => 0,
        };
        (gpu_id, arr_len, atom)
    });
    if heap.gpu_mapped_ptr.is_null() {
        let mut cpu = heap
            .cpu_descriptors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = cpu.get_mut(index as usize) {
            *slot = ShaderVisibleDescriptor::UavTexture { texture, view };
            heap.has_cpu_descriptors.store(true, Ordering::Relaxed);
        }
    }
    write_gpu_entry(heap, index, [res_id, meta, atomic_id, 0]);
    S_OK
}

/// Writes a typed buffer UAV descriptor at `handle` (`AddUnorderedAccessView`,
///
/// `d3d12_descriptor_heap.cpp:419`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_uav_texel_buffer(
    handle: D3d12CpuDescriptorHandle,
    buffer: Option<Arc<Buffer>>,
    view: BufferViewKey,
    slice: BufferSlice,
    atomic_view: Option<BufferViewKey>,
) -> Hresult {
    // SAFETY: Caller guarantees `handle` comes from a live CBV/SRV/UAV `Heap`.
    let Some((heap, index)) = (unsafe { shader_visible(handle) }) else {
        return E_INVALIDARG;
    };
    let words = buffer.as_ref().map_or([0; 4], |buf| {
        let alloc = buf.current();
        let gpu_id = alloc
            .as_ref()
            .map_or(0, |a| buf.view(a, view).gpu_resource_id);
        let packed = (u64::from(slice.element_count) << 32) | (slice.first_element & 0xffff_ffff);
        let atom = match (alloc.as_ref(), atomic_view) {
            (Some(a), Some(av)) => buf.view(a, av).gpu_resource_id,
            _ => 0,
        };
        let addr = alloc.as_ref().map_or(0, |a| a.gpu_address());
        [gpu_id, packed, atom, addr]
    });
    if heap.gpu_mapped_ptr.is_null() {
        let mut cpu = heap
            .cpu_descriptors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = cpu.get_mut(index as usize) {
            *slot = ShaderVisibleDescriptor::UavTexelBuffer {
                buffer,
                view,
                slice,
            };
            heap.has_cpu_descriptors.store(true, Ordering::Relaxed);
        }
    }
    write_gpu_entry(heap, index, words);
    S_OK
}

/// Writes a raw/structured buffer UAV descriptor at `handle` (`AddUnorderedAccessView`,
///
/// `d3d12_descriptor_heap.cpp:446`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`.
#[must_use]
pub unsafe fn write_uav_buffer(
    handle: D3d12CpuDescriptorHandle,
    buffer: Option<Arc<Buffer>>,
    slice: BufferSlice,
    counter: Option<Arc<Buffer>>,
    counter_offset: u32,
) -> Hresult {
    // SAFETY: Caller guarantees `handle` comes from a live CBV/SRV/UAV `Heap`.
    let Some((heap, index)) = (unsafe { shader_visible(handle) }) else {
        return E_INVALIDARG;
    };
    let ctr_va = counter.and_then(|c| c.current()).map_or(0, |a| {
        a.gpu_address().wrapping_add(u64::from(counter_offset))
    });
    let words = buffer.as_ref().map_or([0; 4], |buf| {
        let va = buf
            .current()
            .map_or(0, |a| a.gpu_address().wrapping_add(slice.byte_offset));
        [va, slice.byte_length, ctr_va, 0]
    });
    if heap.gpu_mapped_ptr.is_null() {
        let mut cpu = heap
            .cpu_descriptors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = cpu.get_mut(index as usize) {
            *slot = ShaderVisibleDescriptor::UavBuffer { buffer, slice };
            heap.has_cpu_descriptors.store(true, Ordering::Relaxed);
        }
    }
    write_gpu_entry(heap, index, words);
    S_OK
}

/// Writes a null UAV descriptor at `handle` (`AddUnorderedAccessView`,
///
/// `d3d12_descriptor_heap.cpp:475`).
///
/// # Safety
///
/// `handle` must have been produced by a live CBV/SRV/UAV `Heap`; `desc` is caller-supplied.
#[must_use]
pub unsafe fn write_uav_null(handle: D3d12CpuDescriptorHandle, desc: *const c_void) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: Caller guarantees `handle` comes from a live CBV/SRV/UAV `Heap`.
    let Some((heap, index)) = (unsafe { shader_visible(handle) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `desc` was checked non-null; reading the `D3D12_UNORDERED_ACCESS_VIEW_DESC` prefix.
    let uav_desc = unsafe { read_unaligned(desc.cast::<UnorderedAccessViewDescPrefix>()) };
    if uav_desc.view_dimension == D3D12_UAV_DIMENSION_BUFFER {
        if uav_desc.format == DXGI_FORMAT_UNKNOWN
            || (uav_desc.buffer.flags & D3D12_BUFFER_UAV_FLAG_RAW) != 0
        {
            // SAFETY: `handle` is valid.
            return unsafe { write_uav_buffer(handle, None, BufferSlice::default(), None, 0) };
        }
        if !heap.device.is_null() {
            // SAFETY: `heap.device` is retained by `heap`.
            let dev = unsafe { &*heap.device };
            if let Some((buf, view)) = crate::device::null::null_texel_buffer(dev) {
                // SAFETY: `handle` is valid.
                return unsafe {
                    write_uav_texel_buffer(handle, Some(buf), view, BufferSlice::default(), None)
                };
            }
        }
    }

    if heap.has_cpu_descriptors.load(Ordering::Relaxed) {
        let mut cpu = heap
            .cpu_descriptors
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = cpu.get_mut(index as usize) {
            *slot = ShaderVisibleDescriptor::Null;
        }
    }
    write_gpu_entry(heap, index, [0; 4]);
    S_OK
}

/// Writes an RTV descriptor at `handle` (`AddRenderTarget`, `d3d12_descriptor_heap.cpp:501`).
///
/// # Safety
///
/// `handle` must have been produced by a live RTV `Heap`.
#[must_use]
pub unsafe fn write_rtv(handle: D3d12CpuDescriptorHandle, desc: RenderTargetDesc) -> Hresult {
    let (heap_ptr, index) = EmbeddedDescriptorHandle::unpack(handle);
    if heap_ptr.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: Caller guarantees `handle` comes from a live RTV `Heap`.
    let heap = unsafe { &*heap_ptr };
    if heap.heap_type != D3D12_DESCRIPTOR_HEAP_TYPE_RTV || index >= heap.num_descriptors {
        return E_INVALIDARG;
    }
    let mut rts = heap
        .render_targets
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(slot) = rts.get_mut(index as usize) {
        *slot = desc;
    }
    drop(rts);
    S_OK
}

/// Writes a DSV descriptor at `handle` (`AddRenderTarget`, `d3d12_descriptor_heap.cpp:501`).
///
/// # Safety
///
/// `handle` must have been produced by a live DSV `Heap`.
#[must_use]
pub unsafe fn write_dsv(handle: D3d12CpuDescriptorHandle, desc: RenderTargetDesc) -> Hresult {
    let (heap_ptr, index) = EmbeddedDescriptorHandle::unpack(handle);
    if heap_ptr.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: Caller guarantees `handle` comes from a live DSV `Heap`.
    let heap = unsafe { &*heap_ptr };
    if heap.heap_type != D3D12_DESCRIPTOR_HEAP_TYPE_DSV || index >= heap.num_descriptors {
        return E_INVALIDARG;
    }
    let mut rts = heap
        .render_targets
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(slot) = rts.get_mut(index as usize) {
        *slot = desc;
    }
    drop(rts);
    S_OK
}

/// Writes a sampler descriptor at `handle` (`AddSampler`, `d3d12_descriptor_heap.cpp:508`,
///
/// `d3d12_sampler.cpp:137`).
///
/// # Safety
///
/// `handle` must have been produced by a live sampler `Heap`; `desc` is caller-supplied.
#[must_use]
pub unsafe fn write_sampler(handle: D3d12CpuDescriptorHandle, desc: *const c_void) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: Caller guarantees `handle` comes from a live sampler `Heap`.
    let Some((heap, index)) = (unsafe { sampler(handle) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `desc` was checked non-null; reading `D3D12_SAMPLER_DESC`.
    let samp_desc = unsafe { read_unaligned(desc.cast::<SamplerDesc>()) };

    let mut info = WMTSamplerInfo {
        min_filter: WMT_SAMPLER_MIN_MAG_FILTER_NEAREST,
        mag_filter: WMT_SAMPLER_MIN_MAG_FILTER_NEAREST,
        mip_filter: WMT_SAMPLER_MIP_FILTER_NEAREST,
        r_address_mode: WMT_SAMPLER_ADDRESS_MODE_REPEAT,
        s_address_mode: WMT_SAMPLER_ADDRESS_MODE_REPEAT,
        t_address_mode: WMT_SAMPLER_ADDRESS_MODE_REPEAT,
        border_color: WMT_SAMPLER_BORDER_COLOR_OPAQUE_WHITE,
        compare_function: WMT_COMPARE_FUNCTION_NEVER,
        lod_min_clamp: 0.0,
        lod_max_clamp: 0.0,
        max_anisotroy: 1,
        normalized_coords: 1,
        lod_average: 0,
        support_argument_buffers: 1,
        gpu_resource_id: 0,
    };
    crate::sampler::populate_sampler_info(&mut info, &samp_desc, &mut |warn| {
        crate::warn!("{warn}");
    });

    let mut words = [0, 0, u64::from(samp_desc.mip_lod_bias.to_bits()), 0];
    if !heap.device.is_null() {
        // SAFETY: `heap.device` is retained by `heap`.
        let dev = unsafe { &*heap.device };
        if let Some(ref mtl) = dev.mtl
            && let Some(samp) = Sampler::create(mtl, &info, samp_desc.mip_lod_bias)
        {
            words[0] = samp.gpu_resource_id();
            words[1] = samp.gpu_resource_id_cube();
            words[2] = u64::from(samp.lod_bias().to_bits());
            let heap_addr = ptr::from_ref(heap).expose_provenance();
            if let Ok(mut map) = SAMPLER_OBJECTS.lock() {
                map.insert((heap_addr, index), samp);
            }
        }
    }

    write_gpu_entry(heap, index, words);
    S_OK
}

/// `ID3D12Device::CreateDescriptorHeap` (`d3d12_device.cpp:808`, `d3d12_descriptor_heap.cpp:15`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `heap` are caller-supplied COM pointers.
pub unsafe fn create_descriptor_heap(
    this: *mut c_void,
    desc: *const c_void,
    riid: *const c_void,
    heap: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateDescriptorHeap");
    if !heap.is_null() {
        // SAFETY: Zeroing caller output pointer before validation.
        unsafe { write_unaligned(heap, ptr::null_mut()) };
    }
    if desc.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` was checked non-null.
    let heap_desc = unsafe { read_unaligned(desc.cast::<DescriptorHeapDesc>()) };

    let is_shader_visible = (heap_desc.flags & D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE) != 0;
    if is_shader_visible
        && (heap_desc.r#type == D3D12_DESCRIPTOR_HEAP_TYPE_RTV
            || heap_desc.r#type == D3D12_DESCRIPTOR_HEAP_TYPE_DSV)
    {
        return E_INVALIDARG;
    }
    if heap_desc.num_descriptors > 0x000f_ffff {
        crate::err!(
            "Failed to create descriptor heap: NumDescriptors ({}) > {}",
            heap_desc.num_descriptors,
            0x000f_ffff
        );
        return E_INVALIDARG;
    }
    match heap_desc.r#type {
        D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV
        | D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER
        | D3D12_DESCRIPTOR_HEAP_TYPE_RTV
        | D3D12_DESCRIPTOR_HEAP_TYPE_DSV => {}
        _ => return E_INVALIDARG,
    }

    let device_ptr = this.cast::<Device>();
    let mut gpu_buffer: Option<Arc<Buffer>> = None;
    let mut gpu_mapped_ptr: *mut [u64; 4] = ptr::null_mut();
    let mut gpu_base_address: u64 = 0;
    if is_shader_visible {
        #[cfg(target_pointer_width = "32")]
        let (): () = crate::not_implemented!("DescriptorHeap::CreateShaderVisible");
        if !device_ptr.is_null() {
            // SAFETY: `device_ptr` is a valid `Device` COM pointer when non-null.
            let dev = unsafe { &*device_ptr };
            if let Some(ref mtl) = dev.mtl {
                let buf_len = u64::from(heap_desc.num_descriptors) * 32;
                let buf = Arc::new(Buffer::new(buf_len.max(32), mtl));
                let flags =
                    BufferAllocationFlags::GPU_READONLY | BufferAllocationFlags::CPU_WRITE_COMBINED;
                let alloc = buf.allocate(flags);
                let mapped = alloc.mapped_memory(0);
                if mapped.is_null() {
                    return E_OUTOFMEMORY;
                }
                if let Ok(len) = usize::try_from(buf_len)
                    && len != 0
                {
                    // SAFETY: Zero-initializing the newly allocated descriptor buffer.
                    unsafe { ptr::write_bytes(mapped.cast::<u8>(), 0, len) };
                }
                gpu_mapped_ptr = mapped.cast::<c_void>().cast::<[u64; 4]>();
                gpu_base_address = alloc.gpu_address();
                let _ = buf.rename(alloc);
                crate::device::residency::register(dev, &buf);
                gpu_buffer = Some(buf);
            }
        }
    }

    let count = heap_desc.num_descriptors as usize;
    let (cpu_descriptors, mut samplers, render_targets) = match heap_desc.r#type {
        D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV => {
            if gpu_mapped_ptr.is_null() {
                (
                    vec![ShaderVisibleDescriptor::Null; count],
                    vec![[0u64; 4]; count],
                    Vec::new(),
                )
            } else {
                (Vec::new(), Vec::new(), Vec::new())
            }
        }
        D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER => {
            if gpu_mapped_ptr.is_null() {
                (Vec::new(), vec![[0u64; 4]; count], Vec::new())
            } else {
                (Vec::new(), Vec::new(), Vec::new())
            }
        }
        D3D12_DESCRIPTOR_HEAP_TYPE_RTV | D3D12_DESCRIPTOR_HEAP_TYPE_DSV => {
            let empty_rt = RenderTargetDesc {
                texture: None,
                view: dxmt_core::resource::TextureViewDescriptor {
                    format: 0,
                    texture_type: 0,
                    first_mip: 0,
                    mip_count: 0,
                    first_slice: 0,
                    array_size: 0,
                }
                .key(),
                width: 0,
                height: 0,
                depth_plane: 0,
                render_target_array_length: 0,
                flags: 0,
            };
            (Vec::new(), Vec::new(), vec![empty_rt; count])
        }
        _ => (Vec::new(), Vec::new(), Vec::new()),
    };
    let cpu_words_ptr = if samplers.is_empty() {
        ptr::null_mut()
    } else {
        samplers.as_mut_ptr()
    };

    if !device_ptr.is_null() {
        // SAFETY: Retaining parent `Device` for the lifetime of `Heap`.
        unsafe { com::add_ref_com(device_ptr) };
    }

    let boxed = Box::new(Heap {
        vtbl: &VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: device_ptr,
        heap_type: heap_desc.r#type,
        num_descriptors: heap_desc.num_descriptors,
        flags: heap_desc.flags,
        gpu_buffer,
        gpu_mapped_ptr,
        gpu_base_address,
        cpu_words_ptr,
        has_cpu_descriptors: AtomicBool::new(false),
        cpu_descriptors: Mutex::new(cpu_descriptors),
        samplers: Mutex::new(samplers),
        render_targets: Mutex::new(render_targets),
    });
    let raw = Box::into_raw(boxed).cast::<c_void>();
    if heap.is_null() {
        // SAFETY: Validation-only call (`ppvHeap == NULL`); release the temporary object.
        let hr = unsafe { query_interface(raw, riid, ptr::null_mut()) };
        // SAFETY: Balance the initial reference count of 1.
        unsafe { heap_release(raw) };
        return if hr == E_POINTER { S_OK } else { hr };
    }
    // SAFETY: Querying the requested interface into `heap`.
    let hr = unsafe { query_interface(raw, riid, heap) };
    // SAFETY: Balance the initial reference count of 1 from `ComObject::with_initial_ref()`.
    unsafe { heap_release(raw) };
    hr
}

/// `ID3D12Device::GetDescriptorHandleIncrementSize` (`d3d12_device.cpp:817`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn get_descriptor_handle_increment_size(_this: *mut c_void, heap_type: u32) -> u32 {
    crate::scope!("device.GetDescriptorHandleIncrementSize");
    match heap_type {
        D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV
        | D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER
        | D3D12_DESCRIPTOR_HEAP_TYPE_RTV
        | D3D12_DESCRIPTOR_HEAP_TYPE_DSV => 32,
        _ => {
            crate::err!(
                "D3D12Device::GetDescriptorHandleIncrementSize: Invalid descriptor heap type {heap_type}"
            );
            0
        }
    }
}

/// `ID3D12Device::CreateSampler` (`d3d12_device.cpp:876`).
///
/// # Safety
///
/// `this` and `sampler_desc` are caller-supplied pointers.
pub unsafe fn create_sampler(
    _this: *mut c_void,
    sampler_desc: *const c_void,
    dest_descriptor: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateSampler");
    // SAFETY: Forwarding `sampler_desc` and `dest_descriptor` to `write_sampler`.
    let _ = unsafe { write_sampler(dest_descriptor, sampler_desc) };
}

/// Copies `count` contiguous descriptors from `(src_heap, src_idx)` to `(dst_heap, dst_idx)`.
fn copy_descriptor_range(
    dst_heap: &Heap,
    dst_idx: u32,
    src_heap: &Heap,
    src_idx: u32,
    count: u32,
    heap_type: u32,
) {
    if count == 0 || dst_idx >= dst_heap.num_descriptors || src_idx >= src_heap.num_descriptors {
        return;
    }
    let avail_u32 = (dst_heap.num_descriptors - dst_idx)
        .min(src_heap.num_descriptors - src_idx)
        .min(count);
    let avail = avail_u32 as usize;
    if avail == 0 {
        return;
    }
    let src_start = src_idx as usize;
    let dst_start = dst_idx as usize;
    match heap_type {
        D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV => {
            let src_words = src_heap.words_ptr();
            let dst_words = dst_heap.words_ptr();
            if !src_words.is_null() && !dst_words.is_null() {
                // SAFETY: `src_start + avail <= src_heap.num_descriptors`.
                let src_ptr = unsafe { src_words.add(src_start) };
                // SAFETY: `dst_start + avail <= dst_heap.num_descriptors`.
                let dst_ptr = unsafe { dst_words.add(dst_start) };
                if ptr::eq(src_words, dst_words) {
                    // SAFETY: Both ranges are valid within the same descriptor array.
                    unsafe { ptr::copy(src_ptr, dst_ptr, avail) };
                } else {
                    // SAFETY: Different descriptor arrays do not overlap.
                    unsafe { ptr::copy_nonoverlapping(src_ptr, dst_ptr, avail) };
                }
            }
            if dst_heap.gpu_mapped_ptr.is_null()
                && (src_heap.has_cpu_descriptors.load(Ordering::Relaxed)
                    || dst_heap.has_cpu_descriptors.load(Ordering::Relaxed))
            {
                if ptr::eq(src_heap, dst_heap) {
                    let mut cpu = dst_heap
                        .cpu_descriptors
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    for i in 0..avail {
                        if let Some(val) = cpu.get(src_start + i).cloned()
                            && let Some(slot) = cpu.get_mut(dst_start + i)
                        {
                            *slot = val;
                        }
                    }
                } else {
                    let src_cpu = src_heap
                        .cpu_descriptors
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    let mut dst_cpu = dst_heap
                        .cpu_descriptors
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    if let Some(src_slice) = src_cpu.get(src_start..src_start + avail)
                        && let Some(dst_slice) = dst_cpu.get_mut(dst_start..dst_start + avail)
                    {
                        dst_slice.clone_from_slice(src_slice);
                        dst_heap.has_cpu_descriptors.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
        D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER => {
            let src_words = src_heap.words_ptr();
            let dst_words = dst_heap.words_ptr();
            if !src_words.is_null() && !dst_words.is_null() {
                // SAFETY: `src_start + avail <= src_heap.num_descriptors`.
                let src_ptr = unsafe { src_words.add(src_start) };
                // SAFETY: `dst_start + avail <= dst_heap.num_descriptors`.
                let dst_ptr = unsafe { dst_words.add(dst_start) };
                if ptr::eq(src_words, dst_words) {
                    // SAFETY: Both ranges are valid within the same descriptor array.
                    unsafe { ptr::copy(src_ptr, dst_ptr, avail) };
                } else {
                    // SAFETY: Different descriptor arrays do not overlap.
                    unsafe { ptr::copy_nonoverlapping(src_ptr, dst_ptr, avail) };
                }
            }
            let src_addr = ptr::from_ref(src_heap).expose_provenance();
            let dst_addr = ptr::from_ref(dst_heap).expose_provenance();
            if let Ok(mut map) = SAMPLER_OBJECTS.lock() {
                for i in 0..avail_u32 {
                    let s_idx = src_idx.wrapping_add(i);
                    let d_idx = dst_idx.wrapping_add(i);
                    if let Some(samp) = map.get(&(src_addr, s_idx)).cloned() {
                        map.insert((dst_addr, d_idx), samp);
                    } else {
                        map.remove(&(dst_addr, d_idx));
                    }
                }
            }
        }
        D3D12_DESCRIPTOR_HEAP_TYPE_RTV | D3D12_DESCRIPTOR_HEAP_TYPE_DSV => {
            if ptr::eq(src_heap, dst_heap) {
                let mut rts = dst_heap
                    .render_targets
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                for i in 0..avail {
                    if let Some(rt_desc) = rts.get(src_start + i).cloned()
                        && let Some(slot) = rts.get_mut(dst_start + i)
                    {
                        *slot = rt_desc;
                    }
                }
            } else {
                let src_rts = src_heap
                    .render_targets
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let mut dst_rts = dst_heap
                    .render_targets
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                if let Some(src_slice) = src_rts.get(src_start..src_start + avail)
                    && let Some(dst_slice) = dst_rts.get_mut(dst_start..dst_start + avail)
                {
                    dst_slice.clone_from_slice(src_slice);
                }
            }
        }
        _ => {}
    }
}

/// `ID3D12Device::CopyDescriptors` (`d3d12_device.cpp:888`).
///
/// # Safety
///
/// `this`, `dest_starts`, `dest_sizes`, `src_starts`, and `src_sizes` are caller-supplied pointers.
pub unsafe extern "system-unwind" fn copy_descriptors(
    _this: *mut c_void,
    num_dest: u32,
    dest_starts: *const c_void,
    dest_sizes: *const u32,
    num_src: u32,
    src_starts: *const c_void,
    src_sizes: *const u32,
    heap_type: u32,
) {
    crate::scope!("device.CopyDescriptors");
    if num_dest == 0 || num_src == 0 || dest_starts.is_null() || src_starts.is_null() {
        return;
    }
    let dst_handles = dest_starts.cast::<D3d12CpuDescriptorHandle>();
    let src_handles = src_starts.cast::<D3d12CpuDescriptorHandle>();

    let mut dst_range_idx = 0u32;
    let mut dst_offset = 0u32;
    let mut src_range_idx = 0u32;
    let mut src_offset = 0u32;

    while dst_range_idx < num_dest && src_range_idx < num_src {
        let dst_range_len = if dest_sizes.is_null() {
            1
        } else {
            // SAFETY: `dst_range_idx < num_dest` and `dest_sizes` is non-null.
            let p = unsafe { dest_sizes.add(dst_range_idx as usize) };
            // SAFETY: Reading range size element.
            unsafe { read_unaligned(p) }
        };
        if dst_offset >= dst_range_len {
            dst_range_idx += 1;
            dst_offset = 0;
            continue;
        }

        let src_range_len = if src_sizes.is_null() {
            1
        } else {
            // SAFETY: `src_range_idx < num_src` and `src_sizes` is non-null.
            let p = unsafe { src_sizes.add(src_range_idx as usize) };
            // SAFETY: Reading range size element.
            unsafe { read_unaligned(p) }
        };
        if src_offset >= src_range_len {
            src_range_idx += 1;
            src_offset = 0;
            continue;
        }

        let run_len = (dst_range_len - dst_offset).min(src_range_len - src_offset);

        // SAFETY: `dst_range_idx < num_dest` and `dst_handles` is non-null.
        let dst_base_ptr = unsafe { dst_handles.add(dst_range_idx as usize) };
        // SAFETY: Reading destination start handle.
        let dst_base = unsafe { read_unaligned(dst_base_ptr) };
        // SAFETY: `src_range_idx < num_src` and `src_handles` is non-null.
        let src_base_ptr = unsafe { src_handles.add(src_range_idx as usize) };
        // SAFETY: Reading source start handle.
        let src_base = unsafe { read_unaligned(src_base_ptr) };

        let (dst_heap_ptr, dst_base_idx) = EmbeddedDescriptorHandle::unpack(dst_base);
        let (src_heap_ptr, src_base_idx) = EmbeddedDescriptorHandle::unpack(src_base);
        if !dst_heap_ptr.is_null() && !src_heap_ptr.is_null() {
            // SAFETY: Both handles come from live `Heap` allocations.
            let dst_heap = unsafe { &*dst_heap_ptr };
            // SAFETY: Both handles come from live `Heap` allocations.
            let src_heap = unsafe { &*src_heap_ptr };
            copy_descriptor_range(
                dst_heap,
                dst_base_idx.wrapping_add(dst_offset),
                src_heap,
                src_base_idx.wrapping_add(src_offset),
                run_len,
                heap_type,
            );
        }

        dst_offset += run_len;
        src_offset += run_len;
    }
}

/// `ID3D12Device::CopyDescriptorsSimple` (`d3d12_device.cpp:945`).
///
/// # Safety
///
/// `this` is a valid `Device` pointer.
pub unsafe fn copy_descriptors_simple(
    _this: *mut c_void,
    num_descriptors: u32,
    dest_start: D3d12CpuDescriptorHandle,
    src_start: D3d12CpuDescriptorHandle,
    heap_type: u32,
) {
    crate::scope!("device.CopyDescriptorsSimple");
    if num_descriptors == 0 {
        return;
    }
    let (dst_heap_ptr, dst_idx) = EmbeddedDescriptorHandle::unpack(dest_start);
    let (src_heap_ptr, src_idx) = EmbeddedDescriptorHandle::unpack(src_start);
    if !dst_heap_ptr.is_null() && !src_heap_ptr.is_null() {
        // SAFETY: Both handles come from live `Heap` allocations.
        let dst_heap = unsafe { &*dst_heap_ptr };
        // SAFETY: Both handles come from live `Heap` allocations.
        let src_heap = unsafe { &*src_heap_ptr };
        copy_descriptor_range(
            dst_heap,
            dst_idx,
            src_heap,
            src_idx,
            num_descriptors,
            heap_type,
        );
    }
}

#[cfg(test)]
mod tests;
