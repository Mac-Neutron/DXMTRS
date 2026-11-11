//! One fixed-function draw on the core (C4-9): its pipeline, built once per key, and its records.
//!
//! The front end builds the draw's [`FfPipelineKey`] and its per-draw data. On a key the device
//! has not seen, both stages are built as DXSO (`ff_source`) and translated through the core's
//! DXSO entry (`initialize_dxso`, then `compile`, `compiled_data` and `load_function`, the
//! D3D10-12 path), and the pipeline is made with the vertex descriptor (slot 156) and kept in the
//! device's [`PsoMap`](dxmt_core::pipeline_cache::PsoMap). The draw records the pipeline, the
//! vertex bytes at buffer 0, the stages' constants at D3D9's slots and the draw into the back
//! buffer's render pass. The pipeline is built on the API thread and waited for (no asynchronous
//! compile until C4d).

use core::ffi::CStr;
use std::sync::{Arc, OnceLock};

use airconv_ir::dxso::spec::{StageSpec, VsSamplerKinds, VsSpec};
use dxmt_core::{
    encoder::{NO_NEXT, direct, stage},
    pipeline_cache::Compiled,
    shader::{Entry, compile, compiled_data, initialize_dxso, load_function},
};
use mtld3d_core::ff_pipeline::{FfPipelineKey, FfPipelineSource, ff_source};
use winemetal_sys::{
    MTLDevice_newRenderPipelineStateWithVertexDescriptor, Obj,
    sm50::{SM50_SHADER_COMMON, SM50_SHADER_METAL_310, Sm50ShaderCommonData},
    wire::{
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_SET_PSO,
        WMTVertexAttributeInfo, WMTVertexBufferLayoutInfo, WMTVertexDescriptorInfo,
        WmtcmdRenderDraw, WmtcmdRenderSetpso,
    },
};

use super::{CoreDevice, LOG_TARGET};

/// `MTLVertexStepFunctionPerVertex`.
const STEP_PER_VERTEX: u32 = 1;

/// D3D9's vertex and pixel buffer slots (`mtld3d-shared`'s `mtl.rs`, airconv's `bindings.rs`).
mod slot {
    pub const VERTICES: u8 = 0;
    pub const VS_FLOAT_CONST: u8 = 30;
    pub const VS_POS_FIXUP: u8 = 28;
    pub const VS_DRAW: u8 = 27;
    pub const PS_FLOAT_CONST: u8 = 15;
    pub const PS_ALPHA_REF: u8 = 14;
    pub const PS_FOG: u8 = 13;
}

/// What one fixed-function `DrawPrimitiveUP` binds and draws, as the front end resolved it.
pub struct FfDrawData<'a> {
    /// The `MTLPrimitiveType`.
    pub primitive: u8,
    pub vertex_count: u32,
    pub vertices: &'a [u8],
    /// The vertex stage's `c#` rows.
    pub vs_constants: &'a [u8],
    /// `PosFixup`: `(1/vp_w, -1/vp_h, depth clamp, render scale, depth bias)`.
    pub pos_fixup: [f32; 5],
    pub vs_draw: &'a [u8],
    /// The pixel stage's `c#` rows.
    pub ps_constants: &'a [u8],
    /// The alpha test's reference, empty when the test is off (not bound).
    pub alpha_ref: &'a [u8],
    /// `fog_data`, empty without fog (not bound).
    pub fog: &'a [u8],
}

/// Why a draw recorded nothing.
#[derive(Debug, PartialEq, Eq)]
pub enum DrawRefused {
    /// The device has no back buffer.
    NoBackBuffer,
    /// The draw's pipeline does not build (logged once per key).
    Pipeline,
}

impl CoreDevice {
    /// Records `draw` with the pipeline of `key` into the back buffer's render pass.
    ///
    /// # Errors
    ///
    /// Nothing recorded when the device has no back buffer, or the pipeline does not build.
    pub fn draw_up(
        &mut self,
        key: &FfPipelineKey,
        draw: &FfDrawData<'_>,
    ) -> Result<(), DrawRefused> {
        if self.back_buffer.is_none() {
            return Err(DrawRefused::NoBackBuffer);
        }
        let pso = self.ff_pipeline(key).ok_or(DrawRefused::Pipeline)?;
        self.open_render_pass();
        let vertices = draw.vertices.to_vec();
        let vs_constants = draw.vs_constants.to_vec();
        let pos_fixup = draw
            .pos_fixup
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<u8>>();
        let vs_draw = draw.vs_draw.to_vec();
        let ps_constants = draw.ps_constants.to_vec();
        let alpha_ref = draw.alpha_ref.to_vec();
        let fog = draw.fog.to_vec();
        let (primitive_type, vertex_count) = (draw.primitive, draw.vertex_count);
        self.emit(move |enc| {
            enc.encode_render_command(WmtcmdRenderSetpso {
                r#type: WMT_RENDER_COMMAND_SET_PSO,
                reserved: [0; 3],
                next: NO_NEXT,
                pso,
            });
            direct::bind_bytes::<{ stage::VERTEX }>(enc, slot::VERTICES, &vertices);
            direct::bind_bytes::<{ stage::VERTEX }>(enc, slot::VS_FLOAT_CONST, &vs_constants);
            direct::bind_bytes::<{ stage::VERTEX }>(enc, slot::VS_POS_FIXUP, &pos_fixup);
            direct::bind_bytes::<{ stage::VERTEX }>(enc, slot::VS_DRAW, &vs_draw);
            direct::bind_bytes::<{ stage::PIXEL }>(enc, slot::PS_FLOAT_CONST, &ps_constants);
            if !alpha_ref.is_empty() {
                direct::bind_bytes::<{ stage::PIXEL }>(enc, slot::PS_ALPHA_REF, &alpha_ref);
            }
            if !fog.is_empty() {
                direct::bind_bytes::<{ stage::PIXEL }>(enc, slot::PS_FOG, &fog);
            }
            enc.encode_render_command(WmtcmdRenderDraw {
                r#type: WMT_RENDER_COMMAND_DRAW,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type,
                vertex_start: 0,
                vertex_count: u64::from(vertex_count),
                instance_count: 1,
                base_instance: 0,
            });
        });
        Ok(())
    }

