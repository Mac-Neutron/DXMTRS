//! The passes of a chunk: started, recorded and ended in the order the front end makes them.
//!
//! A pass is one Metal pass (render, compute or blit) or one step `flush` encodes by itself: a
//! clear, a resolve, a present, an upscale, an event's signal or wait, a timestamp's sample.
//! Each takes an encoder id (a render pass two, its vertex half's first) unless it only orders
//! the others; ending it drops the waits another wait implies and appends it to the chunk's list.
//!
//! Behaviour is the C++ `ArgumentEncodingContext`'s (`dxmt_context.cpp:466-860`).

use std::sync::Arc;

use winemetal_sys::{
    Obj,
    wire::{
        WMT_BARRIER_SCOPE_BUFFERS, WMT_BARRIER_SCOPE_TEXTURES, WMT_BLIT_COMMAND_NOP,
        WMT_COMPUTE_COMMAND_MEMORY_BARRIER, WMT_COMPUTE_COMMAND_NOP,
        WMT_RENDER_COMMAND_MEMORY_BARRIER, WMT_RENDER_COMMAND_NOP, WMT_RENDER_STAGE_FRAGMENT,
        WMT_RENDER_STAGE_PRE_RASTER, WMTClearColor, WMTFXTemporalScalerProps, WmtcmdBase,
        WmtcmdComputeMemoryBarrier, WmtcmdRenderMemoryBarrier,
    },
};
use winemetal_wire::MemPtr;

use super::{ArgumentBuffer, Encoder, Record, Records, place, stage};
use crate::{
    deptrack::{Access, BarrierState, FenceSet},
    format,
    presenter::{DXMTPresentMetadata, Presenter},
    resource::{Texture, TextureAllocation, TextureViewKey},
    scaler::Scaler,
};

/// The id of a pass that tracks nothing (events, timestamps): the C++'s `~0ull`.
const NO_ID: u64 = u64::MAX;
/// Colour targets a render pass has.
const COLOR_TARGETS: usize = 8;

/// The pass being recorded, or one the chunk ended: the C++'s `EncoderData` and its kinds.
///
/// A render pass's vertex half has an id of its own (`id - 1`) and its own waits and updates.
pub struct Pass {
    pub(super) kind: PassKind,
    pub(super) id: u64,
    /// The vertex half's id (render passes); `id` otherwise.
    pub(super) id_vertex: u64,
    pub(super) fence_wait: FenceSet,
    pub(super) fence_update: FenceSet,
    /// Empty but in a render pass, as `fence_update_vertex`.
    pub(super) fence_wait_vertex: FenceSet,
    pub(super) fence_update_vertex: FenceSet,
    pub(super) barriers: BarrierState,
    pub(super) arguments: Option<ArgumentBuffer>,
    pub(super) records: Records,
    pub(super) emulated_stream_output: bool,
}

impl Pass {
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Whether the pass draws through the emulated stream output (`encode_stream_output`).
    #[must_use]
    pub const fn uses_emulated_stream_output(&self) -> bool {
        self.emulated_stream_output
    }

    /// The pass's first record, a no-op the others follow: what Metal's encoder is handed.
    #[must_use]
    pub const fn records(&self) -> core::ptr::NonNull<WmtcmdBase> {
        self.records.head
    }
}

/// A texture view a pass draws to, clears or resolves: the C++'s `TextureViewRef`.
///
/// Two are the same view when they share the allocation and the key, as the C++ compares the
/// one view object its allocation keeps per key.
pub struct Attachment {
    /// The texture, whose current allocation a depth or stencil write tracks at the pass's end.
    pub(super) texture: Arc<Texture>,
    pub(super) allocation: Arc<TextureAllocation>,
    pub(super) key: TextureViewKey,
    /// The view's Metal texture.
    pub(super) view: u64,
}

impl Attachment {
    /// The view's Metal texture.
    #[must_use]
    pub const fn view(&self) -> u64 {
        self.view
    }

    fn is(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.allocation, &other.allocation) && self.key == other.key
    }
}

/// What a render pass does with one of its targets.
pub struct Target {
    pub attachment: Option<Attachment>,
    /// A `WMTLoadAction`.
    pub load_action: u32,
    /// A `WMTStoreAction`.
    pub store_action: u32,
    pub level: u16,
    pub slice: u16,
    pub depth_plane: u32,
}

pub struct ColorTarget {
    pub target: Target,
    pub clear_color: WMTClearColor,
    /// The view the target is resolved to.
    pub resolve: Option<Attachment>,
    pub resolve_level: u16,
    pub resolve_slice: u16,
    pub resolve_depth_plane: u32,
}

