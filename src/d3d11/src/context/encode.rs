//! Command buffer state machine and pass encoder transitions (`d3d11_context_impl.cpp`).
//!
//! Tracks `CommandBufferState` transitions (`Idle`, `RenderPipelineReady`, `ComputePipelineReady`,
//! `RenderEncoderActive`, `ComputeEncoderActive`, `BlitEncoderActive`), pass boundaries, and
//! command record emission into the context's chunk arena and [`Encoder`].

use core::{
    alloc::Layout,
    ffi::c_void,
    fmt,
    ptr::{self, NonNull},
    sync::atomic::{AtomicU64, Ordering},
};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use dxmt_core::{
    arena::{BlockPool, Commands},
    deptrack::Access,
    encoder::{Encoder, NO_NEXT, Record, kind, stage},
    resource::{Texture, TextureViewKey},
};
use rustc_hash::FxHashMap;
use winemetal_sys::wire::{
    MemPtr, WMT_BLIT_COMMAND_NOP, WMT_COMPUTE_COMMAND_NOP, WMT_COMPUTE_COMMAND_SET_PSO,
    WMT_LOAD_ACTION_LOAD, WMT_PIXEL_FORMAT_INVALID, WMT_RENDER_COMMAND_NOP,
    WMT_RENDER_COMMAND_SET_PSO, WMT_STORE_ACTION_STORE, WMTScissorRect, WMTSize, WMTViewport,
    WmtcmdBase, WmtcmdComputeSetpso, WmtcmdRenderSetpso,
};

use super::{Context, Mode, state::DirtyFlags};
use crate::view::Dsv;

/// Record alignment in the chunk arena (`RECORD_ALIGN`).
const RECORD_ALIGN: usize = 16;

/// Default arena block size for context command recording.
const ARENA_BLOCK_SIZE: usize = 0x1_0000;

/// Command buffer encoder state (`CommandBufferState`).
#[derive(Default, PartialEq, Eq)]
pub enum CommandBufferState {
    #[default]
    Idle,
    RenderPipelineReady,
    ComputePipelineReady,
    RenderEncoderActive,
    ComputeEncoderActive,
    BlitEncoderActive,
}

impl fmt::Debug for CommandBufferState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Idle => "Idle",
            Self::RenderPipelineReady => "RenderPipelineReady",
            Self::ComputePipelineReady => "ComputePipelineReady",
            Self::RenderEncoderActive => "RenderEncoderActive",
            Self::ComputeEncoderActive => "ComputeEncoderActive",
            Self::BlitEncoderActive => "BlitEncoderActive",
        };
        f.write_str(name)
    }
}

/// Kind of pass recorded by a context.
#[derive(PartialEq, Eq)]
pub enum RecordedPassKind {
    Render,
    Compute,
    Blit,
}

impl RecordedPassKind {
    const fn copy_kind(&self) -> Self {
        match self {
            Self::Render => Self::Render,
            Self::Compute => Self::Compute,
            Self::Blit => Self::Blit,
        }
    }
}

impl fmt::Debug for RecordedPassKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Render => f.write_str("Render"),
            Self::Compute => f.write_str("Compute"),
            Self::Blit => f.write_str("Blit"),
        }
    }
}

/// Rasterizer state fields encoded by `WmtcmdRenderSetrasterizerstate`.
pub struct RasterizerStateCmd {
    pub fill_mode: u8,
    pub cull_mode: u8,
    pub depth_clip_mode: u8,
    pub winding: u8,
    pub depth_bias: f32,
    pub slope_scale: f32,
    pub depth_bias_clamp: f32,
}

impl RasterizerStateCmd {
    pub(super) const fn copy_cmd(&self) -> Self {
        Self {
            fill_mode: self.fill_mode,
            cull_mode: self.cull_mode,
            depth_clip_mode: self.depth_clip_mode,
            winding: self.winding,
            depth_bias: self.depth_bias,
            slope_scale: self.slope_scale,
            depth_bias_clamp: self.depth_bias_clamp,
        }
    }
}

impl Default for RasterizerStateCmd {
    fn default() -> Self {
        Self {
            fill_mode: 0,
            cull_mode: 2,
            depth_clip_mode: 0,
            winding: 0,
            depth_bias: 0.0,
            slope_scale: 0.0,
            depth_bias_clamp: 0.0,
        }
    }
}

/// Active pass record chain inside a context's command arena.
struct ActivePass {
    kind: RecordedPassKind,
    head: NonNull<WmtcmdBase>,
    tail: NonNull<WmtcmdBase>,
}

// SAFETY: `head` and `tail` point into the session's owned `Commands` arena.
unsafe impl Send for ActivePass {}

