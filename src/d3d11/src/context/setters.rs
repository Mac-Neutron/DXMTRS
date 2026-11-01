//! Context state setter methods (`*Set*`, `SetPredication`, `SetResourceMinLOD`).

use core::ffi::c_void;

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    pub const fn vs_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn ps_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn ps_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn ps_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn vs_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn ps_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn ia_set_input_layout(&mut self, input_layout: *mut c_void) {
        let _ = input_layout;
    }

    pub const fn ia_set_vertex_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        strides: *const u32,
        offsets: *const u32,
    ) {
        let _ = (start_slot, num_buffers, buffers, strides, offsets);
    }

    pub const fn ia_set_index_buffer(
        &mut self,
        index_buffer: *mut c_void,
        format: u32,
        offset: u32,
    ) {
        let _ = (index_buffer, format, offset);
    }

    pub const fn gs_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn gs_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn ia_set_primitive_topology(&mut self, topology: u32) {
        self.state.topology = topology;
    }

    pub const fn vs_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn vs_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn set_predication(&mut self, predicate: *mut c_void, predicate_value: i32) {
        let _ = (predicate, predicate_value);
    }

    pub const fn gs_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn gs_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn om_set_render_targets(
        &mut self,
        num_views: u32,
        render_target_views: *const *mut c_void,
        depth_stencil_view: *mut c_void,
    ) {
        let _ = (num_views, render_target_views, depth_stencil_view);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "matches ID3D11DeviceContext::OMSetRenderTargetsAndUnorderedAccessViews"
    )]
    pub const fn om_set_render_targets_and_unordered_access_views(
        &mut self,
        num_rtvs: u32,
        render_target_views: *const *mut c_void,
        depth_stencil_view: *mut c_void,
        uav_start_slot: u32,
        num_uavs: u32,
        unordered_access_views: *const *mut c_void,
        uav_initial_counts: *const u32,
    ) {
        let _ = (
            num_rtvs,
            render_target_views,
            depth_stencil_view,
            uav_start_slot,
            num_uavs,
            unordered_access_views,
            uav_initial_counts,
        );
    }

    pub const fn om_set_blend_state(
        &mut self,
        blend_state: *mut c_void,
        blend_factor: *const f32,
        sample_mask: u32,
    ) {
        let _ = (blend_state, blend_factor, sample_mask);
    }

    pub const fn om_set_depth_stencil_state(
        &mut self,
        depth_stencil_state: *mut c_void,
        stencil_ref: u32,
    ) {
        let _ = (depth_stencil_state, stencil_ref);
    }

    pub const fn so_set_targets(
        &mut self,
        num_buffers: u32,
        targets: *const *mut c_void,
        offsets: *const u32,
    ) {
        let _ = (num_buffers, targets, offsets);
    }

    pub const fn rs_set_state(&mut self, rasterizer_state: *mut c_void) {
        let _ = rasterizer_state;
    }

    pub const fn rs_set_viewports(&mut self, num_viewports: u32, viewports: *const c_void) {
        let _ = (num_viewports, viewports);
    }

    pub const fn rs_set_scissor_rects(&mut self, num_rects: u32, rects: *const c_void) {
        let _ = (num_rects, rects);
    }

    pub const fn set_resource_min_lod(&mut self, resource: *mut c_void, min_lod: f32) {
        let _ = (resource, min_lod);
    }

    pub const fn hs_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn hs_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn hs_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn hs_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn ds_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn ds_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn ds_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn ds_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn cs_set_shader_resources(
        &mut self,
        start_slot: u32,
        num_views: u32,
        views: *const *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn cs_set_unordered_access_views(
        &mut self,
        start_slot: u32,
        num_uavs: u32,
        unordered_access_views: *const *mut c_void,
        uav_initial_counts: *const u32,
    ) {
        let _ = (
            start_slot,
            num_uavs,
            unordered_access_views,
            uav_initial_counts,
        );
    }

    pub const fn cs_set_shader(
        &mut self,
        shader: *mut c_void,
        class_instances: *const *mut c_void,
        num_class_instances: u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn cs_set_samplers(
        &mut self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn cs_set_constant_buffers(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn vs_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    pub const fn hs_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    pub const fn ds_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    pub const fn gs_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    pub const fn ps_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    pub const fn cs_set_constant_buffers1(
        &mut self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *const *mut c_void,
        first_constant: *const u32,
        num_constants: *const u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }
}
