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

use airconv_ir::{
    binding::{
        DepthArgument, FunctionInput, FunctionOutput, FunctionSignatureDesc, Interpolation,
        MeshData, MeshPrimitiveOutput, MeshVertexOutput, MslScalar, MslValue, MslVector,
    },
    handlers::{SignatureHandler as DxbcHandler, msl_value},
};

use crate::{
    air::builder::AirBuilder,
    api::Error,
    dxbc::{
        converter::{invalid, req},
        replay::{self, Options, load_from_array_at},
        resources::Resources,
    },
    dxil::types::{
        EntryInfo, SignatureElement, UNPACKED_INSTANCE_ID, UNPACKED_VERTEX_ID, component_type,
        interpolation, op, semantic_kind, shader_kind, user_name,
    },
    llvm::{Func, Ty, Val},
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
    Id,
    GroupId,
    IdInGroup,
    IdInGroupFlat,
}

/// A geometry shader's output handler (`std::function<IREffect(MeshOutputContext &)>`).
pub enum MeshOutputHandler {
    /// Output `reg` as the vertex's position.
    Position { reg: u32 },
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
                Special::Id,
            ),
            (
                ops.contains(&op::GROUP_ID),
                FunctionInput::ThreadgroupPositionInGrid,
                Special::GroupId,
            ),
            (
                ops.contains(&op::THREAD_ID_IN_GROUP),
                FunctionInput::ThreadPositionInThreadgroup,
                Special::IdInGroup,
            ),
            (
                ops.contains(&op::FLATTENED_THREAD_ID_IN_GROUP) || flat_index,
                FunctionInput::ThreadIndexInThreadgroup,
                Special::IdInGroupFlat,
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
                    mesh.push(MeshOutputHandler::Position { reg });
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

    /// The handlers' pass against a compile's options (their `SignatureContext` pass).
    ///
    /// Each defines what the compile adds to `desc` and leaves its prologue and epilogue steps,
    /// in the C++'s order. A handler whose lambda is the DXBC handler's of the same body is
    /// recorded by D2's replay (`dxbc::replay::record`), one at a time; the varyings, the
    /// compute IDs, the render targets (whose target is the semantic index, not the register)
    /// and the depth, coverage and stencil outputs are this front end's.
    ///
    /// # Errors
    ///
    /// A render target of an unknown component type (`UNKNOWN_COMPONENT_TYPE`, where the C++
    /// fell off `to_msl_type`), and what D2's replay refuses.
    pub fn record<'a>(
        &self,
        desc: &mut FunctionSignatureDesc,
        options: &Options<'a>,
    ) -> Result<Replay<'a>, Error> {
        let (dual, no_depth, unorm_mask, formats) =
            options.pixel.map_or((false, false, 0, [0; 8]), |p| {
                (
                    p.dual_source_blending != 0,
                    p.disable_depth_output != 0,
                    p.unorm_output_reg_mask,
                    p.pixel_formats,
                )
            });
        let skip = options.skip_vertex_output;
        let mut steps = Vec::new();
        for handler in &self.signature_handlers {
            let mut dxbc =
                |h: DxbcHandler, desc: &mut FunctionSignatureDesc| -> Result<(), Error> {
                    steps.push(Step::Dxbc(replay::record(&[h], desc, options)?));
                    Ok(())
                };
            match *handler {
                SignatureHandler::ComputeQuads {
                    thread_index,
                    group,
                    numthreads,
                } => steps.push(Step::ComputeQuads {
                    thread_index,
                    group,
                    numthreads,
                }),
                SignatureHandler::Special {
                    index,
                    ref register,
                } => steps.push(Step::Special {
                    index,
                    register: match register {
                        Special::Id => Special::Id,
                        Special::GroupId => Special::GroupId,
                        Special::IdInGroup => Special::IdInGroup,
                        Special::IdInGroupFlat => Special::IdInGroupFlat,
                    },
                }),
                SignatureHandler::VertexSystemValue { reg, mask, vertex } => dxbc(
                    if vertex {
                        DxbcHandler::VertexId { reg, mask }
                    } else {
                        DxbcHandler::InstanceId { reg, mask }
                    },
                    desc,
                )?,
                SignatureHandler::VertexInput {
                    reg,
                    mask,
                    component_type,
                    ref name,
                } => dxbc(
                    DxbcHandler::VertexInput {
                        reg,
                        mask,
                        component_type,
                        name: name.clone(),
                    },
                    desc,
                )?,
                SignatureHandler::PopPosition { reg, mask, index } => {
                    dxbc(DxbcHandler::PopPosition { reg, mask, index }, desc)?;
                }
                SignatureHandler::PopOutput { reg, mask, index } => {
                    dxbc(DxbcHandler::PopOutput { reg, mask, index }, desc)?;
                }
                SignatureHandler::PopShifted {
                    reg,
                    col,
                    cols,
                    index,
                } => {
                    if !skip {
                        steps.push(Step::PopShifted {
                            reg,
                            col,
                            cols,
                            index,
                        });
                    }
                }
                SignatureHandler::InitInput {
                    index,
                    reg,
                    mask,
                    fix_w,
                } => dxbc(
                    DxbcHandler::InitInput {
                        index,
                        reg,
                        mask,
                        fix_w,
                    },
                    desc,
                )?,
                SignatureHandler::InitShifted {
                    index,
                    reg,
                    col,
                    cols,
                } => steps.push(Step::InitShifted {
                    index,
                    reg,
                    col,
                    cols,
                }),
                // As DCL_OUTPUT in handle_signature_ps, with the semantic index as the target.
                SignatureHandler::RenderTarget {
                    reg,
                    mask,
                    target,
                    component_type,
                } => {
                    // A target past the eighth reads as no format (the C++ read past its array).
                    let format = formats.get(target as usize).copied().unwrap_or(0);
                    let mut ty = component_type_from_pixel_format(format);
                    if dual || ty == UNKNOWN {
                        ty = component_type;
                    }
                    if dual && target > 1 {
                        continue;
                    }
                    let index = desc.define_output(FunctionOutput::RenderTarget {
                        dual_source_blending: dual,
                        index: target,
                        ty: msl_value(ty)?,
                    });
                    if ty == FLOAT && unorm_mask & 1u32.wrapping_shl(target) != 0 {
                        steps.push(Step::FixUnorm { reg, index });
                    } else {
                        dxbc(DxbcHandler::PopOutput { reg, mask, index }, desc)?;
                    }
                }
                // As the OUTPUT_DEPTH case in handle_signature_ps.
                SignatureHandler::Depth { ref argument } => {
                    steps.push(Step::Alloca(Output::Depth));
                    if !no_depth {
                        let index = desc.define_output(FunctionOutput::Depth {
                            argument: argument.clone(),
                        });
                        steps.push(Step::Output(Output::Depth, index));
                    }
                }
                SignatureHandler::Coverage { index } => {
                    steps.push(Step::Alloca(Output::Coverage));
                    steps.push(Step::Output(Output::Coverage, index));
                }
                SignatureHandler::StencilRef { index } => {
                    steps.push(Step::Alloca(Output::Stencil));
                    steps.push(Step::Output(Output::Stencil, index));
                }
            }
        }
        Ok(Replay { steps })
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