/// Snapshot of a context's encoding configuration and bound state.
pub(super) struct EncodeSnapshot {
    pub render_pipeline_valid: bool,
    pub render_pso: u64,
    pub pipeline_kind: u8,
    pub gs_vertex_per_warp: u32,
    pub gs_object_buffer_offset: Option<(u64, u8)>,
    pub gs_mesh_buffer_offset: Option<(u64, u8)>,
    pub gs_draw_arguments_offset: u64,
    pub gs_imm_draw_args_handle: u64,
    pub gs_dispatch_args_handle: u64,
    pub gs_dispatch_args_offset: u64,
    pub ts_threads_per_patch: u32,
    pub ts_patch_per_group: u32,
    pub ts_patch_per_mesh_instance: u32,
    pub ts_draw_arguments_offset: u64,
    pub ts_imm_draw_args_handle: u64,
    pub ts_dispatch_args_handle: u64,
    pub ts_dispatch_args_offset: u64,
    pub compute_pipeline_valid: bool,
    pub compute_pso: u64,
    pub compute_threadgroup_size: WMTSize,
    pub dsso: u64,
    pub rasterizer: RasterizerStateCmd,
    pub viewports: Vec<WMTViewport>,
    pub scissor_rects: Vec<WMTScissorRect>,
    pub index_buffer: u64,
    pub index_format: u32,
    pub index_sub_offset: u32,
    pub stage_dirty_masks: [u32; 6],
    pub render_argbuf_size: Option<Arc<AtomicU64>>,
    pub compute_argbuf_size: Option<Arc<AtomicU64>>,
}

impl Clone for EncodeSnapshot {
    fn clone(&self) -> Self {
        Self {
            render_pipeline_valid: self.render_pipeline_valid,
            render_pso: self.render_pso,
            pipeline_kind: self.pipeline_kind,
            gs_vertex_per_warp: self.gs_vertex_per_warp,
            gs_object_buffer_offset: self.gs_object_buffer_offset,
            gs_mesh_buffer_offset: self.gs_mesh_buffer_offset,
            gs_draw_arguments_offset: self.gs_draw_arguments_offset,
            gs_imm_draw_args_handle: self.gs_imm_draw_args_handle,
            gs_dispatch_args_handle: self.gs_dispatch_args_handle,
            gs_dispatch_args_offset: self.gs_dispatch_args_offset,
            ts_threads_per_patch: self.ts_threads_per_patch,
            ts_patch_per_group: self.ts_patch_per_group,
            ts_patch_per_mesh_instance: self.ts_patch_per_mesh_instance,
            ts_draw_arguments_offset: self.ts_draw_arguments_offset,
            ts_imm_draw_args_handle: self.ts_imm_draw_args_handle,
            ts_dispatch_args_handle: self.ts_dispatch_args_handle,
            ts_dispatch_args_offset: self.ts_dispatch_args_offset,
            compute_pipeline_valid: self.compute_pipeline_valid,
            compute_pso: self.compute_pso,
            compute_threadgroup_size: self.compute_threadgroup_size,
            dsso: self.dsso,
            rasterizer: self.rasterizer.copy_cmd(),
            viewports: self.viewports.clone(),
            scissor_rects: self.scissor_rects.clone(),
            index_buffer: self.index_buffer,
            index_format: self.index_format,
            index_sub_offset: self.index_sub_offset,
            stage_dirty_masks: self.stage_dirty_masks,
            render_argbuf_size: self.render_argbuf_size.clone(),
            compute_argbuf_size: self.compute_argbuf_size.clone(),
        }
    }
}

impl Default for EncodeSnapshot {
    fn default() -> Self {
        Self {
            render_pipeline_valid: false,
            render_pso: 0,
            pipeline_kind: kind::ORDINARY,
            gs_vertex_per_warp: 0,
            gs_object_buffer_offset: None,
            gs_mesh_buffer_offset: None,
            gs_draw_arguments_offset: 0,
            gs_imm_draw_args_handle: 1,
            gs_dispatch_args_handle: 1,
            gs_dispatch_args_offset: 32,
            ts_threads_per_patch: 0,
            ts_patch_per_group: 0,
            ts_patch_per_mesh_instance: 0,
            ts_draw_arguments_offset: 0,
            ts_imm_draw_args_handle: 1,
            ts_dispatch_args_handle: 1,
            ts_dispatch_args_offset: 32,
            compute_pipeline_valid: false,
            compute_pso: 0,
            compute_threadgroup_size: WMTSize {
                width: 1,
                height: 1,
                depth: 1,
            },
            dsso: 0,
            rasterizer: RasterizerStateCmd::default(),
            viewports: Vec::new(),
            scissor_rects: Vec::new(),
            index_buffer: 0,
            index_format: 57,
            index_sub_offset: 0,
            stage_dirty_masks: [0; 6],
            render_argbuf_size: None,
            compute_argbuf_size: None,
        }
    }
}

/// Per-context encoding session holding the command arena, active pass, and state snapshot.
struct EncodeSession {
    commands: Commands,
    active_pass: Option<ActivePass>,
    completed_passes: Vec<(RecordedPassKind, usize)>,
    promote_flush_count: u32,
    snap: EncodeSnapshot,
}

