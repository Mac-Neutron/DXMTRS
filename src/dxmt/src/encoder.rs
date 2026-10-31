//! The encoder: D3D11's (and D3D9's) commands turned into Metal passes.
//!
//! The encode thread runs each chunk's commands on one [`Encoder`]. It holds what the shaders see
//! (the bindings, per [`stage`]), the pass being recorded and the dependency tracking of every
//! resource a pass touches: an access makes the pass wait for the passes that wrote (or read)
//! the resource before, keeps the resource's allocation alive until the chunk is retired, and
//! makes it resident. [`Encoder::encode_constant_buffers`] and its siblings write a shader's
//! argument table from the translator's reflection.
//!
//! Behaviour is the C++ `ArgumentEncodingContext`'s (`dxmt_context.*`). Written ahead for the
//! switches of `d3d11.dll` and `d3d12.dll`, which are its first callers.

use core::{alloc::Layout, ptr::NonNull};
use std::{
    collections::hash_map::Entry,
    sync::{Arc, Weak},
};

use pass::PassKind;
use rustc_hash::FxHashMap;
use winemetal_sys::{
    MTLDevice_newEvent, MTLDevice_newFence, MTLDevice_newSamplerState, Obj,
    wire::{
        MemPtr, WMT_COMPARE_FUNCTION_NEVER, WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK, WMT_SAMPLER_MIN_MAG_FILTER_NEAREST,
        WMT_SAMPLER_MIP_FILTER_NOT_MIPMAPPED, WMTSamplerInfo, WmtcmdBase,
        WmtcmdBlitCopyFromBufferToBuffer, WmtcmdBlitCopyFromBufferToTexture,
        WmtcmdBlitCopyFromBufferToTextureWithblitoption, WmtcmdBlitCopyFromTextureToBuffer,
        WmtcmdBlitCopyFromTextureToBufferWithblitoption, WmtcmdBlitCopyFromTextureToTexture,
        WmtcmdBlitCopyTexture, WmtcmdBlitFenceOp, WmtcmdBlitFillbuffer, WmtcmdBlitGenerateMipmaps,
        WmtcmdBlitResetcommands, WmtcmdBlitResolvecounters, WmtcmdComputeDispatch,
        WmtcmdComputeDispatchIndirect, WmtcmdComputeExecutecommands, WmtcmdComputeFenceOp,
        WmtcmdComputeMemoryBarrier, WmtcmdComputeSetbuffer, WmtcmdComputeSetbufferoffset,
        WmtcmdComputeSetbytes, WmtcmdComputeSetpso, WmtcmdComputeSettexture,
        WmtcmdComputeUseresource, WmtcmdRenderDispatchThreadsPerTile, WmtcmdRenderDraw,
        WmtcmdRenderDrawIndexed, WmtcmdRenderDrawIndexedIndirect, WmtcmdRenderDrawIndirect,
        WmtcmdRenderDrawMeshthreadgroups, WmtcmdRenderDrawMeshthreadgroupsIndirect,
        WmtcmdRenderDxmtGeometryDraw, WmtcmdRenderDxmtGeometryDrawIndexed,
        WmtcmdRenderDxmtGeometryDrawIndexedIndirect, WmtcmdRenderDxmtGeometryDrawIndirect,
        WmtcmdRenderDxmtTessellationMeshDraw, WmtcmdRenderDxmtTessellationMeshDrawIndexed,
        WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect,
        WmtcmdRenderDxmtTessellationMeshDrawIndirect, WmtcmdRenderExecutecommands,
        WmtcmdRenderFenceOp, WmtcmdRenderMemoryBarrier, WmtcmdRenderSetblendcolor,
        WmtcmdRenderSetbuffer, WmtcmdRenderSetbufferoffset, WmtcmdRenderSetbytes,
        WmtcmdRenderSetdepthstencilstate, WmtcmdRenderSetdsso, WmtcmdRenderSetpso,
        WmtcmdRenderSetrasterizerstate, WmtcmdRenderSetsampler, WmtcmdRenderSetscissorrect,
        WmtcmdRenderSetscissorrects, WmtcmdRenderSetstencilref, WmtcmdRenderSettexture,
        WmtcmdRenderSetviewport, WmtcmdRenderSetviewports, WmtcmdRenderSetvisibilitymode,
        WmtcmdRenderUseresource,
    },
};

use crate::{
    arena::{BlockPool, Commands},
    deptrack::{Access, FenceLocalityCheck, GenericAccessTracker, PARITY_LANE},
    resource::{
        Buffer, BufferAllocation, BufferAllocationFlags, BufferView, BufferViewKey, Texture,
        TextureAllocation, TextureAllocationFlags, TextureView, TextureViewKey,
    },
};

