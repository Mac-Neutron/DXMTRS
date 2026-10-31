//! DXIL's signature handlers, as data.
//!
//! What each entry point element asks of the prologue and epilogue, ported from
//! `A/dxil/dxil_signature.cpp`. Each element defines the function inputs and outputs the C++
//! defined at Initialize, grows the register counts, and records a handler that holds what its
//! C++ lambda captured, in the C++'s order; the compile replays them. These are the DXIL front
//! end's 18 lambdas, in their own enums beside the DXBC ones of `airconv_ir::handlers` (which
//! replay through other helpers: the varyings here are shifted to their signature's width).
//! Varyings link by semantic name, so an element's name becomes a function element's name.

use std::collections::{BTreeMap, BTreeSet};

use airconv_ir::binding::{
    DepthArgument, FunctionInput, FunctionOutput, FunctionSignatureDesc, Interpolation, MeshData,
    MeshPrimitiveOutput, MeshVertexOutput, MslScalar, MslValue, MslVector,
};

use crate::{
    api::Error,
    dxil::types::{
        EntryInfo, SignatureElement, UNPACKED_INSTANCE_ID, UNPACKED_VERTEX_ID, component_type,
        interpolation, op, semantic_kind, shader_kind, user_name,
    },
};

/// A signature handler (`std::function<void(SignatureContext &)>`), as what its lambda captured.
///
/// `reg` and `mask` are the element's register and components (x in bit 0); `col` and `cols` its
/// first component and width; `index` an element of the function signature defined at
/// Initialize.
pub enum SignatureHandler {
    /// A compute shader's 2D quads.
    ///
    /// Metal's flat thread index (argument `thread_index`) and group (argument `group`) remapped
    /// so that Metal's lane quads run D3D's 2x2 quads.
    ComputeQuads {
        thread_index: u32,
        group: u32,
        numthreads: [u32; 3],
    },
    /// Argument `index` stored into one of the IO map's special registers (`DefineSpecial`).
    Special { index: u32, register: Special },
    /// A vertex shader's vertex ID (`vertex`) or instance ID stored to input `reg`.
    VertexSystemValue { reg: u32, mask: u32, vertex: bool },
    /// A vertex input row.
    ///
    /// Pulled through the compile's input layout, else a stage-in attribute named `name`.
    /// `component_type` is `InputAttributeComponentType`'s number.
    VertexInput {
        reg: u32,
        mask: u32,
        component_type: u32,
        name: String,
    },
    /// The position output `reg` into element `index`, sanitized, unless vertex output is skipped.
    PopPosition { reg: u32, mask: u32, index: u32 },
    /// Output `reg` into element `index`, unless vertex output is skipped.
    PopOutput { reg: u32, mask: u32, index: u32 },
    /// A varying row: components `[col, col + cols)` of output `reg` into element `index`.
    PopShifted {
        reg: u32,
        col: u32,
        cols: u32,
        index: u32,
    },
    /// Input `reg` from argument `index`; `fix_w` for the position.
    InitInput {
        index: u32,
        reg: u32,
        mask: u32,
        fix_w: bool,
    },
    /// A varying row: argument `index` into components `[col, col + cols)` of input `reg`.
    InitShifted {
        index: u32,
        reg: u32,
        col: u32,
        cols: u32,
    },
    /// Render target `target`, defined at compile time from its pixel format.
    ///
    /// Where the format says nothing, the signature's `component_type` (a
    /// `RegisterComponentType` number).
    RenderTarget {
        reg: u32,
        mask: u32,
        target: u32,
        component_type: u32,
    },
    /// The depth output, unless the compile disables it.
    Depth { argument: DepthArgument },
    /// The coverage mask output, element `index`.
    Coverage { index: u32 },
    /// The stencil reference output, element `index`.
    StencilRef { index: u32 },
}

/// The IO map's special registers a compute shader's IDs go to.
pub enum Special {
    ThreadId,
    ThreadGroupId,
    ThreadIdInGroup,
    ThreadIdInGroupFlat,
}

