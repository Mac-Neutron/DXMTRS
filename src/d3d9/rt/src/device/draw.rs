//! One draw on the core (C4-9, C4b-2, DXMTRS C4c-1): its pipeline, built once a key, its records.
//!
//! The front end builds the draw's [`PipelineKey`] and its per-draw data ([`DrawData`]). On a key
//! the device has not seen, [`pipeline_source`] turns it into both stages' DXSO and the vertex
//! layout, each stage is translated through the core's DXSO entry (`initialize_dxso`, then
//! `compile`, `compiled_data` and `load_function`, the D3D10-12 path) unless the device already
//! holds that stage's function for that specialization, and the pipeline is made with the vertex
//! descriptor (slot 156) and kept in the device's [`PsoMap`](dxmt_core::pipeline_cache::PsoMap).
//! The draw records the pipeline, the viewport, the rasterizer state, each vertex buffer slot the
//! pipeline reads, from the stream it binds (inline bytes, zeros, or a stored buffer, its read
//! tracked), the stages' binds and the draw (its indices, if any, in the argument-buffer ring or a
//! stored buffer) into the colour target's render pass, after the depth-stencil state when its key
//! names a depth format, and the scissor when the pass has not recorded that one last. When
//! asynchronous compile is enabled, new pipelines build on background workers and skippable draws
//! targeting rebuilt surfaces are omitted until ready, while non-skippable ones wait on encode.

use core::{
    hash::{Hash as _, Hasher as _},
    ptr::NonNull,
    sync::atomic::Ordering,
};
use std::{
    borrow::Cow,
    ops::Range,
    sync::{Arc, OnceLock},
};

use dxmt_core::{
    deptrack::Access,
    encoder::{NO_NEXT, direct, stage},
    pipeline_cache::{CompileState, Compiled, PsoMap},
    scheduler::{CompileLane, CompileScheduler},
};
use dxmt_util::sync::lock;
use mtld3d_core::{
    async_compile::ClearPlanes,
    clear_area::clamp_rect,
    depth_stencil_state::DepthStencilSnapshot,
    draw_data::NULL_STREAM_ZEROS,
    ids::BufferId,
    pipeline_key::{MAX_STREAMS, PipelineKey, PipelineSource, StreamKey, pipeline_source},
    pipeline_state::{BlendFactor, BlendOperation, CoreBlend},
    streams::{advanced_binding_offset, instance_count, offset_shift, slot_binding_offset},
};
use winemetal_sys::{
    MTLDevice_newRenderPipelineStateWithVertexDescriptor, Obj,
    wire::{
        MemPtr, WMT_BLEND_FACTOR_BLEND_COLOR, WMT_BLEND_FACTOR_DESTINATION_ALPHA,
        WMT_BLEND_FACTOR_DESTINATION_COLOR, WMT_BLEND_FACTOR_ONE,
        WMT_BLEND_FACTOR_ONE_MINUS_BLEND_COLOR, WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_ALPHA,
        WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_COLOR, WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA,
        WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_COLOR, WMT_BLEND_FACTOR_SOURCE_ALPHA,
        WMT_BLEND_FACTOR_SOURCE_ALPHA_SATURATED, WMT_BLEND_FACTOR_SOURCE_COLOR,
        WMT_BLEND_FACTOR_ZERO, WMT_BLEND_OPERATION_ADD, WMT_BLEND_OPERATION_MAX,
        WMT_BLEND_OPERATION_MIN, WMT_BLEND_OPERATION_REVERSE_SUBTRACT,
        WMT_BLEND_OPERATION_SUBTRACT, WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_DRAW_INDEXED,
        WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF, WMT_RENDER_COMMAND_SET_DSSO,
        WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
        WMT_RENDER_COMMAND_SET_SCISSOR_RECTS, WMT_RENDER_COMMAND_SET_VIEWPORTS,
        WMTColorAttachmentBlendInfo, WMTScissorRect, WMTVertexAttributeInfo,
        WMTVertexBufferLayoutInfo, WMTVertexDescriptorInfo, WMTViewport, WmtcmdRenderDraw,
        WmtcmdRenderDrawIndexed, WmtcmdRenderSetblendcolor, WmtcmdRenderSetdsso,
        WmtcmdRenderSetpso, WmtcmdRenderSetrasterizerstate, WmtcmdRenderSetscissorrects,
        WmtcmdRenderSetviewports,
    },
};

use super::{ClearTarget, CompileContext, CoreDevice, CoreOptions, LOG_TARGET};

/// The stage a [`Bind`] binds at.
// Copy: one byte, copied into the closure's bind list.
#[derive(Clone, Copy)]
pub enum Stage {
    Vertex,
    Pixel,
}

/// Bytes a draw binds at one stage's buffer slot; empty ones are not bound.
pub struct Bind<'a> {
    pub stage: Stage,
    pub slot: u8,
    pub bytes: &'a [u8],
}

/// One vertex stream's data for a draw, at each vertex buffer slot that binds the stream.
// Copy: an entry of the draw's array, which the tests' const fixture builds.
#[derive(Clone, Copy)]
pub enum StreamData<'a> {
    /// None: a stream the declaration does not name.
    None,
    /// Bytes bound inline (a UP draw's stream 0).
    Bytes(&'a [u8]),
    /// Zeros: a stream the declaration names with nothing bound.
    Zeros,
    /// A stored buffer ([`CoreDevice::buffer_create`]) read from byte `offset` (DXMTRS C4c-1).
    Buffer { id: BufferId, offset: u32 },
}

/// Where an indexed draw's indices are.
pub enum IndexData<'a> {
    /// Bytes, copied into the argument-buffer ring.
    Bytes(&'a [u8]),
    /// A stored buffer ([`CoreDevice::buffer_create`]), from byte `offset` (DXMTRS C4c-1).
    Buffer { id: BufferId, offset: u64 },
}

/// An indexed draw's indices.
pub struct Indices<'a> {
    pub data: IndexData<'a>,
    /// `MTLIndexType`: 0 for 16-bit, 1 for 32-bit.
    pub index_type: u8,
    pub count: u32,
}

