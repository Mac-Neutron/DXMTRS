//! One draw on the core (C4-9, C4b-2, DXMTRS C4c-1): its pipeline, built once a key, its records.
//!
//! The front end builds the draw's [`PipelineKey`] and its per-draw data ([`DrawData`]). On a key
//! the device has not seen, [`pipeline_source`] turns it into both stages' DXSO and the vertex
//! layout, each stage is translated through the core's DXSO entry (`initialize_dxso`, then
//! `compile`, `compiled_data` and `load_function`, the D3D10-12 path) unless the device already
//! holds that stage's function for that specialization, and the pipeline is made with the vertex
//! descriptor (slot 156) and kept in the device's [`PsoMap`](dxmt_core::pipeline_cache::PsoMap).
//! The draw records the pipeline, the viewport, the rasterizer state, each vertex buffer slot the
//! pipeline reads, from the stream it binds (inline bytes, zeros, or a stored buffer, its read
//! tracked), the stages' binds and the draw (its indices, if any, in the argument-buffer ring or a
//! stored buffer) into the colour target's render pass, after the depth-stencil state when its key
//! names a depth format, and the scissor when the pass has not recorded that one last. The
//! pipeline is built on the API thread and waited for (no asynchronous compile until C4d).

use core::ffi::CStr;
use std::{borrow::Cow, ops::Range, sync::Arc};

use airconv_ir::dxso::spec::{StageSpec, VARIANT_KEY_WIRE, VS_SPEC_WIRE};
use dxmt_core::{
    deptrack::Access,
    device::Device,
    encoder::{NO_NEXT, direct, stage},
    pipeline_cache::Compiled,
    resource::Buffer,
    shader::{Entry, compile, compiled_data, initialize_dxso, load_function},
};
use mtld3d_core::{
    clear_area::clamp_rect,
    depth_stencil_state::DepthStencilSnapshot,
    draw_data::NULL_STREAM_ZEROS,
    ids::BufferId,
    pipeline_key::{
        MAX_STREAMS, PipelineKey, PipelineSource, ShaderCode, StageSource, StreamKey,
        pipeline_source,
    },
    pipeline_state::{BlendFactor, BlendOperation, CoreBlend},
    streams::{advanced_binding_offset, instance_count, offset_shift, slot_binding_offset},
};
use rustc_hash::FxHashMap;
use winemetal_sys::{
    MTLDevice_newRenderPipelineStateWithVertexDescriptor, Obj,
    sm50::{SM50_SHADER_COMMON, SM50_SHADER_METAL_310, Sm50ShaderCommonData},
    wire::{
        MemPtr, WMT_BLEND_FACTOR_BLEND_COLOR, WMT_BLEND_FACTOR_DESTINATION_ALPHA,
        WMT_BLEND_FACTOR_DESTINATION_COLOR, WMT_BLEND_FACTOR_ONE,
        WMT_BLEND_FACTOR_ONE_MINUS_BLEND_COLOR, WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_ALPHA,
        WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_COLOR, WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA,
        WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_COLOR, WMT_BLEND_FACTOR_SOURCE_ALPHA,
        WMT_BLEND_FACTOR_SOURCE_ALPHA_SATURATED, WMT_BLEND_FACTOR_SOURCE_COLOR,
        WMT_BLEND_FACTOR_ZERO, WMT_BLEND_OPERATION_ADD, WMT_BLEND_OPERATION_MAX,
        WMT_BLEND_OPERATION_MIN, WMT_BLEND_OPERATION_REVERSE_SUBTRACT,
        WMT_BLEND_OPERATION_SUBTRACT, WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_DRAW_INDEXED,
        WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF, WMT_RENDER_COMMAND_SET_DSSO,
        WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_RASTERIZER_STATE,
        WMT_RENDER_COMMAND_SET_SCISSOR_RECTS, WMT_RENDER_COMMAND_SET_VIEWPORTS,
        WMTColorAttachmentBlendInfo, WMTScissorRect, WMTVertexAttributeInfo,
        WMTVertexBufferLayoutInfo, WMTVertexDescriptorInfo, WMTViewport, WmtcmdRenderDraw,
        WmtcmdRenderDrawIndexed, WmtcmdRenderSetblendcolor, WmtcmdRenderSetdsso,
        WmtcmdRenderSetpso, WmtcmdRenderSetrasterizerstate, WmtcmdRenderSetscissorrects,
        WmtcmdRenderSetviewports,
    },
};