mod argument;
mod flush;
mod optimize;
mod pass;
mod resident;

/// Shader stages: the C++'s six bindings sets.
const STAGES: usize = 6;
const CONSTANT_BUFFER_SLOTS: usize = 14;
const SAMPLER_SLOTS: usize = 16;
const RESOURCE_SLOTS: usize = 128;
const UAV_SLOTS: usize = 64;
const VERTEX_BUFFER_SLOTS: usize = 32;
const STREAM_OUTPUT_SLOTS: usize = 4;
/// The zeroed buffer an unbound constant or vertex buffer reads.
const DUMMY_CBUFFER: usize = 0x1_0000;
/// A record's alignment in the encoder's memory, the C++'s.
const RECORD_ALIGN: usize = 16;
/// Allocations the use tables hold before they first look for dead ones.
const PRUNE_FLOOR: usize = 256;
/// `SM50BindingType` (`airconv_public.h`: an `enum class` the C side sees as a `uint32_t`).
const BINDING_SAMPLER: u32 = 1;
const BINDING_SRV: u32 = 2;
const BINDING_UAV: u32 = 3;

/// A null `next` for a record: [`Encoder::encode_render_command`] links it.
#[cfg(target_pointer_width = "64")]
pub const NO_NEXT: MemPtr = MemPtr { ptr: 0 };
/// A null `next` for a record: [`Encoder::encode_render_command`] links it.
#[cfg(target_pointer_width = "32")]
pub const NO_NEXT: MemPtr = MemPtr { ptr: 0, high: 0 };

/// The shader stages, the const generic argument `S` of the per-stage calls.
pub mod stage {
    pub const VERTEX: u8 = 0;
    pub const PIXEL: u8 = 1;
    pub const GEOMETRY: u8 = 2;
    pub const HULL: u8 = 3;
    pub const DOMAIN: u8 = 4;
    pub const COMPUTE: u8 = 5;
}

/// The pipeline kinds, the const generic argument `K` of the calls that encode for one.
pub mod kind {
    pub const ORDINARY: u8 = 0;
    pub const TESSELLATION: u8 = 1;
    pub const GEOMETRY: u8 = 2;
}

/// A `wmtcmd_*` record mirror of `winemetal-wire`, which a pass's records link into a list.
///
/// # Safety
///
/// The type is `#[repr(C)]` and led by `wmtcmd_base`'s fields (`type`, `reserved`, `next`), so a
/// record is linked through a [`WmtcmdBase`] pointer.
pub unsafe trait Record {}

/// A sampler as an argument table takes it.
pub struct BoundSampler {
    /// The sampler state's resource id.
    pub state: u64,
    /// The resource id of the state a cube texture samples with.
    pub cube_state: u64,
    pub lod_bias: f32,
    /// Keeps the sampler states alive while they are bound.
    pub owner: Arc<dyn Send + Sync>,
}

/// The part of a buffer a shader resource or unordered-access view sees: the C++'s `BufferSlice`.
pub struct BufferSlice {
    pub byte_offset: u32,
    pub byte_length: u32,
    pub first_element: u32,
    pub element_count: u32,
}

/// What a shader resource or unordered-access slot holds.
pub enum ResourceView {
    None,
    /// A buffer, read raw or through a texel view.
    Buffer {
        buffer: Arc<Buffer>,
        key: BufferViewKey,
        slice: BufferSlice,
    },
    Texture {
        texture: Arc<Texture>,
        key: TextureViewKey,
    },
}

/// A pass's slice of the argument-buffer ring: memory both the CPU and the GPU reach.
pub struct ArgumentBuffer {
    start: NonNull<u64>,
    qwords: usize,
    buffer: u64,
    offset: u64,
}

// SAFETY: the slice is memory `ArgumentBuffer::new` gave to the one pass holding it, so moving it
// to another thread moves its only user.
unsafe impl Send for ArgumentBuffer {}

impl ArgumentBuffer {
    /// The slice of `len` bytes at `mapping`, offset `offset` of the Metal buffer `buffer`.
    ///
    /// # Safety
    ///
    /// `mapping` is aligned to 8 and the start of `len` bytes that stay mapped, and that nothing
    /// but the pass this slice is given to reads or writes, until the GPU has run the pass.
    #[must_use]
    pub const unsafe fn new(mapping: NonNull<u8>, len: usize, buffer: u64, offset: u64) -> Self {
        Self {
            start: mapping.cast(),
            qwords: len / 8,
            buffer,
            offset,
        }
    }

    /// The Metal buffer the slice is part of.
    #[must_use]
    pub const fn buffer(&self) -> u64 {
        self.buffer
    }
}

