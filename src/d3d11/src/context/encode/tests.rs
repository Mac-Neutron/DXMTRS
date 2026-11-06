use core::{fmt::Write, ptr};

use dxmt_core::encoder::kind;
use winemetal_sys::wire::{
    WMT_COMPUTE_COMMAND_DISPATCH, WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT,
    WMT_COMPUTE_COMMAND_SET_PSO, WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT, WMT_RENDER_COMMAND_DRAW_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW, WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
    WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT, WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
    WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED,
    WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF, WMT_RENDER_COMMAND_SET_DSSO,
    WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_PSO,
    WMT_RENDER_COMMAND_SET_RASTERIZER_STATE, WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
    WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_VIEWPORTS, WMTScissorRect,
    WMTSize, WMTViewport, WmtcmdBase, WmtcmdComputeDispatch, WmtcmdComputeDispatchIndirect,
    WmtcmdComputeSetpso, WmtcmdRenderDraw, WmtcmdRenderDrawIndexed,
    WmtcmdRenderDrawIndexedIndirect, WmtcmdRenderDrawIndirect, WmtcmdRenderDxmtGeometryDraw,
    WmtcmdRenderDxmtGeometryDrawIndexed, WmtcmdRenderDxmtGeometryDrawIndexedIndirect,
    WmtcmdRenderDxmtGeometryDrawIndirect, WmtcmdRenderDxmtTessellationMeshDraw,
    WmtcmdRenderDxmtTessellationMeshDrawIndexed, WmtcmdRenderSetblendcolor,
    WmtcmdRenderSetbufferoffset, WmtcmdRenderSetdsso, WmtcmdRenderSetpso,
    WmtcmdRenderSetrasterizerstate, WmtcmdRenderSetscissorrects, WmtcmdRenderSetviewports,
};

use super::{CommandBufferState, RasterizerStateCmd, RecordedPassKind};
use crate::{
    context::{
        Context, Immediate,
        state::{ContextState, DirtyFlags},
    },
    state::Defaults,
};

const GOLDEN_DRAW_R16: &str = include_str!("../golden/wmtcmd/draw_r16.txt");
const GOLDEN_DRAW_R32: &str = include_str!("../golden/wmtcmd/draw_r32.txt");
const GOLDEN_DRAW_INDIRECT: &str = include_str!("../golden/wmtcmd/draw_indirect.txt");
const GOLDEN_DISPATCH_INDIRECT: &str = include_str!("../golden/wmtcmd/dispatch_indirect.txt");
const GOLDEN_GS: &str = include_str!("../golden/wmtcmd/gs.txt");
const GOLDEN_STATE_CMDS: &str = include_str!("../golden/wmtcmd/state_cmds.txt");

const D3D11_PRIMITIVE_TOPOLOGY_POINTLIST: u32 = 1;
const D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST: u32 = 4;
const D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP: u32 = 5;
const DXGI_FORMAT_R32_UINT: u32 = 42;
const DXGI_FORMAT_R16_UINT: u32 = 57;

fn new_test_context() -> Context<Immediate> {
    Context {
        vtbl: ptr::null(),
        ext_vtbl: ptr::null(),
        annotation_vtbl: ptr::null(),
        refs: crate::com::Managed::default(),
        private_data: dxmt_util::private_data::PrivateData::default(),
        device: ptr::null_mut(),
        state: ContextState {
            sample_mask: 0xffff_ffff,
            stencil_ref: 0,
            blend_factor: [0.0; 4],
            topology: D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            ..ContextState::default()
        },
        cmdbuf_state: CommandBufferState::Idle,
        encode: crate::context::encode::EncodeSession::default(),
        dirty: DirtyFlags::empty(),
        defaults: Defaults::default(),
        mode: Immediate::default(),
    }
}

fn default_viewport_8x4() -> WMTViewport {
    WMTViewport {
        origin_x: 0.0,
        origin_y: 0.0,
        width: 8.0,
        height: 4.0,
        znear: 0.0,
        zfar: 1.0,
    }
}