use super::{CoreDevice, LOG_TARGET};

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

/// One vertex stream's data for a draw, at each vertex buffer slot that binds the stream.
// Copy: an entry of the draw's array, which the tests' const fixture builds.
#[derive(Clone, Copy)]
pub enum StreamData<'a> {
    /// None: a stream the declaration does not name.
    None,
    /// Bytes bound inline (a UP draw's stream 0).
    Bytes(&'a [u8]),
    /// Zeros: a stream the declaration names with nothing bound.
    Zeros,
    /// A stored buffer ([`CoreDevice::buffer_create`]) read from byte `offset` (DXMTRS C4c-1).
    Buffer { id: BufferId, offset: u32 },
}

/// Where an indexed draw's indices are.
pub enum IndexData<'a> {
    /// Bytes, copied into the argument-buffer ring.
    Bytes(&'a [u8]),
    /// A stored buffer ([`CoreDevice::buffer_create`]), from byte `offset` (DXMTRS C4c-1).
    Buffer { id: BufferId, offset: u64 },
}

/// An indexed draw's indices.
pub struct Indices<'a> {
    pub data: IndexData<'a>,
    /// `MTLIndexType`: 0 for 16-bit, 1 for 32-bit.
    pub index_type: u8,
    pub count: u32,
}

/// What one draw binds and draws, as the front end resolved it.
///
/// A `DrawPrimitiveUP` or a `DrawIndexedPrimitiveUP`, or a draw from stored buffers (DXMTRS
/// C4c-1).
pub struct DrawData<'a> {
    /// The `MTLPrimitiveType`.
    pub primitive: u8,
    /// Each stream's data, by stream (DXMTRS C4c-1).
    pub streams: [StreamData<'a>; MAX_STREAMS as usize],
    pub vertex_count: u32,
    /// The first vertex of a draw without indices.
    pub vertex_start: u32,
    /// What an indexed draw adds to each index.
    pub base_vertex: i32,
    /// Stream 0's frequency word, and the bound streams whose word is instance data (bit `n`).
    ///
    /// None for a draw that never instances. The instance count is mtld3d's: the word's count
    /// when a stream the pipeline reads is in the mask, 1 otherwise.
    pub instancing: Option<(u32, u16)>,
    /// The indices of an indexed draw (a fan's triangle list too).
    pub indices: Option<Indices<'a>>,
    /// `(origin x, origin y, width, height, znear, zfar)`.
    pub viewport: [f64; 6],
    /// The rectangle the draw is cut to (`x, y, width, height`), which the pass's extent clamps.
    ///
    /// The scissor rect with the scissor test on, the drawn viewport with it off.
    pub scissor: [u32; 4],
    /// The `MTLTriangleFillMode`.
    pub fill_mode: u8,
    /// The `MTLCullMode`.
    pub cull_mode: u8,
    pub binds: &'a [Bind<'a>],
    /// The sampled slots' textures and samplers, each at its stage.
    pub textures: &'a [super::TextureBind],
    /// The depth and stencil state of the render states, not yet gated on the attached planes.
    ///
    /// Read only by a draw whose key names a depth format.
    pub depth_stencil: DepthStencilSnapshot,
    /// `D3DRS_STENCILREF`'s low byte.
    pub stencil_ref: u8,
    /// Metal's slope-scaled depth bias (the constant bias is the vertex stage's).
    pub slope_scale: f32,
    /// The blend colour (`D3DRS_BLENDFACTOR` as red, green, blue, alpha).
    ///
    /// For a draw whose key's blend reads it; none otherwise.
    pub blend_color: Option<[f32; 4]>,
}

