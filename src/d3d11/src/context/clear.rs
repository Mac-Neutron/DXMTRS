//! Context clear and discard operations (`d3d11_context_impl.cpp`).

use core::ffi::c_void;

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    pub const fn clear_render_target_view(
        &mut self,
        render_target_view: *mut c_void,
        color_rgba: *const f32,
    ) {
        let _ = (render_target_view, color_rgba);
    }

    pub const fn clear_unordered_access_view_uint(
        &mut self,
        unordered_access_view: *mut c_void,
        values: *const u32,
    ) {
        let _ = (unordered_access_view, values);
    }

    pub const fn clear_unordered_access_view_float(
        &mut self,
        unordered_access_view: *mut c_void,
        values: *const f32,
    ) {
        let _ = (unordered_access_view, values);
    }

    pub const fn clear_depth_stencil_view(
        &mut self,
        depth_stencil_view: *mut c_void,
        clear_flags: u32,
        depth: f32,
        stencil: u8,
    ) {
        let _ = (depth_stencil_view, clear_flags, depth, stencil);
    }

    pub const fn clear_view(
        &mut self,
        view: *mut c_void,
        color: *const f32,
        rect: *const c_void,
        num_rects: u32,
    ) {
        let _ = (view, color, rect, num_rects);
    }

    pub const fn discard_resource(&mut self, resource: *mut c_void) {
        let _ = resource;
    }

    pub const fn discard_view(&mut self, resource_view: *mut c_void) {
        let _ = resource_view;
    }

    pub const fn discard_view1(
        &mut self,
        resource_view: *mut c_void,
        rects: *const c_void,
        num_rects: u32,
    ) {
        let _ = (resource_view, rects, num_rects);
    }
}
