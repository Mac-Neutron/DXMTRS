//! Pipeline state finalization and pre-draw/pre-dispatch state emission (`d3d11_context_impl.cpp`).
//!
//! Validates graphics, geometry-shader mesh, tessellation mesh, and compute pipelines before pass
//! startup (`FinalizeRenderPipeline`, `FinalizeComputePipeline`) and emits dirty render and
//! compute state records before draw and dispatch commands (`PreDraw`, `PreDispatch`).

use std::sync::Arc;

use dxmt_core::{
    encoder::{NO_NEXT, kind, stage},
    frame_stats::CompatibilityFlags,
    topology::{is_strip, topology_class},
};
use dxmt_util::dxgi_consts::DXGI_FORMAT_R32_UINT;
use winemetal_sys::{
    sm50::{
        MTL_TESSELLATOR_OUTPUT_LINE, MTL_TESSELLATOR_OUTPUT_POINT, SM50_INDEX_BUFFER_FORMAT_NONE,
        SM50_INDEX_BUFFER_FORMAT_UINT16, SM50_INDEX_BUFFER_FORMAT_UINT32,
    },
    wire::{
        WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET, WMT_COMPUTE_COMMAND_SET_PSO,
        WMT_PIXEL_FORMAT_INVALID, WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF,
        WMT_RENDER_COMMAND_SET_DSSO, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
        WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET,
        WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
        WMT_RENDER_COMMAND_SET_SCISSOR_RECTS, WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
        WMT_RENDER_COMMAND_SET_VIEWPORTS, WMTScissorRect, WMTSize, WMTViewport,
        WmtcmdComputeSetbufferoffset, WmtcmdComputeSetpso, WmtcmdRenderSetblendcolor,
        WmtcmdRenderSetbufferoffset, WmtcmdRenderSetdsso, WmtcmdRenderSetpso,
        WmtcmdRenderSetrasterizerstate, WmtcmdRenderSetscissorrects, WmtcmdRenderSetviewports,
    },
};

use super::{
    CommandBufferState, Context, Mode,
    encode::slice_mem_ptr,
    state::{DirtyFlags, RENDER_TARGET_SLOTS, STREAM_OUTPUT_SLOTS, ShaderStage},
};
use crate::{
    device::Device,
    input_layout::{InputLayout, StreamOutputLayout},
    pipeline::{GraphicsDesc, GraphicsPipelineKind},
    shader::{ManagedShader, Shader},
    state::{BlendState, DepthStencilState, RasterizerState},
    view::{Dsv, Rtv},
};

const D3D11_SO_NO_RASTERIZED_STREAM: u32 = 0xffff_ffff;

impl<M: Mode> Context<M> {
    fn managed_shader(&self, stage: ShaderStage) -> Option<Arc<ManagedShader>> {
        let ptr = self.state.shader_stages[stage.index()].shader.as_ptr();
        Shader::from_iface(ptr).map(|s| Arc::clone(&s.managed))
    }

