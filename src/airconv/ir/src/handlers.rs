//! The signature handlers as data: what each element asks of the prologue and epilogue.
//!
//! `handle_signature` (`A/dxbc_signature.cpp`) is [`Recorder`], the control-flow walk's
//! [`DeclSink`]. Each `dcl_input*` and `dcl_output*` defines the function elements the C++
//! defined at Initialize, updates the register counts and scalars, and records a handler that
//! holds what its C++ lambda captured, in the C++'s order; the compiles replay them. A case the
//! C++ only asserted on does nothing, as its release build did; where the release C++ then read
//! an undefined value, Initialize fails with an `Invalid DXBC bytecode` text.

use crate::{
    binding::{
        DepthArgument, FunctionInput, FunctionOutput, FunctionSignatureDesc, Interpolation,
        MeshData, MeshPrimitiveOutput, MeshVertexOutput, MslValue,
    },
    cfg::DeclSink,
    container::Error,
    ir::NO_PHASE,
    shader_info::{ShaderFlags, ShaderInfo},
    signature::{Parameter, Signature},
    tokens::{self, interpolation, opcode, operand_type, program_type, sb_name},
};

/// A pixel shader input's interpolation mode the C++ had no case for.
pub const UNKNOWN_INTERPOLATION: Error =
    Error::fail("Invalid DXBC bytecode: an unknown interpolation mode");
/// A signature element whose component type is neither uint, int nor float.
pub const UNKNOWN_COMPONENT_TYPE: Error =
    Error::fail("Invalid DXBC bytecode: a signature element of an unknown component type");
/// A pixel shader input of a system value the C++ had no case for.
pub const UNKNOWN_SYSTEM_VALUE: Error =
    Error::fail("Invalid DXBC bytecode: a pixel shader input of an unknown system value");

/// A signature handler (`std::function<void(SignatureContext &)>`), as what its lambda captured.
///
/// `reg` and `mask` are the declaration's register and component mask (x in bit 0); `index` an
/// element of the function signature the C++ defined at Initialize.
#[derive(Debug, PartialEq, Eq)]
pub enum SignatureHandler {
    /// A vertex shader's vertex ID stored to input `reg`.
    VertexId { reg: u32, mask: u32 },
    /// A vertex shader's instance ID stored to input `reg`.
    InstanceId { reg: u32, mask: u32 },
    /// A vertex input: pulled through the compile's input layout, else a stage-in attribute.
    ///
    /// `component_type` is the signature's, as the blob gives it.
    VertexInput {
        reg: u32,
        mask: u32,
        component_type: u32,
        name: String,
    },
    /// Output `reg` into element `index`, unless the compile skips vertex output.
    PopOutput { reg: u32, mask: u32, index: u32 },
    /// The position output `reg` into element `index`, sanitized, unless vertex output is skipped.
    PopPosition { reg: u32, mask: u32, index: u32 },
    /// A pixel shader's input coverage, from argument `index`.
    InputCoverage { index: u32 },
    /// Input `reg` from argument `index`; `fix_w` for the position.
    InitInput {
        index: u32,
        reg: u32,
        mask: u32,
        fix_w: bool,
    },
    /// A pixel shader input, pulled (interpolated in the shader) where the compile asks.
    FragmentInput {
        reg: u32,
        mask: u32,
        interpolation: Interpolation,
        component_type: u32,
        name: String,
    },
    /// The depth output's register.
    DepthRegister,
    /// The depth output, unless the compile disables it.
    DepthOutput { argument: DepthArgument },
    /// The stencil reference output's register.
    StencilRegister,
    /// The stencil reference output, element `index`.
    StencilOutput { index: u32 },
    /// The coverage mask output's register.
    CoverageRegister,
    /// The coverage mask output, element `index`, masked by the pipeline's sample mask.
    CoverageOutput { index: u32 },
    /// Render target `reg`, typed by the pipeline's format where it has one, else by the signature.
    RenderTarget {
        reg: u32,
        mask: u32,
        component_type: u32,
    },
    /// A compute shader's thread position in the grid, argument `index`.
    ThreadId { index: u32 },
    /// A compute shader's thread group position in the grid, argument `index`.
    ThreadGroupId { index: u32 },
    /// A compute shader's thread position in its group, argument `index`.
    ThreadIdInGroup { index: u32 },
    /// A compute shader's thread index in its group, argument `index`.
    ThreadIdInGroupFlattened { index: u32 },
}