    /// The pipeline of `key`: its handle, which the device keeps for its life.
    ///
    /// Built on the first draw of `key`; `None` when it does not build, logged then.
    fn ff_pipeline(&self, key: &FfPipelineKey) -> Option<u64> {
        if let Some(entry) = self.pipelines.get(key) {
            return entry.ready();
        }
        let pso = self.build_pipeline(&ff_source(key));
        if pso.is_none() {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: the fixed-function pipeline of FVF {:#x} (stride {}) does not build; \
                 its draws are dropped",
                key.fvf,
                key.stride
            );
        }
        let handle = pso.as_ref().map_or(0, Obj::handle);
        // Built and ready (C4b-3): no job, so the scheduler is never started.
        let entry = Compiled::ready_with(Arc::new(()), handle, pso);
        self.pipelines
            .insert_or_existing(key.clone(), entry, &OnceLock::new())
            .ready()
    }

    fn build_pipeline(&self, source: &FfPipelineSource) -> Option<Obj> {
        let vs_spec = StageSpec::Vs(VsSpec {
            provided_input_mask: source.provided_input_mask,
            clip_plane_count: source.clip_plane_count,
            sampler_kinds: VsSamplerKinds::default(),
        });
        let vs = self.compile_stage(&source.vs_tokens, &vs_spec, c"d3d9_ff_vs")?;
        let ps_spec = StageSpec::Ps(source.variant);
        let ps = self.compile_stage(&source.ps_tokens, &ps_spec, c"d3d9_ff_ps")?;
        let mut info = dxmt_core::library::render_pipeline_info();
        info.vertex_function = vs.handle();
        info.fragment_function = ps.handle();
        info.colors[0].pixel_format = WMT_PIXEL_FORMAT_BGRA8_UNORM;
        info.colors[0].write_mask = source.write_mask;
        let mut vertex = WMTVertexDescriptorInfo {
            attributes: [WMTVertexAttributeInfo {
                format: 0,
                offset: 0,
                buffer_index: 0,
            }; 31],
            layouts: [WMTVertexBufferLayoutInfo {
                stride: 0,
                step_function: 0,
                step_rate: 0,
            }; 31],
        };
        for &(index, format, offset) in &source.attrs {
            if let Some(attribute) = vertex.attributes.get_mut(index as usize) {
                *attribute = WMTVertexAttributeInfo {
                    format,
                    offset,
                    buffer_index: u32::from(slot::VERTICES),
                };
            }
        }
        vertex.layouts[usize::from(slot::VERTICES)] = WMTVertexBufferLayoutInfo {
            stride: source.stride,
            step_function: STEP_PER_VERTEX,
            step_rate: 1,
        };
        let mut error = 0;
        let device = self.device.device().handle();
        // SAFETY: a live device; the info, the vertex descriptor and the error slot are alive for
        // the call; the functions are retained by the pipeline.
        let pso = unsafe {
            MTLDevice_newRenderPipelineStateWithVertexDescriptor(
                device,
                &raw const info,
                &raw const vertex,
                &raw mut error,
            )
        };
        // SAFETY: a new error's handle carries its reference.
        drop(unsafe { Obj::adopt(error) });
        // SAFETY: a new pipeline's handle carries its reference.
        unsafe { Obj::adopt(pso) }
    }

    /// The function `name` of the DXSO stage `tokens`, translated with `spec`.
    fn compile_stage(&self, tokens: &[u32], spec: &StageSpec, name: &CStr) -> Option<Obj> {
        let bytes = tokens
            .iter()
            .flat_map(|t| t.to_le_bytes())
            .collect::<Vec<u8>>();
        let (shader, mut d3d9) = initialize_dxso(&bytes, spec.to_wire())
            .map_err(|e| log::error!(target: LOG_TARGET, "d3d9 core: DXSO: {}", e.message()))
            .ok()?;
        let mut common = Sm50ShaderCommonData {
            next: (&raw mut d3d9).cast(),
            r#type: SM50_SHADER_COMMON,
            metal_version: SM50_SHADER_METAL_310,
            flags: 0,
        };
        // SAFETY: `shader` is live; the chain (`common`, then `d3d9`) lives for the call.
        let bitcode =
            unsafe { compile(&Entry::Shader, shader.0, 0, (&raw mut common).cast(), name) }
                .map_err(|e| log::error!(target: LOG_TARGET, "d3d9 core: compile: {}", e.message()))
                .ok()?;
        let data = compiled_data(&bitcode)?;
        load_function(self.device.device(), &data, name).ok()
    }
}

#[cfg(test)]
mod tests;
