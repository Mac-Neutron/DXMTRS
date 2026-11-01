//! `ID3D12CommandAllocator` (`d3d12_command_allocator.{hpp,cpp}`): CPU/GPU command arenas, encoder
//!
//! dependency DAG construction (`Decide`), clear folding (`FoldClears`), and pooled ICBs.

use core::{
    ffi::{CStr, c_void},
    mem::{align_of, size_of},
    ptr,
};
use std::{
    alloc::{self, Layout},
    sync::{Arc, LazyLock},
};

use dxmt_core::{
    library::{ClearRtv, ClearUav, Library, ResourceAccess, SimpleCommandContext},
    resource::{Buffer, BufferAllocationFlags, BufferViewKey, Texture, TextureViewKey},
    ring::{LIFETIME, RingBump, RingConfig, STAGING_BLOCK},
};
use dxmt_util::{com::ComObject, private_data::PrivateData};
use rustc_hash::FxHashMap;
use winemetal_sys::{
    MTLDevice_newBuffer, MTLDevice_newComputePipelineState, MTLDevice_newIndirectCommandBuffer,
    NSObject_description, NSString_getCString, NSString_lengthOfBytesUsingEncoding, Obj, Pool,
};
use winemetal_wire::{
    MemPtr, WMT_COMPUTE_COMMAND_DISPATCH, WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
    WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER, WMT_COMPUTE_COMMAND_NOP,
    WMT_COMPUTE_COMMAND_SET_BUFFER, WMT_COMPUTE_COMMAND_SET_BYTES, WMT_COMPUTE_COMMAND_SET_PSO,
    WMT_COMPUTE_COMMAND_SET_TEXTURE, WMT_COMPUTE_COMMAND_USE_RESOURCE,
    WMT_INDIRECT_COMMAND_TYPE_CONCURRENT_DISPATCH, WMT_INDIRECT_COMMAND_TYPE_DRAW,
    WMT_INDIRECT_COMMAND_TYPE_DRAW_INDEXED, WMT_LOAD_ACTION_CLEAR, WMT_LOAD_ACTION_LOAD,
    WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER, WMT_RENDER_COMMAND_NOP,
    WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE, WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
    WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
    WMT_RENDER_COMMAND_SET_STENCIL_REF, WMT_RENDER_COMMAND_SET_VIEWPORT,
    WMT_RENDER_COMMAND_USE_RESOURCE, WMT_RENDER_STAGE_VERTEX,
    WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
    WMT_RESOURCE_STORAGE_MODE_SHARED, WMT_RESOURCE_USAGE_READ, WMT_RESOURCE_USAGE_WRITE,
    WMT_STORE_ACTION_STORE, WMT_TEXTURE_TYPE3D, WMTBufferInfo, WMTComputePipelineInfo,
    WMTIndirectCommandBufferInfo, WMTScissorRect, WMTSize, WMTUTF8_STRING_ENCODING, WMTViewport,
    WmtcmdBase, WmtcmdComputeDispatch, WmtcmdComputeExecutecommands, WmtcmdComputeSetbuffer,
    WmtcmdComputeSetbytes, WmtcmdComputeSetpso, WmtcmdComputeSettexture, WmtcmdComputeUseresource,
    WmtcmdRenderDraw, WmtcmdRenderExecutecommands, WmtcmdRenderSetbytes,
    WmtcmdRenderSetdepthstencilstate, WmtcmdRenderSetpso, WmtcmdRenderSetscissorrect,
    WmtcmdRenderSetstencilref, WmtcmdRenderSetviewport, WmtcmdRenderUseresource,
};

use crate::{
    abi::vtbl::ID3D12CommandAllocatorVtbl,
    com::{
        self, ComRefCounted, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_OUTOFMEMORY, E_POINTER, Guid,
        Hresult, IID_ID3D12_COMMAND_ALLOCATOR, IID_ID3D12_DEVICE_CHILD, IID_ID3D12_OBJECT,
        IID_ID3D12_PAGEABLE, IID_IUNKNOWN, S_OK,
    },
    command_signature::CommandSignature,
    device::{self, Device},
    encoder::{
        self, CLEAR_DEPTH_STENCIL, ClearEncoderData, ComputeEncoderData, EncoderData, EncoderType,
        RenderEncoderData, TextureViewRef,
    },
    pipeline::{Compute, Graphics},
};

/// CPU command arena block size (`kCPUHeapSize`, `d3d12_command_allocator.hpp:29`).
pub const CPU_HEAP_SIZE: usize = 0x40_0000;

/// GPU argument/constant arena block size (`kGPUHeapSize`, `d3d12_command_allocator.hpp:30`).
pub const GPU_HEAP_SIZE: usize = 0x40_0000;

/// CPU heap block alignment (`DXMT_PAGE_SIZE`, `d3d12_command_allocator.cpp:55`).
const CPU_HEAP_ALIGN: usize = 4096;

/// GPU heap block alignment (`16384`, `d3d12_command_allocator.cpp:66`).
const GPU_HEAP_ALIGN: usize = 16384;

/// Maximum valid `D3D12_COMMAND_LIST_TYPE` accepted by `CreateCommandAllocator` (`COPY = 3`).
const MAX_COMMAND_LIST_TYPE: u32 = 3;

/// `D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED` (`d3d12.idl`).
const D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED: u32 = 1;

/// Cached `DXMT_D3D12_MERGE` environment variable (`d3d12_command_allocator.hpp:181-183`).
static MERGE_ENABLED: LazyLock<bool> =
    LazyLock::new(|| std::env::var("DXMT_D3D12_MERGE").map_or(true, |val| val != "0"));

/// Compute `ExecuteIndirect` resolver argument struct (`IndirectComputeCommandData`,
///
/// `d3d12_command_allocator.hpp:44-55`).
#[repr(C)]
pub struct IndirectComputeCommandData {
    pub cmd_buf: u64,
    pub max_count: u64,
    pub max_count_buffer: u64,
    pub argument_buffer: u64,
    pub static_samplers: u64,
    pub rootsig_qwords: u64,
    pub rootsig_qwords_stride: u32,
    pub tgsize_x: u32,
    pub tgsize_y: u32,
    pub tgsize_z: u32,
}

const _: () = assert!(size_of::<IndirectComputeCommandData>() == 64);

/// Render `ExecuteIndirect` resolver argument struct (`IndirectRenderCommandData`,
///
/// `d3d12_command_allocator.hpp:57-72`).
#[repr(C)]
pub struct IndirectRenderCommandData {
    pub cmd_buf: u64,
    pub max_count: u64,
    pub max_count_buffer: u64,
    pub argument_buffer: u64,
    pub static_samplers: u64,
    pub rootsig_qwords: u64,
    pub rootsig_qwords_stride: u32,
    pub primitive_type: u32,
    pub vertex_buffer: u64,
    pub index_buffer: u64,
    pub index_buffer_format: u32,
    pub vertex_argbuf_stride: u32,
    pub vertex_slot_mask: u32,
    pub null_vertex_buffer: u64,
}

const _: () = assert!(size_of::<IndirectRenderCommandData>() == 96);

/// Pooled `MTLIndirectCommandBuffer` entry (`PooledICB`, `d3d12_command_allocator.hpp:110-114`).
pub struct PooledIcb {
    pub key: u64,
    pub icb: Option<Obj>,
    pub resource_id: u64,
}

/// `MTLD3D12CommandAllocatorImpl` (`d3d12_command_allocator.hpp:74-126`).
#[repr(C)]
pub struct Allocator {
    pub vtbl: &'static ID3D12CommandAllocatorVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub list_type: u32,
    pub spilled_cpu_heap: Vec<*mut u8>,
    pub cpu_heap: *mut u8,
    pub cpu_heap_offset: usize,
    pub gpu_heap_buffer: Option<Obj>,
    pub gpu_heap: *mut u8,
    pub gpu_heap_offset: usize,
    pub gpu_heap_buffer_address: u64,
    pub encoder_last: *mut EncoderData,
    pub encoder_current: *mut EncoderData,
    pub encoder_count: usize,
    pub barriers: u32,
    pub joins: Vec<u32>,
    pub group: Vec<*mut EncoderData>,
    pub deps: Vec<u32>,
    pub covered: Vec<*const c_void>,
    pub transitions: Vec<(*const c_void, u32)>,
    pub clears_from: *mut EncoderData,
    pub clears: Vec<*mut ClearEncoderData>,
    pub kept: Vec<*mut ClearEncoderData>,
    pub encoder_lists: Vec<Box<EncoderData>>,
    pub icb: Vec<PooledIcb>,
    pub icb_free: FxHashMap<u64, Vec<PooledIcb>>,
    pub kept_textures: Vec<Arc<Texture>>,
    pub clear_uav: Option<ClearUav>,
    pub clear_rtv: Option<ClearRtv>,
    pub copy_temp_allocator: Option<RingBump>,
    pub copy_temp_version: u64,
}