/// Why a draw recorded nothing.
#[derive(Debug, PartialEq, Eq)]
pub enum DrawRefused {
    /// The colour target does not resolve: the device has no back buffer.
    NoBackBuffer,
    /// The draw's pipeline does not build (logged once per key).
    Pipeline,
    /// A layout no binding can fetch, or a binding it advances past its buffer (DXMTRS C4c-2a).
    ///
    /// The layout: an attribute wider than its stride, or more than 16 attributes under a fetch
    /// (its error logged once per key). mtld3d widens such a layout and draws other bytes.
    Fetch,
    /// A stored buffer the draw reads is not in the store (DXMTRS C4c-1).
    Buffer,
    /// The key names a depth format the bound depth-stencil has not (none is bound).
    ///
    /// Or the bound one is of another sample count than target 0, which no pass attaches beside
    /// it (DXMTRS C4b-3d5).
    Depth,
    /// The key names colour formats the bound targets do not make, or another sample count.
    ///
    /// Its colours are those of the pass set ([`CoreDevice::target_desc`]) under neither sRGB
    /// write (DXMTRS C4b-3d4); its count is target 0's (DXMTRS C4b-3d5).
    Target,
}

/// Where a recorded indexed draw reads its indices.
enum IndexFeed {
    /// Bytes the closure copies into the argument-buffer ring.
    Ring(Vec<u8>),
    /// A stored buffer, from a byte offset.
    Stored(Arc<Buffer>, u64),
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
    /// Records `draw` with the pipeline of `key` into the render pass of the targets it names.
    ///
    /// # Errors
    ///
    /// Nothing recorded when colour target 0 does not resolve (a device with no back buffer),
    /// the key names colour formats the bound targets do not make, another sample count than
    /// target 0's, or a depth format the bound depth-stencil has not or one of another count,
    /// no binding can fetch the layout, the pipeline does not build, a stored buffer it reads is
    /// not in the store, or a slot advanced past a crossing attribute's offset starts past its
    /// buffer. A draw whose scissor holds no pixel of the pass records nothing and is no error.
    pub fn draw(&mut self, key: &PipelineKey, draw: &DrawData<'_>) -> Result<(), DrawRefused> {
        let Some(target) = self.shape(self.target) else {
            return Err(DrawRefused::NoBackBuffer);
        };
        // A pass and its pipeline share one sample count, target 0's, whatever pass is open: an
        // open pass on the same colours does not excuse another count (DXMTRS C4b-3d5).
        if key.samples != target.samples {
            return Err(DrawRefused::Target);
        }
        // The key decides the pass's colour slots and formats. An open pass on them was checked
        // when it opened, and a change of any target closed it; otherwise the pass set must make
        // them, under one sRGB write or the other.
        let colors = key.color_formats();
        let open = self.render_open && self.open_colors == colors;
        if !open
            && [false, true]
                .into_iter()
                .all(|srgb| self.target_desc(srgb).map(|(_, formats)| formats) != Some(colors))
        {
            return Err(DrawRefused::Target);
        }
        // The key decides the pass: with the depth-stencil exactly when it names a format. An
        // open pass with it was checked when it opened, and a change of either binding closed it.
        let with_depth = key.depth_format != 0;
        let bound = || self.depth_format() == Some(key.depth_format);
        if with_depth && self.open_depth.is_none() && !bound() {
            return Err(DrawRefused::Depth);
        }
        // A depth-stencil of another count than target 0 is left out of the pass: a key that
        // names it is not the draw's (DXMTRS C4b-3d5).
        if with_depth && self.depth_samples() != Some(target.samples) {
            return Err(DrawRefused::Depth);
        }
        debug_assert!(
            !with_depth || bound(),
            "a draw's key names the bound depth-stencil's format"
        );
        // Metal takes no scissor past the pass's extent, nor an empty one: a draw cut to nothing
        // writes no pixel, so it records nothing.
        let extent = self
            .pass_extent(with_depth)
            .unwrap_or((target.width, target.height));
        let Some(scissor) = clamp_rect(draw.scissor, extent) else {
            return Ok(());
        };
        let entry = self.pipeline(key)?;
        let (Some(pso), Some(source)) = (entry.ready(), entry.spec.as_ref().as_ref()) else {
            return Err(DrawRefused::Pipeline);
        };
        // The streams the pipeline reads: bit `n` for stream `n`.
        let used = (0_u16..)
            .zip(&source.streams)
            .filter(|(_, layout)| layout.is_used())
            .fold(0_u16, |mask, (stream, _)| mask | 1 << stream);
        // Before anything is recorded, each vertex buffer slot the pipeline reads, from the stream
        // it binds, past that stream's offset by its advance (mtld3d's fetch): a stored buffer at
        // the slot's binding offset, refused when the store does not hold it or an advanced
        // binding starts past its end; inline bytes from the advance, zero-filled to their length;
        // or for a stream with nothing bound zeros of its stride, clamped to the zero feed as
        // mtld3d clamps it.
        let mut stored = Vec::new();
        let mut bytes = Vec::new();
        let mut binds = Vec::<(Stage, u8, Range<usize>)>::new();
        for ((slot, layout), &(stream, advance)) in
            (0_u8..).zip(&source.layouts).zip(&source.bindings)
        {
            if !layout.is_used() {
                continue;
            }
            let fed = match draw.streams[usize::from(stream)] {
                StreamData::Buffer { id, offset } => {
                    let buffer = self.buffers.buffer(id).ok_or(DrawRefused::Buffer)?;
                    debug_assert!(
                        matches!(
                            key.streams[usize::from(stream)],
                            StreamKey::Bound { shift: s, .. } if offset_shift(offset) == s.into()
                        ),
                        "a bound stream's key carries its offset's shift"
                    );
                    if advance != 0
                        && advanced_binding_offset(offset, advance, buffer.length()).is_err()
                    {
                        return Err(DrawRefused::Fetch);
                    }
                    let at = slot_binding_offset(offset, advance);
                    stored.push((slot, Arc::clone(buffer), u64::from(at)));
                    continue;
                }
                StreamData::Bytes(fed) => slot_bytes(fed, advance),
                data @ (StreamData::Zeros | StreamData::None) => {
                    debug_assert!(
                        matches!(data, StreamData::Zeros),
                        "a stream the pipeline reads has data"
                    );
                    debug_assert!(advance == 0, "a stream with nothing bound never crosses");
                    let stride = source.streams[usize::from(stream)].stride as usize;
                    Cow::Borrowed(&NULL_STREAM_ZEROS[..stride.min(NULL_STREAM_ZEROS.len())])
                }
            };
            let start = bytes.len();
            bytes.extend_from_slice(&fed);
            binds.push((Stage::Vertex, slot, start..bytes.len()));
        }
        let indexed = match &draw.indices {
            None => None,
            Some(Indices {
                data,
                index_type,
                count,
            }) => {
                let feed = match *data {
                    IndexData::Bytes(bytes) => IndexFeed::Ring(bytes.to_vec()),
                    IndexData::Buffer { id, offset } => {
                        let buffer = self.buffers.buffer(id).ok_or(DrawRefused::Buffer)?;
                        IndexFeed::Stored(Arc::clone(buffer), offset)
                    }
                };
                Some((feed, *index_type, *count))
            }
        };
        // mtld3d's instance count: stream 0's, when a stream the pipeline reads is per instance.
        let instances = draw.instancing.map_or(1, |(word, instanced)| {
            instance_count(word, instanced & used != 0)
        });
        // The state of the planes the pass has: no stencil test without the plane.
        let depth_state = with_depth.then(|| {
            let has_stencil = super::depth::planes(key.depth_format) & 2 != 0;
            let gated = draw.depth_stencil.gated_on_stencil_attachment(has_stencil);
            self.depth_state_handle(&gated)
        });
        let stencil_ref = draw.stencil_ref;
        let blend_color = draw.blend_color;
        self.open_pass(with_depth, colors);
        // After the pass is open: `open_pass` ends one of the other depth kind, and a new pass
        // has recorded none. Once a pass and on a change, not once a draw.
        let scissor = (self.pass_scissor != Some(scissor)).then(|| {
            self.pass_scissor = Some(scissor);
            WMTScissorRect {
                x: u64::from(scissor[0]),
                y: u64::from(scissor[1]),
                width: u64::from(scissor[2]),
                height: u64::from(scissor[3]),
            }
        });
        for bind in draw.binds.iter().filter(|b| !b.bytes.is_empty()) {
            let start = bytes.len();
            bytes.extend_from_slice(bind.bytes);
            binds.push((bind.stage, bind.slot, start..bytes.len()));
        }
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
            scole_scale: draw.slope_scale,
            depth_bias_clamp: 0.0,
        };
        let (primitive_type, vertex_count) = (draw.primitive, draw.vertex_count);
        let (vertex_start, base_vertex) = (draw.vertex_start, draw.base_vertex);
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
            if let Some(dsso) = depth_state {
                enc.encode_render_command(WmtcmdRenderSetdsso {
                    r#type: WMT_RENDER_COMMAND_SET_DSSO,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    dsso,
                    stencil_ref,
                });
            }
            if let Some([red, green, blue, alpha]) = blend_color {
                // The record that carries the blend colour also sets the stencil reference: the
                // draw's own, which the depth-stencil state just set.
                enc.encode_render_command(WmtcmdRenderSetblendcolor {
                    r#type: WMT_RENDER_COMMAND_SET_BLEND_FACTOR_AND_STENCIL_REF,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    red,
                    green,
                    blue,
                    alpha,
                    stencil_ref,
                });
            }
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
            if let Some(rect) = scissor {
                let slot = enc
                    .allocate_cpu(size_of::<WMTScissorRect>(), align_of::<WMTScissorRect>())
                    .cast::<WMTScissorRect>();
                // SAFETY: the encoder's memory, sized and aligned for one rect, nobody else's.
                unsafe { slot.write(rect) };
                enc.encode_render_command(WmtcmdRenderSetscissorrects {
                    r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECTS,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    scissor_rects: MemPtr::from_ptr(slot.as_ptr().cast::<u8>()),
                    rect_count: 1,
                });
            }
            enc.encode_render_command(rasterizer);
            for (slot, buffer, offset) in &stored {
                direct::bind_buffer::<{ stage::VERTEX }>(enc, *slot, Some(buffer), *offset);
            }
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
            let Some((feed, index_type, index_count)) = &indexed else {
                enc.encode_render_command(WmtcmdRenderDraw {
                    r#type: WMT_RENDER_COMMAND_DRAW,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    primitive_type,
                    vertex_start: u64::from(vertex_start),
                    vertex_count: u64::from(vertex_count),
                    instance_count: instances,
                    base_instance: 0,
                });
                return;
            };
            let (index_buffer, index_buffer_offset) = match feed {
                IndexFeed::Ring(index_bytes) => {
                    let ring = enc.allocate_argument_buffer(index_bytes.len());
                    // SAFETY: the slice's mapping is `index_bytes.len()` bytes the pass alone
                    // uses.
                    unsafe {
                        ring.mapping()
                            .as_ptr()
                            .copy_from_nonoverlapping(index_bytes.as_ptr(), index_bytes.len());
                    }
                    (ring.buffer(), ring.offset())
                }
                IndexFeed::Stored(buffer, offset) => {
                    // A stored buffer has its allocation from its creation on.
                    let Some((allocation, at)) =
                        enc.access_buffer::<{ stage::VERTEX }>(buffer, Access::READ)
                    else {
                        return;
                    };
                    (allocation.buffer(), at + offset)
                }
            };
            enc.encode_render_command(WmtcmdRenderDrawIndexed {
                r#type: WMT_RENDER_COMMAND_DRAW_INDEXED,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type,
                index_type: *index_type,
                index_count: u64::from(*index_count),
                index_buffer,
                index_buffer_offset,
                instance_count: instances,
                base_vertex,
                base_instance: 0,
            });
        });
        // After the record: a lock that renames from here on leaves this draw its name.
        self.stamp_dynamic(draw);
        Ok(())
    }

    /// The pipeline entry of `key`, which the device keeps for its life: its handle and source.
    ///
    /// Built on the first draw of `key`, unless no binding can fetch its layout
    /// ([`refused_layout`]); a key that does not build is kept as handle 0, logged then, and never
    /// built again.
    fn pipeline(
        &mut self,
        key: &PipelineKey,
    ) -> Result<Arc<Compiled<Option<PipelineSource>>>, DrawRefused> {
        let entry = self
            .pipelines
            .get(key)
            .unwrap_or_else(|| self.insert_pipeline(key));
        if entry.spec.as_ref().as_ref().is_some_and(refused_layout) {
            return Err(DrawRefused::Fetch);
        }
        Ok(entry)
    }

    /// The entry of a key the device has not seen: built unless no binding can fetch its layout.
    ///
    /// One that does not build is logged and kept as handle 0, never built again; one no binding
    /// can fetch is kept as handle 0 too, its fetch error logged.
    fn insert_pipeline(&mut self, key: &PipelineKey) -> Arc<Compiled<Option<PipelineSource>>> {
        let source = pipeline_source(key)
            .map_err(|e| {
                log::debug!(target: LOG_TARGET, "d3d9 core: a vertex stage: {e:?}");
            })
            .ok();
        if let Some(Err(e)) = source.as_ref().map(|source| &source.fetch) {
            log::debug!(
                target: LOG_TARGET,
                "d3d9 core: no binding fetches a vertex layout ({e:?}); its draws are refused"
            );
        }
        let refused = source.as_ref().is_some_and(refused_layout);
        let pso = match &source {
            Some(source) if !refused => build_pipeline(&self.device, source, &mut self.functions),
            _ => None,
        };
        if pso.is_none() && !refused {
            let streams = key
                .streams
                .iter()
                .filter(|s| **s != StreamKey::Unused)
                .collect::<Vec<_>>();
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: a pipeline (streams {streams:?}) does not build; its draws are dropped"
            );
            log::debug!(target: LOG_TARGET, "d3d9 core: the pipeline that does not build: {key:?}");
        }
        // A stream with nothing bound reads the zero feed, which its draws clamp to (as mtld3d's
        // do): said once a key.
        let zeros = NULL_STREAM_ZEROS.len();
        for (stream, (entry, layout)) in source
            .iter()
            .flat_map(|source| key.streams.iter().zip(&source.streams))
            .enumerate()
        {
            if *entry == StreamKey::Null && layout.stride as usize > zeros {
                log::warn!(
                    target: LOG_TARGET,
                    "d3d9 core: stream {stream} is unbound and its declaration extent {} exceeds \
                     the zero feed; clamped",
                    layout.stride
                );
            }
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

/// Whether the core refuses `source`'s layout: no binding can fetch it.
const fn refused_layout(source: &PipelineSource) -> bool {
    !source.fetchable()
}

/// The bytes an inline stream binds at a slot advanced `advance` bytes past it.
///
/// `fed` from byte `advance` on, zero-filled to `fed`'s length (all zeros from the end on); `fed`
/// itself at no advance. Vertex `i` of a slot reads `i * stride` bytes in, and an attribute fits
/// its stride, so the slot reads the bytes mtld3d's payload holds from the advance: the stream's,
/// then the zeros it pads past the last vertex.
fn slot_bytes(fed: &[u8], advance: u32) -> Cow<'_, [u8]> {
    if advance == 0 {
        return Cow::Borrowed(fed);
    }
    let read = usize::try_from(advance)
        .ok()
        .and_then(|advance| fed.get(advance..))
        .unwrap_or_default();
    let mut bytes = read.to_vec();
    bytes.resize(fed.len(), 0);
    Cow::Owned(bytes)
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
    info.colors[0].pixel_format = source.color_format;
    info.colors[0].write_mask = source.write_mask;
    if let Some(blend) = &source.blend {
        set_blend(&mut info.colors[0], blend);
    }
    // Slots 1-3 at their own index; an empty one keeps the invalid format, as its pass's slot
    // holds no attachment.
    for (color, extra) in info.colors[1..].iter_mut().zip(&source.extras) {
        let Some(extra) = extra else {
            continue;
        };
        color.pixel_format = extra.format;
        color.write_mask = extra.write_mask;
        if let Some(blend) = &extra.blend {
            set_blend(color, blend);
        }
    }
    // The planes of the pass the key draws in: both formats are the combined format's.
    let planes = super::depth::planes(source.depth_format);
    if planes & 1 != 0 {
        info.depth_pixel_format = source.depth_format;
    }
    if planes & 2 != 0 {
        info.stencil_pixel_format = source.depth_format;
    }
    // The pass's count, and coverage from alpha on a multisampled one (DXMTRS C4b-3d5).
    info.raster_sample_count = source.samples;
    info.alpha_to_coverage_enabled = u8::from(source.alpha_to_coverage);
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
    for &(index, format, offset, slot) in &source.attrs {
        if let Some(attribute) = vertex.attributes.get_mut(index as usize) {
            *attribute = WMTVertexAttributeInfo {
                format,
                offset,
                buffer_index: slot,
            };
        }
    }
    for (wire, layout) in vertex.layouts.iter_mut().zip(&source.layouts) {
        if layout.is_used() {
            *wire = WMTVertexBufferLayoutInfo {
                stride: layout.stride,
                // mtld3d's step function has Metal's numbers: Constant 0, PerVertex 1,
                // PerInstance 2 (the wire names none).
                step_function: layout.step as u32,
                step_rate: layout.step_rate,
            };
        }
    }
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

/// Turns blending on in `color`, with `blend`'s factors and operations in the wire's numbers.
const fn set_blend(color: &mut WMTColorAttachmentBlendInfo, blend: &CoreBlend) {
    color.blending_enabled = 1;
    color.src_rgb_blend_factor = wire_factor(blend.src);
    color.dst_rgb_blend_factor = wire_factor(blend.dst);
    color.rgb_blend_operation = wire_op(blend.op);
    color.src_alpha_blend_factor = wire_factor(blend.src_alpha);
    color.dst_alpha_blend_factor = wire_factor(blend.dst_alpha);
    color.alpha_blend_operation = wire_op(blend.op_alpha);
}

/// The wire's `WMT_BLEND_FACTOR_*` of mtld3d's blend factor.
///
/// Name by name: the two enums number destination colour and alpha the other way round.
const fn wire_factor(factor: BlendFactor) -> u8 {
    match factor {
        BlendFactor::Zero => WMT_BLEND_FACTOR_ZERO,
        BlendFactor::One => WMT_BLEND_FACTOR_ONE,
        BlendFactor::SourceColor => WMT_BLEND_FACTOR_SOURCE_COLOR,
        BlendFactor::OneMinusSourceColor => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_COLOR,
        BlendFactor::SourceAlpha => WMT_BLEND_FACTOR_SOURCE_ALPHA,
        BlendFactor::OneMinusSourceAlpha => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA,
        BlendFactor::DestinationAlpha => WMT_BLEND_FACTOR_DESTINATION_ALPHA,
        BlendFactor::OneMinusDestinationAlpha => WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_ALPHA,
        BlendFactor::DestinationColor => WMT_BLEND_FACTOR_DESTINATION_COLOR,
        BlendFactor::OneMinusDestinationColor => WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_COLOR,
        BlendFactor::SourceAlphaSaturated => WMT_BLEND_FACTOR_SOURCE_ALPHA_SATURATED,
        BlendFactor::BlendColor => WMT_BLEND_FACTOR_BLEND_COLOR,
        BlendFactor::OneMinusBlendColor => WMT_BLEND_FACTOR_ONE_MINUS_BLEND_COLOR,
    }
}

/// The wire's `WMT_BLEND_OPERATION_*` of mtld3d's blend operation.
const fn wire_op(op: BlendOperation) -> u8 {
    match op {
        BlendOperation::Add => WMT_BLEND_OPERATION_ADD,
        BlendOperation::Subtract => WMT_BLEND_OPERATION_SUBTRACT,
        BlendOperation::ReverseSubtract => WMT_BLEND_OPERATION_REVERSE_SUBTRACT,
        BlendOperation::Min => WMT_BLEND_OPERATION_MIN,
        BlendOperation::Max => WMT_BLEND_OPERATION_MAX,
    }
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