pub struct DepthTarget {
    pub target: Target,
    pub clear_depth: f32,
}

pub struct StencilTarget {
    pub target: Target,
    pub clear_stencil: u8,
}

bitflags::bitflags! {
    /// What a render pass uses beyond its targets.
    #[derive(Clone, Copy)]
    pub struct RenderUses: u8 {
        /// Some draw counts samples into the chunk's visibility buffer.
        const VISIBILITY_RESULT = 1;
        const TESSELLATION = 1 << 1;
        const GEOMETRY = 1 << 2;
    }
}

/// A geometry-shader indirect dispatch arguments marshal task queued for a render pass.
pub struct GsMarshalTask {
    pub draw_args_buf: u64,
    pub draw_args_va: u64,
    pub vertex_count_per_warp: u32,
    pub max_object_threadgroups: u64,
    pub dispatch_args_buf: u64,
    pub dispatch_args_va: u64,
}

/// A tessellation indirect dispatch arguments marshal task queued for a render pass.
pub struct TsMarshalTask {
    pub draw_args_buf: u64,
    pub draw_args_va: u64,
    pub control_point_count: u16,
    pub patch_per_group: u16,
    pub max_object_threadgroups: u64,
    pub dispatch_args_buf: u64,
    pub dispatch_args_va: u64,
}

/// A render pass's targets and their sizes: the C++ `RenderEncoderData`'s own fields.
///
/// The front end fills them after [`Encoder::start_render_pass`], as the C++'s does.
pub struct RenderTargets {
    pub colors: [ColorTarget; COLOR_TARGETS],
    pub depth: DepthTarget,
    pub stencil: StencilTarget,
    pub default_raster_sample_count: u8,
    pub render_target_array_length: u16,
    pub render_target_height: u32,
    pub render_target_width: u32,
    /// The planes the depth-stencil view has: depth 1, stencil 2.
    pub dsv_planar_flags: u8,
    /// The planes the depth-stencil view only reads.
    pub dsv_readonly_flags: u8,
    pub render_target_count: u8,
    pub uses: RenderUses,
    pub gs_marshal_tasks: Vec<GsMarshalTask>,
    pub ts_marshal_tasks: Vec<TsMarshalTask>,
    pub tile_barrier_pso_key: [u32; 9],
    pub last_pso: u64,
}

/// The textures [`Encoder::upscale_temporal`] reads and writes.
pub struct TemporalTextures<'a> {
    pub input: &'a Texture,
    pub output: &'a Texture,
    pub depth: &'a Texture,
    pub motion_vector: &'a Texture,
    /// The view of `motion_vector` the scaler reads.
    pub motion_vector_view: TextureViewKey,
    pub exposure: Option<&'a Texture>,
}

impl Encoder {
    /// Starts a render pass with its slice of the argument buffer.
    ///
    /// The front end then sets its targets through [`Encoder::render_targets`].
    pub fn start_render_pass(
        &mut self,
        dsv_planar_flags: u8,
        dsv_readonly_flags: u8,
        render_target_count: u8,
        arguments: Option<ArgumentBuffer>,
    ) {
        let targets = RenderTargets {
            colors: core::array::from_fn(|_| ColorTarget {
                target: Target::NONE,
                clear_color: WMTClearColor {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                },
                resolve: None,
                resolve_level: 0,
                resolve_slice: 0,
                resolve_depth_plane: 0,
            }),
            depth: DepthTarget {
                target: Target::NONE,
                clear_depth: 0.0,
            },
            stencil: StencilTarget {
                target: Target::NONE,
                clear_stencil: 0,
            },
            default_raster_sample_count: 0,
            render_target_array_length: 0,
            render_target_height: 0,
            render_target_width: 0,
            dsv_planar_flags,
            dsv_readonly_flags,
            render_target_count,
            uses: RenderUses::empty(),
            gs_marshal_tasks: Vec::new(),
            ts_marshal_tasks: Vec::new(),
            tile_barrier_pso_key: [0; 9],
            last_pso: 0,
        };
        self.begin(PassKind::Render(Box::new(targets)), arguments);
        self.queries.begin_pass();
        self.frame_stats.render_pass_count += 1;
    }

    /// Starts a compute pass with its slice of the argument buffer.
    pub fn start_compute_pass(&mut self, arguments: Option<ArgumentBuffer>) {
        self.begin(PassKind::Compute, arguments);
        self.frame_stats.compute_pass_count += 1;
    }