fn default_scissor_8x4() -> WMTScissorRect {
    WMTScissorRect {
        x: 0,
        y: 0,
        width: 8,
        height: 4,
    }
}

fn setup_default_render_state(ctx: &mut Context<Immediate>, pso: u64) {
    ctx.set_render_pipeline_ready(pso, kind::ORDINARY);
    ctx.set_encode_depth_stencil_state(71);
    ctx.set_encode_rasterizer_state(RasterizerStateCmd::default());
    ctx.set_encode_viewports(vec![default_viewport_8x4()]);
    ctx.set_encode_scissor_rects(vec![default_scissor_8x4()]);
}

fn f32_hex(val: f32) -> String {
    format!("0x{:08x}", val.to_bits())
}

fn f64_as_f32_hex(val: f64) -> String {
    #[expect(clippy::cast_possible_truncation, reason = "matches winemetal dump")]
    let narrow = val as f32;
    f32_hex(narrow)
}

/// Formats a linked list of `WmtcmdBase` records into `DXMT_WMTCMD_DUMP` lines.
fn format_pass_records(pass_kind: &RecordedPassKind, head: *const WmtcmdBase) -> Vec<String> {
    let mut lines = Vec::new();
    if head.is_null() {
        return lines;
    }
    // SAFETY: `head` is the sentinel node of a pass allocated in the context's arena.
    let mut cur_addr = unsafe { (*head).next.ptr };
    while cur_addr != 0 {
        let base_ptr = ptr::with_exposed_provenance::<WmtcmdBase>(
            usize::try_from(cur_addr).expect("pointer width"),
        );
        // SAFETY: `base_ptr` points to a valid record in the pass's arena.
        let base = unsafe { &*base_ptr };
        let cmd_type = base.r#type;
        match pass_kind {
            RecordedPassKind::Render => match cmd_type {
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
                _ => {}
            },
            RecordedPassKind::Compute => match cmd_type {
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
                _ => {}
            },
            RecordedPassKind::Blit => {}
        }
        cur_addr = base.next.ptr;
    }
    lines
}

fn collect_all_lines(ctx: &Context<Immediate>) -> Vec<String> {
    let mut out = Vec::new();
    for (kind, head) in ctx.recorded_passes() {
        out.extend(format_pass_records(&kind, head));
    }
    out
}

#[test]
fn state_machine_transitions_as_cpp() {
    let mut ctx = new_test_context();
    setup_default_render_state(&mut ctx, 68);
    ctx.set_compute_pipeline_ready(
        140,
        WMTSize {
            width: 64,
            height: 1,
            depth: 1,
        },
    );

    assert_eq!(ctx.cmdbuf_state, CommandBufferState::Idle);
    ctx.invalidate_current_pass(false);
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::Idle);

    // FinalizeGraphicsPipeline transitions Idle -> RenderPipelineReady.
    assert!(ctx.finalize_graphics_pipeline());
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::RenderPipelineReady);
    ctx.invalidate_current_pass(false);
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::RenderPipelineReady);

    // SwitchToRenderEncoder transitions RenderPipelineReady -> RenderEncoderActive and marks all dirty.
    ctx.switch_to_render_encoder();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::RenderEncoderActive);
    assert!(ctx.dirty == DirtyFlags::all());

    // InvalidateCurrentPass on RenderEncoderActive returns to RenderPipelineReady (not Idle!).
    ctx.invalidate_current_pass(false);
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::RenderPipelineReady);

    // SwitchToComputeEncoder from RenderPipelineReady finalizes compute and enters ComputeEncoderActive.
    ctx.switch_to_compute_encoder();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::ComputeEncoderActive);

    // InvalidateCurrentPass on ComputeEncoderActive returns to ComputePipelineReady (not Idle!).
    ctx.invalidate_current_pass(false);
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::ComputePipelineReady);

    // SwitchToBlitEncoder from ComputePipelineReady enters BlitEncoderActive.
    ctx.switch_to_blit_encoder();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::BlitEncoderActive);

    // InvalidateCurrentPass on BlitEncoderActive returns to Idle.
    ctx.invalidate_current_pass(false);
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::Idle);

    // SwitchToRenderEncoder directly from RenderEncoderActive -> SwitchToComputeEncoder invalidates render pass first.
    ctx.switch_to_render_encoder();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::RenderEncoderActive);
    ctx.switch_to_compute_encoder();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::ComputeEncoderActive);
    ctx.switch_to_blit_encoder();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::BlitEncoderActive);
    ctx.switch_to_render_encoder();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::RenderEncoderActive);
    ctx.invalidate_current_pass(false);
}