/// The handlers' steps, in their order (the C++'s prologue and epilogue effects).
pub struct Replay<'a> {
    steps: Vec<Step<'a>>,
}

impl Replay<'_> {
    /// The prologue's steps at the builder's position.
    ///
    /// # Errors
    ///
    /// What D2's replay refuses, an argument or register file the compile did not make.
    pub fn prologue<'cx>(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        res: &mut Resources<'cx>,
    ) -> Result<(), Error> {
        let b = air.builder();
        let types = air.types();
        for step in &self.steps {
            match *step {
                Step::Dxbc(ref replay) => replay.prologue(air, function, res)?,
                Step::ComputeQuads {
                    thread_index,
                    group,
                    numthreads,
                } => compute_quads(air, function, res, thread_index, group, numthreads)?,
                Step::Special {
                    index,
                    ref register,
                } => {
                    let value = Some(param(air, function, index)?);
                    match register {
                        Special::Id => res.thread_id_arg = value,
                        Special::GroupId => res.thread_group_id_arg = value,
                        Special::IdInGroup => res.thread_id_in_group_arg = value,
                        Special::IdInGroupFlat => res.thread_id_in_group_flat_arg = value,
                    }
                }
                Step::InitShifted {
                    index,
                    reg,
                    col,
                    cols,
                } => init_shifted(air, function, res, index, reg, col, cols)?,
                Step::Alloca(Output::Depth) => {
                    res.depth_output_reg = Some(b.alloca(types.float, c""));
                }
                Step::Alloca(Output::Coverage) => {
                    res.coverage_mask_reg = Some(b.alloca(types.int, c""));
                }
                Step::Alloca(Output::Stencil) => {
                    res.stencil_ref_reg = Some(b.alloca(types.int, c""));
                }
                Step::PopShifted { .. } | Step::FixUnorm { .. } | Step::Output(..) => {}
            }
        }
        Ok(())
    }

    /// The epilogue's steps at the builder's position, each inserting an output into `value`.
    ///
    /// `value` is the function's zero return value, `None` for a `void` function;
    /// `pso_sample_mask` masks the coverage output.
    ///
    /// # Errors
    ///
    /// An output step on a `void` function (the C++ inserted into a null), what D2's replay
    /// refuses, and a register file or output register the compile did not make.
    pub fn epilogue<'cx>(
        &self,
        air: &AirBuilder<'_, 'cx>,
        res: &Resources<'cx>,
        mut value: Option<Val<'cx>>,
        pso_sample_mask: u32,
    ) -> Result<Option<Val<'cx>>, Error> {
        let b = air.builder();
        let types = air.types();
        let tcx = b.cx().tcx();
        let void = || invalid("an output of a function that returns void");
        for step in &self.steps {
            value = match *step {
                Step::Dxbc(ref replay) => replay.epilogue(air, res, value, pso_sample_mask)?,
                Step::PopShifted {
                    reg,
                    col,
                    cols,
                    index,
                } => {
                    let ret = value.ok_or_else(void)?;
                    let field_ty = tcx
                        .field(ret.ty(), index)
                        .ok_or_else(|| invalid("an output of no return field"))?;
                    let shifted = shifted(air, res, reg, col, cols, field_ty)?;
                    Some(b.insert_value(ret, shifted, index))
                }
                // `pop_output_reg_fix_unorm`, as D2's replay pops it.
                Step::FixUnorm { reg, index } => {
                    let ret = value.ok_or_else(void)?;
                    let output = req(res.output.ptr_float4, "no output register file")?;
                    let fvec4 = load_from_array_at(air, output, air.get_int(reg))?;
                    // The magic delta of `pop_output_reg_fix_unorm`.
                    let delta = air.get_float(1.0 / 127_500.0);
                    let fixed = b.fsub(fvec4, b.cx().const_vector(&[delta; 4]));
                    Some(b.insert_value(ret, fixed, index))
                }
                Step::Output(ref output, index) => {
                    let ret = value.ok_or_else(void)?;
                    let output = match output {
                        Output::Depth => {
                            let reg = req(res.depth_output_reg, "no depth output")?;
                            b.load(types.float, reg)
                        }
                        Output::Coverage => {
                            let reg = req(res.coverage_mask_reg, "no coverage output")?;
                            let mask = b.load(types.int, reg);
                            if pso_sample_mask == u32::MAX {
                                mask
                            } else {
                                b.and(mask, air.get_int(pso_sample_mask))
                            }
                        }
                        Output::Stencil => {
                            let reg = req(res.stencil_ref_reg, "no stencil output")?;
                            b.load(types.int, reg)
                        }
                    };
                    Some(b.insert_value(ret, output, index))
                }
                Step::ComputeQuads { .. }
                | Step::Special { .. }
                | Step::InitShifted { .. }
                | Step::Alloca(..) => value,
            };
        }
        Ok(value)
    }
}

