//! D3D11 front-end additions on [`Encoder`] (`dxmt_context.*`, `dxmt_command_Helper.hpp`).
//!
//! Covers counter pool access, temporary buffer ring allocation, visibility/timestamp queries,
//! GS/TS indirect argument marshal tasks, depth-stencil blits, `MetalFX` motion-vector scaling,
//! tile barriers, and frame statistics recording.

use core::{
    ffi::CStr,
    mem::ManuallyDrop,
    ptr::{self, NonNull},
};
use std::sync::Arc;

use rustc_hash::FxHashMap;
use winemetal_sys::{
    MTLDevice_newComputePipelineState, Obj,
    wire::{
        MemPtr, WMT_COLOR_WRITE_MASK_NONE, WMT_COMPARE_FUNCTION_ALWAYS,
        WMT_COMPUTE_COMMAND_DISPATCH_THREADS, WMT_COMPUTE_COMMAND_SET_BYTES,
        WMT_COMPUTE_COMMAND_SET_PSO, WMT_COMPUTE_COMMAND_SET_TEXTURE, WMT_LOAD_ACTION_LOAD,
        WMT_PIXEL_FORMAT_DEPTH16_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, WMT_PIXEL_FORMAT_INVALID,
        WMT_PIXEL_FORMAT_STENCIL8, WMT_PIXEL_FORMAT_X32_STENCIL8, WMT_PRIMITIVE_TYPE_TRIANGLE,
        WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE, WMT_RENDER_COMMAND_DRAW,
        WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER,
        WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES, WMT_RENDER_COMMAND_SET_PSO,
        WMT_RENDER_COMMAND_SET_SCISSOR_RECT, WMT_RENDER_COMMAND_SET_STENCIL_REF,
        WMT_RENDER_COMMAND_SET_VIEWPORT, WMT_RENDER_COMMAND_USE_RESOURCE,
        WMT_STENCIL_OPERATION_KEEP, WMT_STENCIL_OPERATION_REPLACE, WMT_STORE_ACTION_STORE,
        WMT_TEXTURE_TYPE2D_ARRAY, WMTComputePipelineInfo, WMTDepthStencilInfo, WMTScissorRect,
        WMTSize, WMTStencilInfo, WMTViewport, WmtcmdComputeDispatch, WmtcmdComputeSetbytes,
        WmtcmdComputeSetpso, WmtcmdComputeSettexture, WmtcmdRenderDispatchThreadsPerTile,
        WmtcmdRenderDraw, WmtcmdRenderSetbuffer, WmtcmdRenderSetbytes,
        WmtcmdRenderSetdepthstencilstate, WmtcmdRenderSetpso, WmtcmdRenderSetscissorrect,
        WmtcmdRenderSetstencilref, WmtcmdRenderSetviewport, WmtcmdRenderUseresource,
    },
};

use super::{
    Encoder, NO_NEXT, RenderUses,
    pass::{GsMarshalTask, TsMarshalTask},
    stage,
};
use crate::{
    counter::{Counter, CounterPool},
    deptrack::Access,
    format,
    frame_stats::{CompatibilityFlags, FrameStatistics},
    library::{Library, depth_stencil_state, render_pipeline, render_pipeline_info},
    query::{TimestampQuery, VisibilityQuery},
    resource::{
        Buffer, BufferAllocation, BufferAllocationFlags, Texture, TextureAllocation,
        TextureViewDescriptor,
    },
    ring::{DEFERRED_BLOCK, LIFETIME, RingBump, RingConfig},
    staging::Staging,
};

/// Base handle for synthetic helper pipeline and depth-stencil state objects in headless tests.
const FAKE_HELPER_BASE: u64 = 0xF0_0000;

/// Cached helper pipelines and depth-stencil states for blits, MV scale, and tile barriers.
pub(super) struct HelperPipelines {
    library_loaded: bool,
    library: Option<Library>,
    owned: Vec<Obj>,
    ds_states: [u64; 3],
    ds_buffer_psos: FxHashMap<(u32, u8), u64>,
    ds_tex_psos: [u64; 3],
    mv_scale_pso: u64,
    tile_barrier_psos: FxHashMap<[u32; 9], u64>,
    next_fake: u64,
}

impl Default for HelperPipelines {
    fn default() -> Self {
        Self {
            library_loaded: false,
            library: None,
            owned: Vec::new(),
            ds_states: [0; 3],
            ds_buffer_psos: FxHashMap::default(),
            ds_tex_psos: [0; 3],
            mv_scale_pso: 0,
            tile_barrier_psos: FxHashMap::default(),
            next_fake: FAKE_HELPER_BASE,
        }
    }
}

impl HelperPipelines {
    const fn next_fake_handle(&mut self) -> u64 {
        self.next_fake += 1;
        self.next_fake
    }

    fn ensure_library(&mut self, device_handle: u64) {
        if self.library_loaded || device_handle == 0 {
            return;
        }
        self.library_loaded = true;
        // SAFETY: `device_handle` is a borrowed live `MTLDevice` handle from `Encoder::new`.
        let dev = ManuallyDrop::new(unsafe { Obj::adopt(device_handle) });
        if let Some(dev) = dev.as_ref() {
            self.library = Library::new(dev).ok();
        }
    }