pub use flush::QueryBuffers;
pub use pass::{
    Attachment, ColorTarget, DepthTarget, Pass, RenderTargets, RenderUses, StencilTarget, Target,
    TemporalTextures,
};

/// The encode thread's state: bindings, the pass being recorded and the dependency tracking.
pub struct Encoder {
    vertex_buffers: Box<[VertexBufferBinding]>,
    index_buffer: Option<Arc<Buffer>>,
    constant_buffers: Box<[ConstantBufferBinding]>,
    samplers: Box<[Option<BoundSampler>]>,
    resources: Box<[ResourceView]>,
    pixel_uavs: Box<[ResourceView]>,
    compute_uavs: Box<[ResourceView]>,
    stream_outputs: Box<[StreamOutputBinding]>,
    _dummy_sampler: Option<Obj>,
    dummy_sampler_id: u64,
    dummy_cbuffer: Option<Arc<BufferAllocation>>,
    /// The pool of fences encoder ids map to, slot `id % PARITY_LANE`.
    fences: Box<[Option<Obj>]>,
    next_id: u64,
    pass: Option<Pass>,
    /// The passes ended in this chunk, in order: the C++'s encoder list.
    // ponytail: a Vec whose capacity survives the flush, not the C++'s list in the encoder's
    // memory; a pass's kind boxes only a render pass's targets.
    passes: Vec<Pass>,
    locality: FenceLocalityCheck,
    /// The event a timestamp sample signals and waits on, so Metal keeps the passes in order.
    barrier_event: Option<Obj>,
    barrier_index: u64,
    /// The barrier bits a pass ignores inside itself (`setIntrapassBarrierControl`).
    intrapass_barriers: u64,
    /// The memory the passes' records live in until the chunk's next reset.
    heap: Commands,
    /// The chunk being encoded.
    seq: u64,
    retained: Vec<Arc<dyn Send + Sync>>,
    buffers_used: UseTable<BufferViewKey>,
    textures_used: UseTable<TextureViewKey>,
}

impl Encoder {
    /// An encoder on `device`, its records in blocks of `heap` bytes (the queue's `encoder_heap`).
    ///
    /// It makes the dummy sampler and the zeroed 64 KiB buffer unbound slots read, and the fence
    /// pool. Natively the buffer is Metal's own memory, as the C++'s (`aligned_alloc` gives it
    /// none there); elsewhere it is placed in page-aligned memory, the VM guest's shared memory
    /// in a guest build.
    #[must_use]
    pub fn new(device: &Obj, heap: usize) -> Self {
        let mut info = WMTSamplerInfo {
            min_filter: WMT_SAMPLER_MIN_MAG_FILTER_NEAREST,
            mag_filter: WMT_SAMPLER_MIN_MAG_FILTER_NEAREST,
            mip_filter: WMT_SAMPLER_MIP_FILTER_NOT_MIPMAPPED,
            r_address_mode: WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
            s_address_mode: WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
            t_address_mode: WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
            border_color: WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK,
            compare_function: WMT_COMPARE_FUNCTION_NEVER,
            lod_min_clamp: 0.0,
            lod_max_clamp: f32::MAX,
            max_anisotroy: 1,
            normalized_coords: 1,
            lod_average: 0,
            support_argument_buffers: 1,
            gpu_resource_id: 0,
        };
        // SAFETY: `device` holds a live device; `info` is a sampler description winemetal fills in.
        let sampler = unsafe { MTLDevice_newSamplerState(device.handle(), &raw mut info) };
        // SAFETY: `newSamplerState` returns a +1 handle or null.
        let sampler = unsafe { Obj::adopt(sampler) };
        let mut flags = BufferAllocationFlags::CPU_WRITE_COMBINED;
        if cfg!(not(target_os = "macos")) {
            flags |= BufferAllocationFlags::CPU_PLACED;
        }
        let cbuffer = Buffer::new(DUMMY_CBUFFER as u64, device).allocate(flags);
        let contents = cbuffer.mapped_memory(0);
        if !contents.is_null() {
            // SAFETY: the allocation's contents, at least `DUMMY_CBUFFER` bytes, nobody else's yet.
            unsafe { contents.write_bytes(0, DUMMY_CBUFFER) };
        }
        let fences = (0..PARITY_LANE)
            .map(|_| {
                // SAFETY: `device` holds a live device.
                let fence = unsafe { MTLDevice_newFence(device.handle()) };
                // SAFETY: `newFence` returns a +1 handle or null.
                unsafe { Obj::adopt(fence) }
            })
            .collect();
        // SAFETY: `device` holds a live device.
        let event = unsafe { MTLDevice_newEvent(device.handle()) };
        // SAFETY: `newEvent` returns a +1 handle or null.
        let event = unsafe { Obj::adopt(event) };
        let mut encoder = Self::with(heap, fences, sampler, info.gpu_resource_id, Some(cbuffer));
        encoder.barrier_event = event;
        encoder
    }