/// `Shifted`: output register `reg`'s components `[col, col + cols)` as a value of `ty`.
///
/// The varying's x, y, … at its signature's width (a scalar for one).
///
/// # Errors
///
/// No output register file.
pub fn shifted<'cx>(
    air: &AirBuilder<'_, 'cx>,
    res: &Resources<'cx>,
    reg: u32,
    col: u32,
    cols: u32,
    ty: Ty,
) -> Result<Val<'cx>, Error> {
    let b = air.builder();
    let array = req(res.output.ptr_int4, "no output register file")?;
    let array_ty = b
        .cx()
        .tcx()
        .pointee(array.ty())
        .ok_or_else(|| invalid("a register file is not a pointer"))?;
    let at = b.gep(array_ty, array, &[air.get_int(0), air.get_int(reg)]);
    let row = b.load(air.types().int4, at);
    if cols == 1 {
        return Ok(b.bitcast(b.extract_element_u64(row, col.into()), ty));
    }
    let mask: Vec<i32> = (col..col.wrapping_add(cols))
        .map(|c| i32::try_from(c).unwrap_or(-1))
        .collect();
    Ok(b.bitcast(b.shuffle_vector1(row, &mask), ty))
}

/// `MeshPopShifted`: a geometry shader output's varying as the mesh vertex's data `index`.
///
/// # Errors
///
/// No output register file; what the AIR builder refuses.
pub fn pop_mesh_shifted<'cx>(
    air: &AirBuilder<'_, 'cx>,
    res: &Resources<'cx>,
    (reg, col, cols): (u32, u32, u32),
    index: u32,
    vertex: Val<'cx>,
    ty: Ty,
) -> Result<(), Error> {
    let value = shifted(air, res, reg, col, cols, ty)?;
    air.create_set_mesh_vertex_data(vertex, air.get_int(index), value)?;
    Ok(())
}