    fn build_graphics_desc(&self, indexed: bool) -> GraphicsDesc {
        let vs = self.managed_shader(ShaderStage::Vertex);
        let ps = self.managed_shader(ShaderStage::Pixel);
        let hs = self.managed_shader(ShaderStage::Hull);
        let ds = self.managed_shader(ShaderStage::Domain);
        let gs = self.managed_shader(ShaderStage::Geometry);

        let gs_ptr = self.state.shader_stages[ShaderStage::Geometry.index()]
            .shader
            .as_ptr();
        let so_layout = StreamOutputLayout::from_iface(gs_ptr).map(core::ptr::from_ref);
        let input_layout =
            InputLayout::from_iface(self.state.input_assembler.input_layout.as_ptr())
                .map(core::ptr::from_ref);

        let num_rtvs = (self.state.output_merger.num_rtvs as usize).min(RENDER_TARGET_SLOTS);
        let mut color_formats = [WMT_PIXEL_FORMAT_INVALID; 8];
        for (i, slot) in color_formats.iter_mut().enumerate().take(num_rtvs) {
            if let Some(rtv) = Rtv::from_iface(self.state.output_merger.rtvs[i].as_ptr()) {
                *slot = rtv.pixel_format;
            }
        }
        let bs_ptr = if self.state.output_merger.blend_state.is_null() {
            self.defaults.blend
        } else {
            self.state.output_merger.blend_state.as_ptr()
        };
        let blend_state = BlendState::from_iface(bs_ptr).map(core::ptr::from_ref);

        let depth_stencil_format = Dsv::from_iface(self.state.output_merger.dsv.as_ptr())
            .map_or(WMT_PIXEL_FORMAT_INVALID, |dsv| dsv.format);

        let topology = self.state.input_assembler.topology;
        let topology_class = topology_class(topology);

        let ds_ptr = if self.state.output_merger.depth_stencil_state.is_null() {
            self.defaults.depth_stencil
        } else {
            self.state.output_merger.depth_stencil_state.as_ptr()
        };
        let ds_enabled =
            DepthStencilState::from_iface(ds_ptr).is_some_and(DepthStencilState::is_enabled);
        let so_allows_raster = so_layout.is_none_or(|so| {
            // SAFETY: `so` was obtained from `StreamOutputLayout::from_iface`.
            unsafe { (*so).rasterized_stream != D3D11_SO_NO_RASTERIZED_STREAM }
        });
        let rasterization_enabled = (ps.is_some() || ds_enabled) && so_allows_raster;
        let pixel_shader = if rasterization_enabled { ps } else { None };

        let gs_passthrough = gs.as_ref().map_or(u32::MAX, |g| {
            // SAFETY: `g` is a geometry shader so `u.geometry_shader` is initialized.
            let gs_refl = unsafe { g.reflection.u.geometry_shader };
            // SAFETY: `gs_pass_through` is the active union arm for geometry shader reflection.
            unsafe { gs_refl.u.gs_pass_through }
        });
        let (geometry_shader, gs_strip_topology) =
            if gs_passthrough == u32::MAX && gs.is_some() && so_layout.is_none() {
                (gs, is_strip(topology))
            } else {
                (None, false)
            };

        let index_buffer_format = if indexed {
            if self.state.input_assembler.index_buffer_format == DXGI_FORMAT_R32_UINT {
                SM50_INDEX_BUFFER_FORMAT_UINT32
            } else {
                SM50_INDEX_BUFFER_FORMAT_UINT16
            }
        } else {
            SM50_INDEX_BUFFER_FORMAT_NONE
        };

        GraphicsDesc {
            vertex_shader: vs,
            hull_shader: hs,
            domain_shader: ds,
            geometry_shader,
            pixel_shader,
            blend_state,
            input_layout,
            so_layout,
            num_color_attachments: self.state.output_merger.num_rtvs,
            color_attachment_formats: color_formats,
            depth_stencil_format,
            topology_class,
            rasterization_enabled,
            sample_count: u8::try_from(self.state.output_merger.sample_count.max(1)).unwrap_or(1),
            gs_strip_topology,
            index_buffer_format,
            sample_mask: self.state.output_merger.sample_mask,
            gs_passthrough,
        }
    }

