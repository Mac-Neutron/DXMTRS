//! Geometry-shader and tessellation mesh-draw encoding (`GsDraw*`, `TessellationDraw*`).
//!
//! Encodes direct and indirect geometry-shader and tessellation mesh-shader draws (`render 29..32`
//! and `render 37..40`) and their indirect argument marshal tasks (`d3d11_context_impl.cpp`).

use core::ffi::c_void;

use dxmt_core::encoder::NO_NEXT;
use winemetal_sys::wire::{
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW, WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT, WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT, WmtcmdRenderDxmtGeometryDraw,
    WmtcmdRenderDxmtGeometryDrawIndexed, WmtcmdRenderDxmtGeometryDrawIndexedIndirect,
    WmtcmdRenderDxmtGeometryDrawIndirect, WmtcmdRenderDxmtTessellationMeshDraw,
    WmtcmdRenderDxmtTessellationMeshDrawIndexed,
    WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect,
    WmtcmdRenderDxmtTessellationMeshDrawIndirect,
};

use super::{Context, Mode};

/// Computes `(vertex_per_warp, warp_count)` for a geometry-shader draw (`GsDraw`).
fn gs_warp_metrics(topology: u32, raw_vpw: u32, count: u32) -> Option<(u32, u32)> {
    let vpw = raw_vpw.max(6);
    match topology {
        // PointList (1) or PatchList (33..=64)
        1 | 33..=64 => Some((vpw, count.div_ceil(vpw))),
        // LineList (2)
        2 => {
            let step = (vpw & !1).max(2);
            Some((step, count.div_ceil(step)))
        }
        // LineStrip (3)
        3 => {
            if count < 2 {
                return None;
            }
            let step = vpw.saturating_sub(1).max(1);
            Some((vpw, (count - 1).div_ceil(step)))
        }
        // TriangleList (4)
        4 => {
            let step = (3 * (vpw / 3)).max(3);
            Some((step, count.div_ceil(step)))
        }
        // TriangleStrip (5)
        5 => {
            if count < 3 {
                return None;
            }
            let adj_vpw = (vpw & !1).max(4);
            Some((adj_vpw, (count - 2).div_ceil(adj_vpw - 2)))
        }
        // LineListAdj (10)
        10 => {
            let step = (vpw & !3).max(4);
            Some((step, count.div_ceil(step)))
        }
        // LineStripAdj (11)
        11 => {
            if count < 4 {
                return None;
            }
            let step = vpw.saturating_sub(3).max(1);
            Some((vpw, (count - 3).div_ceil(step)))
        }
        // TriangleListAdj (12)
        12 => {
            let step = (6 * (vpw / 6)).max(6);
            Some((step, count.div_ceil(step)))
        }
        // TriangleStripAdj (13)
        13 => {
            if count < 6 {
                return None;
            }
            let adj_vpw = (vpw & !1).max(6);
            Some((adj_vpw, (count - 4).div_ceil(adj_vpw - 4)))
        }
        _ => None,
    }
}

impl<M: Mode> Context<M> {
    /// Emits a non-indexed geometry-shader draw (`GeometryDraw`).
    pub fn geometry_draw(&mut self, vertex_count: u32, start_vertex: u32) {
        let _ = start_vertex;
        self.gs_draw_full(vertex_count, 1, core::ptr::null_mut(), 0);
    }

    /// Emits a non-indexed tessellation draw (`TessellationDraw`).
    pub fn tessellation_draw(
        &mut self,
        num_control_points: u32,
        vertex_count_per_instance: u32,
        instance_count: u32,
        start_vertex: u32,
        start_instance: u32,
    ) {
        let _ = (num_control_points, start_vertex, start_instance);
        self.ts_draw_full(
            vertex_count_per_instance,
            instance_count,
            core::ptr::null_mut(),
            0,
        );
    }

