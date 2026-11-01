//! Context state setter methods (`*Set*`, `SetPredication`, `SetResourceMinLOD`).

use core::{ffi::c_void, ptr};

use dxmt_core::{encoder::stage, frame_stats::CompatibilityFlags};

use super::{
    CommandBufferState, Context, Mode,
    state::{
        CONSTANT_BUFFER_SLOTS, ComPtr, ConstantBufferEntry, DEFAULT_BLEND_FACTOR, DirtyFlags,
        KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL, KEEP_UNORDERED_ACCESS_VIEWS, RENDER_TARGET_SLOTS,
        REQ_CONSTANT_BUFFER_ELEMENTS, SAMPLER_SLOTS, SRV_SLOTS, STREAM_OUTPUT_SLOTS, SamplerEntry,
        ShaderResourceViewEntry, ShaderStage, StreamOutputBufferEntry, UAV_SLOTS,
        UnorderedAccessViewEntry, VERTEX_BUFFER_SLOTS, VIEWPORT_AND_SCISSOR_SLOTS,
        VertexBufferEntry,
    },
};
use crate::{
    abi::{
        ext::{IID_IMTL_MIN_LOD_CLAMPABLE, IMTLMinLODClampableVtbl},
        records::{D3d11Viewport, Rect},
        vtbl::IUnknownVtbl,
    },
    com::{S_OK, release},
};

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
        for i in 0..(num_buffers as usize) {
            let slot = (start_slot as usize) + i;
            if slot >= CONSTANT_BUFFER_SLOTS {
                break;
            }
            let buf_ptr = read_ptr_at(buffers, i);
            if buf_ptr.is_null() {
                if !self.state.shader_stages[stage_idx]
                    .constant_buffers
                    .unbind(slot)
                {
                    continue;
                }
                if self.cmdbuf_state == CommandBufferState::RenderEncoderActive && is_vertex {
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
                && replacement
            {
                if entry.offset == arg_offset && entry.length == arg_length {
                    continue;
                }
                entry.offset = arg_offset;
                entry.length = arg_length;
                if self.cmdbuf_state == CommandBufferState::RenderEncoderActive && is_vertex {
                    M::emit_st(self, move |enc| {
                        enc.bind_constant_buffer_offset::<{ stage::VERTEX }>(slot, arg_offset);
                    });
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
        let stage_state = &mut self.state.shader_stages[stage.index()];
        for i in 0..(num_views as usize) {
            let slot = (start_slot as usize) + i;
            if slot >= SRV_SLOTS {
                break;
            }
            let srv_ptr = read_ptr_at(views, i);
            if srv_ptr.is_null() {
                stage_state.srvs.unbind(slot);
            } else {
                let _ = stage_state.srvs.bind_with(
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
        let stage_state = &mut self.state.shader_stages[stage.index()];
        for i in 0..(num_samplers as usize) {
            let slot = (start_slot as usize) + i;
            if slot >= SAMPLER_SLOTS {
                break;
            }
            let samp_ptr = read_ptr_at(samplers, i);
            if samp_ptr.is_null() {
                stage_state.samplers.unbind(slot);
            } else {
                let _ = stage_state.samplers.bind_with(
                    slot,
                    |e| e.sampler.as_ptr() == samp_ptr,
                    || SamplerEntry {
                        sampler: ComPtr::from_raw_or_null(samp_ptr),
                    },
                );
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
        if is_compute {
            if self.cmdbuf_state == CommandBufferState::ComputePipelineReady {
                self.cmdbuf_state = CommandBufferState::ComputeEncoderActive;
            }
        } else if self.cmdbuf_state == CommandBufferState::RenderPipelineReady {
            self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
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
        for i in 0..(num_buffers as usize) {
            let slot = (start_slot as usize) + i;
            if slot >= VERTEX_BUFFER_SLOTS {
                break;
            }
            let buf_ptr = read_ptr_at(buffers, i);
            if buf_ptr.is_null() {
                if !self.state.input_assembler.vertex_buffers.unbind(slot) {
                    continue;
                }
                self.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
                if self.cmdbuf_state == CommandBufferState::RenderEncoderActive
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
                    if self.cmdbuf_state == CommandBufferState::RenderEncoderActive
                        || self.cmdbuf_state == CommandBufferState::RenderPipelineReady
                    {
                        M::emit_st(self, move |enc| {
                            enc.bind_vertex_buffer_offset(slot, offset, stride);
                        });
                    }
                    continue;
                }
                self.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
            }
        }
    }

    /// `ID3D11DeviceContext::IASetIndexBuffer`.
    pub fn ia_set_index_buffer(&mut self, index_buffer: *mut c_void, format: u32, offset: u32) {
        let fmt_or_off_changed = self.state.input_assembler.index_buffer_format != format
            || self.state.input_assembler.index_buffer_offset != offset;
        self.state.input_assembler.index_buffer_format = format;
        self.state.input_assembler.index_buffer_offset = offset;
        let buf_changed = self.state.input_assembler.index_buffer.set(index_buffer);
        if buf_changed || fmt_or_off_changed {
            self.dirty.insert(DirtyFlags::INDEX_BUFFER);
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
    pub const fn ia_set_primitive_topology(&mut self, topology: u32) {
        if self.state.input_assembler.topology == topology {
            return;
        }
        self.state.input_assembler.topology = topology;
        self.state.topology = topology;
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
        let mut should_invalidate_pass = false;

        if num_rtvs != KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL {
            let clamped = (num_rtvs as usize).min(RENDER_TARGET_SLOTS);
            for rtv_index in 0..RENDER_TARGET_SLOTS {
                let rtv_ptr = if rtv_index < clamped {
                    read_ptr_at(render_target_views, rtv_index)
                } else {
                    ptr::null_mut()
                };
                if self.state.output_merger.rtvs[rtv_index].set(rtv_ptr) {
                    should_invalidate_pass = true;
                }
            }
            self.state.output_merger.num_rtvs = u32::try_from(clamped).unwrap_or(0);
            if self.state.output_merger.dsv.set(depth_stencil_view) {
                should_invalidate_pass = true;
            }
        }

        if num_uavs != KEEP_UNORDERED_ACCESS_VIEWS {
            let start = uav_start_slot as usize;
            let end = start.saturating_add(num_uavs as usize);
            for slot in 0..UAV_SLOTS {
                if slot >= start && slot < end {
                    let idx = slot - start;
                    let uav_ptr = read_ptr_at(unordered_access_views, idx);
                    let init_count = read_u32_at(uav_initial_counts, idx, u32::MAX);
                    if uav_ptr.is_null() {
                        self.state.output_merger.uavs.unbind(slot);
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
                } else {
                    self.state.output_merger.uavs.unbind(slot);
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
        if (state_changed || mask_changed)
            && self.cmdbuf_state == CommandBufferState::RenderPipelineReady
        {
            self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
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

            if buf_ptr.is_null() {
                if !self.state.stream_output.targets.unbind(slot) {
                    continue;
                }
                if self.cmdbuf_state == CommandBufferState::RenderEncoderActive
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
            ) && replacement
            {
                if entry.offset == offset {
                    continue;
                }
                entry.offset = offset;
                self.state.stream_output.targets.set_dirty(slot);
                if self.cmdbuf_state == CommandBufferState::RenderEncoderActive
                    || self.cmdbuf_state == CommandBufferState::RenderPipelineReady
                {
                    M::emit_st(self, move |enc| {
                        enc.bind_stream_output_buffer_offset(slot, offset);
                    });
                }
            }
        }
    }

    /// `ID3D11DeviceContext::RSSetState`.
    pub fn rs_set_state(&mut self, rasterizer_state: *mut c_void) {
        if !self.state.rasterizer.rasterizer_state.set(rasterizer_state) {
            return;
        }
        self.dirty.insert(DirtyFlags::RASTERIZER_STATE);
    }

    /// `ID3D11DeviceContext::RSSetViewports`.
    pub fn rs_set_viewports(&mut self, num_viewports: u32, viewports: *const c_void) {
        let count = num_viewports as usize;
        if count > VIEWPORT_AND_SCISSOR_SLOTS || (count > 0 && viewports.is_null()) {
            return;
        }
        let vp_ptr = viewports.cast::<D3d11Viewport>();
        let mut same = self.state.rasterizer.num_viewports == num_viewports;
        for i in 0..count {
            // SAFETY: `vp_ptr` is non-null and `i < count` within the caller's viewport array.
            let elem_ptr = unsafe { vp_ptr.add(i) };
            // SAFETY: `elem_ptr` points to a valid `D3d11Viewport`.
            let incoming = unsafe { *elem_ptr };
            if !viewport_eq(self.state.rasterizer.viewports[i], incoming) {
                same = false;
                self.state.rasterizer.viewports[i] = incoming;
            }
        }
        if same {
            return;
        }
        self.state.rasterizer.num_viewports = num_viewports;
        self.dirty.insert(DirtyFlags::VIEWPORTS);
    }

    /// `ID3D11DeviceContext::RSSetScissorRects`.
    pub fn rs_set_scissor_rects(&mut self, num_rects: u32, rects: *const c_void) {
        let count = num_rects as usize;
        if count > VIEWPORT_AND_SCISSOR_SLOTS || (count > 0 && rects.is_null()) {
            return;
        }
        let rect_ptr = rects.cast::<Rect>();
        let mut same = self.state.rasterizer.num_scissor_rects == num_rects;
        for i in 0..count {
            // SAFETY: `rect_ptr` is non-null and `i < count` within the caller's rect array.
            let elem_ptr = unsafe { rect_ptr.add(i) };
            // SAFETY: `elem_ptr` points to a valid `Rect`.
            let incoming = unsafe { *elem_ptr };
            if !rect_eq(self.state.rasterizer.scissor_rects[i], incoming) {
                same = false;
                self.state.rasterizer.scissor_rects[i] = incoming;
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
        for i in 0..(num_uavs as usize) {
            let slot = (start_slot as usize) + i;
            if slot >= UAV_SLOTS {
                break;
            }
            let uav_ptr = read_ptr_at(unordered_access_views, i);
            let init_count = read_u32_at(uav_initial_counts, i, u32::MAX);
            if uav_ptr.is_null() {
                self.state.compute_stage_uav.uavs.unbind(slot);
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