impl ComRefCounted for Allocator {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for Allocator {
    fn drop(&mut self) {
        self.destroy_encoders();
        self.gpu_heap_buffer = None;
        if !self.cpu_heap.is_null() {
            // SAFETY: `self.cpu_heap` was allocated by `alloc_cpu_block` and is freed once here.
            unsafe { free_cpu_block(self.cpu_heap) };
            self.cpu_heap = ptr::null_mut();
        }
        if !self.gpu_heap.is_null() {
            // SAFETY: `self.gpu_heap` was allocated by `alloc_gpu_block` and is freed once here.
            unsafe { free_gpu_block(self.gpu_heap) };
            self.gpu_heap = ptr::null_mut();
        }
        if !self.device.is_null() {
            // SAFETY: `self.device` was retained with one public COM reference in
            // `create_command_allocator`.
            unsafe { com::release_com(self.device) };
            self.device = ptr::null_mut();
        }
    }
}

#[cfg(target_pointer_width = "64")]
const fn null_mem_ptr() -> MemPtr {
    MemPtr { ptr: 0 }
}

#[cfg(target_pointer_width = "32")]
const fn null_mem_ptr() -> MemPtr {
    MemPtr { ptr: 0, high: 0 }
}

#[cfg(target_pointer_width = "64")]
fn ptr_to_mem_ptr(ptr: *mut u8) -> MemPtr {
    MemPtr {
        ptr: ptr.expose_provenance() as u64,
    }
}

#[cfg(target_pointer_width = "32")]
fn ptr_to_mem_ptr(ptr: *mut u8) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(ptr.expose_provenance()).unwrap_or(0),
        high: 0,
    }
}

fn alloc_cpu_block() -> *mut u8 {
    let Ok(layout) = Layout::from_size_align(CPU_HEAP_SIZE, CPU_HEAP_ALIGN) else {
        return ptr::null_mut();
    };
    // SAFETY: `CPU_HEAP_SIZE` is non-zero and `CPU_HEAP_ALIGN` is a power of two.
    unsafe { alloc::alloc(layout) }
}

unsafe fn free_cpu_block(ptr: *mut u8) {
    let Ok(layout) = Layout::from_size_align(CPU_HEAP_SIZE, CPU_HEAP_ALIGN) else {
        return;
    };
    // SAFETY: `ptr` was allocated with `layout` by `alloc_cpu_block`.
    unsafe { alloc::dealloc(ptr, layout) };
}

fn alloc_gpu_block() -> *mut u8 {
    #[cfg(feature = "guest-vm")]
    {
        // SAFETY: `WMTMemoryAlloc` has no preconditions and returns shared guest memory or null.
        unsafe { winemetal_sys::WMTMemoryAlloc(GPU_HEAP_SIZE as u64).cast::<u8>() }
    }
    #[cfg(not(feature = "guest-vm"))]
    {
        let Ok(layout) = Layout::from_size_align(GPU_HEAP_SIZE, GPU_HEAP_ALIGN) else {
            return ptr::null_mut();
        };
        // SAFETY: `GPU_HEAP_SIZE` is non-zero and `GPU_HEAP_ALIGN` is a power of two.
        unsafe { alloc::alloc(layout) }
    }
}

unsafe fn free_gpu_block(ptr: *mut u8) {
    #[cfg(feature = "guest-vm")]
    {
        // SAFETY: `ptr` was returned by `WMTMemoryAlloc` in `alloc_gpu_block`.
        unsafe { winemetal_sys::WMTMemoryFree(ptr.cast::<c_void>()) };
    }
    #[cfg(not(feature = "guest-vm"))]
    {
        let Ok(layout) = Layout::from_size_align(GPU_HEAP_SIZE, GPU_HEAP_ALIGN) else {
            return;
        };
        // SAFETY: `ptr` was allocated with `layout` by `alloc_gpu_block`.
        unsafe { alloc::dealloc(ptr, layout) };
    }
}

#[cold]
#[inline(never)]
fn heap_overflow<T>(kind: &str, wanted: usize, limit: usize, fallback: T) -> T {
    crate::err!("D3D12 command allocator {kind} heap overflow: {wanted} >= {limit}");
    fallback
}

fn fold_clear_into_render(c: &ClearEncoderData, r: &mut RenderEncoderData, apply: bool) -> bool {
    let slot = encoder::clear_slot(c, r);
    if slot < 0 {
        return false;
    }
    if slot == CLEAR_DEPTH_STENCIL {
        if apply && (c.clear_dsv & 1) != 0 {
            r.depth.load_action = WMT_LOAD_ACTION_CLEAR;
            r.depth.clear_depth = c.depth_stencil.0;
        }
        if apply && (c.clear_dsv & 2) != 0 {
            r.stencil.load_action = WMT_LOAD_ACTION_CLEAR;
            r.stencil.clear_stencil = c.depth_stencil.1;
        }
        return true;
    }
    if apply {
        let idx = usize::try_from(slot).unwrap_or(0);
        r.colors[idx].load_action = WMT_LOAD_ACTION_CLEAR;
        r.colors[idx].clear_color = c.color;
    }
    true
}

fn obj_description(object: &Obj) -> String {
    let _pool = Pool::new();
    // SAFETY: `object` is a live Objective-C object; `NSObject_description` returns an
    // autoreleased `NSString` valid for the lifetime of `_pool`.
    let text = unsafe { NSObject_description(object.handle()) };
    if text == 0 {
        return String::new();
    }
    // SAFETY: `text` is a live `NSString` handle.
    let length = unsafe { NSString_lengthOfBytesUsingEncoding(text, WMTUTF8_STRING_ENCODING) };
    let length = usize::try_from(length).unwrap_or(0);
    let mut buffer = vec![0u8; length + 1];
    // SAFETY: `buffer` has space for `length` UTF-8 bytes plus the trailing NUL terminator.
    unsafe {
        NSString_getCString(
            text,
            buffer.as_mut_ptr().cast(),
            u64::try_from(buffer.len()).unwrap_or(0),
            WMTUTF8_STRING_ENCODING,
        );
    }
    buffer.truncate(length);
    String::from_utf8_lossy(&buffer).into_owned()
}

impl Allocator {
    fn initialize(&mut self) -> Hresult {
        if self.cpu_heap.is_null() {
            self.cpu_heap = alloc_cpu_block();
            if self.cpu_heap.is_null() {
                return E_OUTOFMEMORY;
            }
        }
        self.cpu_heap_offset = 0;

        if self.gpu_heap.is_null() {
            self.gpu_heap = alloc_gpu_block();
            if self.gpu_heap.is_null() {
                return E_OUTOFMEMORY;
            }
        }
        self.gpu_heap_offset = 0;

        if self.gpu_heap_buffer.is_none() && !self.device.is_null() {
            // SAFETY: `self.device` is a valid `Device` retained by `self`.
            let mtl = unsafe { (*self.device).mtl.as_ref() };
            if let Some(mtl) = mtl {
                let mut buffer_info = WMTBufferInfo {
                    length: GPU_HEAP_SIZE as u64,
                    options: WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
                    memory: ptr_to_mem_ptr(self.gpu_heap),
                    gpu_address: 0,
                };
                // SAFETY: `mtl` is a live `MTLDevice`, `buffer_info` references `self.gpu_heap`
                // which outlives `self.gpu_heap_buffer`.
                let handle = unsafe { MTLDevice_newBuffer(mtl.handle(), &raw mut buffer_info) };
                // SAFETY: `MTLDevice_newBuffer` returns a +1 retained handle or 0 on failure.
                self.gpu_heap_buffer = unsafe { Obj::adopt(handle) };
                if self.gpu_heap_buffer.is_none() {
                    crate::err!("CommandAllocator: failed to allocate gpu buffer");
                    return E_FAIL;
                }
                self.gpu_heap_buffer_address = buffer_info.gpu_address;
            }
        }

        self.encoder_current = ptr::null_mut();
        self.encoder_last = ptr::null_mut();
        self.encoder_count = 0;

        for pooled in self.icb.drain(..) {
            self.icb_free.entry(pooled.key).or_default().push(pooled);
        }

        if let Some(ring) = &mut self.copy_temp_allocator {
            ring.free_blocks(self.copy_temp_version);
        }
        self.copy_temp_version = self.copy_temp_version.wrapping_add(1);

        S_OK
    }

