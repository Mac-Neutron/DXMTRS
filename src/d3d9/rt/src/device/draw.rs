//! One UP draw on the core (C4-9, C4b-2): its pipeline, built once per key, and its records.
//!
//! The front end builds the draw's [`PipelineKey`] and its per-draw data ([`DrawData`]). On a key
//! the device has not seen, [`pipeline_source`] turns it into both stages' DXSO and the vertex
//! layout, each stage is translated through the core's DXSO entry (`initialize_dxso`, then
//! `compile`, `compiled_data` and `load_function`, the D3D10-12 path) unless the device already
//! holds that stage's function for that specialization, and the pipeline is made with the vertex
//! descriptor (slot 156) and kept in the device's [`PsoMap`](dxmt_core::pipeline_cache::PsoMap).
//! The draw records the pipeline, the viewport, the rasterizer state, the vertex bytes at buffer
//! 0, the stages' binds and the draw (its indices, if any, in the argument-buffer ring) into the
//! back buffer's render pass. The pipeline is built on the API thread and waited for (no
//! asynchronous compile until C4d).

use core::ffi::CStr;
use std::{ops::Range, sync::Arc};

use airconv_ir::dxso::spec::{StageSpec, VARIANT_KEY_WIRE, VS_SPEC_WIRE};
use dxmt_core::{
    device::Device,
    encoder::{NO_NEXT, direct, stage},
    pipeline_cache::Compiled,
    shader::{Entry, compile, compiled_data, initialize_dxso, load_function},
};
use mtld3d_core::pipeline_key::{
    PipelineKey, PipelineSource, ShaderCode, StageSource, pipeline_source,
};
use rustc_hash::FxHashMap;
use winemetal_sys::{
    MTLDevice_newRenderPipelineStateWithVertexDescriptor, Obj,
    sm50::{SM50_SHADER_COMMON, SM50_SHADER_METAL_310, Sm50ShaderCommonData},
    wire::{
        MemPtr, WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_RENDER_COMMAND_DRAW,
        WMT_RENDER_COMMAND_DRAW_INDEXED, WMT_RENDER_COMMAND_SET_PSO,
        WMT_RENDER_COMMAND_SET_RASTERIZER_STATE, WMT_RENDER_COMMAND_SET_VIEWPORTS,
        WMTVertexAttributeInfo, WMTVertexBufferLayoutInfo, WMTVertexDescriptorInfo, WMTViewport,
        WmtcmdRenderDraw, WmtcmdRenderDrawIndexed, WmtcmdRenderSetpso,
        WmtcmdRenderSetrasterizerstate, WmtcmdRenderSetviewports,
    },
};

use super::{CoreDevice, LOG_TARGET};

/// `MTLVertexStepFunctionPerVertex`.
const STEP_PER_VERTEX: u32 = 1;

/// D3D9's vertex buffer slot of a UP draw's vertices (the front end names the others).
mod slot {
    pub const VERTICES: u8 = 0;
}

/// The stage a [`Bind`] binds at.
// Copy: one byte, copied into the closure's bind list.
#[derive(Clone, Copy)]
pub enum Stage {
    Vertex,
    Pixel,
}

/// Bytes a draw binds at one stage's buffer slot; empty ones are not bound.
pub struct Bind<'a> {
    pub stage: Stage,
    pub slot: u8,
    pub bytes: &'a [u8],
}

/// An indexed draw's indices.
pub struct Indices<'a> {
    pub bytes: &'a [u8],
    /// `MTLIndexType`: 0 for 16-bit, 1 for 32-bit.
    pub index_type: u8,
    pub count: u32,
}

/// What one UP draw binds and draws, as the front end resolved it.
///
/// A `DrawPrimitiveUP` or a `DrawIndexedPrimitiveUP`.
pub struct DrawData<'a> {
    /// The `MTLPrimitiveType`.
    pub primitive: u8,
    pub vertices: &'a [u8],
    pub vertex_count: u32,
    /// The indices of an indexed draw (a fan's triangle list too).
    pub indices: Option<Indices<'a>>,
    /// `(origin x, origin y, width, height, znear, zfar)`.
    pub viewport: [f64; 6],
    /// The `MTLTriangleFillMode`.
    pub fill_mode: u8,
    /// The `MTLCullMode`.
    pub cull_mode: u8,
    pub binds: &'a [Bind<'a>],
    /// The sampled slots' textures and samplers, each at its stage.
    pub textures: &'a [super::TextureBind],
}

/// Why a draw recorded nothing.
#[derive(Debug, PartialEq, Eq)]
pub enum DrawRefused {
    /// The device has no back buffer.
    NoBackBuffer,
    /// The draw's pipeline does not build (logged once per key).
    Pipeline,
    /// An attribute crosses the stride, which the core cannot fetch yet.
    Crossing,
}

/// A compiled stage function's key: its program and its specialization's wire form.
#[derive(PartialEq, Eq, Hash)]
struct StageFnKey {
    code: ShaderCode,
    wire: (u32, [u8; VS_SPEC_WIRE], [u8; VARIANT_KEY_WIRE]),
}