impl EncodeSession {
    fn new() -> Self {
        Self {
            commands: Commands::new(Arc::new(BlockPool::new(ARENA_BLOCK_SIZE))),
            active_pass: None,
            completed_passes: Vec::new(),
            promote_flush_count: 0,
            snap: EncodeSnapshot::default(),
        }
    }

    fn allocate_record<T>(&mut self, value: T) -> NonNull<T> {
        let layout = Layout::new::<T>()
            .align_to(RECORD_ALIGN)
            .unwrap_or_else(|_| Layout::new::<T>());
        let ptr = self.commands.allocate(layout).cast::<T>();
        // SAFETY: `allocate` returns fresh memory with `T`'s size and 16-byte alignment.
        unsafe { ptr.write(value) };
        ptr
    }

    fn begin_pass(&mut self, pass_kind: RecordedPassKind) {
        let nop_type = match pass_kind {
            RecordedPassKind::Render => WMT_RENDER_COMMAND_NOP,
            RecordedPassKind::Compute => WMT_COMPUTE_COMMAND_NOP,
            RecordedPassKind::Blit => WMT_BLIT_COMMAND_NOP,
        };
        let head = self.allocate_record(WmtcmdBase {
            r#type: nop_type,
            reserved: [0; 3],
            next: NO_NEXT,
        });
        self.active_pass = Some(ActivePass {
            kind: pass_kind,
            head,
            tail: head,
        });
    }

    fn end_pass(&mut self) {
        if let Some(pass) = self.active_pass.take() {
            self.completed_passes
                .push((pass.kind, pass.head.as_ptr().expose_provenance()));
        }
    }

    fn append_record<T: Record>(&mut self, cmd: T) {
        let record = self.allocate_record(cmd).cast::<WmtcmdBase>();
        // SAFETY: `record` was just written and starts with `WmtcmdBase`.
        unsafe { (*record.as_ptr()).next = NO_NEXT };
        if let Some(pass) = self.active_pass.as_mut() {
            // SAFETY: `pass.tail` is a live record in `self.commands`.
            unsafe { (*pass.tail.as_ptr()).next = record_mem_ptr(record) };
            pass.tail = record;
        }
    }
}

static SESSIONS: LazyLock<Mutex<FxHashMap<usize, EncodeSession>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

fn with_session<M: Mode, R>(ctx: &Context<M>, f: impl FnOnce(&mut EncodeSession) -> R) -> R {
    let key = ptr::from_ref(ctx).addr();
    f(SESSIONS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(key)
        .or_insert_with(EncodeSession::new))
}

#[cfg(target_pointer_width = "64")]
fn record_mem_ptr(record: NonNull<WmtcmdBase>) -> MemPtr {
    MemPtr {
        ptr: record.as_ptr().expose_provenance() as u64,
    }
}

#[cfg(target_pointer_width = "32")]
fn record_mem_ptr(record: NonNull<WmtcmdBase>) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(record.as_ptr().expose_provenance()).unwrap_or(0),
        high: 0,
    }
}

#[cfg(target_pointer_width = "64")]
pub(super) fn slice_mem_ptr<T>(ptr: NonNull<T>) -> MemPtr {
    MemPtr {
        ptr: ptr.as_ptr().expose_provenance() as u64,
    }
}

#[cfg(target_pointer_width = "32")]
pub(super) fn slice_mem_ptr<T>(ptr: NonNull<T>) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(ptr.as_ptr().expose_provenance()).unwrap_or(0),
        high: 0,
    }
}

impl<M: Mode> Context<M> {
    /// Active command queue sequence number if immediate, or `0` if deferred.
    pub(super) fn active_seq_any(&mut self) -> u64 {
        self.as_immediate_mut()
            .map_or(0, super::Context::<super::Immediate>::active_seq)
    }

    /// Returns a clone of the current encoding state snapshot.
    pub(super) fn encode_snapshot(&self) -> EncodeSnapshot {
        with_session(self, |s| s.snap.clone())
    }

    /// Mutates the current encoding state snapshot.
    pub(super) fn update_encode_snapshot(&self, f: impl FnOnce(&mut EncodeSnapshot)) {
        with_session(self, |s| f(&mut s.snap));
    }

    /// Allocates and initializes a contiguous slice copy in the current command chunk arena.
    pub(super) fn allocate_chunk_slice<T: Copy>(&self, items: &[T]) -> NonNull<T> {
        with_session(self, |s| {
            let byte_len = size_of_val(items).max(1);
            let align = align_of::<T>().max(8);
            let layout = Layout::from_size_align(byte_len, align)
                .unwrap_or_else(|_| Layout::from_size_align(byte_len, 8).expect("valid layout"));
            let dst = s.commands.allocate(layout).cast::<T>();
            if !items.is_empty() {
                // SAFETY: `dst` has space for `items.len()` elements of `T` and does not overlap.
                unsafe {
                    ptr::copy_nonoverlapping(items.as_ptr(), dst.as_ptr(), items.len());
                }
            }
            dst
        })
    }

