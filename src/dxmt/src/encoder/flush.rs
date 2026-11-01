//! The flush: a chunk's passes, merged, encoded into its command buffer in order.
//!
//! Each pass becomes one Metal encoder (two blit encoders around an upscale, none for an event or
//! a timestamp without a sample buffer). Its fence waits, the argument buffer's bindings and its
//! records go to Metal in one call, its fence updates in a second: the records and their order
//! are the C++'s, which hands each fence operation and binding to Metal in a call of its own.
//! The queue's shared event is signalled last.
//!
//! Behaviour is the C++ `flushCommands`' (`dxmt_context.cpp:873-1283`).

use core::{ffi::CStr, mem, ptr::NonNull};

use winemetal_sys::{
    MTLBlitCommandEncoder_encodeCommands, MTLCommandBuffer_blitCommandEncoder,
    MTLCommandBuffer_blitCommandEncoderWithSampleBuffers, MTLCommandBuffer_computeCommandEncoder,
    MTLCommandBuffer_encodeSignalEvent, MTLCommandBuffer_encodeSpatialScale,
    MTLCommandBuffer_encodeTemporalScale, MTLCommandBuffer_encodeWaitForEvent,
    MTLCommandBuffer_presentDrawable, MTLCommandBuffer_presentDrawableAfterMinimumDuration,
    MTLCommandBuffer_renderCommandEncoder, MTLCommandEncoder_endEncoding,
    MTLCommandEncoder_setLabel, MTLComputeCommandEncoder_encodeCommands,
    MTLRenderCommandEncoder_encodeCommands, NSString_string, Obj,
    wire::{
        SM50_BINDING_INDEX_ARGUMENT_TABLE, SM50_BINDING_INDEX_ARGUMENT_TABLE2,
        SM50_BINDING_INDEX_CONSTANT_BUFFER, SM50_BINDING_INDEX_CONSTANT_BUFFER2,
        SM50_BINDING_INDEX_DRAW_ARGUMENTS, SM50_BINDING_INDEX_STREAM_OUTPUT0,
        SM50_BINDING_INDEX_VERTEX_BUFFER, WMT_BARRIER_SCOPE_BUFFERS, WMT_BLIT_COMMAND_FILL_BUFFER,
        WMT_BLIT_COMMAND_UPDATE_FENCE, WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
        WMT_COMPUTE_COMMAND_SET_BUFFER, WMT_COMPUTE_COMMAND_UPDATE_FENCE,
        WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE, WMT_LOAD_ACTION_CLEAR, WMT_LOAD_ACTION_LOAD,
        WMT_RENDER_COMMAND_MEMORY_BARRIER, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER,
        WMT_RENDER_COMMAND_SET_MESH_BUFFER, WMT_RENDER_COMMAND_SET_OBJECT_BUFFER,
        WMT_RENDER_COMMAND_SET_VERTEX_BUFFER, WMT_RENDER_COMMAND_UPDATE_FENCE,
        WMT_RENDER_COMMAND_USE_RESOURCE, WMT_RENDER_COMMAND_WAIT_FOR_FENCE,
        WMT_RENDER_STAGE_FRAGMENT, WMT_RENDER_STAGE_MESH, WMT_RENDER_STAGE_OBJECT,
        WMT_RENDER_STAGE_PRE_RASTER, WMT_RENDER_STAGE_VERTEX, WMT_RESOURCE_USAGE_READ,
        WMT_RESOURCE_USAGE_WRITE, WMT_STORE_ACTION_STORE,
        WMT_STORE_ACTION_STORE_AND_MULTISAMPLE_RESOLVE, WMTClearColor, WMTColorAttachmentInfo,
        WMTDepthAttachmentInfo, WMTRenderPassInfo, WMTRenderPassSampleBufferInfo,
        WMTSampleBufferAttachmentInfo, WMTStencilAttachmentInfo, WMTUTF8_STRING_ENCODING,
        WmtcmdBase, WmtcmdBlitFenceOp, WmtcmdBlitFillbuffer, WmtcmdComputeFenceOp,
        WmtcmdComputeSetbuffer, WmtcmdRenderFenceOp, WmtcmdRenderMemoryBarrier,
        WmtcmdRenderSetbuffer, WmtcmdRenderUseresource,
    },
};

use super::{
    Encoder, NO_NEXT, Record, binding_index, mem_ptr,
    pass::{
        Clear, Pass, PassKind, Present, RenderTargets, RenderUses, Resolve, Target, argument_buffer,
    },
    place,
};
use crate::{arena::Commands, deptrack::FenceSet, scaler::Scaler};

/// The buffers the chunk's queries are written to, which their readbacks made before the flush.
///
/// 0 where the chunk has none.
pub struct QueryBuffers {
    /// The visibility-result buffer the render passes that count samples write.
    pub visibility: u64,
    /// The counter sample buffer timestamps are sampled into.
    pub timestamps: u64,
}