/// The device's compiled stage functions, `None` for one that did not compile.
///
/// One per program and specialization, as D3D11's variants are: a vertex stage paired with many
/// pixel stages compiles once.
#[derive(Default)]
pub struct StageFunctions {
    map: FxHashMap<StageFnKey, Option<Obj>>,
    /// The stage compiles so far.
    #[cfg(test)]
    compiles: usize,
}

impl StageFunctions {
    /// The handle of `stage`'s function `name`, compiled on its first use.
    fn function(&mut self, device: &Device, stage: &StageSource, name: &CStr) -> Option<u64> {
        let key = StageFnKey {
            code: stage.code.clone(),
            wire: stage.spec.to_wire(),
        };
        let function = self.map.entry(key).or_insert_with(|| {
            #[cfg(test)]
            {
                self.compiles += 1;
            }
            compile_stage(device, stage.code.tokens(), &stage.spec, name)
        });
        function.as_ref().map(Obj::handle)
    }
}

impl CoreDevice {
    /// Records `draw` with the pipeline of `key` into the back buffer's render pass.
    ///
    /// # Errors
    ///
    /// Nothing recorded when the device has no back buffer, the layout crosses its stride, or
    /// the pipeline does not build.
    pub fn draw_up(&mut self, key: &PipelineKey, draw: &DrawData<'_>) -> Result<(), DrawRefused> {
        if self.back_buffer.is_none() {
            return Err(DrawRefused::NoBackBuffer);
        }
        let pso = self.pipeline(key)?;
        self.open_render_pass();
        let vertices = draw.vertices.to_vec();
        let mut bytes = Vec::new();
        let mut binds = Vec::<(Stage, u8, Range<usize>)>::new();
        for bind in draw.binds.iter().filter(|b| !b.bytes.is_empty()) {
            let start = bytes.len();
            bytes.extend_from_slice(bind.bytes);
            binds.push((bind.stage, bind.slot, start..bytes.len()));
        }
        let indices = draw
            .indices
            .as_ref()
            .map(|i| (i.bytes.to_vec(), i.index_type, i.count));
        let [origin_x, origin_y, width, height, znear, zfar] = draw.viewport;
        let viewport = WMTViewport {
            origin_x,
            origin_y,
            width,
            height,
            znear,
            zfar,
        };
        let rasterizer = WmtcmdRenderSetrasterizerstate {
            r#type: WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
            reserved: [0; 3],
            next: NO_NEXT,
            fill_mode: draw.fill_mode,
            cull_mode: draw.cull_mode,
            depth_clip_mode: 0,
            winding: 0,
            depth_bias: 0.0,
            scole_scale: 0.0,
            depth_bias_clamp: 0.0,
        };
        let (primitive_type, vertex_count) = (draw.primitive, draw.vertex_count);
        let textures: Vec<_> = draw
            .textures
            .iter()
            .map(|t| {
                (
                    t.stage,
                    t.slot,
                    Arc::clone(&t.texture),
                    t.key,
                    t.sampler.clone(),
                )
            })
            .collect();
        self.emit(move |enc| {
            enc.encode_render_command(WmtcmdRenderSetpso {
                r#type: WMT_RENDER_COMMAND_SET_PSO,
                reserved: [0; 3],
                next: NO_NEXT,
                pso,
            });
            let slot = enc
                .allocate_cpu(size_of::<WMTViewport>(), align_of::<WMTViewport>())
                .cast::<WMTViewport>();
            // SAFETY: the encoder's memory, sized and aligned for one viewport, nobody else's.
            unsafe { slot.write(viewport) };
            enc.encode_render_command(WmtcmdRenderSetviewports {
                r#type: WMT_RENDER_COMMAND_SET_VIEWPORTS,
                reserved: [0; 3],
                next: NO_NEXT,
                viewports: MemPtr::from_ptr(slot.as_ptr().cast::<u8>()),
                viewport_count: 1,
            });
            enc.encode_render_command(rasterizer);
            direct::bind_bytes::<{ stage::VERTEX }>(enc, slot::VERTICES, &vertices);
            for (stage, slot, range) in &binds {
                let bound = &bytes[range.clone()];
                match stage {
                    Stage::Vertex => direct::bind_bytes::<{ stage::VERTEX }>(enc, *slot, bound),
                    Stage::Pixel => direct::bind_bytes::<{ stage::PIXEL }>(enc, *slot, bound),
                }
            }
            // The plain sampler state, not the cube twin: mtld3d's cube samples clamp in the
            // shader as its own binds do (C4b3b-5).
            for (stage, slot, texture, key, sampler) in &textures {
                let (view, sampler) = (Some((&**texture, *key)), sampler.as_ref());
                match stage {
                    Stage::Vertex => {
                        direct::bind_texture::<{ stage::VERTEX }>(enc, *slot, view);
                        direct::bind_sampler::<{ stage::VERTEX }>(enc, *slot, sampler, false);
                    }
                    Stage::Pixel => {
                        direct::bind_texture::<{ stage::PIXEL }>(enc, *slot, view);
                        direct::bind_sampler::<{ stage::PIXEL }>(enc, *slot, sampler, false);
                    }
                }
            }
            if !textures.is_empty() {
                // A texture read in a pass that wrote it waits on a tile barrier.
                enc.resolve_render_pass_barrier(dxmt_core::encoder::Encoder::dispatch_tile_barrier);
            }
            let Some((index_bytes, index_type, index_count)) = &indices else {
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
                return;
            };
            let ring = enc.allocate_argument_buffer(index_bytes.len());
            // SAFETY: the slice's mapping is `index_bytes.len()` bytes the pass alone uses.
            unsafe {
                ring.mapping()
                    .as_ptr()
                    .copy_from_nonoverlapping(index_bytes.as_ptr(), index_bytes.len());
            }
            enc.encode_render_command(WmtcmdRenderDrawIndexed {
                r#type: WMT_RENDER_COMMAND_DRAW_INDEXED,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type,
                index_type: *index_type,
                index_count: u64::from(*index_count),
                index_buffer: ring.buffer(),
                index_buffer_offset: ring.offset(),
                instance_count: 1,
                base_vertex: 0,
                base_instance: 0,
            });
        });
        Ok(())
    }