    /// Pre-allocates space in the active pass argument buffer (`PreAllocateArgumentBuffer`).
    pub(super) fn pre_allocate_argument_buffer(
        &self,
        is_compute: bool,
        size: u64,
        alignment: u64,
    ) -> u64 {
        let cell = with_session(self, |s| {
            if is_compute {
                s.snap.compute_argbuf_size.clone()
            } else {
                s.snap.render_argbuf_size.clone()
            }
        });
        let Some(counter) = cell else {
            return 0;
        };
        let cur = counter.load(Ordering::Relaxed);
        let align = alignment.max(1);
        let aligned = (cur + align - 1) & !(align - 1);
        counter.store(aligned + size, Ordering::Relaxed);
        aligned
    }

    /// Invalidates a ready render pipeline to `RenderEncoderActive` (`InvalidateRenderPipeline`).
    pub(super) fn invalidate_render_pipeline(&mut self) {
        if self.cmdbuf_state == CommandBufferState::RenderPipelineReady {
            self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
        }
    }

    /// Invalidates a ready compute pipeline (`InvalidateComputePipeline`).
    pub(super) fn invalidate_compute_pipeline(&mut self) {
        if self.cmdbuf_state == CommandBufferState::ComputePipelineReady {
            self.cmdbuf_state = CommandBufferState::ComputeEncoderActive;
        }
    }

    /// Records a render state command into the active pass and emits `encode_fn` via `emit_st`.
    pub(super) fn emit_render_state_cmd<T, F>(&mut self, cmd: T, encode_fn: F)
    where
        T: Record + Copy + Send + 'static,
        F: Fn(&mut Encoder) + Send + 'static,
    {
        if self.is_live() {
            with_session(self, |s| {
                s.append_record(cmd);
            });
            M::emit_st(self, encode_fn);
        } else {
            with_session(self, |s| {
                s.append_record(cmd);
                s.commands.emit(encode_fn);
            });
        }
    }

    /// Records a render draw command into the active pass and emits `encode_fn` via `emit_op`.
    pub(super) fn emit_render_cmd<T, F>(&mut self, cmd: T, encode_fn: F)
    where
        T: Record + Copy + Send + 'static,
        F: Fn(&mut Encoder) + Send + 'static,
    {
        if self.is_live() {
            with_session(self, |s| {
                s.append_record(cmd);
            });
            M::emit_op(self, encode_fn);
        } else {
            with_session(self, |s| {
                s.append_record(cmd);
                s.commands.emit(encode_fn);
            });
        }
    }

    /// Records a compute state command into the active pass and emits `encode_fn` via `emit_st`.
    pub(super) fn emit_compute_state_cmd<T, F>(&mut self, cmd: T, encode_fn: F)
    where
        T: Record + Copy + Send + 'static,
        F: Fn(&mut Encoder) + Send + 'static,
    {
        if self.is_live() {
            with_session(self, |s| {
                s.append_record(cmd);
            });
            M::emit_st(self, encode_fn);
        } else {
            with_session(self, |s| {
                s.append_record(cmd);
                s.commands.emit(encode_fn);
            });
        }
    }

    /// Records a compute dispatch command into the active pass and emits `encode_fn` via `emit_op`.
    pub(super) fn emit_compute_cmd<T, F>(&mut self, cmd: T, encode_fn: F)
    where
        T: Record + Copy + Send + 'static,
        F: Fn(&mut Encoder) + Send + 'static,
    {
        if self.is_live() {
            with_session(self, |s| {
                s.append_record(cmd);
            });
            M::emit_op(self, encode_fn);
        } else {
            with_session(self, |s| {
                s.append_record(cmd);
                s.commands.emit(encode_fn);
            });
        }
    }

    /// Emits a general [`Encoder`] closure into the active command chunk.
    pub(super) fn emit_encoder<F>(&mut self, encode_fn: F)
    where
        F: FnOnce(&mut Encoder) + Send + 'static,
    {
        let cell = Mutex::new(Some(encode_fn));
        let wrapped = move |enc: &mut Encoder| {
            let f_opt = cell.lock().unwrap_or_else(PoisonError::into_inner).take();
            if let Some(f) = f_opt {
                f(enc);
            }
        };
        if self.is_live() {
            M::emit_st(self, wrapped);
        } else {
            with_session(self, |s| {
                s.commands.emit(wrapped);
            });
        }
    }

    /// Resolves an indirect argument buffer pointer to its Metal handle and base offset.
    pub(super) fn resolve_indirect_buffer(buffer_for_args: *mut c_void) -> Option<(u64, u64)> {
        if buffer_for_args.is_null() {
            return None;
        }
        let addr = buffer_for_args.addr();
        Some((addr as u64, 0))
    }

