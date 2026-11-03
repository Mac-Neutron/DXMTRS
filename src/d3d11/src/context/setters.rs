//! Context state setter methods (`*Set*`, `SetPredication`, `SetResourceMinLOD`).

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::{
    encoder::{BoundSampler, ResourceView, stage},
    frame_stats::CompatibilityFlags,
    resource::Buffer,
};

use super::{
    CommandBufferState, Context, Mode,
    state::{
        CONSTANT_BUFFER_SLOTS, ComPtr, ConstantBufferEntry, DEFAULT_BLEND_FACTOR, DirtyFlags,
        KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL, KEEP_UNORDERED_ACCESS_VIEWS, NUM_SHADER_STAGES,
        RENDER_TARGET_SLOTS, REQ_CONSTANT_BUFFER_ELEMENTS, SAMPLER_SLOTS, SRV_SLOTS,
        STREAM_OUTPUT_SLOTS, SamplerEntry, ShaderResourceViewEntry, ShaderStage,
        StreamOutputBufferEntry, UAV_SLOTS, UnorderedAccessViewEntry, VERTEX_BUFFER_SLOTS,
        VIEWPORT_AND_SCISSOR_SLOTS, VertexBufferEntry,
    },
};
use crate::{
    abi::{
        ext::{IID_IMTL_MIN_LOD_CLAMPABLE, IMTLMinLODClampableVtbl},
        records::{D3d11Viewport, Rect},
        vtbl::IUnknownVtbl,
    },
    com::{S_OK, release},
    subresource::ResourceSubset,
    view::{Dsv, Rtv, Srv, Uav},
};

const D3D11_BIND_VERTEX_BUFFER: u32 = 0x1;
const D3D11_BIND_INDEX_BUFFER: u32 = 0x2;
const D3D11_BIND_CONSTANT_BUFFER: u32 = 0x4;
const D3D11_BIND_SHADER_RESOURCE: u32 = 0x8;
const D3D11_BIND_STREAM_OUTPUT: u32 = 0x10;
const D3D11_BIND_RENDER_TARGET: u32 = 0x20;
const D3D11_BIND_DEPTH_STENCIL: u32 = 0x40;
const D3D11_BIND_UNORDERED_ACCESS: u32 = 0x80;

const fn viewport_eq(a: D3d11Viewport, b: D3d11Viewport) -> bool {
    a.top_left_x.to_bits() == b.top_left_x.to_bits()
        && a.top_left_y.to_bits() == b.top_left_y.to_bits()
        && a.width.to_bits() == b.width.to_bits()
        && a.height.to_bits() == b.height.to_bits()
        && a.min_depth.to_bits() == b.min_depth.to_bits()
        && a.max_depth.to_bits() == b.max_depth.to_bits()
}

const fn rect_eq(a: Rect, b: Rect) -> bool {
    a.left == b.left && a.top == b.top && a.right == b.right && a.bottom == b.bottom
}

const fn read_ptr_at(arr: *const *mut c_void, index: usize) -> *mut c_void {
    if arr.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `arr` is non-null and the caller bounds `index` within the API array length.
    let slot_ptr = unsafe { arr.add(index) };
    // SAFETY: `slot_ptr` points to element `index` of the caller-supplied pointer array.
    unsafe { *slot_ptr }
}

const fn read_u32_at(arr: *const u32, index: usize, default: u32) -> u32 {
    if arr.is_null() {
        return default;
    }
    // SAFETY: `arr` is non-null and the caller bounds `index` within the API array length.
    let elem_ptr = unsafe { arr.add(index) };
    // SAFETY: `elem_ptr` points to element `index` of the caller-supplied `u32` array.
    unsafe { *elem_ptr }
}

const fn read_blend_factor(blend_factor: *const f32) -> [f32; 4] {
    if blend_factor.is_null() {
        return DEFAULT_BLEND_FACTOR;
    }
    // SAFETY: `blend_factor` is non-null and points to `FLOAT[4]` (index 0).
    let f0 = unsafe { *blend_factor };
    // SAFETY: `blend_factor` points to `FLOAT[4]` (index 1).
    let p1 = unsafe { blend_factor.add(1) };
    // SAFETY: `p1` points to element 1 of `blend_factor`.
    let f1 = unsafe { *p1 };
    // SAFETY: `blend_factor` points to `FLOAT[4]` (index 2).
    let p2 = unsafe { blend_factor.add(2) };
    // SAFETY: `p2` points to element 2 of `blend_factor`.
    let f2 = unsafe { *p2 };
    // SAFETY: `blend_factor` points to `FLOAT[4]` (index 3).
    let p3 = unsafe { blend_factor.add(3) };
    // SAFETY: `p3` points to element 3 of `blend_factor`.
    let f3 = unsafe { *p3 };
    [f0, f1, f2, f3]
}

fn set_resource_min_lod_inner(resource: *mut c_void, min_lod: f32) {
    if resource.is_null() {
        return;
    }
    // SAFETY: `resource` is a non-null COM object whose first word is its vtable pointer.
    let vtbl = unsafe { *resource.cast::<*const IUnknownVtbl>() };
    if vtbl.is_null() {
        return;
    }
    let iid = IID_IMTL_MIN_LOD_CLAMPABLE;
    let mut clamp_iface: *mut c_void = ptr::null_mut();
    // SAFETY: `vtbl` points to a valid `IUnknownVtbl`.
    let qi = unsafe { (*vtbl).query_interface };
    // SAFETY: calls `IUnknown::QueryInterface` for `IMTLMinLODClampable`.
    let hr = unsafe { qi(resource, (&raw const iid).cast(), &raw mut clamp_iface) };
    if hr == S_OK && !clamp_iface.is_null() {
        // SAFETY: `clamp_iface` is a valid `IMTLMinLODClampable` interface pointer.
        let clamp_vtbl = unsafe { *clamp_iface.cast::<*const IMTLMinLODClampableVtbl>() };
        // SAFETY: `clamp_vtbl` points to `IMTLMinLODClampableVtbl`.
        let set_fn = unsafe { (*clamp_vtbl).set_min_lod };
        // SAFETY: invokes `IMTLMinLODClampable::SetMinLOD`.
        unsafe { set_fn(clamp_iface, min_lod) };
        // SAFETY: balances the reference returned by `QueryInterface` on `resource`.
        unsafe { release(resource) };
    }
}

impl<M: Mode> Context<M> {
    fn emit_bind_cb(
        &mut self,
        stage_idx: usize,
        slot: usize,
        offset: u32,
        buf: Option<Arc<Buffer>>,
    ) {
        M::emit_st(self, move |enc| match stage_idx {
            0 => enc.bind_constant_buffer::<{ stage::VERTEX }>(slot, offset, buf.clone()),
            1 => enc.bind_constant_buffer::<{ stage::PIXEL }>(slot, offset, buf.clone()),
            2 => enc.bind_constant_buffer::<{ stage::GEOMETRY }>(slot, offset, buf.clone()),
            3 => enc.bind_constant_buffer::<{ stage::HULL }>(slot, offset, buf.clone()),
            4 => enc.bind_constant_buffer::<{ stage::DOMAIN }>(slot, offset, buf.clone()),
            5 => enc.bind_constant_buffer::<{ stage::COMPUTE }>(slot, offset, buf.clone()),
            _ => {}
        });
    }