/// A geometry shader's output handler (`std::function<IREffect(MeshOutputContext &)>`).
pub enum MeshOutputHandler {
    /// Output `reg` as the vertex's position.
    Position { reg: u32, mask: u32 },
    /// Output `reg` as the primitive's render target array index.
    RenderTargetArrayIndex { reg: u32, mask: u32 },
    /// Output `reg` as the primitive's viewport array index.
    ViewportArrayIndex { reg: u32, mask: u32 },
    /// Components `[col, col + cols)` of output `reg` as the vertex's user data.
    ///
    /// User data `index`, of type `ty`.
    Shifted {
        reg: u32,
        col: u32,
        cols: u32,
        index: u32,
        ty: MslValue,
    },
}

/// What the signature handlers set on the shader (`SM50ShaderInternal`'s fields).
#[derive(Default)]
pub struct Io {
    pub func_signature: FunctionSignatureDesc,
    pub signature_handlers: Vec<SignatureHandler>,
    pub mesh_output_handlers: Vec<MeshOutputHandler>,
    pub max_input_register: u32,
    pub max_output_register: u32,
    pub pso_valid_output_reg_mask: u32,
    pub ps_has_coverage_output: u32,
    pub num_mesh_vertex_data: u32,
    /// Input elements that are in no register (unpacked system values).
    ///
    /// Element ID to function argument, or [`UNPACKED_VERTEX_ID`] / [`UNPACKED_INSTANCE_ID`].
    pub unpacked_inputs: BTreeMap<u32, u32>,
}

/// A name that is not UTF-8, which a function element's name must be.
pub const NAME_NOT_UTF8: &[u8] = b"DXIL: a signature element's name is not UTF-8";

impl Io {
    /// `AddSignatureHandlers`: the stage's inputs and outputs.
    ///
    /// `ops` are the opcodes the module calls; `quads_2d` remaps a compute shader's thread IDs;
    /// `flat_index` defines its flattened thread index even when no call reads it (the 64-bit
    /// groupshared lock array's zeroing reads it).
    ///
    /// # Errors
    ///
    /// `DXIL: <what> not supported` for an element the stage cannot take, [`NAME_NOT_UTF8`].
    pub fn add_signature_handlers(
        &mut self,
        entry: &EntryInfo,
        ops: &BTreeSet<u32>,
        quads_2d: bool,
        flat_index: bool,
    ) -> Result<(), Error> {
        match entry.kind {
            shader_kind::COMPUTE => {
                self.add_compute(entry, ops, quads_2d, flat_index);
                Ok(())
            }
            shader_kind::VERTEX => self.add_vertex(entry),
            shader_kind::PIXEL => self.add_pixel(entry),
            shader_kind::GEOMETRY => self.add_geometry(entry),
            _ => Err(unsupported(b"this shader stage")),
        }
    }

    /// `AddWaveInputs`: the simdgroup lane index and size.
    ///
    /// Defined when the module uses wave operations (or, in a compute shader, derivatives,
    /// implicit-LOD samples or LODs, which read the quad lane). Answers the two arguments,
    /// `u32::MAX` each without them.
    pub fn add_wave_inputs(&mut self, ops: &BTreeSet<u32>, program_type: u32) -> (u32, u32) {
        let calls = |list: &[u32]| list.iter().any(|o| ops.contains(o));
        let waves = calls(&[
            op::WAVE_IS_FIRST_LANE,
            op::WAVE_GET_LANE_INDEX,
            op::WAVE_GET_LANE_COUNT,
            op::WAVE_ANY_TRUE,
            op::WAVE_ALL_TRUE,
            op::WAVE_ACTIVE_ALL_EQUAL,
            op::WAVE_ACTIVE_BALLOT,
            op::WAVE_READ_LANE_AT,
            op::WAVE_READ_LANE_FIRST,
            op::WAVE_ACTIVE_OP,
            op::WAVE_ACTIVE_BIT,
            op::WAVE_PREFIX_OP,
            op::WAVE_ALL_BIT_COUNT,
            op::WAVE_PREFIX_BIT_COUNT,
            op::WAVE_MATCH,
            op::WAVE_MULTI_PREFIX_OP,
            op::WAVE_MULTI_PREFIX_BIT_COUNT,
        ]);
        let quads = program_type == shader_kind::COMPUTE
            && calls(&[
                op::SAMPLE,
                op::SAMPLE_BIAS,
                op::SAMPLE_CMP,
                op::SAMPLE_CMP_BIAS,
                op::CALCULATE_LOD,
                op::DERIV_COARSE_X,
                op::DERIV_COARSE_Y,
                op::DERIV_FINE_X,
                op::DERIV_FINE_Y,
            ]);
        if !(waves || quads) {
            return (u32::MAX, u32::MAX);
        }
        let fs = &mut self.func_signature;
        (
            fs.define_input(FunctionInput::ThreadIndexInSimdgroup),
            fs.define_input(FunctionInput::ThreadsPerSimdgroup),
        )
    }