    /// Whether a render, compute, or blit encoder pass is currently active (`hasOpenPass`).
    #[must_use]
    pub const fn has_open_pass(&self) -> bool {
        matches!(
            self.cmdbuf_state,
            CommandBufferState::RenderEncoderActive
                | CommandBufferState::ComputeEncoderActive
                | CommandBufferState::BlitEncoderActive
        )
    }

    /// Ends the current Metal encoder pass and returns to idle/ready (`InvalidateCurrentPass`).
    pub fn invalidate_current_pass(&mut self, promote_flush: bool) {
        let live = self.is_live();
        match self.cmdbuf_state {
            CommandBufferState::RenderEncoderActive => {
                with_session(self, |s| {
                    s.end_pass();
                    if !live {
                        s.commands.emit(Encoder::end_pass);
                    }
                });
                if live {
                    M::emit_st(self, Encoder::end_pass);
                }
                self.cmdbuf_state = if live {
                    CommandBufferState::Idle
                } else {
                    CommandBufferState::RenderPipelineReady
                };
            }
            CommandBufferState::RenderPipelineReady => {
                if live {
                    with_session(self, |s| {
                        s.end_pass();
                    });
                    M::emit_st(self, Encoder::end_pass);
                    self.cmdbuf_state = CommandBufferState::Idle;
                }
            }
            CommandBufferState::ComputeEncoderActive => {
                with_session(self, |s| {
                    s.end_pass();
                    if !live {
                        s.commands.emit(Encoder::end_pass);
                    }
                });
                if live {
                    M::emit_st(self, Encoder::end_pass);
                }
                self.cmdbuf_state = if live {
                    CommandBufferState::Idle
                } else {
                    CommandBufferState::ComputePipelineReady
                };
            }
            CommandBufferState::ComputePipelineReady => {
                if live {
                    with_session(self, |s| {
                        s.end_pass();
                    });
                    M::emit_st(self, Encoder::end_pass);
                    self.cmdbuf_state = CommandBufferState::Idle;
                }
            }
            CommandBufferState::BlitEncoderActive => {
                with_session(self, |s| {
                    s.end_pass();
                    if !live {
                        s.commands.emit(Encoder::end_pass);
                    }
                });
                if live {
                    M::emit_st(self, Encoder::end_pass);
                }
                self.cmdbuf_state = CommandBufferState::Idle;
            }
            CommandBufferState::Idle => {}
        }
        if promote_flush && !self.has_open_pass() {
            with_session(self, |s| {
                s.promote_flush_count += 1;
            });
            if live {
                if let Some(imm) = self.as_immediate_mut() {
                    imm.mode.has_dirty_op = true;
                } else if let Some(def) = self.as_deferred_mut() {
                    def.mode.has_promoted_flush = true;
                }
            }
        }
    }

