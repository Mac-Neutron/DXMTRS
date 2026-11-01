//! Context state getter methods (`*Get*`, `GetPredication`, `GetResourceMinLOD`).

use core::{ffi::c_void, ptr};

use super::{
    Context, Mode,
    state::{
        RENDER_TARGET_SLOTS, REQ_CONSTANT_BUFFER_ELEMENTS, ShaderStage, VIEWPORT_AND_SCISSOR_SLOTS,
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

const fn write_ptr_at(arr: *mut *mut c_void, index: usize, value: *mut c_void) {
    if arr.is_null() {
        return;
    }
    // SAFETY: `arr` is non-null and the caller bounds `index` within the output array length.
    let slot_ptr = unsafe { arr.add(index) };
    // SAFETY: `slot_ptr` points to element `index` of the caller-supplied output array.
    unsafe { *slot_ptr = value };
}

const fn write_u32_at(arr: *mut u32, index: usize, value: u32) {
    if arr.is_null() {
        return;
    }
    // SAFETY: `arr` is non-null and the caller bounds `index` within the output array length.
    let elem_ptr = unsafe { arr.add(index) };
    // SAFETY: `elem_ptr` points to element `index` of the caller-supplied `u32` output array.
    unsafe { *elem_ptr = value };
}

const fn write_opt<T: Copy>(out: *mut T, value: T) {
    if !out.is_null() {
        // SAFETY: `out` was checked non-null and points to caller-supplied output storage.
        unsafe { *out = value };
    }
}

const fn read_u32(ptr: *const u32) -> u32 {
    // SAFETY: caller verified `ptr` is non-null.
    unsafe { *ptr }
}

const fn write_blend_factor(blend_factor: *mut f32, values: [f32; 4]) {
    if blend_factor.is_null() {
        return;
    }
    let mut i = 0_usize;
    while i < 4 {
        // SAFETY: `blend_factor` is non-null and points to `FLOAT[4]`.
        let dst = unsafe { blend_factor.add(i) };
        // SAFETY: `dst` points to element `i` of `blend_factor`.
        unsafe { *dst = values[i] };
        i += 1;
    }
}

fn get_resource_min_lod_inner(resource: *mut c_void) -> f32 {
    if resource.is_null() {
        return 0.0;
    }
    // SAFETY: `resource` is a non-null COM object whose first word is its vtable pointer.
    let vtbl = unsafe { *resource.cast::<*const IUnknownVtbl>() };
    if !vtbl.is_null() {
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
            let get_fn = unsafe { (*clamp_vtbl).get_min_lod };
            // SAFETY: invokes `IMTLMinLODClampable::GetMinLOD`.
            let lod = unsafe { get_fn(clamp_iface) };
            // SAFETY: balances the reference returned by `QueryInterface` on `resource`.
            unsafe { release(resource) };
            return lod;
        }
    }
    if let Some(common) = crate::resource::common(resource) {
        return common.min_lod;
    }
    0.0
}

impl<M: Mode> Context<M> {
    fn get_constant_buffer(
        &self,
        stage: ShaderStage,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        let stage_state = &self.state.shader_stages[stage.index()];
        for i in 0..(num_buffers as usize) {
            let slot = (start_slot as usize) + i;
            if let Some(entry) = stage_state.constant_buffers.get(slot) {
                let ptr = if buffers.is_null() {
                    ptr::null_mut()
                } else {
                    entry.buffer.get_add_ref()
                };
                write_ptr_at(buffers, i, ptr);
                write_u32_at(first_constant, i, entry.offset >> 4);
                write_u32_at(num_constants, i, entry.length >> 4);
            } else {
                write_ptr_at(buffers, i, ptr::null_mut());
                write_u32_at(first_constant, i, 0);
                write_u32_at(num_constants, i, REQ_CONSTANT_BUFFER_ELEMENTS);
            }
        }
    }

    fn get_shader_resource(
        &self,
        stage: ShaderStage,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        if views.is_null() {
            return;
        }
        let stage_state = &self.state.shader_stages[stage.index()];
        for i in 0..(num_views as usize) {
            let slot = (start_slot as usize) + i;
            let ptr = stage_state
                .srvs
                .get(slot)
                .map_or(ptr::null_mut(), |e| e.srv.get_add_ref());
            write_ptr_at(views, i, ptr);
        }
    }

    fn get_samplers(
        &self,
        stage: ShaderStage,
        start_slot: u32,
        num_samplers: u32,
        samplers: *mut *mut c_void,
    ) {
        if samplers.is_null() {
            return;
        }
        let stage_state = &self.state.shader_stages[stage.index()];
        for i in 0..(num_samplers as usize) {
            let slot = (start_slot as usize) + i;
            let ptr = stage_state
                .samplers
                .get(slot)
                .map_or(ptr::null_mut(), |e| e.sampler.get_add_ref());
            write_ptr_at(samplers, i, ptr);
        }
    }

    fn get_shader(
        &self,
        stage: ShaderStage,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        let _ = class_instances;
        if !shader.is_null() {
            let ptr = self.state.shader_stages[stage.index()].shader.get_add_ref();
            write_opt(shader, ptr);
        }
        write_opt(num_class_instances, 0);
    }

    /// `ID3D11DeviceContext::GetResourceMinLOD`.
    #[must_use]
    pub fn get_resource_min_lod(&self, resource: *mut c_void) -> f32 {
        let _ = self;
        get_resource_min_lod_inner(resource)
    }

    /// `ID3D11DeviceContext::VSGetConstantBuffers`.
    pub fn vs_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        self.vs_get_constant_buffers1(
            start_slot,
            num_buffers,
            buffers,
            ptr::null_mut(),
            ptr::null_mut(),
        );
    }

    /// `ID3D11DeviceContext::PSGetShaderResources`.
    pub fn ps_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        self.get_shader_resource(ShaderStage::Pixel, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::PSGetShader`.
    pub fn ps_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        self.get_shader(
            ShaderStage::Pixel,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::PSGetSamplers`.
    pub fn ps_get_samplers(&self, start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void) {
        self.get_samplers(ShaderStage::Pixel, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::VSGetShader`.
    pub fn vs_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        self.get_shader(
            ShaderStage::Vertex,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::PSGetConstantBuffers`.
    pub fn ps_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        self.ps_get_constant_buffers1(
            start_slot,
            num_buffers,
            buffers,
            ptr::null_mut(),
            ptr::null_mut(),
        );
    }

    /// `ID3D11DeviceContext::IAGetInputLayout`.
    pub fn ia_get_input_layout(&self, input_layout: *mut *mut c_void) {
        if !input_layout.is_null() {
            let ptr = self.state.input_assembler.input_layout.get_add_ref();
            write_opt(input_layout, ptr);
        }
    }

    /// `ID3D11DeviceContext::IAGetVertexBuffers`.
    pub fn ia_get_vertex_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        strides: *mut u32,
        offsets: *mut u32,
    ) {
        for i in 0..(num_buffers as usize) {
            let slot = (start_slot as usize) + i;
            if let Some(entry) = self.state.input_assembler.vertex_buffers.get(slot) {
                let ptr = if buffers.is_null() {
                    ptr::null_mut()
                } else {
                    entry.buffer.get_add_ref()
                };
                write_ptr_at(buffers, i, ptr);
                write_u32_at(strides, i, entry.stride);
                write_u32_at(offsets, i, entry.offset);
            } else {
                write_ptr_at(buffers, i, ptr::null_mut());
                write_u32_at(strides, i, 0);
                write_u32_at(offsets, i, 0);
            }
        }
    }

    /// `ID3D11DeviceContext::IAGetIndexBuffer`.
    pub fn ia_get_index_buffer(
        &self,
        index_buffer: *mut *mut c_void,
        format: *mut u32,
        offset: *mut u32,
    ) {
        if !index_buffer.is_null() {
            let ptr = self.state.input_assembler.index_buffer.get_add_ref();
            write_opt(index_buffer, ptr);
        }
        write_opt(format, self.state.input_assembler.index_buffer_format);
        write_opt(offset, self.state.input_assembler.index_buffer_offset);
    }

    /// `ID3D11DeviceContext::GSGetConstantBuffers`.
    pub fn gs_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        self.gs_get_constant_buffers1(
            start_slot,
            num_buffers,
            buffers,
            ptr::null_mut(),
            ptr::null_mut(),
        );
    }

    /// `ID3D11DeviceContext::GSGetShader`.
    pub fn gs_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        self.get_shader(
            ShaderStage::Geometry,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::IAGetPrimitiveTopology`.
    pub const fn ia_get_primitive_topology(&self, topology: *mut u32) {
        write_opt(topology, self.state.input_assembler.topology);
    }

    /// `ID3D11DeviceContext::VSGetShaderResources`.
    pub fn vs_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        self.get_shader_resource(ShaderStage::Vertex, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::VSGetSamplers`.
    pub fn vs_get_samplers(&self, start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void) {
        self.get_samplers(ShaderStage::Vertex, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::GetPredication`.
    pub fn get_predication(&self, predicate: *mut *mut c_void, predicate_value: *mut i32) {
        if !predicate.is_null() {
            let ptr = self.state.predicate.get_add_ref();
            write_opt(predicate, ptr);
        }
        write_opt(predicate_value, i32::from(self.state.predicate_value));
    }

    /// `ID3D11DeviceContext::GSGetShaderResources`.
    pub fn gs_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        self.get_shader_resource(ShaderStage::Geometry, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::GSGetSamplers`.
    pub fn gs_get_samplers(&self, start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void) {
        self.get_samplers(ShaderStage::Geometry, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::OMGetRenderTargets`.
    pub fn om_get_render_targets(
        &self,
        num_views: u32,
        render_target_views: *mut *mut c_void,
        depth_stencil_view: *mut *mut c_void,
    ) {
        self.om_get_render_targets_and_unordered_access_views(
            num_views,
            render_target_views,
            depth_stencil_view,
            0,
            0,
            ptr::null_mut(),
        );
    }

    /// `ID3D11DeviceContext::OMGetRenderTargetsAndUnorderedAccessViews`.
    pub fn om_get_render_targets_and_unordered_access_views(
        &self,
        num_rtvs: u32,
        render_target_views: *mut *mut c_void,
        depth_stencil_view: *mut *mut c_void,
        uav_start_slot: u32,
        num_uavs: u32,
        unordered_access_views: *mut *mut c_void,
    ) {
        if !render_target_views.is_null() {
            let bound_count = self.state.output_merger.num_rtvs as usize;
            for i in 0..(num_rtvs as usize) {
                let ptr = if i < bound_count && i < RENDER_TARGET_SLOTS {
                    self.state.output_merger.rtvs[i].get_add_ref()
                } else {
                    ptr::null_mut()
                };
                write_ptr_at(render_target_views, i, ptr);
            }
        }
        if !depth_stencil_view.is_null() {
            let ptr = self.state.output_merger.dsv.get_add_ref();
            write_opt(depth_stencil_view, ptr);
        }
        if !unordered_access_views.is_null() {
            for i in 0..(num_uavs as usize) {
                let slot = (uav_start_slot as usize) + i;
                let ptr = self
                    .state
                    .output_merger
                    .uavs
                    .get(slot)
                    .map_or(ptr::null_mut(), |e| e.uav.get_add_ref());
                write_ptr_at(unordered_access_views, i, ptr);
            }
        }
    }

    /// `ID3D11DeviceContext::OMGetBlendState`.
    pub fn om_get_blend_state(
        &self,
        blend_state: *mut *mut c_void,
        blend_factor: *mut f32,
        sample_mask: *mut u32,
    ) {
        if !blend_state.is_null() {
            let ptr = self.state.output_merger.blend_state.get_add_ref();
            write_opt(blend_state, ptr);
        }
        write_blend_factor(blend_factor, self.state.output_merger.blend_factor);
        write_opt(sample_mask, self.state.output_merger.sample_mask);
    }

    /// `ID3D11DeviceContext::OMGetDepthStencilState`.
    pub fn om_get_depth_stencil_state(
        &self,
        depth_stencil_state: *mut *mut c_void,
        stencil_ref: *mut u32,
    ) {
        if !depth_stencil_state.is_null() {
            let ptr = self.state.output_merger.depth_stencil_state.get_add_ref();
            write_opt(depth_stencil_state, ptr);
        }
        write_opt(stencil_ref, self.state.output_merger.stencil_ref);
    }

    /// `ID3D11DeviceContext::SOGetTargets`.
    pub fn so_get_targets(&self, num_buffers: u32, targets: *mut *mut c_void) {
        if targets.is_null() {
            return;
        }
        for i in 0..(num_buffers as usize) {
            let ptr = self
                .state
                .stream_output
                .targets
                .get(i)
                .map_or(ptr::null_mut(), |e| e.buffer.get_add_ref());
            write_ptr_at(targets, i, ptr);
        }
    }

    /// `ID3D11DeviceContext::RSGetState`.
    pub fn rs_get_state(&self, rasterizer_state: *mut *mut c_void) {
        if !rasterizer_state.is_null() {
            let ptr = self.state.rasterizer.rasterizer_state.get_add_ref();
            write_opt(rasterizer_state, ptr);
        }
    }

    /// `ID3D11DeviceContext::RSGetViewports`.
    pub const fn rs_get_viewports(&self, num_viewports: *mut u32, viewports: *mut c_void) {
        if num_viewports.is_null() {
            return;
        }
        let available = self.state.rasterizer.num_viewports;
        if viewports.is_null() {
            write_opt(num_viewports, available);
            return;
        }
        let requested = read_u32(num_viewports);
        let out_vps = viewports.cast::<D3d11Viewport>();
        let mut i = 0_usize;
        while i < (requested as usize) {
            let vp = if i < (available as usize) && i < VIEWPORT_AND_SCISSOR_SLOTS {
                self.state.rasterizer.viewports[i]
            } else {
                D3d11Viewport {
                    top_left_x: 0.0,
                    top_left_y: 0.0,
                    width: 0.0,
                    height: 0.0,
                    min_depth: 0.0,
                    max_depth: 0.0,
                }
            };
            // SAFETY: `out_vps` is non-null and `i < requested`.
            let dst = unsafe { out_vps.add(i) };
            // SAFETY: `dst` points to element `i` of the caller's viewport buffer.
            unsafe { *dst = vp };
            i += 1;
        }
        let written = if available < requested {
            available
        } else {
            requested
        };
        write_opt(num_viewports, written);
    }

    /// `ID3D11DeviceContext::RSGetScissorRects`.
    pub const fn rs_get_scissor_rects(&self, num_rects: *mut u32, rects: *mut c_void) {
        if num_rects.is_null() {
            return;
        }
        let available = self.state.rasterizer.num_scissor_rects;
        if rects.is_null() {
            write_opt(num_rects, available);
            return;
        }
        let requested = read_u32(num_rects);
        let out_rects = rects.cast::<Rect>();
        let mut i = 0_usize;
        while i < (requested as usize) {
            let rect = if i < (available as usize) && i < VIEWPORT_AND_SCISSOR_SLOTS {
                self.state.rasterizer.scissor_rects[i]
            } else {
                Rect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                }
            };
            // SAFETY: `out_rects` is non-null and `i < requested`.
            let dst = unsafe { out_rects.add(i) };
            // SAFETY: `dst` points to element `i` of the caller's rect buffer.
            unsafe { *dst = rect };
            i += 1;
        }
        let written = if available < requested {
            available
        } else {
            requested
        };
        write_opt(num_rects, written);
    }

    /// `ID3D11DeviceContext::HSGetShaderResources`.
    pub fn hs_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        self.get_shader_resource(ShaderStage::Hull, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::HSGetShader`.
    pub fn hs_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        self.get_shader(
            ShaderStage::Hull,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::HSGetSamplers`.
    pub fn hs_get_samplers(&self, start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void) {
        self.get_samplers(ShaderStage::Hull, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::HSGetConstantBuffers`.
    pub fn hs_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        self.hs_get_constant_buffers1(
            start_slot,
            num_buffers,
            buffers,
            ptr::null_mut(),
            ptr::null_mut(),
        );
    }

    /// `ID3D11DeviceContext::DSGetShaderResources`.
    pub fn ds_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        self.get_shader_resource(ShaderStage::Domain, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::DSGetShader`.
    pub fn ds_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        self.get_shader(
            ShaderStage::Domain,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::DSGetSamplers`.
    pub fn ds_get_samplers(&self, start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void) {
        self.get_samplers(ShaderStage::Domain, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::DSGetConstantBuffers`.
    pub fn ds_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        self.ds_get_constant_buffers1(
            start_slot,
            num_buffers,
            buffers,
            ptr::null_mut(),
            ptr::null_mut(),
        );
    }

    /// `ID3D11DeviceContext::CSGetShaderResources`.
    pub fn cs_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        self.get_shader_resource(ShaderStage::Compute, start_slot, num_views, views);
    }

    /// `ID3D11DeviceContext::CSGetUnorderedAccessViews`.
    pub fn cs_get_unordered_access_views(
        &self,
        start_slot: u32,
        num_uavs: u32,
        unordered_access_views: *mut *mut c_void,
    ) {
        if unordered_access_views.is_null() {
            return;
        }
        for i in 0..(num_uavs as usize) {
            let slot = (start_slot as usize) + i;
            let ptr = self
                .state
                .compute_stage_uav
                .uavs
                .get(slot)
                .map_or(ptr::null_mut(), |e| e.uav.get_add_ref());
            write_ptr_at(unordered_access_views, i, ptr);
        }
    }

    /// `ID3D11DeviceContext::CSGetShader`.
    pub fn cs_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        self.get_shader(
            ShaderStage::Compute,
            shader,
            class_instances,
            num_class_instances,
        );
    }

    /// `ID3D11DeviceContext::CSGetSamplers`.
    pub fn cs_get_samplers(&self, start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void) {
        self.get_samplers(ShaderStage::Compute, start_slot, num_samplers, samplers);
    }

    /// `ID3D11DeviceContext::CSGetConstantBuffers`.
    pub fn cs_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        self.cs_get_constant_buffers1(
            start_slot,
            num_buffers,
            buffers,
            ptr::null_mut(),
            ptr::null_mut(),
        );
    }

    /// `ID3D11DeviceContext1::VSGetConstantBuffers1`.
    pub fn vs_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        self.get_constant_buffer(
            ShaderStage::Vertex,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    /// `ID3D11DeviceContext1::HSGetConstantBuffers1`.
    pub fn hs_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        self.get_constant_buffer(
            ShaderStage::Hull,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    /// `ID3D11DeviceContext1::DSGetConstantBuffers1`.
    pub fn ds_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        self.get_constant_buffer(
            ShaderStage::Domain,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    /// `ID3D11DeviceContext1::GSGetConstantBuffers1`.
    pub fn gs_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        self.get_constant_buffer(
            ShaderStage::Geometry,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    /// `ID3D11DeviceContext1::PSGetConstantBuffers1`.
    pub fn ps_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        self.get_constant_buffer(
            ShaderStage::Pixel,
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    /// `ID3D11DeviceContext1::CSGetConstantBuffers1`.
    pub fn cs_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        self.get_constant_buffer(
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