/// `MTLPixelFormat`s by the register type a render target of them holds.
///
/// D2's replay's table (`A/air_signature.hpp`), which it keeps private.
const PIXEL_SINT: [u32; 9] = [14, 24, 34, 54, 64, 74, 104, 114, 124];
const PIXEL_UINT: [u32; 10] = [13, 23, 33, 53, 63, 73, 91, 103, 113, 123];
const PIXEL_FLOAT: [u32; 50] = [
    1, 10, 11, 12, 20, 22, 25, 30, 31, 32, 40, 41, 42, 43, 55, 60, 62, 65, 70, 71, 72, 80, 81, 90,
    92, 93, 94, 554, 555, 105, 110, 112, 115, 552, 553, 125, 130, 131, 132, 133, 134, 135, 140,
    141, 142, 143, 150, 151, 152, 153,
];
/// `RegisterComponentType`'s numbers.
const UNKNOWN: u32 = 0;
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

/// `component_type_from_pixel_format`: a render target format's register type.
fn component_type_from_pixel_format(format: u32) -> u32 {
    if PIXEL_SINT.contains(&format) {
        INT
    } else if PIXEL_UINT.contains(&format) {
        UINT
    } else if PIXEL_FLOAT.contains(&format) {
        FLOAT
    } else {
        UNKNOWN
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

/// A replay step: D2's for a DXBC-shaped handler, else this front end's.
enum Step<'a> {
    Dxbc(replay::Replay<'a>),
    ComputeQuads {
        thread_index: u32,
        group: u32,
        numthreads: [u32; 3],
    },
    Special {
        index: u32,
        register: Special,
    },
    InitShifted {
        index: u32,
        reg: u32,
        col: u32,
        cols: u32,
    },
    PopShifted {
        reg: u32,
        col: u32,
        cols: u32,
        index: u32,
    },
    FixUnorm {
        reg: u32,
        index: u32,
    },
    /// The output register's alloca, in the prologue.
    Alloca(Output),
    /// The output register into return element `index`, in the epilogue.
    Output(Output, u32),
}

/// A pixel shader's special output.
enum Output {
    Depth,
    Coverage,
    Stencil,
}

/// Function argument `index`.
fn param<'cx>(
    air: &AirBuilder<'_, 'cx>,
    function: Func<'cx>,
    index: u32,
) -> Result<Val<'cx>, Error> {
    function
        .param(air.builder().cx().tcx(), index)
        .ok_or_else(|| invalid("a signature handler's argument the function lacks"))
}