    pub fn start_blit_pass(&mut self) {
        self.begin(PassKind::Blit, None);
        self.frame_stats.blit_pass_count += 1;
    }

    /// The render pass's targets, while a render pass is being recorded.
    pub fn render_targets(&mut self) -> Option<&mut RenderTargets> {
        match self.pass.as_mut().map(|p| &mut p.kind) {
            Some(PassKind::Render(targets)) => Some(targets),
            _ => None,
        }
    }

    /// `texture`'s view `key` as a target of the pass, used as `access` says by stage `S`.
    ///
    /// `None` when the texture has no allocation.
    pub fn attachment<const S: u8>(
        &mut self,
        texture: &Arc<Texture>,
        key: TextureViewKey,
        access: Access,
    ) -> Option<Attachment> {
        let (allocation, view) = self.texture_view::<S>(texture, key, access, None)?;
        Some(Attachment {
            texture: Arc::clone(texture),
            allocation,
            key,
            view: view.texture,
        })
    }

    /// Appends `cmd` to the blit pass's records.
    pub fn encode_blit_command<T: Record>(&mut self, cmd: T) {
        self.append(cmd, |k| matches!(k, PassKind::Blit));
    }

    /// Ends the pass and appends it to the chunk's.
    ///
    /// A render pass first writes its depth and stencil targets (the planes it does not only
    /// read); then each half keeps only the waits no other of its waits implies.
    pub fn end_pass(&mut self) {
        for (texture, key) in self.depth_stencil_writes().into_iter().flatten() {
            self.texture_view::<{ stage::PIXEL }>(&texture, key, Access::WRITE, None);
        }
        let Some(mut pass) = self.pass.take() else {
            debug_assert!(false, "a pass to end");
            return;
        };
        if matches!(pass.kind, PassKind::Render(_)) {
            self.queries.end_pass();
        }
        if pass.id != NO_ID {
            let wait = core::mem::replace(&mut pass.fence_wait, FenceSet::EMPTY);
            if matches!(pass.kind, PassKind::Render(_)) {
                let vertex = core::mem::replace(&mut pass.fence_wait_vertex, FenceSet::EMPTY);
                pass.fence_wait_vertex =
                    self.locality
                        .collect_and_simplify_waits(vertex, pass.id_vertex, false);
                pass.fence_wait = self
                    .locality
                    .collect_and_simplify_waits(wait, pass.id, true);
            } else {
                pass.fence_wait = self
                    .locality
                    .collect_and_simplify_waits(wait, pass.id, false);
            }
        }
        self.passes.push(pass);
    }

    /// Clears `texture`'s view `key` (`array_length` slices) to `color`, in a pass of its own.
    pub fn clear_color(
        &mut self,
        texture: &Arc<Texture>,
        key: TextureViewKey,
        array_length: u32,
        mut color: WMTClearColor,
    ) {
        format::sanitize_clear_color(key.format(), &mut color);
        let [width, height, _] = texture.view_size(key);
        self.clear(
            Clear {
                attachment: None,
                planes: 0,
                color,
                depth: 0.0,
                stencil: 0,
                array_length,
                width,
                height,
            },
            texture,
            key,
        );
    }

    /// Clears the `planes` (depth 1, stencil 2) of `texture`'s view `key` it has.
    pub fn clear_depth_stencil(
        &mut self,
        texture: &Arc<Texture>,
        key: TextureViewKey,
        array_length: u32,
        planes: u32,
        depth: f32,
        stencil: u8,
    ) {
        let [width, height, _] = texture.view_size(key);
        self.clear(
            Clear {
                attachment: None,
                planes: planes & format::depth_stencil_planar_flags(texture.info().pixel_format),
                color: WMTClearColor {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                },
                depth,
                stencil,
                array_length,
                width,
                height,
            },
            texture,
            key,
        );
    }

    /// Resolves the multisampled view `src_key` of `src` into `dst`'s view `dst_key`.
    pub fn resolve_texture(
        &mut self,
        src: &Arc<Texture>,
        src_key: TextureViewKey,
        dst: &Arc<Texture>,
        dst_key: TextureViewKey,
    ) {
        self.begin(
            PassKind::Resolve(Resolve {
                src: None,
                dst: None,
            }),
            None,
        );
        let src = self.attachment::<{ stage::COMPUTE }>(src, src_key, Access::READ);
        let dst = self.attachment::<{ stage::COMPUTE }>(dst, dst_key, Access::WRITE);
        if let Some(PassKind::Resolve(resolve)) = self.current_kind() {
            resolve.src = src;
            resolve.dst = dst;
        }
        self.end_pass();
    }