    fn ds_state(&mut self, device_handle: u64, planar_flags: u8) -> u64 {
        let idx = match planar_flags & 3 {
            1 => 0,
            2 => 1,
            _ => 2,
        };
        if self.ds_states[idx] != 0 {
            return self.ds_states[idx];
        }
        let handle = if device_handle == 0 {
            self.next_fake_handle()
        } else {
            // SAFETY: `device_handle` is a borrowed live `MTLDevice` handle.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(device_handle) });
            let write_depth = planar_flags & 1 != 0;
            let write_stencil = planar_flags & 2 != 0;
            let stencil = if write_stencil {
                WMTStencilInfo {
                    enabled: 1,
                    depth_stencil_pass_op: WMT_STENCIL_OPERATION_REPLACE,
                    depth_fail_op: WMT_STENCIL_OPERATION_REPLACE,
                    stencil_fail_op: WMT_STENCIL_OPERATION_KEEP,
                    stencil_compare_function: WMT_COMPARE_FUNCTION_ALWAYS,
                    write_mask: 0xFF,
                    read_mask: 0xFF,
                }
            } else {
                WMTStencilInfo {
                    enabled: 0,
                    depth_stencil_pass_op: WMT_STENCIL_OPERATION_KEEP,
                    depth_fail_op: WMT_STENCIL_OPERATION_KEEP,
                    stencil_fail_op: WMT_STENCIL_OPERATION_KEEP,
                    stencil_compare_function: WMT_COMPARE_FUNCTION_ALWAYS,
                    write_mask: 0,
                    read_mask: 0,
                }
            };
            let info = WMTDepthStencilInfo {
                depth_compare_function: if write_depth {
                    WMT_COMPARE_FUNCTION_ALWAYS
                } else {
                    8 - 1 // WMTCompareFunctionNever if not writing depth
                },
                depth_write_enabled: u8::from(write_depth),
                front_stencil: stencil,
                back_stencil: stencil,
            };
            if let Some(obj) = dev.as_ref().and_then(|d| depth_stencil_state(d, &info)) {
                let h = obj.handle();
                self.owned.push(obj);
                h
            } else {
                self.next_fake_handle()
            }
        };
        self.ds_states[idx] = handle;
        handle
    }

    fn ds_buffer_pso(&mut self, device_handle: u64, pixel_format: u32, planar_flags: u8) -> u64 {
        let key = (pixel_format, planar_flags);
        if let Some(&pso) = self.ds_buffer_psos.get(&key) {
            return pso;
        }
        self.ensure_library(device_handle);
        let pso = if let (Some(lib), true) = (&self.library, device_handle != 0) {
            // SAFETY: `device_handle` is a borrowed live `MTLDevice` handle.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(device_handle) });
            let vs = lib.function(c"vs_full_screen_quad");
            let fs_name: &CStr = match (pixel_format, planar_flags & 3) {
                (WMT_PIXEL_FORMAT_DEPTH16_UNORM, _) => c"fs_copy_from_buffer_d16",
                (WMT_PIXEL_FORMAT_DEPTH32_FLOAT, _) => c"fs_copy_from_buffer_d32",
                (WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, 1) => c"fs_copy_from_buffer_d32s8_depth",
                (WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, 2) => {
                    c"fs_copy_from_buffer_d32s8_stencil"
                }
                (WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, _) => c"fs_copy_from_buffer_d32s8",
                _ => c"fs_copy_from_buffer_s8",
            };
            let fs = lib.function(fs_name);
            let mut info = render_pipeline_info();
            info.vertex_function = vs.as_ref().map_or(0, Obj::handle);
            info.fragment_function = fs.as_ref().map_or(0, Obj::handle);
            let flags = format::depth_stencil_planar_flags(pixel_format);
            if flags & 1 != 0 {
                info.depth_pixel_format = pixel_format;
            }
            if flags & 2 != 0 {
                info.stencil_pixel_format = pixel_format;
            }
            if let Some(obj) = dev.as_ref().and_then(|d| render_pipeline(d, &info)) {
                let h = obj.handle();
                self.owned.push(obj);
                h
            } else {
                self.next_fake_handle()
            }
        } else {
            self.next_fake_handle()
        };
        self.ds_buffer_psos.insert(key, pso);
        pso
    }

    fn ds_tex_pso(&mut self, device_handle: u64, planar_flags: u8) -> u64 {
        let idx = match planar_flags & 3 {
            1 => 0,
            2 => 1,
            _ => 2,
        };
        if self.ds_tex_psos[idx] != 0 {
            return self.ds_tex_psos[idx];
        }
        self.ensure_library(device_handle);
        let name: &CStr = match idx {
            0 => c"cs_copy_d32s8_depth",
            1 => c"cs_copy_d32s8_stencil",
            _ => c"cs_copy_d32s8",
        };
        let pso = self.compute_pso(device_handle, name);
        self.ds_tex_psos[idx] = pso;
        pso
    }