/// A compute shader's thread IDs remapped onto 2D quads (`AddComputeHandlers`' `quads_2d`).
///
/// Metal's quads are four consecutive lanes, t & ~3 .. t | 3 of its flat thread index t; D3D's
/// 2D quads are 2x2 blocks of `SV_GroupThreadID`. Metal thread t runs D3D's thread (x, y, z),
/// quad lane l = t & 3 at (l & 1, l >> 1) of the block, so derivatives and quad reads see D3D's
/// quad. `SV_GroupIndex` stays D3D's flattening of (x, y, z).
fn compute_quads<'cx>(
    air: &AirBuilder<'_, 'cx>,
    function: Func<'cx>,
    res: &mut Resources<'cx>,
    thread_index: u32,
    group: u32,
    numthreads: [u32; 3],
) -> Result<(), Error> {
    let b = air.builder();
    let cx = b.cx();
    let int = |v| air.get_int(v);
    let flat = param(air, function, thread_index)?;
    let slice = int(numthreads[0].wrapping_mul(numthreads[1]));
    let half = int(numthreads[0] / 2);
    let layer = b.udiv(flat, slice);
    let in_layer = b.urem(flat, slice);
    let quad = b.lshr(in_layer, int(2));
    let lane = b.and(in_layer, int(3));
    let column = b.add(b.shl(b.urem(quad, half), int(1)), b.and(lane, int(1)));
    let row = b.add(b.shl(b.udiv(quad, half), int(1)), b.lshr(lane, int(1)));
    let mut id = cx.undef(air.int_ty(3));
    id = b.insert_element_u64(id, column, 0);
    id = b.insert_element_u64(id, row, 1);
    id = b.insert_element_u64(id, layer, 2);
    let group_id = param(air, function, group)?;
    res.thread_group_id_arg = Some(group_id);
    res.thread_id_in_group_arg = Some(id);
    let plane = b.mul(layer, slice);
    let row_start = b.mul(row, int(numthreads[0]));
    res.thread_id_in_group_flat_arg = Some(b.add(plane, b.add(row_start, column)));
    let size = cx.const_vector(&numthreads.map(int));
    res.thread_id_arg = Some(b.add(b.mul(group_id, size), id));
    Ok(())
}

/// `InitShifted`: argument `index`'s x, y, … into input register `reg`'s `[col, col + cols)`.
fn init_shifted<'cx>(
    air: &AirBuilder<'_, 'cx>,
    function: Func<'cx>,
    res: &Resources<'cx>,
    index: u32,
    reg: u32,
    col: u32,
    cols: u32,
) -> Result<(), Error> {
    let b = air.builder();
    let tcx = b.cx().tcx();
    let array = req(res.input.ptr_int4, "no input register file")?;
    let array_ty = tcx
        .pointee(array.ty())
        .ok_or_else(|| invalid("a register file is not a pointer"))?;
    let int_ty = if cols == 1 {
        tcx.int(32)
    } else {
        tcx.vector(tcx.int(32), cols)
    };
    let arg = b.bitcast(param(air, function, index)?, int_ty);
    for i in 0..cols {
        let component = if cols == 1 {
            arg
        } else {
            b.extract_element_u64(arg, i.into())
        };
        let at = b.gep(
            array_ty,
            array,
            &[
                air.get_int(0),
                air.get_int(reg),
                air.get_int(col.wrapping_add(i)),
            ],
        );
        b.store(component, at);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