impl QueryBuffers {
    /// A chunk without queries.
    pub const NONE: Self = Self {
        visibility: 0,
        timestamps: 0,
    };
}

impl Encoder {
    /// Encodes the chunk's passes into `cmdbuf`, then signals `event` at `event_seq`.
    ///
    /// The passes are merged first where nothing between depends on them (`optimize`); the
    /// queue's own event, which a pass may signal too, is `event`.
    ///
    /// # Safety
    ///
    /// `cmdbuf` is a live command buffer or nil; `event` and the buffers in `queries` are live or
    /// 0; the Metal objects the passes name are alive (the chunk keeps their allocations, the
    /// passes their scalers, presenters and events).
    pub unsafe fn flush(
        &mut self,
        cmdbuf: u64,
        event: u64,
        event_seq: u64,
        queries: &QueryBuffers,
    ) {
        debug_assert!(self.pass.is_none(), "a flush between passes");
        self.optimize();
        let mut passes = mem::take(&mut self.passes);
        for pass in &passes {
            // SAFETY: the caller's promise.
            unsafe { self.encode(cmdbuf, event, pass, queries) };
        }
        passes.clear();
        self.passes = passes;
        // SAFETY: the caller's promise.
        unsafe { MTLCommandBuffer_encodeSignalEvent(cmdbuf, event, event_seq) };
    }

    /// # Safety
    ///
    /// As [`Encoder::flush`].
    unsafe fn encode(&mut self, cmdbuf: u64, event: u64, pass: &Pass, queries: &QueryBuffers) {
        match &pass.kind {
            PassKind::Null => {}
            PassKind::Render(targets) => {
                // SAFETY: the caller's promise.
                unsafe { self.encode_render(cmdbuf, pass, targets, queries.visibility) };
            }
            PassKind::Compute => {
                // SAFETY: the caller's promise.
                unsafe { self.encode_compute(cmdbuf, pass) };
            }
            PassKind::Blit => {
                // SAFETY: the caller's promise.
                unsafe { self.encode_blit(cmdbuf, pass) };
            }
            PassKind::Clear(clear) => {
                // SAFETY: the caller's promise.
                unsafe { self.encode_clear(cmdbuf, pass, clear) };
            }
            PassKind::Resolve(resolve) => {
                // SAFETY: the caller's promise.
                unsafe { self.encode_resolve(cmdbuf, pass, resolve) };
            }
            PassKind::Present(present) => {
                // SAFETY: the caller's promise.
                unsafe { self.encode_present(cmdbuf, pass, present) };
            }
            PassKind::SpatialUpscale(u) => {
                let scale = |scaler, fence| {
                    // SAFETY: the caller's promise.
                    unsafe {
                        MTLCommandBuffer_encodeSpatialScale(
                            cmdbuf,
                            scaler,
                            u.backbuffer,
                            u.upscaled,
                            fence,
                        );
                    };
                };
                // SAFETY: the caller's promise.
                unsafe { self.encode_scaled(cmdbuf, pass, &u.scaler, scale) };
            }
            PassKind::TemporalUpscale(u) => {
                let scale = |scaler, fence| {
                    // SAFETY: the caller's promise; the properties are alive for the call.
                    unsafe {
                        MTLCommandBuffer_encodeTemporalScale(
                            cmdbuf,
                            scaler,
                            u.input,
                            u.output,
                            u.depth,
                            u.motion_vector,
                            u.exposure,
                            fence,
                            &raw const u.props,
                            0,
                        );
                    };
                };
                // SAFETY: the caller's promise.
                unsafe { self.encode_scaled(cmdbuf, pass, &u.scaler, scale) };
            }
            PassKind::SignalEvent(on, value) => {
                let on = on.as_ref().map_or(event, Obj::handle);
                // SAFETY: the caller's promise.
                unsafe { MTLCommandBuffer_encodeSignalEvent(cmdbuf, on, *value) };
            }
            PassKind::WaitForEvent(on, value) => {
                // SAFETY: the caller's promise.
                unsafe { MTLCommandBuffer_encodeWaitForEvent(cmdbuf, on.handle(), *value) };
            }
            PassKind::SampleTimestamp(index) => {
                if queries.timestamps != 0 {
                    // SAFETY: the caller's promise.
                    unsafe { self.encode_timestamp(cmdbuf, *index, queries.timestamps) };
                }
                // Without a sample buffer the command buffer's GPU end time is the timestamp.
            }
        }
    }