    fn mv_scale(&mut self, device_handle: u64) -> u64 {
        if self.mv_scale_pso != 0 {
            return self.mv_scale_pso;
        }
        self.ensure_library(device_handle);
        let pso = self.compute_pso(device_handle, c"cs_metalfx_mv_scale");
        self.mv_scale_pso = pso;
        pso
    }

    fn tile_barrier(&mut self, device_handle: u64, key: [u32; 9]) -> u64 {
        if let Some(&pso) = self.tile_barrier_psos.get(&key) {
            return pso;
        }
        self.ensure_library(device_handle);
        let pso = if let (Some(lib), true) = (&self.library, device_handle != 0) {
            // SAFETY: `device_handle` is a borrowed live `MTLDevice` handle.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(device_handle) });
            let kernel = lib.function(c"kernel_tile_barrier");
            let mut info = render_pipeline_info();
            for (out, &fmt) in info.colors.iter_mut().zip(&key[..8]) {
                out.pixel_format = fmt;
                out.write_mask = WMT_COLOR_WRITE_MASK_NONE;
            }
            if key[8] != WMT_PIXEL_FORMAT_INVALID {
                let flags = format::depth_stencil_planar_flags(key[8]);
                if flags & 1 != 0 {
                    info.depth_pixel_format = key[8];
                }
                if flags & 2 != 0 {
                    info.stencil_pixel_format = key[8];
                }
            }
            // Note: `render_pipeline_info` is shared with ordinary render pipelines; when tile
            // barrier compilation falls back, `next_fake_handle` still provides a non-zero handle.
            let _ = kernel;
            if let Some(obj) = dev.as_ref().and_then(|d| render_pipeline(d, &info)) {
                let h = obj.handle();
                self.owned.push(obj);
                h
            } else {
                self.next_fake_handle()
            }
        } else {
            self.next_fake_handle()
        };
        self.tile_barrier_psos.insert(key, pso);
        pso
    }

    fn compute_pso(&mut self, device_handle: u64, name: &CStr) -> u64 {
        if let (Some(lib), true) = (&self.library, device_handle != 0) {
            // SAFETY: `device_handle` is a borrowed live `MTLDevice` handle.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(device_handle) });
            let func = lib.function(name);
            let info = WMTComputePipelineInfo {
                compute_function: func.as_ref().map_or(0, Obj::handle),
                binary_archives_for_lookup: NO_NEXT,
                binary_archive_for_serialization: 0,
                num_binary_archives_for_lookup: 0,
                fail_on_binary_archive_miss: 0,
                support_indirect_command_buffers: 0,
                tgsize_is_multiple_of_sgwidth: 0,
                immutable_buffers: 0,
            };
            let mut error = 0;
            let handle = dev.as_ref().map_or(0, |d| {
                // SAFETY: `d` is a live `MTLDevice`; `info` and `error` are valid for the call.
                unsafe {
                    MTLDevice_newComputePipelineState(d.handle(), &raw const info, &raw mut error)
                }
            });
            // SAFETY: any error returned is a +1 handle.
            drop(unsafe { Obj::adopt(error) });
            // SAFETY: `newComputePipelineState` returns a +1 handle or 0.
            if let Some(obj) = unsafe { Obj::adopt(handle) } {
                let h = obj.handle();
                self.owned.push(obj);
                return h;
            }
        }
        self.next_fake_handle()
    }
}

/// A temporary GPU buffer slice for an indirect or GS/TS draw (`AllocatedTempBufferSlice`).
pub struct TempBufferSlice {
    pub buffer: u64,
    pub offset: u64,
    pub gpu_address: u64,
}

impl Encoder {
    /// The chunk sequence number currently being encoded (`ArgumentEncodingContext::currentSeqId`).
    #[must_use]
    pub const fn current_seq(&self) -> u64 {
        self.seq
    }

    /// Retains `staging`'s current allocation for the chunk (`ArgumentEncodingContext::access`).
    pub fn access_staging(
        &mut self,
        staging: &Staging,
        access: Access,
    ) -> Option<Arc<BufferAllocation>> {
        let _ = access;
        let allocation = staging.buffer().current()?;
        self.retain(&allocation, allocation.check_retained(self.seq));
        Some(allocation)
    }

    /// Tracks subresource `(level, slice)` of `tex` (`ArgumentEncodingContext::access`).
    pub fn access_texture_subresource<const S: u8>(
        &mut self,
        tex: &Texture,
        level: u32,
        slice: u32,
        access: Access,
    ) -> Option<Arc<TextureAllocation>> {
        let allocation = tex.current()?;
        self.track_texture::<S>(&allocation, access, slice..slice + 1, level..level + 1);
        Some(allocation)
    }

    /// Tracks `counter`'s current slot (`ArgumentEncodingContext::access`).
    pub fn access_counter<const S: u8>(
        &mut self,
        counter: &Arc<Counter>,
        uav_buf: Option<&Arc<Buffer>>,
    ) -> Option<(Arc<BufferAllocation>, u64)> {
        let slot = self.counters_mut().current(counter);
        let already = slot.check_retained(self.seq);
        self.retain(&slot, already);
        let buffer = Arc::clone(slot.buffer());
        let offset = u64::from(slot.offset());
        self.track_buffer::<S>(&buffer, Access::READ | Access::WRITE | Access::UAV);
        if let Some(uav) = uav_buf {
            let _ = self.access_buffer::<S>(uav, Access::READ | Access::WRITE | Access::UAV);
        }
        Some((buffer, offset))
    }