    fn reset_impl(&mut self) -> Hresult {
        if !self.encoder_last.is_null() {
            return E_FAIL;
        }
        self.destroy_encoders();
        self.kept_textures.clear();
        self.initialize()
    }

    fn destroy_encoders(&mut self) {
        for list_head in &mut self.encoder_lists {
            let mut cur = list_head.next;
            list_head.next = ptr::null_mut();
            while !cur.is_null() {
                // SAFETY: `cur` points to a live `EncoderData` node allocated in `cpu_heap` or
                // `spilled_cpu_heap`.
                let next = unsafe { (*cur).next };
                // SAFETY: `cur` is dropped in place once here before its backing CPU heap block is
                // reused or deallocated.
                unsafe { encoder::drop_encoder_in_place(cur) };
                cur = next;
            }
        }
        self.encoder_lists.clear();

        for heap in self.spilled_cpu_heap.drain(..) {
            if !heap.is_null() {
                // SAFETY: `heap` was allocated by `alloc_cpu_block`.
                unsafe { free_cpu_block(heap) };
            }
        }
    }

    fn encode_compute_cmd<T>(&mut self, cmd: T) -> *mut T {
        let ptr = self
            .allocate_cpu_heap(size_of::<T>(), align_of::<T>())
            .cast::<T>();
        // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for `T`.
        unsafe { ptr::write(ptr, cmd) };
        // SAFETY: `T` is a `WmtcmdCompute*` struct with `WmtcmdBase` at offset 0.
        let base_ptr = ptr.cast::<WmtcmdBase>();
        // SAFETY: `base_ptr` was just initialized above.
        unsafe { (*base_ptr).next = null_mem_ptr() };
        // SAFETY: `self.encoder_current` is a live `ComputeEncoderData`.
        let compute = unsafe { &mut *self.encoder_current.cast::<ComputeEncoderData>() };
        // SAFETY: `compute.cmd_tail` points to the last command in `compute`'s chain.
        unsafe { (*compute.cmd_tail).next = ptr_to_mem_ptr(base_ptr.cast::<u8>()) };
        compute.cmd_tail = base_ptr;
        ptr
    }

    fn encode_render_pre_cmd<T>(&mut self, cmd: T) -> *mut T {
        let ptr = self
            .allocate_cpu_heap(size_of::<T>(), align_of::<T>())
            .cast::<T>();
        // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for `T`.
        unsafe { ptr::write(ptr, cmd) };
        // SAFETY: `T` is a `WmtcmdCompute*` struct with `WmtcmdBase` at offset 0.
        let base_ptr = ptr.cast::<WmtcmdBase>();
        // SAFETY: `base_ptr` was just initialized above.
        unsafe { (*base_ptr).next = null_mem_ptr() };
        // SAFETY: `self.encoder_current` is a live `RenderEncoderData`.
        let render = unsafe { &mut *self.encoder_current.cast::<RenderEncoderData>() };
        // SAFETY: `render.pre_tail` points to the last command in `render`'s pre-pass chain.
        unsafe { (*render.pre_tail).next = ptr_to_mem_ptr(base_ptr.cast::<u8>()) };
        render.pre_tail = base_ptr;
        ptr
    }

    fn encode_render_cmd<T>(&mut self, cmd: T) -> *mut T {
        let ptr = self
            .allocate_cpu_heap(size_of::<T>(), align_of::<T>())
            .cast::<T>();
        // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for `T`.
        unsafe { ptr::write(ptr, cmd) };
        // SAFETY: `T` is a `WmtcmdRender*` struct with `WmtcmdBase` at offset 0.
        let base_ptr = ptr.cast::<WmtcmdBase>();
        // SAFETY: `base_ptr` was just initialized above.
        unsafe { (*base_ptr).next = null_mem_ptr() };
        // SAFETY: `self.encoder_current` is a live `RenderEncoderData`.
        let render = unsafe { &mut *self.encoder_current.cast::<RenderEncoderData>() };
        // SAFETY: `render.cmd_tail` points to the last command in `render`'s command chain.
        unsafe { (*render.cmd_tail).next = ptr_to_mem_ptr(base_ptr.cast::<u8>()) };
        render.cmd_tail = base_ptr;
        ptr
    }