    /// An encoder without Metal objects: no dummies (unbound slots read address 0), no fences.
    #[cfg(test)]
    #[must_use]
    pub fn headless() -> Self {
        Self::with(0x1000, Box::default(), None, 0, None)
    }

    /// Starts encoding chunk `seq`: the previous chunk's records are gone.
    ///
    /// Allocations the encoder tracked that died since are forgotten once enough are tracked.
    pub fn begin_chunk(&mut self, seq: u64) {
        debug_assert!(self.pass.is_none(), "a chunk ends between passes");
        debug_assert!(self.passes.is_empty(), "the last chunk was flushed");
        // Passes not flushed would name records the reset frees.
        self.passes.clear();
        self.seq = seq;
        self.heap.reset();
        self.buffers_used.prune();
        self.textures_used.prune();
    }

    /// Ends the chunk: the allocations its passes used go to `retained`, the chunk's.
    pub fn end_chunk(&mut self, retained: &mut Vec<Arc<dyn Send + Sync>>) {
        retained.append(&mut self.retained);
    }

    /// The pass being recorded.
    #[must_use]
    pub const fn pass(&self) -> Option<&Pass> {
        self.pass.as_ref()
    }

    /// Fence `slot` of the pool, or the null handle.
    #[must_use]
    pub fn fence(&self, slot: usize) -> u64 {
        self.fences
            .get(slot)
            .and_then(Option::as_ref)
            .map_or(0, Obj::handle)
    }

    /// Appends `cmd` to the render pass's records.
    pub fn encode_render_command<T: Record>(&mut self, cmd: T) {
        self.append(cmd, |k| matches!(k, PassKind::Render(_)));
    }

    /// Appends `cmd` to the compute pass's records.
    pub fn encode_compute_command<T: Record>(&mut self, cmd: T) {
        self.append(cmd, |k| matches!(k, PassKind::Compute));
    }

    /// Uninitialized memory of `size` bytes aligned to `align`, valid until the next chunk.
    ///
    /// What a record points to (a set-bytes record's bytes) lives here.
    ///
    /// # Panics
    ///
    /// When `align` is not a power of two, or `size` rounded up to it overflows `isize`.
    pub fn allocate_cpu(&mut self, size: usize, align: usize) -> NonNull<u8> {
        let layout = Layout::from_size_align(size, align).expect("a valid size and alignment");
        self.heap.allocate(layout)
    }

    pub fn bind_vertex_buffer(
        &mut self,
        slot: usize,
        offset: u32,
        stride: u32,
        buffer: Option<Arc<Buffer>>,
    ) {
        self.vertex_buffers[slot] = VertexBufferBinding {
            buffer,
            offset,
            stride,
        };
    }

    pub fn bind_vertex_buffer_offset(&mut self, slot: usize, offset: u32, stride: u32) {
        let binding = &mut self.vertex_buffers[slot];
        binding.offset = offset;
        binding.stride = stride;
    }

    pub fn bind_index_buffer(&mut self, buffer: Option<Arc<Buffer>>) {
        self.index_buffer = buffer;
    }

    pub fn bind_constant_buffer<const S: u8>(
        &mut self,
        slot: usize,
        offset: u32,
        buffer: Option<Arc<Buffer>>,
    ) {
        self.constant_buffers[CONSTANT_BUFFER_SLOTS * usize::from(S) + slot] =
            ConstantBufferBinding { buffer, offset };
    }

    pub fn bind_constant_buffer_offset<const S: u8>(&mut self, slot: usize, offset: u32) {
        self.constant_buffers[CONSTANT_BUFFER_SLOTS * usize::from(S) + slot].offset = offset;
    }

    pub fn bind_sampler<const S: u8>(&mut self, slot: usize, sampler: Option<BoundSampler>) {
        self.samplers[SAMPLER_SLOTS * usize::from(S) + slot] = sampler;
    }

    /// Binds a shader resource view (a buffer or a texture) of stage `S`, or unbinds the slot.
    pub fn bind_shader_resource<const S: u8>(&mut self, slot: usize, view: ResourceView) {
        self.resources[RESOURCE_SLOTS * usize::from(S) + slot] = view;
    }

    /// Binds an unordered-access view: the compute stage's, or the pixel stage's for the others.
    pub fn bind_unordered_access<const S: u8>(&mut self, slot: usize, view: ResourceView) {
        if S == stage::COMPUTE {
            self.compute_uavs[slot] = view;
        } else {
            self.pixel_uavs[slot] = view;
        }
    }