    fn finalize_live_render_pipeline(&mut self, indexed: bool) -> Option<u8> {
        let topology = self.state.input_assembler.topology;
        if topology == 0 {
            return None;
        }
        let vs = self.managed_shader(ShaderStage::Vertex)?;
        let _ = vs;
        let hs = self.managed_shader(ShaderStage::Hull);
        let ds = self.managed_shader(ShaderStage::Domain);
        let gs = self.managed_shader(ShaderStage::Geometry);
        let gs_passthrough = gs.as_ref().map_or(u32::MAX, |g| {
            // SAFETY: `g` is a geometry shader so `u.geometry_shader` is initialized.
            let gs_refl = unsafe { g.reflection.u.geometry_shader };
            // SAFETY: `gs_pass_through` is the active union arm for geometry shader reflection.
            unsafe { gs_refl.u.gs_pass_through }
        });
        let gs_ptr = self.state.shader_stages[ShaderStage::Geometry.index()]
            .shader
            .as_ptr();
        let is_so = StreamOutputLayout::from_iface(gs_ptr).is_some();

        let target_kind = if let Some(ref hs_shader) = hs {
            if !(33..=64).contains(&topology) {
                return None;
            }
            // SAFETY: `hs_shader` is a hull shader so `u.tessellator` is initialized.
            let out_prim = unsafe { hs_shader.reflection.u.tessellator.output_primitive };
            if out_prim == MTL_TESSELLATOR_OUTPUT_LINE || out_prim == MTL_TESSELLATOR_OUTPUT_POINT {
                M::emit_st(self, |enc| {
                    enc.set_compatibility_flag(
                        CompatibilityFlags::UNSUPPORTED_TESSELLATION_OUTPUT_PRIMITIVE,
                    );
                });
                return None;
            }
            ds.as_ref()?;
            if gs.is_some() && !is_so && gs_passthrough == u32::MAX {
                M::emit_st(self, |enc| {
                    enc.set_compatibility_flag(
                        CompatibilityFlags::UNSUPPORTED_GEOMETRY_TESSELLATION_DRAW,
                    );
                });
                return None;
            }
            kind::TESSELLATION
        } else if gs.is_some() && !is_so && gs_passthrough == u32::MAX {
            if (33..=64).contains(&topology) {
                return None;
            }
            kind::GEOMETRY
        } else {
            if (33..=64).contains(&topology) {
                return None;
            }
            kind::ORDINARY
        };

        let prev_kind = self.encode_snapshot().pipeline_kind;
        if self.cmdbuf_state == CommandBufferState::RenderPipelineReady && prev_kind == target_kind
        {
            return Some(target_kind);
        }

        if self.cmdbuf_state == CommandBufferState::RenderPipelineReady {
            self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
        }
        self.switch_to_render_encoder();
        if self.cmdbuf_state != CommandBufferState::RenderEncoderActive {
            return None;
        }

        let desc = self.build_graphics_desc(indexed);
        let dev = Device::from_iface(self.device)?;
        let compiled = dev.pipeline_cache().graphics(&desc);
        let pso_handle = compiled.ready().unwrap_or(0);
        if pso_handle == 0 {
            return None;
        }

        let (num_cp_elem, threads_per_patch) = match &*compiled.kind {
            GraphicsPipelineKind::Tessellation(spec) => {
                (spec.num_output_element, spec.threads_per_patch)
            }
            _ => (0, 0),
        };
        let patch_per_group = 32_u32.checked_div(threads_per_patch).unwrap_or(1);

        self.update_encode_snapshot(|snap| {
            snap.render_pipeline_valid = true;
            snap.render_pso = pso_handle;
            snap.pipeline_kind = target_kind;
            if target_kind == kind::TESSELLATION {
                snap.ts_threads_per_patch = threads_per_patch;
                snap.ts_patch_per_group = patch_per_group;
            }
        });

        M::emit_st(self, move |enc| {
            if target_kind == kind::TESSELLATION {
                enc.tess_num_output_control_point_element = num_cp_elem;
                enc.tess_threads_per_patch = threads_per_patch;
                if let Some(re) = enc.render_targets() {
                    re.uses |= dxmt_core::encoder::RenderUses::TESSELLATION;
                    re.last_pso = pso_handle;
                }
            } else if target_kind == kind::GEOMETRY {
                if let Some(re) = enc.render_targets() {
                    re.uses |= dxmt_core::encoder::RenderUses::GEOMETRY;
                    re.last_pso = pso_handle;
                }
            } else if let Some(re) = enc.render_targets() {
                re.last_pso = pso_handle;
            }
            let cmd = WmtcmdRenderSetpso {
                r#type: WMT_RENDER_COMMAND_SET_PSO,
                reserved: [0; 3],
                next: NO_NEXT,
                pso: pso_handle,
            };
            enc.encode_render_command(cmd);
        });

        if prev_kind != target_kind {
            self.state.input_assembler.vertex_buffers.set_all_dirty();
            for st_idx in [
                ShaderStage::Vertex.index(),
                ShaderStage::Hull.index(),
                ShaderStage::Domain.index(),
                ShaderStage::Geometry.index(),
            ] {
                self.state.shader_stages[st_idx]
                    .constant_buffers
                    .set_all_dirty();
                self.state.shader_stages[st_idx].srvs.set_all_dirty();
                self.state.shader_stages[st_idx].samplers.set_all_dirty();
            }
        }

        self.cmdbuf_state = CommandBufferState::RenderPipelineReady;
        Some(target_kind)
    }