    /// Presents `texture` through `presenter`, `after` seconds after the last present if above 0.
    pub fn present(
        &mut self,
        texture: &Texture,
        presenter: Arc<Presenter>,
        after: f64,
        metadata: DXMTPresentMetadata,
    ) {
        self.begin(
            PassKind::Present(Present {
                backbuffer: 0,
                presenter,
                after,
                metadata,
            }),
            None,
        );
        let backbuffer = self.whole::<false>(texture);
        if let Some(PassKind::Present(present)) = self.current_kind() {
            present.backbuffer = backbuffer;
        }
        self.end_pass();
    }

    /// Upscales `texture` into `upscaled` with the spatial `scaler`.
    pub fn upscale(&mut self, texture: &Texture, upscaled: &Texture, scaler: Arc<Scaler>) {
        self.begin(
            PassKind::SpatialUpscale(SpatialUpscale {
                backbuffer: 0,
                upscaled: 0,
                scaler,
            }),
            None,
        );
        let backbuffer = self.whole::<false>(texture);
        let upscaled = self.whole::<true>(upscaled);
        if let Some(PassKind::SpatialUpscale(upscale)) = self.current_kind() {
            upscale.backbuffer = backbuffer;
            upscale.upscaled = upscaled;
        }
        self.end_pass();
    }

    /// Upscales `textures.input` into `textures.output` with the temporal `scaler`.
    pub fn upscale_temporal(
        &mut self,
        textures: &TemporalTextures<'_>,
        scaler: Arc<Scaler>,
        props: WMTFXTemporalScalerProps,
    ) {
        self.begin(
            PassKind::TemporalUpscale(TemporalUpscale {
                input: 0,
                output: 0,
                depth: 0,
                motion_vector: 0,
                exposure: 0,
                scaler,
                props,
            }),
            None,
        );
        let input = self.whole::<false>(textures.input);
        let depth = self.whole::<false>(textures.depth);
        let motion_vector = self
            .texture_view::<{ stage::COMPUTE }>(
                textures.motion_vector,
                textures.motion_vector_view,
                Access::READ,
                None,
            )
            .map_or(0, |(_, view)| view.texture);
        let output = self.whole::<true>(textures.output);
        let exposure = textures.exposure.map_or(0, |e| self.whole::<false>(e));
        if let Some(PassKind::TemporalUpscale(upscale)) = self.current_kind() {
            upscale.input = input;
            upscale.depth = depth;
            upscale.motion_vector = motion_vector;
            upscale.output = output;
            upscale.exposure = exposure;
        }
        self.end_pass();
    }

    /// Signals the queue's shared event at `value` once the passes before it are done.
    pub fn signal_queue_event(&mut self, value: u64) {
        self.begin(PassKind::SignalEvent(None, value), None);
        self.end_pass();
    }

    /// Signals `event` at `value` once the passes before it are done.
    pub fn signal_event(&mut self, event: Obj, value: u64) {
        self.begin(PassKind::SignalEvent(Some(event), value), None);
        self.end_pass();
    }

    /// Makes the passes after it wait until `event` reaches `value`.
    pub fn wait_event(&mut self, event: Obj, value: u64) {
        self.begin(PassKind::WaitForEvent(event, value), None);
        self.end_pass();
    }

    /// Whether the last pass samples a timestamp: a query made now shares its sample.
    #[must_use]
    pub fn last_samples_timestamp(&self) -> bool {
        matches!(
            self.passes.last(),
            Some(Pass {
                kind: PassKind::SampleTimestamp(_),
                ..
            })
        )
    }

    /// Samples the GPU's timestamp into sample `index` of the chunk's sample buffer.
    pub fn sample_timestamp(&mut self, index: u64) {
        self.begin(PassKind::SampleTimestamp(index), None);
        self.end_pass();
    }

    /// Ends the compute pass's barrier: a memory barrier when an access needs one.
    pub fn resolve_compute_pass_barrier(&mut self) {
        let mask = !self.intrapass_barriers;
        let Some(pass) = self.pass.as_mut() else {
            debug_assert!(false, "a compute pass to resolve");
            return;
        };
        debug_assert!(matches!(pass.kind, PassKind::Compute), "a compute pass");
        if u64::from(pass.barriers.general) & mask == 0 {
            return;
        }
        pass.barriers.general = 0;
        self.encode_compute_command(WmtcmdComputeMemoryBarrier {
            r#type: WMT_COMPUTE_COMMAND_MEMORY_BARRIER,
            reserved: [0; 3],
            next: MemPtr::NULL,
            scope: WMT_BARRIER_SCOPE_BUFFERS | WMT_BARRIER_SCOPE_TEXTURES,
        });
    }

