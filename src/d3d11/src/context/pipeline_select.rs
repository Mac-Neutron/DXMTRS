//! Pipeline state finalization and pre-draw/pre-dispatch state emission (`d3d11_context_impl.cpp`).
//!
//! Validates graphics, geometry-shader mesh, tessellation mesh, and compute pipelines before pass
//! startup (`FinalizeRenderPipeline`, `FinalizeComputePipeline`) and emits dirty render and
//! compute state records before draw and dispatch commands (`PreDraw`, `PreDispatch`).

use dxmt_core::encoder::{NO_NEXT, kind, stage};
use winemetal_sys::wire::{
    WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF,
    WMT_RENDER_COMMAND_SET_DSSO, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_RASTERIZER_STATE, WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
    WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_VIEWPORTS,
    WmtcmdComputeSetbufferoffset, WmtcmdRenderSetblendcolor, WmtcmdRenderSetbufferoffset,
    WmtcmdRenderSetdsso, WmtcmdRenderSetrasterizerstate, WmtcmdRenderSetscissorrects,
    WmtcmdRenderSetviewports,
};

use super::{CommandBufferState, Context, Mode, encode::slice_mem_ptr, state::DirtyFlags};

impl<M: Mode> Context<M> {
    /// Selects and binds the active graphics pipeline (`FinalizeRenderPipeline`).
    #[must_use]
    pub fn finalize_graphics_pipeline(&mut self) -> bool {
        if self.cmdbuf_state == CommandBufferState::RenderPipelineReady {
            return true;
        }
        let snap = self.encode_snapshot();
        if !snap.render_pipeline_valid || snap.render_pso == 0 || self.state.topology == 0 {
            self.cmdbuf_state = CommandBufferState::Idle;
            return false;
        }
        if snap.pipeline_kind == kind::TESSELLATION && !(33..=64).contains(&self.state.topology) {
            self.cmdbuf_state = CommandBufferState::Idle;
            return false;
        }
        if snap.pipeline_kind == kind::GEOMETRY && snap.gs_vertex_per_warp == 0 {
            self.cmdbuf_state = CommandBufferState::Idle;
            return false;
        }
        self.cmdbuf_state = CommandBufferState::RenderPipelineReady;
        true
    }

    /// Selects and binds the active compute pipeline (`FinalizeComputePipeline`).
    #[must_use]
    pub fn finalize_compute_pipeline(&mut self) -> bool {
        if self.cmdbuf_state == CommandBufferState::ComputePipelineReady {
            return true;
        }
        let snap = self.encode_snapshot();
        if !snap.compute_pipeline_valid || snap.compute_pso == 0 {
            self.cmdbuf_state = CommandBufferState::Idle;
            return false;
        }
        self.cmdbuf_state = CommandBufferState::ComputePipelineReady;
        true
    }

    /// Prepares the render encoder and dirty state before a non-indexed draw (`PreDraw`).
    #[must_use]
    pub fn pre_draw(&mut self) -> bool {
        self.pre_draw_inner(false)
    }

    /// Prepares the render encoder and dirty state before an indexed draw (`PreDraw<..., true>`).
    #[must_use]
    pub fn pre_draw_indexed(&mut self) -> bool {
        self.pre_draw_inner(true)
    }