#[test]
fn draw_without_pipeline_skips() {
    let mut ctx = new_test_context();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::Idle);

    // Without a valid render or compute pipeline, pre_draw and pre_dispatch return false,
    // and Draw*/Dispatch* emit no commands and leave cmdbuf_state Idle.
    assert!(!ctx.pre_draw());
    assert!(!ctx.pre_dispatch());
    ctx.switch_to_render_encoder();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::Idle);
    ctx.switch_to_compute_encoder();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::Idle);

    ctx.draw(3, 0);
    ctx.draw_indexed(3, 0, 0);
    ctx.draw_instanced(3, 2, 0, 0);
    ctx.draw_indexed_instanced(3, 2, 0, 0, 0);
    let indirect_buf = ptr::without_provenance_mut(109);
    ctx.draw_instanced_indirect(indirect_buf, 16);
    ctx.draw_indexed_instanced_indirect(indirect_buf, 32);
    ctx.dispatch(1, 1, 1);
    ctx.dispatch_indirect(indirect_buf, 52);

    assert_eq!(ctx.cmdbuf_state, CommandBufferState::Idle);
    assert_eq!(ctx.recorded_passes().len(), 0);
}

#[test]
fn dirty_state_emits_once() {
    let mut ctx = new_test_context();
    setup_default_render_state(&mut ctx, 68);
    ctx.set_encode_index_buffer(72, DXGI_FORMAT_R16_UINT, 4);

    // First draw opens the render pass, emits PSO + 5 dirty state commands + draw_indexed,
    // and clears DirtyFlags.
    ctx.draw_indexed(3, 2, -3);
    assert!(ctx.dirty.is_empty());

    // Second draw in the same pass with no state changes must NOT re-emit the 5 state commands.
    ctx.draw_indexed(3, 2, -3);
    ctx.invalidate_current_pass(false);

    let lines = collect_all_lines(&ctx);
    let dsso_count = lines.iter().filter(|l| l.starts_with("render 16 ")).count();
    let rs_count = lines.iter().filter(|l| l.starts_with("render 12 ")).count();
    let blend_count = lines.iter().filter(|l| l.starts_with("render 17 ")).count();
    let vp_count = lines.iter().filter(|l| l.starts_with("render 13 ")).count();
    let sc_count = lines.iter().filter(|l| l.starts_with("render 14 ")).count();
    let draw_count = lines.iter().filter(|l| l.starts_with("render 20 ")).count();

    assert_eq!(dsso_count, 1);
    assert_eq!(rs_count, 1);
    assert_eq!(blend_count, 1);
    assert_eq!(vp_count, 1);
    assert_eq!(sc_count, 1);
    assert_eq!(draw_count, 2);
}