    /// # Safety
    ///
    /// As [`Encoder::flush`].
    unsafe fn encode_render(
        &mut self,
        cmdbuf: u64,
        pass: &Pass,
        targets: &RenderTargets,
        visibility: u64,
    ) {
        let mut info = render_pass_info(targets);
        if targets.uses.contains(RenderUses::VISIBILITY_RESULT) {
            debug_assert!(
                visibility != 0,
                "a visibility buffer for a pass that counts"
            );
            info.visibility_buffer = visibility;
        }
        // SAFETY: the caller's promise; the info is alive for the call.
        let encoder = unsafe { MTLCommandBuffer_renderCommandEncoder(cmdbuf, &raw mut info) };
        let Self { heap, fences, .. } = self;
        let mut chain = render_waits(heap, fences, pass);
        let buffer = argument_buffer(pass.arguments.as_ref());
        for (r#type, index) in render_bindings(pass.emulated_stream_output, targets.uses) {
            chain.push(
                heap,
                WmtcmdRenderSetbuffer {
                    r#type,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    buffer,
                    offset: 0,
                    index: binding_index(index),
                },
            );
        }
        for task in &targets.gs_marshal_tasks {
            chain.push(
                heap,
                WmtcmdRenderUseresource {
                    r#type: WMT_RENDER_COMMAND_USE_RESOURCE,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    resource: task.draw_args_buf,
                    usage: WMT_RESOURCE_USAGE_READ,
                    stages: WMT_RENDER_STAGE_VERTEX,
                },
            );
            chain.push(
                heap,
                WmtcmdRenderUseresource {
                    r#type: WMT_RENDER_COMMAND_USE_RESOURCE,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    resource: task.dispatch_args_buf,
                    usage: WMT_RESOURCE_USAGE_WRITE,
                    stages: WMT_RENDER_STAGE_VERTEX,
                },
            );
        }
        for task in &targets.ts_marshal_tasks {
            chain.push(
                heap,
                WmtcmdRenderUseresource {
                    r#type: WMT_RENDER_COMMAND_USE_RESOURCE,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    resource: task.draw_args_buf,
                    usage: WMT_RESOURCE_USAGE_READ,
                    stages: WMT_RENDER_STAGE_VERTEX,
                },
            );
            chain.push(
                heap,
                WmtcmdRenderUseresource {
                    r#type: WMT_RENDER_COMMAND_USE_RESOURCE,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    resource: task.dispatch_args_buf,
                    usage: WMT_RESOURCE_USAGE_WRITE,
                    stages: WMT_RENDER_STAGE_VERTEX,
                },
            );
        }
        if !targets.gs_marshal_tasks.is_empty() || !targets.ts_marshal_tasks.is_empty() {
            chain.push(
                heap,
                WmtcmdRenderMemoryBarrier {
                    r#type: WMT_RENDER_COMMAND_MEMORY_BARRIER,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    scope: WMT_BARRIER_SCOPE_BUFFERS,
                    stages_before: WMT_RENDER_STAGE_VERTEX,
                    stages_after: WMT_RENDER_STAGE_VERTEX
                        | WMT_RENDER_STAGE_MESH
                        | WMT_RENDER_STAGE_OBJECT,
                },
            );
        }
        chain.link(pass.records.head);
        // SAFETY: the encoder just made; the records live in the encoder's memory.
        unsafe { MTLRenderCommandEncoder_encodeCommands(encoder, chain.first()) };
        let updates = render_updates(heap, fences, pass);
        // SAFETY: the same encoder; the records live in the encoder's memory.
        unsafe { render_chain(encoder, &updates) };
        // SAFETY: the same encoder, ended once after its commands.
        unsafe { MTLCommandEncoder_endEncoding(encoder) };
    }

    /// # Safety
    ///
    /// As [`Encoder::flush`].
    unsafe fn encode_compute(&mut self, cmdbuf: u64, pass: &Pass) {
        // SAFETY: the caller's promise.
        let encoder = unsafe { MTLCommandBuffer_computeCommandEncoder(cmdbuf, true) };
        let Self { heap, fences, .. } = self;
        let mut chain = Chain::new();
        compute_fences(
            &mut chain,
            heap,
            fences,
            &pass.fence_wait,
            WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE,
        );
        let buffer = argument_buffer(pass.arguments.as_ref());
        for index in [
            SM50_BINDING_INDEX_CONSTANT_BUFFER,
            SM50_BINDING_INDEX_ARGUMENT_TABLE,
        ] {
            chain.push(
                heap,
                WmtcmdComputeSetbuffer {
                    r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    buffer,
                    offset: 0,
                    index: binding_index(index),
                },
            );
        }
        chain.link(pass.records.head);
        let mut updates = Chain::new();
        compute_fences(
            &mut updates,
            heap,
            fences,
            &pass.fence_update,
            WMT_COMPUTE_COMMAND_UPDATE_FENCE,
        );
        // SAFETY: the encoder just made; the records live in the encoder's memory.
        unsafe { MTLComputeCommandEncoder_encodeCommands(encoder, chain.first()) };
        if let Some(first) = updates.head {
            // SAFETY: as above.
            unsafe { MTLComputeCommandEncoder_encodeCommands(encoder, first.as_ptr()) };
        }
        // SAFETY: the same encoder, ended once after its commands.
        unsafe { MTLCommandEncoder_endEncoding(encoder) };
    }