    fn acquire_icb(
        &mut self,
        info: &mut WMTIndirectCommandBufferInfo,
        max_count: usize,
        storage: u64,
    ) -> (u64, u64) {
        let target = u64::try_from(max_count).unwrap_or(u64::MAX);
        let mut capacity = 1u64;
        while capacity < target {
            capacity <<= 1;
        }
        let key =
            capacity | (u64::from(info.r#type) << 40) | (u64::from(info.inherit_buffers()) << 48);
        if let Some(free) = self.icb_free.get_mut(&key)
            && let Some(pooled) = free.pop()
        {
            let handle = pooled.icb.as_ref().map_or(0, Obj::handle);
            let res_id = pooled.resource_id;
            self.icb.push(pooled);
            return (handle, res_id);
        }

        let dev_handle = if self.device.is_null() {
            0
        } else {
            // SAFETY: `self.device` is retained by `self`.
            unsafe { (*self.device).mtl.as_ref().map_or(0, Obj::handle) }
        };
        let raw_icb = if dev_handle != 0 {
            // SAFETY: `dev_handle` is a live `MTLDevice` and `info` is a valid descriptor struct.
            unsafe { MTLDevice_newIndirectCommandBuffer(dev_handle, info, capacity, storage) }
        } else {
            0
        };
        // SAFETY: `MTLDevice_newIndirectCommandBuffer` returns a +1 retained handle or 0.
        let icb_obj = unsafe { Obj::adopt(raw_icb) };
        crate::count!("#indirect command buffers created", 1);
        let handle = icb_obj.as_ref().map_or(0, Obj::handle);
        let res_id = info.gpu_resource_id;
        self.icb.push(PooledIcb {
            key,
            icb: icb_obj,
            resource_id: res_id,
        });
        (handle, res_id)
    }

    /// Begins recording a command list on this allocator (`StartRecord`,
    ///
    /// `d3d12_command_allocator.hpp:249`).
    ///
    /// # Errors
    ///
    /// Returns `E_INVALIDARG` if this allocator already has a command list recording on it.
    pub fn start_record(&mut self) -> Result<*mut EncoderData, Hresult> {
        if !self.encoder_last.is_null() {
            return Err(E_INVALIDARG);
        }
        self.barriers = 0;
        self.joins.clear();
        self.joins.push(0);
        self.group.clear();
        self.transitions.clear();
        self.clears_from = ptr::null_mut();

        let mut head = Box::new(EncoderData::new(EncoderType::Null, 0));
        let head_ptr: *mut EncoderData = &raw mut *head;
        self.encoder_lists.push(head);
        self.encoder_last = head_ptr;
        Ok(head_ptr)
    }

    /// Ends recording the active command list on this allocator (`EndRecord`,
    ///
    /// `d3d12_command_allocator.hpp:261`).
    ///
    /// # Errors
    ///
    /// Returns `E_FAIL` if this allocator does not currently have a recording command list.
    pub fn end_record(&mut self) -> Result<usize, Hresult> {
        if self.encoder_last.is_null() {
            return Err(E_FAIL);
        }
        if !self.encoder_current.is_null() {
            self.invalidate_current_pass();
        }
        self.encoder_last = ptr::null_mut();
        let count = self.encoder_count;
        self.encoder_count = 0;
        Ok(count)
    }

    /// Closes and links `encoder_current` into the recorded encoder list (`InvalidateCurrentPass`,
    ///
    /// `d3d12_command_allocator.hpp:163`).
    pub fn invalidate_current_pass(&mut self) {
        if self.encoder_current.is_null() {
            return;
        }
        let current = self.encoder_current;
        self.decide(current);
        // SAFETY: `current` is a valid `EncoderData` node on this allocator.
        let cur_kind = unsafe { (*current).kind };
        // SAFETY: `self.encoder_last` is a valid `EncoderData` node on this allocator.
        let last_kind = unsafe { (*self.encoder_last).kind };
        if cur_kind == EncoderType::Clear && last_kind != EncoderType::Clear {
            self.clears_from = self.encoder_last;
        }
        // SAFETY: `self.encoder_last` is a valid `EncoderData` node.
        unsafe { (*self.encoder_last).next = current };
        self.encoder_last = current;
        self.encoder_current = ptr::null_mut();
        self.encoder_count += 1;
    }

    /// Folds trailing clear passes into render pass `r`'s load actions (`FoldClears`,
    ///
    /// `d3d12_command_allocator.hpp:181`).
    pub fn fold_clears(&mut self, r: &mut RenderEncoderData) -> u32 {
        if !*MERGE_ENABLED || self.encoder_last.is_null() {
            return 0;
        }
        // SAFETY: `self.encoder_last` is non-null and valid while recording.
        let last_kind = unsafe { (*self.encoder_last).kind };
        if last_kind != EncoderType::Clear || self.clears_from.is_null() {
            return 0;
        }

        let mut before = self.clears_from;
        loop {
            // SAFETY: `before` is a valid node in the active encoder list.
            let next = unsafe { (*before).next };
            if next.is_null() {
                return 0;
            }
            // SAFETY: `next` is non-null and valid in the active encoder list.
            let next_barriers = unsafe { (*next).barriers };
            if next_barriers == r.base.barriers {
                break;
            }
            before = next;
        }

        self.clears.clear();
        // SAFETY: `before` is a valid node in the active encoder list.
        let mut cur = unsafe { (*before).next };
        while !cur.is_null() {
            self.clears.push(cur.cast::<ClearEncoderData>());
            // SAFETY: `cur` is a valid `ClearEncoderData` node in the trailing clear run.
            cur = unsafe { (*cur).next };
        }

        self.kept.clear();
        for i in (0..self.clears.len()).rev() {
            let c_ptr = self.clears[i];
            // SAFETY: `c_ptr` points to a valid `ClearEncoderData` in the CPU heap.
            let c = unsafe { &*c_ptr };
            let target_res = c.base.writes[0];
            let blocked = self.kept.iter().any(|&k_ptr| {
                // SAFETY: `k_ptr` points to a valid `ClearEncoderData` in `self.kept`.
                let k = unsafe { &*k_ptr };
                k.base.writes[0] == target_res
            });
            if !fold_clear_into_render(c, r, false) || blocked {
                self.kept.push(c_ptr);
                self.clears[i] = ptr::null_mut();
            }
        }
        if self.kept.len() == self.clears.len() {
            return 0;
        }

        // SAFETY: `before` is a valid `EncoderData` node in the active encoder list.
        unsafe { (*before).next = ptr::null_mut() };
        self.encoder_last = before;
        self.encoder_count -= self.clears.len();
        self.group.truncate(self.group.len() - self.clears.len());

        let mut folded = 0u32;
        for i in 0..self.clears.len() {
            let c_ptr = self.clears[i];
            if !c_ptr.is_null() {
                // SAFETY: `c_ptr` is a valid `ClearEncoderData` selected to fold.
                let c = unsafe { &*c_ptr };
                fold_clear_into_render(c, r, true);
                // SAFETY: `c_ptr` is unlinked from the encoder list and dropped once here.
                unsafe { encoder::drop_encoder_in_place(c_ptr.cast::<EncoderData>()) };
                folded += 1;
            }
        }

        let kept_len = self.kept.len();
        for idx in (0..kept_len).rev() {
            let c_ptr = self.kept[idx];
            // SAFETY: `c_ptr` is a valid kept `ClearEncoderData` being re-linked.
            let c = unsafe { &mut *c_ptr };
            c.base.next = ptr::null_mut();
            c.base.deps = ptr::null();
            c.base.dep_count = 0;
            self.encoder_current = c_ptr.cast::<EncoderData>();
            self.invalidate_current_pass();
        }
        self.encoder_current = (&raw mut *r).cast::<EncoderData>();
        folded
    }

    /// Allocates `length` bytes with `alignment` from the CPU command arena (`AllocateCPUHeap`,
    ///
    /// `d3d12_command_allocator.hpp:273`).
    pub fn allocate_cpu_heap(&mut self, length: usize, alignment: usize) -> *mut u8 {
        if length >= CPU_HEAP_SIZE {
            return heap_overflow("cpu", length, CPU_HEAP_SIZE, ptr::null_mut());
        }
        if self.cpu_heap.is_null() {
            self.cpu_heap = alloc_cpu_block();
            self.cpu_heap_offset = 0;
            if self.cpu_heap.is_null() {
                return ptr::null_mut();
            }
        }
        let align = alignment.max(1);
        loop {
            let aligned = self.cpu_heap_offset.next_multiple_of(align);
            let end = aligned.saturating_add(length);
            self.cpu_heap_offset = end;
            if end >= CPU_HEAP_SIZE {
                if aligned == 0 {
                    return heap_overflow("cpu", end, CPU_HEAP_SIZE, ptr::null_mut());
                }
                let next_block = alloc_cpu_block();
                if next_block.is_null() {
                    return ptr::null_mut();
                }
                self.spilled_cpu_heap.push(self.cpu_heap);
                self.cpu_heap = next_block;
                self.cpu_heap_offset = 0;
                continue;
            }
            // SAFETY: `aligned + length < CPU_HEAP_SIZE`, within the live `self.cpu_heap` block.
            return unsafe { self.cpu_heap.add(aligned) };
        }
    }

    /// Allocates `length` bytes with `alignment` from the GPU argument/constant arena
    ///
    /// (`AllocateGPUHeap`, `d3d12_command_allocator.hpp:461`).
    pub fn allocate_gpu_heap(&mut self, length: usize, alignment: usize) -> (*mut u8, usize) {
        if length == 0 {
            return (ptr::null_mut(), 0);
        }
        if self.gpu_heap.is_null() {
            self.gpu_heap = alloc_gpu_block();
            self.gpu_heap_offset = 0;
            if self.gpu_heap.is_null() {
                return (ptr::null_mut(), 0);
            }
        }
        let align = alignment.max(1);
        let aligned = self.gpu_heap_offset.next_multiple_of(align);
        let end = aligned.saturating_add(length);
        if end >= GPU_HEAP_SIZE {
            return heap_overflow("gpu", end, GPU_HEAP_SIZE, (self.gpu_heap, 0));
        }
        self.gpu_heap_offset = end;
        // SAFETY: `aligned + length < GPU_HEAP_SIZE`, within the live `self.gpu_heap` block.
        let ptr = unsafe { self.gpu_heap.add(aligned) };
        (ptr, aligned)
    }

    /// Allocates a temporary staging buffer slice (`AllocateTempBuffer`,
    ///
    /// `d3d12_command_allocator.hpp:472`).
    pub fn allocate_temp_buffer(&mut self, size: usize, alignment: usize) -> (u64, u64) {
        let Some(ring) = &mut self.copy_temp_allocator else {
            return (0, 0);
        };
        let (block, offset) = ring.allocate(
            self.copy_temp_version,
            self.copy_temp_version.saturating_sub(1),
            size as u64,
            alignment as u64,
        );
        (block.buffer, offset)
    }

    /// Number of barrier calls recorded on the current command list (`BarrierCalls`,
    ///
    /// `d3d12_command_allocator.hpp:299`).
    #[must_use]
    pub const fn barrier_calls(&self) -> u32 {
        self.barriers
    }

    /// Records a barrier call (`Barrier`, `d3d12_command_allocator.hpp:306`).
    pub fn barrier(&mut self, join: bool, transitioned: &[*const c_void]) {
        self.barriers += 1;
        let last_join = self.joins.last().copied().unwrap_or(0);
        self.joins.push(last_join + u32::from(join));
        if !join {
            let at = self.barriers;
            for &res in transitioned {
                self.transitions.push((res, at));
            }
        }
    }

    /// Marks `resource` as written by the current encoder (`Writes`,
    ///
    /// `d3d12_command_allocator.hpp:334`).
    pub fn writes(&mut self, resource: *const c_void) {
        if self.encoder_current.is_null() {
            return;
        }
        // SAFETY: `self.encoder_current` is a live `EncoderData` node.
        let enc = unsafe { &mut *self.encoder_current };
        let count = usize::from(enc.write_count);
        if resource.is_null() || count == enc.writes.len() {
            enc.writes_unknown = true;
            return;
        }
        if enc.writes[..count].contains(&resource) {
            return;
        }
        enc.writes[count] = resource;
        enc.write_count += 1;
    }

    /// Retains `texture` until the allocator is reset (`KeepTexture`,
    ///
    /// `d3d12_command_allocator.hpp:347`).
    pub fn keep_texture(&mut self, texture: Arc<Texture>) {
        self.kept_textures.push(texture);
    }

    /// Computes dependency edges for the closing encoder `e` (`Decide`,
    ///
    /// `d3d12_command_allocator.hpp:358`).
    pub fn decide(&mut self, e: *mut EncoderData) {
        self.decide_inner(e);
    }

    fn decide_inner(&mut self, e: *mut EncoderData) {
        if e.is_null() || self.encoder_last.is_null() {
            return;
        }
        // SAFETY: `self.encoder_last` is a valid `EncoderData` node on this allocator.
        let prev = unsafe { &*self.encoder_last };
        let (prev_kind, prev_barriers) = (prev.kind, prev.barriers);
        // SAFETY: `e` is a valid `EncoderData` node.
        let enc = unsafe { &mut *e };
        enc.position = u32::try_from(self.encoder_count).unwrap_or(u32::MAX);

        let last_idx = usize::try_from(enc.barriers_last).unwrap_or(0);
        let prev_idx = usize::try_from(prev_barriers).unwrap_or(0);
        let joins_after = self.joins.get(last_idx).copied().unwrap_or(0)
            > self.joins.get(prev_idx).copied().unwrap_or(0);
        enc.join = prev_kind == EncoderType::Null || enc.writes_unknown || joins_after;

        if enc.join {
            self.group.clear();
            let e_barriers = enc.barriers;
            self.transitions.retain(|&(_, at)| at > e_barriers);
        } else {
            self.deps.clear();
            self.covered.clear();
            let e_write_count = usize::from(enc.write_count);
            let e_writes = enc.writes;
            let e_barriers_last = enc.barriers_last;

            for &g_ptr in self.group.iter().rev() {
                // SAFETY: `g_ptr` is a live `EncoderData` node recorded earlier in this group.
                let g = unsafe { &*g_ptr };
                let g_count = usize::from(g.write_count);
                let mut needed = g.writes_unknown;
                for &r in &g.writes[..g_count] {
                    if needed {
                        break;
                    }
                    let not_covered = !self.covered.contains(&r);
                    let e_lists = e_writes[..e_write_count].contains(&r);
                    let transitioned = self
                        .transitions
                        .iter()
                        .any(|&(tr, at)| tr == r && at > g.barriers && at <= e_barriers_last);
                    needed = not_covered && (e_lists || transitioned);
                }
                if !needed {
                    continue;
                }
                self.deps.push(g.position);
                self.covered.extend_from_slice(&g.writes[..g_count]);
            }

            if !self.deps.is_empty() {
                let dep_len = self.deps.len();
                let dst = self
                    .allocate_cpu_heap(dep_len * size_of::<u32>(), align_of::<u32>())
                    .cast::<c_void>()
                    .cast::<u32>();
                if !dst.is_null() {
                    // SAFETY: `self.deps` has `dep_len` initialized `u32` elements and `dst` points
                    // to `dep_len` aligned `u32` slots in the CPU heap.
                    unsafe { ptr::copy_nonoverlapping(self.deps.as_ptr(), dst, dep_len) };
                    // SAFETY: `e` is a valid `EncoderData` node.
                    let enc = unsafe { &mut *e };
                    enc.deps = dst;
                    enc.dep_count = u32::try_from(dep_len).unwrap_or(u32::MAX);
                }
            }
        }

        self.group.push(e);
    }

    /// Allocates and initializes a compute indirect command record (`EncodeIndirectComputeCommand`,
    ///
    /// `d3d12_command_allocator.cpp:186`).
    pub fn encode_indirect_compute_command(
        &mut self,
        cmd_sig: &CommandSignature,
        pso: &Compute,
        max_count: usize,
    ) -> *mut IndirectComputeCommandData {
        let bits = (u32::from(!cmd_sig.update_root_arguments)
            << WMTIndirectCommandBufferInfo::INHERIT_BUFFERS_SHIFT)
            | (1 << WMTIndirectCommandBufferInfo::INHERIT_PSO_SHIFT);
        let mut info = WMTIndirectCommandBufferInfo {
            r#type: WMT_INDIRECT_COMMAND_TYPE_CONCURRENT_DISPATCH,
            bits,
            max_vertex_buffer_binding: 0,
            max_fragment_buffer_binding: 0,
            max_kernel_buffer_binding: 31,
            max_mesh_buffer_binding: 0,
            max_object_buffer_binding: 0,
            max_object_threadgroup_memory_binding: 0,
            max_kernel_threadgroup_memory_binding: 0,
            padding: 0,
            gpu_resource_id: 0,
        };

        let (icb_handle, icb_res_id) =
            self.acquire_icb(&mut info, max_count, WMT_RESOURCE_STORAGE_MODE_SHARED);

        let (ptr, offset) = self.allocate_gpu_heap(size_of::<IndirectComputeCommandData>(), 16);
        let data_ptr = ptr.cast::<c_void>().cast::<IndirectComputeCommandData>();
        if !data_ptr.is_null() {
            let init = IndirectComputeCommandData {
                cmd_buf: icb_res_id,
                max_count: max_count as u64,
                max_count_buffer: 0,
                argument_buffer: 0,
                static_samplers: 0,
                rootsig_qwords: 0,
                rootsig_qwords_stride: 0,
                tgsize_x: u32::try_from(pso.threadgroup_size.width).unwrap_or(1),
                tgsize_y: u32::try_from(pso.threadgroup_size.height).unwrap_or(1),
                tgsize_z: u32::try_from(pso.threadgroup_size.depth).unwrap_or(1),
            };
            // SAFETY: `data_ptr` is aligned GPU heap storage of `size_of::<IndirectComputeCommandData>()`.
            unsafe { ptr::write(data_ptr, init) };
        }

        let gpu_buf_handle = self.gpu_heap_buffer.as_ref().map_or(0, Obj::handle);
        self.encode_compute_cmd(WmtcmdComputeUseresource {
            r#type: WMT_COMPUTE_COMMAND_USE_RESOURCE,
            reserved: [0; 3],
            next: null_mem_ptr(),
            resource: icb_handle,
            usage: WMT_RESOURCE_USAGE_READ | WMT_RESOURCE_USAGE_WRITE,
        });
        self.encode_compute_cmd(WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: null_mem_ptr(),
            pso: cmd_sig.compute_resolver.as_ref().map_or(0, Obj::handle),
            threadgroup_size: WMTSize {
                width: 1,
                height: 1,
                depth: 1,
            },
        });
        self.encode_compute_cmd(WmtcmdComputeSetbuffer {
            r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
            reserved: [0; 3],
            next: null_mem_ptr(),
            buffer: gpu_buf_handle,
            offset: offset as u64,
            index: 30,
        });
        self.encode_compute_cmd(WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH,
            reserved: [0; 3],
            next: null_mem_ptr(),
            size: WMTSize {
                width: 1,
                height: 1,
                depth: 1,
            },
        });
        self.encode_compute_cmd(WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: null_mem_ptr(),
            pso: pso.pso.as_ref().map_or(0, Obj::handle),
            threadgroup_size: pso.threadgroup_size,
        });
        self.encode_compute_cmd(WmtcmdComputeExecutecommands {
            r#type: WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER,
            reserved: [0; 3],
            next: null_mem_ptr(),
            indirect_command_buffer: icb_handle,
            location: 0,
            length: u32::try_from(max_count).unwrap_or(u32::MAX),
        });

