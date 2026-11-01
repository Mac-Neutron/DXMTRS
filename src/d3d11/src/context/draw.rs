//! Context draw and indirect draw commands (`d3d11_context_impl.cpp`).
//!
//! Implements `Draw`, `DrawIndexed`, `DrawInstanced`, `DrawIndexedInstanced`, `DrawAuto`,
//! `DrawInstancedIndirect`, and `DrawIndexedInstancedIndirect`, routing geometry-shader and
//! tessellation draws to [`super::gs_ts`] and encoding ordinary draws into the active render pass.

use core::{ffi::c_void, ptr};

use dxmt_core::encoder::{NO_NEXT, kind};
use winemetal_sys::wire::{
    WMT_INDEX_TYPE_U_INT16, WMT_INDEX_TYPE_U_INT32, WMT_PRIMITIVE_TYPE_LINE,
    WMT_PRIMITIVE_TYPE_LINE_STRIP, WMT_PRIMITIVE_TYPE_POINT, WMT_PRIMITIVE_TYPE_TRIANGLE,
    WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP, WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT, WMT_RENDER_COMMAND_DRAW_INDIRECT, WmtcmdRenderDraw,
    WmtcmdRenderDrawIndexed, WmtcmdRenderDrawIndexedIndirect, WmtcmdRenderDrawIndirect,
};

use super::{Context, Mode};

/// `DXGI_FORMAT_R32_UINT` (`42`).
const DXGI_FORMAT_R32_UINT: u32 = 42;

/// Converts a D3D11 primitive topology to a Metal `WMTPrimitiveType`.
const fn to_metal_primitive_type(topology: u32) -> Option<u8> {
    match topology {
        1 => Some(WMT_PRIMITIVE_TYPE_POINT),
        2 => Some(WMT_PRIMITIVE_TYPE_LINE),
        3 => Some(WMT_PRIMITIVE_TYPE_LINE_STRIP),
        4 => Some(WMT_PRIMITIVE_TYPE_TRIANGLE),
        5 => Some(WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP),
        _ => None,
    }
}

/// Converts a DXGI index buffer format (`R16_UINT` or `R32_UINT`) to `(WMTIndexType, stride)`.
const fn index_type_and_stride(format: u32) -> (u8, u64) {
    if format == DXGI_FORMAT_R32_UINT {
        (WMT_INDEX_TYPE_U_INT32, 4)
    } else {
        (WMT_INDEX_TYPE_U_INT16, 2)
    }
}