    /// # Safety
    ///
    /// As [`Encoder::flush`].
    unsafe fn encode_blit(&mut self, cmdbuf: u64, pass: &Pass) {
        // SAFETY: the caller's promise.
        let encoder = unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf) };
        let Self { heap, fences, .. } = self;
        let mut chain = Chain::new();
        blit_fences(
            &mut chain,
            heap,
            fences,
            &pass.fence_wait,
            WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
        );
        chain.link(pass.records.head);
        let mut updates = Chain::new();
        blit_fences(
            &mut updates,
            heap,
            fences,
            &pass.fence_update,
            WMT_BLIT_COMMAND_UPDATE_FENCE,
        );
        // SAFETY: the encoder just made; the records live in the encoder's memory.
        unsafe { MTLBlitCommandEncoder_encodeCommands(encoder, chain.first()) };
        // SAFETY: as above.
        unsafe { blit_chain(encoder, &updates) };
        // SAFETY: the same encoder, ended once after its commands.
        unsafe { MTLCommandEncoder_endEncoding(encoder) };
    }

    /// # Safety
    ///
    /// As [`Encoder::flush`].
    unsafe fn encode_clear(&mut self, cmdbuf: u64, pass: &Pass, clear: &Clear) {
        let mut info = initial_render_pass_info();
        let view = clear.attachment.as_ref().map_or(0, |a| a.view);
        if clear.planes == 0 {
            info.colors[0].clear_color = clear.color;
            info.colors[0].texture = view;
            info.colors[0].load_action = WMT_LOAD_ACTION_CLEAR;
            info.colors[0].store_action = WMT_STORE_ACTION_STORE;
        } else {
            if clear.planes & 1 != 0 {
                info.depth.clear_depth = clear.depth;
                info.depth.texture = view;
                info.depth.load_action = WMT_LOAD_ACTION_CLEAR;
                info.depth.store_action = WMT_STORE_ACTION_STORE;
            }
            if clear.planes & 2 != 0 {
                info.stencil.clear_stencil = clear.stencil;
                info.stencil.texture = view;
                info.stencil.load_action = WMT_LOAD_ACTION_CLEAR;
                info.stencil.store_action = WMT_STORE_ACTION_STORE;
            }
            info.render_target_width = clear.width;
            info.render_target_height = clear.height;
        }
        info.render_target_array_length = low_byte(clear.array_length);
        // SAFETY: the caller's promise.
        unsafe { self.encode_fragment_pass(cmdbuf, pass, &mut info, c"ClearPass") };
    }

    /// # Safety
    ///
    /// As [`Encoder::flush`].
    unsafe fn encode_resolve(&mut self, cmdbuf: u64, pass: &Pass, resolve: &Resolve) {
        let mut info = initial_render_pass_info();
        info.colors[0].texture = resolve.src.as_ref().map_or(0, |a| a.view);
        info.colors[0].load_action = WMT_LOAD_ACTION_LOAD;
        info.colors[0].store_action = WMT_STORE_ACTION_STORE_AND_MULTISAMPLE_RESOLVE;
        info.colors[0].resolve_texture = resolve.dst.as_ref().map_or(0, |a| a.view);
        // SAFETY: the caller's promise.
        unsafe { self.encode_fragment_pass(cmdbuf, pass, &mut info, c"ResolvePass") };
    }

    /// A render pass of `info` that only waits for and updates the pass's fences, at fragment.
    ///
    /// # Safety
    ///
    /// As [`Encoder::flush`].
    unsafe fn encode_fragment_pass(
        &mut self,
        cmdbuf: u64,
        pass: &Pass,
        info: &mut WMTRenderPassInfo,
        name: &CStr,
    ) {
        // SAFETY: the caller's promise; the info is alive for the call.
        let encoder = unsafe { MTLCommandBuffer_renderCommandEncoder(cmdbuf, info) };
        // SAFETY: the encoder just made.
        unsafe { label(encoder, name) };
        let Self { heap, fences, .. } = self;
        let mut chain = Chain::new();
        render_fences(
            &mut chain,
            heap,
            fences,
            &pass.fence_wait,
            WMT_RENDER_COMMAND_WAIT_FOR_FENCE,
        );
        render_fences(
            &mut chain,
            heap,
            fences,
            &pass.fence_update,
            WMT_RENDER_COMMAND_UPDATE_FENCE,
        );
        // SAFETY: the same encoder; the records live in the encoder's memory.
        unsafe { render_chain(encoder, &chain) };
        // SAFETY: the same encoder, ended once after its commands.
        unsafe { MTLCommandEncoder_endEncoding(encoder) };
    }

    /// # Safety
    ///
    /// As [`Encoder::flush`].
    unsafe fn encode_present(&mut self, cmdbuf: u64, pass: &Pass, present: &Present) {
        let Self { heap, fences, .. } = self;
        let mut waits = Chain::new();
        render_fences(
            &mut waits,
            heap,
            fences,
            &pass.fence_wait,
            WMT_RENDER_COMMAND_WAIT_FOR_FENCE,
        );
        let mut updates = Chain::new();
        render_fences(
            &mut updates,
            heap,
            fences,
            &pass.fence_update,
            WMT_RENDER_COMMAND_UPDATE_FENCE,
        );
        // SAFETY: `encode` hands each callback the present pass's live encoder; the records live
        // in the encoder's memory.
        let wait = |encoder| unsafe { render_chain(encoder, &waits) };
        // SAFETY: as above.
        let update = |encoder| unsafe { render_chain(encoder, &updates) };
        // SAFETY: the caller's promise for the command buffer and the backbuffer.
        let drawable = unsafe {
            present
                .presenter
                .encode(cmdbuf, present.backbuffer, &present.metadata, wait, update)
        };
        let drawable = drawable.as_ref().map_or(0, Obj::handle);
        // ponytail: the time `encode` waited for the drawable is FrameStatistics', 5b's.
        if present.after > 0.0 {
            // SAFETY: the caller's promise; the drawable is alive until the end of the scope.
            unsafe {
                MTLCommandBuffer_presentDrawableAfterMinimumDuration(
                    cmdbuf,
                    drawable,
                    present.after,
                );
            };
        } else {
            // SAFETY: as above.
            unsafe { MTLCommandBuffer_presentDrawable(cmdbuf, drawable) };
        }
    }

    /// An upscale, `scale(scaler, fence)`, between two blit encoders.
    ///
    /// The first waits for the pass's fences and updates the scaler's, the second waits for the
    /// scaler's and updates the pass's.
    ///
    /// # Safety
    ///
    /// As [`Encoder::flush`].
    unsafe fn encode_scaled(
        &mut self,
        cmdbuf: u64,
        pass: &Pass,
        scaler: &Scaler,
        scale: impl FnOnce(u64, u64),
    ) {
        let fence = scaler.fence().map_or(0, Obj::handle);
        let Self { heap, fences, .. } = self;
        let mut begin = Chain::new();
        blit_fences(
            &mut begin,
            heap,
            fences,
            &pass.fence_wait,
            WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
        );
        begin.push(heap, blit_fence(WMT_BLIT_COMMAND_UPDATE_FENCE, fence));
        let mut end = Chain::new();
        end.push(heap, blit_fence(WMT_BLIT_COMMAND_WAIT_FOR_FENCE, fence));
        blit_fences(
            &mut end,
            heap,
            fences,
            &pass.fence_update,
            WMT_BLIT_COMMAND_UPDATE_FENCE,
        );
        // SAFETY: the caller's promise.
        unsafe { blit_pass(cmdbuf, c"BeginScaler", &begin) };
        scale(scaler.scaler().map_or(0, Obj::handle), fence);
        // SAFETY: the caller's promise.
        unsafe { blit_pass(cmdbuf, c"EndScaler", &end) };
    }

    /// A blit encoder that samples the timestamp at its start into `sample_buffer`'s `index`.
    ///
    /// # Safety
    ///
    /// As [`Encoder::flush`]; `sample_buffer` is a live counter sample buffer.
    unsafe fn encode_timestamp(&mut self, cmdbuf: u64, index: u64, sample_buffer: u64) {
        // Metal may run the encoders out of order: the passes before are waited for first.
        // SAFETY: the caller's promise.
        unsafe { self.barrier_on_queue(cmdbuf) };
        let mut info = WMTSampleBufferAttachmentInfo {
            sample_buffer,
            start_of_encoder_sample_index: index,
            // `MTLCounterDontSample`.
            end_of_encoder_sample_index: u64::MAX,
        };
        // SAFETY: the caller's promise; the info is alive for the call.
        let encoder = unsafe {
            MTLCommandBuffer_blitCommandEncoderWithSampleBuffers(cmdbuf, &raw mut info, 1)
        };
        // An empty blit encoder samples nothing: it fills 4 bytes of the zeroed buffer with 0.
        let fill = WmtcmdBlitFillbuffer {
            r#type: WMT_BLIT_COMMAND_FILL_BUFFER,
            reserved: [0; 3],
            next: NO_NEXT,
            buffer: self.dummy_cbuffer.as_ref().map_or(0, |b| b.buffer()),
            offset: 0,
            length: 4,
            value: 0,
        };
        // SAFETY: the encoder just made.
        unsafe { label(encoder, c"SampleTimestamp") };
        let fill = (&raw const fill).cast::<WmtcmdBase>();
        // SAFETY: the same encoder; the record is alive for the call, led by `wmtcmd_base`'s
        // fields.
        unsafe { MTLBlitCommandEncoder_encodeCommands(encoder, fill) };
        // SAFETY: the same encoder, ended once after its command.
        unsafe { MTLCommandEncoder_endEncoding(encoder) };
    }

    /// Makes the command buffer wait, at this point, for everything encoded before.
    ///
    /// # Safety
    ///
    /// `cmdbuf` is a live command buffer or nil.
    unsafe fn barrier_on_queue(&mut self, cmdbuf: u64) {
        self.barrier_index += 1;
        let event = self.barrier_event.as_ref().map_or(0, Obj::handle);
        // SAFETY: the caller's promise; the event is the encoder's, live or nil.
        unsafe { MTLCommandBuffer_encodeSignalEvent(cmdbuf, event, self.barrier_index) };
        // SAFETY: as above.
        unsafe { MTLCommandBuffer_encodeWaitForEvent(cmdbuf, event, self.barrier_index) };
    }
}