    /// Transitions to an active render command encoder (`SwitchToRenderEncoder`).
    pub fn switch_to_render_encoder(&mut self) {
        struct RtvAttachment {
            index: usize,
            texture: Arc<Texture>,
            view: TextureViewKey,
            depth_plane: u32,
            pixel_format: u32,
        }
        if self.is_live() {
            if matches!(
                self.cmdbuf_state,
                CommandBufferState::RenderEncoderActive | CommandBufferState::RenderPipelineReady
            ) {
                return;
            }
            self.invalidate_current_pass(false);
            for stage in self.state.shader_stages.iter_mut().take(5) {
                stage.constant_buffers.set_all_dirty();
                stage.samplers.set_all_dirty();
                stage.srvs.set_all_dirty();
            }
            self.state.input_assembler.vertex_buffers.set_all_dirty();
            if self.state.stream_output.targets.any_bound() {
                self.state.stream_output.targets.set_all_dirty();
            }
            self.state.output_merger.uavs.set_all_dirty();
            self.dirty = DirtyFlags::all();

            let num_rtvs = (self.state.output_merger.num_rtvs as usize).min(8);
            let mut rtv_list = Vec::with_capacity(num_rtvs);
            for (i, rtv_slot) in self
                .state
                .output_merger
                .rtvs
                .iter()
                .enumerate()
                .take(num_rtvs)
            {
                if let Some(rtv_obj) = crate::view::RtvObject::from_iface(rtv_slot.as_ptr())
                    && let Some(tex) = rtv_obj.inner.texture.clone()
                    && rtv_obj.inner.pixel_format != WMT_PIXEL_FORMAT_INVALID
                {
                    rtv_list.push(RtvAttachment {
                        index: i,
                        texture: tex,
                        view: rtv_obj.inner.texture_view.unwrap_or_default(),
                        depth_plane: rtv_obj.pass_desc.depth_plane,
                        pixel_format: rtv_obj.inner.pixel_format,
                    });
                }
            }
            let effective_rt = rtv_list.len();
            let dsv_attachment =
                Dsv::from_iface(self.state.output_merger.dsv.as_ptr()).and_then(|dsv| {
                    dsv.texture.clone().map(|tex| {
                        (
                            tex,
                            dsv.texture_view.unwrap_or_default(),
                            dsv.planar_flags,
                            dsv.readonly_flags,
                        )
                    })
                });
            let mut rt_width = self.state.output_merger.render_target_width;
            let mut rt_height = self.state.output_merger.render_target_height;
            let mut sample_count = self.state.output_merger.sample_count.max(1);
            let rt_array = self.state.output_merger.array_length;
            let uav_only = if dsv_attachment.is_none() && effective_rt == 0 {
                if !self.state.output_merger.uavs.any_bound() {
                    return;
                }
                if self.state.rasterizer.num_viewports == 0 {
                    return;
                }
                let vp = &self.state.rasterizer.viewports[0];
                #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    rt_width = vp.width.max(0.0) as u32;
                    rt_height = vp.height.max(0.0) as u32;
                }
                if rt_width == 0 || rt_height == 0 {
                    return;
                }
                let rs_ptr = if self.state.rasterizer.rasterizer_state.is_null() {
                    self.defaults.rasterizer
                } else {
                    self.state.rasterizer.rasterizer_state.as_ptr()
                };
                sample_count = crate::state::RasterizerState::from_iface(rs_ptr)
                    .map_or(1, |rs| rs.uav_only_sample_count().max(1));
                true
            } else {
                false
            };
            let argbuf_size = Arc::new(AtomicU64::new(0));
            let argbuf_clone = Arc::clone(&argbuf_size);
            with_session(self, |s| {
                s.snap.render_argbuf_size = Some(argbuf_size);
                s.begin_pass(RecordedPassKind::Render);
            });
            let rtv_count = u8::try_from(num_rtvs).unwrap_or(0);
            M::emit_st(self, move |enc| {
                let (dsv_planar, dsv_ro) = dsv_attachment
                    .as_ref()
                    .map_or((0, 0), |(_, _, p, r)| (*p, *r));
                let sz = usize::try_from(argbuf_clone.load(Ordering::Relaxed)).unwrap_or(0);
                let argbuf = enc.allocate_argument_buffer(sz);
                enc.start_render_pass(dsv_planar, dsv_ro, rtv_count, Some(argbuf));
                let mut color_attachments = Vec::with_capacity(rtv_list.len());
                for rtv in &rtv_list {
                    let att = enc.attachment::<{ stage::PIXEL }>(
                        &rtv.texture,
                        rtv.view,
                        Access::READ | Access::WRITE,
                    );
                    color_attachments.push((rtv.index, att, rtv.depth_plane, rtv.pixel_format));
                }
                let depth_att = dsv_attachment.as_ref().and_then(|(tex, view, planar, _)| {
                    ((planar & 1) != 0)
                        .then(|| enc.attachment::<{ stage::PIXEL }>(tex, *view, Access::READ))
                        .flatten()
                });
                let stencil_att = dsv_attachment.as_ref().and_then(|(tex, view, planar, _)| {
                    ((planar & 2) != 0)
                        .then(|| enc.attachment::<{ stage::PIXEL }>(tex, *view, Access::READ))
                        .flatten()
                });
                let Some(info) = enc.render_targets() else {
                    return;
                };
                for (index, att, depth_plane, pixel_format) in color_attachments {
                    let color = &mut info.colors[index];
                    color.target.attachment = att;
                    color.target.depth_plane = depth_plane;
                    color.target.load_action = WMT_LOAD_ACTION_LOAD;
                    color.target.store_action = WMT_STORE_ACTION_STORE;
                    info.tile_barrier_pso_key[index] = pixel_format;
                }
                info.tile_barrier_pso_key[8] = sample_count;
                if let Some((_, _, planar, _)) = dsv_attachment {
                    if (planar & 1) != 0 {
                        info.depth.target.attachment = depth_att;
                        info.depth.target.load_action = WMT_LOAD_ACTION_LOAD;
                        info.depth.target.store_action = WMT_STORE_ACTION_STORE;
                    }
                    if (planar & 2) != 0 {
                        info.stencil.target.attachment = stencil_att;
                        info.stencil.target.load_action = WMT_LOAD_ACTION_LOAD;
                        info.stencil.target.store_action = WMT_STORE_ACTION_STORE;
                    }
                }
                if effective_rt == 0 && uav_only {
                    info.default_raster_sample_count = u8::try_from(sample_count).unwrap_or(1);
                }
                info.render_target_width = rt_width;
                info.render_target_height = rt_height;
                info.render_target_array_length = u16::try_from(rt_array).unwrap_or(0);
            });
            self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
            return;
        }

        match self.cmdbuf_state {
            CommandBufferState::RenderEncoderActive => return,
            CommandBufferState::ComputeEncoderActive | CommandBufferState::BlitEncoderActive => {
                self.invalidate_current_pass(false);
                self.cmdbuf_state = CommandBufferState::Idle;
                if !self.finalize_graphics_pipeline() {
                    return;
                }
            }
            CommandBufferState::ComputePipelineReady | CommandBufferState::Idle => {
                self.cmdbuf_state = CommandBufferState::Idle;
                if !self.finalize_graphics_pipeline() {
                    return;
                }
            }
            CommandBufferState::RenderPipelineReady => {}
        }

