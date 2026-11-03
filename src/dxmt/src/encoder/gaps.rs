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
    MTLDevice_newComputePipelineState, MTLDevice_newTileRenderPipelineState, Obj,
    wire::{
        MemPtr, WMT_COMPARE_FUNCTION_ALWAYS, WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
        WMT_COMPUTE_COMMAND_SET_BUFFER, WMT_COMPUTE_COMMAND_SET_BYTES, WMT_COMPUTE_COMMAND_SET_PSO,
        WMT_COMPUTE_COMMAND_SET_TEXTURE, WMT_LOAD_ACTION_LOAD, WMT_PIXEL_FORMAT_DEPTH16_UNORM,
        WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
        WMT_PIXEL_FORMAT_INVALID, WMT_PIXEL_FORMAT_STENCIL8, WMT_PIXEL_FORMAT_X32_STENCIL8,
        WMT_PRIMITIVE_TOPOLOGY_CLASS_TRIANGLE, WMT_PRIMITIVE_TYPE_TRIANGLE,
        WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE, WMT_RENDER_COMMAND_DRAW,
        WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE, WMT_RENDER_COMMAND_SET_DSSO,
        WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER, WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
        WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
        WMT_RENDER_COMMAND_SET_STENCIL_REF, WMT_RENDER_COMMAND_SET_VIEWPORT,
        WMT_RENDER_COMMAND_USE_RESOURCE, WMT_STENCIL_OPERATION_KEEP, WMT_STENCIL_OPERATION_REPLACE,
        WMT_STORE_ACTION_STORE, WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE_CUBE_ARRAY,
        WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY, WMT_TEXTURE_TYPE3D, WMTComputePipelineInfo,
        WMTDepthStencilInfo, WMTScissorRect, WMTSize, WMTStencilInfo, WMTTileRenderPipelineInfo,
        WMTViewport, WmtcmdComputeDispatch, WmtcmdComputeSetbuffer, WmtcmdComputeSetbytes,
        WmtcmdComputeSetpso, WmtcmdComputeSettexture, WmtcmdRenderDispatchThreadsPerTile,
        WmtcmdRenderDraw, WmtcmdRenderSetbuffer, WmtcmdRenderSetbytes,
        WmtcmdRenderSetdepthstencilstate, WmtcmdRenderSetdsso, WmtcmdRenderSetpso,
        WmtcmdRenderSetscissorrect, WmtcmdRenderSetstencilref, WmtcmdRenderSetviewport,
        WmtcmdRenderUseresource,
    },
};