    /// Compute IDs are no signature elements in DXIL: the inputs the module's calls read.
    fn add_compute(
        &mut self,
        entry: &EntryInfo,
        ops: &BTreeSet<u32>,
        quads_2d: bool,
        flat_index: bool,
    ) {
        let fs = &mut self.func_signature;
        if quads_2d {
            let thread_index = fs.define_input(FunctionInput::ThreadIndexInThreadgroup);
            let group = fs.define_input(FunctionInput::ThreadgroupPositionInGrid);
            self.signature_handlers
                .push(SignatureHandler::ComputeQuads {
                    thread_index,
                    group,
                    numthreads: entry.numthreads,
                });
            return;
        }
        for (wanted, input, register) in [
            (
                ops.contains(&op::THREAD_ID),
                FunctionInput::ThreadPositionInGrid,
                Special::ThreadId,
            ),
            (
                ops.contains(&op::GROUP_ID),
                FunctionInput::ThreadgroupPositionInGrid,
                Special::ThreadGroupId,
            ),
            (
                ops.contains(&op::THREAD_ID_IN_GROUP),
                FunctionInput::ThreadPositionInThreadgroup,
                Special::ThreadIdInGroup,
            ),
            (
                ops.contains(&op::FLATTENED_THREAD_ID_IN_GROUP) || flat_index,
                FunctionInput::ThreadIndexInThreadgroup,
                Special::ThreadIdInGroupFlat,
            ),
        ] {
            if wanted {
                let index = self.func_signature.define_input(input);
                self.signature_handlers
                    .push(SignatureHandler::Special { index, register });
            }
        }
    }