    fn emit_bind_cb_offset(&mut self, stage_idx: usize, slot: usize, offset: u32) {
        M::emit_st(self, move |enc| match stage_idx {
            0 => enc.bind_constant_buffer_offset::<{ stage::VERTEX }>(slot, offset),
            1 => enc.bind_constant_buffer_offset::<{ stage::PIXEL }>(slot, offset),
            2 => enc.bind_constant_buffer_offset::<{ stage::GEOMETRY }>(slot, offset),
            3 => enc.bind_constant_buffer_offset::<{ stage::HULL }>(slot, offset),
            4 => enc.bind_constant_buffer_offset::<{ stage::DOMAIN }>(slot, offset),
            5 => enc.bind_constant_buffer_offset::<{ stage::COMPUTE }>(slot, offset),
            _ => {}
        });
    }

    fn emit_bind_sampler(&mut self, stage_idx: usize, slot: usize, sampler: Option<BoundSampler>) {
        let clone_samp = move || {
            sampler.as_ref().map(|s| BoundSampler {
                state: s.state,
                cube_state: s.cube_state,
                lod_bias: s.lod_bias,
                owner: Arc::clone(&s.owner),
            })
        };
        M::emit_st(self, move |enc| match stage_idx {
            0 => enc.bind_sampler::<{ stage::VERTEX }>(slot, clone_samp()),
            1 => enc.bind_sampler::<{ stage::PIXEL }>(slot, clone_samp()),
            2 => enc.bind_sampler::<{ stage::GEOMETRY }>(slot, clone_samp()),
            3 => enc.bind_sampler::<{ stage::HULL }>(slot, clone_samp()),
            4 => enc.bind_sampler::<{ stage::DOMAIN }>(slot, clone_samp()),
            5 => enc.bind_sampler::<{ stage::COMPUTE }>(slot, clone_samp()),
            _ => {}
        });
    }

    fn emit_unbind_srv(&mut self, stage_idx: usize, slot: usize) {
        M::emit_st(self, move |enc| match stage_idx {
            0 => enc.bind_shader_resource::<{ stage::VERTEX }>(slot, ResourceView::None),
            1 => enc.bind_shader_resource::<{ stage::PIXEL }>(slot, ResourceView::None),
            2 => enc.bind_shader_resource::<{ stage::GEOMETRY }>(slot, ResourceView::None),
            3 => enc.bind_shader_resource::<{ stage::HULL }>(slot, ResourceView::None),
            4 => enc.bind_shader_resource::<{ stage::DOMAIN }>(slot, ResourceView::None),
            5 => enc.bind_shader_resource::<{ stage::COMPUTE }>(slot, ResourceView::None),
            _ => {}
        });
    }

    fn emit_bind_srv(&mut self, stage_idx: usize, slot: usize, srv: &Srv) {
        let slice = srv.slice;
        if let Some(buf) = srv.buffer.clone() {
            let key = srv.buffer_view.unwrap_or_default();
            M::emit_st(self, move |enc| {
                let view = ResourceView::Buffer {
                    buffer: Arc::clone(&buf),
                    key,
                    slice,
                };
                match stage_idx {
                    0 => enc.bind_shader_resource::<{ stage::VERTEX }>(slot, view),
                    1 => enc.bind_shader_resource::<{ stage::PIXEL }>(slot, view),
                    2 => enc.bind_shader_resource::<{ stage::GEOMETRY }>(slot, view),
                    3 => enc.bind_shader_resource::<{ stage::HULL }>(slot, view),
                    4 => enc.bind_shader_resource::<{ stage::DOMAIN }>(slot, view),
                    5 => enc.bind_shader_resource::<{ stage::COMPUTE }>(slot, view),
                    _ => {}
                }
            });
        } else if let Some(tex) = srv.texture.clone() {
            let key = srv.texture_view.unwrap_or_default();
            M::emit_st(self, move |enc| {
                let view = ResourceView::Texture {
                    texture: Arc::clone(&tex),
                    key,
                };
                match stage_idx {
                    0 => enc.bind_shader_resource::<{ stage::VERTEX }>(slot, view),
                    1 => enc.bind_shader_resource::<{ stage::PIXEL }>(slot, view),
                    2 => enc.bind_shader_resource::<{ stage::GEOMETRY }>(slot, view),
                    3 => enc.bind_shader_resource::<{ stage::HULL }>(slot, view),
                    4 => enc.bind_shader_resource::<{ stage::DOMAIN }>(slot, view),
                    5 => enc.bind_shader_resource::<{ stage::COMPUTE }>(slot, view),
                    _ => {}
                }
            });
        }
    }

    fn emit_unbind_uav<const STAGE: u8>(&mut self, slot: usize) {
        M::emit_st(self, move |enc| {
            enc.bind_unordered_access::<STAGE>(slot, ResourceView::None);
        });
    }

    fn emit_bind_uav<const STAGE: u8>(&mut self, slot: usize, uav: &Uav) {
        let slice = uav.slice;
        let counter = uav.counter.clone();
        if let Some(buf) = uav.buffer.clone() {
            let key = uav.buffer_view.unwrap_or_default();
            M::emit_st(self, move |enc| {
                let view = ResourceView::Buffer {
                    buffer: Arc::clone(&buf),
                    key,
                    slice,
                };
                enc.bind_unordered_access_with_counter::<STAGE>(slot, view, counter.clone());
            });
        } else if let Some(tex) = uav.texture.clone() {
            let key = uav.texture_view.unwrap_or_default();
            M::emit_st(self, move |enc| {
                let view = ResourceView::Texture {
                    texture: Arc::clone(&tex),
                    key,
                };
                enc.bind_unordered_access_with_counter::<STAGE>(slot, view, counter.clone());
            });
        }
    }

    fn resolve_cs_srv_hazard(&mut self, resource: *mut c_void, subset: ResourceSubset) {
        let cs_idx = ShaderStage::Compute.index();
        let mut to_unbind = Vec::new();
        for slot in 0..SRV_SLOTS {
            if let Some(entry) = self.state.shader_stages[cs_idx].srvs.get(slot)
                && let Some(srv) = Srv::from_iface(entry.srv.as_ptr())
                && srv.resource == resource
                && srv.subset.overlaps(subset)
            {
                to_unbind.push(slot);
            }
        }
        for slot in to_unbind {
            self.state.shader_stages[cs_idx].srvs.unbind(slot);
            self.emit_unbind_srv(cs_idx, slot);
        }
    }

    fn resolve_om_srv_hazard(&mut self, resource: *mut c_void, subset: ResourceSubset) {
        for stage_idx in 0..5 {
            let mut to_unbind = Vec::new();
            for slot in 0..SRV_SLOTS {
                if let Some(entry) = self.state.shader_stages[stage_idx].srvs.get(slot)
                    && let Some(srv) = Srv::from_iface(entry.srv.as_ptr())
                    && srv.resource == resource
                    && srv.subset.overlaps(subset)
                {
                    to_unbind.push(slot);
                }
            }
            for slot in to_unbind {
                self.state.shader_stages[stage_idx].srvs.unbind(slot);
                self.emit_unbind_srv(stage_idx, slot);
            }
        }
    }

    fn resolve_om_ia_hazard(&mut self, resource: *mut c_void) {
        let Some(common) = crate::resource::common(resource) else {
            return;
        };
        if (common.bind_flags & D3D11_BIND_VERTEX_BUFFER) != 0 {
            let mut to_unbind = Vec::new();
            for slot in 0..VERTEX_BUFFER_SLOTS {
                if let Some(entry) = self.state.input_assembler.vertex_buffers.get(slot)
                    && entry.buffer.as_ptr() == resource
                {
                    to_unbind.push(slot);
                }
            }
            for slot in to_unbind {
                self.state.input_assembler.vertex_buffers.unbind(slot);
                self.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
                M::emit_st(self, move |enc| {
                    enc.bind_vertex_buffer(slot, 0, 0, None);
                });
            }
        }
        if (common.bind_flags & D3D11_BIND_INDEX_BUFFER) != 0
            && self.state.input_assembler.index_buffer.as_ptr() == resource
        {
            let _ = self.state.input_assembler.index_buffer.set(ptr::null_mut());
            self.dirty.insert(DirtyFlags::INDEX_BUFFER);
            let fmt = self.state.input_assembler.index_buffer_format;
            let off = self.state.input_assembler.index_buffer_offset;
            self.set_encode_index_buffer(0, fmt, off);
            M::emit_st(self, |enc| {
                enc.bind_index_buffer(None);
            });
        }
    }