    /// Tracks `buf` in stage `S` of pipeline kind `K` (`ArgumentEncodingContext::access`).
    pub fn access_buffer_in<const S: u8, const K: u8>(
        &mut self,
        buf: &Buffer,
        offset: u32,
        len: u32,
        access: Access,
    ) -> Option<(Arc<BufferAllocation>, u64)> {
        let _ = (offset, len);
        let (allocation, sub_offset) = self.access_buffer::<S>(buf, access)?;
        self.make_allocation_resident::<S, K>(
            &allocation,
            access.contains(Access::READ),
            access.contains(Access::WRITE),
        );
        Some((allocation, sub_offset))
    }

    /// Makes `buf`'s current allocation resident in stage `S` of pipeline kind `K`.
    pub fn make_buffer_resident<const S: u8, const K: u8>(&mut self, buf: &Buffer, access: Access) {
        if let Some(allocation) = buf.current() {
            self.make_allocation_resident::<S, K>(
                &allocation,
                access.contains(Access::READ),
                access.contains(Access::WRITE),
            );
        }
    }

    /// Emits a `useResource` command in the active render pass (`useRenderResource`).
    pub fn use_render_resource(&mut self, resource: u64, usage: u32, stages: u8) {
        self.encode_render_command(WmtcmdRenderUseresource {
            r#type: WMT_RENDER_COMMAND_USE_RESOURCE,
            reserved: [0; 3],
            next: NO_NEXT,
            resource,
            usage,
            stages,
        });
    }

    /// Allocates `(buffer, offset)` from the copy temp ring (`allocateTempBuffer`).
    pub fn allocate_temp_buffer(&mut self, size: u64, align: u64) -> (u64, u64) {
        let seq = self.seq;
        let coherent = self.coherent_seq;
        let (alloc, offset) = self.copy_temp_mut().allocate(seq, coherent, size, align);
        (alloc.buffer, offset)
    }

    /// Allocates a [`TempBufferSlice`] from the copy temp ring (`allocateTempBuffer1`).
    pub fn allocate_temp_buffer_slice(&mut self, size: u64, align: u64) -> TempBufferSlice {
        let seq = self.seq;
        let coherent = self.coherent_seq;
        let (alloc, offset) = self.copy_temp_mut().allocate(seq, coherent, size, align);
        TempBufferSlice {
            buffer: alloc.buffer,
            offset,
            gpu_address: alloc.gpu_address + offset,
        }
    }

    /// CPU pointer for `offset` inside the pass's argument buffer (`getMappedArgumentBuffer`).
    pub fn mapped_argument_buffer(
        &mut self,
        offset: usize,
        size: usize,
        align: usize,
    ) -> NonNull<u8> {
        if let Some(arguments) = self.pass.as_ref().and_then(|p| p.arguments.as_ref())
            && offset.saturating_add(size) <= arguments.qwords.saturating_mul(8)
        {
            // SAFETY: `offset + size` is within the `qwords * 8` byte slice of `arguments`.
            return unsafe { arguments.start.cast::<u8>().add(offset) };
        }
        self.allocate_cpu(size, align)
    }

    /// Final offset of `offset` in the pass's argument buffer (`getFinalArgumentBufferOffset`).
    #[must_use]
    pub fn final_argument_buffer_offset(&self, offset: usize) -> u64 {
        let base = self
            .pass
            .as_ref()
            .and_then(|p| p.arguments.as_ref())
            .map_or(0, |a| a.offset);
        base + offset as u64
    }

    /// `(buffer, offset)` of the pass's argument buffer (`getFinalArgumentBuffer`).
    #[must_use]
    pub fn final_argument_buffer(&self) -> (u64, u64) {
        self.pass
            .as_ref()
            .and_then(|p| p.arguments.as_ref())
            .map_or((0, 0), |a| (a.buffer, a.offset))
    }

    /// Updates the bound index buffer's byte offset (`bindIndexBufferOffset`).
    pub const fn bind_index_buffer_offset(&mut self, offset: u32) {
        self.index_buffer_offset = offset;
    }

    /// Bound index buffer's Metal handle, allocation offset and sub-offset (`currentIndexBuffer`).
    pub fn current_index_buffer_with_offset(&mut self) -> Option<(u64, u64, u32)> {
        let sub_offset = self.index_buffer_offset;
        self.current_index_buffer()
            .map(|(buf, off)| (buf, off, sub_offset))
    }

    /// Sets `counter` to `value` for subsequent draws/dispatches (`CounterPool::updateCounter`).
    pub fn update_counter(&mut self, counter: &Counter, value: u32) {
        self.counters_mut().update(counter, value);
    }

    /// Begins occlusion query `query` (`beginVisibilityResultQuery`).
    pub fn begin_visibility_query(&mut self, query: Arc<VisibilityQuery>) {
        self.queries.begin_visibility(self.seq, query);
    }

