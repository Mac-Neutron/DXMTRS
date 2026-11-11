//! Test support for a front end's host tests (the `headless` feature): records as text.
//!
//! [`format_pass_records`] prints a record list as the `DXMT_WMTCMD_DUMP` lines d3d11's golden
//! files hold (its tests keep their own copy), plus the direct binds D3D9's front end records.

use core::{fmt::Write, ptr};

use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_NOP, WMT_BLIT_COMMAND_UPDATE_FENCE, WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
    WMT_COMPUTE_COMMAND_DISPATCH, WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT, WMT_COMPUTE_COMMAND_NOP,
    WMT_COMPUTE_COMMAND_SET_PSO, WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT, WMT_RENDER_COMMAND_DRAW_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW, WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT, WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED, WMT_RENDER_COMMAND_NOP,
    WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF, WMT_RENDER_COMMAND_SET_DSSO,
    WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES, WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE,
    WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE, WMT_RENDER_COMMAND_SET_MESH_BUFFER,
    WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_OBJECT_BUFFER,
    WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_PSO,
    WMT_RENDER_COMMAND_SET_RASTERIZER_STATE, WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
    WMT_RENDER_COMMAND_SET_VERTEX_BUFFER, WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_VERTEX_BYTES, WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE,
    WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE, WMT_RENDER_COMMAND_SET_VIEWPORTS,
    WMT_RENDER_COMMAND_UPDATE_FENCE, WMT_RENDER_COMMAND_WAIT_FOR_FENCE, WMTScissorRect,
    WMTViewport, WmtcmdBase, WmtcmdBlitFenceOp, WmtcmdComputeDispatch,
    WmtcmdComputeDispatchIndirect, WmtcmdComputeSetpso, WmtcmdRenderDraw, WmtcmdRenderDrawIndexed,
    WmtcmdRenderDrawIndexedIndirect, WmtcmdRenderDrawIndirect, WmtcmdRenderDxmtGeometryDraw,
    WmtcmdRenderDxmtGeometryDrawIndexed, WmtcmdRenderDxmtGeometryDrawIndexedIndirect,
    WmtcmdRenderDxmtGeometryDrawIndirect, WmtcmdRenderDxmtTessellationMeshDraw,
    WmtcmdRenderDxmtTessellationMeshDrawIndexed, WmtcmdRenderFenceOp, WmtcmdRenderSetblendcolor,
    WmtcmdRenderSetbuffer, WmtcmdRenderSetbufferoffset, WmtcmdRenderSetbytes, WmtcmdRenderSetdsso,
    WmtcmdRenderSetpso, WmtcmdRenderSetrasterizerstate, WmtcmdRenderSetsampler,
    WmtcmdRenderSetscissorrects, WmtcmdRenderSettexture, WmtcmdRenderSetviewports,
};

/// The encoder a record list is for: record types are numbered per encoder.
pub enum Records {
    Render,
    Compute,
    Blit,
}

fn f32_hex(val: f32) -> String {
    format!("0x{:08x}", val.to_bits())
}

fn f64_as_f32_hex(val: f64) -> String {
    #[expect(clippy::cast_possible_truncation, reason = "matches winemetal dump")]
    let narrow = val as f32;
    f32_hex(narrow)
}