/// A domain or geometry shader's mesh output handler.
///
/// The C++'s `std::function<IREffect(MeshOutputContext &)>`, as what its lambda captured.
#[derive(Debug, PartialEq, Eq)]
pub enum MeshOutputHandler {
    /// Output `reg` as the vertex's position.
    Position { reg: u32, mask: u32 },
    /// Output `reg` as the primitive's render target array index.
    RenderTargetArrayIndex { reg: u32, mask: u32 },
    /// Output `reg` as the primitive's viewport array index.
    ViewportArrayIndex { reg: u32, mask: u32 },
    /// Output `reg` as the vertex's user data `index`, of type `ty`.
    VertexData {
        reg: u32,
        mask: u32,
        index: u32,
        ty: MslValue,
    },
}

/// One component of a clip or cull distance output (`ScalarInfo`).
///
/// `reg` keeps the C++ bitfield's six bits.
#[derive(Debug, PartialEq, Eq)]
pub struct ScalarInfo {
    pub component: u8,
    pub reg: u8,
}

/// One component of a hull shader's patch constant output (`PatchConstantScalarInfo`).
///
/// `tess_factor_index` is the factor's place among its domain's, -1 for user data.
#[derive(Debug, PartialEq, Eq)]
pub struct PatchConstantScalarInfo {
    pub component: u8,
    pub reg: u8,
    pub tess_factor_index: i8,
}

/// What the signature declarations set on the shader (`SM50ShaderInternal`'s fields).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SignatureState {
    pub func_signature: FunctionSignatureDesc,
    pub signature_handlers: Vec<SignatureHandler>,
    pub mesh_output_handlers: Vec<MeshOutputHandler>,
    /// For a domain shader, the patch constant inputs.
    pub max_input_register: u32,
    pub max_output_register: u32,
    pub pso_valid_output_reg_mask: u32,
    pub max_patch_constant_output_register: u32,
    pub patch_constant_scalars: Vec<PatchConstantScalarInfo>,
    pub clip_distance_scalars: Vec<ScalarInfo>,
    pub cull_distance_scalars: Vec<ScalarInfo>,
    pub num_mesh_vertex_data: u32,
    pub ps_has_coverage_output: u32,
}

/// `handle_signature` over a shader's signatures: the walk's [`DeclSink`].
pub struct Recorder<'s> {
    program_type: u32,
    input: &'s Signature,
    /// The output signature's stream 0.
    output: &'s Signature,
    pub state: SignatureState,
}

/// A declaration's first operand: its register and its component mask, x in bit 0.
struct Decl<'a> {
    inst: &'a tokens::Instruction,
    ty: u32,
    reg: u32,
    mask: u32,
}

impl<'s> Recorder<'s> {
    /// A recorder for a shader of `program_type` with these signatures.
    #[must_use]
    pub fn new(program_type: u32, input: &'s Signature, output: &'s Signature) -> Self {
        Self {
            program_type,
            input,
            output,
            state: SignatureState::default(),
        }
    }
}