    /// Full direct or indirect geometry-shader draw (`GsDraw`, `d3d11_context_impl.cpp:1938-2031`).
    pub(super) fn gs_draw_full(
        &mut self,
        vertex_count: u32,
        instance_count: u32,
        arg_buf: *mut c_void,
        arg_offset: u32,
    ) {
        if !self.pre_draw() {
            return;
        }
        let snap = self.encode_snapshot();
        if arg_buf.is_null() {
            let Some((vertex_per_warp, warp_count)) =
                gs_warp_metrics(self.state.topology, snap.gs_vertex_per_warp, vertex_count)
            else {
                return;
            };
            if warp_count == 0 {
                return;
            }
            // C++ closure #11 (`GsDraw` direct, d3d11_context_impl.cpp:2002):
            // emits `WmtcmdRenderDxmtGeometryDraw` (`render 29`).
            let cmd = WmtcmdRenderDxmtGeometryDraw {
                r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW,
                reserved: [0; 3],
                next: NO_NEXT,
                draw_arguments_offset: snap.gs_draw_arguments_offset,
                warp_count,
                instance_count,
                vertex_per_warp,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.bump_visibility_result_offset();
                enc.encode_render_command(cmd);
            });
        } else {
            let Some((indirect_args_buffer, sub_off)) = Self::resolve_indirect_buffer(arg_buf)
            else {
                return;
            };
            let indirect_args_offset = u64::from(arg_offset) + sub_off;
            let vertex_per_warp = snap.gs_vertex_per_warp.max(1);
            let imm_draw_arguments = snap.gs_imm_draw_args_handle;
            let dispatch_args_buffer = snap.gs_dispatch_args_handle;
            let dispatch_args_offset = snap.gs_dispatch_args_offset;
            // C++ closure #12 (`GsDraw` indirect, d3d11_context_impl.cpp:2020):
            // queues GS marshal task and emits `WmtcmdRenderDxmtGeometryDrawIndirect` (`render 31`).
            let cmd = WmtcmdRenderDxmtGeometryDrawIndirect {
                r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT,
                reserved: [0; 3],
                next: NO_NEXT,
                imm_draw_arguments,
                indirect_args_buffer,
                indirect_args_offset,
                dispatch_args_buffer,
                dispatch_args_offset,
                vertex_per_warp,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.bump_visibility_result_offset();
                enc.encode_gs_dispatch_arguments_marshal(
                    indirect_args_buffer,
                    indirect_args_offset,
                    vertex_per_warp,
                    dispatch_args_buffer,
                    dispatch_args_offset,
                );
                enc.encode_render_command(cmd);
            });
        }
    }

    /// Full direct or indirect indexed geometry-shader draw (`GsDrawIndexed`).
    pub(super) fn gs_draw_indexed_full(
        &mut self,
        index_count: u32,
        instance_count: u32,
        arg_buf: *mut c_void,
        arg_offset: u32,
    ) {
        if !self.pre_draw_indexed() {
            return;
        }
        let snap = self.encode_snapshot();
        let index_buffer = snap.index_buffer;
        let index_buffer_offset = u64::from(snap.index_sub_offset);
        if arg_buf.is_null() {
            let Some((vertex_per_warp, warp_count)) =
                gs_warp_metrics(self.state.topology, snap.gs_vertex_per_warp, index_count)
            else {
                return;
            };
            if warp_count == 0 {
                return;
            }
            // C++ closure #13 (`GsDrawIndexed` direct, d3d11_context_impl.cpp:2103):
            // emits `WmtcmdRenderDxmtGeometryDrawIndexed` (`render 30`).
            let cmd = WmtcmdRenderDxmtGeometryDrawIndexed {
                r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
                reserved: [0; 3],
                next: NO_NEXT,
                draw_arguments_offset: snap.gs_draw_arguments_offset,
                index_buffer,
                index_buffer_offset,
                warp_count,
                instance_count,
                vertex_per_warp,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.bump_visibility_result_offset();
                enc.encode_render_command(cmd);
            });
        } else {
            let Some((indirect_args_buffer, sub_off)) = Self::resolve_indirect_buffer(arg_buf)
            else {
                return;
            };
            let indirect_args_offset = u64::from(arg_offset) + sub_off;
            let vertex_per_warp = snap.gs_vertex_per_warp.max(1);
            let imm_draw_arguments = snap.gs_imm_draw_args_handle;
            let dispatch_args_buffer = snap.gs_dispatch_args_handle;
            let dispatch_args_offset = snap.gs_dispatch_args_offset;
            // C++ closure #14 (`GsDrawIndexed` indirect, d3d11_context_impl.cpp:2124):
            // queues GS marshal task and emits `WmtcmdRenderDxmtGeometryDrawIndexedIndirect` (`render 32`).
            let cmd = WmtcmdRenderDxmtGeometryDrawIndexedIndirect {
                r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
                reserved: [0; 3],
                next: NO_NEXT,
                index_buffer,
                index_buffer_offset,
                imm_draw_arguments,
                indirect_args_buffer,
                indirect_args_offset,
                dispatch_args_buffer,
                dispatch_args_offset,
                vertex_per_warp,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.bump_visibility_result_offset();
                enc.encode_gs_dispatch_arguments_marshal(
                    indirect_args_buffer,
                    indirect_args_offset,
                    vertex_per_warp,
                    dispatch_args_buffer,
                    dispatch_args_offset,
                );
                enc.encode_render_command(cmd);
            });
        }
    }

    /// Full direct or indirect tessellation mesh draw (`TessellationDraw`).
    pub(super) fn ts_draw_full(
        &mut self,
        vertex_count: u32,
        instance_count: u32,
        arg_buf: *mut c_void,
        arg_offset: u32,
    ) {
        if !self.pre_draw() {
            return;
        }
        let snap = self.encode_snapshot();
        let control_points = self.state.topology.saturating_sub(32).max(1);
        let threads_per_patch = snap.ts_threads_per_patch.max(control_points);
        let patch_per_group = snap.ts_patch_per_group.max(1);
        if arg_buf.is_null() {
            let patch_count = vertex_count / control_points;
            if patch_count == 0 {
                return;
            }
            let patch_per_mesh_instance = if snap.ts_patch_per_mesh_instance != 0 {
                snap.ts_patch_per_mesh_instance
            } else {
                patch_count
            };
            // C++ closure #15 (`TessellationDraw` direct, d3d11_context_impl.cpp:2177):
            // emits `WmtcmdRenderDxmtTessellationMeshDraw` (`render 37`).
            let cmd = WmtcmdRenderDxmtTessellationMeshDraw {
                r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
                reserved: [0; 3],
                next: NO_NEXT,
                draw_arguments_offset: snap.ts_draw_arguments_offset,
                instance_count,
                threads_per_patch,
                patch_per_group,
                patch_per_mesh_instance,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.bump_visibility_result_offset();
                enc.encode_render_command(cmd);
            });
        } else {
            let Some((indirect_args_buffer, sub_off)) = Self::resolve_indirect_buffer(arg_buf)
            else {
                return;
            };
            let indirect_args_offset = u64::from(arg_offset) + sub_off;
            let imm_draw_arguments = snap.ts_imm_draw_args_handle;
            let dispatch_args_buffer = snap.ts_dispatch_args_handle;
            let dispatch_args_offset = snap.ts_dispatch_args_offset;
            let cp_u16 = u16::try_from(control_points).unwrap_or(1);
            let ppg_u16 = u16::try_from(patch_per_group).unwrap_or(1);
            // C++ closure #16 (`TessellationDraw` indirect, d3d11_context_impl.cpp:2196):
            // queues TS marshal task and emits `WmtcmdRenderDxmtTessellationMeshDrawIndirect` (`render 39`).
            let cmd = WmtcmdRenderDxmtTessellationMeshDrawIndirect {
                r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT,
                reserved: [0; 3],
                next: NO_NEXT,
                imm_draw_arguments,
                indirect_args_buffer,
                indirect_args_offset,
                dispatch_args_buffer,
                dispatch_args_offset,
                threads_per_patch,
                patch_per_group,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.bump_visibility_result_offset();
                enc.encode_ts_dispatch_arguments_marshal(
                    indirect_args_buffer,
                    indirect_args_offset,
                    cp_u16,
                    ppg_u16,
                    dispatch_args_buffer,
                    dispatch_args_offset,
                );
                enc.encode_render_command(cmd);
            });
        }
    }

    /// Full direct or indirect indexed tessellation mesh draw (`TessellationDrawIndexed`).
    pub(super) fn ts_draw_indexed_full(
        &mut self,
        index_count: u32,
        instance_count: u32,
        arg_buf: *mut c_void,
        arg_offset: u32,
    ) {
        if !self.pre_draw_indexed() {
            return;
        }
        let snap = self.encode_snapshot();
        let control_points = self.state.topology.saturating_sub(32).max(1);
        let threads_per_patch = snap.ts_threads_per_patch.max(control_points);
        let patch_per_group = snap.ts_patch_per_group.max(1);
        let index_buffer = snap.index_buffer;
        let index_buffer_offset = u64::from(snap.index_sub_offset);
        if arg_buf.is_null() {
            let patch_count = index_count / control_points;
            if patch_count == 0 {
                return;
            }
            let patch_per_mesh_instance = if snap.ts_patch_per_mesh_instance != 0 {
                snap.ts_patch_per_mesh_instance
            } else {
                patch_count
            };
            // C++ closure #17 (`TessellationDrawIndexed` direct, d3d11_context_impl.cpp:2252):
            // emits `WmtcmdRenderDxmtTessellationMeshDrawIndexed` (`render 38`).
            let cmd = WmtcmdRenderDxmtTessellationMeshDrawIndexed {
                r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED,
                reserved: [0; 3],
                next: NO_NEXT,
                draw_arguments_offset: snap.ts_draw_arguments_offset,
                index_buffer,
                index_buffer_offset,
                instance_count,
                threads_per_patch,
                patch_per_group,
                patch_per_mesh_instance,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.bump_visibility_result_offset();
                enc.encode_render_command(cmd);
            });
        } else {
            let Some((indirect_args_buffer, sub_off)) = Self::resolve_indirect_buffer(arg_buf)
            else {
                return;
            };
            let indirect_args_offset = u64::from(arg_offset) + sub_off;
            let imm_draw_arguments = snap.ts_imm_draw_args_handle;
            let dispatch_args_buffer = snap.ts_dispatch_args_handle;
            let dispatch_args_offset = snap.ts_dispatch_args_offset;
            let cp_u16 = u16::try_from(control_points).unwrap_or(1);
            let ppg_u16 = u16::try_from(patch_per_group).unwrap_or(1);
            // C++ closure #18 (`TessellationDrawIndexed` indirect, d3d11_context_impl.cpp:2274):
            // queues TS marshal task and emits `WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect` (`render 40`).
            let cmd = WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect {
                r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT,
                reserved: [0; 3],
                next: NO_NEXT,
                imm_draw_arguments,
                indirect_args_buffer,
                indirect_args_offset,
                dispatch_args_buffer,
                dispatch_args_offset,
                index_buffer,
                index_buffer_offset,
                threads_per_patch,
                patch_per_group,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.bump_visibility_result_offset();
                enc.encode_ts_dispatch_arguments_marshal(
                    indirect_args_buffer,
                    indirect_args_offset,
                    cp_u16,
                    ppg_u16,
                    dispatch_args_buffer,
                    dispatch_args_offset,
                );
                enc.encode_render_command(cmd);
            });
        }
    }
}