#[test]
fn promote_flush_commits() {
    let mut ctx = new_test_context();
    setup_default_render_state(&mut ctx, 68);

    ctx.switch_to_render_encoder();
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::RenderEncoderActive);
    assert_eq!(ctx.promote_flush_count(), 0);

    // invalidate_current_pass(false) ends the pass without promoting a flush.
    ctx.invalidate_current_pass(false);
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::RenderPipelineReady);
    assert_eq!(ctx.promote_flush_count(), 0);

    // invalidate_current_pass(true) ends the active pass and commits/promotes flush.
    ctx.switch_to_render_encoder();
    ctx.invalidate_current_pass(true);
    assert_eq!(ctx.cmdbuf_state, CommandBufferState::RenderPipelineReady);
    assert_eq!(ctx.promote_flush_count(), 1);

    // Calling invalidate_current_pass(true) when no pass is open also promotes flush.
    ctx.invalidate_current_pass(true);
    assert_eq!(ctx.promote_flush_count(), 2);
}

#[test]
fn records_as_the_dump() {
    let mut ctx = new_test_context();

    // 1. State commands + R16 indexed draws (`state_cmds.txt` and `draw_r16.txt`).
    setup_default_render_state(&mut ctx, 68);
    ctx.state.topology = D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
    ctx.set_encode_index_buffer(72, DXGI_FORMAT_R16_UINT, 4);
    ctx.draw_indexed(3, 2, -3);
    ctx.invalidate_current_pass(false);

    let pass0_lines = collect_all_lines(&ctx);
    let state_lines: String = pass0_lines
        .iter()
        .filter(|l| {
            l.starts_with("render 16 ")
                || l.starts_with("render 12 ")
                || l.starts_with("render 17 ")
                || l.starts_with("render 13 ")
                || l.starts_with("render 14 ")
        })
        .fold(String::new(), |mut acc, l| {
            acc.push_str(l);
            acc.push('\n');
            acc
        });
    assert_eq!(state_lines, GOLDEN_STATE_CMDS);

    ctx.set_encode_index_buffer(94, DXGI_FORMAT_R16_UINT, 0);
    ctx.draw_indexed_instanced(6, 2, 0, 0, 3);
    ctx.invalidate_current_pass(false);

    ctx.state.topology = D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP;
    ctx.set_encode_index_buffer(104, DXGI_FORMAT_R16_UINT, 0);
    ctx.draw_indexed(7, 0, 0);
    ctx.invalidate_current_pass(false);

    let r16_lines: String = collect_all_lines(&ctx)
        .into_iter()
        .filter(|l| l.starts_with("render 20 "))
        .fold(String::new(), |mut acc, l| {
            acc.push_str(&l);
            acc.push('\n');
            acc
        });
    assert_eq!(r16_lines, GOLDEN_DRAW_R16);

    // 2. R32 indexed draws (`draw_r32.txt`).
    ctx.clear_recorded_passes();
    ctx.cmdbuf_state = CommandBufferState::Idle;
    setup_default_render_state(&mut ctx, 83);
    ctx.state.topology = D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
    ctx.set_encode_index_buffer(84, DXGI_FORMAT_R32_UINT, 8);
    ctx.draw_indexed(3, 2, -3);
    ctx.invalidate_current_pass(false);

    ctx.state.topology = D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP;
    ctx.set_encode_index_buffer(114, DXGI_FORMAT_R32_UINT, 0);
    ctx.draw_indexed(7, 0, 0);
    ctx.invalidate_current_pass(false);

    let r32_lines: String = collect_all_lines(&ctx)
        .into_iter()
        .filter(|l| l.starts_with("render 20 "))
        .fold(String::new(), |mut acc, l| {
            acc.push_str(&l);
            acc.push('\n');
            acc
        });
    assert_eq!(r32_lines, GOLDEN_DRAW_R32);

    // 3. Indirect draws (`draw_indirect.txt`).
    ctx.clear_recorded_passes();
    ctx.cmdbuf_state = CommandBufferState::Idle;
    setup_default_render_state(&mut ctx, 123);
    ctx.state.topology = D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
    let indirect_buf = ptr::without_provenance_mut(109);
    ctx.draw_instanced_indirect(indirect_buf, 16);
    ctx.invalidate_current_pass(false);

    ctx.cmdbuf_state = CommandBufferState::Idle;
    setup_default_render_state(&mut ctx, 68);
    ctx.set_encode_index_buffer(72, DXGI_FORMAT_R16_UINT, 0);
    ctx.draw_indexed_instanced_indirect(indirect_buf, 32);
    ctx.invalidate_current_pass(false);

    let indirect_lines: String = collect_all_lines(&ctx)
        .into_iter()
        .filter(|l| l.starts_with("render 21 ") || l.starts_with("render 22 "))
        .fold(String::new(), |mut acc, l| {
            acc.push_str(&l);
            acc.push('\n');
            acc
        });
    assert_eq!(indirect_lines, GOLDEN_DRAW_INDIRECT);

    // 4. Indirect compute dispatch (`dispatch_indirect.txt`).
    ctx.clear_recorded_passes();
    ctx.cmdbuf_state = CommandBufferState::Idle;
    ctx.set_compute_pipeline_ready(
        140,
        WMTSize {
            width: 64,
            height: 1,
            depth: 1,
        },
    );
    ctx.dispatch_indirect(indirect_buf, 52);
    ctx.invalidate_current_pass(false);

    let dispatch_lines: String = collect_all_lines(&ctx)
        .into_iter()
        .filter(|l| l.starts_with("compute 3 ") || l.starts_with("compute 2 "))
        .fold(String::new(), |mut acc, l| {
            acc.push_str(&l);
            acc.push('\n');
            acc
        });
    assert_eq!(dispatch_lines, GOLDEN_DISPATCH_INDIRECT);

    // 5. Geometry shader mesh draw (`gs.txt`).
    ctx.clear_recorded_passes();
    ctx.cmdbuf_state = CommandBufferState::Idle;
    setup_default_render_state(&mut ctx, 199);
    ctx.state.topology = D3D11_PRIMITIVE_TOPOLOGY_POINTLIST;
    ctx.set_gs_pipeline_config(199, 30, Some((960, 2)), Some((992, 0)), 1024);
    ctx.draw(6, 0);
    ctx.invalidate_current_pass(false);

    let gs_lines: String = collect_all_lines(&ctx)
        .into_iter()
        .filter(|l| {
            l.starts_with("render 15 ")
                || l.starts_with("render 9 ")
                || l.starts_with("render 5 ")
                || l.starts_with("render 29 ")
        })
        .fold(String::new(), |mut acc, l| {
            acc.push_str(&l);
            acc.push('\n');
            acc
        });
    assert_eq!(gs_lines, GOLDEN_GS);
}