/// Records linked in the encoder's memory, handed to Metal with one call.
struct Chain {
    head: Option<NonNull<WmtcmdBase>>,
    tail: Option<NonNull<WmtcmdBase>>,
}

impl Chain {
    const fn new() -> Self {
        Self {
            head: None,
            tail: None,
        }
    }

    /// Places `cmd` (its `next` null) in `heap` and links it last.
    fn push<T: Record>(&mut self, heap: &mut Commands, cmd: T) {
        self.link(place(heap, cmd).cast::<WmtcmdBase>());
    }

    /// Links `record` last, and so the records it leads: nothing is pushed after a pass's list.
    fn link(&mut self, record: NonNull<WmtcmdBase>) {
        match self.tail {
            // SAFETY: the chain's last record, in the encoder's memory until its next reset.
            Some(tail) => unsafe { (*tail.as_ptr()).next = mem_ptr(record) },
            None => self.head = Some(record),
        }
        self.tail = Some(record);
    }

    /// The first record, or null for an empty chain.
    fn first(&self) -> *const WmtcmdBase {
        self.head
            .map_or(core::ptr::null(), |h| h.as_ptr().cast_const())
    }
}

/// `InitializeRenderPassInfo`'s render pass descriptor.
///
/// No targets; colours stored and cleared to opaque black, depth and stencil cleared (depth to 1).
const fn initial_render_pass_info() -> WMTRenderPassInfo {
    let color = WMTColorAttachmentInfo {
        texture: 0,
        load_action: 0,
        store_action: WMT_STORE_ACTION_STORE,
        level: 0,
        slice: 0,
        depth_plane: 0,
        clear_color: WMTClearColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
        resolve_texture: 0,
        resolve_level: 0,
        resolve_slice: 0,
        resolve_depth_plane: 0,
    };
    WMTRenderPassInfo {
        colors: [color; 8],
        depth: WMTDepthAttachmentInfo {
            texture: 0,
            load_action: WMT_LOAD_ACTION_CLEAR,
            store_action: 0,
            level: 0,
            slice: 0,
            depth_plane: 0,
            clear_depth: 1.0,
        },
        stencil: WMTStencilAttachmentInfo {
            texture: 0,
            load_action: WMT_LOAD_ACTION_CLEAR,
            store_action: 0,
            level: 0,
            slice: 0,
            depth_plane: 0,
            clear_stencil: 0,
        },
        default_raster_sample_count: 0,
        render_target_array_length: 0,
        tile_width: 0,
        tile_height: 0,
        render_target_height: 0,
        render_target_width: 0,
        visibility_accumulate: 0,
        visibility_buffer: 0,
        sample_buffers: [WMTRenderPassSampleBufferInfo {
            sample_buffer: 0,
            end_of_fragment_sample_index: 0,
            start_of_vertex_sample_index: 0,
        }; 4],
    }
}