    /// Ends the render pass's barriers.
    ///
    /// `tile_barrier` encodes the tile barrier a fragment access after a fragment one needs;
    /// memory barriers between the stages serve the others.
    ///
    /// A pre-raster access after a fragment one needs the pass split, which is not done: its
    /// barriers are dropped, as the C++'s.
    pub fn resolve_render_pass_barrier(&mut self, tile_barrier: impl FnOnce(&mut Self)) {
        let mask = !self.intrapass_barriers;
        let Some(pass) = self.pass.as_mut() else {
            debug_assert!(false, "a render pass to resolve");
            return;
        };
        debug_assert!(matches!(pass.kind, PassKind::Render(_)), "a render pass");
        let barriers = &mut pass.barriers;
        let pending = |bits: u8| u64::from(bits) & mask != 0;
        if pending(barriers.pre_raster_after_fragment) {
            // ponytail: the C++ warns "A fragment-vertex barrier is omitted" here; the core has
            // no log yet.
            *barriers = BarrierState::default();
            return;
        }
        let tile = pending(barriers.general);
        let pre_raster = pending(barriers.pre_raster);
        let fragment = pending(barriers.fragment_after_pre_raster);
        if tile {
            barriers.general = 0;
        }
        if pre_raster {
            barriers.pre_raster = 0;
        }
        if fragment {
            barriers.fragment_after_pre_raster = 0;
        }
        if tile {
            tile_barrier(self);
        }
        if pre_raster {
            self.encode_render_command(memory_barrier(WMT_RENDER_STAGE_PRE_RASTER));
        }
        if fragment {
            self.encode_render_command(memory_barrier(WMT_RENDER_STAGE_FRAGMENT));
        }
    }

    /// Sets the barrier bits passes ignore inside themselves.
    pub const fn set_intrapass_barrier_control(&mut self, bits: u64) {
        self.intrapass_barriers = bits;
    }

    /// Starts a pass of `kind`: its ids, the updates that mark its end, its records' head.
    fn begin(&mut self, kind: PassKind, arguments: Option<ArgumentBuffer>) {
        debug_assert!(self.pass.is_none(), "one pass at a time");
        let render = matches!(kind, PassKind::Render(_));
        let (id_vertex, id) = match kind {
            PassKind::Render(_) => {
                self.next_id += 2;
                (self.next_id - 2, self.next_id - 1)
            }
            PassKind::SignalEvent(..)
            | PassKind::WaitForEvent(..)
            | PassKind::SampleTimestamp(_) => (NO_ID, NO_ID),
            _ => {
                self.next_id += 1;
                (self.next_id - 1, self.next_id - 1)
            }
        };
        let r#type = match kind {
            PassKind::Compute => WMT_COMPUTE_COMMAND_NOP,
            PassKind::Blit => WMT_BLIT_COMMAND_NOP,
            _ => WMT_RENDER_COMMAND_NOP,
        };
        let head = place(
            &mut self.heap,
            WmtcmdBase {
                r#type,
                reserved: [0; 3],
                next: MemPtr::NULL,
            },
        );
        self.pass = Some(Pass {
            kind,
            id,
            id_vertex,
            fence_wait: FenceSet::EMPTY,
            fence_update: if id == NO_ID {
                FenceSet::EMPTY
            } else {
                FenceSet::of(id)
            },
            fence_wait_vertex: FenceSet::EMPTY,
            fence_update_vertex: if render {
                FenceSet::of(id_vertex)
            } else {
                FenceSet::EMPTY
            },
            barriers: BarrierState::default(),
            arguments,
            records: Records { head, tail: head },
            emulated_stream_output: false,
        });
    }

    fn current_kind(&mut self) -> Option<&mut PassKind> {
        self.pass.as_mut().map(|p| &mut p.kind)
    }

    /// A clear pass of `clear`, which writes `texture`'s view `key`.
    fn clear(&mut self, clear: Clear, texture: &Arc<Texture>, key: TextureViewKey) {
        self.begin(PassKind::Clear(clear), None);
        self.frame_stats.clear_pass_count += 1;
        let attachment = self.attachment::<{ stage::COMPUTE }>(texture, key, Access::WRITE);
        if let Some(PassKind::Clear(clear)) = self.current_kind() {
            clear.attachment = attachment;
        }
        self.end_pass();
    }

    /// The Metal texture of `texture`'s whole view, read (or written if `WRITE`) by the pass.
    fn whole<const WRITE: bool>(&mut self, texture: &Texture) -> u64 {
        let access = if WRITE { Access::WRITE } else { Access::READ };
        self.texture_view::<{ stage::COMPUTE }>(texture, texture.full_view(), access, None)
            .map_or(0, |(_, view)| view.texture)
    }

    /// The depth and stencil targets the render pass being ended writes.
    fn depth_stencil_writes(&self) -> [Option<(Arc<Texture>, TextureViewKey)>; 2] {
        let Some(Pass {
            kind: PassKind::Render(targets),
            ..
        }) = &self.pass
        else {
            return [None, None];
        };
        let written = |target: &Target, plane: u8| {
            let a = target.attachment.as_ref()?;
            (targets.dsv_readonly_flags & plane == 0).then(|| (Arc::clone(&a.texture), a.key))
        };
        [
            written(&targets.depth.target, 1),
            written(&targets.stencil.target, 2),
        ]
    }
}

