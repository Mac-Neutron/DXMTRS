//! Context draw and indirect draw commands (`d3d11_context_impl.cpp`).
//!
//! Implements `Draw`, `DrawIndexed`, `DrawInstanced`, `DrawIndexedInstanced`, `DrawAuto`,
//! `DrawInstancedIndirect`, and `DrawIndexedInstancedIndirect`, routing geometry-shader and
//! tessellation draws to [`super::gs_ts`] and encoding ordinary draws into the active render pass.

use core::{ffi::c_void, ptr};

use dxmt_core::{
    deptrack::Access,
    encoder::{Encoder, NO_NEXT, kind, stage},
    frame_stats::CompatibilityFlags,
};
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

/// Converts a D3D11 primitive topology to a Metal `WMTPrimitiveTopologyClass` (`1..=3`).
pub(super) const fn to_metal_primitive_topology(topology: u32) -> u32 {
    match topology {
        1 => 1,
        2 | 3 | 10 | 11 => 2,
        4 | 5 | 12 | 13 | 33..=64 => 3,
        _ => 0,
    }
}

/// Returns `true` if `topology` is a line-strip or triangle-strip topology.
pub(super) const fn is_strip_topology(topology: u32) -> bool {
    matches!(topology, 3 | 5 | 11 | 13)
}

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
    /// Active pipeline kind (`ORDINARY`, `GEOMETRY`, or `TESSELLATION`) for draw dispatch routing.
    fn active_draw_pipeline_kind(&self) -> u8 {
        if self.is_live() {
            let hs = self.state.shader_stages[super::state::ShaderStage::Hull.index()]
                .shader
                .as_ptr();
            let ds = self.state.shader_stages[super::state::ShaderStage::Domain.index()]
                .shader
                .as_ptr();
            let gs = self.state.shader_stages[super::state::ShaderStage::Geometry.index()]
                .shader
                .as_ptr();
            if !hs.is_null() && !ds.is_null() {
                kind::TESSELLATION
            } else if !gs.is_null()
                && crate::input_layout::StreamOutputLayout::from_iface(gs).is_none()
            {
                kind::GEOMETRY
            } else {
                kind::ORDINARY
            }
        } else {
            self.encode_snapshot().pipeline_kind
        }
    }

    /// `ID3D11DeviceContext::Draw` (`d3d11_context_impl.cpp:2287-2315`).
    pub fn draw(&mut self, vertex_count: u32, start_vertex: u32) {
        if vertex_count == 0 {
            return;
        }
        let pipeline_kind = self.active_draw_pipeline_kind();
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_full_with_args(vertex_count, 1, start_vertex, 0, ptr::null_mut(), 0);
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_full_with_args(vertex_count, 1, start_vertex, 0, ptr::null_mut(), 0);
            return;
        }
        if !self.pre_draw() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        let is_live = self.is_live();
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
            if is_live {
                enc.resolve_render_pass_barrier(Encoder::dispatch_tile_barrier);
            }
            enc.encode_render_command(cmd);
        });
    }

    /// `ID3D11DeviceContext::DrawIndexed` (`d3d11_context_impl.cpp:2317-2351`).
    pub fn draw_indexed(&mut self, index_count: u32, start_index: u32, base_vertex: i32) {
        if index_count == 0 {
            return;
        }
        let pipeline_kind = self.active_draw_pipeline_kind();
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_indexed_full_with_args(
                index_count,
                1,
                start_index,
                base_vertex,
                0,
                ptr::null_mut(),
                0,
            );
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_indexed_full_with_args(
                index_count,
                1,
                start_index,
                base_vertex,
                0,
                ptr::null_mut(),
                0,
            );
            return;
        }
        if !self.pre_draw_indexed() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        if self.is_live() {
            let (index_type, stride) =
                index_type_and_stride(self.state.input_assembler.index_buffer_format);
            let start_byte_offset = u64::from(start_index) * stride
                + u64::from(self.state.input_assembler.index_buffer_offset);
            let base = WmtcmdRenderDrawIndexed {
                r#type: WMT_RENDER_COMMAND_DRAW_INDEXED,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type,
                index_type,
                index_count: u64::from(index_count),
                index_buffer: 0,
                index_buffer_offset: start_byte_offset,
                instance_count: 1,
                base_vertex,
                base_instance: 0,
            };
            self.emit_render_cmd(base, move |enc| {
                let (ib_handle, ib_sub_off) = enc.current_index_buffer().unwrap_or((0, 0));
                let mut cmd = base;
                cmd.index_buffer = ib_handle;
                cmd.index_buffer_offset = start_byte_offset + ib_sub_off;
                enc.bump_visibility_result_offset();
                enc.resolve_render_pass_barrier(Encoder::dispatch_tile_barrier);
                enc.encode_render_command(cmd);
            });
            return;
        }
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
        let pipeline_kind = self.active_draw_pipeline_kind();
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_full_with_args(
                vertex_count_per_instance,
                instance_count,
                start_vertex,
                start_instance,
                ptr::null_mut(),
                0,
            );
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_full_with_args(
                vertex_count_per_instance,
                instance_count,
                start_vertex,
                start_instance,
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
        let is_live = self.is_live();
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
            if is_live {
                enc.resolve_render_pass_barrier(Encoder::dispatch_tile_barrier);
            }
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
        let pipeline_kind = self.active_draw_pipeline_kind();
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_indexed_full_with_args(
                index_count_per_instance,
                instance_count,
                start_index,
                base_vertex,
                start_instance,
                ptr::null_mut(),
                0,
            );
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_indexed_full_with_args(
                index_count_per_instance,
                instance_count,
                start_index,
                base_vertex,
                start_instance,
                ptr::null_mut(),
                0,
            );
            return;
        }
        if !self.pre_draw_indexed() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        if self.is_live() {
            let (index_type, stride) =
                index_type_and_stride(self.state.input_assembler.index_buffer_format);
            let start_byte_offset = u64::from(start_index) * stride
                + u64::from(self.state.input_assembler.index_buffer_offset);
            let base = WmtcmdRenderDrawIndexed {
                r#type: WMT_RENDER_COMMAND_DRAW_INDEXED,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type,
                index_type,
                index_count: u64::from(index_count_per_instance),
                index_buffer: 0,
                index_buffer_offset: start_byte_offset,
                instance_count,
                base_vertex,
                base_instance: start_instance,
            };
            self.emit_render_cmd(base, move |enc| {
                let (ib_handle, ib_sub_off) = enc.current_index_buffer().unwrap_or((0, 0));
                let mut cmd = base;
                cmd.index_buffer = ib_handle;
                cmd.index_buffer_offset = start_byte_offset + ib_sub_off;
                enc.bump_visibility_result_offset();
                enc.resolve_render_pass_barrier(Encoder::dispatch_tile_barrier);
                enc.encode_render_command(cmd);
            });
            return;
        }
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
        M::emit_st(self, |enc| {
            enc.set_compatibility_flag(CompatibilityFlags::UNSUPPORTED_DRAW_AUTO);
        });
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
        let pipeline_kind = self.active_draw_pipeline_kind();
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_full_with_args(0, 0, 0, 0, buffer_for_args, aligned_byte_offset_for_args);
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_full_with_args(0, 0, 0, 0, buffer_for_args, aligned_byte_offset_for_args);
            return;
        }
        if !self.pre_draw() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        if self.is_live() {
            let Some(arg_buf) =
                crate::resource::common(buffer_for_args).and_then(|c| c.buffer.clone())
            else {
                return;
            };
            let base = WmtcmdRenderDrawIndirect {
                r#type: WMT_RENDER_COMMAND_DRAW_INDIRECT,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type,
                indirect_args_buffer: 0,
                indirect_args_offset: u64::from(aligned_byte_offset_for_args),
            };
            self.emit_render_cmd(base, move |enc| {
                let Some((alloc, sub_off)) =
                    enc.access_buffer::<{ stage::VERTEX }>(&arg_buf, Access::READ)
                else {
                    return;
                };
                let mut cmd = base;
                cmd.indirect_args_buffer = alloc.buffer();
                cmd.indirect_args_offset = u64::from(aligned_byte_offset_for_args) + sub_off;
                enc.bump_visibility_result_offset();
                enc.resolve_render_pass_barrier(Encoder::dispatch_tile_barrier);
                enc.encode_render_command(cmd);
            });
            return;
        }
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
        let pipeline_kind = self.active_draw_pipeline_kind();
        if pipeline_kind == kind::TESSELLATION {
            self.ts_draw_indexed_full_with_args(
                0,
                0,
                0,
                0,
                0,
                buffer_for_args,
                aligned_byte_offset_for_args,
            );
            return;
        }
        if pipeline_kind == kind::GEOMETRY {
            self.gs_draw_indexed_full_with_args(
                0,
                0,
                0,
                0,
                0,
                buffer_for_args,
                aligned_byte_offset_for_args,
            );
            return;
        }
        if !self.pre_draw_indexed() {
            return;
        }
        let Some(primitive_type) = to_metal_primitive_type(self.state.topology) else {
            return;
        };
        if self.is_live() {
            let Some(arg_buf) =
                crate::resource::common(buffer_for_args).and_then(|c| c.buffer.clone())
            else {
                return;
            };
            let (index_type, _) =
                index_type_and_stride(self.state.input_assembler.index_buffer_format);
            let ib_offset = u64::from(self.state.input_assembler.index_buffer_offset);
            let base = WmtcmdRenderDrawIndexedIndirect {
                r#type: WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type,
                index_type,
                index_buffer: 0,
                index_buffer_offset: ib_offset,
                indirect_args_buffer: 0,
                indirect_args_offset: u64::from(aligned_byte_offset_for_args),
            };
            self.emit_render_cmd(base, move |enc| {
                let Some((alloc, sub_off)) =
                    enc.access_buffer::<{ stage::VERTEX }>(&arg_buf, Access::READ)
                else {
                    return;
                };
                let (ib_handle, ib_sub_off) = enc.current_index_buffer().unwrap_or((0, 0));
                let mut cmd = base;
                cmd.index_buffer = ib_handle;
                cmd.index_buffer_offset = ib_offset + ib_sub_off;
                cmd.indirect_args_buffer = alloc.buffer();
                cmd.indirect_args_offset = u64::from(aligned_byte_offset_for_args) + sub_off;
                enc.bump_visibility_result_offset();
                enc.resolve_render_pass_barrier(Encoder::dispatch_tile_barrier);
                enc.encode_render_command(cmd);
            });
            return;
        }
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