/// The records from `first` on, one line each, as `DXMT_WMTCMD_DUMP` prints them.
///
/// No-op records (a pass's head) print nothing; a type without its own format prints its kind and
/// number only. `first` may be a pass's head ([`crate::encoder::Pass::records`]) or the first
/// record Metal's encoder is handed.
///
/// # Panics
///
/// When a record's address does not fit a pointer, which no live list's does.
///
/// # Safety
///
/// `first` is null or the first of a list of live records of `kind`'s encoder, each led by
/// [`WmtcmdBase`]'s fields and linked through `next`.
#[must_use]
pub unsafe fn format_pass_records(kind: &Records, first: *const WmtcmdBase) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur_addr = first.expose_provenance() as u64;
    while cur_addr != 0 {
        let base_ptr = ptr::with_exposed_provenance::<WmtcmdBase>(
            usize::try_from(cur_addr).expect("pointer width"),
        );
        // SAFETY: a record of the caller's list.
        let base = unsafe { &*base_ptr };
        let cmd_type = base.r#type;
        match kind {
            Records::Render => match cmd_type {
                WMT_RENDER_COMMAND_NOP => {}
                WMT_RENDER_COMMAND_WAIT_FOR_FENCE | WMT_RENDER_COMMAND_UPDATE_FENCE => {
                    // SAFETY: `cmd_type` verified as a fence command.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderFenceOp>() };
                    lines.push(format!(
                        "render {cmd_type} fence=#{} stages={}",
                        c.fence, c.stages
                    ));
                }
                WMT_RENDER_COMMAND_SET_VERTEX_BUFFER
                | WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER
                | WMT_RENDER_COMMAND_SET_MESH_BUFFER
                | WMT_RENDER_COMMAND_SET_OBJECT_BUFFER => {
                    // SAFETY: `cmd_type` verified as a set-buffer command.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetbuffer>() };
                    lines.push(format!(
                        "render {cmd_type} buffer=#{} offset={} index={}",
                        c.buffer, c.offset, c.index
                    ));
                }
                WMT_RENDER_COMMAND_SET_VERTEX_BYTES | WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES => {
                    // SAFETY: `cmd_type` verified as a set-bytes command.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetbytes>() };
                    lines.push(format!(
                        "render {cmd_type} length={} index={}",
                        c.length, c.index
                    ));
                }
                WMT_RENDER_COMMAND_SET_VERTEX_TEXTURE | WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE => {
                    // SAFETY: `cmd_type` verified as a set-texture command.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSettexture>() };
                    lines.push(format!(
                        "render {cmd_type} texture=#{} index={}",
                        c.texture, c.index
                    ));
                }
                WMT_RENDER_COMMAND_SET_VERTEX_SAMPLER_STATE
                | WMT_RENDER_COMMAND_SET_FRAGMENT_SAMPLER_STATE => {
                    // SAFETY: `cmd_type` verified as a set-sampler command.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetsampler>() };
                    lines.push(format!(
                        "render {cmd_type} sampler=#{} index={}",
                        c.sampler, c.index
                    ));
                }
                WMT_RENDER_COMMAND_SET_PSO => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_SET_PSO`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetpso>() };
                    lines.push(format!("render 15 pso=#{}", c.pso));
                }
                WMT_RENDER_COMMAND_SET_DSSO => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_SET_DSSO`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetdsso>() };
                    lines.push(format!(
                        "render 16 dsso=#{} stencil_ref={}",
                        c.dsso, c.stencil_ref
                    ));
                }
                WMT_RENDER_COMMAND_SET_RASTERIZER_STATE => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_SET_RASTERIZER_STATE`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetrasterizerstate>() };
                    lines.push(format!(
                        "render 12 fill_mode={} cull_mode={} depth_clip_mode={} winding={} depth_bias={} scole_scale={} depth_bias_clamp={}",
                        c.fill_mode,
                        c.cull_mode,
                        c.depth_clip_mode,
                        c.winding,
                        f32_hex(c.depth_bias),
                        f32_hex(c.scole_scale),
                        f32_hex(c.depth_bias_clamp),
                    ));
                }
                WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetblendcolor>() };
                    lines.push(format!(
                        "render 17 red={} green={} blue={} alpha={} stencil_ref={}",
                        f32_hex(c.red),
                        f32_hex(c.green),
                        f32_hex(c.blue),
                        f32_hex(c.alpha),
                        c.stencil_ref,
                    ));
                }
                WMT_RENDER_COMMAND_SET_VIEWPORTS => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_SET_VIEWPORTS`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetviewports>() };
                    let count = usize::from(c.viewport_count);
                    let bytes = count * size_of::<WMTViewport>();
                    let vp_ptr = ptr::with_exposed_provenance::<WMTViewport>(
                        usize::try_from(c.viewports.ptr).expect("pointer width"),
                    );
                    let mut s = format!("render 13 viewports={bytes}B viewport_count={count}");
                    for i in 0..count {
                        // SAFETY: `i < count` is within the allocated viewport array.
                        let vp_elem = unsafe { vp_ptr.add(i) };
                        // SAFETY: `vp_elem` points to an initialized `WMTViewport`.
                        let vp = unsafe { &*vp_elem };
                        let _ = write!(
                            s,
                            " viewports[{i}].origin_x={} viewports[{i}].origin_y={} viewports[{i}].width={} viewports[{i}].height={} viewports[{i}].znear={} viewports[{i}].zfar={}",
                            f64_as_f32_hex(vp.origin_x),
                            f64_as_f32_hex(vp.origin_y),
                            f64_as_f32_hex(vp.width),
                            f64_as_f32_hex(vp.height),
                            f64_as_f32_hex(vp.znear),
                            f64_as_f32_hex(vp.zfar),
                        );
                    }
                    lines.push(s);
                }
                WMT_RENDER_COMMAND_SET_SCISSOR_RECTS => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_SET_SCISSOR_RECTS`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetscissorrects>() };
                    let count = usize::from(c.rect_count);
                    let bytes = count * size_of::<WMTScissorRect>();
                    let sc_ptr = ptr::with_exposed_provenance::<WMTScissorRect>(
                        usize::try_from(c.scissor_rects.ptr).expect("pointer width"),
                    );
                    let mut s = format!("render 14 scissor_rects={bytes}B rect_count={count}");
                    for i in 0..count {
                        // SAFETY: `i < count` is within the allocated scissor rect array.
                        let sc_elem = unsafe { sc_ptr.add(i) };
                        // SAFETY: `sc_elem` points to an initialized `WMTScissorRect`.
                        let sc = unsafe { &*sc_elem };
                        let _ = write!(
                            s,
                            " scissor_rects[{i}].x={} scissor_rects[{i}].y={} scissor_rects[{i}].width={} scissor_rects[{i}].height={}",
                            sc.x, sc.y, sc.width, sc.height,
                        );
                    }
                    lines.push(s);
                }
                WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET
                | WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET
                | WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET
                | WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET => {
                    // SAFETY: `cmd_type` verified as a buffer-offset command.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetbufferoffset>() };
                    lines.push(format!(
                        "render {cmd_type} offset={} index={}",
                        c.offset, c.index
                    ));
                }
                WMT_RENDER_COMMAND_DRAW => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_DRAW`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderDraw>() };
                    lines.push(format!(
                        "render 19 primitive_type={} vertex_start={} vertex_count={} instance_count={} base_instance={}",
                        c.primitive_type,
                        c.vertex_start,
                        c.vertex_count,
                        c.instance_count,
                        c.base_instance,
                    ));
                }
                WMT_RENDER_COMMAND_DRAW_INDEXED => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_DRAW_INDEXED`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderDrawIndexed>() };
                    lines.push(format!(
                        "render 20 primitive_type={} index_type={} index_count={} index_buffer=#{} index_buffer_offset={} instance_count={} base_vertex={} base_instance={}",
                        c.primitive_type,
                        c.index_type,
                        c.index_count,
                        c.index_buffer,
                        c.index_buffer_offset,
                        c.instance_count,
                        c.base_vertex,
                        c.base_instance,
                    ));
                }
                WMT_RENDER_COMMAND_DRAW_INDIRECT => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_DRAW_INDIRECT`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderDrawIndirect>() };
                    lines.push(format!(
                        "render 21 primitive_type={} indirect_args_buffer=#{} indirect_args_offset={}",
                        c.primitive_type, c.indirect_args_buffer, c.indirect_args_offset,
                    ));
                }
                WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderDrawIndexedIndirect>() };
                    lines.push(format!(
                        "render 22 primitive_type={} index_type={} index_buffer=#{} index_buffer_offset={} indirect_args_buffer=#{} indirect_args_offset={}",
                        c.primitive_type,
                        c.index_type,
                        c.index_buffer,
                        c.index_buffer_offset,
                        c.indirect_args_buffer,
                        c.indirect_args_offset,
                    ));
                }
                WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderDxmtGeometryDraw>() };
                    lines.push(format!(
                        "render 29 draw_arguments_offset={} warp_count={} instance_count={} vertex_per_warp={}",
                        c.draw_arguments_offset,
                        c.warp_count,
                        c.instance_count,
                        c.vertex_per_warp,
                    ));
                }
                WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderDxmtGeometryDrawIndexed>() };
                    lines.push(format!(
                        "render 30 draw_arguments_offset={} index_buffer=#{} index_buffer_offset={} warp_count={} instance_count={} vertex_per_warp={}",
                        c.draw_arguments_offset,
                        c.index_buffer,
                        c.index_buffer_offset,
                        c.warp_count,
                        c.instance_count,
                        c.vertex_per_warp,
                    ));
                }
                WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderDxmtGeometryDrawIndirect>() };
                    lines.push(format!(
                        "render 31 imm_draw_arguments=#{} indirect_args_buffer=#{} indirect_args_offset={} dispatch_args_buffer=#{} dispatch_args_offset={} vertex_per_warp={}",
                        c.imm_draw_arguments,
                        c.indirect_args_buffer,
                        c.indirect_args_offset,
                        c.dispatch_args_buffer,
                        c.dispatch_args_offset,
                        c.vertex_per_warp,
                    ));
                }
                WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT`.
                    let c =
                        unsafe { &*base_ptr.cast::<WmtcmdRenderDxmtGeometryDrawIndexedIndirect>() };
                    lines.push(format!(
                        "render 32 index_buffer=#{} index_buffer_offset={} imm_draw_arguments=#{} indirect_args_buffer=#{} indirect_args_offset={} dispatch_args_buffer=#{} dispatch_args_offset={} vertex_per_warp={}",
                        c.index_buffer,
                        c.index_buffer_offset,
                        c.imm_draw_arguments,
                        c.indirect_args_buffer,
                        c.indirect_args_offset,
                        c.dispatch_args_buffer,
                        c.dispatch_args_offset,
                        c.vertex_per_warp,
                    ));
                }
                WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdRenderDxmtTessellationMeshDraw>() };
                    lines.push(format!(
                        "render 37 draw_arguments_offset={} instance_count={} threads_per_patch={} patch_per_group={} patch_per_mesh_instance={}",
                        c.draw_arguments_offset,
                        c.instance_count,
                        c.threads_per_patch,
                        c.patch_per_group,
                        c.patch_per_mesh_instance,
                    ));
                }
                WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED => {
                    // SAFETY: `cmd_type` verified as `WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED`.
                    let c =
                        unsafe { &*base_ptr.cast::<WmtcmdRenderDxmtTessellationMeshDrawIndexed>() };
                    lines.push(format!(
                        "render 38 draw_arguments_offset={} index_buffer=#{} index_buffer_offset={} instance_count={} threads_per_patch={} patch_per_group={} patch_per_mesh_instance={}",
                        c.draw_arguments_offset,
                        c.index_buffer,
                        c.index_buffer_offset,
                        c.instance_count,
                        c.threads_per_patch,
                        c.patch_per_group,
                        c.patch_per_mesh_instance,
                    ));
                }
                _ => lines.push(format!("render {cmd_type}")),
            },
            Records::Compute => match cmd_type {
                WMT_COMPUTE_COMMAND_NOP => {}
                WMT_COMPUTE_COMMAND_SET_PSO => {
                    // SAFETY: `cmd_type` verified as `WMT_COMPUTE_COMMAND_SET_PSO`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdComputeSetpso>() };
                    lines.push(format!(
                        "compute 3 pso=#{} threadgroup_size.width={} threadgroup_size.height={} threadgroup_size.depth={}",
                        c.pso,
                        c.threadgroup_size.width,
                        c.threadgroup_size.height,
                        c.threadgroup_size.depth,
                    ));
                }
                WMT_COMPUTE_COMMAND_DISPATCH => {
                    // SAFETY: `cmd_type` verified as `WMT_COMPUTE_COMMAND_DISPATCH`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdComputeDispatch>() };
                    lines.push(format!(
                        "compute 1 size.width={} size.height={} size.depth={}",
                        c.size.width, c.size.height, c.size.depth,
                    ));
                }
                WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT => {
                    // SAFETY: `cmd_type` verified as `WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT`.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdComputeDispatchIndirect>() };
                    lines.push(format!(
                        "compute 2 indirect_args_buffer=#{} indirect_args_offset={}",
                        c.indirect_args_buffer, c.indirect_args_offset,
                    ));
                }
                _ => lines.push(format!("compute {cmd_type}")),
            },
            Records::Blit => match cmd_type {
                WMT_BLIT_COMMAND_NOP => {}
                WMT_BLIT_COMMAND_WAIT_FOR_FENCE | WMT_BLIT_COMMAND_UPDATE_FENCE => {
                    // SAFETY: `cmd_type` verified as a fence command.
                    let c = unsafe { &*base_ptr.cast::<WmtcmdBlitFenceOp>() };
                    lines.push(format!("blit {cmd_type} fence=#{}", c.fence));
                }
                _ => lines.push(format!("blit {cmd_type}")),
            },
        }
        cur_addr = base.next.ptr;
    }
    lines
}