    /// The pipeline of `key`: its handle, which the device keeps for its life.
    ///
    /// Built on the first draw of `key`, unless its layout crosses the stride; a key that does
    /// not build is kept as handle 0, logged then, and never built again.
    fn pipeline(&mut self, key: &PipelineKey) -> Result<u64, DrawRefused> {
        let entry = self
            .pipelines
            .get(key)
            .unwrap_or_else(|| self.insert_pipeline(key));
        if entry
            .spec
            .as_ref()
            .as_ref()
            .is_some_and(PipelineSource::crosses_stride)
        {
            return Err(DrawRefused::Crossing);
        }
        entry.ready().ok_or(DrawRefused::Pipeline)
    }

    /// The entry of a key the device has not seen: built unless its layout crosses the stride.
    ///
    /// One that does not build is logged and kept as handle 0, never built again.
    fn insert_pipeline(&mut self, key: &PipelineKey) -> Arc<Compiled<Option<PipelineSource>>> {
        let source = pipeline_source(key)
            .map_err(|e| {
                log::debug!(target: LOG_TARGET, "d3d9 core: a vertex stage: {e:?}");
            })
            .ok();
        let crossing = source.as_ref().is_some_and(PipelineSource::crosses_stride);
        let pso = match &source {
            Some(source) if !crossing => build_pipeline(&self.device, source, &mut self.functions),
            _ => None,
        };
        if pso.is_none() && !crossing {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: a pipeline (stride {}) does not build; its draws are dropped",
                key.stride
            );
            log::debug!(target: LOG_TARGET, "d3d9 core: the pipeline that does not build: {key:?}");
        }
        let handle = pso.as_ref().map_or(0, Obj::handle);
        // Built and ready (C4b-1's ruling C4b-3): no job, so no scheduler.
        let entry = Compiled::ready_with(Arc::new(source), handle, pso);
        self.pipelines.insert_ready(key.clone(), entry)
    }

    /// The stage compiles so far.
    #[cfg(test)]
    pub const fn compiles(&self) -> usize {
        self.functions.compiles
    }

    /// The pipelines kept, built or not.
    #[cfg(test)]
    pub fn pipelines_len(&self) -> usize {
        self.pipelines.len()
    }
}

/// The pipeline `source` describes, its stages from `functions` (compiled there on a miss).
///
/// A free function, so a compile job can run it (C4d).
fn build_pipeline(
    device: &Device,
    source: &PipelineSource,
    functions: &mut StageFunctions,
) -> Option<Obj> {
    let vs = functions.function(device, &source.vs, c"d3d9_vs")?;
    let ps = functions.function(device, &source.ps, c"d3d9_ps")?;
    let mut info = dxmt_core::library::render_pipeline_info();
    info.vertex_function = vs;
    info.fragment_function = ps;
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
    // SAFETY: a live device; the info, the vertex descriptor and the error slot are alive for
    // the call; the functions are retained by the pipeline.
    let pso = unsafe {
        MTLDevice_newRenderPipelineStateWithVertexDescriptor(
            device.device().handle(),
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
fn compile_stage(device: &Device, tokens: &[u32], spec: &StageSpec, name: &CStr) -> Option<Obj> {
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
    let bitcode = unsafe { compile(&Entry::Shader, shader.0, 0, (&raw mut common).cast(), name) }
        .map_err(|e| log::error!(target: LOG_TARGET, "d3d9 core: compile: {}", e.message()))
        .ok()?;
    let data = compiled_data(&bitcode)?;
    load_function(device.device(), &data, name).ok()
}

#[cfg(test)]
mod tests;