/// The render pass descriptor of `targets`: each target that has a view, and the sizes.
fn render_pass_info(targets: &RenderTargets) -> WMTRenderPassInfo {
    let mut info = initial_render_pass_info();
    for (color, out) in targets.colors.iter().zip(&mut info.colors) {
        let Some(view) = &color.target.attachment else {
            continue;
        };
        let t = &color.target;
        *out = WMTColorAttachmentInfo {
            texture: view.view,
            load_action: t.load_action,
            store_action: t.store_action,
            level: t.level,
            slice: t.slice,
            depth_plane: t.depth_plane,
            clear_color: color.clear_color,
            resolve_texture: color.resolve.as_ref().map_or(0, |a| a.view),
            resolve_level: color.resolve_level,
            resolve_slice: color.resolve_slice,
            resolve_depth_plane: color.resolve_depth_plane,
        };
    }
    if let Some(view) = target_view(&targets.depth.target) {
        let t = &targets.depth.target;
        info.depth = WMTDepthAttachmentInfo {
            texture: view,
            load_action: t.load_action,
            store_action: t.store_action,
            level: t.level,
            slice: t.slice,
            depth_plane: t.depth_plane,
            clear_depth: targets.depth.clear_depth,
        };
    }
    if let Some(view) = target_view(&targets.stencil.target) {
        let t = &targets.stencil.target;
        info.stencil = WMTStencilAttachmentInfo {
            texture: view,
            load_action: t.load_action,
            store_action: t.store_action,
            level: t.level,
            slice: t.slice,
            depth_plane: t.depth_plane,
            clear_stencil: targets.stencil.clear_stencil,
        };
    }
    info.default_raster_sample_count = targets.default_raster_sample_count;
    info.render_target_array_length = low_byte(u32::from(targets.render_target_array_length));
    info.render_target_width = targets.render_target_width;
    info.render_target_height = targets.render_target_height;
    info
}