    fn add_vertex(&mut self, entry: &EntryInfo) -> Result<(), Error> {
        for e in &entry.inputs {
            if matches!(
                e.kind,
                semantic_kind::VERTEX_ID | semantic_kind::INSTANCE_ID
            ) {
                let vertex = e.kind == semantic_kind::VERTEX_ID;
                if e.start_row < 0 {
                    // Read from the IO map's vertex and instance IDs (the converter computes them).
                    let id = if vertex {
                        UNPACKED_VERTEX_ID
                    } else {
                        UNPACKED_INSTANCE_ID
                    };
                    self.unpacked_inputs.insert(e.id, id);
                    continue;
                }
                self.signature_handlers
                    .push(SignatureHandler::VertexSystemValue {
                        reg: e.start_row.cast_unsigned(),
                        mask: mask(e),
                        vertex,
                    });
                grow(&mut self.max_input_register, e);
                continue;
            }
            if e.kind != semantic_kind::ARBITRARY {
                return Err(unsupported_element(b"vertex shader input ", e));
            }
            for r in 0..e.rows {
                // As DCL_INPUT in handle_signature_vs.
                self.signature_handlers.push(SignatureHandler::VertexInput {
                    reg: e.start_row.cast_unsigned().wrapping_add(r),
                    mask: mask(e),
                    component_type: register_type(e.ty),
                    name: utf8(user_name(e, r))?,
                });
            }
            grow(&mut self.max_input_register, e);
        }
        for e in &entry.outputs {
            let (reg, mask) = (e.start_row.cast_unsigned(), mask(e));
            let fs = &mut self.func_signature;
            match e.kind {
                semantic_kind::POSITION => {
                    let index = fs.define_output(FunctionOutput::Position {
                        ty: MslValue::FLOAT4,
                    });
                    self.signature_handlers.push(SignatureHandler::PopPosition {
                        reg,
                        mask,
                        index,
                    });
                }
                semantic_kind::RENDER_TARGET_ARRAY_INDEX | semantic_kind::VIEWPORT_ARRAY_INDEX => {
                    let index =
                        fs.define_output(if e.kind == semantic_kind::RENDER_TARGET_ARRAY_INDEX {
                            FunctionOutput::RenderTargetArrayIndex
                        } else {
                            FunctionOutput::ViewportArrayIndex
                        });
                    self.signature_handlers
                        .push(SignatureHandler::PopOutput { reg, mask, index });
                }
                semantic_kind::ARBITRARY => {
                    for r in 0..e.rows {
                        let (user, ty) = varying(e, r)?;
                        let index = self
                            .func_signature
                            .define_output(FunctionOutput::Vertex { user, ty });
                        self.signature_handlers.push(SignatureHandler::PopShifted {
                            reg: reg.wrapping_add(r),
                            col: e.start_col.cast_unsigned(),
                            cols: e.cols,
                            index,
                        });
                    }
                }
                _ => return Err(unsupported_element(b"vertex shader output ", e)),
            }
            grow(&mut self.max_output_register, e);
        }
        Ok(())
    }