    /// Ends occlusion query `query` (`endVisibilityResultQuery`).
    pub fn end_visibility_query(&mut self, query: Arc<VisibilityQuery>) {
        self.queries.end_visibility(self.seq, &query);
        drop(query);
    }

    /// Advances the visibility-result offset before a draw (`bumpVisibilityResultOffset`).
    pub fn bump_visibility_result_offset(&mut self) {
        if self.queries.visibility_active()
            && let Some(targets) = self.render_targets()
        {
            targets.uses.insert(RenderUses::VISIBILITY_RESULT);
        }
        if let Some(cmd) = self.queries.visibility_mode() {
            self.encode_render_command(cmd);
        }
    }

    /// Pushes a deferred command list's visibility query array (`pushDeferredVisibilityQueries`).
    pub fn push_deferred_visibility_queries(&mut self, list: *const Arc<VisibilityQuery>) {
        self.deferred_queries.push(list.expose_provenance());
    }

    /// Pops the top deferred visibility query array (`popDeferredVisibilityQueries`).
    pub fn pop_deferred_visibility_queries(&mut self) {
        self.deferred_queries.pop();
    }

    /// Looks up query `query_id` in the active deferred visibility query array.
    #[must_use]
    pub fn current_deferred_visibility_query(&self, query_id: u32) -> Option<Arc<VisibilityQuery>> {
        if let Some(&addr) = self.deferred_queries.last()
            && addr != 0
        {
            let base = ptr::with_exposed_provenance::<Arc<VisibilityQuery>>(addr);
            // SAFETY: the caller of `push_deferred_visibility_queries` guarantees that the query
            // array has at least `query_id + 1` elements.
            let elem = unsafe { base.add(query_id as usize) };
            // SAFETY: the deferred visibility query array outlives the replay scope.
            return Some(unsafe { &*elem }.clone());
        }
        self.queries.deferred(query_id as usize)
    }

    /// Issues timestamp query `query` at the current pass boundary (`sampleTimestamp`).
    pub fn sample_timestamp_query(&mut self, query: Arc<TimestampQuery>) {
        let needs_sample_pass = !self.last_samples_timestamp();
        self.queries.sample_timestamp(query);
        if needs_sample_pass {
            self.sample_timestamp(0);
        }
    }

    /// Queues a GS indirect argument marshal task (`encodeGSDispatchArgumentsMarshal`).
    pub fn encode_gs_dispatch_arguments_marshal(
        &mut self,
        draw_args_buf: u64,
        draw_args_va: u64,
        vertex_count_per_warp: u32,
        dispatch_args_buf: u64,
        dispatch_args_va: u64,
    ) {
        let max_object_threadgroups = u64::from(self.max_object_threadgroups);
        if let Some(targets) = self.render_targets() {
            targets.gs_marshal_tasks.push(GsMarshalTask {
                draw_args_buf,
                draw_args_va,
                vertex_count_per_warp,
                max_object_threadgroups,
                dispatch_args_buf,
                dispatch_args_va,
            });
        }
    }

    /// Queues a tessellation indirect argument marshal task (`encodeTSDispatchArgumentsMarshal`).
    pub fn encode_ts_dispatch_arguments_marshal(
        &mut self,
        draw_args_buf: u64,
        draw_args_va: u64,
        control_point_count: u16,
        patch_per_group: u16,
        dispatch_args_buf: u64,
        dispatch_args_va: u64,
    ) {
        let max_object_threadgroups = u64::from(self.max_object_threadgroups);
        if let Some(targets) = self.render_targets() {
            targets.ts_marshal_tasks.push(TsMarshalTask {
                draw_args_buf,
                draw_args_va,
                control_point_count,
                patch_per_group,
                max_object_threadgroups,
                dispatch_args_buf,
                dispatch_args_va,
            });
        }
    }