        let pso = self.encode_snapshot().render_pso;
        with_session(self, |s| {
            s.begin_pass(RecordedPassKind::Render);
            s.commands.emit(|enc| {
                enc.start_render_pass(0, 0, 0, None);
            });
        });

        // C++ closure #1 (`SwitchToRenderEncoder`, d3d11_context_impl.cpp:2663):
        // binds the compiled graphics/mesh pipeline state on the new render encoder.
        let cmd = WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso,
        };
        self.emit_render_state_cmd(cmd, move |enc| {
            enc.encode_render_command(cmd);
        });

        self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
        self.dirty = DirtyFlags::all();
    }

    /// Transitions to an active compute command encoder (`SwitchToComputeEncoder`).
    pub fn switch_to_compute_encoder(&mut self) {
        if self.is_live() {
            if matches!(
                self.cmdbuf_state,
                CommandBufferState::ComputeEncoderActive | CommandBufferState::ComputePipelineReady
            ) {
                return;
            }
            self.invalidate_current_pass(false);
            let cs_idx = super::state::ShaderStage::Compute.index();
            self.state.shader_stages[cs_idx]
                .constant_buffers
                .set_all_dirty();
            self.state.shader_stages[cs_idx].samplers.set_all_dirty();
            self.state.shader_stages[cs_idx].srvs.set_all_dirty();
            self.state.compute_stage_uav.uavs.set_all_dirty();

            let argbuf_size = Arc::new(AtomicU64::new(0));
            let argbuf_clone = Arc::clone(&argbuf_size);
            with_session(self, |s| {
                s.snap.compute_argbuf_size = Some(argbuf_size);
                s.begin_pass(RecordedPassKind::Compute);
            });
            M::emit_st(self, move |enc| {
                let sz = usize::try_from(argbuf_clone.load(Ordering::Relaxed)).unwrap_or(0);
                let argbuf = enc.allocate_argument_buffer(sz);
                enc.start_compute_pass(Some(argbuf));
            });
            self.cmdbuf_state = CommandBufferState::ComputeEncoderActive;
            return;
        }

        match self.cmdbuf_state {
            CommandBufferState::ComputeEncoderActive => return,
            CommandBufferState::RenderEncoderActive | CommandBufferState::BlitEncoderActive => {
                self.invalidate_current_pass(false);
                self.cmdbuf_state = CommandBufferState::Idle;
                if !self.finalize_compute_pipeline() {
                    return;
                }
            }
            CommandBufferState::RenderPipelineReady | CommandBufferState::Idle => {
                self.cmdbuf_state = CommandBufferState::Idle;
                if !self.finalize_compute_pipeline() {
                    return;
                }
            }
            CommandBufferState::ComputePipelineReady => {}
        }

        let snap = self.encode_snapshot();
        let pso = snap.compute_pso;
        let threadgroup_size = snap.compute_threadgroup_size;
        with_session(self, |s| {
            s.begin_pass(RecordedPassKind::Compute);
            s.commands.emit(|enc| {
                enc.start_compute_pass(None);
            });
        });

        // C++ closure #2 (`SwitchToComputeEncoder`, d3d11_context_impl.cpp:2688):
        // binds the compiled compute pipeline state and threadgroup size.
        let cmd = WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso,
            threadgroup_size,
        };
        self.emit_compute_state_cmd(cmd, move |enc| {
            enc.encode_compute_command(cmd);
        });

        self.cmdbuf_state = CommandBufferState::ComputeEncoderActive;
    }

    /// Transitions to an active blit command encoder (`SwitchToBlitEncoder`).
    pub fn switch_to_blit_encoder(&mut self) {
        match self.cmdbuf_state {
            CommandBufferState::BlitEncoderActive => return,
            CommandBufferState::RenderEncoderActive
            | CommandBufferState::ComputeEncoderActive
            | CommandBufferState::RenderPipelineReady
            | CommandBufferState::ComputePipelineReady => {
                self.invalidate_current_pass(false);
            }
            CommandBufferState::Idle => {}
        }
        let live = self.is_live();
        with_session(self, |s| {
            s.begin_pass(RecordedPassKind::Blit);
            if !live {
                s.commands.emit(Encoder::start_blit_pass);
            }
        });
        if live {
            M::emit_st(self, Encoder::start_blit_pass);
        }
        self.cmdbuf_state = CommandBufferState::BlitEncoderActive;
    }

    /// Configures a ready render pipeline for encoding.
    pub fn set_render_pipeline_ready(&mut self, pso: u64, pipeline_kind: u8) {
        self.update_encode_snapshot(|snap| {
            snap.render_pipeline_valid = pso != 0;
            snap.render_pso = pso;
            snap.pipeline_kind = pipeline_kind;
        });
    }

    /// Configures geometry-shader stage parameters for encoding.
    pub fn set_gs_pipeline_config(
        &mut self,
        pso: u64,
        vertex_per_warp: u32,
        object_buffer_offset: Option<(u64, u8)>,
        mesh_buffer_offset: Option<(u64, u8)>,
        draw_arguments_offset: u64,
    ) {
        self.update_encode_snapshot(|snap| {
            snap.render_pipeline_valid = pso != 0;
            snap.render_pso = pso;
            snap.pipeline_kind = kind::GEOMETRY;
            snap.gs_vertex_per_warp = vertex_per_warp;
            snap.gs_object_buffer_offset = object_buffer_offset;
            snap.gs_mesh_buffer_offset = mesh_buffer_offset;
            snap.gs_draw_arguments_offset = draw_arguments_offset;
        });
    }

    /// Configures tessellation mesh pipeline parameters for encoding.
    pub fn set_ts_pipeline_config(
        &mut self,
        pso: u64,
        threads_per_patch: u32,
        patch_per_group: u32,
        patch_per_mesh_instance: u32,
        draw_arguments_offset: u64,
    ) {
        self.update_encode_snapshot(|snap| {
            snap.render_pipeline_valid = pso != 0;
            snap.render_pso = pso;
            snap.pipeline_kind = kind::TESSELLATION;
            snap.ts_threads_per_patch = threads_per_patch;
            snap.ts_patch_per_group = patch_per_group;
            snap.ts_patch_per_mesh_instance = patch_per_mesh_instance;
            snap.ts_draw_arguments_offset = draw_arguments_offset;
        });
    }

    /// Configures a ready compute pipeline for encoding.
    pub fn set_compute_pipeline_ready(&mut self, pso: u64, threadgroup_size: WMTSize) {
        self.update_encode_snapshot(|snap| {
            snap.compute_pipeline_valid = pso != 0;
            snap.compute_pso = pso;
            snap.compute_threadgroup_size = threadgroup_size;
        });
    }

    /// Sets the bound depth-stencil state handle for encoding.
    pub fn set_encode_depth_stencil_state(&mut self, dsso: u64) {
        self.update_encode_snapshot(|snap| {
            snap.dsso = dsso;
        });
        self.dirty.insert(DirtyFlags::DEPTH_STENCIL_STATE);
    }

    /// Sets the bound rasterizer state parameters for encoding.
    pub fn set_encode_rasterizer_state(&mut self, rs: RasterizerStateCmd) {
        self.update_encode_snapshot(|snap| {
            snap.rasterizer = rs;
        });
        self.dirty.insert(DirtyFlags::RASTERIZER_STATE);
    }

    /// Sets the bound viewports for encoding.
    pub fn set_encode_viewports(&mut self, viewports: Vec<WMTViewport>) {
        self.update_encode_snapshot(|snap| {
            snap.viewports = viewports;
        });
        self.dirty.insert(DirtyFlags::VIEWPORTS);
    }

    /// Sets the bound scissor rects for encoding.
    pub fn set_encode_scissor_rects(&mut self, rects: Vec<WMTScissorRect>) {
        self.update_encode_snapshot(|snap| {
            snap.scissor_rects = rects;
        });
        self.dirty.insert(DirtyFlags::SCISSOR_RECTS);
    }

    /// Sets the bound index buffer handle, DXGI format, and byte offset for encoding.
    pub fn set_encode_index_buffer(&mut self, buffer: u64, format: u32, offset: u32) {
        self.update_encode_snapshot(|snap| {
            snap.index_buffer = buffer;
            snap.index_format = format;
            snap.index_sub_offset = offset;
        });
        self.dirty.insert(DirtyFlags::INDEX_BUFFER);
    }

    /// Returns the number of times `invalidate_current_pass(true)` promoted a flush.
    #[must_use]
    pub fn promote_flush_count(&self) -> u32 {
        with_session(self, |s| s.promote_flush_count)
    }

    /// Returns the recorded passes (`(kind, head_ptr)`) for this context.
    #[must_use]
    pub fn recorded_passes(&self) -> Vec<(RecordedPassKind, *const WmtcmdBase)> {
        with_session(self, |s| {
            let mut out: Vec<(RecordedPassKind, *const WmtcmdBase)> = s
                .completed_passes
                .iter()
                .map(|(k, addr)| {
                    (
                        k.copy_kind(),
                        ptr::with_exposed_provenance::<WmtcmdBase>(*addr),
                    )
                })
                .collect();
            if let Some(active) = s.active_pass.as_ref() {
                out.push((active.kind.copy_kind(), active.head.as_ptr().cast_const()));
            }
            out
        })
    }

    /// Clears recorded passes for this context.
    pub fn clear_recorded_passes(&mut self) {
        with_session(self, |s| {
            s.active_pass = None;
            s.completed_passes.clear();
            s.commands.reset();
        });
    }
}

#[cfg(test)]
mod tests;