    fn add_pixel(&mut self, entry: &EntryInfo) -> Result<(), Error> {
        for e in &entry.inputs {
            let (reg, mask) = (e.start_row.cast_unsigned(), mask(e));
            let fs = &mut self.func_signature;
            match e.kind {
                semantic_kind::POSITION => {
                    let index = fs.define_input(FunctionInput::Position {
                        interpolation: air_interpolation(e.interpolation),
                    });
                    self.signature_handlers.push(SignatureHandler::InitInput {
                        index,
                        reg,
                        mask,
                        fix_w: true,
                    });
                }
                semantic_kind::IS_FRONT_FACE
                | semantic_kind::SAMPLE_INDEX
                | semantic_kind::PRIMITIVE_ID
                | semantic_kind::COVERAGE
                | semantic_kind::RENDER_TARGET_ARRAY_INDEX
                | semantic_kind::VIEWPORT_ARRAY_INDEX => {
                    let index = fs.define_input(match e.kind {
                        semantic_kind::IS_FRONT_FACE => FunctionInput::FrontFacing,
                        semantic_kind::SAMPLE_INDEX => FunctionInput::SampleIndex,
                        semantic_kind::PRIMITIVE_ID => FunctionInput::PrimitiveId,
                        semantic_kind::RENDER_TARGET_ARRAY_INDEX => {
                            FunctionInput::RenderTargetArrayIndex
                        }
                        semantic_kind::VIEWPORT_ARRAY_INDEX => FunctionInput::ViewportArrayIndex,
                        _ => FunctionInput::InputCoverage,
                    });
                    if e.start_row < 0 {
                        self.unpacked_inputs.insert(e.id, index);
                    } else {
                        self.signature_handlers.push(SignatureHandler::InitInput {
                            index,
                            reg,
                            mask,
                            fix_w: false,
                        });
                    }
                }
                semantic_kind::ARBITRARY => {
                    for r in 0..e.rows {
                        let (user, ty) = varying(e, r)?;
                        let index =
                            self.func_signature
                                .define_input(FunctionInput::FragmentStageIn {
                                    user,
                                    ty,
                                    interpolation: air_interpolation(e.interpolation),
                                    pull_mode: false,
                                });
                        self.signature_handlers.push(SignatureHandler::InitShifted {
                            index,
                            reg: reg.wrapping_add(r),
                            col: e.start_col.cast_unsigned(),
                            cols: e.cols,
                        });
                    }
                }
                _ => return Err(unsupported_element(b"pixel shader input ", e)),
            }
            grow(&mut self.max_input_register, e);
        }
        for e in &entry.outputs {
            let (reg, mask) = (e.start_row.cast_unsigned(), mask(e));
            let fs = &mut self.func_signature;
            match e.kind {
                // As DCL_OUTPUT in handle_signature_ps, the semantic index as the target.
                semantic_kind::TARGET => {
                    let target = e.semantic_indices.first().copied().unwrap_or(0);
                    self.pso_valid_output_reg_mask |= 1u32.wrapping_shl(target);
                    self.signature_handlers
                        .push(SignatureHandler::RenderTarget {
                            reg,
                            mask,
                            target,
                            component_type: register_type(e.ty),
                        });
                }
                // As the OUTPUT_DEPTH case of handle_signature_ps.
                semantic_kind::DEPTH
                | semantic_kind::DEPTH_LESS_EQUAL
                | semantic_kind::DEPTH_GREATER_EQUAL => {
                    let argument = match e.kind {
                        semantic_kind::DEPTH_GREATER_EQUAL => DepthArgument::Greater,
                        semantic_kind::DEPTH_LESS_EQUAL => DepthArgument::Less,
                        _ => DepthArgument::Any,
                    };
                    self.signature_handlers
                        .push(SignatureHandler::Depth { argument });
                }
                semantic_kind::COVERAGE => {
                    let index = fs.define_output(FunctionOutput::CoverageMask);
                    self.signature_handlers
                        .push(SignatureHandler::Coverage { index });
                    self.ps_has_coverage_output = 1;
                }
                semantic_kind::STENCIL_REF => {
                    let index = fs.define_output(FunctionOutput::StencilRef);
                    self.signature_handlers
                        .push(SignatureHandler::StencilRef { index });
                }
                _ => return Err(unsupported_element(b"pixel shader output ", e)),
            }
            grow(&mut self.max_output_register, e);
        }
        Ok(())
    }

    /// A geometry shader's elements.
    ///
    /// Outputs become the mesh function's vertex and primitive outputs, written at each emit;
    /// inputs come from the vertex stage's registers in the payload (the lowering's loads).
    fn add_geometry(&mut self, entry: &EntryInfo) -> Result<(), Error> {
        // In registers: relinked to the vertex outputs; unpacked: only these two IDs.
        if let Some(e) = entry.inputs.iter().find(|e| {
            e.start_row < 0
                && e.kind != semantic_kind::PRIMITIVE_ID
                && e.kind != semantic_kind::GS_INSTANCE_ID
        }) {
            return Err(unsupported_element(b"geometry shader input ", e));
        }
        for e in &entry.outputs {
            let (reg, mask) = (e.start_row.cast_unsigned(), mask(e));
            let fs = &mut self.func_signature;
            let mesh = &mut self.mesh_output_handlers;
            match e.kind {
                semantic_kind::POSITION => {
                    fs.define_mesh_vertex_output(MeshVertexOutput::Position {
                        ty: MslValue::FLOAT4,
                    });
                    mesh.push(MeshOutputHandler::Position { reg, mask });
                }
                semantic_kind::RENDER_TARGET_ARRAY_INDEX => {
                    fs.define_mesh_primitive_output(MeshPrimitiveOutput::RenderTargetArrayIndex);
                    mesh.push(MeshOutputHandler::RenderTargetArrayIndex { reg, mask });
                }
                semantic_kind::VIEWPORT_ARRAY_INDEX => {
                    fs.define_mesh_primitive_output(MeshPrimitiveOutput::ViewportArrayIndex);
                    mesh.push(MeshOutputHandler::ViewportArrayIndex { reg, mask });
                }
                semantic_kind::ARBITRARY => {
                    for r in 0..e.rows {
                        let (user, ty) = varying(e, r)?;
                        let index = self.num_mesh_vertex_data;
                        self.num_mesh_vertex_data = index.wrapping_add(1);
                        fs.define_mesh_vertex_output(MeshVertexOutput::Data(MeshData {
                            user,
                            ty,
                            index,
                        }));
                        mesh.push(MeshOutputHandler::Shifted {
                            reg: reg.wrapping_add(r),
                            col: e.start_col.cast_unsigned(),
                            cols: e.cols,
                            index,
                            ty,
                        });
                    }
                }
                _ => return Err(unsupported_element(b"geometry shader output ", e)),
            }
            grow(&mut self.max_output_register, e);
        }
        Ok(())
    }
}