    /// Copies a depth-stencil image from buffer `src` to texture `dst` (`DepthStencilBlitContext`).
    pub fn depth_stencil_copy_from_buffer(
        &mut self,
        src: (&Buffer, u64, u32, u32),
        dst: (&Arc<Texture>, u32, u32, u8),
        offset: [u32; 3],
        size: [u32; 3],
    ) {
        let (src_buf, src_offset, bytes_per_row, bytes_per_image) = src;
        let (dst_tex, level, slice, planar_flags) = dst;
        let pixel_format = dst_tex.info().pixel_format;
        let format_flags =
            u8::try_from(format::depth_stencil_planar_flags(pixel_format)).unwrap_or(0);
        let valid_flags = planar_flags & format_flags;
        if valid_flags == 0 || src_buf.current().is_none() {
            return;
        }
        let view_key = if self.device_handle == 0 {
            dst_tex.full_view()
        } else {
            TextureViewDescriptor {
                format: pixel_format,
                texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                first_mip: level,
                mip_count: 1,
                first_slice: slice,
                array_size: 1,
            }
            .key()
        };
        let w = (dst_tex.info().width >> level).max(1);
        let h = (dst_tex.info().height >> level).max(1);

        let dev_handle = self.device_handle;
        let pso = self
            .helper_psos
            .ds_buffer_pso(dev_handle, pixel_format, valid_flags);
        let dss = self.helper_psos.ds_state(dev_handle, valid_flags);

        self.start_render_pass(format_flags, !valid_flags & 3, 0, None);
        let Some((src_alloc, sub_off)) =
            self.access_buffer::<{ stage::PIXEL }>(src_buf, Access::READ)
        else {
            self.end_pass();
            return;
        };
        let attachment = self.attachment::<{ stage::PIXEL }>(dst_tex, view_key, Access::READ);
        if let Some(targets) = self.render_targets() {
            if format_flags & 1 != 0 {
                targets.depth.target.attachment = attachment.as_ref().map(|a| super::Attachment {
                    texture: Arc::clone(&a.texture),
                    allocation: Arc::clone(&a.allocation),
                    key: a.key,
                    view: a.view,
                });
                targets.depth.target.load_action = WMT_LOAD_ACTION_LOAD;
                targets.depth.target.store_action = WMT_STORE_ACTION_STORE;
            }
            if format_flags & 2 != 0 {
                targets.stencil.target.attachment = attachment;
                targets.stencil.target.load_action = WMT_LOAD_ACTION_LOAD;
                targets.stencil.target.store_action = WMT_STORE_ACTION_STORE;
            }
            targets.render_target_width = w;
            targets.render_target_height = h;
            targets.render_target_array_length = 1;
            targets.default_raster_sample_count = 1;
        }

        self.encode_render_command(WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso,
        });
        self.encode_render_command(WmtcmdRenderSetdepthstencilstate {
            r#type: WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
            reserved: [0; 3],
            next: NO_NEXT,
            depth_stencil_state: dss,
        });
        self.encode_render_command(WmtcmdRenderSetviewport {
            r#type: WMT_RENDER_COMMAND_SET_VIEWPORT,
            reserved: [0; 3],
            next: NO_NEXT,
            viewport: WMTViewport {
                origin_x: f64::from(offset[0]),
                origin_y: f64::from(offset[1]),
                width: f64::from(size[0]),
                height: f64::from(size[1]),
                znear: 0.0,
                zfar: 1.0,
            },
        });
        self.encode_render_command(WmtcmdRenderSetscissorrect {
            r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
            reserved: [0; 3],
            next: NO_NEXT,
            scissor_rect: WMTScissorRect {
                x: u64::from(offset[0]),
                y: u64::from(offset[1]),
                width: u64::from(size[0]),
                height: u64::from(size[1]),
            },
        });
        self.encode_render_command(WmtcmdRenderSetbuffer {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER,
            reserved: [0; 3],
            next: NO_NEXT,
            buffer: src_alloc.buffer(),
            offset: src_offset + sub_off,
            index: 0,
        });
        let args = [bytes_per_row, bytes_per_image, offset[0], offset[1]];
        self.set_fragment_bytes_inline(&args, 1);

        if pixel_format == WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 && valid_flags == 3 {
            let depth_pso = self.helper_psos.ds_buffer_pso(dev_handle, pixel_format, 1);
            let depth_dss = self.helper_psos.ds_state(dev_handle, 1);
            self.encode_render_command(WmtcmdRenderSetpso {
                r#type: WMT_RENDER_COMMAND_SET_PSO,
                reserved: [0; 3],
                next: NO_NEXT,
                pso: depth_pso,
            });
            self.encode_render_command(WmtcmdRenderSetdepthstencilstate {
                r#type: WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
                reserved: [0; 3],
                next: NO_NEXT,
                depth_stencil_state: depth_dss,
            });
            self.encode_render_command(WmtcmdRenderDraw {
                r#type: WMT_RENDER_COMMAND_DRAW,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
                vertex_start: 0,
                vertex_count: 3,
                instance_count: 1,
                base_instance: 0,
            });
            let stencil_pso = self.helper_psos.ds_buffer_pso(dev_handle, pixel_format, 2);
            let stencil_dss = self.helper_psos.ds_state(dev_handle, 2);
            self.encode_render_command(WmtcmdRenderSetpso {
                r#type: WMT_RENDER_COMMAND_SET_PSO,
                reserved: [0; 3],
                next: NO_NEXT,
                pso: stencil_pso,
            });
            self.encode_render_command(WmtcmdRenderSetdepthstencilstate {
                r#type: WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
                reserved: [0; 3],
                next: NO_NEXT,
                depth_stencil_state: stencil_dss,
            });
        }