fn target_view(target: &Target) -> Option<u64> {
    target.attachment.as_ref().map(|a| a.view)
}

/// The argument buffer's bindings a render pass sets, each `(record type, binding index)`.
fn render_bindings(emulated: bool, uses: RenderUses) -> impl Iterator<Item = (u16, u32)> {
    let vertex = WMT_RENDER_COMMAND_SET_VERTEX_BUFFER;
    let fragment = WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER;
    let object = WMT_RENDER_COMMAND_SET_OBJECT_BUFFER;
    let mesh = WMT_RENDER_COMMAND_SET_MESH_BUFFER;
    let staged = uses.intersects(RenderUses::GEOMETRY | RenderUses::TESSELLATION);
    let tessellation = uses.contains(RenderUses::TESSELLATION);
    [
        (vertex, SM50_BINDING_INDEX_VERTEX_BUFFER, true),
        (vertex, SM50_BINDING_INDEX_CONSTANT_BUFFER, true),
        (vertex, SM50_BINDING_INDEX_ARGUMENT_TABLE, true),
        (vertex, SM50_BINDING_INDEX_STREAM_OUTPUT0, emulated),
        (fragment, SM50_BINDING_INDEX_CONSTANT_BUFFER, true),
        (fragment, SM50_BINDING_INDEX_ARGUMENT_TABLE, true),
        (object, SM50_BINDING_INDEX_VERTEX_BUFFER, staged),
        (object, SM50_BINDING_INDEX_DRAW_ARGUMENTS, staged),
        (object, SM50_BINDING_INDEX_CONSTANT_BUFFER2, tessellation),
        (object, SM50_BINDING_INDEX_ARGUMENT_TABLE2, tessellation),
        (object, SM50_BINDING_INDEX_CONSTANT_BUFFER, staged),
        (object, SM50_BINDING_INDEX_ARGUMENT_TABLE, staged),
        (mesh, SM50_BINDING_INDEX_CONSTANT_BUFFER, staged),
        (mesh, SM50_BINDING_INDEX_ARGUMENT_TABLE, staged),
    ]
    .into_iter()
    .filter_map(|(r#type, index, set)| set.then_some((r#type, index)))
}

/// A render pass's fence waits, in slot order.
///
/// Before pre-raster for the vertex half's, before the fragment stage for the others: a fence the
/// vertex half waits for is not waited for again.
fn render_waits(heap: &mut Commands, fences: &[Option<Obj>], pass: &Pass) -> Chain {
    let mut chain = Chain::new();
    pass.fence_wait
        .union(&pass.fence_wait_vertex)
        .for_each(|slot| {
            let stages = if pass.fence_wait_vertex.test(slot as u64) {
                WMT_RENDER_STAGE_PRE_RASTER
            } else {
                WMT_RENDER_STAGE_FRAGMENT
            };
            let fence = handle(fences, slot);
            chain.push(
                heap,
                render_fence(WMT_RENDER_COMMAND_WAIT_FOR_FENCE, fence, stages),
            );
        });
    chain
}

/// A render pass's fence updates, in slot order.
///
/// After the fragment stage for the fragment half's, after pre-raster for the vertex half's
/// others: a fence the fragment half updates is not updated by the vertex half too.
fn render_updates(heap: &mut Commands, fences: &[Option<Obj>], pass: &Pass) -> Chain {
    let mut chain = Chain::new();
    pass.fence_update_vertex
        .union(&pass.fence_update)
        .for_each(|slot| {
            let stages = if pass.fence_update.test(slot as u64) {
                WMT_RENDER_STAGE_FRAGMENT
            } else {
                WMT_RENDER_STAGE_PRE_RASTER
            };
            let fence = handle(fences, slot);
            chain.push(
                heap,
                render_fence(WMT_RENDER_COMMAND_UPDATE_FENCE, fence, stages),
            );
        });
    chain
}

/// Each fence of `set` as a render fence operation of `r#type` at fragment, pushed in slot order.
fn render_fences(
    chain: &mut Chain,
    heap: &mut Commands,
    fences: &[Option<Obj>],
    set: &FenceSet,
    r#type: u16,
) {
    set.for_each(|slot| {
        let fence = handle(fences, slot);
        chain.push(heap, render_fence(r#type, fence, WMT_RENDER_STAGE_FRAGMENT));
    });
}

fn compute_fences(
    chain: &mut Chain,
    heap: &mut Commands,
    fences: &[Option<Obj>],
    set: &FenceSet,
    r#type: u16,
) {
    set.for_each(|slot| {
        chain.push(
            heap,
            WmtcmdComputeFenceOp {
                r#type,
                reserved: [0; 3],
                next: NO_NEXT,
                fence: handle(fences, slot),
            },
        );
    });
}

fn blit_fences(
    chain: &mut Chain,
    heap: &mut Commands,
    fences: &[Option<Obj>],
    set: &FenceSet,
    r#type: u16,
) {
    set.for_each(|slot| chain.push(heap, blit_fence(r#type, handle(fences, slot))));
}

const fn render_fence(r#type: u16, fence: u64, stages: u8) -> WmtcmdRenderFenceOp {
    WmtcmdRenderFenceOp {
        r#type,
        reserved: [0; 3],
        next: NO_NEXT,
        fence,
        stages,
    }
}

const fn blit_fence(r#type: u16, fence: u64) -> WmtcmdBlitFenceOp {
    WmtcmdBlitFenceOp {
        r#type,
        reserved: [0; 3],
        next: NO_NEXT,
        fence,
    }
}

/// Fence `slot` of the pool, or the null handle.
fn handle(fences: &[Option<Obj>], slot: usize) -> u64 {
    fences
        .get(slot)
        .and_then(Option::as_ref)
        .map_or(0, Obj::handle)
}

/// `v`'s low byte: what the C++'s assignment to a `uint8_t` field keeps.
const fn low_byte(v: u32) -> u8 {
    v.to_le_bytes()[0]
}

/// Encodes `chain` on the render `encoder`, if it has records.
///
/// # Safety
///
/// `encoder` is a live render encoder; the chain's records are in the encoder's memory.
unsafe fn render_chain(encoder: u64, chain: &Chain) {
    if let Some(first) = chain.head {
        // SAFETY: the caller's promise.
        unsafe { MTLRenderCommandEncoder_encodeCommands(encoder, first.as_ptr()) };
    }
}

/// Encodes `chain` on the blit `encoder`, if it has records.
///
/// # Safety
///
/// `encoder` is a live blit encoder; the chain's records are in the encoder's memory.
unsafe fn blit_chain(encoder: u64, chain: &Chain) {
    if let Some(first) = chain.head {
        // SAFETY: the caller's promise.
        unsafe { MTLBlitCommandEncoder_encodeCommands(encoder, first.as_ptr()) };
    }
}

/// A blit encoder named `name` that encodes `chain`.
///
/// # Safety
///
/// `cmdbuf` is a live command buffer or nil; the chain's records are in the encoder's memory.
unsafe fn blit_pass(cmdbuf: u64, name: &CStr, chain: &Chain) {
    // SAFETY: the caller's promise.
    let encoder = unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf) };
    // SAFETY: the encoder just made.
    unsafe { label(encoder, name) };
    // SAFETY: the same encoder; the caller's promise for the records.
    unsafe { blit_chain(encoder, chain) };
    // SAFETY: the same encoder, ended once after its commands.
    unsafe { MTLCommandEncoder_endEncoding(encoder) };
}

/// Labels `encoder` `name`.
///
/// # Safety
///
/// `encoder` is a live encoder; an autorelease pool is pushed on the thread.
unsafe fn label(encoder: u64, name: &CStr) {
    // SAFETY: a NUL-terminated UTF-8 string, alive for the call.
    let string = unsafe { NSString_string(name.as_ptr(), WMTUTF8_STRING_ENCODING) };
    // SAFETY: the caller's promise; the string is autoreleased into the thread's pool.
    unsafe { MTLCommandEncoder_setLabel(encoder, string) };
}

#[cfg(test)]
mod tests;