#[test]
fn a_destroyed_deferred_context_takes_its_session_with_it() {
    let held = std::sync::Arc::new(());
    let mut ctx = Context::new_deferred(ptr::null_mut(), 0);
    assert_eq!(ctx.recorded_passes().len(), 0);
    assert_eq!(ctx.promote_flush_count(), 0);
    let in_closure = std::sync::Arc::clone(&held);
    ctx.emit_encoder(move |_| drop(in_closure));
    assert_eq!(std::sync::Arc::strong_count(&held), 2);
    drop(ctx);
    // The global session map kept the closure, and the arena under it, for the process's life.
    assert_eq!(std::sync::Arc::strong_count(&held), 1);
}

#[test]
fn an_emitted_fn_once_runs_once_however_often_its_commands_run() {
    let runs = std::sync::Arc::new(core::sync::atomic::AtomicU32::new(0));
    let mut ctx = new_test_context();
    let counter = std::sync::Arc::clone(&runs);
    ctx.emit_encoder(move |_| {
        counter.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    });
    let mut imm = Context::new_immediate(ptr::null_mut());
    ctx.encode.commands.run(imm.mode.encoder_mut());
    ctx.encode.commands.run(imm.mode.encoder_mut());
    assert_eq!(runs.load(core::sync::atomic::Ordering::Relaxed), 1);
    drop(ctx);
    // The closure (and the counter it held) went with its first run.
    assert_eq!(std::sync::Arc::strong_count(&runs), 1);
}