/// What a pass is, with what its kind needs to encode.
pub(super) enum PassKind {
    /// Merged into another pass: nothing to encode.
    Null,
    Render(Box<RenderTargets>),
    Compute,
    Blit,
    Clear(Clear),
    Resolve(Resolve),
    Present(Present),
    SpatialUpscale(SpatialUpscale),
    TemporalUpscale(TemporalUpscale),
    /// A shared event's signal at a value; `None` is the queue's own event.
    SignalEvent(Option<Obj>, u64),
    WaitForEvent(Obj, u64),
    /// The GPU's timestamp, into this sample of the chunk's sample buffer.
    SampleTimestamp(u64),
}

/// A clear pass: a colour view's, or the planes of a depth-stencil view.
pub(super) struct Clear {
    pub(super) attachment: Option<Attachment>,
    /// 0 for a colour clear, else the planes cleared: depth 1, stencil 2.
    pub(super) planes: u32,
    pub(super) color: WMTClearColor,
    pub(super) depth: f32,
    pub(super) stencil: u8,
    pub(super) array_length: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) struct Resolve {
    pub(super) src: Option<Attachment>,
    pub(super) dst: Option<Attachment>,
}

/// A present: the backbuffer's Metal texture, whose allocation the chunk keeps.
pub(super) struct Present {
    pub(super) backbuffer: u64,
    pub(super) presenter: Arc<Presenter>,
    pub(super) after: f64,
    pub(super) metadata: DXMTPresentMetadata,
}

pub(super) struct SpatialUpscale {
    pub(super) backbuffer: u64,
    pub(super) upscaled: u64,
    pub(super) scaler: Arc<Scaler>,
}

pub(super) struct TemporalUpscale {
    pub(super) input: u64,
    pub(super) output: u64,
    pub(super) depth: u64,
    pub(super) motion_vector: u64,
    pub(super) exposure: u64,
    pub(super) scaler: Arc<Scaler>,
    pub(super) props: WMTFXTemporalScalerProps,
}

impl Target {
    const NONE: Self = Self {
        attachment: None,
        load_action: 0,
        store_action: 0,
        level: 0,
        slice: 0,
        depth_plane: 0,
    };
}

/// Whether `a` and `b` are the same view, or both no view.
pub(super) fn same(a: Option<&Attachment>, b: Option<&Attachment>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.is(b),
        (None, None) => true,
        _ => false,
    }
}

/// The Metal buffer a pass's argument buffer is part of, or 0.
pub(super) fn argument_buffer(arguments: Option<&ArgumentBuffer>) -> u64 {
    arguments.map_or(0, ArgumentBuffer::buffer)
}

/// A render memory barrier on buffers and textures, before stages `before`, after pre-raster.
const fn memory_barrier(before: u8) -> WmtcmdRenderMemoryBarrier {
    WmtcmdRenderMemoryBarrier {
        r#type: WMT_RENDER_COMMAND_MEMORY_BARRIER,
        reserved: [0; 3],
        next: MemPtr::NULL,
        scope: WMT_BARRIER_SCOPE_BUFFERS | WMT_BARRIER_SCOPE_TEXTURES,
        stages_before: before,
        stages_after: WMT_RENDER_STAGE_PRE_RASTER,
    }
}