impl DeclSink for Recorder<'_> {
    fn signature(
        &mut self,
        inst: &tokens::Instruction,
        phase: u32,
        info: &mut ShaderInfo,
    ) -> Result<(), Error> {
        let op = &inst.operands[0];
        let decl = Decl {
            inst,
            ty: op.ty,
            reg: op.index[0].reg,
            mask: op.write_mask() >> 4,
        };
        match self.program_type {
            program_type::PIXEL => self.pixel(&decl),
            program_type::VERTEX => self.vertex(&decl),
            program_type::HULL => {
                self.hull(&decl, phase, info);
                Ok(())
            }
            program_type::DOMAIN => {
                self.domain_inputs(&decl);
                self.mesh_outputs(&decl)
            }
            program_type::COMPUTE => {
                self.compute(&decl);
                Ok(())
            }
            program_type::GEOMETRY => self.mesh_outputs(&decl),
            _ => Ok(()),
        }
    }
}

impl Recorder<'_> {
    /// `handle_signature_vs`.
    fn vertex(&mut self, d: &Decl<'_>) -> Result<(), Error> {
        let (reg, mask) = (d.reg, d.mask);
        let s = &mut self.state;
        match d.inst.opcode {
            opcode::DCL_INPUT_SGV => {
                match d.inst.name() {
                    sb_name::VERTEX_ID => {
                        s.signature_handlers
                            .push(SignatureHandler::VertexId { reg, mask });
                    }
                    sb_name::INSTANCE_ID => {
                        s.signature_handlers
                            .push(SignatureHandler::InstanceId { reg, mask });
                    }
                    _ => {}
                }
                raise(&mut s.max_input_register, reg);
            }
            opcode::DCL_INPUT if d.ty == operand_type::INPUT => {
                let found = find(self.input, reg, mask);
                raise(&mut s.max_input_register, reg);
                if let Some(p) = found {
                    s.signature_handlers.push(SignatureHandler::VertexInput {
                        reg,
                        mask,
                        component_type: p.component_type,
                        name: attribute_name(p),
                    });
                }
            }
            opcode::DCL_OUTPUT_SIV => {
                let output = match d.inst.name() {
                    sb_name::CLIP_DISTANCE => {
                        push_scalars(&mut s.clip_distance_scalars, reg, mask);
                        raise(&mut s.max_output_register, reg);
                        Some(FunctionOutput::Vertex {
                            user: format!("SV_ClipDistance{reg}"),
                            ty: MslValue::FLOAT4,
                        })
                    }
                    sb_name::CULL_DISTANCE => {
                        push_scalars(&mut s.cull_distance_scalars, reg, mask);
                        raise(&mut s.max_output_register, reg);
                        Some(FunctionOutput::Vertex {
                            user: format!("SV_CullDistance{reg}"),
                            ty: MslValue::FLOAT4,
                        })
                    }
                    sb_name::POSITION => {
                        let index = s.func_signature.define_output(FunctionOutput::Position {
                            ty: MslValue::FLOAT4,
                        });
                        s.signature_handlers.push(SignatureHandler::PopPosition {
                            reg,
                            mask,
                            index,
                        });
                        None
                    }
                    sb_name::RENDER_TARGET_ARRAY_INDEX => {
                        Some(FunctionOutput::RenderTargetArrayIndex)
                    }
                    sb_name::VIEWPORT_ARRAY_INDEX => Some(FunctionOutput::ViewportArrayIndex),
                    _ => None,
                };
                if let Some(output) = output {
                    let index = s.func_signature.define_output(output);
                    s.signature_handlers
                        .push(SignatureHandler::PopOutput { reg, mask, index });
                }
                raise(&mut s.max_output_register, reg);
            }
            opcode::DCL_OUTPUT if d.ty == operand_type::OUTPUT => {
                let found = find(self.output, reg, mask);
                raise(&mut s.max_output_register, reg);
                if let Some(p) = found {
                    let output = FunctionOutput::Vertex {
                        user: attribute_name(p),
                        ty: msl_value(p.component_type)?,
                    };
                    let index = s.func_signature.define_output(output);
                    s.signature_handlers
                        .push(SignatureHandler::PopOutput { reg, mask, index });
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// `handle_signature_ps`.
    fn pixel(&mut self, d: &Decl<'_>) -> Result<(), Error> {
        let (reg, mask) = (d.reg, d.mask);
        let s = &mut self.state;
        match d.inst.opcode {
            opcode::DCL_INPUT if d.ty == operand_type::INPUT_COVERAGE_MASK => {
                let index = s.func_signature.define_input(FunctionInput::InputCoverage);
                s.signature_handlers
                    .push(SignatureHandler::InputCoverage { index });
            }
            opcode::DCL_INPUT_PS_SIV => {
                let name = d.inst.name();
                // The C++ read the mode for every name; only these use it.
                let input = match name {
                    sb_name::POSITION => FunctionInput::Position {
                        interpolation: air_interpolation(d.inst.interpolation())?,
                    },
                    sb_name::RENDER_TARGET_ARRAY_INDEX => FunctionInput::RenderTargetArrayIndex,
                    sb_name::VIEWPORT_ARRAY_INDEX => FunctionInput::ViewportArrayIndex,
                    sb_name::CLIP_DISTANCE | sb_name::CULL_DISTANCE => {
                        let kind = if name == sb_name::CLIP_DISTANCE {
                            "Clip"
                        } else {
                            "Cull"
                        };
                        FunctionInput::FragmentStageIn {
                            user: format!("SV_{kind}Distance{reg}"),
                            ty: MslValue::FLOAT4,
                            interpolation: air_interpolation(d.inst.interpolation())?,
                            pull_mode: false,
                        }
                    }
                    _ => return Err(UNKNOWN_SYSTEM_VALUE),
                };
                let index = s.func_signature.define_input(input);
                s.signature_handlers.push(SignatureHandler::InitInput {
                    index,
                    reg,
                    mask,
                    fix_w: name == sb_name::POSITION,
                });
                raise(&mut s.max_input_register, reg);
            }
            opcode::DCL_INPUT_PS_SGV => {
                // The mode is read but unused (the C++'s `[[maybe_unused]]`).
                let input = match d.inst.name() {
                    sb_name::IS_FRONT_FACE => FunctionInput::FrontFacing,
                    sb_name::SAMPLE_INDEX => FunctionInput::SampleIndex,
                    sb_name::PRIMITIVE_ID => FunctionInput::PrimitiveId,
                    _ => return Err(UNKNOWN_SYSTEM_VALUE),
                };
                let index = s.func_signature.define_input(input);
                s.signature_handlers.push(SignatureHandler::InitInput {
                    index,
                    reg,
                    mask,
                    fix_w: false,
                });
                raise(&mut s.max_input_register, reg);
            }
            opcode::DCL_INPUT_PS => {
                let interpolation = air_interpolation(d.inst.interpolation())?;
                let found = find(self.input, reg, mask);
                raise(&mut s.max_input_register, reg);
                if let Some(p) = found {
                    s.signature_handlers.push(SignatureHandler::FragmentInput {
                        reg,
                        mask,
                        interpolation,
                        component_type: p.component_type,
                        name: attribute_name(p),
                    });
                }
            }
            opcode::DCL_OUTPUT_SIV => raise(&mut s.max_output_register, reg),
            opcode::DCL_OUTPUT => match d.ty {
                operand_type::OUTPUT_DEPTH
                | operand_type::OUTPUT_DEPTH_GREATER_EQUAL
                | operand_type::OUTPUT_DEPTH_LESS_EQUAL => {
                    let argument = match d.ty {
                        operand_type::OUTPUT_DEPTH_GREATER_EQUAL => DepthArgument::Greater,
                        operand_type::OUTPUT_DEPTH_LESS_EQUAL => DepthArgument::Less,
                        _ => DepthArgument::Any,
                    };
                    s.signature_handlers.push(SignatureHandler::DepthRegister);
                    s.signature_handlers
                        .push(SignatureHandler::DepthOutput { argument });
                }
                operand_type::OUTPUT_STENCIL_REF => {
                    s.signature_handlers.push(SignatureHandler::StencilRegister);
                    let index = s.func_signature.define_output(FunctionOutput::StencilRef);
                    s.signature_handlers
                        .push(SignatureHandler::StencilOutput { index });
                }
                operand_type::OUTPUT_COVERAGE_MASK => {
                    s.signature_handlers
                        .push(SignatureHandler::CoverageRegister);
                    let index = s.func_signature.define_output(FunctionOutput::CoverageMask);
                    s.signature_handlers
                        .push(SignatureHandler::CoverageOutput { index });
                    s.ps_has_coverage_output = 1;
                }
                operand_type::OUTPUT => {
                    let found = find(self.output, reg, mask);
                    raise(&mut s.max_output_register, reg);
                    // ARM64 shifts by the count mod 32, as the C++'s `1 << reg` ran.
                    s.pso_valid_output_reg_mask |= 1u32.wrapping_shl(reg);
                    if let Some(p) = found {
                        s.signature_handlers.push(SignatureHandler::RenderTarget {
                            reg,
                            mask,
                            component_type: p.component_type,
                        });
                    }
                }
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }

    /// `handle_signature_hs`: register counts and patch constant scalars, no handler.
    fn hull(&mut self, d: &Decl<'_>, phase: u32, info: &mut ShaderInfo) {
        let (reg, mask) = (d.reg, d.mask);
        let s = &mut self.state;
        match d.inst.opcode {
            opcode::DCL_INPUT_SGV => raise(&mut s.max_input_register, reg),
            opcode::DCL_INPUT if d.ty == operand_type::OUTPUT_CONTROL_POINT => {
                info.flags.insert(ShaderFlags::OUTPUT_CONTROL_POINT_READ);
            }
            opcode::DCL_OUTPUT_SIV => {
                let name = d.inst.name();
                let first = match name {
                    sb_name::FINAL_QUAD_U_EQ_0_EDGE_TESSFACTOR
                        ..=sb_name::FINAL_QUAD_V_INSIDE_TESSFACTOR => {
                        Some(sb_name::FINAL_QUAD_U_EQ_0_EDGE_TESSFACTOR)
                    }
                    sb_name::FINAL_TRI_U_EQ_0_EDGE_TESSFACTOR
                        ..=sb_name::FINAL_TRI_INSIDE_TESSFACTOR => {
                        Some(sb_name::FINAL_TRI_U_EQ_0_EDGE_TESSFACTOR)
                    }
                    sb_name::FINAL_LINE_DETAIL_TESSFACTOR
                    | sb_name::FINAL_LINE_DENSITY_TESSFACTOR => {
                        Some(sb_name::FINAL_LINE_DETAIL_TESSFACTOR)
                    }
                    _ => None,
                };
                if let Some(first) = first {
                    // The ranges above keep the place below 12.
                    let place = i8::try_from(name - first).unwrap_or(-1);
                    push_patch_scalars(&mut s.patch_constant_scalars, reg, mask, place);
                }
                if phase == NO_PHASE {
                    raise(&mut s.max_output_register, reg);
                } else {
                    raise(&mut s.max_patch_constant_output_register, reg);
                }
            }
            opcode::DCL_OUTPUT if d.ty == operand_type::OUTPUT => {
                if phase == NO_PHASE {
                    raise(&mut s.max_output_register, reg);
                } else {
                    push_patch_scalars(&mut s.patch_constant_scalars, reg, mask, -1);
                    raise(&mut s.max_patch_constant_output_register, reg);
                }
            }
            _ => {}
        }
    }

    /// `handle_signature_ds`'s inputs: the patch constant register count.
    fn domain_inputs(&mut self, d: &Decl<'_>) {
        let reg = d.reg;
        let s = &mut self.state;
        match d.inst.opcode {
            opcode::DCL_INPUT_SIV
                if (sb_name::FINAL_QUAD_U_EQ_0_EDGE_TESSFACTOR
                    ..=sb_name::FINAL_LINE_DENSITY_TESSFACTOR)
                    .contains(&d.inst.name()) =>
            {
                raise(&mut s.max_input_register, reg);
            }
            opcode::DCL_INPUT if d.ty == operand_type::INPUT_PATCH_CONSTANT => {
                raise(&mut s.max_input_register, reg);
            }
            _ => {}
        }
    }

    /// The outputs of `handle_signature_ds` and `handle_signature_gs`, which are the same.
    fn mesh_outputs(&mut self, d: &Decl<'_>) -> Result<(), Error> {
        let (reg, mask) = (d.reg, d.mask);
        let s = &mut self.state;
        match d.inst.opcode {
            opcode::DCL_OUTPUT_SIV => {
                match d.inst.name() {
                    sb_name::CLIP_DISTANCE => {
                        push_scalars(&mut s.clip_distance_scalars, reg, mask);
                        raise(&mut s.max_output_register, reg);
                    }
                    sb_name::CULL_DISTANCE => {
                        push_scalars(&mut s.cull_distance_scalars, reg, mask);
                        raise(&mut s.max_output_register, reg);
                    }
                    sb_name::POSITION => {
                        s.func_signature
                            .define_mesh_vertex_output(MeshVertexOutput::Position {
                                ty: MslValue::FLOAT4,
                            });
                        s.mesh_output_handlers
                            .push(MeshOutputHandler::Position { reg, mask });
                    }
                    sb_name::RENDER_TARGET_ARRAY_INDEX => {
                        s.func_signature.define_mesh_primitive_output(
                            MeshPrimitiveOutput::RenderTargetArrayIndex,
                        );
                        s.mesh_output_handlers
                            .push(MeshOutputHandler::RenderTargetArrayIndex { reg, mask });
                    }
                    sb_name::VIEWPORT_ARRAY_INDEX => {
                        s.func_signature
                            .define_mesh_primitive_output(MeshPrimitiveOutput::ViewportArrayIndex);
                        s.mesh_output_handlers
                            .push(MeshOutputHandler::ViewportArrayIndex { reg, mask });
                    }
                    _ => {}
                }
                raise(&mut s.max_output_register, reg);
            }
            opcode::DCL_OUTPUT if d.ty == operand_type::OUTPUT => {
                let found = find(self.output, reg, mask);
                raise(&mut s.max_output_register, reg);
                if let Some(p) = found {
                    let index = s.num_mesh_vertex_data;
                    s.num_mesh_vertex_data = index.wrapping_add(1);
                    let ty = msl_value(p.component_type)?;
                    s.func_signature
                        .define_mesh_vertex_output(MeshVertexOutput::Data(MeshData {
                            user: attribute_name(p),
                            ty,
                            index,
                        }));
                    s.mesh_output_handlers.push(MeshOutputHandler::VertexData {
                        reg,
                        mask,
                        index,
                        ty,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// `handle_signature_cs`.
    fn compute(&mut self, d: &Decl<'_>) {
        if d.inst.opcode != opcode::DCL_INPUT {
            return;
        }
        let s = &mut self.state;
        let (input, handler): (_, fn(u32) -> SignatureHandler) = match d.ty {
            operand_type::INPUT_THREAD_ID => (FunctionInput::ThreadPositionInGrid, |index| {
                SignatureHandler::ThreadId { index }
            }),
            operand_type::INPUT_THREAD_GROUP_ID => {
                (FunctionInput::ThreadgroupPositionInGrid, |index| {
                    SignatureHandler::ThreadGroupId { index }
                })
            }
            operand_type::INPUT_THREAD_ID_IN_GROUP => {
                (FunctionInput::ThreadPositionInThreadgroup, |index| {
                    SignatureHandler::ThreadIdInGroup { index }
                })
            }
            operand_type::INPUT_THREAD_ID_IN_GROUP_FLATTENED => {
                (FunctionInput::ThreadIndexInThreadgroup, |index| {
                    SignatureHandler::ThreadIdInGroupFlattened { index }
                })
            }
            _ => return,
        };
        let index = s.func_signature.define_input(input);
        s.signature_handlers.push(handler(index));
    }
}

/// `max_*_register = std::max(reg + 1, max_*_register)`, the sum wrapping as the C++'s did.
fn raise(max: &mut u32, reg: u32) {
    *max = (*max).max(reg.wrapping_add(1));
}

/// `findInputElement`/`findOutputElement`: an element on register `reg` that `mask` touches.
///
/// The first that shares a component with `mask`.
/// The C++'s `Signature` kept the register in a byte, so a register is compared modulo 256.
fn find(signature: &Signature, reg: u32, mask: u32) -> Option<&Parameter> {
    signature
        .parameters
        .iter()
        .find(|p| p.register & 0xff == reg && u32::from(p.mask) & mask != 0)
}

/// `consistentAttributeName`: `reg<register>_<first component>`.
///
/// The register is the C++'s byte; an element with no component in xyzw names component 32, as
/// `std::countr_zero(0u)`.
#[must_use]
pub fn attribute_name(p: &Parameter) -> String {
    let first = (u32::from(p.mask) & 0xf).trailing_zeros();
    format!("reg{}_{first}", p.register & 0xff)
}

/// `to_msl_type`: a signature component type's four-component type.
///
/// # Errors
///
/// [`UNKNOWN_COMPONENT_TYPE`] for any type but uint, int and float, where the C++ fell off its
/// end.
pub const fn msl_value(component_type: u32) -> Result<MslValue, Error> {
    match component_type {
        1 => Ok(MslValue::UINT4),
        2 => Ok(MslValue::INT4),
        3 => Ok(MslValue::FLOAT4),
        _ => Err(UNKNOWN_COMPONENT_TYPE),
    }
}

/// `to_air_interpolation`.
///
/// # Errors
///
/// [`UNKNOWN_INTERPOLATION`] for a mode the C++ had no case for (it fell off its end).
pub const fn air_interpolation(mode: u32) -> Result<Interpolation, Error> {
    Ok(match mode {
        interpolation::LINEAR_NOPERSPECTIVE_SAMPLE => Interpolation::SampleNoPerspective,
        interpolation::LINEAR_SAMPLE => Interpolation::SamplePerspective,
        interpolation::LINEAR_NOPERSPECTIVE_CENTROID => Interpolation::CentroidNoPerspective,
        interpolation::LINEAR_CENTROID => Interpolation::CentroidPerspective,
        interpolation::CONSTANT => Interpolation::Flat,
        interpolation::LINEAR => Interpolation::CenterPerspective,
        interpolation::LINEAR_NOPERSPECTIVE => Interpolation::CenterNoPerspective,
        _ => return Err(UNKNOWN_INTERPOLATION),
    })
}

/// A register as the C++'s six-bit bitfield keeps it.
const fn bitfield_reg(reg: u32) -> u8 {
    let [low, ..] = (reg & 0x3f).to_le_bytes();
    low
}

/// One scalar per component of `mask`.
fn push_scalars(scalars: &mut Vec<ScalarInfo>, reg: u32, mask: u32) {
    for component in 0..4u8 {
        if mask & (1 << component) != 0 {
            scalars.push(ScalarInfo {
                component,
                reg: bitfield_reg(reg),
            });
        }
    }
}

/// One patch constant scalar per component of `mask`.
fn push_patch_scalars(
    scalars: &mut Vec<PatchConstantScalarInfo>,
    reg: u32,
    mask: u32,
    tess_factor_index: i8,
) {
    for component in 0..4u8 {
        if mask & (1 << component) != 0 {
            scalars.push(PatchConstantScalarInfo {
                component,
                reg: bitfield_reg(reg),
                tess_factor_index,
            });
        }
    }
}

#[cfg(test)]
mod tests;
