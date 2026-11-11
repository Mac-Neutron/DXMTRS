//! One fixed-function draw on the core (C4-9): its pipeline, built once per key, and its records.
//!
//! The front end builds both stages as DXSO (`ff_build`) with their specialization; this
//! translates each through the core's DXSO entry (`initialize_dxso`, then `compile`,
//! `compiled_data` and `load_function`, the D3D10-12 path), makes the pipeline with the vertex
//! descriptor (slot 156), and records the pipeline, the vertex bytes at buffer 0, the stages'
//! constants at D3D9's slots and the draw into the back buffer's render pass. The pipeline is
//! built on the API thread and waited for (no asynchronous compile until C4d).

use core::ffi::CStr;

use airconv_ir::dxso::spec::{StageSpec, VariantKey, VsSamplerKinds, VsSpec};
use dxmt_core::{
    encoder::{NO_NEXT, direct, stage},
    shader::{Entry, compile, compiled_data, initialize_dxso, load_function},
};
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

/// What a fixed-function `DrawPrimitiveUP` needs, as the front end resolved it.
pub struct FfDraw<'a> {
    /// The vertex stage, `ff_build::vertex`'s tokens.
    pub vs_tokens: &'a [u32],
    /// The vertex inputs the vertex stage declares (`FfProgram::provided_input_mask`).
    pub vs_inputs: u16,
    /// User clip planes the vertex stage writes.
    pub clip_planes: u8,
    /// The pixel stage, `ff_build::pixel`'s tokens.
    pub ps_tokens: &'a [u32],
    /// The pixel stage's specialization.
    pub variant: VariantKey,
    /// Each vertex attribute: `(attribute index, MTLVertexFormat, byte offset)`, from buffer 0.
    pub attributes: &'a [(u32, u32, u32)],
    /// Bytes from one vertex to the next.
    pub stride: u32,
    /// The colour target's `MTLColorWriteMask`.
    pub write_mask: u8,
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

impl CoreDevice {
    /// Records `draw` into the back buffer's render pass.
    ///
    /// False, and nothing recorded, when its pipeline does not build (logged once per key), or
    /// the device has no back buffer.
    pub fn draw_up(&mut self, draw: &FfDraw<'_>) -> bool {
        if self.back_buffer.is_none() {
            return false;
        }
        let Some(pso) = self.ff_pipeline(draw) else {
            return false;
        };
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
        true
    }

    /// The pipeline of `draw`'s key: its handle, which the device keeps for its life.
    fn ff_pipeline(&mut self, draw: &FfDraw<'_>) -> Option<u64> {
        let key = pipeline_key(draw);
        if let Some(built) = self.pipelines.get(&key) {
            return built.as_ref().map(Obj::handle);
        }
        let built = self.build_pipeline(draw);
        if built.is_none() {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: the fixed-function pipeline {key:#018x} does not build; its draws are \
                 dropped"
            );
        }
        let handle = built.as_ref().map(Obj::handle);
        self.pipelines.insert(key, built);
        handle
    }

    fn build_pipeline(&self, draw: &FfDraw<'_>) -> Option<Obj> {
        let vs_spec = StageSpec::Vs(VsSpec {
            provided_input_mask: draw.vs_inputs,
            clip_plane_count: draw.clip_planes,
            sampler_kinds: VsSamplerKinds::default(),
        });
        let vs = self.compile_stage(draw.vs_tokens, &vs_spec, c"d3d9_ff_vs")?;
        let ps_spec = StageSpec::Ps(draw.variant);
        let ps = self.compile_stage(draw.ps_tokens, &ps_spec, c"d3d9_ff_ps")?;
        let mut info = dxmt_core::library::render_pipeline_info();
        info.vertex_function = vs.handle();
        info.fragment_function = ps.handle();
        info.colors[0].pixel_format = WMT_PIXEL_FORMAT_BGRA8_UNORM;
        info.colors[0].write_mask = draw.write_mask;
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
        for &(index, format, offset) in draw.attributes {
            if let Some(attribute) = vertex.attributes.get_mut(index as usize) {
                *attribute = WMTVertexAttributeInfo {
                    format,
                    offset,
                    buffer_index: u32::from(slot::VERTICES),
                };
            }
        }
        vertex.layouts[usize::from(slot::VERTICES)] = WMTVertexBufferLayoutInfo {
            stride: draw.stride,
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

/// What a pipeline is built from, hashed.
///
/// Both stages and their specializations, the vertex layout and the write mask.
fn pipeline_key(draw: &FfDraw<'_>) -> u64 {
    let mut bytes = Vec::with_capacity((draw.vs_tokens.len() + draw.ps_tokens.len()) * 4 + 64);
    for token in draw.vs_tokens.iter().chain(draw.ps_tokens) {
        bytes.extend_from_slice(&token.to_le_bytes());
    }
    let (_, vs, _) = StageSpec::Vs(VsSpec {
        provided_input_mask: draw.vs_inputs,
        clip_plane_count: draw.clip_planes,
        sampler_kinds: VsSamplerKinds::default(),
    })
    .to_wire();
    let (_, _, ps) = StageSpec::Ps(draw.variant).to_wire();
    bytes.extend_from_slice(&vs);
    bytes.extend_from_slice(&ps);
    for &attribute in draw.attributes {
        for word in <[u32; 3]>::from(attribute) {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&draw.stride.to_le_bytes());
    bytes.push(draw.write_mask);
    xxhash_rust::xxh3::xxh3_64(&bytes)
}

#[cfg(test)]
mod tests;