    fn resolve_om_rtv_hazard(&mut self, resource: *mut c_void, subset: ResourceSubset) -> bool {
        let Some(common) = crate::resource::common(resource) else {
            return false;
        };
        if (common.bind_flags & D3D11_BIND_RENDER_TARGET) == 0 {
            return false;
        }
        let mut invalidated = false;
        for rtv_slot in &mut self.state.output_merger.rtvs {
            if let Some(rtv) = Rtv::from_iface(rtv_slot.as_ptr())
                && rtv.resource == resource
                && rtv.subset.overlaps(subset)
            {
                let _ = rtv_slot.set(ptr::null_mut());
                invalidated = true;
            }
        }
        invalidated
    }

    fn validate_srv_hazard(&self, stage: ShaderStage, srv: &Srv) -> bool {
        let bind_flags = crate::resource::common(srv.resource).map_or(0, |c| c.bind_flags);
        if stage == ShaderStage::Compute {
            if (bind_flags & D3D11_BIND_UNORDERED_ACCESS) != 0 {
                for slot in 0..UAV_SLOTS {
                    if let Some(entry) = self.state.compute_stage_uav.uavs.get(slot)
                        && let Some(uav) = Uav::from_iface(entry.uav.as_ptr())
                        && uav.resource == srv.resource
                        && uav.subset.overlaps(srv.subset)
                    {
                        return false;
                    }
                }
            }
        } else {
            if (bind_flags & D3D11_BIND_RENDER_TARGET) != 0 {
                let num_rtvs =
                    (self.state.output_merger.num_rtvs as usize).min(RENDER_TARGET_SLOTS);
                for rtv_slot in self.state.output_merger.rtvs.iter().take(num_rtvs) {
                    if let Some(rtv) = Rtv::from_iface(rtv_slot.as_ptr())
                        && rtv.resource == srv.resource
                        && rtv.subset.overlaps(srv.subset)
                    {
                        return false;
                    }
                }
            }
            if (bind_flags & D3D11_BIND_DEPTH_STENCIL) != 0
                && let Some(dsv) = Dsv::from_iface(self.state.output_merger.dsv.as_ptr())
                && dsv.resource == srv.resource
                && dsv.subset.overlaps(srv.subset)
            {
                return false;
            }
            if (bind_flags & D3D11_BIND_UNORDERED_ACCESS) != 0 {
                for slot in 0..UAV_SLOTS {
                    if let Some(entry) = self.state.output_merger.uavs.get(slot)
                        && let Some(uav) = Uav::from_iface(entry.uav.as_ptr())
                        && uav.resource == srv.resource
                        && uav.subset.overlaps(srv.subset)
                    {
                        return false;
                    }
                }
            }
            if (bind_flags & D3D11_BIND_STREAM_OUTPUT) != 0 {
                for slot in 0..STREAM_OUTPUT_SLOTS {
                    if let Some(entry) = self.state.stream_output.targets.get(slot)
                        && entry.buffer.as_ptr() == srv.resource
                    {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn validate_ia_hazard(&self, buf_ptr: *mut c_void, bind_flags: u32) -> bool {
        if (bind_flags & D3D11_BIND_STREAM_OUTPUT) != 0 {
            for slot in 0..STREAM_OUTPUT_SLOTS {
                if let Some(entry) = self.state.stream_output.targets.get(slot)
                    && entry.buffer.as_ptr() == buf_ptr
                {
                    return false;
                }
            }
        }
        if (bind_flags & D3D11_BIND_UNORDERED_ACCESS) != 0 {
            for slot in 0..UAV_SLOTS {
                if let Some(entry) = self.state.output_merger.uavs.get(slot)
                    && let Some(uav) = Uav::from_iface(entry.uav.as_ptr())
                    && uav.resource == buf_ptr
                {
                    return false;
                }
            }
        }
        if (bind_flags & D3D11_BIND_RENDER_TARGET) != 0 {
            let num_rtvs = (self.state.output_merger.num_rtvs as usize).min(RENDER_TARGET_SLOTS);
            for rtv_slot in self.state.output_merger.rtvs.iter().take(num_rtvs) {
                if let Some(rtv) = Rtv::from_iface(rtv_slot.as_ptr())
                    && rtv.resource == buf_ptr
                {
                    return false;
                }
            }
        }
        true
    }

    fn validate_multi_output(
        num_rtvs: u32,
        render_target_views: *const *mut c_void,
        num_uavs: u32,
        unordered_access_views: *const *mut c_void,
    ) -> bool {
        let rtv_count = if num_rtvs == KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL {
            0
        } else {
            (num_rtvs as usize).min(RENDER_TARGET_SLOTS)
        };
        let uav_count = if num_uavs == KEEP_UNORDERED_ACCESS_VIEWS {
            0
        } else {
            (num_uavs as usize).min(UAV_SLOTS)
        };

        for i in 0..rtv_count {
            let rtv_ptr = read_ptr_at(render_target_views, i);
            let Some(rtv_i) = Rtv::from_iface(rtv_ptr) else {
                continue;
            };
            for j in 0..i {
                let prev_ptr = read_ptr_at(render_target_views, j);
                if let Some(rtv_j) = Rtv::from_iface(prev_ptr)
                    && rtv_i.resource == rtv_j.resource
                    && rtv_i.subset.overlaps(rtv_j.subset)
                {
                    return false;
                }
            }
            for j in 0..uav_count {
                let uav_ptr = read_ptr_at(unordered_access_views, j);
                if let Some(uav_j) = Uav::from_iface(uav_ptr)
                    && rtv_i.resource == uav_j.resource
                    && rtv_i.subset.overlaps(uav_j.subset)
                {
                    return false;
                }
            }
        }

        for i in 0..uav_count {
            let ptr_i = read_ptr_at(unordered_access_views, i);
            let Some(uav_i) = Uav::from_iface(ptr_i) else {
                continue;
            };
            for j in 0..i {
                let ptr_j = read_ptr_at(unordered_access_views, j);
                if let Some(uav_j) = Uav::from_iface(ptr_j)
                    && uav_i.resource == uav_j.resource
                    && uav_i.subset.overlaps(uav_j.subset)
                {
                    return false;
                }
            }
        }
        true
    }

    fn validate_set_render_targets(
        &mut self,
        num_rtvs: u32,
        render_target_views: *const *mut c_void,
        depth_stencil_view: *mut c_void,
    ) -> bool {
        let (mut has_ref, mut render_target_array_length, mut sample_count, mut width, mut height) =
            if depth_stencil_view.is_null() {
                (false, 0_u32, 1_u32, 0_u32, 0_u32)
            } else {
                let Some(dsv) = Dsv::from_iface(depth_stencil_view) else {
                    return false;
                };
                (
                    true,
                    dsv.array_length,
                    dsv.sample_count,
                    dsv.width,
                    dsv.height,
                )
            };

        let clamped = (num_rtvs as usize).min(RENDER_TARGET_SLOTS);
        for i in 0..clamped {
            let rtv_ptr = read_ptr_at(render_target_views, i);
            if rtv_ptr.is_null() {
                continue;
            }
            let Some(rtv) = Rtv::from_iface(rtv_ptr) else {
                return false;
            };
            if has_ref {
                if rtv.sample_count != sample_count {
                    return false;
                }
                if rtv.array_length != render_target_array_length
                    && rtv.array_length.max(render_target_array_length) != 1
                {
                    return false;
                }
                if width < rtv.width || height < rtv.height {
                    return false;
                }
                render_target_array_length = render_target_array_length.min(rtv.array_length);
                width = width.min(rtv.width);
                height = height.min(rtv.height);
            } else {
                has_ref = true;
                render_target_array_length = rtv.array_length;
                sample_count = rtv.sample_count;
                width = rtv.width;
                height = rtv.height;
            }
        }

        self.state.output_merger.sample_count = sample_count;
        self.state.output_merger.array_length = render_target_array_length;
        self.state.output_merger.render_target_width = width;
        self.state.output_merger.render_target_height = height;
        true
    }

    /// Re-emits encoder bindings for the current `ContextState` (`RestoreEncodingContextState`).
    pub fn restore_encoding_context_state(&mut self) {
        if !self.is_live() {
            return;
        }
        for slot in 0..VERTEX_BUFFER_SLOTS {
            if let Some(entry) = self.state.input_assembler.vertex_buffers.get(slot) {
                let offset = entry.offset;
                let stride = entry.stride;
                let buf =
                    crate::resource::common(entry.buffer.as_ptr()).and_then(|c| c.buffer.clone());
                M::emit_st(self, move |enc| {
                    enc.bind_vertex_buffer(slot, offset, stride, buf.clone());
                });
            }
        }
        let ib_ptr = self.state.input_assembler.index_buffer.as_ptr();
        let ib_fmt = self.state.input_assembler.index_buffer_format;
        let ib_off = self.state.input_assembler.index_buffer_offset;
        if ib_ptr.is_null() {
            self.set_encode_index_buffer(0, ib_fmt, ib_off);
        } else {
            let buf = crate::resource::common(ib_ptr).and_then(|c| c.buffer.clone());
            self.set_encode_index_buffer(u64::from(buf.is_some()), ib_fmt, ib_off);
            M::emit_st(self, move |enc| {
                enc.bind_index_buffer(buf.clone());
            });
        }
        for slot in 0..STREAM_OUTPUT_SLOTS {
            if let Some(entry) = self.state.stream_output.targets.get(slot) {
                let offset = entry.offset;
                let buf =
                    crate::resource::common(entry.buffer.as_ptr()).and_then(|c| c.buffer.clone());
                M::emit_st(self, move |enc| {
                    enc.bind_stream_output_buffer(slot, offset, buf.clone());
                });
            }
        }
        for stage_idx in 0..NUM_SHADER_STAGES {
            for slot in 0..CONSTANT_BUFFER_SLOTS {
                if let Some(entry) = self.state.shader_stages[stage_idx]
                    .constant_buffers
                    .get(slot)
                {
                    let offset = entry.offset;
                    let buf = crate::resource::common(entry.buffer.as_ptr())
                        .and_then(|c| c.buffer.clone());
                    self.emit_bind_cb(stage_idx, slot, offset, buf);
                }
            }
            for slot in 0..SAMPLER_SLOTS {
                if let Some(entry) = self.state.shader_stages[stage_idx].samplers.get(slot)
                    && let Some(samp) =
                        crate::state::SamplerState::from_iface(entry.sampler.as_ptr())
                {
                    self.emit_bind_sampler(stage_idx, slot, Some(samp.to_bound_sampler()));
                }
            }
            for slot in 0..SRV_SLOTS {
                if let Some(entry) = self.state.shader_stages[stage_idx].srvs.get(slot)
                    && let Some(srv) = Srv::from_iface(entry.srv.as_ptr())
                {
                    self.emit_bind_srv(stage_idx, slot, srv);
                }
            }
        }
        for slot in 0..UAV_SLOTS {
            if let Some(entry) = self.state.output_merger.uavs.get(slot)
                && let Some(uav) = Uav::from_iface(entry.uav.as_ptr())
            {
                self.emit_bind_uav::<{ stage::PIXEL }>(slot, uav);
            }
            if let Some(entry) = self.state.compute_stage_uav.uavs.get(slot)
                && let Some(uav) = Uav::from_iface(entry.uav.as_ptr())
            {
                self.emit_bind_uav::<{ stage::COMPUTE }>(slot, uav);
            }
        }
    }

    fn set_constant_buffer(
        &mut self,
        stage: ShaderStage,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        let is_vertex = stage == ShaderStage::Vertex;
        let stage_idx = stage.index();
        let live = self.is_live();
        for i in 0..(num_buffers as usize) {
            let slot = (start_slot as usize) + i;
            if slot >= CONSTANT_BUFFER_SLOTS {
                break;
            }
            let buf_ptr = read_ptr_at(buffers, i);
            let bind_ok = !buf_ptr.is_null()
                && (!live
                    || crate::resource::common(buf_ptr)
                        .is_some_and(|c| (c.bind_flags & D3D11_BIND_CONSTANT_BUFFER) != 0));
            if !bind_ok {
                if !self.state.shader_stages[stage_idx]
                    .constant_buffers
                    .unbind(slot)
                {
                    continue;
                }
                if live {
                    self.emit_bind_cb(stage_idx, slot, 0, None);
                } else if self.cmdbuf_state == CommandBufferState::RenderEncoderActive && is_vertex
                {
                    M::emit_st(self, move |enc| {
                        enc.bind_constant_buffer::<{ stage::VERTEX }>(slot, 0, None);
                    });
                }
                continue;
            }
            let fc = read_u32_at(first_constant, i, 0);
            let nc = read_u32_at(num_constants, i, REQ_CONSTANT_BUFFER_ELEMENTS);
            let arg_offset = fc * 16;
            let arg_length = nc * 16;
            if let Some((replacement, entry)) = self.state.shader_stages[stage_idx]
                .constant_buffers
                .bind_with(
                    slot,
                    |e| e.buffer.as_ptr() == buf_ptr,
                    || ConstantBufferEntry {
                        buffer: ComPtr::from_raw_or_null(buf_ptr),
                        offset: arg_offset,
                        length: arg_length,
                    },
                )
            {
                if replacement {
                    if entry.offset == arg_offset && entry.length == arg_length {
                        continue;
                    }
                    let offset_changed = entry.offset != arg_offset;
                    entry.offset = arg_offset;
                    entry.length = arg_length;
                    self.state.shader_stages[stage_idx]
                        .constant_buffers
                        .set_dirty(slot);
                    if live {
                        if offset_changed {
                            self.emit_bind_cb_offset(stage_idx, slot, arg_offset);
                        }
                    } else if self.cmdbuf_state == CommandBufferState::RenderEncoderActive
                        && is_vertex
                    {
                        M::emit_st(self, move |enc| {
                            enc.bind_constant_buffer_offset::<{ stage::VERTEX }>(slot, arg_offset);
                        });
                    }
                } else if live {
                    let buf = crate::resource::common(buf_ptr).and_then(|c| c.buffer.clone());
                    self.emit_bind_cb(stage_idx, slot, arg_offset, buf);
                }
            }
        }
    }

    fn set_shader_resource(
        &mut self,
        stage: ShaderStage,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        let stage_idx = stage.index();
        let live = self.is_live();
        for i in 0..(num_views as usize) {
            let slot = (start_slot as usize) + i;
            if slot >= SRV_SLOTS {
                break;
            }
            let srv_ptr = read_ptr_at(views, i);
            if srv_ptr.is_null() {
                if self.state.shader_stages[stage_idx].srvs.unbind(slot) && live {
                    self.emit_unbind_srv(stage_idx, slot);
                }
            } else if live {
                let valid_srv = Srv::from_iface(srv_ptr).filter(|srv| {
                    let bf = crate::resource::common(srv.resource).map_or(0, |c| c.bind_flags);
                    (bf & D3D11_BIND_SHADER_RESOURCE) != 0 && self.validate_srv_hazard(stage, srv)
                });
                let Some(srv) = valid_srv else {
                    if self.state.shader_stages[stage_idx].srvs.unbind(slot) {
                        self.emit_unbind_srv(stage_idx, slot);
                    }
                    continue;
                };
                if let Some((replacement, _)) = self.state.shader_stages[stage_idx].srvs.bind_with(
                    slot,
                    |e| e.srv.as_ptr() == srv_ptr,
                    || ShaderResourceViewEntry {
                        srv: ComPtr::from_raw_or_null(srv_ptr),
                    },
                ) && !replacement
                {
                    self.emit_bind_srv(stage_idx, slot, srv);
                }
            } else {
                let _ = self.state.shader_stages[stage_idx].srvs.bind_with(
                    slot,
                    |e| e.srv.as_ptr() == srv_ptr,
                    || ShaderResourceViewEntry {
                        srv: ComPtr::from_raw_or_null(srv_ptr),
                    },
                );
            }
        }
    }

    fn set_samplers(
        &mut self,
        stage: ShaderStage,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        let stage_idx = stage.index();
        let live = self.is_live();
        for i in 0..(num_samplers as usize) {
            let slot = (start_slot as usize) + i;
            if slot >= SAMPLER_SLOTS {
                break;
            }
            let samp_ptr = read_ptr_at(samplers, i);
            if samp_ptr.is_null() {
                if self.state.shader_stages[stage_idx].samplers.unbind(slot) && live {
                    self.emit_bind_sampler(stage_idx, slot, None);
                }
            } else if let Some((replacement, _)) =
                self.state.shader_stages[stage_idx].samplers.bind_with(
                    slot,
                    |e| e.sampler.as_ptr() == samp_ptr,
                    || SamplerEntry {
                        sampler: ComPtr::from_raw_or_null(samp_ptr),
                    },
                )
                && !replacement
                && live
                && let Some(samp) = crate::state::SamplerState::from_iface(samp_ptr)
            {
                self.emit_bind_sampler(stage_idx, slot, Some(samp.to_bound_sampler()));
            }
        }
    }

    fn set_shader(
        &mut self,
        stage: ShaderStage,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        let _ = (class_instances, num_class_instances);
        let is_compute = stage == ShaderStage::Compute;
        let stage_idx = stage.index();
        if !self.state.shader_stages[stage_idx].shader.set(shader) {
            return;
        }
        self.state.shader_stages[stage_idx]
            .constant_buffers
            .set_all_dirty();
        self.state.shader_stages[stage_idx].srvs.set_all_dirty();
        self.state.shader_stages[stage_idx].samplers.set_all_dirty();
        if stage == ShaderStage::Vertex {
            self.state.input_assembler.vertex_buffers.set_all_dirty();
            self.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
        }
        if is_compute {
            self.state.compute_stage_uav.uavs.set_all_dirty();
            self.invalidate_compute_pipeline();
            if self.cmdbuf_state == CommandBufferState::ComputePipelineReady {
                self.cmdbuf_state = CommandBufferState::ComputeEncoderActive;
            }
        } else {
            self.state.output_merger.uavs.set_all_dirty();
            self.invalidate_render_pipeline();
            if self.cmdbuf_state == CommandBufferState::RenderPipelineReady {
                self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
            }
        }
    }

    fn update_uav_counter(&mut self, uav_ptr: *mut c_void, initial_count: u32) {
        if let Some(uav) = crate::view::Uav::from_iface(uav_ptr)
            && let Some(counter) = uav.counter.clone()
        {
            M::emit_op(self, move |enc| {
                enc.update_counter(&counter, initial_count);
            });
        }
    }

    /// `ID3D11DeviceContext::VSSetConstantBuffers`.
    pub fn vs_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        self.vs_set_constant_buffers1(start_slot, num_buffers, buffers, ptr::null(), ptr::null());
    }

    /// `ID3D11DeviceContext::PSSetShaderResources`.
    pub fn ps_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        self.set_shader_resource(ShaderStage::Pixel, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::PSSetShader`.
    pub fn ps_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        self.set_shader(
            ShaderStage::Pixel,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::PSSetSamplers`.
    pub fn ps_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        self.set_samplers(ShaderStage::Pixel, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::VSSetShader`.
    pub fn vs_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        self.set_shader(
            ShaderStage::Vertex,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::PSSetConstantBuffers`.
    pub fn ps_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        self.ps_set_constant_buffers1(start_slot, num_buffers, buffers, ptr::null(), ptr::null());
    }

    /// `ID3D11DeviceContext::IASetInputLayout`.
    pub fn ia_set_input_layout(&mut self, input_layout: *mut c_void) {
        if !self.state.input_assembler.input_layout.set(input_layout) {
            return;
        }
        self.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
        self.invalidate_render_pipeline();
        if self.cmdbuf_state == CommandBufferState::RenderPipelineReady {
            self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
        }
    }

    /// `ID3D11DeviceContext::IASetVertexBuffers`.
    pub fn ia_set_vertex_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        strides: *const u32,
        offsets: *const u32,
    ) {
        let live = self.is_live();
        for i in 0..(num_buffers as usize) {
            let slot = (start_slot as usize) + i;
            if slot >= VERTEX_BUFFER_SLOTS {
                break;
            }
            let buf_ptr = read_ptr_at(buffers, i);
            let valid_buf = !buf_ptr.is_null()
                && (!live
                    || crate::resource::common(buf_ptr).is_some_and(|c| {
                        (c.bind_flags & D3D11_BIND_VERTEX_BUFFER) != 0
                            && self.validate_ia_hazard(buf_ptr, c.bind_flags)
                    }));
            if !valid_buf {
                if !self.state.input_assembler.vertex_buffers.unbind(slot) {
                    continue;
                }
                self.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
                if live
                    || self.cmdbuf_state == CommandBufferState::RenderEncoderActive
                    || self.cmdbuf_state == CommandBufferState::RenderPipelineReady
                {
                    M::emit_st(self, move |enc| {
                        enc.bind_vertex_buffer(slot, 0, 0, None);
                    });
                }
                continue;
            }
            let stride = read_u32_at(strides, i, 0);
            let offset = read_u32_at(offsets, i, 0);
            if let Some((replacement, entry)) = self.state.input_assembler.vertex_buffers.bind_with(
                slot,
                |e| e.buffer.as_ptr() == buf_ptr,
                || VertexBufferEntry {
                    buffer: ComPtr::from_raw_or_null(buf_ptr),
                    stride,
                    offset,
                },
            ) {
                if replacement {
                    if entry.stride == stride && entry.offset == offset {
                        continue;
                    }
                    entry.stride = stride;
                    entry.offset = offset;
                    self.state.input_assembler.vertex_buffers.set_dirty(slot);
                    self.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
                    if live
                        || self.cmdbuf_state == CommandBufferState::RenderEncoderActive
                        || self.cmdbuf_state == CommandBufferState::RenderPipelineReady
                    {
                        M::emit_st(self, move |enc| {
                            enc.bind_vertex_buffer_offset(slot, offset, stride);
                        });
                    }
                    continue;
                }
                self.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
                if live {
                    let buf = crate::resource::common(buf_ptr).and_then(|c| c.buffer.clone());
                    M::emit_st(self, move |enc| {
                        enc.bind_vertex_buffer(slot, offset, stride, buf.clone());
                    });
                }
            }
        }
    }

    /// `ID3D11DeviceContext::IASetIndexBuffer`.
    pub fn ia_set_index_buffer(&mut self, index_buffer: *mut c_void, format: u32, offset: u32) {
        let live = self.is_live();
        let valid_ptr = if !index_buffer.is_null()
            && (!live
                || crate::resource::common(index_buffer).is_some_and(|c| {
                    (c.bind_flags & D3D11_BIND_INDEX_BUFFER) != 0
                        && self.validate_ia_hazard(index_buffer, c.bind_flags)
                })) {
            index_buffer
        } else {
            ptr::null_mut()
        };
        let fmt_or_off_changed = self.state.input_assembler.index_buffer_format != format
            || self.state.input_assembler.index_buffer_offset != offset;
        self.state.input_assembler.index_buffer_format = format;
        self.state.input_assembler.index_buffer_offset = offset;
        let buf_changed = self.state.input_assembler.index_buffer.set(valid_ptr);
        if buf_changed || fmt_or_off_changed {
            self.dirty.insert(DirtyFlags::INDEX_BUFFER);
        }
        if live {
            if buf_changed {
                let buf = if valid_ptr.is_null() {
                    None
                } else {
                    crate::resource::common(valid_ptr).and_then(|c| c.buffer.clone())
                };
                self.set_encode_index_buffer(u64::from(buf.is_some()), format, offset);
                M::emit_st(self, move |enc| {
                    enc.bind_index_buffer(buf.clone());
                });
            } else if fmt_or_off_changed {
                self.set_encode_index_buffer(u64::from(!valid_ptr.is_null()), format, offset);
            }
        }
    }

    /// `ID3D11DeviceContext::GSSetConstantBuffers`.
    pub fn gs_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        self.gs_set_constant_buffers1(start_slot, num_buffers, buffers, ptr::null(), ptr::null());
    }

    /// `ID3D11DeviceContext::GSSetShader`.
    pub fn gs_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        self.set_shader(
            ShaderStage::Geometry,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::IASetPrimitiveTopology`.
    pub fn ia_set_primitive_topology(&mut self, topology: u32) {
        if self.state.input_assembler.topology == topology {
            return;
        }
        self.state.input_assembler.topology = topology;
        self.state.topology = topology;
        self.invalidate_render_pipeline();
        if matches!(self.cmdbuf_state, CommandBufferState::RenderPipelineReady) {
            self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
        }
    }

    /// `ID3D11DeviceContext::VSSetShaderResources`.
    pub fn vs_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        self.set_shader_resource(ShaderStage::Vertex, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::VSSetSamplers`.
    pub fn vs_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        self.set_samplers(ShaderStage::Vertex, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::SetPredication`.
    pub fn set_predication(&mut self, predicate: *mut c_void, predicate_value: i32) {
        let _ = self.state.predicate.set(predicate);
        self.state.predicate_value = predicate_value != 0;
        if !predicate.is_null() {
            M::emit_st(self, |enc| {
                enc.set_compatibility_flag(CompatibilityFlags::UNSUPPORTED_PREDICATION);
            });
        }
    }

    /// `ID3D11DeviceContext::GSSetShaderResources`.
    pub fn gs_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        self.set_shader_resource(ShaderStage::Geometry, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::GSSetSamplers`.
    pub fn gs_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        self.set_samplers(ShaderStage::Geometry, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::OMSetRenderTargets`.
    pub fn om_set_render_targets(
        &mut self,
        num_views: u32,
        render_target_views: *const *mut c_void,
        depth_stencil_view: *mut c_void,
    ) {
        self.om_set_render_targets_and_unordered_access_views(
            num_views,
            render_target_views,
            depth_stencil_view,
            0,
            KEEP_UNORDERED_ACCESS_VIEWS,
            ptr::null(),
            ptr::null(),
        );
    }

    /// `ID3D11DeviceContext::OMSetRenderTargetsAndUnorderedAccessViews`.
    #[expect(
        clippy::too_many_arguments,
        reason = "matches ID3D11DeviceContext::OMSetRenderTargetsAndUnorderedAccessViews"
    )]
    pub fn om_set_render_targets_and_unordered_access_views(
        &mut self,
        num_rtvs: u32,
        render_target_views: *const *mut c_void,
        depth_stencil_view: *mut c_void,
        uav_start_slot: u32,
        num_uavs: u32,
        unordered_access_views: *const *mut c_void,
        uav_initial_counts: *const u32,
    ) {
        let live = self.is_live();
        if live
            && !Self::validate_multi_output(
                num_rtvs,
                render_target_views,
                num_uavs,
                unordered_access_views,
            )
        {
            return;
        }
        let mut should_invalidate_pass = false;

        if num_rtvs != KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL {
            if live
                && !self.validate_set_render_targets(
                    num_rtvs,
                    render_target_views,
                    depth_stencil_view,
                )
            {
                return;
            }
            let clamped = (num_rtvs as usize).min(RENDER_TARGET_SLOTS);
            for rtv_index in 0..RENDER_TARGET_SLOTS {
                let rtv_ptr = if rtv_index < clamped {
                    read_ptr_at(render_target_views, rtv_index)
                } else {
                    ptr::null_mut()
                };
                if live && let Some(rtv) = Rtv::from_iface(rtv_ptr) {
                    self.resolve_om_srv_hazard(rtv.resource, rtv.subset);
                    self.resolve_om_ia_hazard(rtv.resource);
                }
                if self.state.output_merger.rtvs[rtv_index].set(rtv_ptr) {
                    should_invalidate_pass = true;
                }
            }
            self.state.output_merger.num_rtvs = u32::try_from(clamped).unwrap_or(0);
            if live && let Some(dsv) = Dsv::from_iface(depth_stencil_view) {
                self.resolve_om_srv_hazard(dsv.resource, dsv.subset);
            }
            if self.state.output_merger.dsv.set(depth_stencil_view) {
                should_invalidate_pass = true;
            }
        }

        if num_uavs != KEEP_UNORDERED_ACCESS_VIEWS {
            if live && !unordered_access_views.is_null() {
                let uav_count = (num_uavs as usize).min(UAV_SLOTS);
                for i in 0..uav_count {
                    let ptr_i = read_ptr_at(unordered_access_views, i);
                    if let Some(uav_i) = Uav::from_iface(ptr_i) {
                        for j in 0..i {
                            let ptr_j = read_ptr_at(unordered_access_views, j);
                            if let Some(uav_j) = Uav::from_iface(ptr_j)
                                && uav_i.resource == uav_j.resource
                                && uav_i.subset.overlaps(uav_j.subset)
                            {
                                return;
                            }
                        }
                    }
                }
            }
            let start = uav_start_slot as usize;
            let end = start.saturating_add(num_uavs as usize);
            for slot in 0..UAV_SLOTS {
                if slot >= start && slot < end {
                    let idx = slot - start;
                    let uav_ptr = read_ptr_at(unordered_access_views, idx);
                    let init_count = read_u32_at(uav_initial_counts, idx, u32::MAX);
                    if uav_ptr.is_null() {
                        if self.state.output_merger.uavs.unbind(slot) && live {
                            self.emit_unbind_uav::<{ stage::PIXEL }>(slot);
                        }
                    } else if live {
                        let Some(uav) = Uav::from_iface(uav_ptr) else {
                            if self.state.output_merger.uavs.unbind(slot) {
                                self.emit_unbind_uav::<{ stage::PIXEL }>(slot);
                            }
                            continue;
                        };
                        self.resolve_om_srv_hazard(uav.resource, uav.subset);
                        self.resolve_om_ia_hazard(uav.resource);
                        if self.resolve_om_rtv_hazard(uav.resource, uav.subset) {
                            should_invalidate_pass = true;
                        }
                        if let Some((replacement, _)) = self.state.output_merger.uavs.bind_with(
                            slot,
                            |e| e.uav.as_ptr() == uav_ptr,
                            || UnorderedAccessViewEntry {
                                uav: ComPtr::from_raw_or_null(uav_ptr),
                            },
                        ) && !replacement
                        {
                            self.emit_bind_uav::<{ stage::PIXEL }>(slot, uav);
                        }
                        if init_count != u32::MAX {
                            self.update_uav_counter(uav_ptr, init_count);
                        }
                    } else {
                        let _ = self.state.output_merger.uavs.bind_with(
                            slot,
                            |e| e.uav.as_ptr() == uav_ptr,
                            || UnorderedAccessViewEntry {
                                uav: ComPtr::from_raw_or_null(uav_ptr),
                            },
                        );
                        if init_count != u32::MAX {
                            self.update_uav_counter(uav_ptr, init_count);
                        }
                    }
                } else if self.state.output_merger.uavs.unbind(slot) && live {
                    self.emit_unbind_uav::<{ stage::PIXEL }>(slot);
                }
            }
        }

        if should_invalidate_pass {
            self.invalidate_current_pass(false);
        }
    }

    /// `ID3D11DeviceContext::OMSetBlendState`.
    pub fn om_set_blend_state(
        &mut self,
        blend_state: *mut c_void,
        blend_factor: *const f32,
        sample_mask: u32,
    ) {
        let state_changed = self.state.output_merger.blend_state.set(blend_state);
        let bf = read_blend_factor(blend_factor);
        self.state.output_merger.blend_factor = bf;
        self.state.blend_factor = bf;
        let mask_changed = self.state.output_merger.sample_mask != sample_mask;
        self.state.output_merger.sample_mask = sample_mask;
        self.state.sample_mask = sample_mask;
        if state_changed || mask_changed {
            self.invalidate_render_pipeline();
            if self.cmdbuf_state == CommandBufferState::RenderPipelineReady {
                self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
            }
        }
        self.dirty.insert(DirtyFlags::BLEND_FACTOR_AND_SAMPLE_MASK);
    }

    /// `ID3D11DeviceContext::OMSetDepthStencilState`.
    pub fn om_set_depth_stencil_state(
        &mut self,
        depth_stencil_state: *mut c_void,
        stencil_ref: u32,
    ) {
        let _ = self
            .state
            .output_merger
            .depth_stencil_state
            .set(depth_stencil_state);
        self.state.output_merger.stencil_ref = stencil_ref;
        self.state.stencil_ref = stencil_ref;
        self.dirty.insert(DirtyFlags::DEPTH_STENCIL_STATE);
    }

    /// `ID3D11DeviceContext::SOSetTargets`.
    pub fn so_set_targets(
        &mut self,
        num_buffers: u32,
        targets: *const *mut c_void,
        offsets: *const u32,
    ) {
        let live = self.is_live();
        let count = (num_buffers as usize).min(STREAM_OUTPUT_SLOTS);
        if !targets.is_null() {
            for i in 0..count {
                let buf_i = read_ptr_at(targets, i);
                if !buf_i.is_null() {
                    for j in 0..i {
                        let buf_j = read_ptr_at(targets, j);
                        if buf_i == buf_j {
                            return;
                        }
                    }
                }
            }
        }

        for slot in 0..STREAM_OUTPUT_SLOTS {
            let buf_ptr = if slot < count {
                read_ptr_at(targets, slot)
            } else {
                ptr::null_mut()
            };
            let offset = if slot < count {
                read_u32_at(offsets, slot, 0)
            } else {
                0
            };

            let valid_buf = !buf_ptr.is_null()
                && (!live
                    || crate::resource::common(buf_ptr)
                        .is_some_and(|c| (c.bind_flags & D3D11_BIND_STREAM_OUTPUT) != 0));
            if !valid_buf {
                if !self.state.stream_output.targets.unbind(slot) {
                    continue;
                }
                if live
                    || self.cmdbuf_state == CommandBufferState::RenderEncoderActive
                    || self.cmdbuf_state == CommandBufferState::RenderPipelineReady
                {
                    M::emit_st(self, move |enc| {
                        enc.bind_stream_output_buffer(slot, 0, None);
                    });
                }
                continue;
            }

            if let Some((replacement, entry)) = self.state.stream_output.targets.bind_with(
                slot,
                |e| e.buffer.as_ptr() == buf_ptr,
                || StreamOutputBufferEntry {
                    buffer: ComPtr::from_raw_or_null(buf_ptr),
                    offset,
                },
            ) {
                if replacement {
                    if entry.offset == offset {
                        continue;
                    }
                    entry.offset = offset;
                    self.state.stream_output.targets.set_dirty(slot);
                    if live
                        || self.cmdbuf_state == CommandBufferState::RenderEncoderActive
                        || self.cmdbuf_state == CommandBufferState::RenderPipelineReady
                    {
                        M::emit_st(self, move |enc| {
                            enc.bind_stream_output_buffer_offset(slot, offset);
                        });
                    }
                } else if live {
                    self.resolve_om_srv_hazard(buf_ptr, ResourceSubset::WHOLE);
                    self.resolve_om_ia_hazard(buf_ptr);
                    let buf = crate::resource::common(buf_ptr).and_then(|c| c.buffer.clone());
                    M::emit_st(self, move |enc| {
                        enc.bind_stream_output_buffer(slot, offset, buf.clone());
                    });
                }
            }
        }
    }

    /// `ID3D11DeviceContext::RSSetState`.
    pub fn rs_set_state(&mut self, rasterizer_state: *mut c_void) {
        let old_rs_ptr = self.state.rasterizer.rasterizer_state.as_ptr();
        let old_scissor = if self.is_live() {
            let p = if old_rs_ptr.is_null() {
                self.defaults.rasterizer
            } else {
                old_rs_ptr
            };
            crate::state::RasterizerState::from_iface(p)
                .is_some_and(crate::state::RasterizerState::is_scissor_enabled)
        } else {
            false
        };
        if !self.state.rasterizer.rasterizer_state.set(rasterizer_state) {
            return;
        }
        self.dirty.insert(DirtyFlags::RASTERIZER_STATE);
        if self.is_live() {
            let new_p = if rasterizer_state.is_null() {
                self.defaults.rasterizer
            } else {
                rasterizer_state
            };
            let new_scissor = crate::state::RasterizerState::from_iface(new_p)
                .is_some_and(crate::state::RasterizerState::is_scissor_enabled);
            if old_scissor != new_scissor {
                self.dirty.insert(DirtyFlags::SCISSOR_RECTS);
            }
        }
    }

    /// `ID3D11DeviceContext::RSSetViewports`.
    pub fn rs_set_viewports(&mut self, num_viewports: u32, viewports: *const c_void) {
        let count = num_viewports as usize;
        if count > VIEWPORT_AND_SCISSOR_SLOTS || (count > 0 && viewports.is_null()) {
            return;
        }
        let vp_ptr = viewports.cast::<D3d11Viewport>();
        let mut same = self.state.rasterizer.num_viewports == num_viewports;
        for (i, slot) in self
            .state
            .rasterizer
            .viewports
            .iter_mut()
            .enumerate()
            .take(count)
        {
            // SAFETY: `vp_ptr` is non-null and `i < count` within the caller's viewport array.
            let elem_ptr = unsafe { vp_ptr.add(i) };
            // SAFETY: `elem_ptr` points to a valid `D3d11Viewport`.
            let incoming = unsafe { *elem_ptr };
            if !viewport_eq(*slot, incoming) {
                same = false;
                *slot = incoming;
            }
        }
        if same {
            return;
        }
        self.state.rasterizer.num_viewports = num_viewports;
        self.dirty.insert(DirtyFlags::VIEWPORTS);
        if self.is_live() {
            let rs_p = if self.state.rasterizer.rasterizer_state.is_null() {
                self.defaults.rasterizer
            } else {
                self.state.rasterizer.rasterizer_state.as_ptr()
            };
            let scissor_enabled = crate::state::RasterizerState::from_iface(rs_p)
                .is_some_and(crate::state::RasterizerState::is_scissor_enabled);
            if !scissor_enabled {
                self.dirty.insert(DirtyFlags::SCISSOR_RECTS);
            }
        }
    }

    /// `ID3D11DeviceContext::RSSetScissorRects`.
    pub fn rs_set_scissor_rects(&mut self, num_rects: u32, rects: *const c_void) {
        let count = num_rects as usize;
        if count > VIEWPORT_AND_SCISSOR_SLOTS || (count > 0 && rects.is_null()) {
            return;
        }
        let rect_ptr = rects.cast::<Rect>();
        let mut same = self.state.rasterizer.num_scissor_rects == num_rects;
        for (i, slot) in self
            .state
            .rasterizer
            .scissor_rects
            .iter_mut()
            .enumerate()
            .take(count)
        {
            // SAFETY: `rect_ptr` is non-null and `i < count` within the caller's rect array.
            let elem_ptr = unsafe { rect_ptr.add(i) };
            // SAFETY: `elem_ptr` points to a valid `Rect`.
            let incoming = unsafe { *elem_ptr };
            if !rect_eq(*slot, incoming) {
                same = false;
                *slot = incoming;
            }
        }
        if same {
            return;
        }
        self.state.rasterizer.num_scissor_rects = num_rects;
        self.dirty.insert(DirtyFlags::SCISSOR_RECTS);
    }

    /// `ID3D11DeviceContext::SetResourceMinLOD`.
    pub fn set_resource_min_lod(&mut self, resource: *mut c_void, min_lod: f32) {
        let _ = self;
        set_resource_min_lod_inner(resource, min_lod);
    }

    /// `ID3D11DeviceContext::HSSetShaderResources`.
    pub fn hs_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        self.set_shader_resource(ShaderStage::Hull, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::HSSetShader`.
    pub fn hs_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        self.set_shader(
            ShaderStage::Hull,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::HSSetSamplers`.
    pub fn hs_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        self.set_samplers(ShaderStage::Hull, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::HSSetConstantBuffers`.
    pub fn hs_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        self.hs_set_constant_buffers1(start_slot, num_buffers, buffers, ptr::null(), ptr::null());
    }

    /// `ID3D11DeviceContext::DSSetShaderResources`.
    pub fn ds_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        self.set_shader_resource(ShaderStage::Domain, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::DSSetShader`.
    pub fn ds_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        self.set_shader(
            ShaderStage::Domain,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::DSSetSamplers`.
    pub fn ds_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        self.set_samplers(ShaderStage::Domain, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::DSSetConstantBuffers`.
    pub fn ds_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        self.ds_set_constant_buffers1(start_slot, num_buffers, buffers, ptr::null(), ptr::null());
    }

    /// `ID3D11DeviceContext::CSSetShaderResources`.
    pub fn cs_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        self.set_shader_resource(ShaderStage::Compute, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::CSSetUnorderedAccessViews`.
    pub fn cs_set_unordered_access_views(
        &mut self,
        start_slot: u32,
        num_uavs: u32,
        unordered_access_views: *const *mut c_void,
        uav_initial_counts: *const u32,
    ) {
        let live = self.is_live();
        if live && !unordered_access_views.is_null() {
            let uav_count = (num_uavs as usize).min(UAV_SLOTS);
            for i in 0..uav_count {
                let ptr_i = read_ptr_at(unordered_access_views, i);
                if let Some(uav_i) = Uav::from_iface(ptr_i) {
                    for j in 0..i {
                        let ptr_j = read_ptr_at(unordered_access_views, j);
                        if let Some(uav_j) = Uav::from_iface(ptr_j)
                            && uav_i.resource == uav_j.resource
                            && uav_i.subset.overlaps(uav_j.subset)
                        {
                            return;
                        }
                    }
                }
            }
        }
        for i in 0..(num_uavs as usize) {
            let slot = (start_slot as usize) + i;
            if slot >= UAV_SLOTS {
                break;
            }
            let uav_ptr = read_ptr_at(unordered_access_views, i);
            let init_count = read_u32_at(uav_initial_counts, i, u32::MAX);
            if uav_ptr.is_null() {
                if self.state.compute_stage_uav.uavs.unbind(slot) && live {
                    self.emit_unbind_uav::<{ stage::COMPUTE }>(slot);
                }
            } else if live {
                let Some(uav) = Uav::from_iface(uav_ptr) else {
                    if self.state.compute_stage_uav.uavs.unbind(slot) {
                        self.emit_unbind_uav::<{ stage::COMPUTE }>(slot);
                    }
                    continue;
                };
                self.resolve_cs_srv_hazard(uav.resource, uav.subset);
                if let Some((replacement, _)) = self.state.compute_stage_uav.uavs.bind_with(
                    slot,
                    |e| e.uav.as_ptr() == uav_ptr,
                    || UnorderedAccessViewEntry {
                        uav: ComPtr::from_raw_or_null(uav_ptr),
                    },
                ) && !replacement
                {
                    self.emit_bind_uav::<{ stage::COMPUTE }>(slot, uav);
                }
                if init_count != u32::MAX {
                    self.update_uav_counter(uav_ptr, init_count);
                }
            } else {
                let _ = self.state.compute_stage_uav.uavs.bind_with(
                    slot,
                    |e| e.uav.as_ptr() == uav_ptr,
                    || UnorderedAccessViewEntry {
                        uav: ComPtr::from_raw_or_null(uav_ptr),
                    },
                );
                if init_count != u32::MAX {
                    self.update_uav_counter(uav_ptr, init_count);
                }
            }
        }
    }

    /// `ID3D11DeviceContext::CSSetShader`.
    pub fn cs_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        self.set_shader(
            ShaderStage::Compute,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::CSSetSamplers`.
    pub fn cs_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        self.set_samplers(ShaderStage::Compute, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::CSSetConstantBuffers`.
    pub fn cs_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        self.cs_set_constant_buffers1(start_slot, num_buffers, buffers, ptr::null(), ptr::null());
    }

    /// `ID3D11DeviceContext1::VSSetConstantBuffers1`.
    pub fn vs_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        self.set_constant_buffer(
            ShaderStage::Vertex,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    /// `ID3D11DeviceContext1::HSSetConstantBuffers1`.
    pub fn hs_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        self.set_constant_buffer(
            ShaderStage::Hull,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    /// `ID3D11DeviceContext1::DSSetConstantBuffers1`.
    pub fn ds_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        self.set_constant_buffer(
            ShaderStage::Domain,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    /// `ID3D11DeviceContext1::GSSetConstantBuffers1`.
    pub fn gs_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        self.set_constant_buffer(
            ShaderStage::Geometry,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    /// `ID3D11DeviceContext1::PSSetConstantBuffers1`.
    pub fn ps_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        self.set_constant_buffer(
            ShaderStage::Pixel,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    /// `ID3D11DeviceContext1::CSSetConstantBuffers1`.
    pub fn cs_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        self.set_constant_buffer(
            ShaderStage::Compute,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }
}

#[cfg(test)]
mod tests;
