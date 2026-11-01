//! Context draw and indirect draw commands (`d3d11_context_impl.cpp`).

use core::ffi::c_void;

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    pub const fn draw(&mut self, vertex_count: u32, start_vertex: u32) {
        let _ = (vertex_count, start_vertex);
    }

    pub const fn draw_indexed(&mut self, index_count: u32, start_index: u32, base_vertex: i32) {
        let _ = (index_count, start_index, base_vertex);
    }

    pub const fn draw_instanced(
        &mut self,
        vertex_count_per_instance: u32,
        instance_count: u32,
        start_vertex: u32,
        start_instance: u32,
    ) {
        let _ = (
            vertex_count_per_instance,
            instance_count,
            start_vertex,
            start_instance,
        );
    }

    pub const fn draw_indexed_instanced(
        &mut self,
        index_count_per_instance: u32,
        instance_count: u32,
        start_index: u32,
        base_vertex: i32,
        start_instance: u32,
    ) {
        let _ = (
            index_count_per_instance,
            instance_count,
            start_index,
            base_vertex,
            start_instance,
        );
    }

    pub const fn draw_auto(&mut self) {}

    pub const fn draw_instanced_indirect(
        &mut self,
        buffer_for_args: *mut c_void,
        aligned_byte_offset_for_args: u32,
    ) {
        let _ = (buffer_for_args, aligned_byte_offset_for_args);
    }

    pub const fn draw_indexed_instanced_indirect(
        &mut self,
        buffer_for_args: *mut c_void,
        aligned_byte_offset_for_args: u32,
    ) {
        let _ = (buffer_for_args, aligned_byte_offset_for_args);
    }
}