    /// Binds a stream-output target at byte `offset`.
    ///
    /// Its filled-size counter is not bound: the C++ never reads it (it writes 0 in its place).
    pub fn bind_stream_output_buffer(
        &mut self,
        slot: usize,
        offset: u32,
        buffer: Option<Arc<Buffer>>,
    ) {
        self.stream_outputs[slot] = StreamOutputBinding { buffer, offset };
    }

    pub fn bind_stream_output_buffer_offset(&mut self, slot: usize, offset: u32) {
        self.stream_outputs[slot].offset = offset;
    }

    /// Unbinds every slot but the stream-output targets, as the C++'s `clearState`.
    pub fn clear_state(&mut self) {
        let unbound = |n| (0..n).map(|_| ResourceView::None).collect();
        self.vertex_buffers = vertex_buffers();
        self.index_buffer = None;
        self.constant_buffers = constant_buffers();
        self.samplers = (0..SAMPLER_SLOTS * STAGES).map(|_| None).collect();
        self.resources = unbound(RESOURCE_SLOTS * STAGES);
        self.pixel_uavs = unbound(UAV_SLOTS);
        self.compute_uavs = unbound(UAV_SLOTS);
    }

    /// The index buffer's Metal buffer and current suballocation's offset, read by stage vertex.
    pub fn current_index_buffer(&mut self) -> Option<(u64, u64)> {
        let buffer = self.index_buffer.take()?;
        let current = self.access_buffer::<{ stage::VERTEX }>(&buffer, Access::READ);
        self.index_buffer = Some(buffer);
        current.map(|(allocation, offset)| (allocation.buffer(), offset))
    }

    /// `buffer`'s current allocation and current suballocation's offset, used as `access` says.
    ///
    /// The pass waits for the earlier passes the access depends on, and the chunk keeps the
    /// allocation alive. `None` when the buffer has no allocation.
    pub fn access_buffer<const S: u8>(
        &mut self,
        buffer: &Buffer,
        access: Access,
    ) -> Option<(Arc<BufferAllocation>, u64)> {
        let allocation = buffer.current()?;
        self.track_buffer::<S>(&allocation, access);
        let offset = allocation.current_suballocation_offset();
        Some((allocation, offset))
    }

    /// `buffer`'s view `key` and its current suballocation's first texel, used as `access` says.
    pub fn access_buffer_view<const S: u8>(
        &mut self,
        buffer: &Buffer,
        key: BufferViewKey,
        access: Access,
    ) -> Option<(BufferView, u64)> {
        self.buffer_view::<S>(buffer, key, access)
            .map(|(_, view, texel)| (view, texel))
    }

    /// The Metal texture of `texture`'s current allocation, subresource `level` of `slice` used.
    pub fn access_texture<const S: u8>(
        &mut self,
        texture: &Texture,
        level: u32,
        slice: u32,
        access: Access,
    ) -> u64 {
        let Some(allocation) = texture.current() else {
            return 0;
        };
        self.track_texture::<S>(&allocation, access, slice..slice + 1, level..level + 1);
        allocation.texture()
    }

    /// `texture`'s view `key`, each subresource it covers used as `access` says.
    pub fn access_texture_view<const S: u8>(
        &mut self,
        texture: &Texture,
        key: TextureViewKey,
        access: Access,
    ) -> Option<TextureView> {
        self.texture_view::<S>(texture, key, access)
            .map(|(_, view)| view)
    }

    fn with(
        heap: usize,
        fences: Box<[Option<Obj>]>,
        dummy_sampler: Option<Obj>,
        dummy_sampler_id: u64,
        dummy_cbuffer: Option<Arc<BufferAllocation>>,
    ) -> Self {
        let unbound = |n| (0..n).map(|_| ResourceView::None).collect();
        Self {
            vertex_buffers: vertex_buffers(),
            index_buffer: None,
            constant_buffers: constant_buffers(),
            samplers: (0..SAMPLER_SLOTS * STAGES).map(|_| None).collect(),
            resources: unbound(RESOURCE_SLOTS * STAGES),
            pixel_uavs: unbound(UAV_SLOTS),
            compute_uavs: unbound(UAV_SLOTS),
            stream_outputs: (0..STREAM_OUTPUT_SLOTS)
                .map(|_| StreamOutputBinding {
                    buffer: None,
                    offset: 0,
                })
                .collect(),
            _dummy_sampler: dummy_sampler,
            dummy_sampler_id,
            dummy_cbuffer,
            fences,
            next_id: PARITY_LANE,
            pass: None,
            passes: Vec::new(),
            locality: FenceLocalityCheck::default(),
            barrier_event: None,
            barrier_index: 0,
            intrapass_barriers: 0,
            heap: Commands::new(Arc::new(BlockPool::new(heap))),
            seq: 0,
            retained: Vec::new(),
            buffers_used: UseTable::default(),
            textures_used: UseTable::default(),
        }
    }

