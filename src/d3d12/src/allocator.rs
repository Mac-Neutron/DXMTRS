//! `ID3D12CommandAllocator` (`d3d12_command_allocator.{hpp,cpp}`): CPU/GPU command arenas, encoder
//!
//! dependency DAG construction (`Decide`), clear folding (`FoldClears`), and pooled ICBs.

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::{
    library::{ClearRtv, ClearUav},
    resource::Texture,
    ring::RingBump,
};
use dxmt_util::{com::ComObject, private_data::PrivateData};
use rustc_hash::FxHashMap;
use winemetal_sys::Obj;

use crate::{
    abi::vtbl::ID3D12CommandAllocatorVtbl,
    com::{ComRefCounted, Hresult},
    command_signature::CommandSignature,
    device::Device,
    encoder::{ClearEncoderData, EncoderData, RenderEncoderData},
    pipeline::{Compute, Graphics},
};

/// CPU command arena block size (`kCPUHeapSize`, `d3d12_command_allocator.hpp:29`).
pub const CPU_HEAP_SIZE: usize = 0x40_0000;

/// GPU argument/constant arena block size (`kGPUHeapSize`, `d3d12_command_allocator.hpp:30`).
pub const GPU_HEAP_SIZE: usize = 0x40_0000;

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

impl Allocator {
    /// Begins recording a command list on this allocator (`StartRecord`,
    ///
    /// `d3d12_command_allocator.hpp:249`).
    ///
    /// # Errors
    ///
    /// Returns `E_FAIL` if this allocator already has a command list recording on it.
    pub fn start_record(&mut self) -> Result<*mut EncoderData, Hresult> {
        crate::not_implemented!("allocator::Allocator::start_record")
    }

    /// Ends recording the active command list on this allocator (`EndRecord`,
    ///
    /// `d3d12_command_allocator.hpp:261`).
    ///
    /// # Errors
    ///
    /// Returns `E_FAIL` if this allocator does not currently have a recording command list.
    pub fn end_record(&mut self) -> Result<usize, Hresult> {
        crate::not_implemented!("allocator::Allocator::end_record")
    }

    /// Closes and links `encoder_current` into the recorded encoder list (`InvalidateCurrentPass`,
    ///
    /// `d3d12_command_allocator.hpp:163`).
    pub fn invalidate_current_pass(&mut self) {
        let (): () = crate::not_implemented!("allocator::Allocator::invalidate_current_pass");
    }

    /// Folds trailing clear passes into render pass `r`'s load actions (`FoldClears`,
    ///
    /// `d3d12_command_allocator.hpp:181`).
    pub fn fold_clears(&mut self, _r: &mut RenderEncoderData) -> u32 {
        crate::not_implemented!("allocator::Allocator::fold_clears")
    }

    /// Allocates `length` bytes with `alignment` from the CPU command arena (`AllocateCPUHeap`,
    ///
    /// `d3d12_command_allocator.hpp:273`).
    pub fn allocate_cpu_heap(&mut self, _length: usize, _alignment: usize) -> *mut u8 {
        crate::not_implemented!("allocator::Allocator::allocate_cpu_heap")
    }

    /// Allocates `length` bytes with `alignment` from the GPU argument/constant arena
    ///
    /// (`AllocateGPUHeap`, `d3d12_command_allocator.hpp:461`).
    pub fn allocate_gpu_heap(&mut self, _length: usize, _alignment: usize) -> (*mut u8, usize) {
        let (): () = crate::not_implemented!("allocator::Allocator::allocate_gpu_heap");
        (core::ptr::null_mut(), 0)
    }

    /// Allocates a temporary staging buffer slice (`AllocateTempBuffer`,
    ///
    /// `d3d12_command_allocator.hpp:472`).
    pub fn allocate_temp_buffer(&mut self, _size: usize, _alignment: usize) -> (u64, u64) {
        let (): () = crate::not_implemented!("allocator::Allocator::allocate_temp_buffer");
        (0, 0)
    }

    /// Number of barrier calls recorded on the current command list (`BarrierCalls`,
    ///
    /// `d3d12_command_allocator.hpp:299`).
    #[must_use]
    pub const fn barrier_calls(&self) -> u32 {
        self.barriers
    }

    /// Records a barrier call (`Barrier`, `d3d12_command_allocator.hpp:306`).
    pub fn barrier(&mut self, _join: bool, _transitioned: &[*const c_void]) {
        let (): () = crate::not_implemented!("allocator::Allocator::barrier");
    }

    /// Marks `resource` as written by the current encoder (`Writes`,
    ///
    /// `d3d12_command_allocator.hpp:334`).
    pub fn writes(&mut self, _resource: *const c_void) {
        let (): () = crate::not_implemented!("allocator::Allocator::writes");
    }

    /// Retains `texture` until the allocator is reset (`KeepTexture`,
    ///
    /// `d3d12_command_allocator.hpp:347`).
    pub fn keep_texture(&mut self, _texture: Arc<Texture>) {
        let (): () = crate::not_implemented!("allocator::Allocator::keep_texture");
    }

    /// Computes dependency edges for the closing encoder `e` (`Decide`,
    ///
    /// `d3d12_command_allocator.hpp:358`).
    pub fn decide(&mut self, _e: *mut EncoderData) {
        let (): () = crate::not_implemented!("allocator::Allocator::decide");
    }

    /// Allocates and initializes a compute indirect command record (`EncodeIndirectComputeCommand`,
    ///
    /// `d3d12_command_allocator.cpp:95`).
    pub fn encode_indirect_compute_command(
        &mut self,
        _cmd_sig: &CommandSignature,
        _pso: &Compute,
        _max_count: usize,
    ) -> *mut IndirectComputeCommandData {
        crate::not_implemented!("allocator::Allocator::encode_indirect_compute_command")
    }

    /// Allocates and initializes a render indirect command record (`EncodeIndirectRenderCommand`,
    ///
    /// `d3d12_command_allocator.cpp:113`).
    pub fn encode_indirect_render_command(
        &mut self,
        _cmd_sig: &CommandSignature,
        _pso: &Graphics,
        _max_count: usize,
    ) -> *mut IndirectRenderCommandData {
        crate::not_implemented!("allocator::Allocator::encode_indirect_render_command")
    }
}

/// Casts a non-null `ID3D12CommandAllocator` pointer if its vtable matches [`Allocator`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_allocator(raw: *mut c_void) -> Option<*mut Allocator> {
    let _ = raw;
    crate::not_implemented!("allocator::cast_allocator")
}

/// `ID3D12Device::CreateCommandAllocator` (`d3d12_device.cpp:489`).
///
/// # Safety
///
/// `this`, `riid`, and `allocator` are caller-supplied COM pointers.
pub unsafe fn create_command_allocator(
    _this: *mut c_void,
    _list_type: u32,
    _riid: *const c_void,
    _allocator: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandAllocator");
    crate::not_implemented!("ID3D12Device::CreateCommandAllocator")
}