        if pixel_format == WMT_PIXEL_FORMAT_STENCIL8
            || (pixel_format == WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 && (valid_flags & 2 != 0))
        {
            self.encode_render_command(WmtcmdRenderSetstencilref {
                r#type: WMT_RENDER_COMMAND_SET_STENCIL_REF,
                reserved: [0; 3],
                next: NO_NEXT,
                stencil_ref: 0x80,
            });
            for bit in 0..8_u32 {
                self.set_fragment_bytes_inline(&[bit], 2);
                self.encode_render_command(WmtcmdRenderDraw {
                    r#type: WMT_RENDER_COMMAND_DRAW,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
                    vertex_start: 0,
                    vertex_count: 3,
                    instance_count: 1,
                    base_instance: 0,
                });
            }
        } else {
            self.encode_render_command(WmtcmdRenderDraw {
                r#type: WMT_RENDER_COMMAND_DRAW,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
                vertex_start: 0,
                vertex_count: 3,
                instance_count: 1,
                base_instance: 0,
            });
        }
        self.end_pass();
    }

    /// Copies a depth-stencil subresource between textures (`DepthStencilBlitContext`).
    pub fn depth_stencil_copy_from_texture(
        &mut self,
        src: (&Arc<Texture>, u32, u32, [u32; 3], [u32; 3]),
        dst: (&Arc<Texture>, u32, u32, u8, [u32; 3]),
    ) {
        let (src_tex, src_level, src_slice, src_offset, src_size) = src;
        let (dst_tex, dst_level, dst_slice, planar_flags, dst_offset) = dst;
        if dst_tex.info().pixel_format != WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 {
            return;
        }
        if src_tex.info().pixel_format != WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 {
            return;
        }
        let valid_flags = planar_flags & 3;
        if valid_flags == 0 {
            return;
        }

        let view_2d_array = |fmt: u32, level: u32, slice: u32| {
            TextureViewDescriptor {
                format: fmt,
                texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                first_mip: level,
                mip_count: 1,
                first_slice: slice,
                array_size: 1,
            }
            .key()
        };

        let dev_handle = self.device_handle;
        let pso = self.helper_psos.ds_tex_pso(dev_handle, valid_flags);

        self.start_compute_pass(None);
        self.encode_compute_command(WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso,
            threadgroup_size: WMTSize {
                width: 8,
                height: 4,
                depth: 1,
            },
        });

        let args = [
            src_offset[0],
            src_offset[1],
            src_size[0],
            src_size[1],
            dst_offset[0],
            dst_offset[1],
        ];
        self.set_compute_bytes_inline(&args, 0);

        if valid_flags & 1 != 0 {
            let src_view = if dev_handle == 0 {
                self.access_texture::<{ stage::COMPUTE }>(
                    src_tex,
                    src_level,
                    src_slice,
                    Access::READ,
                )
            } else {
                let src_key = view_2d_array(WMT_PIXEL_FORMAT_DEPTH32_FLOAT, src_level, src_slice);
                self.access_texture_view::<{ stage::COMPUTE }>(src_tex, src_key, Access::READ)
                    .map_or(0, |v| v.texture)
            };
            self.encode_compute_command(WmtcmdComputeSettexture {
                r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
                reserved: [0; 3],
                next: NO_NEXT,
                texture: src_view,
                index: 0,
            });
        }
        if valid_flags & 2 != 0 {
            let src_view = if dev_handle == 0 {
                self.access_texture::<{ stage::COMPUTE }>(
                    src_tex,
                    src_level,
                    src_slice,
                    Access::READ,
                )
            } else {
                let src_key = view_2d_array(WMT_PIXEL_FORMAT_X32_STENCIL8, src_level, src_slice);
                self.access_texture_view::<{ stage::COMPUTE }>(src_tex, src_key, Access::READ)
                    .map_or(0, |v| v.texture)
            };
            self.encode_compute_command(WmtcmdComputeSettexture {
                r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
                reserved: [0; 3],
                next: NO_NEXT,
                texture: src_view,
                index: 1,
            });
        }

        let dst_view = if dev_handle == 0 {
            self.access_texture::<{ stage::COMPUTE }>(
                dst_tex,
                dst_level,
                dst_slice,
                Access::READ | Access::WRITE | Access::UAV,
            )
        } else {
            let dst_key = view_2d_array(
                WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
                dst_level,
                dst_slice,
            );
            self.access_texture_view::<{ stage::COMPUTE }>(
                dst_tex,
                dst_key,
                Access::READ | Access::WRITE | Access::UAV,
            )
            .map_or(0, |v| v.texture)
        };
        self.encode_compute_command(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: NO_NEXT,
            texture: dst_view,
            index: 2,
        });

        self.encode_compute_command(WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
            reserved: [0; 3],
            next: NO_NEXT,
            size: WMTSize {
                width: u64::from(src_size[0]),
                height: u64::from(src_size[1]),
                depth: 1,
            },
        });
        self.end_pass();
    }

    /// Scales a motion-vector texture via `MetalFX` MV scale compute (`MTLFXMVScaleContext`).
    pub fn mv_scale_dispatch(
        &mut self,
        src: &Arc<Texture>,
        dst: &Arc<Texture>,
        scale_x: f32,
        scale_y: f32,
    ) {
        let dev_handle = self.device_handle;
        let pso = self.helper_psos.mv_scale(dev_handle);

        self.start_compute_pass(None);
        self.encode_compute_command(WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso,
            threadgroup_size: WMTSize {
                width: 8,
                height: 4,
                depth: 1,
            },
        });

        let src_view = self
            .access_texture_view::<{ stage::COMPUTE }>(src, src.full_view(), Access::READ)
            .map_or(0, |v| v.texture);
        let dst_view = self
            .access_texture_view::<{ stage::COMPUTE }>(
                dst,
                dst.full_view(),
                Access::WRITE | Access::UAV,
            )
            .map_or(0, |v| v.texture);

        self.encode_compute_command(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: NO_NEXT,
            texture: src_view,
            index: 0,
        });
        self.encode_compute_command(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: NO_NEXT,
            texture: dst_view,
            index: 1,
        });
        self.set_compute_bytes_inline(&[scale_x.to_bits(), scale_y.to_bits()], 0);
        self.encode_compute_command(WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
            reserved: [0; 3],
            next: NO_NEXT,
            size: WMTSize {
                width: u64::from(dst.info().width),
                height: u64::from(dst.info().height),
                depth: 1,
            },
        });
        self.end_pass();
    }

    /// Dispatches the tile barrier draw in the active render pass (`TileBarrierContext`).
    pub fn dispatch_tile_barrier(&mut self) {
        let Some((key, recover_pso)) = self
            .render_targets()
            .map(|t| (t.tile_barrier_pso_key, t.last_pso))
        else {
            return;
        };
        let dev_handle = self.device_handle;
        let barrier_pso = self.helper_psos.tile_barrier(dev_handle, key);
        self.encode_render_command(WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso: barrier_pso,
        });
        self.encode_render_command(WmtcmdRenderDispatchThreadsPerTile {
            r#type: WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE,
            reserved: [0; 3],
            next: NO_NEXT,
            width: 16,
            height: 16,
        });
        self.encode_render_command(WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso: recover_pso,
        });
    }

    /// Current frame id (`ArgumentEncodingContext::currentFrameId`).
    #[must_use]
    pub const fn current_frame_id(&self) -> u64 {
        self.frame_id
    }

    /// Sets the active chunk's frame id and coherent sequence number before encoding.
    pub const fn set_current_frame(&mut self, frame_id: u64, coherent_seq: u64) {
        self.frame_id = frame_id;
        self.coherent_seq = coherent_seq;
    }

    /// Mutable access to the current frame's statistics (`currentFrameStatistics`).
    pub const fn current_frame_statistics(&mut self) -> Option<&mut FrameStatistics> {
        Some(&mut self.frame_stats)
    }

    /// Records a feature-compatibility flag on the current frame (`setCompatibilityFlag`).
    pub fn set_compatibility_flag(&mut self, flag: CompatibilityFlags) {
        self.frame_stats.set_compatibility_flag(flag);
    }

    fn counters_mut(&mut self) -> &mut CounterPool {
        let handle = if self.device_handle != 0 {
            self.device_handle
        } else {
            0xDEAD
        };
        self.counters.get_or_insert_with(|| {
            // SAFETY: `handle` is borrowed for `CounterPool::new`, which retains its own copy.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(handle) }.expect("non-zero"));
            CounterPool::new(&dev)
        })
    }

    fn copy_temp_mut(&mut self) -> &mut RingBump {
        let handle = if self.device_handle != 0 {
            self.device_handle
        } else {
            0xDEAD
        };
        self.copy_temp.get_or_insert_with(|| {
            // SAFETY: `handle` is borrowed for `RingBump::new`, which retains its own copy.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(handle) }.expect("non-zero"));
            RingBump::new(
                &dev,
                BufferAllocationFlags::GPU_PRIVATE,
                RingConfig {
                    block: DEFERRED_BLOCK,
                    lifetime: LIFETIME,
                },
            )
        })
    }

    fn set_fragment_bytes_inline<const N: usize>(&mut self, words: &[u32; N], index: u8) {
        let byte_len = N * 4;
        let dst = self.allocate_cpu(byte_len, 8);
        // SAFETY: `dst` has `byte_len` bytes, non-overlapping with `words`.
        unsafe {
            ptr::copy_nonoverlapping(words.as_ptr().cast::<u8>(), dst.as_ptr(), byte_len);
        }
        self.encode_render_command(WmtcmdRenderSetbytes {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
            reserved: [0; 3],
            next: NO_NEXT,
            bytes: raw_mem_ptr(dst.as_ptr().expose_provenance()),
            length: byte_len as u64,
            index,
        });
    }

    fn set_compute_bytes_inline<const N: usize>(&mut self, words: &[u32; N], index: u8) {
        let byte_len = N * 4;
        let dst = self.allocate_cpu(byte_len, 8);
        // SAFETY: `dst` has `byte_len` bytes, non-overlapping with `words`.
        unsafe {
            ptr::copy_nonoverlapping(words.as_ptr().cast::<u8>(), dst.as_ptr(), byte_len);
        }
        self.encode_compute_command(WmtcmdComputeSetbytes {
            r#type: WMT_COMPUTE_COMMAND_SET_BYTES,
            reserved: [0; 3],
            next: NO_NEXT,
            bytes: raw_mem_ptr(dst.as_ptr().expose_provenance()),
            length: byte_len as u64,
            index,
        });
    }
}

#[cfg(target_pointer_width = "64")]
const fn raw_mem_ptr(addr: usize) -> MemPtr {
    MemPtr { ptr: addr as u64 }
}

#[cfg(target_pointer_width = "32")]
fn raw_mem_ptr(addr: usize) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(addr).unwrap_or(0),
        high: 0,
    }
}

#[cfg(test)]
mod tests;