    fn append<T: Record>(&mut self, cmd: T, kind_ok: impl Fn(&PassKind) -> bool) {
        let Some(pass) = self.pass.as_mut() else {
            debug_assert!(false, "a record outside a pass");
            return;
        };
        debug_assert!(kind_ok(&pass.kind), "a record of another pass kind");
        let record = place(&mut self.heap, cmd).cast::<WmtcmdBase>();
        // SAFETY: `tail` is the pass's last record, in `heap` until its next reset (between
        // chunks, with no pass open); a record starts with `WmtcmdBase`'s fields (`Record`).
        unsafe { (*pass.records.tail.as_ptr()).next = mem_ptr(record) };
        // SAFETY: `record` was just placed, a `Record` led by `WmtcmdBase`'s fields.
        unsafe { (*record.as_ptr()).next = NO_NEXT };
        pass.records.tail = record;
    }

    /// Keeps `allocation` alive until the chunk is retired, once per chunk.
    fn retain<T: Send + Sync + 'static>(&mut self, allocation: &Arc<T>, already: bool) {
        if !already {
            self.retained
                .push(Arc::clone(allocation) as Arc<dyn Send + Sync>);
        }
    }

    fn track_buffer<const S: u8>(&mut self, allocation: &Arc<BufferAllocation>, access: Access) {
        self.retain(allocation, allocation.check_retained(self.seq));
        if allocation
            .flags()
            .contains(BufferAllocationFlags::GPU_READONLY)
        {
            return;
        }
        let used = self
            .buffers_used
            .entry(allocation, buffer_trackers(allocation));
        let suballocation = usize::try_from(allocation.current_suballocation()).ok();
        let Some(tracker) = suballocation.and_then(|s| used.trackers.get_mut(s)) else {
            debug_assert!(false, "a suballocation past the buffer's");
            return;
        };
        track::<S>(self.pass.as_mut(), tracker, access);
    }

    /// Tracks subresources `levels` of `slices`, or the one tracker of a texture shaders only read.
    fn track_texture<const S: u8>(
        &mut self,
        allocation: &Arc<TextureAllocation>,
        access: Access,
        slices: core::ops::Range<u32>,
        levels: core::ops::Range<u32>,
    ) {
        self.retain(allocation, allocation.check_retained(self.seq));
        let flags = allocation.flags();
        if flags.contains(TextureAllocationFlags::GPU_READONLY) {
            return;
        }
        let used = self
            .textures_used
            .entry(allocation, texture_trackers(allocation));
        let mut pass = self.pass.as_mut();
        if flags.contains(TextureAllocationFlags::SHADER_READONLY) {
            if let Some(tracker) = used.trackers.first_mut() {
                track::<S>(pass, tracker, access);
            }
            return;
        }
        let levels_per_slice = allocation.mip_level_count();
        for slice in slices {
            for level in levels.clone() {
                let index = usize::try_from(slice * levels_per_slice + level).ok();
                let Some(tracker) = index.and_then(|i| used.trackers.get_mut(i)) else {
                    debug_assert!(false, "a subresource past the texture's");
                    continue;
                };
                track::<S>(pass.as_deref_mut(), tracker, access);
            }
        }
    }

    fn buffer_view<const S: u8>(
        &mut self,
        buffer: &Buffer,
        key: BufferViewKey,
        access: Access,
    ) -> Option<(Arc<BufferAllocation>, BufferView, u64)> {
        let allocation = buffer.current()?;
        self.track_buffer::<S>(&allocation, access);
        let view = buffer.view(&allocation, key);
        let texel = allocation.current_suballocation_offset_in(view.suballocation_texel);
        Some((allocation, view, texel))
    }

    fn texture_view<const S: u8>(
        &mut self,
        texture: &Texture,
        key: TextureViewKey,
        access: Access,
    ) -> Option<(Arc<TextureAllocation>, TextureView)> {
        let allocation = texture.current()?;
        let view = allocation.view(key);
        let slices = key.first_slice()..key.first_slice() + key.array_size();
        let levels = key.first_mip()..key.first_mip() + key.mip_count();
        self.track_texture::<S>(&allocation, access, slices, levels);
        Some((allocation, view))
    }
}

/// The records of a pass: a list from a no-op head, linked through `next`.
struct Records {
    head: NonNull<WmtcmdBase>,
    tail: NonNull<WmtcmdBase>,
}

// SAFETY: the records live in the memory of the encoder whose pass holds them, which moves with it.
unsafe impl Send for Records {}

struct VertexBufferBinding {
    buffer: Option<Arc<Buffer>>,
    offset: u32,
    stride: u32,
}