/// `Varying`: a vertex output row's name and type, at its signature's width.
///
/// The type is a vector of `cols` components, a vector of one for a scalar (the C++'s
/// `MSLVector{cols, …}`), as Metal Shader Converter declares them: the module flag
/// `agx.allow_mismatched_component_counts` links a narrower pixel input.
///
/// # Errors
///
/// [`NAME_NOT_UTF8`].
pub fn varying(e: &SignatureElement, row: u32) -> Result<(String, MslValue), Error> {
    let scalar = match register_type(e.ty) {
        UINT => MslScalar::Uint,
        INT => MslScalar::Int,
        _ => MslScalar::Float,
    };
    let ty = MslValue::Vector(MslVector {
        dimension: e.cols,
        scalar,
    });
    Ok((utf8(user_name(e, row))?, ty))
}

/// `RegisterComponentType`'s numbers.
const UINT: u32 = 1;
const INT: u32 = 2;
const FLOAT: u32 = 3;

/// `RegType`: a component type's register type (`RegisterComponentType`'s number).
const fn register_type(ty: u32) -> u32 {
    match ty {
        component_type::I16 | component_type::I32 | component_type::I64 => INT,
        component_type::I1 | component_type::U16 | component_type::U32 | component_type::U64 => {
            UINT
        }
        _ => FLOAT,
    }
}

/// `ToInterpolation`.
const fn air_interpolation(i: u32) -> Interpolation {
    match i {
        interpolation::CONSTANT => Interpolation::Flat,
        interpolation::LINEAR_CENTROID => Interpolation::CentroidPerspective,
        interpolation::LINEAR_NOPERSPECTIVE => Interpolation::CenterNoPerspective,
        interpolation::LINEAR_NOPERSPECTIVE_CENTROID => Interpolation::CentroidNoPerspective,
        interpolation::LINEAR_SAMPLE => Interpolation::SamplePerspective,
        interpolation::LINEAR_NOPERSPECTIVE_SAMPLE => Interpolation::SampleNoPerspective,
        _ => Interpolation::CenterPerspective,
    }
}

/// `Mask`: the element's components, `((1 << cols) - 1) << start_col` as ARM64 shifts.
const fn mask(e: &SignatureElement) -> u32 {
    1u32.wrapping_shl(e.cols)
        .wrapping_sub(1)
        .wrapping_shl(e.start_col.cast_unsigned())
}

/// `Grow`: a packed element's last register raises the count.
fn grow(max_register: &mut u32, e: &SignatureElement) {
    if e.start_row >= 0 {
        *max_register = (*max_register).max(e.start_row.cast_unsigned().wrapping_add(e.rows));
    }
}

fn utf8(name: Vec<u8>) -> Result<String, Error> {
    String::from_utf8(name).map_err(|_| Error::new(NAME_NOT_UTF8))
}

fn unsupported(what: &[u8]) -> Error {
    Error::new(&[&b"DXIL: "[..], what, b" not supported"].concat())
}

fn unsupported_element(what: &[u8], e: &SignatureElement) -> Error {
    unsupported(&[what, &e.name].concat())
}

#[cfg(test)]
mod tests;