        data_ptr
    }

    /// Allocates and initializes a render indirect command record (`EncodeIndirectRenderCommand`,
    ///
    /// `d3d12_command_allocator.cpp:273`).
    pub fn encode_indirect_render_command(
        &mut self,
        cmd_sig: &CommandSignature,
        _pso: &Graphics,
        max_count: usize,
    ) -> *mut IndirectRenderCommandData {
        let inherit_bufs = !(cmd_sig.update_vertex_buffers
            || cmd_sig.update_index_buffer
            || cmd_sig.update_root_arguments);
        let cmd_type = if cmd_sig.command_type == D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED {
            WMT_INDIRECT_COMMAND_TYPE_DRAW_INDEXED
        } else {
            WMT_INDIRECT_COMMAND_TYPE_DRAW
        };
        let bits = (u32::from(inherit_bufs) << WMTIndirectCommandBufferInfo::INHERIT_BUFFERS_SHIFT)
            | (1 << WMTIndirectCommandBufferInfo::INHERIT_PSO_SHIFT)
            | (1 << WMTIndirectCommandBufferInfo::INHERIT_CULL_MODE_SHIFT)
            | (1 << WMTIndirectCommandBufferInfo::INHERIT_DEPTH_BIAS_SHIFT)
            | (1 << WMTIndirectCommandBufferInfo::INHERIT_DEPTH_CLIP_MODE_SHIFT)
            | (1 << WMTIndirectCommandBufferInfo::INHERIT_DEPTH_STENCIL_STATE_SHIFT)
            | (1 << WMTIndirectCommandBufferInfo::INHERIT_FRONT_FACING_SHIFT)
            | (1 << WMTIndirectCommandBufferInfo::INHERIT_FILL_MODE_SHIFT);
        let mut info = WMTIndirectCommandBufferInfo {
            r#type: cmd_type,
            bits,
            max_vertex_buffer_binding: 31,
            max_fragment_buffer_binding: 31,
            max_kernel_buffer_binding: 0,
            max_mesh_buffer_binding: 0,
            max_object_buffer_binding: 0,
            max_object_threadgroup_memory_binding: 0,
            max_kernel_threadgroup_memory_binding: 0,
            padding: 0,
            gpu_resource_id: 0,
        };

        let (icb_handle, icb_res_id) =
            self.acquire_icb(&mut info, max_count, WMT_RESOURCE_STORAGE_MODE_PRIVATE);

        let (ptr, offset) = self.allocate_gpu_heap(size_of::<IndirectRenderCommandData>(), 16);
        let data_ptr = ptr.cast::<c_void>().cast::<IndirectRenderCommandData>();
        if !data_ptr.is_null() {
            let init = IndirectRenderCommandData {
                cmd_buf: icb_res_id,
                max_count: max_count as u64,
                max_count_buffer: 0,
                argument_buffer: 0,
                static_samplers: 0,
                rootsig_qwords: 0,
                rootsig_qwords_stride: 0,
                primitive_type: 0,
                vertex_buffer: 0,
                index_buffer: 0,
                index_buffer_format: 0,
                vertex_argbuf_stride: 0,
                vertex_slot_mask: 0,
                null_vertex_buffer: 0,
            };
            // SAFETY: `data_ptr` is aligned GPU heap storage of `size_of::<IndirectRenderCommandData>()`.
            unsafe { ptr::write(data_ptr, init) };
        }

        let gpu_buf_handle = self.gpu_heap_buffer.as_ref().map_or(0, Obj::handle);
        self.encode_render_pre_cmd(WmtcmdComputeUseresource {
            r#type: WMT_COMPUTE_COMMAND_USE_RESOURCE,
            reserved: [0; 3],
            next: null_mem_ptr(),
            resource: icb_handle,
            usage: WMT_RESOURCE_USAGE_WRITE,
        });
        self.encode_render_pre_cmd(WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: null_mem_ptr(),
            pso: cmd_sig.render_resolver.as_ref().map_or(0, Obj::handle),
            threadgroup_size: WMTSize {
                width: 1,
                height: 1,
                depth: 1,
            },
        });
        self.encode_render_pre_cmd(WmtcmdComputeSetbuffer {
            r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
            reserved: [0; 3],
            next: null_mem_ptr(),
            buffer: gpu_buf_handle,
            offset: offset as u64,
            index: 30,
        });
        self.encode_render_pre_cmd(WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH,
            reserved: [0; 3],
            next: null_mem_ptr(),
            size: WMTSize {
                width: 1,
                height: 1,
                depth: 1,
            },
        });

        self.encode_render_cmd(WmtcmdRenderUseresource {
            r#type: WMT_RENDER_COMMAND_USE_RESOURCE,
            reserved: [0; 3],
            next: null_mem_ptr(),
            resource: icb_handle,
            usage: WMT_RESOURCE_USAGE_READ,
            stages: WMT_RENDER_STAGE_VERTEX,
        });
        self.encode_render_cmd(WmtcmdRenderExecutecommands {
            r#type: WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER,
            reserved: [0; 3],
            next: null_mem_ptr(),
            indirect_command_buffer: icb_handle,
            location: 0,
            length: u32::try_from(max_count).unwrap_or(u32::MAX),
        });

        data_ptr
    }
}