impl<M: Mode> Context<M> {
    /// `ID3D11DeviceContext::Draw` (`d3d11_context_impl.cpp:2287-2315`).
    pub fn draw(&mut self, vertex_count: u32, start_vertex: u32) {
        if vertex_count == 0 {
            return;
        }
        let pipeline_kind = self.encode_snapshot().pipeline_kind;
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_full(vertex_count, 1, ptr::null_mut(), 0);
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_full(vertex_count, 1, ptr::null_mut(), 0);
            return;
        }
        if !self.pre_draw() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        // C++ closure #19 (`Draw`, d3d11_context_impl.cpp:2310):
        // emits `WmtcmdRenderDraw` (`render 19`).
        let cmd = WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type,
            vertex_start: u64::from(start_vertex),
            vertex_count: u64::from(vertex_count),
            instance_count: 1,
            base_instance: 0,
        };
        self.emit_render_cmd(cmd, move |enc| {
            enc.bump_visibility_result_offset();
            enc.encode_render_command(cmd);
        });
    }

    /// `ID3D11DeviceContext::DrawIndexed` (`d3d11_context_impl.cpp:2317-2351`).
    pub fn draw_indexed(&mut self, index_count: u32, start_index: u32, base_vertex: i32) {
        if index_count == 0 {
            return;
        }
        let pipeline_kind = self.encode_snapshot().pipeline_kind;
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_indexed_full(index_count, 1, ptr::null_mut(), 0);
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_indexed_full(index_count, 1, ptr::null_mut(), 0);
            return;
        }
        if !self.pre_draw_indexed() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        let snap = self.encode_snapshot();
        let (index_type, stride) = index_type_and_stride(snap.index_format);
        let index_sub_offset = u64::from(snap.index_sub_offset);
        let index_buffer_offset = u64::from(start_index) * stride + index_sub_offset;

        // C++ closure #20 (`DrawIndexed`, d3d11_context_impl.cpp:2340):
        // emits `WmtcmdRenderDrawIndexed` (`render 20`).
        let cmd = WmtcmdRenderDrawIndexed {
            r#type: WMT_RENDER_COMMAND_DRAW_INDEXED,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type,
            index_type,
            index_count: u64::from(index_count),
            index_buffer: snap.index_buffer,
            index_buffer_offset,
            instance_count: 1,
            base_vertex,
            base_instance: 0,
        };
        self.emit_render_cmd(cmd, move |enc| {
            enc.bump_visibility_result_offset();
            enc.encode_render_command(cmd);
        });
    }

    /// `ID3D11DeviceContext::DrawInstanced` (`d3d11_context_impl.cpp:2353-2387`).
    pub fn draw_instanced(
        &mut self,
        vertex_count_per_instance: u32,
        instance_count: u32,
        start_vertex: u32,
        start_instance: u32,
    ) {
        if vertex_count_per_instance == 0 || instance_count == 0 {
            return;
        }
        let pipeline_kind = self.encode_snapshot().pipeline_kind;
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_full(
                vertex_count_per_instance,
                instance_count,
                ptr::null_mut(),
                0,
            );
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_full(
                vertex_count_per_instance,
                instance_count,
                ptr::null_mut(),
                0,
            );
            return;
        }
        if !self.pre_draw() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        // C++ closure #21 (`DrawInstanced`, d3d11_context_impl.cpp:2377):
        // emits `WmtcmdRenderDraw` (`render 19`).
        let cmd = WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type,
            vertex_start: u64::from(start_vertex),
            vertex_count: u64::from(vertex_count_per_instance),
            instance_count,
            base_instance: start_instance,
        };
        self.emit_render_cmd(cmd, move |enc| {
            enc.bump_visibility_result_offset();
            enc.encode_render_command(cmd);
        });
    }

    /// `ID3D11DeviceContext::DrawIndexedInstanced` (`d3d11_context_impl.cpp:2389-2427`).
    pub fn draw_indexed_instanced(
        &mut self,
        index_count_per_instance: u32,
        instance_count: u32,
        start_index: u32,
        base_vertex: i32,
        start_instance: u32,
    ) {
        if index_count_per_instance == 0 || instance_count == 0 {
            return;
        }
        let pipeline_kind = self.encode_snapshot().pipeline_kind;
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_indexed_full(index_count_per_instance, instance_count, ptr::null_mut(), 0);
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_indexed_full(index_count_per_instance, instance_count, ptr::null_mut(), 0);
            return;
        }
        if !self.pre_draw_indexed() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        let snap = self.encode_snapshot();
        let (index_type, stride) = index_type_and_stride(snap.index_format);
        let index_sub_offset = u64::from(snap.index_sub_offset);
        let index_buffer_offset = u64::from(start_index) * stride + index_sub_offset;

        // C++ closure #22 (`DrawIndexedInstanced`, d3d11_context_impl.cpp:2415):
        // emits `WmtcmdRenderDrawIndexed` (`render 20`).
        let cmd = WmtcmdRenderDrawIndexed {
            r#type: WMT_RENDER_COMMAND_DRAW_INDEXED,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type,
            index_type,
            index_count: u64::from(index_count_per_instance),
            index_buffer: snap.index_buffer,
            index_buffer_offset,
            instance_count,
            base_vertex,
            base_instance: start_instance,
        };
        self.emit_render_cmd(cmd, move |enc| {
            enc.bump_visibility_result_offset();
            enc.encode_render_command(cmd);
        });
    }

    /// `ID3D11DeviceContext::DrawAuto` (`d3d11_context_impl.cpp:2515-2517`).
    pub fn draw_auto(&mut self) {
        crate::LOG.err("DrawAuto: not implemented.");
    }

    /// `ID3D11DeviceContext::DrawInstancedIndirect` (`d3d11_context_impl.cpp:2474-2513`).
    pub fn draw_instanced_indirect(
        &mut self,
        buffer_for_args: *mut c_void,
        aligned_byte_offset_for_args: u32,
    ) {
        if buffer_for_args.is_null() {
            return;
        }
        let pipeline_kind = self.encode_snapshot().pipeline_kind;
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_full(0, 0, buffer_for_args, aligned_byte_offset_for_args);
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_full(0, 0, buffer_for_args, aligned_byte_offset_for_args);
            return;
        }
        if !self.pre_draw() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        let Some((indirect_args_buffer, sub_off)) = Self::resolve_indirect_buffer(buffer_for_args)
        else {
            return;
        };
        let indirect_args_offset = u64::from(aligned_byte_offset_for_args) + sub_off;

        // C++ closure #24 (`DrawInstancedIndirect`, d3d11_context_impl.cpp:2497):
        // emits `WmtcmdRenderDrawIndirect` (`render 21`).
        let cmd = WmtcmdRenderDrawIndirect {
            r#type: WMT_RENDER_COMMAND_DRAW_INDIRECT,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type,
            indirect_args_buffer,
            indirect_args_offset,
        };
        self.emit_render_cmd(cmd, move |enc| {
            enc.bump_visibility_result_offset();
            enc.encode_render_command(cmd);
        });
    }

    /// `ID3D11DeviceContext::DrawIndexedInstancedIndirect` (`d3d11_context_impl.cpp:2429-2472`).
    pub fn draw_indexed_instanced_indirect(
        &mut self,
        buffer_for_args: *mut c_void,
        aligned_byte_offset_for_args: u32,
    ) {
        if buffer_for_args.is_null() {
            return;
        }
        let pipeline_kind = self.encode_snapshot().pipeline_kind;
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_indexed_full(0, 0, buffer_for_args, aligned_byte_offset_for_args);
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_indexed_full(0, 0, buffer_for_args, aligned_byte_offset_for_args);
            return;
        }
        if !self.pre_draw_indexed() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        let Some((indirect_args_buffer, sub_off)) = Self::resolve_indirect_buffer(buffer_for_args)
        else {
            return;
        };
        let snap = self.encode_snapshot();
        let (index_type, _) = index_type_and_stride(snap.index_format);
        let index_buffer_offset = u64::from(snap.index_sub_offset);
        let indirect_args_offset = u64::from(aligned_byte_offset_for_args) + sub_off;

        // C++ closure #23 (`DrawIndexedInstancedIndirect`, d3d11_context_impl.cpp:2453):
        // emits `WmtcmdRenderDrawIndexedIndirect` (`render 22`).
        let cmd = WmtcmdRenderDrawIndexedIndirect {
            r#type: WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type,
            index_type,
            index_buffer: snap.index_buffer,
            index_buffer_offset,
            indirect_args_buffer,
            indirect_args_offset,
        };
        self.emit_render_cmd(cmd, move |enc| {
            enc.bump_visibility_result_offset();
            enc.encode_render_command(cmd);
        });
    }
}