/// What one draw binds and draws, as the front end resolved it.
///
/// A `DrawPrimitiveUP` or a `DrawIndexedPrimitiveUP`, or a draw from stored buffers (DXMTRS
/// C4c-1).
pub struct DrawData<'a> {
    /// The `MTLPrimitiveType`.
    pub primitive: u8,
    /// Each stream's data, by stream (DXMTRS C4c-1).
    pub streams: [StreamData<'a>; MAX_STREAMS as usize],
    pub vertex_count: u32,
    /// The first vertex of a draw without indices.
    pub vertex_start: u32,
    /// What an indexed draw adds to each index.
    pub base_vertex: i32,
    /// Stream 0's frequency word, and the bound streams whose word is instance data (bit `n`).
    ///
    /// None for a draw that never instances. The instance count is mtld3d's: the word's count
    /// when a stream the pipeline reads is in the mask, 1 otherwise.
    pub instancing: Option<(u32, u16)>,
    /// The indices of an indexed draw (a fan's triangle list too).
    pub indices: Option<Indices<'a>>,
    /// `(origin x, origin y, width, height, znear, zfar)`.
    pub viewport: [f64; 6],
    /// The rectangle the draw is cut to (`x, y, width, height`), which the pass's extent clamps.
    ///
    /// The scissor rect with the scissor test on, the drawn viewport with it off.
    pub scissor: [u32; 4],
    /// The `MTLTriangleFillMode`.
    pub fill_mode: u8,
    /// The `MTLCullMode`.
    pub cull_mode: u8,
    pub binds: &'a [Bind<'a>],
    /// The sampled slots' textures and samplers, each at its stage.
    pub textures: &'a [super::TextureBind],
    /// The depth and stencil state of the render states, not yet gated on the attached planes.
    ///
    /// Read only by a draw whose key names a depth format.
    pub depth_stencil: DepthStencilSnapshot,
    /// `D3DRS_STENCILREF`'s low byte.
    pub stencil_ref: u8,
    /// Metal's slope-scaled depth bias (the constant bias is the vertex stage's).
    pub slope_scale: f32,
    /// The blend colour (`D3DRS_BLENDFACTOR` as red, green, blue, alpha).
    ///
    /// For a draw whose key's blend reads it; none otherwise.
    pub blend_color: Option<[f32; 4]>,
    /// Monotonic front-end draw-cache generation for O(1) PSO and pass-state reuse; 0 to bypass.
    pub key_gen: u64,
}

/// Why a draw recorded nothing.
#[derive(Debug, PartialEq, Eq)]
pub enum DrawRefused {
    /// The colour target does not resolve: the device has no back buffer.
    NoBackBuffer,
    /// The draw's pipeline does not build (logged once per key).
    Pipeline,
    /// A layout no binding can fetch, or a binding it advances past its buffer (DXMTRS C4c-2a).
    ///
    /// The layout: an attribute wider than its stride, or more than 16 attributes under a fetch
    /// (its error logged once per key). mtld3d widens such a layout and draws other bytes.
    Fetch,
    /// A stored buffer the draw reads is not in the store (DXMTRS C4c-1).
    Buffer,
    /// The key names a depth format the bound depth-stencil has not (none is bound).
    ///
    /// Or the bound one is of another sample count than target 0, which no pass attaches beside
    /// it (DXMTRS C4b-3d5).
    Depth,
    /// The key names colour formats the bound targets do not make, or another sample count.
    ///
    /// Its colours are those of the pass set ([`CoreDevice::target_desc`]) under neither sRGB
    /// write (DXMTRS C4b-3d4); its count is target 0's (DXMTRS C4b-3d5).
    Target,
}

use dxmt_core::resource::{Buffer, Texture, TextureViewKey};
use mtld3d_core::pipeline_state::StreamLayout;

/// Maximum entries kept in [`PsoMemo`].
const PSO_MEMO_CAP: usize = 24;

/// A single active vertex-buffer slot binding precomputed from [`PipelineSource`].
struct ActiveBinding {
    slot: u8,
    stream: u8,
    advance: u32,
    stride: u32,
}

impl ActiveBinding {
    const EMPTY: Self = Self {
        slot: 0,
        stream: 0,
        advance: 0,
        stride: 0,
    };
}

/// One resolved pipeline entry cached in [`PsoMemo`].
struct PsoMemoEntry {
    key_gen: u64,
    key_hash: u64,
    key: PipelineKey,
    _entry: Arc<Compiled<Option<PipelineSource>>>,
    pso: u64,
    used_mask: u16,
    bindings: [ActiveBinding; MAX_STREAMS as usize],
    binding_count: u8,
}

impl PsoMemoEntry {
    fn active_bindings(&self) -> &[ActiveBinding] {
        &self.bindings[..usize::from(self.binding_count)]
    }
}

/// Pipeline state emission mode for [`CoreDevice::emit_general`].
enum PsoEmit {
    None,
    Handle(u64),
    Deferred(
        Arc<Compiled<Option<PipelineSource>>>,
        Option<Arc<OnceLock<CompileScheduler>>>,
    ),
}

/// Resolution of a pipeline lookup in [`PsoMemo::lookup`].
enum ResolvedPso<'a> {
    Ready(&'a PsoMemoEntry),
    Pending {
        entry: Arc<Compiled<Option<PipelineSource>>>,
        used_mask: u16,
        bindings: [ActiveBinding; MAX_STREAMS as usize],
        binding_count: u8,
    },
}

fn active_bindings_from_source(
    source: &PipelineSource,
) -> (u16, [ActiveBinding; MAX_STREAMS as usize], u8) {
    let used_mask = (0_u16..)
        .zip(&source.streams)
        .filter(|(_, layout)| layout.is_used())
        .fold(0_u16, |mask, (stream, _)| mask | (1 << stream));
    let mut bindings = [const { ActiveBinding::EMPTY }; MAX_STREAMS as usize];
    let mut binding_count = 0_u8;
    for ((slot, layout), &(stream, advance)) in (0_u8..).zip(&source.layouts).zip(&source.bindings)
    {
        if !layout.is_used() {
            continue;
        }
        let stride = source
            .streams
            .get(usize::from(stream))
            .map_or(0, |s: &StreamLayout| s.stride);
        bindings[usize::from(binding_count)] = ActiveBinding {
            slot,
            stream,
            advance,
            stride,
        };
        binding_count += 1;
    }
    (used_mask, bindings, binding_count)
}

/// Per-device 24-entry MRU cache over resolved pipelines.
///
/// Matches by front-end `key_gen` at index 0 in O(1) without hashing, falling back to
/// `(key_hash, key)` across the MRU entries before consulting [`PsoMap`]. Refused, pending, or
/// failed pipelines are never cached here.
#[derive(Default)]
pub(super) struct PsoMemo {
    entries: Vec<PsoMemoEntry>,
}

impl PsoMemo {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    fn lookup(
        &mut self,
        key: &PipelineKey,
        key_gen: u64,
        ctx: &Arc<CompileContext>,
        pipelines: &PsoMap<PipelineKey, Option<PipelineSource>>,
        scheduler: Option<&Arc<OnceLock<CompileScheduler>>>,
    ) -> Result<ResolvedPso<'_>, DrawRefused> {
        if key_gen != 0
            && let Some(front) = self.entries.first()
            && front.key_gen == key_gen
        {
            debug_assert_eq!(&front.key, key);
            return Ok(ResolvedPso::Ready(&self.entries[0]));
        }
        let key_hash = hash_pipeline_key(key);
        if let Some(idx) = self
            .entries
            .iter()
            .position(|entry| entry.key_hash == key_hash && entry.key == *key)
        {
            if key_gen != 0 {
                self.entries[idx].key_gen = key_gen;
            }
            self.entries[..=idx].rotate_right(1);
            return Ok(ResolvedPso::Ready(&self.entries[0]));
        }
        let entry = pipelines
            .get(key)
            .unwrap_or_else(|| insert_pipeline(ctx, pipelines, scheduler, key));
        if entry.spec.as_ref().as_ref().is_some_and(refused_layout) {
            return Err(DrawRefused::Fetch);
        }
        let Some(source) = entry.spec.as_ref().as_ref() else {
            return Err(DrawRefused::Pipeline);
        };
        let (used_mask, bindings, binding_count) = active_bindings_from_source(source);
        let pso = match entry.try_ready() {
            CompileState::Ready(h) => h,
            CompileState::Failed => return Err(DrawRefused::Pipeline),
            CompileState::Pending => {
                if scheduler.is_some() {
                    return Ok(ResolvedPso::Pending {
                        entry,
                        used_mask,
                        bindings,
                        binding_count,
                    });
                }
                entry.ready().ok_or(DrawRefused::Pipeline)?
            }
        };
        if self.entries.len() == PSO_MEMO_CAP {
            self.entries.pop();
        }
        self.entries.insert(
            0,
            PsoMemoEntry {
                key_gen,
                key_hash,
                key: key.clone(),
                _entry: entry,
                pso,
                used_mask,
                bindings,
                binding_count,
            },
        );
        Ok(ResolvedPso::Ready(&self.entries[0]))
    }

    #[cfg(test)]
    pub(super) const fn len(&self) -> usize {
        self.entries.len()
    }
}