struct ConstantBufferBinding {
    buffer: Option<Arc<Buffer>>,
    offset: u32,
}

struct StreamOutputBinding {
    buffer: Option<Arc<Buffer>>,
    offset: u32,
}

/// What the encoder keeps of one allocation it used: the C++ allocation's trackers and residency.
struct Used<K> {
    /// Keeps the allocation's memory, and so its address (the key), from reuse while it exists.
    owner: Weak<dyn Send + Sync>,
    trackers: Box<[GenericAccessTracker]>,
    residency: resident::Residency,
    views: Vec<(K, resident::Residency)>,
}

/// The allocations the encoder used, by address.
// ponytail: one hash lookup per access, and a sweep for dead allocations when the table doubles;
// the trackers and residency become fields of the allocations once their files take them.
struct UseTable<K> {
    map: FxHashMap<usize, Used<K>>,
    prune_at: usize,
}

impl<K> Default for UseTable<K> {
    fn default() -> Self {
        Self {
            map: FxHashMap::default(),
            prune_at: PRUNE_FLOOR,
        }
    }
}

impl<K> UseTable<K> {
    /// `allocation`'s entry, made with `trackers` trackers the first time.
    ///
    /// An entry's `Weak` keeps the allocation's memory, so no other allocation takes its address
    /// while the entry exists: the address names one allocation.
    fn entry<T: Send + Sync + 'static>(
        &mut self,
        allocation: &Arc<T>,
        trackers: u32,
    ) -> &mut Used<K> {
        match self.map.entry(Arc::as_ptr(allocation).addr()) {
            Entry::Occupied(e) => {
                debug_assert!(
                    e.get().owner.strong_count() > 0,
                    "a live allocation's entry"
                );
                e.into_mut()
            }
            Entry::Vacant(e) => e.insert(Used {
                owner: Arc::downgrade(allocation) as Weak<dyn Send + Sync>,
                trackers: (0..trackers)
                    .map(|_| GenericAccessTracker::default())
                    .collect(),
                residency: resident::Residency::default(),
                views: Vec::new(),
            }),
        }
    }

    /// Forgets the dead allocations once the table has doubled since the last time.
    fn prune(&mut self) {
        if self.map.len() < self.prune_at {
            return;
        }
        self.map.retain(|_, used| used.owner.strong_count() > 0);
        self.prune_at = (self.map.len() * 2).max(PRUNE_FLOOR);
    }
}

/// Tracks one access of the pass, by stage.
///
/// Compute (and blit) on the pass's id, pixel on the render pass's fragment half, every other
/// stage on its vertex half.
fn track<const S: u8>(pass: Option<&mut Pass>, tracker: &mut GenericAccessTracker, access: Access) {
    let Some(pass) = pass else {
        debug_assert!(false, "an access outside a pass");
        return;
    };
    let write = access.contains(Access::WRITE);
    let uav = access.contains(Access::UAV);
    let barriers = &mut pass.barriers;
    let (id, wait) = match S {
        stage::COMPUTE | stage::PIXEL => (pass.id, &mut pass.fence_wait),
        _ => (pass.id_vertex, &mut pass.fence_wait_vertex),
    };
    match S {
        stage::COMPUTE if write => tracker.access_exclusive(id, wait, barriers, uav),
        stage::COMPUTE => tracker.access_shared(id, wait, barriers),
        stage::PIXEL if write => tracker.access_exclusive_fragment(id, wait, barriers, uav),
        stage::PIXEL => tracker.access_shared_fragment(id, wait, barriers),
        _ if write => tracker.access_exclusive_pre_raster(id, wait, barriers, uav),
        _ => tracker.access_shared_pre_raster(id, wait, barriers),
    }
}

/// A buffer allocation's trackers: one per suballocation, none when the GPU only reads it.
const fn buffer_trackers(allocation: &BufferAllocation) -> u32 {
    if allocation
        .flags()
        .contains(BufferAllocationFlags::GPU_READONLY)
    {
        0
    } else {
        allocation.suballocation_count()
    }
}

const fn texture_trackers(allocation: &TextureAllocation) -> u32 {
    if allocation
        .flags()
        .contains(TextureAllocationFlags::GPU_READONLY)
    {
        0
    } else {
        allocation.tracker_count()
    }
}

/// An `SM50_BINDING_INDEX_*` (all below 8) as a record's byte.
fn binding_index(index: u32) -> u8 {
    u8::try_from(index).unwrap_or(u8::MAX)
}