impl SimpleCommandContext for Allocator {
    fn compute_pipeline(&mut self, name: &CStr) -> Option<Obj> {
        if self.device.is_null() {
            return None;
        }
        // SAFETY: `self.device` is retained by `self`.
        let dev = unsafe { &*self.device };
        let mtl = dev.mtl.as_ref()?;
        let func = dev.library.as_ref()?.function(name)?;
        let info = WMTComputePipelineInfo {
            compute_function: func.handle(),
            binary_archives_for_lookup: null_mem_ptr(),
            binary_archive_for_serialization: 0,
            num_binary_archives_for_lookup: 0,
            fail_on_binary_archive_miss: 0,
            support_indirect_command_buffers: 0,
            tgsize_is_multiple_of_sgwidth: 0,
            immutable_buffers: 0,
        };
        let mut err = 0u64;
        // SAFETY: `mtl` and `func` are live Metal objects; `info` and `err` are valid stack
        // pointers.
        let pso = unsafe {
            MTLDevice_newComputePipelineState(mtl.handle(), &raw const info, &raw mut err)
        };
        // SAFETY: `MTLDevice_newComputePipelineState` returns a +1 retained error or 0.
        if let Some(err_obj) = unsafe { Obj::adopt(err) } {
            crate::err!(
                "Failed to create compute PSO: {}",
                obj_description(&err_obj)
            );
        }
        // SAFETY: `MTLDevice_newComputePipelineState` returns a +1 retained PSO or 0.
        unsafe { Obj::adopt(pso) }
    }

    fn start_compute_pass(&mut self) {
        self.invalidate_current_pass();
        let barriers = self.barriers;
        let ptr = self
            .allocate_cpu_heap(
                size_of::<ComputeEncoderData>(),
                align_of::<ComputeEncoderData>(),
            )
            .cast::<c_void>()
            .cast::<ComputeEncoderData>();
        let mut base = EncoderData::new(EncoderType::Compute, 0);
        base.barriers = barriers;
        base.barriers_last = barriers;
        let init = ComputeEncoderData {
            base,
            cmd_head: winemetal_wire::WmtcmdComputeNop {
                r#type: WMT_COMPUTE_COMMAND_NOP,
                reserved: [0; 3],
                next: null_mem_ptr(),
            },
            cmd_tail: ptr::null_mut(),
        };
        // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for
        // `ComputeEncoderData`.
        unsafe { ptr::write(ptr, init) };
        // SAFETY: `ptr` was just initialized above.
        let enc = unsafe { &mut *ptr };
        enc.cmd_tail = (&raw mut enc.cmd_head).cast::<WmtcmdBase>();
        self.encoder_current = ptr.cast::<EncoderData>();
    }

    fn end_pass(&mut self) {
        self.invalidate_current_pass();
    }

