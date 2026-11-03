//! Geometry-shader and tessellation mesh-draw encoding (`GsDraw*`, `TessellationDraw*`).
//!
//! Encodes direct and indirect geometry-shader and tessellation mesh-shader draws (`render 29..32`
//! and `render 37..40`) and their indirect argument marshal tasks (`d3d11_context_impl.cpp`).

use core::ffi::c_void;

use dxmt_core::{
    deptrack::Access,
    encoder::{NO_NEXT, stage},
};
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

/// Computes `(vertex_per_warp, vertex_increment_per_warp)` for indirect geometry-shader draws.
const fn gs_indirect_vertex_counts(topology: u32, raw_vpw: u32) -> (u32, u32) {
    let vpw = if raw_vpw < 6 { 32 } else { raw_vpw };
    match topology {
        2 => {
            let s = if (vpw & !1) < 2 { 2 } else { vpw & !1 };
            (s, s)
        }
        3 => (vpw, vpw.saturating_sub(1)),
        4 => {
            let s = 3 * (vpw / 3);
            let s = if s < 3 { 3 } else { s };
            (s, s)
        }
        5 => {
            let s = if (vpw & !1) < 4 { 4 } else { vpw & !1 };
            (s, s - 2)
        }
        10 => {
            let s = if (vpw & !3) < 4 { 4 } else { vpw & !3 };
            (s, s)
        }
        11 => (vpw, vpw.saturating_sub(3)),
        12 => {
            let s = 6 * (vpw / 6);
            let s = if s < 6 { 6 } else { s };
            (s, s)
        }
        13 => {
            let s = if (vpw & !1) < 6 { 6 } else { vpw & !1 };
            (s, s - 4)
        }
        _ => (vpw, vpw),
    }
}

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
        self.gs_draw_full_with_args(vertex_count, 1, start_vertex, 0, core::ptr::null_mut(), 0);
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
        let _ = num_control_points;
        self.ts_draw_full_with_args(
            vertex_count_per_instance,
            instance_count,
            start_vertex,
            start_instance,
            core::ptr::null_mut(),
            0,
        );
    }

    /// Full direct or indirect geometry-shader draw with explicit start vertex/instance offsets.
    pub(super) fn gs_draw_full_with_args(
        &mut self,
        vertex_count: u32,
        instance_count: u32,
        start_vertex: u32,
        start_instance: u32,
        arg_buf: *mut c_void,
        arg_offset: u32,
    ) {
        if !self.pre_draw() {
            return;
        }
        if self.is_live() {
            let topo = self.state.topology;
            if arg_buf.is_null() {
                let Some((vertex_per_warp, warp_count)) = gs_warp_metrics(topo, 32, vertex_count)
                else {
                    return;
                };
                if warp_count == 0 {
                    return;
                }
                let draw_args_off = self.pre_allocate_argument_buffer(false, 16, 32);
                let dwords = [vertex_count, instance_count, start_vertex, start_instance];
                let base = WmtcmdRenderDxmtGeometryDraw {
                    r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    draw_arguments_offset: draw_args_off,
                    warp_count,
                    instance_count,
                    vertex_per_warp,
                };
                self.emit_render_cmd(base, move |enc| {
                    let final_off = enc.write_argument_dwords(draw_args_off, &dwords);
                    let mut cmd = base;
                    cmd.draw_arguments_offset = final_off;
                    enc.bump_visibility_result_offset();
                    enc.resolve_render_pass_barrier(
                        dxmt_core::encoder::Encoder::dispatch_tile_barrier,
                    );
                    enc.encode_render_command(cmd);
                });
            } else {
                let Some(arg_buf_obj) =
                    crate::resource::common(arg_buf).and_then(|c| c.buffer.clone())
                else {
                    return;
                };
                let (vertex_per_warp, vertex_inc) = gs_indirect_vertex_counts(topo, 32);
                let base = WmtcmdRenderDxmtGeometryDrawIndirect {
                    r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    imm_draw_arguments: 0,
                    indirect_args_buffer: 0,
                    indirect_args_offset: u64::from(arg_offset),
                    dispatch_args_buffer: 0,
                    dispatch_args_offset: 0,
                    vertex_per_warp,
                };
                self.emit_render_cmd(base, move |enc| {
                    let Some((alloc, sub_off)) =
                        enc.access_buffer::<{ stage::VERTEX }>(&arg_buf_obj, Access::READ)
                    else {
                        return;
                    };
                    let (dispatch_args_buffer, dispatch_args_offset) =
                        enc.allocate_temp_buffer(12, 4);
                    let indirect_args_buffer = alloc.buffer();
                    let indirect_args_offset = u64::from(arg_offset) + sub_off;
                    enc.bump_visibility_result_offset();
                    enc.encode_gs_dispatch_arguments_marshal(
                        indirect_args_buffer,
                        indirect_args_offset,
                        vertex_inc,
                        dispatch_args_buffer,
                        dispatch_args_offset,
                    );
                    enc.resolve_render_pass_barrier(
                        dxmt_core::encoder::Encoder::dispatch_tile_barrier,
                    );
                    let mut cmd = base;
                    cmd.imm_draw_arguments = enc.final_argument_buffer().0;
                    cmd.indirect_args_buffer = indirect_args_buffer;
                    cmd.indirect_args_offset = indirect_args_offset;
                    cmd.dispatch_args_buffer = dispatch_args_buffer;
                    cmd.dispatch_args_offset = dispatch_args_offset;
                    enc.encode_render_command(cmd);
                });
            }
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

    /// Full direct or indirect indexed geometry-shader draw with explicit offsets.
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors D3D11 DrawIndexedInstanced + indirect parameters"
    )]
    pub(super) fn gs_draw_indexed_full_with_args(
        &mut self,
        index_count: u32,
        instance_count: u32,
        start_index: u32,
        base_vertex: i32,
        start_instance: u32,
        arg_buf: *mut c_void,
        arg_offset: u32,
    ) {
        if !self.pre_draw_indexed() {
            return;
        }
        if self.is_live() {
            let topo = self.state.topology;
            let ib_offset = u64::from(self.state.input_assembler.index_buffer_offset);
            if arg_buf.is_null() {
                let Some((vertex_per_warp, warp_count)) = gs_warp_metrics(topo, 32, index_count)
                else {
                    return;
                };
                if warp_count == 0 {
                    return;
                }
                let draw_args_off = self.pre_allocate_argument_buffer(false, 20, 32);
                let dwords = [
                    index_count,
                    instance_count,
                    start_index,
                    base_vertex.cast_unsigned(),
                    start_instance,
                ];
                let base = WmtcmdRenderDxmtGeometryDrawIndexed {
                    r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    draw_arguments_offset: draw_args_off,
                    index_buffer: 0,
                    index_buffer_offset: ib_offset,
                    warp_count,
                    instance_count,
                    vertex_per_warp,
                };
                self.emit_render_cmd(base, move |enc| {
                    let final_off = enc.write_argument_dwords(draw_args_off, &dwords);
                    let (ib_handle, ib_sub_off) = enc.current_index_buffer().unwrap_or((0, 0));
                    let mut cmd = base;
                    cmd.draw_arguments_offset = final_off;
                    cmd.index_buffer = ib_handle;
                    cmd.index_buffer_offset = ib_offset + ib_sub_off;
                    enc.bump_visibility_result_offset();
                    enc.resolve_render_pass_barrier(
                        dxmt_core::encoder::Encoder::dispatch_tile_barrier,
                    );
                    enc.encode_render_command(cmd);
                });
            } else {
                let Some(arg_buf_obj) =
                    crate::resource::common(arg_buf).and_then(|c| c.buffer.clone())
                else {
                    return;
                };
                let (vertex_per_warp, vertex_inc) = gs_indirect_vertex_counts(topo, 32);
                let base = WmtcmdRenderDxmtGeometryDrawIndexedIndirect {
                    r#type: WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    index_buffer: 0,
                    index_buffer_offset: ib_offset,
                    imm_draw_arguments: 0,
                    indirect_args_buffer: 0,
                    indirect_args_offset: u64::from(arg_offset),
                    dispatch_args_buffer: 0,
                    dispatch_args_offset: 0,
                    vertex_per_warp,
                };
                self.emit_render_cmd(base, move |enc| {
                    let Some((alloc, sub_off)) =
                        enc.access_buffer::<{ stage::VERTEX }>(&arg_buf_obj, Access::READ)
                    else {
                        return;
                    };
                    let (dispatch_args_buffer, dispatch_args_offset) =
                        enc.allocate_temp_buffer(12, 4);
                    let (ib_handle, ib_sub_off) = enc.current_index_buffer().unwrap_or((0, 0));
                    let indirect_args_buffer = alloc.buffer();
                    let indirect_args_offset = u64::from(arg_offset) + sub_off;
                    enc.bump_visibility_result_offset();
                    enc.encode_gs_dispatch_arguments_marshal(
                        indirect_args_buffer,
                        indirect_args_offset,
                        vertex_inc,
                        dispatch_args_buffer,
                        dispatch_args_offset,
                    );
                    enc.resolve_render_pass_barrier(
                        dxmt_core::encoder::Encoder::dispatch_tile_barrier,
                    );
                    let mut cmd = base;
                    cmd.index_buffer = ib_handle;
                    cmd.index_buffer_offset = ib_offset + ib_sub_off;
                    cmd.imm_draw_arguments = enc.final_argument_buffer().0;
                    cmd.indirect_args_buffer = indirect_args_buffer;
                    cmd.indirect_args_offset = indirect_args_offset;
                    cmd.dispatch_args_buffer = dispatch_args_buffer;
                    cmd.dispatch_args_offset = dispatch_args_offset;
                    enc.encode_render_command(cmd);
                });
            }
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

    /// Full direct or indirect tessellation mesh draw with explicit start vertex/instance offsets.
    pub(super) fn ts_draw_full_with_args(
        &mut self,
        vertex_count: u32,
        instance_count: u32,
        start_vertex: u32,
        start_instance: u32,
        arg_buf: *mut c_void,
        arg_offset: u32,
    ) {
        if !self.pre_draw() {
            return;
        }
        let control_points = self.state.topology.saturating_sub(32).max(1);
        if self.is_live() {
            if arg_buf.is_null() {
                let patch_count = vertex_count / control_points;
                if patch_count == 0 {
                    return;
                }
                let draw_args_off = self.pre_allocate_argument_buffer(false, 16, 32);
                let dwords = [vertex_count, instance_count, start_vertex, start_instance];
                let base = WmtcmdRenderDxmtTessellationMeshDraw {
                    r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    draw_arguments_offset: draw_args_off,
                    instance_count,
                    threads_per_patch: control_points,
                    patch_per_group: 1,
                    patch_per_mesh_instance: patch_count,
                };
                self.emit_render_cmd(base, move |enc| {
                    let final_off = enc.write_argument_dwords(draw_args_off, &dwords);
                    let threads_per_patch = enc.tess_threads_per_patch.max(1);
                    let patch_per_group = (32 / threads_per_patch).max(1);
                    let patch_per_mesh_instance = patch_count.div_ceil(patch_per_group);
                    let mut cmd = base;
                    cmd.draw_arguments_offset = final_off;
                    cmd.threads_per_patch = threads_per_patch;
                    cmd.patch_per_group = patch_per_group;
                    cmd.patch_per_mesh_instance = patch_per_mesh_instance;
                    enc.bump_visibility_result_offset();
                    enc.resolve_render_pass_barrier(
                        dxmt_core::encoder::Encoder::dispatch_tile_barrier,
                    );
                    enc.encode_render_command(cmd);
                });
            } else {
                let Some(arg_buf_obj) =
                    crate::resource::common(arg_buf).and_then(|c| c.buffer.clone())
                else {
                    return;
                };
                let cp_u16 = u16::try_from(control_points).unwrap_or(1);
                let base = WmtcmdRenderDxmtTessellationMeshDrawIndirect {
                    r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    imm_draw_arguments: 0,
                    indirect_args_buffer: 0,
                    indirect_args_offset: u64::from(arg_offset),
                    dispatch_args_buffer: 0,
                    dispatch_args_offset: 0,
                    threads_per_patch: control_points,
                    patch_per_group: 1,
                };
                self.emit_render_cmd(base, move |enc| {
                    let Some((alloc, sub_off)) =
                        enc.access_buffer::<{ stage::VERTEX }>(&arg_buf_obj, Access::READ)
                    else {
                        return;
                    };
                    let (dispatch_args_buffer, dispatch_args_offset) =
                        enc.allocate_temp_buffer(12, 4);
                    let threads_per_patch = enc.tess_threads_per_patch.max(1);
                    let patch_per_group = (32 / threads_per_patch).max(1);
                    let ppg_u16 = u16::try_from(patch_per_group).unwrap_or(1);
                    let indirect_args_buffer = alloc.buffer();
                    let indirect_args_offset = u64::from(arg_offset) + sub_off;
                    enc.bump_visibility_result_offset();
                    enc.encode_ts_dispatch_arguments_marshal(
                        indirect_args_buffer,
                        indirect_args_offset,
                        cp_u16,
                        ppg_u16,
                        dispatch_args_buffer,
                        dispatch_args_offset,
                    );
                    enc.resolve_render_pass_barrier(
                        dxmt_core::encoder::Encoder::dispatch_tile_barrier,
                    );
                    let mut cmd = base;
                    cmd.imm_draw_arguments = enc.final_argument_buffer().0;
                    cmd.indirect_args_buffer = indirect_args_buffer;
                    cmd.indirect_args_offset = indirect_args_offset;
                    cmd.dispatch_args_buffer = dispatch_args_buffer;
                    cmd.dispatch_args_offset = dispatch_args_offset;
                    cmd.threads_per_patch = threads_per_patch;
                    cmd.patch_per_group = patch_per_group;
                    enc.encode_render_command(cmd);
                });
            }
            return;
        }
        let snap = self.encode_snapshot();
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

    /// Full direct or indirect indexed tessellation mesh draw with explicit offsets.
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors D3D11 DrawIndexedInstanced + indirect parameters"
    )]
    pub(super) fn ts_draw_indexed_full_with_args(
        &mut self,
        index_count: u32,
        instance_count: u32,
        start_index: u32,
        base_vertex: i32,
        start_instance: u32,
        arg_buf: *mut c_void,
        arg_offset: u32,
    ) {
        if !self.pre_draw_indexed() {
            return;
        }
        let control_points = self.state.topology.saturating_sub(32).max(1);
        if self.is_live() {
            let ib_offset = u64::from(self.state.input_assembler.index_buffer_offset);
            if arg_buf.is_null() {
                let patch_count = index_count / control_points;
                if patch_count == 0 {
                    return;
                }
                let draw_args_off = self.pre_allocate_argument_buffer(false, 20, 32);
                let dwords = [
                    index_count,
                    instance_count,
                    start_index,
                    base_vertex.cast_unsigned(),
                    start_instance,
                ];
                let base = WmtcmdRenderDxmtTessellationMeshDrawIndexed {
                    r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    draw_arguments_offset: draw_args_off,
                    index_buffer: 0,
                    index_buffer_offset: ib_offset,
                    instance_count,
                    threads_per_patch: control_points,
                    patch_per_group: 1,
                    patch_per_mesh_instance: patch_count,
                };
                self.emit_render_cmd(base, move |enc| {
                    let final_off = enc.write_argument_dwords(draw_args_off, &dwords);
                    let (ib_handle, ib_sub_off) = enc.current_index_buffer().unwrap_or((0, 0));
                    let threads_per_patch = enc.tess_threads_per_patch.max(1);
                    let patch_per_group = (32 / threads_per_patch).max(1);
                    let patch_per_mesh_instance = patch_count.div_ceil(patch_per_group);
                    let mut cmd = base;
                    cmd.draw_arguments_offset = final_off;
                    cmd.index_buffer = ib_handle;
                    cmd.index_buffer_offset = ib_offset + ib_sub_off;
                    cmd.threads_per_patch = threads_per_patch;
                    cmd.patch_per_group = patch_per_group;
                    cmd.patch_per_mesh_instance = patch_per_mesh_instance;
                    enc.bump_visibility_result_offset();
                    enc.resolve_render_pass_barrier(
                        dxmt_core::encoder::Encoder::dispatch_tile_barrier,
                    );
                    enc.encode_render_command(cmd);
                });
            } else {
                let Some(arg_buf_obj) =
                    crate::resource::common(arg_buf).and_then(|c| c.buffer.clone())
                else {
                    return;
                };
                let cp_u16 = u16::try_from(control_points).unwrap_or(1);
                let base = WmtcmdRenderDxmtTessellationMeshDrawIndexedIndirect {
                    r#type: WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    imm_draw_arguments: 0,
                    indirect_args_buffer: 0,
                    indirect_args_offset: u64::from(arg_offset),
                    dispatch_args_buffer: 0,
                    dispatch_args_offset: 0,
                    index_buffer: 0,
                    index_buffer_offset: ib_offset,
                    threads_per_patch: control_points,
                    patch_per_group: 1,
                };
                self.emit_render_cmd(base, move |enc| {
                    let Some((alloc, sub_off)) =
                        enc.access_buffer::<{ stage::VERTEX }>(&arg_buf_obj, Access::READ)
                    else {
                        return;
                    };
                    let (dispatch_args_buffer, dispatch_args_offset) =
                        enc.allocate_temp_buffer(12, 4);
                    let (ib_handle, ib_sub_off) = enc.current_index_buffer().unwrap_or((0, 0));
                    let threads_per_patch = enc.tess_threads_per_patch.max(1);
                    let patch_per_group = (32 / threads_per_patch).max(1);
                    let ppg_u16 = u16::try_from(patch_per_group).unwrap_or(1);
                    let indirect_args_buffer = alloc.buffer();
                    let indirect_args_offset = u64::from(arg_offset) + sub_off;
                    enc.bump_visibility_result_offset();
                    enc.encode_ts_dispatch_arguments_marshal(
                        indirect_args_buffer,
                        indirect_args_offset,
                        cp_u16,
                        ppg_u16,
                        dispatch_args_buffer,
                        dispatch_args_offset,
                    );
                    enc.resolve_render_pass_barrier(
                        dxmt_core::encoder::Encoder::dispatch_tile_barrier,
                    );
                    let mut cmd = base;
                    cmd.imm_draw_arguments = enc.final_argument_buffer().0;
                    cmd.indirect_args_buffer = indirect_args_buffer;
                    cmd.indirect_args_offset = indirect_args_offset;
                    cmd.dispatch_args_buffer = dispatch_args_buffer;
                    cmd.dispatch_args_offset = dispatch_args_offset;
                    cmd.index_buffer = ib_handle;
                    cmd.index_buffer_offset = ib_offset + ib_sub_off;
                    cmd.threads_per_patch = threads_per_patch;
                    cmd.patch_per_group = patch_per_group;
                    enc.encode_render_command(cmd);
                });
            }
            return;
        }
        let snap = self.encode_snapshot();
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