    fn update_live_vertex_buffers(&mut self, pipeline_kind: u8) {
        let Some(il) = InputLayout::from_iface(self.state.input_assembler.input_layout.as_ptr())
        else {
            return;
        };
        let slot_mask = il.slot_mask();
        if slot_mask == 0 {
            return;
        }
        if (self.state.input_assembler.vertex_buffers.dirty_mask() & u128::from(slot_mask)) == 0 {
            return;
        }
        let num_slots = u64::from(slot_mask.count_ones());
        let offset = self.pre_allocate_argument_buffer(false, 16 * num_slots, 32);
        M::emit_st(self, move |enc| match pipeline_kind {
            kind::TESSELLATION => {
                enc.encode_vertex_buffers::<{ kind::TESSELLATION }>(slot_mask, offset);
            }
            kind::GEOMETRY => {
                enc.encode_vertex_buffers::<{ kind::GEOMETRY }>(slot_mask, offset);
            }
            _ => {
                enc.encode_vertex_buffers::<{ kind::ORDINARY }>(slot_mask, offset);
            }
        });
        for slot in 0..32 {
            if (slot_mask & (1 << slot)) != 0 {
                self.state
                    .input_assembler
                    .vertex_buffers
                    .clear_dirty_slot(slot);
            }
        }
    }

    fn update_live_so_targets(&mut self, pipeline_kind: u8) {
        if self.state.shader_stages[ShaderStage::Geometry.index()]
            .shader
            .is_null()
        {
            return;
        }
        if !self.state.stream_output.targets.any_dirty() {
            return;
        }
        let offset =
            self.pre_allocate_argument_buffer(false, 16 * (STREAM_OUTPUT_SLOTS as u64), 32);
        if pipeline_kind == kind::ORDINARY {
            M::emit_st(self, move |enc| {
                enc.encode_stream_output(offset);
            });
        }
        self.state.stream_output.targets.clear_dirty();
    }

    fn upload_live_shader_stage_binding<const S: u8, const K: u8>(&mut self) {
        let stage_idx = usize::from(S);
        let ptr = self.state.shader_stages[stage_idx].shader.as_ptr();
        let Some(shader_obj) = Shader::from_iface(ptr) else {
            return;
        };
        let managed = Arc::clone(&shader_obj.managed);
        let refl = &managed.reflection;

        let cb_mask = u128::from(refl.constant_buffer_slot_mask);
        let samp_mask = u128::from(refl.sampler_slot_mask);
        let srv_mask =
            (u128::from(refl.srv_slot_mask_hi) << 64) | u128::from(refl.srv_slot_mask_lo);
        let uav_mask = u128::from(refl.uav_slot_mask);

        let dirty_cb = (self.state.shader_stages[stage_idx]
            .constant_buffers
            .dirty_mask()
            & cb_mask)
            != 0;
        let dirty_samp =
            (self.state.shader_stages[stage_idx].samplers.dirty_mask() & samp_mask) != 0;
        let dirty_srv = (self.state.shader_stages[stage_idx].srvs.dirty_mask() & srv_mask) != 0;
        let uav_bound = if S == stage::COMPUTE {
            (self.state.compute_stage_uav.uavs.bound_mask() & uav_mask) != 0
        } else {
            (self.state.output_merger.uavs.bound_mask() & uav_mask) != 0
        };

        if !dirty_cb && !dirty_samp && !dirty_srv && !uav_bound {
            return;
        }

        let is_compute = S == stage::COMPUTE;
        if refl.num_constant_buffers > 0 && dirty_cb {
            let cb_bytes = u64::from(refl.num_constant_buffers) << 3;
            let offset = self.pre_allocate_argument_buffer(is_compute, cb_bytes, 32);
            let managed_cb = Arc::clone(&managed);
            M::emit_st(self, move |enc| {
                enc.encode_constant_buffers::<S, K>(&managed_cb.constant_buffers, offset);
            });
            self.state.shader_stages[stage_idx]
                .constant_buffers
                .clear_dirty();
        }

        if refl.num_arguments > 0 && (dirty_samp || dirty_srv || uav_bound) {
            let arg_bytes = u64::from(refl.argument_table_qwords) << 3;
            let offset = self.pre_allocate_argument_buffer(is_compute, arg_bytes, 32);
            let managed_arg = Arc::clone(&managed);
            M::emit_st(self, move |enc| {
                enc.encode_shader_resources::<S, K>(&managed_arg.arguments, offset);
            });
            self.state.shader_stages[stage_idx].samplers.clear_dirty();
            self.state.shader_stages[stage_idx].srvs.clear_dirty();
            if S == stage::PIXEL {
                self.state.output_merger.uavs.clear_dirty();
            } else if S == stage::COMPUTE {
                self.state.compute_stage_uav.uavs.clear_dirty();
            }
        }
    }