    fn pre_draw_inner(&mut self, indexed: bool) -> bool {
        if matches!(
            self.cmdbuf_state,
            CommandBufferState::Idle | CommandBufferState::ComputePipelineReady
        ) {
            self.cmdbuf_state = CommandBufferState::Idle;
            if !self.finalize_graphics_pipeline() {
                return false;
            }
        }
        if self.cmdbuf_state != CommandBufferState::RenderEncoderActive {
            self.switch_to_render_encoder();
        }
        if self.cmdbuf_state != CommandBufferState::RenderEncoderActive {
            return false;
        }

        let snap = self.encode_snapshot();
        if snap.viewports.is_empty() {
            return false;
        }
        if indexed && snap.index_buffer == 0 {
            return false;
        }

        let stencil_ref = u8::try_from(self.state.stencil_ref & 0xff).unwrap_or(0);

        if self.dirty.contains(DirtyFlags::DEPTH_STENCIL_STATE) {
            // Delegated state closure #1 (`SetupDepthStencilState`, d3d11_context_impl.cpp:2973):
            // emits `WmtcmdRenderSetdsso` (`render 16`).
            let cmd = WmtcmdRenderSetdsso {
                r#type: WMT_RENDER_COMMAND_SET_DSSO,
                reserved: [0; 3],
                next: NO_NEXT,
                dsso: snap.dsso,
                stencil_ref,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        }

        if self.dirty.contains(DirtyFlags::RASTERIZER_STATE) {
            // Delegated state closure #2 (`SetupRasterizerState`, d3d11_context_impl.cpp:2981):
            // emits `WmtcmdRenderSetrasterizerstate` (`render 12`).
            let rs = &snap.rasterizer;
            let cmd = WmtcmdRenderSetrasterizerstate {
                r#type: WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
                reserved: [0; 3],
                next: NO_NEXT,
                fill_mode: rs.fill_mode,
                cull_mode: rs.cull_mode,
                depth_clip_mode: rs.depth_clip_mode,
                winding: rs.winding,
                depth_bias: rs.depth_bias,
                scole_scale: rs.slope_scale,
                depth_bias_clamp: rs.depth_bias_clamp,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        }

        if self
            .dirty
            .contains(DirtyFlags::BLEND_FACTOR_AND_SAMPLE_MASK)
        {
            // C++ closure #3 (`PreDraw` blend color, d3d11_context_impl.cpp:2989):
            // emits `WmtcmdRenderSetblendcolor` (`render 17`).
            let bf = self.state.blend_factor;
            let cmd = WmtcmdRenderSetblendcolor {
                r#type: WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF,
                reserved: [0; 3],
                next: NO_NEXT,
                red: bf[0],
                green: bf[1],
                blue: bf[2],
                alpha: bf[3],
                stencil_ref,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        }

        if self.dirty.contains(DirtyFlags::VIEWPORTS) {
            // C++ closure #4 (`PreDraw` viewports, d3d11_context_impl.cpp:3006):
            // copies viewports into chunk memory and emits `WmtcmdRenderSetviewports` (`render 13`).
            let vp_ptr = self.allocate_chunk_slice(&snap.viewports);
            let viewport_count = u8::try_from(snap.viewports.len()).unwrap_or(0);
            let cmd = WmtcmdRenderSetviewports {
                r#type: WMT_RENDER_COMMAND_SET_VIEWPORTS,
                reserved: [0; 3],
                next: NO_NEXT,
                viewports: slice_mem_ptr(vp_ptr),
                viewport_count,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        }

        if self.dirty.contains(DirtyFlags::SCISSOR_RECTS) {
            // C++ closure #5 (`PreDraw` scissor rects, d3d11_context_impl.cpp:3022):
            // copies scissor rects into chunk memory and emits `WmtcmdRenderSetscissorrects` (`render 14`).
            let sc_ptr = self.allocate_chunk_slice(&snap.scissor_rects);
            let rect_count = u8::try_from(snap.scissor_rects.len()).unwrap_or(0);
            let cmd = WmtcmdRenderSetscissorrects {
                r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
                reserved: [0; 3],
                next: NO_NEXT,
                scissor_rects: slice_mem_ptr(sc_ptr),
                rect_count,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        }

        if self.dirty.contains(DirtyFlags::INDEX_BUFFER) {
            let sub_offset = snap.index_sub_offset;
            self.emit_encoder(move |enc| {
                enc.bind_index_buffer_offset(sub_offset);
            });
        }

        if self.dirty.contains(DirtyFlags::VERTEX_BUFFERS) {
            let vb_mask = snap.stage_dirty_masks[usize::from(stage::VERTEX)];
            if vb_mask != 0 {
                self.emit_encoder(move |enc| {
                    enc.encode_vertex_buffers::<{ kind::ORDINARY }>(vb_mask, 0);
                });
            }
        }

        // C++ closure #6 (`PreDraw` vertex stage resources, d3d11_context_impl.cpp:3106).
        if let Some((offset, index)) = snap.gs_object_buffer_offset
            && snap.pipeline_kind == kind::GEOMETRY
        {
            let cmd = WmtcmdRenderSetbufferoffset {
                r#type: WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET,
                reserved: [0; 3],
                next: NO_NEXT,
                offset,
                index,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        } else if snap.stage_dirty_masks[usize::from(stage::VERTEX)] != 0 {
            let cmd = WmtcmdRenderSetbufferoffset {
                r#type: WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
                reserved: [0; 3],
                next: NO_NEXT,
                offset: 0,
                index: 2,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        }

        // C++ closure #7 (`PreDraw` pixel stage resources, d3d11_context_impl.cpp:3120).
        if let Some((offset, index)) = snap.gs_mesh_buffer_offset
            && snap.pipeline_kind == kind::GEOMETRY
        {
            let cmd = WmtcmdRenderSetbufferoffset {
                r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
                reserved: [0; 3],
                next: NO_NEXT,
                offset,
                index,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        } else if snap.stage_dirty_masks[usize::from(stage::PIXEL)] != 0 {
            let cmd = WmtcmdRenderSetbufferoffset {
                r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
                reserved: [0; 3],
                next: NO_NEXT,
                offset: 0,
                index: 2,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        }

        // C++ closure #8 (`PreDraw` hull + domain stage resources, d3d11_context_impl.cpp:3133).
        if snap.pipeline_kind == kind::TESSELLATION
            && (snap.stage_dirty_masks[usize::from(stage::HULL)] != 0
                || snap.stage_dirty_masks[usize::from(stage::DOMAIN)] != 0)
        {
            let cmd = WmtcmdRenderSetbufferoffset {
                r#type: WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET,
                reserved: [0; 3],
                next: NO_NEXT,
                offset: 0,
                index: 2,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        }

        // C++ closure #9 (`PreDraw` geometry stage resources, d3d11_context_impl.cpp:3148).
        if snap.pipeline_kind == kind::GEOMETRY
            && snap.stage_dirty_masks[usize::from(stage::GEOMETRY)] != 0
        {
            let cmd = WmtcmdRenderSetbufferoffset {
                r#type: WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET,
                reserved: [0; 3],
                next: NO_NEXT,
                offset: 0,
                index: 2,
            };
            self.emit_render_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        }

        self.dirty = DirtyFlags::empty();
        true
    }

    /// Prepares the compute encoder and dirty state before a dispatch (`PreDispatch`).
    #[must_use]
    pub fn pre_dispatch(&mut self) -> bool {
        if matches!(
            self.cmdbuf_state,
            CommandBufferState::Idle | CommandBufferState::RenderPipelineReady
        ) {
            self.cmdbuf_state = CommandBufferState::Idle;
            if !self.finalize_compute_pipeline() {
                return false;
            }
        }
        if self.cmdbuf_state != CommandBufferState::ComputeEncoderActive {
            self.switch_to_compute_encoder();
        }
        if self.cmdbuf_state != CommandBufferState::ComputeEncoderActive {
            return false;
        }

        let snap = self.encode_snapshot();
        // C++ closure #10 (`PreDispatch` compute stage resources, d3d11_context_impl.cpp:3172).
        if snap.stage_dirty_masks[usize::from(stage::COMPUTE)] != 0 {
            let cmd = WmtcmdComputeSetbufferoffset {
                r#type: WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET,
                reserved: [0; 3],
                next: NO_NEXT,
                offset: 0,
                index: 1,
            };
            self.emit_compute_cmd(cmd, move |enc| {
                enc.encode_compute_command(cmd);
            });
        }
        true
    }
}