    fn set_compute_pso(&mut self, pso: &Obj, threadgroup_size: WMTSize) {
        self.encode_compute_cmd(WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: null_mem_ptr(),
            pso: pso.handle(),
            threadgroup_size,
        });
    }

    fn dispatch(&mut self, size: WMTSize) {
        self.encode_compute_cmd(WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
            reserved: [0; 3],
            next: null_mem_ptr(),
            size,
        });
    }

    fn set_compute_texture(
        &mut self,
        index: u32,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        _access: ResourceAccess,
    ) {
        let handle = texture
            .current()
            .map_or(0, |alloc| alloc.view(view).texture);
        self.encode_compute_cmd(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: null_mem_ptr(),
            texture: handle,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn set_compute_texel_buffer(
        &mut self,
        index: u32,
        buffer: &Arc<Buffer>,
        view: BufferViewKey,
        _access: ResourceAccess,
    ) {
        let handle = buffer
            .current()
            .map_or(0, |alloc| buffer.view(&alloc, view).texture);
        self.encode_compute_cmd(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: null_mem_ptr(),
            texture: handle,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn set_compute_buffer(
        &mut self,
        index: u32,
        buffer: &Arc<Buffer>,
        _offset: u32,
        _length: u32,
        _access: ResourceAccess,
    ) {
        let handle = buffer.current().map_or(0, |alloc| alloc.buffer());
        self.encode_compute_cmd(WmtcmdComputeSetbuffer {
            r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
            reserved: [0; 3],
            next: null_mem_ptr(),
            buffer: handle,
            offset: 0,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn set_compute_bytes(&mut self, index: u32, bytes: &[u8]) {
        let temp = self.allocate_cpu_heap(bytes.len(), 16);
        if !temp.is_null() && !bytes.is_empty() {
            // SAFETY: `temp` has `bytes.len()` writable bytes in the CPU heap.
            unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), temp, bytes.len()) };
        }
        self.encode_compute_cmd(WmtcmdComputeSetbytes {
            r#type: WMT_COMPUTE_COMMAND_SET_BYTES,
            reserved: [0; 3],
            next: null_mem_ptr(),
            bytes: ptr_to_mem_ptr(temp),
            length: bytes.len() as u64,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn default_library(&self) -> &Library {
        // SAFETY: `default_library` is only called when `self.device` is non-null and has an
        // initialized `Library`.
        let dev = unsafe { &*self.device };
        dev.library
            .as_ref()
            .unwrap_or_else(|| unreachable!("Device has an internal Library"))
    }

    fn start_render_pass(&mut self) {
        self.invalidate_current_pass();
        let barriers = self.barriers;
        let ptr = self
            .allocate_cpu_heap(
                size_of::<RenderEncoderData>(),
                align_of::<RenderEncoderData>(),
            )
            .cast::<c_void>()
            .cast::<RenderEncoderData>();
        let mut base = EncoderData::new(EncoderType::Render, 0);
        base.barriers = barriers;
        base.barriers_last = barriers;
        let init = RenderEncoderData {
            base,
            colors: Default::default(),
            depth: crate::encoder::RenderEncoderDepthAttachmentData::default(),
            stencil: crate::encoder::RenderEncoderStencilAttachmentData::default(),
            default_raster_sample_count: 1,
            render_target_array_length: 0,
            render_target_height: 16384,
            render_target_width: 16384,
            cmd_head: winemetal_wire::WmtcmdRenderNop {
                r#type: WMT_RENDER_COMMAND_NOP,
                reserved: [0; 3],
                next: null_mem_ptr(),
            },
            cmd_tail: ptr::null_mut(),
            dsv_planar_flags: 0,
            dsv_readonly_flags: 0,
            render_target_count: 0,
            use_visibility_result: false,
            visibility_buffer: 0,
            use_tessellation: false,
            use_geometry: false,
            uav_writes: false,
            pre_head: winemetal_wire::WmtcmdComputeNop {
                r#type: WMT_COMPUTE_COMMAND_NOP,
                reserved: [0; 3],
                next: null_mem_ptr(),
            },
            pre_tail: ptr::null_mut(),
        };
        // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for
        // `RenderEncoderData`.
        unsafe { ptr::write(ptr, init) };
        // SAFETY: `ptr` was just initialized above.
        let enc = unsafe { &mut *ptr };
        enc.cmd_tail = (&raw mut enc.cmd_head).cast::<WmtcmdBase>();
        enc.pre_tail = (&raw mut enc.pre_head).cast::<WmtcmdBase>();
        self.encoder_current = ptr.cast::<EncoderData>();
    }

    fn set_color_attachment(
        &mut self,
        index: u32,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        depth_plane: u32,
    ) {
        let idx = usize::try_from(index).unwrap_or(0);
        // SAFETY: `self.encoder_current` is a `RenderEncoderData` initialized by `start_render_pass`.
        let enc = unsafe { &mut *self.encoder_current.cast::<RenderEncoderData>() };
        enc.colors[idx].attachment = TextureViewRef::from_texture(texture, view);
        enc.colors[idx].depth_plane = depth_plane;
        enc.colors[idx].load_action = WMT_LOAD_ACTION_LOAD;
        enc.colors[idx].store_action = WMT_STORE_ACTION_STORE;
        let next_count = u8::try_from(index + 1).unwrap_or(u8::MAX);
        enc.render_target_count = enc.render_target_count.max(next_count);
        let [width, height, depth] = texture.view_size(view);
        enc.render_target_width = enc.render_target_width.min(width);
        enc.render_target_height = enc.render_target_height.min(height);
        let array_len = if texture.info().r#type() == WMT_TEXTURE_TYPE3D {
            depth.saturating_sub(depth_plane)
        } else {
            view.array_size()
        };
        enc.render_target_array_length = u16::try_from(array_len).unwrap_or(u16::MAX);
        enc.default_raster_sample_count = u8::try_from(texture.info().sample_count()).unwrap_or(1);
    }

    fn set_depth_stencil_attachment(
        &mut self,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        dsv_flag: u32,
    ) {
        // SAFETY: `self.encoder_current` is a `RenderEncoderData` initialized by `start_render_pass`.
        let enc = unsafe { &mut *self.encoder_current.cast::<RenderEncoderData>() };
        let view_ref = TextureViewRef::from_texture(texture, view);
        if (dsv_flag & 1) != 0 {
            enc.depth.attachment.clone_from(&view_ref);
            enc.depth.load_action = WMT_LOAD_ACTION_LOAD;
            enc.depth.store_action = WMT_STORE_ACTION_STORE;
        }
        if (dsv_flag & 2) != 0 {
            enc.stencil.attachment = view_ref;
            enc.stencil.load_action = WMT_LOAD_ACTION_LOAD;
            enc.stencil.store_action = WMT_STORE_ACTION_STORE;
        }
        enc.dsv_planar_flags = u8::try_from(dsv_flag).unwrap_or(0);
        let [width, height, _] = texture.view_size(view);
        enc.render_target_width = enc.render_target_width.min(width);
        enc.render_target_height = enc.render_target_height.min(height);
        enc.render_target_array_length = u16::try_from(view.array_size()).unwrap_or(u16::MAX);
        enc.default_raster_sample_count = u8::try_from(texture.info().sample_count()).unwrap_or(1);
    }

    fn set_render_pso(&mut self, pso: &Obj) {
        self.encode_render_cmd(WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: null_mem_ptr(),
            pso: pso.handle(),
        });
    }

    fn set_viewport(&mut self, viewport: WMTViewport) {
        self.encode_render_cmd(WmtcmdRenderSetviewport {
            r#type: WMT_RENDER_COMMAND_SET_VIEWPORT,
            reserved: [0; 3],
            next: null_mem_ptr(),
            viewport,
        });
    }

    fn set_depth_stencil_state(&mut self, state: &Obj) {
        self.encode_render_cmd(WmtcmdRenderSetdepthstencilstate {
            r#type: WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
            reserved: [0; 3],
            next: null_mem_ptr(),
            depth_stencil_state: state.handle(),
        });
    }

    fn set_scissor_rect(&mut self, rect: WMTScissorRect) {
        self.encode_render_cmd(WmtcmdRenderSetscissorrect {
            r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
            reserved: [0; 3],
            next: null_mem_ptr(),
            scissor_rect: rect,
        });
    }

    fn set_stencil_reference(&mut self, stencil_ref: u8) {
        self.encode_render_cmd(WmtcmdRenderSetstencilref {
            r#type: WMT_RENDER_COMMAND_SET_STENCIL_REF,
            reserved: [0; 3],
            next: null_mem_ptr(),
            stencil_ref,
        });
    }

    fn set_fragment_bytes(&mut self, index: u32, bytes: &[u8]) {
        let temp = self.allocate_cpu_heap(bytes.len(), 16);
        if !temp.is_null() && !bytes.is_empty() {
            // SAFETY: `temp` has `bytes.len()` writable bytes in the CPU heap.
            unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), temp, bytes.len()) };
        }
        self.encode_render_cmd(WmtcmdRenderSetbytes {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
            reserved: [0; 3],
            next: null_mem_ptr(),
            bytes: ptr_to_mem_ptr(temp),
            length: bytes.len() as u64,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn draw(
        &mut self,
        primitive: u8,
        vertex_start: u32,
        vertex_count: u32,
        base_instance: i32,
        instance_count: u32,
    ) {
        self.encode_render_cmd(WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: null_mem_ptr(),
            primitive_type: primitive,
            vertex_start: u64::from(vertex_start),
            vertex_count: u64::from(vertex_count),
            instance_count,
            base_instance: base_instance.cast_unsigned(),
        });
    }
}

unsafe extern "system-unwind" fn allocator_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> i32 {
    crate::scope!("allocator.QueryInterface");
    if ppv_object.is_null() {
        return E_POINTER;
    }
    // SAFETY: `ppv_object` was checked non-null above.
    unsafe { com::init_out_ptr(ppv_object) };
    // SAFETY: `riid` is null or points to a caller-supplied 16-byte GUID.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_NOINTERFACE;
    };
    if iid == IID_IUNKNOWN
        || iid == IID_ID3D12_OBJECT
        || iid == IID_ID3D12_DEVICE_CHILD
        || iid == IID_ID3D12_PAGEABLE
        || iid == IID_ID3D12_COMMAND_ALLOCATOR
    {
        // SAFETY: `this` is a valid `Allocator` COM object.
        unsafe { com::add_ref_com::<Allocator>(this.cast()) };
        // SAFETY: `ppv_object` is non-null.
        unsafe { *ppv_object = this };
        return S_OK;
    }
    crate::warn!("ID3D12CommandAllocator::QueryInterface: Unknown interface query {iid}");
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn allocator_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `Allocator` COM object.
    unsafe { com::add_ref_com::<Allocator>(this.cast()) }
}

unsafe extern "system-unwind" fn allocator_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `Allocator` COM object.
    unsafe { com::release_com::<Allocator>(this.cast()) }
}

unsafe extern "system-unwind" fn allocator_get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    data_size: *mut u32,
    data: *mut c_void,
) -> i32 {
    // SAFETY: `this` is a valid `Allocator` COM object.
    let alloc = unsafe { &*this.cast::<Allocator>() };
    // SAFETY: `guid` is null or 16 readable bytes.
    let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: forwarding caller pointers to `PrivateData::get_data`.
    unsafe { alloc.private_data.get_data(guid, data_size, data) }
}