fn hash_pipeline_key(key: &PipelineKey) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Chunk-arena slice copied on the API thread and read once by the encode thread.
struct ChunkSlice {
    ptr: NonNull<u8>,
    len: usize,
}

// SAFETY: allocated in the queue's current chunk via `Chunk::allocate_cpu` on the recording
// thread and read only by the closure inside that same chunk on the encode thread before the
// chunk is retired and reset.
unsafe impl Send for ChunkSlice {}

impl ChunkSlice {
    /// # Safety
    ///
    /// The chunk that allocated this slice must still be live and un-retired.
    const unsafe fn as_slice(&self) -> &[u8] {
        // SAFETY: `self.ptr` points to `self.len` initialized bytes in the live chunk's arena.
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

/// Validated index feed before emission.
enum PendingIndex<'a> {
    Bytes(&'a [u8]),
    Stored(Arc<Buffer>, u64),
}

/// Un-deduplicated state and draw payload for [`CoreDevice::emit_general`].
struct GeneralEmit {
    pso: PsoEmit,
    dsso: Option<(u64, u8)>,
    blend: Option<([f32; 4], u8)>,
    viewport: Option<WMTViewport>,
    scissor: Option<WMTScissorRect>,
    rasterizer: Option<WmtcmdRenderSetrasterizerstate>,
    stored: Vec<(u8, Arc<Buffer>, u64)>,
    bytes: Vec<u8>,
    binds: Vec<(Stage, u8, Range<usize>)>,
    textures: Vec<(Stage, u8, Arc<Texture>, TextureViewKey)>,
    samplers: Vec<(Stage, u8, Option<Arc<dxmt_core::sampler::Sampler>>)>,
    indexed: Option<(IndexFeed, u8, u32)>,
    primitive_type: u8,
    vertex_start: u32,
    vertex_count: u32,
    instances: u32,
    base_vertex: i32,
}

/// Where a recorded indexed draw reads its indices.
enum IndexFeed {
    /// Bytes the closure copies into the argument-buffer ring.
    Ring(Vec<u8>),
    /// A stored buffer, from a byte offset.
    Stored(Arc<Buffer>, u64),
}

impl CoreDevice {
    /// Records `draw` with the pipeline of `key` into the render pass of the targets it names.
    ///
    /// # Errors
    ///
    /// Nothing recorded when colour target 0 does not resolve (a device with no back buffer),
    /// the key names colour formats the bound targets do not make, another sample count than
    /// target 0's, or a depth format the bound depth-stencil has not or one of another count,
    /// no binding can fetch the layout, the pipeline does not build, a stored buffer it reads is
    /// not in the store, or a slot advanced past a crossing attribute's offset starts past its
    /// buffer. A draw whose scissor holds no pixel of the pass records nothing and is no error.
    pub fn draw(&mut self, key: &PipelineKey, draw: &DrawData<'_>) -> Result<(), DrawRefused> {
        // Phase 1: pure validation with zero `PassBound` reads or writes.
        let Some(target) = self.shape(self.target) else {
            return Err(DrawRefused::NoBackBuffer);
        };
        // A pass and its pipeline share one sample count, target 0's, whatever pass is open: an
        // open pass on the same colours does not excuse another count (DXMTRS C4b-3d5).
        if key.samples != target.samples {
            return Err(DrawRefused::Target);
        }
        // The key decides the pass's colour slots and formats. An open pass on them was checked
        // when it opened, and a change of any target closed it; otherwise the pass set must make
        // them, under one sRGB write or the other.
        let colors = key.color_formats();
        let open = self.render_open && self.open_colors == colors;
        if !open
            && [false, true]
                .into_iter()
                .all(|srgb| self.target_desc(srgb).map(|(_, formats)| formats) != Some(colors))
        {
            return Err(DrawRefused::Target);
        }
        // The key decides the pass: with the depth-stencil exactly when it names a format. An
        // open pass with it was checked when it opened, and a change of either binding closed it.
        let with_depth = key.depth_format != 0;
        let bound = || self.depth_format() == Some(key.depth_format);
        if with_depth && self.open_depth.is_none() && !bound() {
            return Err(DrawRefused::Depth);
        }
        // A depth-stencil of another count than target 0 is left out of the pass: a key that
        // names it is not the draw's (DXMTRS C4b-3d5).
        if with_depth && self.depth_samples() != Some(target.samples) {
            return Err(DrawRefused::Depth);
        }
        debug_assert!(
            !with_depth || bound(),
            "a draw's key names the bound depth-stencil's format"
        );
        // Metal takes no scissor past the pass's extent, nor an empty one: a draw cut to nothing
        // writes no pixel, so it records nothing.
        let extent = self
            .pass_extent(with_depth)
            .unwrap_or((target.width, target.height));
        let Some(scissor) = clamp_rect(draw.scissor, extent) else {
            return Ok(());
        };
        let (
            pending_entry,
            pso_ready,
            used,
            mut stored,
            stored_len,
            mut stream_bytes,
            stream_bytes_len,
            mut dyn_ids,
            mut dyn_count,
        ) = {
            struct PendingStored {
                slot: u8,
                id: BufferId,
                generation: u64,
                buffer: Arc<Buffer>,
                offset: u64,
            }
            struct PendingBytes<'a> {
                slot: u8,
                fed: Cow<'a, [u8]>,
                unconditional: bool,
            }
            let resolved = self.pso_memo.lookup(
                key,
                draw.key_gen,
                &self.compile_ctx,
                &self.pipelines,
                self.scheduler.as_ref(),
            )?;
            let (pending_entry, pso_ready, used, active_slice) = match &resolved {
                ResolvedPso::Ready(entry) => {
                    (None, entry.pso, entry.used_mask, entry.active_bindings())
                }
                ResolvedPso::Pending {
                    entry,
                    used_mask,
                    bindings,
                    binding_count,
                } => (
                    Some(Arc::clone(entry)),
                    0,
                    *used_mask,
                    &bindings[..usize::from(*binding_count)],
                ),
            };
            let mut stored: [Option<PendingStored>; MAX_STREAMS as usize] =
                [const { None }; MAX_STREAMS as usize];
            let mut stored_len = 0_usize;
            let mut stream_bytes: [Option<PendingBytes<'_>>; MAX_STREAMS as usize] =
                [const { None }; MAX_STREAMS as usize];
            let mut stream_bytes_len = 0_usize;
            let mut dyn_ids = [BufferId::from_raw(0); MAX_STREAMS as usize + 1];
            let mut dyn_count = 0_usize;
            for binding in active_slice {
                match draw.streams[usize::from(binding.stream)] {
                    StreamData::Buffer { id, offset } => {
                        let (buffer, generation, is_dynamic) =
                            self.buffers.buffer_entry(id).ok_or(DrawRefused::Buffer)?;
                        debug_assert!(
                            matches!(
                                key.streams[usize::from(binding.stream)],
                                StreamKey::Bound { shift: s, .. }
                                    if offset_shift(offset) == s.into()
                            ),
                            "a bound stream's key carries its offset's shift"
                        );
                        if binding.advance != 0
                            && advanced_binding_offset(offset, binding.advance, buffer.length())
                                .is_err()
                        {
                            return Err(DrawRefused::Fetch);
                        }
                        let at = slot_binding_offset(offset, binding.advance);
                        if is_dynamic {
                            dyn_ids[dyn_count] = id;
                            dyn_count += 1;
                        }
                        stored[stored_len] = Some(PendingStored {
                            slot: binding.slot,
                            id,
                            generation,
                            buffer: Arc::clone(buffer),
                            offset: u64::from(at),
                        });
                        stored_len += 1;
                    }
                    StreamData::Bytes(fed) => {
                        stream_bytes[stream_bytes_len] = Some(PendingBytes {
                            slot: binding.slot,
                            fed: slot_bytes(fed, binding.advance),
                            unconditional: true,
                        });
                        stream_bytes_len += 1;
                    }
                    data @ (StreamData::Zeros | StreamData::None) => {
                        debug_assert!(
                            matches!(data, StreamData::Zeros),
                            "a stream the pipeline reads has data"
                        );
                        debug_assert!(
                            binding.advance == 0,
                            "a stream with nothing bound never crosses"
                        );
                        let stride = binding.stride as usize;
                        stream_bytes[stream_bytes_len] = Some(PendingBytes {
                            slot: binding.slot,
                            fed: Cow::Borrowed(
                                &NULL_STREAM_ZEROS[..stride.min(NULL_STREAM_ZEROS.len())],
                            ),
                            unconditional: false,
                        });
                        stream_bytes_len += 1;
                    }
                }
            }
            (
                pending_entry,
                pso_ready,
                used,
                stored,
                stored_len,
                stream_bytes,
                stream_bytes_len,
                dyn_ids,
                dyn_count,
            )
        };
        let indexed = match &draw.indices {
            None => None,
            Some(Indices {
                data,
                index_type,
                count,
            }) => {
                let feed = match *data {
                    IndexData::Bytes(bytes) => PendingIndex::Bytes(bytes),
                    IndexData::Buffer { id, offset } => {
                        let (buffer, _, is_dynamic) =
                            self.buffers.buffer_entry(id).ok_or(DrawRefused::Buffer)?;
                        if is_dynamic {
                            dyn_ids[dyn_count] = id;
                            dyn_count += 1;
                        }
                        PendingIndex::Stored(Arc::clone(buffer), offset)
                    }
                };
                Some((feed, *index_type, *count))
            }
        };
        // mtld3d's instance count: stream 0's, when a stream the pipeline reads is per instance.
        let instances = draw.instancing.map_or(1, |(word, instanced)| {
            instance_count(word, instanced & used != 0)
        });

        let depth_planes = if with_depth {
            super::depth::planes(key.depth_format)
        } else {
            0
        };
        if self.open_brackets.is_some() {
            self.record_open_query_areas();
        }
        if let Some(pending) = pending_entry.as_ref() {
            let mut check_planes = ClearPlanes::COLOR;
            if with_depth && (depth_planes & 1 != 0) && draw.depth_stencil.uses_depth() {
                check_planes.insert(ClearPlanes::DEPTH);
            }
            if with_depth && (depth_planes & 2 != 0) && draw.depth_stencil.stencil_enable != 0 {
                check_planes.insert(ClearPlanes::STENCIL);
            }
            let skippable = self.options.contains(CoreOptions::ASYNC_COMPILE)
                && self.open_brackets.is_none_or(|n| n == 0)
                && self.targets_rebuilt(colors, check_planes);
            if skippable {
                self.compile_ctx
                    .stats
                    .draws_skipped
                    .fetch_add(1, Ordering::Relaxed);
                if !self.options.contains(CoreOptions::LOGGED_SKIP) {
                    self.options.insert(CoreOptions::LOGGED_SKIP);
                    log::log!(
                        target: LOG_TARGET,
                        super::gate_level(),
                        "d3d9 core: leaving draws out of their frame while their shader or \
                         pipeline builds (shader.asyncCompile); they appear once the build lands"
                    );
                }
                return Ok(());
            }
            if let Some(sched) = self.scheduler.as_deref().and_then(OnceLock::get)
                && let Some(ticket) = pending.ticket()
            {
                let _ = sched.promote(ticket);
            }
        }

        // Phase 2: open or reuse the render pass (`open_pass` resets `pass_bound` on a new pass).
        self.open_pass(with_depth, colors);
        if self.options.contains(CoreOptions::ASYNC_COMPILE)
            && with_depth
            && (depth_planes & 2 != 0)
            && draw.depth_stencil.writes_stencil()
        {
            self.pass_reads.mark_writes_stencil();
        }

        // Phase 3: diff `PassBound`, emit compact fast paths or general draw closure, stamp dynamic.
        let is_deferred = pending_entry.is_some();
        let fast_state = !is_deferred
            && draw.key_gen != 0
            && self.render_open
            && self.pass_bound.last_draw_gen == draw.key_gen
            && self.pass_bound.pso != 0;

        let mut pso_emit = PsoEmit::None;
        let mut dsso_emit = None;
        let mut blend_emit = None;
        let mut viewport_emit = None;
        let mut scissor_emit = None;
        let mut rasterizer_emit = None;
        let mut const_binds_emit: Vec<(Stage, u8, Range<usize>)> = Vec::new();
        let mut bytes: Vec<u8> = Vec::new();
        let mut textures_emit: Vec<(Stage, u8, Arc<Texture>, TextureViewKey)> = Vec::new();
        let mut samplers_emit: Vec<(Stage, u8, Option<Arc<dxmt_core::sampler::Sampler>>)> =
            Vec::new();

        if !fast_state {
            if let Some(compiled) = pending_entry {
                pso_emit = PsoEmit::Deferred(compiled, self.scheduler.clone());
            } else if self.pass_bound.pso != pso_ready {
                self.pass_bound.pso = pso_ready;
                pso_emit = PsoEmit::Handle(pso_ready);
            }
            if with_depth {
                let has_stencil = depth_planes & 2 != 0;
                let gated = draw.depth_stencil.gated_on_stencil_attachment(has_stencil);
                let dsso = self.depth_state_handle(&gated);
                let next = (dsso, draw.stencil_ref);
                if self.pass_bound.dsso != Some(next) {
                    self.pass_bound.dsso = Some(next);
                    dsso_emit = Some(next);
                }
            }
            if let Some(rgba) = draw.blend_color {
                let bits = rgba.map(f32::to_bits);
                if self.pass_bound.blend_color != Some(bits) {
                    self.pass_bound.blend_color = Some(bits);
                    blend_emit = Some((rgba, draw.stencil_ref));
                }
            }
            let vp_bits = draw.viewport.map(f64::to_bits);
            if self.pass_bound.viewport != Some(vp_bits) {
                self.pass_bound.viewport = Some(vp_bits);
                let [origin_x, origin_y, width, height, znear, zfar] = draw.viewport;
                viewport_emit = Some(WMTViewport {
                    origin_x,
                    origin_y,
                    width,
                    height,
                    znear,
                    zfar,
                });
            }
            if self.pass_bound.scissor != Some(scissor) {
                self.pass_bound.scissor = Some(scissor);
                scissor_emit = Some(WMTScissorRect {
                    x: u64::from(scissor[0]),
                    y: u64::from(scissor[1]),
                    width: u64::from(scissor[2]),
                    height: u64::from(scissor[3]),
                });
            }
            let rast_key = (draw.fill_mode, draw.cull_mode, draw.slope_scale.to_bits());
            if self.pass_bound.rasterizer != Some(rast_key) {
                self.pass_bound.rasterizer = Some(rast_key);
                rasterizer_emit = Some(WmtcmdRenderSetrasterizerstate {
                    r#type: WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    fill_mode: draw.fill_mode,
                    cull_mode: draw.cull_mode,
                    depth_clip_mode: 0,
                    winding: 0,
                    depth_bias: 0.0,
                    scole_scale: draw.slope_scale,
                    depth_bias_clamp: 0.0,
                });
            }
            for bind in draw.binds.iter().filter(|b| !b.bytes.is_empty()) {
                if self
                    .pass_bound
                    .should_bind_bytes(bind.stage, bind.slot, bind.bytes)
                {
                    let start = bytes.len();
                    bytes.extend_from_slice(bind.bytes);
                    const_binds_emit.push((bind.stage, bind.slot, start..bytes.len()));
                }
            }
            for t in draw.textures {
                if self
                    .pass_bound
                    .should_bind_texture(t.stage, t.slot, &t.texture, t.key)
                {
                    textures_emit.push((t.stage, t.slot, Arc::clone(&t.texture), t.key));
                    if self.options.contains(CoreOptions::ASYNC_COMPILE)
                        && let Some(id) = t.id
                    {
                        self.pass_reads.push_read(ClearTarget::Texture(id));
                    }
                }
                if self
                    .pass_bound
                    .should_bind_sampler(t.stage, t.slot, t.sampler.as_ref())
                {
                    samplers_emit.push((t.stage, t.slot, t.sampler.clone()));
                }
            }
            self.pass_bound.last_draw_gen = draw.key_gen;
        }
        if is_deferred {
            self.pass_bound.pso = 0;
            self.pass_bound.last_draw_gen = 0;
        }

        let mut stored_emit: [Option<(u8, Arc<Buffer>, u64)>; MAX_STREAMS as usize] =
            [const { None }; MAX_STREAMS as usize];
        let mut stored_emit_len = 0_usize;
        for entry in &mut stored[..stored_len] {
            if let Some(item) = entry.take()
                && self.pass_bound.should_bind_vertex_buffer(
                    item.slot,
                    item.id,
                    item.generation,
                    item.offset,
                )
            {
                stored_emit[stored_emit_len] = Some((item.slot, item.buffer, item.offset));
                stored_emit_len += 1;
            }
        }

        let mut stream_bytes_emit: [Option<(u8, Cow<'_, [u8]>)>; MAX_STREAMS as usize] =
            [const { None }; MAX_STREAMS as usize];
        let mut stream_bytes_emit_len = 0_usize;
        let mut stream_bytes_single_unconditional = false;
        for entry in &mut stream_bytes[..stream_bytes_len] {
            if let Some(item) = entry.take()
                && self.pass_bound.should_bind_stream_bytes(
                    item.slot,
                    &item.fed,
                    item.unconditional,
                )
            {
                stream_bytes_single_unconditional = item.unconditional;
                stream_bytes_emit[stream_bytes_emit_len] = Some((item.slot, item.fed));
                stream_bytes_emit_len += 1;
            }
        }

        let (primitive_type, vertex_count) = (draw.primitive, draw.vertex_count);
        let (vertex_start, base_vertex) = (draw.vertex_start, draw.base_vertex);
        let no_state_changes = matches!(pso_emit, PsoEmit::None)
            && dsso_emit.is_none()
            && blend_emit.is_none()
            && viewport_emit.is_none()
            && scissor_emit.is_none()
            && rasterizer_emit.is_none()
            && const_binds_emit.is_empty()
            && textures_emit.is_empty()
            && samplers_emit.is_empty();

        if no_state_changes && stored_emit_len == 0 && stream_bytes_emit_len == 0 {
            match indexed {
                None => {
                    // Closure A: Copy non-indexed draw with zero state or buffer changes.
                    self.emit(move |enc| {
                        enc.bump_visibility_result_offset();
                        enc.encode_render_command(WmtcmdRenderDraw {
                            r#type: WMT_RENDER_COMMAND_DRAW,
                            reserved: [0; 3],
                            next: NO_NEXT,
                            primitive_type,
                            vertex_start: u64::from(vertex_start),
                            vertex_count: u64::from(vertex_count),
                            instance_count: instances,
                            base_instance: 0,
                        });
                    });
                }
                Some((PendingIndex::Stored(index_buf, index_offset), index_type, index_count)) => {
                    // Closure B: zero-Vec stored indexed draw with zero state/VB changes.
                    self.emit(move |enc| {
                        let Some((allocation, at)) =
                            enc.access_buffer::<{ stage::VERTEX }>(&index_buf, Access::READ)
                        else {
                            return;
                        };
                        enc.bump_visibility_result_offset();
                        enc.encode_render_command(WmtcmdRenderDrawIndexed {
                            r#type: WMT_RENDER_COMMAND_DRAW_INDEXED,
                            reserved: [0; 3],
                            next: NO_NEXT,
                            primitive_type,
                            index_type,
                            index_count: u64::from(index_count),
                            index_buffer: allocation.buffer(),
                            index_buffer_offset: at + index_offset,
                            instance_count: instances,
                            base_vertex,
                            base_instance: 0,
                        });
                    });
                }
                Some((PendingIndex::Bytes(index_bytes), index_type, index_count)) => {
                    self.emit_general(GeneralEmit {
                        pso: pso_emit,
                        dsso: dsso_emit,
                        blend: blend_emit,
                        viewport: viewport_emit,
                        scissor: scissor_emit,
                        rasterizer: rasterizer_emit,
                        stored: Vec::new(),
                        bytes,
                        binds: const_binds_emit,
                        textures: textures_emit,
                        samplers: samplers_emit,
                        indexed: Some((
                            IndexFeed::Ring(index_bytes.to_vec()),
                            index_type,
                            index_count,
                        )),
                        primitive_type,
                        vertex_start,
                        vertex_count,
                        instances,
                        base_vertex,
                    });
                }
            }
        } else if no_state_changes
            && stored_emit_len == 0
            && indexed.is_none()
            && stream_bytes_emit_len == 1
            && stream_bytes_single_unconditional
            && let Some((slot, fed)) = stream_bytes_emit[0].take()
        {
            // Closure C: zero-heap-allocation single inline UP vertex draw.
            let dst = self.device.queue_mut().chunk().allocate_cpu(fed.len(), 16);
            if !fed.is_empty() {
                // SAFETY: `dst` holds `fed.len()` writable bytes allocated in the current chunk
                // and is disjoint from `fed`.
                unsafe {
                    dst.as_ptr()
                        .copy_from_nonoverlapping(fed.as_ptr(), fed.len());
                }
            }
            let chunk_slice = ChunkSlice {
                ptr: dst,
                len: fed.len(),
            };
            self.emit(move |enc| {
                // SAFETY: `chunk_slice` lives in this chunk's arena until the chunk retires.
                let bound = unsafe { chunk_slice.as_slice() };
                direct::bind_bytes::<{ stage::VERTEX }>(enc, slot, bound);
                enc.bump_visibility_result_offset();
                enc.encode_render_command(WmtcmdRenderDraw {
                    r#type: WMT_RENDER_COMMAND_DRAW,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    primitive_type,
                    vertex_start: u64::from(vertex_start),
                    vertex_count: u64::from(vertex_count),
                    instance_count: instances,
                    base_instance: 0,
                });
            });
        } else {
            let mut stored_vec = Vec::with_capacity(stored_emit_len);
            for slot_entry in &mut stored_emit[..stored_emit_len] {
                if let Some(item) = slot_entry.take() {
                    stored_vec.push(item);
                }
            }
            let mut binds_emit = const_binds_emit;
            for (slot, fed) in stream_bytes_emit[..stream_bytes_emit_len].iter().flatten() {
                let start = bytes.len();
                bytes.extend_from_slice(fed);
                binds_emit.push((Stage::Vertex, *slot, start..bytes.len()));
            }
            let indexed_feed = indexed.map(|(feed, index_type, index_count)| {
                let resolved = match feed {
                    PendingIndex::Bytes(index_bytes) => IndexFeed::Ring(index_bytes.to_vec()),
                    PendingIndex::Stored(buffer, offset) => IndexFeed::Stored(buffer, offset),
                };
                (resolved, index_type, index_count)
            });
            self.emit_general(GeneralEmit {
                pso: pso_emit,
                dsso: dsso_emit,
                blend: blend_emit,
                viewport: viewport_emit,
                scissor: scissor_emit,
                rasterizer: rasterizer_emit,
                stored: stored_vec,
                bytes,
                binds: binds_emit,
                textures: textures_emit,
                samplers: samplers_emit,
                indexed: indexed_feed,
                primitive_type,
                vertex_start,
                vertex_count,
                instances,
                base_vertex,
            });
        }

        // After the record: a lock that renames from here on leaves this draw its name.
        if dyn_count != 0 {
            self.stamp_dynamic_ids(&dyn_ids[..dyn_count]);
        }
        Ok(())
    }

    fn emit_general(&mut self, general: GeneralEmit) {
        let GeneralEmit {
            pso: pso_emit,
            dsso: dsso_emit,
            blend: blend_emit,
            viewport: viewport_emit,
            scissor: scissor_emit,
            rasterizer: rasterizer_emit,
            stored,
            bytes,
            binds: binds_emit,
            textures: textures_emit,
            samplers: samplers_emit,
            indexed,
            primitive_type,
            vertex_start,
            vertex_count,
            instances,
            base_vertex,
        } = general;
        self.emit(move |enc| {
            let (is_deferred, pso_handle) = match &pso_emit {
                PsoEmit::None => (false, None),
                PsoEmit::Handle(h) => (false, Some(*h)),
                PsoEmit::Deferred(compiled, sched) => {
                    let h = compiled.ready_or_steal(sched.as_deref().and_then(OnceLock::get));
                    (true, h)
                }
            };
            if let Some(pso) = pso_handle {
                enc.encode_render_command(WmtcmdRenderSetpso {
                    r#type: WMT_RENDER_COMMAND_SET_PSO,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    pso,
                });
            }
            if let Some((dsso, stencil_ref)) = dsso_emit {
                enc.encode_render_command(WmtcmdRenderSetdsso {
                    r#type: WMT_RENDER_COMMAND_SET_DSSO,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    dsso,
                    stencil_ref,
                });
            }
            if let Some(([red, green, blue, alpha], stencil_ref)) = blend_emit {
                enc.encode_render_command(WmtcmdRenderSetblendcolor {
                    r#type: WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    red,
                    green,
                    blue,
                    alpha,
                    stencil_ref,
                });
            }
            if let Some(viewport) = viewport_emit {
                let slot = enc
                    .allocate_cpu(size_of::<WMTViewport>(), align_of::<WMTViewport>())
                    .cast::<WMTViewport>();
                // SAFETY: the encoder's memory, sized and aligned for one viewport, nobody else's.
                unsafe { slot.write(viewport) };
                enc.encode_render_command(WmtcmdRenderSetviewports {
                    r#type: WMT_RENDER_COMMAND_SET_VIEWPORTS,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    viewports: MemPtr::from_ptr(slot.as_ptr().cast::<u8>()),
                    viewport_count: 1,
                });
            }
            if let Some(rect) = scissor_emit {
                let slot = enc
                    .allocate_cpu(size_of::<WMTScissorRect>(), align_of::<WMTScissorRect>())
                    .cast::<WMTScissorRect>();
                // SAFETY: the encoder's memory, sized and aligned for one rect, nobody else's.
                unsafe { slot.write(rect) };
                enc.encode_render_command(WmtcmdRenderSetscissorrects {
                    r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    scissor_rects: MemPtr::from_ptr(slot.as_ptr().cast::<u8>()),
                    rect_count: 1,
                });
            }
            if let Some(rasterizer) = rasterizer_emit {
                enc.encode_render_command(rasterizer);
            }
            for (slot, buffer, offset) in &stored {
                direct::bind_buffer::<{ stage::VERTEX }>(enc, *slot, Some(buffer), *offset);
            }
            for (stage, slot, range) in &binds_emit {
                let bound = &bytes[range.clone()];
                match stage {
                    Stage::Vertex => direct::bind_bytes::<{ stage::VERTEX }>(enc, *slot, bound),
                    Stage::Pixel => direct::bind_bytes::<{ stage::PIXEL }>(enc, *slot, bound),
                }
            }
            // The plain sampler state, not the cube twin: mtld3d's cube samples clamp in the
            // shader as its own binds do (C4b3b-5).
            for (stage, slot, texture, key) in &textures_emit {
                let view = Some((&**texture, *key));
                match stage {
                    Stage::Vertex => direct::bind_texture::<{ stage::VERTEX }>(enc, *slot, view),
                    Stage::Pixel => direct::bind_texture::<{ stage::PIXEL }>(enc, *slot, view),
                }
            }
            for (stage, slot, sampler) in &samplers_emit {
                let sampler = sampler.as_ref();
                match stage {
                    Stage::Vertex => {
                        direct::bind_sampler::<{ stage::VERTEX }>(enc, *slot, sampler, false);
                    }
                    Stage::Pixel => {
                        direct::bind_sampler::<{ stage::PIXEL }>(enc, *slot, sampler, false);
                    }
                }
            }
            if !textures_emit.is_empty() {
                // A texture read in a pass that wrote it waits on a tile barrier.
                enc.resolve_render_pass_barrier(dxmt_core::encoder::Encoder::dispatch_tile_barrier);
            }
            if is_deferred && pso_handle.is_none() {
                return;
            }
            // An application's draw counts into the occlusion queries active; no pass of the
            // device's own bumps (DXMTRS C4c-5).
            let Some((feed, index_type, index_count)) = &indexed else {
                enc.bump_visibility_result_offset();
                enc.encode_render_command(WmtcmdRenderDraw {
                    r#type: WMT_RENDER_COMMAND_DRAW,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    primitive_type,
                    vertex_start: u64::from(vertex_start),
                    vertex_count: u64::from(vertex_count),
                    instance_count: instances,
                    base_instance: 0,
                });
                return;
            };
            let (index_buffer, index_buffer_offset) = match feed {
                IndexFeed::Ring(index_bytes) => {
                    let ring = enc.allocate_argument_buffer(index_bytes.len());
                    // SAFETY: the slice's mapping is `index_bytes.len()` bytes the pass alone
                    // uses.
                    unsafe {
                        ring.mapping()
                            .as_ptr()
                            .copy_from_nonoverlapping(index_bytes.as_ptr(), index_bytes.len());
                    }
                    (ring.buffer(), ring.offset())
                }
                IndexFeed::Stored(buffer, offset) => {
                    // A stored buffer has its allocation from its creation on.
                    let Some((allocation, at)) =
                        enc.access_buffer::<{ stage::VERTEX }>(buffer, Access::READ)
                    else {
                        return;
                    };
                    (allocation.buffer(), at + offset)
                }
            };
            enc.bump_visibility_result_offset();
            enc.encode_render_command(WmtcmdRenderDrawIndexed {
                r#type: WMT_RENDER_COMMAND_DRAW_INDEXED,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type,
                index_type: *index_type,
                index_count: u64::from(*index_count),
                index_buffer,
                index_buffer_offset,
                instance_count: instances,
                base_vertex,
                base_instance: 0,
            });
        });
    }
}

fn log_pipeline_failure(key: &PipelineKey) {
    let streams = key
        .streams
        .iter()
        .filter(|s| **s != StreamKey::Unused)
        .collect::<Vec<_>>();
    log::error!(
        target: LOG_TARGET,
        "d3d9 core: a pipeline (streams {streams:?}) does not build; its draws are dropped"
    );
    log::debug!(
        target: LOG_TARGET,
        "d3d9 core: the pipeline that does not build: {key:?}"
    );
}

/// The entry of a key the device has not seen: built synchronously or queued on `scheduler`.
///
/// One that does not build is logged and kept as handle 0, never built again; one no binding
/// can fetch is kept as handle 0 too, its fetch error logged.
fn insert_pipeline(
    ctx: &Arc<CompileContext>,
    pipelines: &PsoMap<PipelineKey, Option<PipelineSource>>,
    scheduler: Option<&Arc<OnceLock<CompileScheduler>>>,
    key: &PipelineKey,
) -> Arc<Compiled<Option<PipelineSource>>> {
    let source = pipeline_source(key)
        .map_err(|e| {
            log::debug!(target: LOG_TARGET, "d3d9 core: a vertex stage: {e:?}");
        })
        .ok();
    if let Some(Err(e)) = source.as_ref().map(|source| &source.fetch) {
        log::debug!(
            target: LOG_TARGET,
            "d3d9 core: no binding fetches a vertex layout ({e:?}); its draws are refused"
        );
    }
    let refused = source.as_ref().is_some_and(refused_layout);
    // A stream with nothing bound reads the zero feed, which its draws clamp to (as mtld3d's
    // do): said once a key.
    let zeros = NULL_STREAM_ZEROS.len();
    for (stream, (entry, layout)) in source
        .iter()
        .flat_map(|source| key.streams.iter().zip(&source.streams))
        .enumerate()
    {
        if *entry == StreamKey::Null && layout.stride as usize > zeros {
            log::warn!(
                target: LOG_TARGET,
                "d3d9 core: stream {stream} is unbound and its declaration extent {} exceeds \
                 the zero feed; clamped",
                layout.stride
            );
        }
    }
    let Some(src_ref) = source.as_ref().filter(|_| !refused) else {
        let entry = Compiled::ready_with(Arc::new(source), 0, None);
        return pipelines.insert_ready(key.clone(), entry);
    };

    if let Some(sched_lock) = scheduler {
        let lane = if ctx.functions.is_done(&src_ref.vs) && ctx.functions.is_done(&src_ref.ps) {
            CompileLane::Pipeline
        } else {
            CompileLane::Normal
        };
        let ctx_cl = Arc::clone(ctx);
        let key_cl = key.clone();
        let entry = Compiled::pending(Arc::new(source), move |spec| {
            let src = spec.as_ref()?;
            let pso = build_pipeline(&ctx_cl, src);
            if pso.is_some() {
                ctx_cl.stats.async_compiled.fetch_add(1, Ordering::Relaxed);
                lock(&ctx_cl.recipes).record(&key_cl);
            } else {
                log_pipeline_failure(&key_cl);
            }
            pso
        });
        return pipelines.insert_or_existing_lane(
            key.clone(),
            entry,
            sched_lock,
            || CompileScheduler::with_bounds(2, 4),
            lane,
        );
    }

    let pso = build_pipeline(ctx, src_ref);
    if pso.is_some() {
        lock(&ctx.recipes).record(key);
    } else {
        log_pipeline_failure(key);
    }
    let handle = pso.as_ref().map_or(0, Obj::handle);
    let entry = Compiled::ready_with(Arc::new(source), handle, pso);
    pipelines.insert_ready(key.clone(), entry)
}

impl CoreDevice {
    /// The stage compiles so far.
    #[cfg(test)]
    pub fn compiles(&self) -> usize {
        self.compile_ctx.functions.stage_compiles()
    }

    /// The pipelines kept, built or not.
    #[cfg(test)]
    pub fn pipelines_len(&self) -> usize {
        self.pipelines.len()
    }
}

/// Whether the core refuses `source`'s layout: no binding can fetch it.
const fn refused_layout(source: &PipelineSource) -> bool {
    !source.fetchable()
}

/// The bytes an inline stream binds at a slot advanced `advance` bytes past it.
///
/// `fed` from byte `advance` on, zero-filled to `fed`'s length (all zeros from the end on); `fed`
/// itself at no advance. Vertex `i` of a slot reads `i * stride` bytes in, and an attribute fits
/// its stride, so the slot reads the bytes mtld3d's payload holds from the advance: the stream's,
/// then the zeros it pads past the last vertex.
fn slot_bytes(fed: &[u8], advance: u32) -> Cow<'_, [u8]> {
    if advance == 0 {
        return Cow::Borrowed(fed);
    }
    let read = usize::try_from(advance)
        .ok()
        .and_then(|advance| fed.get(advance..))
        .unwrap_or_default();
    let mut bytes = read.to_vec();
    bytes.resize(fed.len(), 0);
    Cow::Owned(bytes)
}

/// The pipeline `source` describes, its stages from `ctx.functions` (compiled there on a miss).
///
/// A free function, so a compile job or prewarm worker can run it (C4d).
pub(super) fn build_pipeline(ctx: &CompileContext, source: &PipelineSource) -> Option<Obj> {
    let _pool = winemetal_sys::Pool::new();
    let vs = ctx
        .functions
        .function(&ctx.mtl_device, ctx.metal_version, &source.vs, c"d3d9_vs")?;
    let ps = ctx
        .functions
        .function(&ctx.mtl_device, ctx.metal_version, &source.ps, c"d3d9_ps")?;
    let mut info = dxmt_core::library::render_pipeline_info();
    info.vertex_function = vs;
    info.fragment_function = ps;
    info.colors[0].pixel_format = source.color_format;
    info.colors[0].write_mask = source.write_mask;
    if let Some(blend) = &source.blend {
        set_blend(&mut info.colors[0], blend);
    }
    // Slots 1-3 at their own index; an empty one keeps the invalid format, as its pass's slot
    // holds no attachment.
    for (color, extra) in info.colors[1..].iter_mut().zip(&source.extras) {
        let Some(extra) = extra else {
            continue;
        };
        color.pixel_format = extra.format;
        color.write_mask = extra.write_mask;
        if let Some(blend) = &extra.blend {
            set_blend(color, blend);
        }
    }
    // The planes of the pass the key draws in: both formats are the combined format's.
    let planes = super::depth::planes(source.depth_format);
    if planes & 1 != 0 {
        info.depth_pixel_format = source.depth_format;
    }
    if planes & 2 != 0 {
        info.stencil_pixel_format = source.depth_format;
    }
    // The pass's count, and coverage from alpha on a multisampled one (DXMTRS C4b-3d5).
    info.raster_sample_count = source.samples;
    info.alpha_to_coverage_enabled = u8::from(source.alpha_to_coverage);
    let mut vertex = WMTVertexDescriptorInfo {
        attributes: [WMTVertexAttributeInfo {
            format: 0,
            offset: 0,
            buffer_index: 0,
        }; 31],
        layouts: [WMTVertexBufferLayoutInfo {
            stride: 0,
            step_function: 0,
            step_rate: 0,
        }; 31],
    };
    for &(index, format, offset, slot) in &source.attrs {
        if let Some(attribute) = vertex.attributes.get_mut(index as usize) {
            *attribute = WMTVertexAttributeInfo {
                format,
                offset,
                buffer_index: slot,
            };
        }
    }
    for (wire, layout) in vertex.layouts.iter_mut().zip(&source.layouts) {
        if layout.is_used() {
            *wire = WMTVertexBufferLayoutInfo {
                stride: layout.stride,
                // mtld3d's step function has Metal's numbers: Constant 0, PerVertex 1,
                // PerInstance 2 (the wire names none).
                step_function: layout.step as u32,
                step_rate: layout.step_rate,
            };
        }
    }
    let mut error = 0;
    // SAFETY: a live device; the info, the vertex descriptor and the error slot are alive for
    // the call; the functions are retained by the pipeline.
    let pso = unsafe {
        MTLDevice_newRenderPipelineStateWithVertexDescriptor(
            ctx.mtl_device.handle(),
            &raw const info,
            &raw const vertex,
            &raw mut error,
        )
    };
    // SAFETY: a new error's handle carries its reference.
    drop(unsafe { Obj::adopt(error) });
    // SAFETY: a new pipeline's handle carries its reference.
    unsafe { Obj::adopt(pso) }
}

/// Turns blending on in `color`, with `blend`'s factors and operations in the wire's numbers.
const fn set_blend(color: &mut WMTColorAttachmentBlendInfo, blend: &CoreBlend) {
    color.blending_enabled = 1;
    color.src_rgb_blend_factor = wire_factor(blend.src);
    color.dst_rgb_blend_factor = wire_factor(blend.dst);
    color.rgb_blend_operation = wire_op(blend.op);
    color.src_alpha_blend_factor = wire_factor(blend.src_alpha);
    color.dst_alpha_blend_factor = wire_factor(blend.dst_alpha);
    color.alpha_blend_operation = wire_op(blend.op_alpha);
}

/// The wire's `WMT_BLEND_FACTOR_*` of mtld3d's blend factor.
///
/// Name by name: the two enums number destination colour and alpha the other way round.
const fn wire_factor(factor: BlendFactor) -> u8 {
    match factor {
        BlendFactor::Zero => WMT_BLEND_FACTOR_ZERO,
        BlendFactor::One => WMT_BLEND_FACTOR_ONE,
        BlendFactor::SourceColor => WMT_BLEND_FACTOR_SOURCE_COLOR,
        BlendFactor::OneMinusSourceColor => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_COLOR,
        BlendFactor::SourceAlpha => WMT_BLEND_FACTOR_SOURCE_ALPHA,
        BlendFactor::OneMinusSourceAlpha => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA,
        BlendFactor::DestinationAlpha => WMT_BLEND_FACTOR_DESTINATION_ALPHA,
        BlendFactor::OneMinusDestinationAlpha => WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_ALPHA,
        BlendFactor::DestinationColor => WMT_BLEND_FACTOR_DESTINATION_COLOR,
        BlendFactor::OneMinusDestinationColor => WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_COLOR,
        BlendFactor::SourceAlphaSaturated => WMT_BLEND_FACTOR_SOURCE_ALPHA_SATURATED,
        BlendFactor::BlendColor => WMT_BLEND_FACTOR_BLEND_COLOR,
        BlendFactor::OneMinusBlendColor => WMT_BLEND_FACTOR_ONE_MINUS_BLEND_COLOR,
    }
}

/// The wire's `WMT_BLEND_OPERATION_*` of mtld3d's blend operation.
const fn wire_op(op: BlendOperation) -> u8 {
    match op {
        BlendOperation::Add => WMT_BLEND_OPERATION_ADD,
        BlendOperation::Subtract => WMT_BLEND_OPERATION_SUBTRACT,
        BlendOperation::ReverseSubtract => WMT_BLEND_OPERATION_REVERSE_SUBTRACT,
        BlendOperation::Min => WMT_BLEND_OPERATION_MIN,
        BlendOperation::Max => WMT_BLEND_OPERATION_MAX,
    }
}

#[cfg(test)]
mod tests;