/// `value` placed in `heap`, aligned as a record.
fn place<T>(heap: &mut Commands, value: T) -> NonNull<T> {
    let layout = Layout::new::<T>()
        .align_to(RECORD_ALIGN)
        .unwrap_or_else(|_| Layout::new::<T>());
    let p = heap.allocate(layout).cast::<T>();
    // SAFETY: fresh memory of `T`'s size, aligned at least as `T`.
    unsafe { p.write(value) };
    p
}

/// `record`'s address as a `next` of `winemetal.h`.
#[cfg(target_pointer_width = "64")]
fn mem_ptr(record: NonNull<WmtcmdBase>) -> MemPtr {
    MemPtr {
        ptr: record.as_ptr().expose_provenance() as u64,
    }
}

/// `record`'s address as a `next` of `winemetal.h`: a 32-bit address and a high half of 0.
#[cfg(target_pointer_width = "32")]
fn mem_ptr(record: NonNull<WmtcmdBase>) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(record.as_ptr().expose_provenance()).unwrap_or(0),
        high: 0,
    }
}

fn vertex_buffers() -> Box<[VertexBufferBinding]> {
    (0..VERTEX_BUFFER_SLOTS)
        .map(|_| VertexBufferBinding {
            buffer: None,
            offset: 0,
            stride: 0,
        })
        .collect()
}

fn constant_buffers() -> Box<[ConstantBufferBinding]> {
    (0..CONSTANT_BUFFER_SLOTS * STAGES)
        .map(|_| ConstantBufferBinding {
            buffer: None,
            offset: 0,
        })
        .collect()
}

macro_rules! records {
    ($($t:ty),* $(,)?) => {
        $(
            // SAFETY: a `wmtcmd_*` mirror of winemetal-wire, `#[repr(C)]` and led by
            // `wmtcmd_base`'s fields (its layout test pins each against the C).
            unsafe impl Record for $t {}
        )*
    };
}

records!(
    WmtcmdBlitCopyFromBufferToBuffer,
    WmtcmdBlitCopyFromBufferToTexture,
    WmtcmdBlitCopyFromBufferToTextureWithblitoption,
    WmtcmdBlitCopyFromTextureToBuffer,
    WmtcmdBlitCopyFromTextureToBufferWithblitoption,
    WmtcmdBlitCopyFromTextureToTexture,
    WmtcmdBlitCopyTexture,
    WmtcmdBlitFenceOp,
    WmtcmdBlitFillbuffer,
    WmtcmdBlitGenerateMipmaps,
    WmtcmdBlitResetcommands,
    WmtcmdBlitResolvecounters,
    WmtcmdComputeDispatch,
    WmtcmdComputeDispatchIndirect,
    WmtcmdComputeExecutecommands,
    WmtcmdComputeFenceOp,
    WmtcmdComputeMemoryBarrier,
    WmtcmdComputeSetbuffer,
    WmtcmdComputeSetbufferoffset,
    WmtcmdComputeSetbytes,
    WmtcmdComputeSetpso,
    WmtcmdComputeSettexture,
    WmtcmdComputeUseresource,
    WmtcmdRenderDispatchThreadsPerTile,
    WmtcmdRenderDraw,
    WmtcmdRenderDrawIndexed,
    WmtcmdRenderDrawIndexedIndirect,
    WmtcmdRenderDrawIndirect,
    WmtcmdRenderDrawMeshthreadgroups,
    WmtcmdRenderDrawMeshthreadgroupsIndirect,
    WmtcmdRenderDxmtGeometryDraw,
    WmtcmdRenderDxmtGeometryDrawIndexed,
    WmtcmdRenderDxmtGeometryDrawIndexedIndirect,
    WmtcmdRenderDxmtGeometryDrawIndirect,
    WmtcmdRenderDxmtTessellationMeshDraw,
    WmtcmdRenderDxmtTessellationMeshDrawIndexed,
    WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect,
    WmtcmdRenderDxmtTessellationMeshDrawIndirect,
    WmtcmdRenderExecutecommands,
    WmtcmdRenderFenceOp,
    WmtcmdRenderMemoryBarrier,
    WmtcmdRenderSetblendcolor,
    WmtcmdRenderSetbuffer,
    WmtcmdRenderSetbufferoffset,
    WmtcmdRenderSetbytes,
    WmtcmdRenderSetdepthstencilstate,
    WmtcmdRenderSetdsso,
    WmtcmdRenderSetpso,
    WmtcmdRenderSetrasterizerstate,
    WmtcmdRenderSetsampler,
    WmtcmdRenderSetscissorrect,
    WmtcmdRenderSetscissorrects,
    WmtcmdRenderSetstencilref,
    WmtcmdRenderSettexture,
    WmtcmdRenderSetviewport,
    WmtcmdRenderSetviewports,
    WmtcmdRenderSetvisibilitymode,
    WmtcmdRenderUseresource,
);

#[cfg(test)]
mod tests;