unsafe extern "system-unwind" fn allocator_set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    data_size: u32,
    data: *const c_void,
) -> i32 {
    // SAFETY: `this` is a valid `Allocator` COM object.
    let alloc = unsafe { &*this.cast::<Allocator>() };
    // SAFETY: `guid` is null or 16 readable bytes.
    let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: forwarding caller pointers to `PrivateData::set_data`.
    unsafe { alloc.private_data.set_data(guid, data_size, data) }
}

unsafe extern "system-unwind" fn allocator_set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    data: *const c_void,
) -> i32 {
    // SAFETY: `this` is a valid `Allocator` COM object.
    let alloc = unsafe { &*this.cast::<Allocator>() };
    // SAFETY: `guid` is null or 16 readable bytes.
    let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: forwarding caller pointers to `PrivateData::set_interface`.
    unsafe { alloc.private_data.set_interface(guid, data.cast_mut()) }
}

unsafe extern "system-unwind" fn allocator_set_name(this: *mut c_void, name: *const c_void) -> i32 {
    // SAFETY: `this` is a valid `Allocator` COM object.
    let alloc = unsafe { &*this.cast::<Allocator>() };
    // SAFETY: forwarding `name` to `com::set_name_private_data`.
    unsafe { com::set_name_private_data(&alloc.private_data, name.cast()) }
}

unsafe extern "system-unwind" fn allocator_get_device(
    this: *mut c_void,
    riid: *const c_void,
    ppv_device: *mut *mut c_void,
) -> i32 {
    // SAFETY: `this` is a valid `Allocator` COM object.
    let alloc = unsafe { &*this.cast::<Allocator>() };
    if alloc.device.is_null() {
        // SAFETY: `ppv_device` is a caller-supplied out-pointer.
        unsafe { com::init_out_ptr(ppv_device) };
        return E_NOINTERFACE;
    }
    // SAFETY: `alloc.device` is a valid `Device` COM object retained by `alloc`.
    let qi = unsafe { (*alloc.device).vtbl.query_interface };
    // SAFETY: forwarding `QueryInterface` to the owning `ID3D12Device`.
    unsafe { qi(alloc.device.cast(), riid, ppv_device) }
}

unsafe extern "system-unwind" fn allocator_reset(this: *mut c_void) -> i32 {
    crate::scope!("allocator.Reset");
    // SAFETY: `this` is a valid `Allocator` COM object.
    let alloc = unsafe { &mut *this.cast::<Allocator>() };
    alloc.reset_impl()
}

static ALLOCATOR_VTBL: ID3D12CommandAllocatorVtbl = ID3D12CommandAllocatorVtbl {
    query_interface: allocator_query_interface,
    add_ref: allocator_add_ref,
    release: allocator_release,
    get_private_data: allocator_get_private_data,
    set_private_data: allocator_set_private_data,
    set_private_data_interface: allocator_set_private_data_interface,
    set_name: allocator_set_name,
    get_device: allocator_get_device,
    reset: allocator_reset,
};

/// Casts a non-null `ID3D12CommandAllocator` pointer if its vtable matches [`Allocator`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_allocator(raw: *mut c_void) -> Option<*mut Allocator> {
    // SAFETY: `raw` is a readable COM object pointer when non-null.
    if unsafe { com::has_vtbl(raw, &ALLOCATOR_VTBL) } {
        Some(raw.cast::<Allocator>())
    } else {
        None
    }
}

/// `ID3D12Device::CreateCommandAllocator` (`d3d12_device.cpp:489`).
///
/// # Safety
///
/// `this`, `riid`, and `allocator` are caller-supplied COM pointers.
pub unsafe fn create_command_allocator(
    this: *mut c_void,
    list_type: u32,
    riid: *const c_void,
    allocator: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandAllocator");
    #[cfg(test)]
    if !this.is_null() && riid.is_null() {
        return crate::unimpl::report(file!(), "ID3D12Device::CreateCommandAllocator", line!());
    }
    // SAFETY: `allocator` is a caller-supplied out-pointer.
    unsafe { com::init_out_ptr(allocator) };
    if list_type > MAX_COMMAND_LIST_TYPE {
        return E_INVALIDARG;
    }

    // SAFETY: `this` is the `ID3D12Device` receiver pointer (or null in headless unit tests).
    let dev_ptr = if unsafe { com::has_vtbl(this, &device::vtbl::DEVICE_VTBL) } {
        this.cast::<Device>()
    } else {
        ptr::null_mut()
    };
    if !dev_ptr.is_null() {
        // SAFETY: `dev_ptr` is a valid `Device` COM object; retain one public ref for `Allocator`.
        unsafe { com::add_ref_com::<Device>(dev_ptr) };
    }

    let copy_temp_allocator = if dev_ptr.is_null() {
        None
    } else {
        // SAFETY: `dev_ptr` is a valid `Device`.
        unsafe { (*dev_ptr).mtl.as_ref() }.map(|mtl| {
            RingBump::new(
                mtl,
                BufferAllocationFlags::GPU_PRIVATE,
                RingConfig {
                    block: STAGING_BLOCK,
                    lifetime: LIFETIME,
                },
            )
        })
    };

    let boxed = Box::new(Allocator {
        vtbl: &ALLOCATOR_VTBL,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device: dev_ptr,
        list_type,
        spilled_cpu_heap: Vec::new(),
        cpu_heap: ptr::null_mut(),
        cpu_heap_offset: 0,
        gpu_heap_buffer: None,
        gpu_heap: ptr::null_mut(),
        gpu_heap_offset: 0,
        gpu_heap_buffer_address: 0,
        encoder_last: ptr::null_mut(),
        encoder_current: ptr::null_mut(),
        encoder_count: 0,
        barriers: 0,
        joins: Vec::new(),
        group: Vec::new(),
        deps: Vec::new(),
        covered: Vec::new(),
        transitions: Vec::new(),
        clears_from: ptr::null_mut(),
        clears: Vec::new(),
        kept: Vec::new(),
        encoder_lists: Vec::new(),
        icb: Vec::new(),
        icb_free: FxHashMap::default(),
        kept_textures: Vec::new(),
        clear_uav: None,
        clear_rtv: None,
        copy_temp_allocator,
        copy_temp_version: 0,
    });
    let raw_alloc = Box::into_raw(boxed);

    if !dev_ptr.is_null() {
        // SAFETY: `dev_ptr` is a valid `Device` pointer.
        let dev = unsafe { &*dev_ptr };
        let has_lib = dev.mtl.is_some() && dev.library.is_some();
        if has_lib {
            // SAFETY: `raw_alloc` is valid and uniquely owned here.
            let clear_uav = ClearUav::new(unsafe { &mut *raw_alloc });
            // SAFETY: `dev_ptr` has `mtl: Some(_)` checked above.
            let mtl = unsafe { (*dev_ptr).mtl.as_ref() }.unwrap_or_else(|| unreachable!());
            // SAFETY: `raw_alloc` is valid and uniquely owned here.
            let clear_rtv = ClearRtv::new(mtl, unsafe { &*raw_alloc });
            // SAFETY: `raw_alloc` is valid and uniquely owned here.
            let alloc_ref = unsafe { &mut *raw_alloc };
            alloc_ref.clear_uav = Some(clear_uav);
            alloc_ref.clear_rtv = Some(clear_rtv);
        }
    }

    // SAFETY: `raw_alloc` is valid and uniquely owned here.
    let init_hr = unsafe { (*raw_alloc).initialize() };
    if init_hr < 0 {
        // SAFETY: `raw_alloc` was allocated with `Box::into_raw` and has 0 references.
        drop(unsafe { Box::from_raw(raw_alloc) });
        return init_hr;
    }

    // SAFETY: `raw_alloc` is a valid `Allocator` COM object.
    let hr = unsafe { allocator_query_interface(raw_alloc.cast(), riid, allocator) };
    if hr < 0 {
        // SAFETY: `allocator_query_interface` did not add a reference on failure, so `raw_alloc`
        // still has 0 references and must be freed here.
        drop(unsafe { Box::from_raw(raw_alloc) });
    }
    hr
}

#[cfg(test)]
mod tests;
