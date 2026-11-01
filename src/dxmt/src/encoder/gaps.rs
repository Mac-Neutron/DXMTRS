//! D3D11 front-end additions on [`Encoder`] (`dxmt_context.*`).
//!
//! Stubbed in Task 3 with their final signatures from the Task 0 audit; Task 6 fills in the
//! bodies.

use core::ptr::NonNull;
use std::sync::Arc;

use super::Encoder;
use crate::{
    counter::Counter,
    deptrack::Access,
    frame_stats::{CompatibilityFlags, FrameStatistics},
    query::{TimestampQuery, VisibilityQuery},
    resource::{Buffer, BufferAllocation, Texture, TextureAllocation},
    staging::Staging,
};

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
    pub const fn access_counter<const S: u8>(
        &mut self,
        counter: &Arc<Counter>,
        uav_buf: Option<&Arc<Buffer>>,
    ) -> Option<(Arc<BufferAllocation>, u64)> {
        let _ = (counter, uav_buf);
        None
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
        self.access_buffer::<S>(buf, access)
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
    pub const fn use_render_resource(&mut self, resource: u64, usage: u32, stages: u8) {
        let _ = (resource, usage, stages);
    }

    /// Allocates `(buffer, offset)` from the copy temp ring (`allocateTempBuffer`).
    pub const fn allocate_temp_buffer(&mut self, size: u64, align: u64) -> (u64, u64) {
        let _ = (size, align);
        (0, 0)
    }

    /// Allocates a [`TempBufferSlice`] from the copy temp ring (`allocateTempBuffer1`).
    pub const fn allocate_temp_buffer_slice(&mut self, size: u64, align: u64) -> TempBufferSlice {
        let _ = (size, align);
        TempBufferSlice {
            buffer: 0,
            offset: 0,
            gpu_address: 0,
        }
    }

    /// CPU pointer for `offset` inside the pass's argument buffer (`getMappedArgumentBuffer`).
    pub fn mapped_argument_buffer(
        &mut self,
        offset: usize,
        size: usize,
        align: usize,
    ) -> NonNull<u8> {
        let _ = offset;
        self.allocate_cpu(size, align)
    }

    /// Final offset of `offset` in the pass's argument buffer (`getFinalArgumentBufferOffset`).
    #[must_use]
    pub const fn final_argument_buffer_offset(&self, offset: usize) -> u64 {
        offset as u64
    }

    /// `(buffer, offset)` of the pass's argument buffer (`getFinalArgumentBuffer`).
    #[must_use]
    pub const fn final_argument_buffer(&self) -> (u64, u64) {
        (0, 0)
    }

    /// Updates the bound index buffer's byte offset (`bindIndexBufferOffset`).
    pub const fn bind_index_buffer_offset(&mut self, offset: u32) {
        let _ = offset;
    }

    /// Bound index buffer's Metal handle, allocation offset and sub-offset (`currentIndexBuffer`).
    pub fn current_index_buffer_with_offset(&mut self) -> Option<(u64, u64, u32)> {
        self.current_index_buffer().map(|(buf, off)| (buf, off, 0))
    }

    /// Sets `counter` to `value` for subsequent draws/dispatches (`CounterPool::updateCounter`).
    pub const fn update_counter(&mut self, counter: &Counter, value: u32) {
        let _ = (counter, value);
    }

    /// Begins occlusion query `query` (`beginVisibilityResultQuery`).
    pub fn begin_visibility_query(&mut self, query: Arc<VisibilityQuery>) {
        drop(query);
    }

    /// Ends occlusion query `query` (`endVisibilityResultQuery`).
    pub fn end_visibility_query(&mut self, query: Arc<VisibilityQuery>) {
        drop(query);
    }

    /// Advances the visibility-result offset before a draw (`bumpVisibilityResultOffset`).
    pub const fn bump_visibility_result_offset(&mut self) {}

    /// Pushes a deferred command list's visibility query array (`pushDeferredVisibilityQueries`).
    pub const fn push_deferred_visibility_queries(&mut self, list: *const Arc<VisibilityQuery>) {
        let _ = list;
    }

    /// Pops the top deferred visibility query array (`popDeferredVisibilityQueries`).
    pub const fn pop_deferred_visibility_queries(&mut self) {}

    /// Looks up query `query_id` in the active deferred visibility query array.
    #[must_use]
    pub const fn current_deferred_visibility_query(
        &self,
        query_id: u32,
    ) -> Option<Arc<VisibilityQuery>> {
        let _ = query_id;
        None
    }

    /// Issues timestamp query `query` at the current pass boundary (`sampleTimestamp`).
    pub fn sample_timestamp_query(&mut self, query: Arc<TimestampQuery>) {
        drop(query);
    }

    /// Queues a GS indirect argument marshal task (`encodeGSDispatchArgumentsMarshal`).
    pub const fn encode_gs_dispatch_arguments_marshal(
        &mut self,
        draw_args_buf: u64,
        draw_args_va: u64,
        vertex_count_per_warp: u32,
        dispatch_args_buf: u64,
        dispatch_args_va: u64,
    ) {
        let _ = (
            draw_args_buf,
            draw_args_va,
            vertex_count_per_warp,
            dispatch_args_buf,
            dispatch_args_va,
        );
    }

    /// Queues a tessellation indirect argument marshal task (`encodeTSDispatchArgumentsMarshal`).
    pub const fn encode_ts_dispatch_arguments_marshal(
        &mut self,
        draw_args_buf: u64,
        draw_args_va: u64,
        control_point_count: u16,
        patch_per_group: u16,
        dispatch_args_buf: u64,
        dispatch_args_va: u64,
    ) {
        let _ = (
            draw_args_buf,
            draw_args_va,
            control_point_count,
            patch_per_group,
            dispatch_args_buf,
            dispatch_args_va,
        );
    }

    /// Copies a depth-stencil image from buffer `src` to texture `dst` (`DepthStencilBlitContext`).
    pub const fn depth_stencil_copy_from_buffer(
        &mut self,
        src: (&Buffer, u64, u32, u32),
        dst: (&Arc<Texture>, u32, u32, u8),
        offset: [u32; 3],
        size: [u32; 3],
    ) {
        let _ = (src, dst, offset, size);
    }

    /// Copies a depth-stencil subresource between textures (`DepthStencilBlitContext`).
    pub const fn depth_stencil_copy_from_texture(
        &mut self,
        src: (&Arc<Texture>, u32, u32, [u32; 3], [u32; 3]),
        dst: (&Arc<Texture>, u32, u32, u8, [u32; 3]),
    ) {
        let _ = (src, dst);
    }

    /// Scales a motion-vector texture via `MetalFX` MV scale compute (`MTLFXMVScaleContext`).
    pub const fn mv_scale_dispatch(
        &mut self,
        src: &Arc<Texture>,
        dst: &Arc<Texture>,
        scale_x: f32,
        scale_y: f32,
    ) {
        let _ = (src, dst, scale_x, scale_y);
    }

    /// Dispatches the tile barrier draw in the active render pass (`TileBarrierContext`).
    pub const fn dispatch_tile_barrier(&mut self) {}

    /// Current frame id (`ArgumentEncodingContext::currentFrameId`).
    #[must_use]
    pub const fn current_frame_id(&self) -> u64 {
        0
    }

    /// Mutable access to the current frame's statistics (`currentFrameStatistics`).
    pub const fn current_frame_statistics(&mut self) -> Option<&mut FrameStatistics> {
        None
    }

    /// Records a feature-compatibility flag on the current frame (`setCompatibilityFlag`).
    pub const fn set_compatibility_flag(&mut self, flag: CompatibilityFlags) {
        let _ = flag;
    }
}