    /// Selects and binds the active graphics pipeline (`FinalizeRenderPipeline`).
    #[must_use]
    pub fn finalize_graphics_pipeline(&mut self) -> bool {
        if self.is_live() {
            return self.finalize_live_render_pipeline(false).is_some();
        }
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
        if self.is_live() {
            if self.cmdbuf_state == CommandBufferState::ComputePipelineReady {
                return true;
            }
            let Some(cs) = self.managed_shader(ShaderStage::Compute) else {
                return false;
            };
            self.switch_to_compute_encoder();
            if self.cmdbuf_state != CommandBufferState::ComputeEncoderActive {
                return false;
            }
            let Some(dev) = Device::from_iface(self.device) else {
                return false;
            };
            let compiled = dev.pipeline_cache().compute(&cs);
            let pso_handle = compiled.ready().unwrap_or(0);
            if pso_handle == 0 {
                return false;
            }
            let tg = compiled.spec.threadgroup_size;
            let tg_size = WMTSize {
                width: u64::from(tg[0]),
                height: u64::from(tg[1]),
                depth: u64::from(tg[2]),
            };
            self.update_encode_snapshot(|snap| {
                snap.compute_pipeline_valid = true;
                snap.compute_pso = pso_handle;
                snap.compute_threadgroup_size = tg_size;
            });
            M::emit_st(self, move |enc| {
                let cmd = WmtcmdComputeSetpso {
                    r#type: WMT_COMPUTE_COMMAND_SET_PSO,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    pso: pso_handle,
                    threadgroup_size: tg_size,
                };
                enc.encode_compute_command(cmd);
            });
            self.cmdbuf_state = CommandBufferState::ComputePipelineReady;
            return true;
        }
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
        if self.is_live() {
            if indexed && self.state.input_assembler.index_buffer.is_null() {
                return false;
            }
            if self.state.rasterizer.num_viewports == 0 {
                return false;
            }
            let Some(pipeline_kind) = self.finalize_live_render_pipeline(indexed) else {
                return false;
            };
            self.update_live_vertex_buffers(pipeline_kind);
            self.update_live_so_targets(pipeline_kind);

            let stencil_ref =
                u8::try_from(self.state.output_merger.stencil_ref & 0xff).unwrap_or(0);
            if self.dirty.contains(DirtyFlags::DEPTH_STENCIL_STATE) {
                let ds_ptr = if self.state.output_merger.depth_stencil_state.is_null() {
                    self.defaults.depth_stencil
                } else {
                    self.state.output_merger.depth_stencil_state.as_ptr()
                };
                if let Some(dss) = DepthStencilState::from_iface(ds_ptr) {
                    let handles = [dss.state(0), dss.state(1), dss.state(2), dss.state(3)];
                    M::emit_st(self, move |enc| {
                        let planar = enc
                            .render_targets()
                            .map_or(0, |re| usize::from(re.dsv_planar_flags & 3));
                        let cmd = WmtcmdRenderSetdsso {
                            r#type: WMT_RENDER_COMMAND_SET_DSSO,
                            reserved: [0; 3],
                            next: NO_NEXT,
                            dsso: handles[planar],
                            stencil_ref,
                        };
                        enc.encode_render_command(cmd);
                    });
                }
            }

            let rs_ptr = if self.state.rasterizer.rasterizer_state.is_null() {
                self.defaults.rasterizer
            } else {
                self.state.rasterizer.rasterizer_state.as_ptr()
            };
            let current_rs = RasterizerState::from_iface(rs_ptr);
            let allow_scissor = current_rs.is_some_and(RasterizerState::is_scissor_enabled);

            if self.dirty.contains(DirtyFlags::RASTERIZER_STATE)
                && let Some(rs) = current_rs
            {
                let mut cmd = WmtcmdRenderSetrasterizerstate {
                    r#type: WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    fill_mode: 0,
                    cull_mode: 0,
                    depth_clip_mode: 0,
                    winding: 0,
                    depth_bias: 0.0,
                    scole_scale: 0.0,
                    depth_bias_clamp: 0.0,
                };
                rs.setup(&mut cmd);
                M::emit_st(self, move |enc| {
                    enc.encode_render_command(cmd);
                });
            }

            if self
                .dirty
                .contains(DirtyFlags::BLEND_FACTOR_AND_SAMPLE_MASK)
            {
                let bf = self.state.output_merger.blend_factor;
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
                M::emit_st(self, move |enc| {
                    enc.encode_render_command(cmd);
                });
            }

            let num_vp = (self.state.rasterizer.num_viewports as usize).min(16);
            if self.dirty.contains(DirtyFlags::VIEWPORTS) {
                let mut vps = Vec::with_capacity(num_vp);
                for vp in self.state.rasterizer.viewports.iter().take(num_vp) {
                    vps.push(WMTViewport {
                        origin_x: f64::from(vp.top_left_x),
                        origin_y: f64::from(vp.top_left_y),
                        width: f64::from(vp.width),
                        height: f64::from(vp.height),
                        znear: f64::from(vp.min_depth),
                        zfar: f64::from(vp.max_depth),
                    });
                }
                let (raw_vp, _) = M::alloc(
                    self,
                    vps.len() * size_of::<WMTViewport>(),
                    align_of::<WMTViewport>(),
                );
                #[expect(
                    clippy::cast_ptr_alignment,
                    reason = "M::alloc aligns to align_of::<WMTViewport>()"
                )]
                let vp_ptr = core::ptr::NonNull::new(raw_vp.cast::<WMTViewport>())
                    .unwrap_or(core::ptr::NonNull::dangling());
                if !raw_vp.is_null() && !vps.is_empty() {
                    // SAFETY: `M::alloc` allocated space for `vps.len()` `WMTViewport` items.
                    unsafe {
                        core::ptr::copy_nonoverlapping(vps.as_ptr(), vp_ptr.as_ptr(), vps.len());
                    }
                }
                let viewport_count = u8::try_from(vps.len()).unwrap_or(0);
                let cmd = WmtcmdRenderSetviewports {
                    r#type: WMT_RENDER_COMMAND_SET_VIEWPORTS,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    viewports: slice_mem_ptr(vp_ptr),
                    viewport_count,
                };
                M::emit_st(self, move |enc| {
                    enc.encode_render_command(cmd);
                });
            }