use super::{
    ArgumentBuffer, Encoder, NO_NEXT, RenderUses,
    pass::{GsMarshalTask, TsMarshalTask},
    stage,
};
use crate::{
    counter::{Counter, CounterPool},
    deptrack::Access,
    emulated::EmulatedContext,
    format,
    frame_stats::{CompatibilityFlags, FrameStatistics},
    library::{
        CUSTOM_BUFFER_ARGUMENT_INDEX_0, ClearRtv, ClearUav, Library, ResourceAccess,
        SimpleCommandContext, depth_stencil_state, render_pipeline, render_pipeline_info,
    },
    query::{TimestampQuery, VisibilityQuery},
    resource::{
        Buffer, BufferAllocation, BufferAllocationFlags, BufferViewKey, Texture, TextureAllocation,
        TextureViewDescriptor, TextureViewKey,
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
    ds_copy_from_buf_psos: [u64; 2],
    ds_copy_to_buf_psos: [u64; 2],
    ds_copy_dss: u64,
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
            ds_copy_from_buf_psos: [0; 2],
            ds_copy_to_buf_psos: [0; 2],
            ds_copy_dss: 0,
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
        let pso = self.compute_pso(device_handle, c"cs_downscale_dilated_mv");
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
            let kernel = lib.function(c"tile_barrier");
            let mut color_formats = [WMT_PIXEL_FORMAT_INVALID; 8];
            color_formats.copy_from_slice(&key[..8]);
            let info = WMTTileRenderPipelineInfo {
                color_formats,
                tile_function: kernel.as_ref().map_or(0, Obj::handle),
                immutable_tile_buffers: 0,
                raster_sample_count: u8::try_from(key[8]).unwrap_or(1).max(1),
                tgsize_matches_tile_size: 1,
                binary_archive_for_serialization: 0,
                binary_archives_for_lookup: NO_NEXT,
                num_binary_archives_for_lookup: 0,
                fail_on_binary_archive_miss: 0,
                padding: [0; 6],
            };
            let mut error = 0;
            let handle = dev.as_ref().map_or(0, |d| {
                // SAFETY: `d` is a live `MTLDevice`; `info` and `error` are valid for the call.
                unsafe {
                    MTLDevice_newTileRenderPipelineState(
                        d.handle(),
                        &raw const info,
                        &raw mut error,
                    )
                }
            });
            // SAFETY: any error returned is a +1 handle.
            drop(unsafe { Obj::adopt(error) });
            // SAFETY: `newTileRenderPipelineState` returns a +1 handle or 0.
            if let Some(obj) = unsafe { Obj::adopt(handle) } {
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

    fn ds_copy_from_buf_pso(&mut self, device_handle: u64, from_d24s8: bool) -> u64 {
        let idx = usize::from(from_d24s8);
        if self.ds_copy_from_buf_psos[idx] != 0 {
            return self.ds_copy_from_buf_psos[idx];
        }
        self.ensure_library(device_handle);
        let pso = if let (Some(lib), true) = (&self.library, device_handle != 0) {
            // SAFETY: `device_handle` is a borrowed live `MTLDevice` handle.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(device_handle) });
            let vs = lib.function(c"vs_present_quad");
            let fs = lib.function(if from_d24s8 {
                c"fs_copy_from_buffer_d24s8"
            } else {
                c"fs_copy_from_buffer_d32s8"
            });
            let mut info = render_pipeline_info();
            info.vertex_function = vs.as_ref().map_or(0, Obj::handle);
            info.fragment_function = fs.as_ref().map_or(0, Obj::handle);
            info.depth_pixel_format = WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8;
            info.stencil_pixel_format = WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8;
            info.rasterization_enabled = 1;
            info.input_primitive_topology = WMT_PRIMITIVE_TOPOLOGY_CLASS_TRIANGLE;
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
        self.ds_copy_from_buf_psos[idx] = pso;
        pso
    }

    fn ds_copy_to_buf_pso(&mut self, device_handle: u64, to_d24s8: bool) -> u64 {
        let idx = usize::from(to_d24s8);
        if self.ds_copy_to_buf_psos[idx] != 0 {
            return self.ds_copy_to_buf_psos[idx];
        }
        self.ensure_library(device_handle);
        let name: &CStr = if to_d24s8 {
            c"cs_copy_to_buffer_d24s8"
        } else {
            c"cs_copy_to_buffer_d32s8"
        };
        let pso = self.compute_pso(device_handle, name);
        self.ds_copy_to_buf_psos[idx] = pso;
        pso
    }

    fn ds_copy_dss(&mut self, device_handle: u64) -> u64 {
        if self.ds_copy_dss != 0 {
            return self.ds_copy_dss;
        }
        let handle = if device_handle == 0 {
            self.next_fake_handle()
        } else {
            // SAFETY: `device_handle` is a borrowed live `MTLDevice` handle.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(device_handle) });
            let stencil = WMTStencilInfo {
                enabled: 1,
                depth_stencil_pass_op: WMT_STENCIL_OPERATION_REPLACE,
                depth_fail_op: WMT_STENCIL_OPERATION_REPLACE,
                stencil_fail_op: WMT_STENCIL_OPERATION_REPLACE,
                stencil_compare_function: WMT_COMPARE_FUNCTION_ALWAYS,
                write_mask: 0xFF,
                read_mask: 0,
            };
            let info = WMTDepthStencilInfo {
                depth_compare_function: WMT_COMPARE_FUNCTION_ALWAYS,
                depth_write_enabled: 1,
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
        self.ds_copy_dss = handle;
        handle
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
        let already_buf = buffer.check_retained(self.seq);
        self.retain(&buffer, already_buf);
        let offset = u64::from(slot.offset());
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

    /// Allocates an [`ArgumentBuffer`] of `size` bytes from the encoder's argument-buffer ring.
    pub fn allocate_argument_buffer(&mut self, size: usize) -> ArgumentBuffer {
        let seq = self.seq;
        let coherent = self.coherent_seq;
        if self.device_handle != 0 {
            let (block, offset) = self
                .argbuf_ring_mut()
                .allocate(seq, coherent, size as u64, 64);
            let off = usize::try_from(offset).unwrap_or(0);
            // SAFETY: `off + size` is inside the mapped block, aligned to 64.
            let mapped = unsafe { block.mapped.add(off) };
            // SAFETY: `mapped` is non-null since `block.mapped` is non-null.
            let ptr = unsafe { NonNull::new_unchecked(mapped) };
            // SAFETY: `ptr` is valid for `size` bytes until the chunk retires.
            unsafe { ArgumentBuffer::new(ptr, size, block.buffer, offset) }
        } else {
            let alloc_size = size.max(8);
            let ptr = self.allocate_cpu(alloc_size, 64);
            // SAFETY: `ptr` is valid for `alloc_size` bytes in the encoder's CPU heap.
            unsafe { ArgumentBuffer::new(ptr, alloc_size, 0, 0) }
        }
    }

    /// Runs `f` with the encoder's lazily initialized [`EmulatedContext`].
    pub fn with_emulated<R>(
        &mut self,
        f: impl FnOnce(&EmulatedContext, &mut Self) -> R,
    ) -> Option<R> {
        self.ensure_emulated();
        let em = self.emulated.take()?;
        let res = f(&em, self);
        self.emulated = Some(em);
        Some(res)
    }

    /// Runs `f` with the encoder's lazily initialized [`ClearRtv`].
    pub fn with_clear_rtv<R>(
        &mut self,
        f: impl FnOnce(&mut ClearRtv, &mut Self) -> R,
    ) -> Option<R> {
        self.ensure_clear_rtv();
        let mut rtv = self.clear_rtv.take()?;
        let res = f(&mut rtv, self);
        self.clear_rtv = Some(rtv);
        Some(res)
    }

    /// Runs `f` with the encoder's lazily initialized [`ClearUav`].
    pub fn with_clear_uav<R>(
        &mut self,
        f: impl FnOnce(&mut ClearUav, &mut Self) -> R,
    ) -> Option<R> {
        self.ensure_clear_uav();
        let mut uav = self.clear_uav.take()?;
        let res = f(&mut uav, self);
        self.clear_uav = Some(uav);
        Some(res)
    }

    fn ensure_emulated(&mut self) {
        if self.emulated.is_some() || self.device_handle == 0 {
            return;
        }
        self.helper_psos.ensure_library(self.device_handle);
        if let Some(lib) = &self.helper_psos.library {
            // SAFETY: `device_handle` is a borrowed live `MTLDevice` handle.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(self.device_handle) });
            if let Some(dev) = dev.as_ref() {
                self.emulated = Some(EmulatedContext::new(dev, lib));
            }
        }
    }

    fn ensure_clear_rtv(&mut self) {
        if self.clear_rtv.is_some() || self.device_handle == 0 {
            return;
        }
        self.helper_psos.ensure_library(self.device_handle);
        if self.helper_psos.library.is_some() {
            // SAFETY: `device_handle` is a borrowed live `MTLDevice` handle.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(self.device_handle) });
            if let Some(dev) = dev.as_ref() {
                let rtv = ClearRtv::new(dev, self);
                self.clear_rtv = Some(rtv);
            }
        }
    }

    fn ensure_clear_uav(&mut self) {
        if self.clear_uav.is_some() || self.device_handle == 0 {
            return;
        }
        self.helper_psos.ensure_library(self.device_handle);
        if self.helper_psos.library.is_some() {
            let uav = ClearUav::new(self);
            self.clear_uav = Some(uav);
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

        let argbuf = self.allocate_argument_buffer(0);
        self.start_render_pass(format_flags, !valid_flags & 3, 0, Some(argbuf));
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

        let argbuf = self.allocate_argument_buffer(0);
        self.start_compute_pass(Some(argbuf));
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

        let argbuf = self.allocate_argument_buffer(0);
        self.start_compute_pass(Some(argbuf));
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

    /// Scales a motion-vector texture view via `MetalFX` MV scale (`MTLFXMVScaleContext`).
    pub fn mv_scale_dispatch_views(
        &mut self,
        src: &Arc<Texture>,
        src_view_key: TextureViewKey,
        dst: &Arc<Texture>,
        dst_view_key: TextureViewKey,
        scale_x: f32,
        scale_y: f32,
    ) {
        let dev_handle = self.device_handle;
        let pso = self.helper_psos.mv_scale(dev_handle);

        let argbuf = self.allocate_argument_buffer(0);
        self.start_compute_pass(Some(argbuf));
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
            .access_texture_view::<{ stage::COMPUTE }>(src, src_view_key, Access::READ)
            .map_or(0, |v| v.texture);
        let dst_view = self
            .access_texture_view::<{ stage::COMPUTE }>(
                dst,
                dst_view_key,
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

    /// Copies a depth-stencil image from `src` buffer to `dst` (`copyFromBuffer`).
    pub fn ds_copy_from_buffer(
        &mut self,
        src: (&Buffer, u64, u64, u32, u32),
        dst: (&Arc<Texture>, u32, u32),
        from_d24s8: bool,
    ) {
        let (src, src_offset, src_length, bytes_per_row, bytes_per_image) = src;
        let (depth_stencil, level, slice) = dst;
        let _ = src_length;
        match depth_stencil.info().r#type() {
            WMT_TEXTURE_TYPE2D
            | WMT_TEXTURE_TYPE2D_ARRAY
            | WMT_TEXTURE_TYPE_CUBE
            | WMT_TEXTURE_TYPE_CUBE_ARRAY => {}
            _ => return,
        }
        let view_key = TextureViewDescriptor {
            format: depth_stencil.info().pixel_format,
            texture_type: WMT_TEXTURE_TYPE2D,
            first_mip: level,
            mip_count: 1,
            first_slice: slice,
            array_size: 1,
        }
        .key();
        let [width, height, _] = depth_stencil.view_size(view_key);
        let dev_handle = self.device_handle;
        let pso = self
            .helper_psos
            .ds_copy_from_buf_pso(dev_handle, from_d24s8);
        let dsso = self.helper_psos.ds_copy_dss(dev_handle);

        let argbuf = self.allocate_argument_buffer(0);
        self.start_render_pass(0b11, 0, 0, Some(argbuf));
        let depth_att = self.attachment::<{ stage::PIXEL }>(depth_stencil, view_key, Access::WRITE);
        let stencil_att =
            self.attachment::<{ stage::PIXEL }>(depth_stencil, view_key, Access::WRITE);
        let Some((src_alloc, src_sub_offset)) =
            self.access_buffer::<{ stage::PIXEL }>(src, Access::READ)
        else {
            self.end_pass();
            return;
        };
        if let Some(targets) = self.render_targets() {
            targets.depth.target.attachment = depth_att;
            targets.depth.target.depth_plane = 0;
            targets.depth.target.load_action = WMT_LOAD_ACTION_LOAD;
            targets.depth.target.store_action = WMT_STORE_ACTION_STORE;
            targets.stencil.target.attachment = stencil_att;
            targets.stencil.target.depth_plane = 0;
            targets.stencil.target.load_action = WMT_LOAD_ACTION_LOAD;
            targets.stencil.target.store_action = WMT_STORE_ACTION_STORE;
            targets.render_target_width = width;
            targets.render_target_height = height;
            targets.render_target_array_length = 0;
            targets.default_raster_sample_count =
                u8::try_from(depth_stencil.info().sample_count()).unwrap_or(1);
        }
        self.encode_render_command(WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso,
        });
        self.encode_render_command(WmtcmdRenderSetviewport {
            r#type: WMT_RENDER_COMMAND_SET_VIEWPORT,
            reserved: [0; 3],
            next: NO_NEXT,
            viewport: WMTViewport {
                origin_x: 0.0,
                origin_y: 0.0,
                width: f64::from(width),
                height: f64::from(height),
                znear: 0.0,
                zfar: 1.0,
            },
        });
        self.encode_render_command(WmtcmdRenderSetdsso {
            r#type: WMT_RENDER_COMMAND_SET_DSSO,
            reserved: [0; 3],
            next: NO_NEXT,
            dsso,
            stencil_ref: 0,
        });
        self.encode_render_command(WmtcmdRenderSetbuffer {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER,
            reserved: [0; 3],
            next: NO_NEXT,
            buffer: src_alloc.buffer(),
            offset: src_offset + src_sub_offset,
            index: u8::try_from(CUSTOM_BUFFER_ARGUMENT_INDEX_0).unwrap_or(30),
        });
        self.set_fragment_bytes_inline(
            &[bytes_per_row, bytes_per_image],
            u8::try_from(CUSTOM_BUFFER_ARGUMENT_INDEX_0 + 1).unwrap_or(31),
        );
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
        self.end_pass();
    }

    /// Copies a depth-stencil image from `src` texture to `dst` (`copyFromTexture`).
    pub fn ds_copy_from_texture(
        &mut self,
        src: (&Arc<Texture>, u32, u32),
        dst: (&Buffer, u64, u64, u32, u32),
        to_d24s8: bool,
    ) {
        let (depth_stencil, level, slice) = src;
        let (dst, dst_offset, dst_length, bytes_per_row, bytes_per_image) = dst;
        let _ = dst_length;
        match depth_stencil.info().r#type() {
            WMT_TEXTURE_TYPE2D
            | WMT_TEXTURE_TYPE2D_ARRAY
            | WMT_TEXTURE_TYPE_CUBE
            | WMT_TEXTURE_TYPE_CUBE_ARRAY => {}
            _ => return,
        }
        let depth_view = TextureViewDescriptor {
            format: WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
            texture_type: WMT_TEXTURE_TYPE2D,
            first_mip: level,
            mip_count: 1,
            first_slice: slice,
            array_size: 1,
        }
        .key();
        let stencil_view = TextureViewDescriptor {
            format: WMT_PIXEL_FORMAT_X32_STENCIL8,
            texture_type: WMT_TEXTURE_TYPE2D,
            first_mip: level,
            mip_count: 1,
            first_slice: slice,
            array_size: 1,
        }
        .key();
        let dev_handle = self.device_handle;
        let pso = self.helper_psos.ds_copy_to_buf_pso(dev_handle, to_d24s8);

        let argbuf = self.allocate_argument_buffer(0);
        self.start_compute_pass(Some(argbuf));
        let tex_depth = self
            .access_texture_view::<{ stage::COMPUTE }>(depth_stencil, depth_view, Access::READ)
            .map_or(0, |v| v.texture);
        let tex_stencil = self
            .access_texture_view::<{ stage::COMPUTE }>(depth_stencil, stencil_view, Access::READ)
            .map_or(0, |v| v.texture);
        let Some((dst_alloc, dst_sub_offset)) =
            self.access_buffer::<{ stage::COMPUTE }>(dst, Access::WRITE)
        else {
            self.end_pass();
            return;
        };
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
        self.encode_compute_command(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: NO_NEXT,
            texture: tex_depth,
            index: 0,
        });
        self.encode_compute_command(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: NO_NEXT,
            texture: tex_stencil,
            index: 1,
        });
        self.encode_compute_command(WmtcmdComputeSetbuffer {
            r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
            reserved: [0; 3],
            next: NO_NEXT,
            buffer: dst_alloc.buffer(),
            offset: dst_offset + dst_sub_offset,
            index: 0,
        });
        self.set_compute_bytes_inline(&[bytes_per_row, bytes_per_image], 1);
        let [width, height, _] = depth_stencil.view_size(depth_view);
        self.encode_compute_command(WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
            reserved: [0; 3],
            next: NO_NEXT,
            size: WMTSize {
                width: u64::from(width),
                height: u64::from(height),
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

    pub(super) fn counters_mut(&mut self) -> &mut CounterPool {
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

    fn argbuf_ring_mut(&mut self) -> &mut RingBump {
        let handle = if self.device_handle != 0 {
            self.device_handle
        } else {
            0xDEAD
        };
        self.argbuf_ring.get_or_insert_with(|| {
            // SAFETY: `handle` is borrowed for `RingBump::new`, which retains its own copy.
            let dev = ManuallyDrop::new(unsafe { Obj::adopt(handle) }.expect("non-zero"));
            let mut flags = BufferAllocationFlags::CPU_WRITE_COMBINED;
            if cfg!(not(target_os = "macos")) {
                flags |= BufferAllocationFlags::CPU_PLACED;
            }
            RingBump::new(
                &dev,
                flags,
                RingConfig {
                    block: 0x40_0000,
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

const fn to_access(ra: ResourceAccess) -> Access {
    let mut out = Access::empty();
    if ra.contains(ResourceAccess::READ) {
        out = out.union(Access::READ);
    }
    if ra.contains(ResourceAccess::WRITE) {
        out = out.union(Access::WRITE);
    }
    if ra.contains(ResourceAccess::UAV) {
        out = out.union(Access::UAV);
    }
    out
}

impl SimpleCommandContext for Encoder {
    fn compute_pipeline(&mut self, name: &CStr) -> Option<Obj> {
        let dev_handle = self.device_handle;
        self.helper_psos.ensure_library(dev_handle);
        let lib = self.helper_psos.library.as_ref()?;
        let func = lib.function(name)?;
        // SAFETY: `dev_handle` is a borrowed live `MTLDevice` handle.
        let dev = ManuallyDrop::new(unsafe { Obj::adopt(dev_handle) });
        let dev = dev.as_ref()?;
        let info = WMTComputePipelineInfo {
            compute_function: func.handle(),
            binary_archives_for_lookup: NO_NEXT,
            binary_archive_for_serialization: 0,
            num_binary_archives_for_lookup: 0,
            fail_on_binary_archive_miss: 0,
            support_indirect_command_buffers: 0,
            tgsize_is_multiple_of_sgwidth: 0,
            immutable_buffers: 0,
        };
        let mut error = 0;
        // SAFETY: `dev` is a live `MTLDevice`; `info` and `error` are valid for the call.
        let handle = unsafe {
            MTLDevice_newComputePipelineState(dev.handle(), &raw const info, &raw mut error)
        };
        // SAFETY: any error returned is a +1 handle.
        drop(unsafe { Obj::adopt(error) });
        // SAFETY: `newComputePipelineState` returns a +1 handle or 0.
        unsafe { Obj::adopt(handle) }
    }

    fn start_compute_pass(&mut self) {
        let argbuf = self.allocate_argument_buffer(0);
        Self::start_compute_pass(self, Some(argbuf));
    }

    fn end_pass(&mut self) {
        Self::end_pass(self);
    }

    fn set_compute_pso(&mut self, pso: &Obj, threadgroup_size: WMTSize) {
        self.encode_compute_command(WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso: pso.handle(),
            threadgroup_size,
        });
    }

    fn dispatch(&mut self, size: WMTSize) {
        self.encode_compute_command(WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
            reserved: [0; 3],
            next: NO_NEXT,
            size,
        });
    }

    fn set_compute_texture(
        &mut self,
        index: u32,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        access: ResourceAccess,
    ) {
        let Some(v) =
            self.access_texture_view::<{ stage::COMPUTE }>(texture, view, to_access(access))
        else {
            return;
        };
        self.encode_compute_command(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: NO_NEXT,
            texture: v.texture,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn set_compute_texel_buffer(
        &mut self,
        index: u32,
        buffer: &Arc<Buffer>,
        view: BufferViewKey,
        access: ResourceAccess,
    ) {
        let Some((v, _)) =
            self.access_buffer_view::<{ stage::COMPUTE }>(buffer, view, to_access(access))
        else {
            return;
        };
        self.encode_compute_command(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: NO_NEXT,
            texture: v.texture,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn set_compute_buffer(
        &mut self,
        index: u32,
        buffer: &Arc<Buffer>,
        offset: u32,
        length: u32,
        access: ResourceAccess,
    ) {
        let _ = (offset, length);
        let Some((alloc, _)) = self.access_buffer::<{ stage::COMPUTE }>(buffer, to_access(access))
        else {
            return;
        };
        self.encode_compute_command(WmtcmdComputeSetbuffer {
            r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
            reserved: [0; 3],
            next: NO_NEXT,
            buffer: alloc.buffer(),
            offset: 0,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn set_compute_bytes(&mut self, index: u32, bytes: &[u8]) {
        let dst = self.allocate_cpu(bytes.len().max(1), 16);
        // SAFETY: `dst` has at least `bytes.len()` bytes.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), dst.as_ptr(), bytes.len());
        }
        self.encode_compute_command(WmtcmdComputeSetbytes {
            r#type: WMT_COMPUTE_COMMAND_SET_BYTES,
            reserved: [0; 3],
            next: NO_NEXT,
            bytes: raw_mem_ptr(dst.as_ptr().expose_provenance()),
            length: bytes.len() as u64,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn default_library(&self) -> &Library {
        self.helper_psos
            .library
            .as_ref()
            .expect("default library initialized")
    }

    fn start_render_pass(&mut self) {
        let argbuf = self.allocate_argument_buffer(0);
        Self::start_render_pass(self, 0, 0, 0, Some(argbuf));
        if let Some(targets) = self.render_targets() {
            targets.render_target_width = 16384;
            targets.render_target_height = 16384;
            targets.render_target_array_length = 0;
            targets.default_raster_sample_count = 1;
        }
    }

    fn set_color_attachment(
        &mut self,
        index: u32,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        depth_plane: u32,
    ) {
        let attachment = self.attachment::<{ stage::PIXEL }>(texture, view, Access::WRITE);
        let [w, h, d] = texture.view_size(view);
        let array_len = if texture.info().r#type() == WMT_TEXTURE_TYPE3D {
            d.saturating_sub(depth_plane)
        } else {
            view.array_size()
        };
        let samples = u8::try_from(texture.info().sample_count()).unwrap_or(1);
        let Some(targets) = self.render_targets() else {
            return;
        };
        let idx = index as usize;
        if let Some(color) = targets.colors.get_mut(idx) {
            color.target.attachment = attachment;
            color.target.depth_plane = depth_plane;
            color.target.load_action = WMT_LOAD_ACTION_LOAD;
            color.target.store_action = WMT_STORE_ACTION_STORE;
        }
        targets.render_target_count = targets
            .render_target_count
            .max(u8::try_from(index + 1).unwrap_or(8));
        targets.render_target_width = targets.render_target_width.min(w);
        targets.render_target_height = targets.render_target_height.min(h);
        targets.render_target_array_length = u16::try_from(array_len).unwrap_or(0);
        targets.default_raster_sample_count = samples;
    }

    fn set_depth_stencil_attachment(
        &mut self,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        dsv_flag: u32,
    ) {
        let depth_att = (dsv_flag & 1 != 0)
            .then(|| self.attachment::<{ stage::PIXEL }>(texture, view, Access::WRITE))
            .flatten();
        let stencil_att = (dsv_flag & 2 != 0)
            .then(|| self.attachment::<{ stage::PIXEL }>(texture, view, Access::WRITE))
            .flatten();
        let [w, h, _] = texture.view_size(view);
        let array_len = view.array_size();
        let samples = u8::try_from(texture.info().sample_count()).unwrap_or(1);
        let Some(targets) = self.render_targets() else {
            return;
        };
        if dsv_flag & 1 != 0 {
            targets.depth.target.attachment = depth_att;
            targets.depth.target.load_action = WMT_LOAD_ACTION_LOAD;
            targets.depth.target.store_action = WMT_STORE_ACTION_STORE;
        }
        if dsv_flag & 2 != 0 {
            targets.stencil.target.attachment = stencil_att;
            targets.stencil.target.load_action = WMT_LOAD_ACTION_LOAD;
            targets.stencil.target.store_action = WMT_STORE_ACTION_STORE;
        }
        targets.dsv_planar_flags = u8::try_from(dsv_flag).unwrap_or(0);
        targets.render_target_width = targets.render_target_width.min(w);
        targets.render_target_height = targets.render_target_height.min(h);
        targets.render_target_array_length = u16::try_from(array_len).unwrap_or(0);
        targets.default_raster_sample_count = samples;
    }

    fn set_render_pso(&mut self, pso: &Obj) {
        self.encode_render_command(WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso: pso.handle(),
        });
    }

    fn set_viewport(&mut self, viewport: WMTViewport) {
        self.encode_render_command(WmtcmdRenderSetviewport {
            r#type: WMT_RENDER_COMMAND_SET_VIEWPORT,
            reserved: [0; 3],
            next: NO_NEXT,
            viewport,
        });
    }

    fn set_depth_stencil_state(&mut self, state: &Obj) {
        self.encode_render_command(WmtcmdRenderSetdepthstencilstate {
            r#type: WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
            reserved: [0; 3],
            next: NO_NEXT,
            depth_stencil_state: state.handle(),
        });
    }

    fn set_scissor_rect(&mut self, rect: WMTScissorRect) {
        self.encode_render_command(WmtcmdRenderSetscissorrect {
            r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
            reserved: [0; 3],
            next: NO_NEXT,
            scissor_rect: rect,
        });
    }

    fn set_stencil_reference(&mut self, stencil_ref: u8) {
        self.encode_render_command(WmtcmdRenderSetstencilref {
            r#type: WMT_RENDER_COMMAND_SET_STENCIL_REF,
            reserved: [0; 3],
            next: NO_NEXT,
            stencil_ref,
        });
    }

    fn set_fragment_bytes(&mut self, index: u32, bytes: &[u8]) {
        let dst = self.allocate_cpu(bytes.len().max(1), 16);
        // SAFETY: `dst` has at least `bytes.len()` bytes.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), dst.as_ptr(), bytes.len());
        }
        self.encode_render_command(WmtcmdRenderSetbytes {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
            reserved: [0; 3],
            next: NO_NEXT,
            bytes: raw_mem_ptr(dst.as_ptr().expose_provenance()),
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
        self.encode_render_command(WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type: primitive,
            vertex_start: u64::from(vertex_start),
            vertex_count: u64::from(vertex_count),
            instance_count,
            base_instance: base_instance.cast_unsigned(),
        });
    }
}

#[cfg(test)]
mod tests;
