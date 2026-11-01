//! Context state getter methods (`*Get*`, `GetPredication`, `GetResourceMinLOD`).

use core::ffi::c_void;

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    #[must_use]
    pub const fn get_resource_min_lod(&self, resource: *mut c_void) -> f32 {
        let _ = resource;
        0.0
    }

    pub const fn vs_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn ps_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn ps_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn ps_get_samplers(
        &self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn vs_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn ps_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn ia_get_input_layout(&self, input_layout: *mut *mut c_void) {
        let _ = input_layout;
    }

    pub const fn ia_get_vertex_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        strides: *mut u32,
        offsets: *mut u32,
    ) {
        let _ = (start_slot, num_buffers, buffers, strides, offsets);
    }

    pub const fn ia_get_index_buffer(
        &self,
        index_buffer: *mut *mut c_void,
        format: *mut u32,
        offset: *mut u32,
    ) {
        let _ = (index_buffer, format, offset);
    }

    pub const fn gs_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn gs_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn ia_get_primitive_topology(&self, topology: *mut u32) {
        let _ = topology;
    }

    pub const fn vs_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn vs_get_samplers(
        &self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn get_predication(&self, predicate: *mut *mut c_void, predicate_value: *mut i32) {
        let _ = (predicate, predicate_value);
    }

    pub const fn gs_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn gs_get_samplers(
        &self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn om_get_render_targets(
        &self,
        num_views: u32,
        render_target_views: *mut *mut c_void,
        depth_stencil_view: *mut *mut c_void,
    ) {
        let _ = (num_views, render_target_views, depth_stencil_view);
    }

    pub const fn om_get_render_targets_and_unordered_access_views(
        &self,
        num_rtvs: u32,
        render_target_views: *mut *mut c_void,
        depth_stencil_view: *mut *mut c_void,
        uav_start_slot: u32,
        num_uavs: u32,
        unordered_access_views: *mut *mut c_void,
    ) {
        let _ = (
            num_rtvs,
            render_target_views,
            depth_stencil_view,
            uav_start_slot,
            num_uavs,
            unordered_access_views,
        );
    }

    pub const fn om_get_blend_state(
        &self,
        blend_state: *mut *mut c_void,
        blend_factor: *mut f32,
        sample_mask: *mut u32,
    ) {
        let _ = (blend_state, blend_factor, sample_mask);
    }

    pub const fn om_get_depth_stencil_state(
        &self,
        depth_stencil_state: *mut *mut c_void,
        stencil_ref: *mut u32,
    ) {
        let _ = (depth_stencil_state, stencil_ref);
    }

    pub const fn so_get_targets(&self, num_buffers: u32, targets: *mut *mut c_void) {
        let _ = (num_buffers, targets);
    }

    pub const fn rs_get_state(&self, rasterizer_state: *mut *mut c_void) {
        let _ = rasterizer_state;
    }

    pub const fn rs_get_viewports(&self, num_viewports: *mut u32, viewports: *mut c_void) {
        let _ = (num_viewports, viewports);
    }

    pub const fn rs_get_scissor_rects(&self, num_rects: *mut u32, rects: *mut c_void) {
        let _ = (num_rects, rects);
    }

    pub const fn hs_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn hs_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn hs_get_samplers(
        &self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn hs_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn ds_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn ds_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn ds_get_samplers(
        &self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn ds_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn cs_get_shader_resources(
        &self,
        start_slot: u32,
        num_views: u32,
        views: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_views, views);
    }

    pub const fn cs_get_unordered_access_views(
        &self,
        start_slot: u32,
        num_uavs: u32,
        unordered_access_views: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_uavs, unordered_access_views);
    }

    pub const fn cs_get_shader(
        &self,
        shader: *mut *mut c_void,
        class_instances: *mut *mut c_void,
        num_class_instances: *mut u32,
    ) {
        let _ = (shader, class_instances, num_class_instances);
    }

    pub const fn cs_get_samplers(
        &self,
        start_slot: u32,
        num_samplers: u32,
        samplers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_samplers, samplers);
    }

    pub const fn cs_get_constant_buffers(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
    ) {
        let _ = (start_slot, num_buffers, buffers);
    }

    pub const fn vs_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    pub const fn hs_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    pub const fn ds_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    pub const fn gs_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    pub const fn ps_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
    ) {
        let _ = (
            start_slot,
            num_buffers,
            buffers,
            first_constant,
            num_constants,
        );
    }

    pub const fn cs_get_constant_buffers1(
        &self,
        start_slot: u32,
        num_buffers: u32,
        buffers: *mut *mut c_void,
        first_constant: *mut u32,
        num_constants: *mut u32,
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