            if self.dirty.contains(DirtyFlags::SCISSOR_RECTS) {
                #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let rt_w = if self.state.output_merger.render_target_width == 0 {
                    self.state.rasterizer.viewports[0].width.max(0.0) as u32
                } else {
                    self.state.output_merger.render_target_width
                };
                #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let rt_h = if self.state.output_merger.render_target_height == 0 {
                    self.state.rasterizer.viewports[0].height.max(0.0) as u32
                } else {
                    self.state.output_merger.render_target_height
                };
                let num_sc = self.state.rasterizer.num_scissor_rects as usize;
                let mut scs = Vec::with_capacity(num_vp);
                for (i, r) in self
                    .state
                    .rasterizer
                    .scissor_rects
                    .iter()
                    .enumerate()
                    .take(num_vp)
                {
                    if allow_scissor {
                        if i < num_sc {
                            let max_w = i32::try_from(rt_w).unwrap_or(i32::MAX);
                            let max_h = i32::try_from(rt_h).unwrap_or(i32::MAX);
                            let left = r.left.clamp(0, max_w);
                            let top = r.top.clamp(0, max_h);
                            let right = r.right.clamp(left, max_w);
                            let bottom = r.bottom.clamp(top, max_h);
                            scs.push(WMTScissorRect {
                                x: u64::try_from(left).unwrap_or(0),
                                y: u64::try_from(top).unwrap_or(0),
                                width: u64::try_from(right - left).unwrap_or(0),
                                height: u64::try_from(bottom - top).unwrap_or(0),
                            });
                        } else {
                            scs.push(WMTScissorRect {
                                x: 0,
                                y: 0,
                                width: 0,
                                height: 0,
                            });
                        }
                    } else {
                        scs.push(WMTScissorRect {
                            x: 0,
                            y: 0,
                            width: u64::from(rt_w),
                            height: u64::from(rt_h),
                        });
                    }
                }
                let (raw_sc, _) = M::alloc(
                    self,
                    scs.len() * size_of::<WMTScissorRect>(),
                    align_of::<WMTScissorRect>(),
                );
                #[expect(
                    clippy::cast_ptr_alignment,
                    reason = "M::alloc aligns to align_of::<WMTScissorRect>()"
                )]
                let sc_ptr = core::ptr::NonNull::new(raw_sc.cast::<WMTScissorRect>())
                    .unwrap_or(core::ptr::NonNull::dangling());
                if !raw_sc.is_null() && !scs.is_empty() {
                    // SAFETY: `M::alloc` allocated space for `scs.len()` `WMTScissorRect` items.
                    unsafe {
                        core::ptr::copy_nonoverlapping(scs.as_ptr(), sc_ptr.as_ptr(), scs.len());
                    }
                }
                let rect_count = u8::try_from(scs.len()).unwrap_or(0);
                let cmd = WmtcmdRenderSetscissorrects {
                    r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    scissor_rects: slice_mem_ptr(sc_ptr),
                    rect_count,
                };
                M::emit_st(self, move |enc| {
                    enc.encode_render_command(cmd);
                });
            }

            self.dirty = DirtyFlags::empty();

            match pipeline_kind {
                kind::TESSELLATION => {
                    self.upload_live_shader_stage_binding::<{ stage::VERTEX }, { kind::TESSELLATION }>();
                    self.upload_live_shader_stage_binding::<{ stage::PIXEL }, { kind::TESSELLATION }>();
                    self.upload_live_shader_stage_binding::<{ stage::HULL }, { kind::TESSELLATION }>();
                    self.upload_live_shader_stage_binding::<{ stage::DOMAIN }, { kind::TESSELLATION }>();
                }
                kind::GEOMETRY => {
                    self.upload_live_shader_stage_binding::<{ stage::VERTEX }, { kind::GEOMETRY }>(
                    );
                    self.upload_live_shader_stage_binding::<{ stage::PIXEL }, { kind::GEOMETRY }>();
                    self.upload_live_shader_stage_binding::<{ stage::GEOMETRY }, { kind::GEOMETRY }>();
                }
                _ => {
                    self.upload_live_shader_stage_binding::<{ stage::VERTEX }, { kind::ORDINARY }>(
                    );
                    self.upload_live_shader_stage_binding::<{ stage::PIXEL }, { kind::ORDINARY }>();
                }
            }
            return true;
        }

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
            self.emit_render_state_cmd(cmd, move |enc| {
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
            self.emit_render_state_cmd(cmd, move |enc| {
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
            self.emit_render_state_cmd(cmd, move |enc| {
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
            self.emit_render_state_cmd(cmd, move |enc| {
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
            self.emit_render_state_cmd(cmd, move |enc| {
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
            self.emit_render_state_cmd(cmd, move |enc| {
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
            self.emit_render_state_cmd(cmd, move |enc| {
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
            self.emit_render_state_cmd(cmd, move |enc| {
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
            self.emit_render_state_cmd(cmd, move |enc| {
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
            self.emit_render_state_cmd(cmd, move |enc| {
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
            self.emit_render_state_cmd(cmd, move |enc| {
                enc.encode_render_command(cmd);
            });
        }

        self.dirty = DirtyFlags::empty();
        true
    }

    /// Prepares the compute encoder and dirty state before a dispatch (`PreDispatch`).
    #[must_use]
    pub fn pre_dispatch(&mut self) -> bool {
        if self.is_live() {
            if !self.finalize_compute_pipeline() {
                return false;
            }
            self.upload_live_shader_stage_binding::<{ stage::COMPUTE }, { kind::ORDINARY }>();
            return true;
        }
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
            self.emit_compute_state_cmd(cmd, move |enc| {
                enc.encode_compute_command(cmd);
            });
        }
        true
    }
}
