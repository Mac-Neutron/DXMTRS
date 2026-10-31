//! The control-flow graph of a DXBC program, from its structured flow instructions.
//!
//! Ported from `read_control_flow` (`A/dxbc_converter_cfg.cpp`): one walk over the program
//! builds the basic blocks, records the declarations in [`ShaderInfo`] and [`Declarations`],
//! hands each `dcl_input*`/`dcl_output*` to a [`DeclSink`], and decodes every other instruction
//! with [`decode::read_instruction`]. Calls are then inlined up to 32 deep.
//!
//! The blocks are listed in the order the C++ made them; the entry is the first. Code after a
//! `break`, `continue` or nested `ret` goes to a block outside the list and is dropped, as the
//! C++'s `bb_void` dropped it. Where the C++ took the top of an empty stack, followed a pointer
//! into a terminator since overwritten, called a label nobody declared or ran `callc` (which
//! aborted it), the walk returns an error.

use std::collections::BTreeMap;

use crate::{
    container::Error,
    decode,
    ir::{
        BasicBlock, BlockId, InstSync, Instruction, NO_PHASE, OperandDataType, Target, UavBoundary,
    },
    shader_info::{
        ConstantBufferInfo, Declarations, PhaseInfo, ResourceRange, ResourceType, SamplerInfo,
        ScalerDataType, ShaderFlags, ShaderInfo, ShaderResourceViewInfo, ThreadgroupBufferInfo,
        UavFlags, UnorderedAccessViewInfo,
    },
    tokens::{
        self, Payload, Program, instruction, opcode, operand, program_type, resource_dimension,
        return_type,
    },
};

/// A flow instruction outside the construct it closes, or a phase count outside a phase.
pub const UNBALANCED: Error =
    Error::fail("Invalid DXBC bytecode: a flow instruction outside its construct");
/// A `call` to a label the program does not declare.
pub const UNDECLARED_LABEL: Error =
    Error::fail("Invalid DXBC bytecode: a call to an undeclared label");
/// A `callc` (the C++ aborted on every one).
pub const CONDITIONAL_CALL: Error = Error::fail("Invalid DXBC bytecode: a conditional call");
/// A declaration whose register index has a dimension the C++ has no case for.
pub const BAD_DECLARATION: Error =
    Error::fail("Invalid DXBC bytecode: a declaration's index has an unexpected dimension");
/// A typed view of a dimension or return type the translator has no type for.
pub const BAD_RESOURCE: Error =
    Error::fail("Invalid DXBC bytecode: a resource of an unknown dimension or return type");
/// Inlining the calls would make more blocks than [`MAX_BLOCKS`].
pub const TOO_MANY_BLOCKS: Error =
    Error::fail("Invalid DXBC bytecode: inlining the calls makes too many blocks");
/// The most blocks inlining may make (the C++ grew without bound).
pub const MAX_BLOCKS: usize = 1 << 20;

/// Where the walk hands the signature declarations (`handle_signature`).
pub trait DeclSink {
    /// One `dcl_input*` or `dcl_output*`, in program order, with the hull shader phase it is in.
    ///
    /// # Errors
    ///
    /// Whatever the sink refuses; the walk stops with it.
    fn signature(
        &mut self,
        decl: &tokens::Instruction,
        phase: u32,
        info: &mut ShaderInfo,
    ) -> Result<(), Error>;
}

/// A program's basic blocks (the entry first) and what its declarations set.
#[derive(Debug)]
pub struct Cfg {
    pub blocks: Vec<BasicBlock>,
    pub info: ShaderInfo,
    pub decls: Declarations,
}

/// `read_control_flow`: the program's blocks, declarations and uses.
///
/// # Errors
///
/// The parser's and the decoder's errors, the sink's, [`UNBALANCED`], [`UNDECLARED_LABEL`],
/// [`CONDITIONAL_CALL`], [`BAD_DECLARATION`], [`BAD_RESOURCE`] and [`TOO_MANY_BLOCKS`].
pub fn read(program: &Program, sink: &mut dyn DeclSink) -> Result<Cfg, Error> {
    let mut walk = Walk {
        blocks: Vec::new(),
        void: BasicBlock::new("voidbb"),
        info: ShaderInfo::default(),
        decls: Declarations::default(),
    };
    let mut func_entries: BTreeMap<u32, BlockId> = BTreeMap::new();
    let mut current = Slot::Block(walk.fresh("entrybb"));
    let ret = walk.fresh("returnbb");
    walk.at(Slot::Block(ret))?.target = Target::Return;
    let mut phase = NO_PHASE;
    let mut if_ends: Vec<BlockId> = Vec::new();
    let mut continue_targets: Vec<BlockId> = Vec::new();
    let mut break_targets: Vec<BlockId> = Vec::new();
    // The blocks whose terminator a `case` or a phase count still updates.
    let mut switches: Vec<Slot> = Vec::new();
    let mut instances: Vec<Slot> = Vec::new();
    let mut func_return_point: Option<BlockId> = None;
    let mut call_targets: Vec<Slot> = Vec::new();
    let sm51 = program.major_version() == 5 && program.minor_version() >= 1;
    let shader_type = program.program_type();

    for inst in program.instructions() {
        let inst = inst?;
        let op0 = &inst.operands[0];
        match (inst.opcode, &inst.payload) {
            (opcode::IF, _) => {
                let if_true = walk.fresh("if_true");
                let end_if = walk.fresh("end_if");
                if_ends.push(end_if);
                let cond = decode::read_condition(&inst, 0, phase)?;
                walk.at(current)?.target = Target::ConditionalBranch {
                    cond,
                    true_branch: if_true,
                    false_branch: end_if,
                };
                current = Slot::Block(if_true);
            }
            (opcode::ELSE, _) => {
                let end_if = walk.fresh("end_if");
                walk.at(current)?.target = Target::UnconditionalBranch(end_if);
                let alternative = if_ends.pop().ok_or(UNBALANCED)?;
                current = Slot::Block(alternative);
                // Until now it was the `if`'s `end_if`.
                walk.at(current)?.debug_name = "if_alternative";
                if_ends.push(end_if);
            }
            (opcode::ENDIF, _) => {
                let end_if = if_ends.pop().ok_or(UNBALANCED)?;
                walk.at(current)?.target = Target::UnconditionalBranch(end_if);
                current = Slot::Block(end_if);
            }
            (opcode::LOOP, _) => {
                let entrance = walk.fresh("loop_entrance");
                let end_loop = walk.fresh("end_loop");
                continue_targets.push(entrance);
                break_targets.push(end_loop);
                walk.at(current)?.target = Target::UnconditionalBranch(entrance);
                current = Slot::Block(entrance);
            }
            (opcode::ENDLOOP, _) => {
                let entrance = continue_targets.pop().ok_or(UNBALANCED)?;
                let end_loop = break_targets.pop().ok_or(UNBALANCED)?;
                walk.at(current)?.target = Target::UnconditionalBranch(entrance);
                current = Slot::Block(end_loop);
            }
            (opcode::BREAK | opcode::CONTINUE, _) => {
                let stack = if inst.opcode == opcode::BREAK {
                    &break_targets
                } else {
                    &continue_targets
                };
                let to = *stack.last().ok_or(UNBALANCED)?;
                walk.at(current)?.target = Target::UnconditionalBranch(to);
                current = Slot::Void;
            }
            (opcode::BREAKC | opcode::CONTINUEC, _) => {
                let (stack, name) = if inst.opcode == opcode::BREAKC {
                    (&break_targets, "after_breakc")
                } else {
                    (&continue_targets, "after_continuec")
                };
                let after = walk.fresh(name);
                let cond = decode::read_condition(&inst, 0, phase)?;
                let to = *stack.last().ok_or(UNBALANCED)?;
                walk.at(current)?.target = Target::ConditionalBranch {
                    cond,
                    true_branch: to,
                    false_branch: after,
                };
                current = Slot::Block(after);
            }
            (opcode::SWITCH, _) => {
                let end_switch = walk.fresh("end_switch");
                break_targets.push(end_switch);
                let value = decode::read_src_operand(op0, phase, OperandDataType::Integer)?;
                walk.at(current)?.target = Target::Switch {
                    value,
                    cases: BTreeMap::new(),
                    case_default: end_switch,
                };
                switches.push(current);
                current = Slot::Void;
            }
            (opcode::CASE, _) => {
                let case_body = walk.fresh("switch_case");
                let switch = *switches.last().ok_or(UNBALANCED)?;
                let Target::Switch { cases, .. } = &mut walk.at(switch)?.target else {
                    return Err(UNBALANCED);
                };
                // `std::map::insert`: the first block for a value stays.
                cases.entry(op0.value[0]).or_insert(case_body);
                walk.at(current)?.target = Target::UnconditionalBranch(case_body);
                current = Slot::Block(case_body);
            }
            (opcode::DEFAULT, _) => {
                let case_default = walk.fresh("case_default");
                let switch = *switches.last().ok_or(UNBALANCED)?;
                let Target::Switch {
                    case_default: slot, ..
                } = &mut walk.at(switch)?.target
                else {
                    return Err(UNBALANCED);
                };
                *slot = case_default;
                walk.at(current)?.target = Target::UnconditionalBranch(case_default);
                current = Slot::Block(case_default);
            }
            (opcode::ENDSWITCH, _) => {
                let end_switch = break_targets.pop().ok_or(UNBALANCED)?;
                switches.pop().ok_or(UNBALANCED)?;
                walk.at(current)?.target = Target::UnconditionalBranch(end_switch);
                current = Slot::Block(end_switch);
            }
            (opcode::RET, _) => {
                let to = walk.return_target(func_return_point, &instances, ret)?;
                walk.at(current)?.target = Target::UnconditionalBranch(to);
                if break_targets.is_empty() && if_ends.is_empty() {
                    // A return at the top level.
                    current = Slot::Block(to);
                    if func_return_point.is_some() {
                        current = Slot::Block(ret);
                        func_return_point = None;
                    } else {
                        // A hull shader phase's `ret` ends that phase only.
                        instances.pop();
                    }
                } else {
                    current = Slot::Void;
                }
            }
            (opcode::RETC, _) => {
                let to = walk.return_target(func_return_point, &instances, ret)?;
                let after = walk.fresh("after_retc");
                let cond = decode::read_condition(&inst, 0, phase)?;
                walk.at(current)?.target = Target::ConditionalBranch {
                    cond,
                    true_branch: to,
                    false_branch: after,
                };
                current = Slot::Block(after);
            }
            (opcode::DISCARD, _) => {
                let fulfilled = walk.fresh("discard_fulfilled");
                let otherwise = walk.fresh("discard_otherwise");
                let cond = decode::read_condition(&inst, 0, phase)?;
                walk.at(current)?.target = Target::ConditionalBranch {
                    cond,
                    true_branch: fulfilled,
                    false_branch: otherwise,
                };
                let block = walk.at(Slot::Block(fulfilled))?;
                block.target = Target::UnconditionalBranch(otherwise);
                block.instructions.push(Instruction::PixelDiscard);
                current = Slot::Block(otherwise);
            }
            (opcode::HS_CONTROL_POINT_PHASE | opcode::HS_FORK_PHASE | opcode::HS_JOIN_PHASE, _) => {
                let (names, instance_count) = if inst.opcode == opcode::HS_CONTROL_POINT_PHASE {
                    walk.info
                        .flags
                        .insert(ShaderFlags::NO_CONTROL_POINT_PHASE_PASSTHROUGH);
                    (
                        ["control_point_active", "control_point_end"],
                        walk.decls.output_control_point_count,
                    )
                } else {
                    phase = phase.wrapping_add(1);
                    walk.info.phases.push(PhaseInfo::default());
                    (["fork_join_active", "fork_join_end"], 1)
                };
                let active = walk.fresh(names[0]);
                let end = walk.fresh(names[1]);
                walk.at(Slot::Block(end))?
                    .instructions
                    .push(Instruction::Sync(InstSync {
                        uav_boundary: UavBoundary::None,
                        tgsm_memory_barrier: true,
                        tgsm_execution_barrier: true,
                    }));
                walk.at(current)?.target = Target::InstanceBarrier {
                    instance_count,
                    active,
                    sync: end,
                };
                instances.push(current);
                current = Slot::Block(active);
            }
            (
                opcode::DCL_HS_JOIN_PHASE_INSTANCE_COUNT | opcode::DCL_HS_FORK_PHASE_INSTANCE_COUNT,
                &Payload::Word(count),
            ) => {
                let phase_block = *instances.last().ok_or(UNBALANCED)?;
                let Target::InstanceBarrier { instance_count, .. } =
                    &mut walk.at(phase_block)?.target
                else {
                    return Err(UNBALANCED);
                };
                *instance_count = count;
                let max = &mut walk.decls.hull_maximum_threads_per_patch;
                *max = (*max).max(count);
            }
            (opcode::LABEL, _) => {
                let func_body = walk.fresh("func_body");
                let func_end = walk.fresh("func_end");
                walk.at(Slot::Block(func_end))?.target = Target::Return;
                func_entries.insert(op0.index[0].reg, func_body);
                func_return_point = Some(func_end);
                current = Slot::Block(func_body);
            }
            (opcode::CALL, _) => {
                let after = walk.fresh("after_call");
                walk.at(current)?.target = Target::Call {
                    func_id: op0.index[0].reg,
                    return_point: after,
                };
                call_targets.push(current);
                current = Slot::Block(after);
            }
            // `std::get<BasicBlockCall>` of the branch it had just made: the C++ aborted.
            (opcode::CALLC, _) => return Err(CONDITIONAL_CALL),
            (
                opcode::DCL_CONSTANT_BUFFER,
                &Payload::ConstantBuffer {
                    size: size_in_vec4,
                    space,
                    ..
                },
            ) => {
                let (lower_bound, size) = match op0.index_dimension {
                    operand::INDEX_2D => (op0.index[0].reg, 1),
                    operand::INDEX_3D => range(op0),
                    _ => return Err(BAD_DECLARATION),
                };
                walk.info.cbuffer_map.insert(
                    op0.index[0].reg,
                    ConstantBufferInfo {
                        range: binding(op0, lower_bound, size, space),
                        size_in_vec4,
                        arg_index: 0,
                    },
                );
            }
            (opcode::DCL_SAMPLER, &Payload::Sampler { space, .. }) => {
                let (lower_bound, size) = match op0.index_dimension {
                    operand::INDEX_1D => (op0.index[0].reg, 1),
                    operand::INDEX_3D => range(op0),
                    _ => return Err(BAD_DECLARATION),
                };
                walk.info.sampler_map.insert(
                    op0.index[0].reg,
                    SamplerInfo {
                        range: binding(op0, lower_bound, size, space),
                        ..SamplerInfo::default()
                    },
                );
            }
            (
                opcode::DCL_RESOURCE | opcode::DCL_RESOURCE_RAW | opcode::DCL_RESOURCE_STRUCTURED,
                payload,
            ) => {
                let (lower_bound, size) = if sm51 {
                    range(op0)
                } else {
                    (op0.index[0].reg, 1)
                };
                let (space, resource_type, scaler_type, structure_stride) = match *payload {
                    Payload::Resource {
                        dimension,
                        return_type,
                        space,
                        ..
                    } => (
                        space,
                        resource_type_of(dimension)?,
                        scaler_type_of(return_type[0])?,
                        u32::MAX,
                    ),
                    Payload::Raw { space, .. } => {
                        (space, ResourceType::NonApplicable, ScalerDataType::Uint, 0)
                    }
                    Payload::Structured { stride, space, .. } => (
                        space,
                        ResourceType::NonApplicable,
                        ScalerDataType::Uint,
                        stride,
                    ),
                    _ => (0, ResourceType::default(), ScalerDataType::default(), 0),
                };
                walk.info.srv_map.insert(
                    op0.index[0].reg,
                    ShaderResourceViewInfo {
                        range: binding(op0, lower_bound, size, space),
                        scaler_type,
                        resource_type,
                        structure_stride,
                        ..ShaderResourceViewInfo::default()
                    },
                );
            }
            (
                opcode::DCL_UNORDERED_ACCESS_VIEW_TYPED
                | opcode::DCL_UNORDERED_ACCESS_VIEW_RAW
                | opcode::DCL_UNORDERED_ACCESS_VIEW_STRUCTURED,
                payload,
            ) => {
                let (lower_bound, size) = if sm51 {
                    range(op0)
                } else {
                    (op0.index[0].reg, 1)
                };
                let (space, flags, resource_type, scaler_type, structure_stride) = match *payload {
                    Payload::TypedUav {
                        dimension,
                        flags,
                        return_type,
                        space,
                    } => (
                        space,
                        flags,
                        resource_type_of(dimension)?,
                        scaler_type_of(return_type[0])?,
                        u32::MAX,
                    ),
                    Payload::Raw { flags, space } => (
                        space,
                        flags,
                        ResourceType::NonApplicable,
                        ScalerDataType::Uint,
                        0,
                    ),
                    Payload::Structured {
                        flags,
                        stride,
                        space,
                    } => (
                        space,
                        flags,
                        ResourceType::NonApplicable,
                        ScalerDataType::Uint,
                        stride,
                    ),
                    _ => (0, 0, ResourceType::default(), ScalerDataType::default(), 0),
                };
                let mut uav_flags = UavFlags::default();
                if flags & instruction::GLOBALLY_COHERENT_ACCESS != 0 {
                    uav_flags.insert(UavFlags::GLOBAL_COHERENT);
                }
                if flags & instruction::RASTERIZER_ORDERED_ACCESS != 0 {
                    uav_flags.insert(UavFlags::RASTERIZER_ORDER);
                }
                walk.info.uav_map.insert(
                    op0.index[0].reg,
                    UnorderedAccessViewInfo {
                        range: binding(op0, lower_bound, size, space),
                        scaler_type,
                        resource_type,
                        flags: uav_flags,
                        structure_stride,
                        ..UnorderedAccessViewInfo::default()
                    },
                );
            }
            (opcode::DCL_TEMPS, &Payload::Word(count)) => {
                *walk.temp_count(phase)? = count;
            }
            (
                opcode::DCL_INDEXABLE_TEMP,
                &Payload::IndexableTemp {
                    number,
                    registers,
                    mask,
                },
            ) => {
                walk.indexable_temps(phase)?
                    .insert(number, (registers, mask >> 4));
            }
            (opcode::DCL_THREAD_GROUP, &Payload::ThreadGroup(size)) => {
                walk.decls.threadgroup_size = size;
                walk.decls.max_workgroup_size = size[0].wrapping_mul(size[1]).wrapping_mul(size[2]);
            }
            (opcode::DCL_THREAD_GROUP_SHARED_MEMORY_RAW, &Payload::Word(size)) => {
                walk.info.tgsm_map.insert(
                    op0.index[0].reg,
                    ThreadgroupBufferInfo {
                        size_in_uint: size / 4,
                        size,
                        stride: 0,
                        structured: false,
                    },
                );
            }
            (
                opcode::DCL_THREAD_GROUP_SHARED_MEMORY_STRUCTURED,
                &Payload::Tgsm { stride, count },
            ) => {
                walk.info.tgsm_map.insert(
                    op0.index[0].reg,
                    ThreadgroupBufferInfo {
                        size_in_uint: stride.wrapping_mul(count) / 4,
                        size: count,
                        stride,
                        structured: true,
                    },
                );
            }
            (opcode::DCL_GLOBAL_FLAGS, &Payload::Word(flags)) => {
                if flags & instruction::GLOBAL_FLAG_FORCE_EARLY_DEPTH_STENCIL != 0 {
                    walk.decls.early_fragment_tests = true;
                }
                if flags & instruction::GLOBAL_FLAG_SKIP_OPTIMIZATION != 0 {
                    walk.info.flags.insert(ShaderFlags::SKIP_OPTIMIZATION);
                }
                if flags & instruction::GLOBAL_FLAG_REFACTORING_ALLOWED != 0 {
                    walk.info.flags.insert(ShaderFlags::REFACTORING_ALLOWED);
                }
            }
            (
                opcode::DCL_INPUT_SIV
                | opcode::DCL_INPUT_SGV
                | opcode::DCL_INPUT
                | opcode::DCL_INPUT_PS_SIV
                | opcode::DCL_INPUT_PS_SGV
                | opcode::DCL_INPUT_PS
                | opcode::DCL_OUTPUT_SGV
                | opcode::DCL_OUTPUT_SIV
                | opcode::DCL_OUTPUT,
                _,
            ) => sink.signature(&inst, phase, &mut walk.info)?,
            (opcode::CUSTOMDATA, Payload::CustomData { class, data }) => {
                if *class == instruction::CUSTOMDATA_DCL_IMMEDIATE_CONSTANT_BUFFER {
                    // Whole vectors only, as the C++'s `DataSizeInBytes >> 4`.
                    walk.info.imm_constant_buffer_data = data.as_chunks::<4>().0.to_vec();
                }
            }
            (opcode::DCL_GS_INPUT_PRIMITIVE, &Payload::Word(value)) => {
                walk.decls.gs_input_primitive = value;
            }
            (opcode::DCL_GS_OUTPUT_PRIMITIVE_TOPOLOGY, &Payload::Word(value)) => {
                walk.decls.gs_output_topology = value;
            }
            (opcode::DCL_MAX_OUTPUT_VERTEX_COUNT, &Payload::Word(value)) => {
                walk.decls.gs_max_vertex_output = value;
            }
            (opcode::DCL_GS_INSTANCE_COUNT, &Payload::Word(value)) => {
                walk.decls.gs_instance_count = value;
            }
            (opcode::DCL_TESS_PARTITIONING, &Payload::Word(value)) => {
                walk.decls.tessellation_partition = value;
            }
            (opcode::DCL_TESS_OUTPUT_PRIMITIVE, &Payload::Word(value)) => {
                walk.decls.tessellator_output_primitive = value;
            }
            (opcode::DCL_TESS_DOMAIN, &Payload::Word(value)) => {
                walk.decls.tessellation_domain = value;
            }
            (opcode::DCL_HS_MAX_TESSFACTOR, &Payload::Word(bits)) => {
                walk.decls.max_tessellation_factor = f32::from_bits(bits);
            }
            (opcode::DCL_INPUT_CONTROL_POINT_COUNT, &Payload::Word(count)) => {
                walk.decls.input_control_point_count = count;
                let max = &mut walk.decls.hull_maximum_threads_per_patch;
                *max = (*max).max(count);
            }
            (opcode::DCL_OUTPUT_CONTROL_POINT_COUNT, &Payload::Word(count)) => {
                walk.decls.output_control_point_count = count;
                let max = &mut walk.decls.hull_maximum_threads_per_patch;
                *max = (*max).max(count);
            }
            // The backend handles an indexed range as an array; streams, interfaces and the
            // hull shader's declaration phase change nothing here.
            (
                opcode::DCL_INDEX_RANGE
                | opcode::DCL_STREAM
                | opcode::DCL_INTERFACE
                | opcode::DCL_FUNCTION_TABLE
                | opcode::DCL_FUNCTION_BODY
                | opcode::HS_DECLS,
                _,
            ) => {}
            // Only stream 0 exists; the C++ asserted it and emitted to 0 whatever the operand.
            (opcode::EMIT | opcode::EMIT_STREAM, _) => {
                walk.at(current)?.instructions.push(Instruction::Emit);
            }
            (opcode::EMITTHENCUT | opcode::EMITTHENCUT_STREAM, _) => {
                let block = walk.at(current)?;
                block.instructions.push(Instruction::Emit);
                block.instructions.push(Instruction::Cut);
            }
            (opcode::CUT | opcode::CUT_STREAM, _) => {
                walk.at(current)?.instructions.push(Instruction::Cut);
            }
            _ => {
                let decoded = decode::read_instruction(&inst, &mut walk.info, phase)?;
                walk.at(current)?.instructions.push(decoded);
            }
        }
    }

    if current != Slot::Block(ret) {
        // A hull shader copies its control points out from threadgroup memory when a later
        // phase reads them or it has no control point phase; otherwise they are already out.
        let flags = walk.info.flags;
        walk.at(current)?.target = if shader_type == program_type::HULL
            && (flags.contains(ShaderFlags::OUTPUT_CONTROL_POINT_READ)
                || !flags.contains(ShaderFlags::NO_CONTROL_POINT_PHASE_PASSTHROUGH))
        {
            Target::HullShaderWriteOutput {
                instance_count: walk.decls.output_control_point_count,
                epilogue: ret,
            }
        } else {
            Target::UnconditionalBranch(ret)
        };
    }

    walk.inline_calls(call_targets, &func_entries)?;
    Ok(Cfg {
        blocks: walk.blocks,
        info: walk.info,
        decls: walk.decls,
    })
}

/// A block in the list, or the block that drops what follows a `break` (`bb_void`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Block(BlockId),
    Void,
}

/// The walk's state that its helpers reach.
struct Walk {
    blocks: Vec<BasicBlock>,
    void: BasicBlock,
    info: ShaderInfo,
    decls: Declarations,
}

impl Walk {
    /// `fresh_bb`: a new block at the end of the list.
    fn fresh(&mut self, name: &'static str) -> BlockId {
        BlockId::push(&mut self.blocks, name)
    }

    fn at(&mut self, slot: Slot) -> Result<&mut BasicBlock, Error> {
        match slot {
            Slot::Void => Ok(&mut self.void),
            Slot::Block(id) => self.blocks.get_mut(id.index()).ok_or(UNBALANCED),
        }
    }

    /// Where a `ret` goes: the function's end, the hull shader phase's end, or the shader's.
    fn return_target(
        &mut self,
        func_return_point: Option<BlockId>,
        instances: &[Slot],
        ret: BlockId,
    ) -> Result<BlockId, Error> {
        if let Some(end) = func_return_point {
            return Ok(end);
        }
        let Some(&phase_block) = instances.last() else {
            return Ok(ret);
        };
        match self.at(phase_block)?.target {
            Target::InstanceBarrier { sync, .. } => Ok(sync),
            _ => Err(UNBALANCED),
        }
    }

    /// The temp count of the phase, or of the shader outside one.
    fn temp_count(&mut self, phase: u32) -> Result<&mut u32, Error> {
        if phase == NO_PHASE {
            return Ok(&mut self.info.temp_register_count);
        }
        let index = usize::try_from(phase).map_err(|_| UNBALANCED)?;
        let phase = self.info.phases.get_mut(index).ok_or(UNBALANCED)?;
        Ok(&mut phase.temp_register_count)
    }

    /// The indexable temps of the phase, or of the shader outside one.
    fn indexable_temps(&mut self, phase: u32) -> Result<&mut BTreeMap<u32, (u32, u32)>, Error> {
        if phase == NO_PHASE {
            return Ok(&mut self.info.indexable_temp_register_counts);
        }
        let index = usize::try_from(phase).map_err(|_| UNBALANCED)?;
        let phase = self.info.phases.get_mut(index).ok_or(UNBALANCED)?;
        Ok(&mut phase.indexable_temp_register_counts)
    }

    /// Inlines every call, 32 levels deep; a call still left at the last level jumps past.
    ///
    /// Each call's block jumps to a fresh copy of the function's blocks, the copies made in the
    /// C++'s order (the branch's true side before its false one, a switch's default before its
    /// cases, a phase's active block before its end; the work list last in, first out), and the
    /// copy's returns go to the call's return point.
    fn inline_calls(
        &mut self,
        mut call_targets: Vec<Slot>,
        func_entries: &BTreeMap<u32, BlockId>,
    ) -> Result<(), Error> {
        for depth in (1..=32).rev() {
            if call_targets.is_empty() {
                break;
            }
            let mut new_call_targets = Vec::new();
            for call_block in call_targets {
                let Target::Call {
                    func_id,
                    return_point,
                } = self.at(call_block)?.target
                else {
                    // The C++ followed a pointer into a terminator since overwritten.
                    return Err(UNBALANCED);
                };
                let entry = *func_entries.get(&func_id).ok_or(UNDECLARED_LABEL)?;
                let mut copies = Copies::default();
                let first = self.copy(&mut copies, entry)?;
                self.at(call_block)?.target = Target::UnconditionalBranch(first);
                while let Some(original) = copies.work.pop() {
                    let copy = *copies.made.get(&original).ok_or(UNBALANCED)?;
                    let old = self.at(Slot::Block(original))?.target.clone();
                    let new = match old {
                        Target::Undefined => Target::Undefined,
                        Target::Return => Target::UnconditionalBranch(return_point),
                        Target::UnconditionalBranch(to) => {
                            Target::UnconditionalBranch(self.copy(&mut copies, to)?)
                        }
                        Target::ConditionalBranch {
                            cond,
                            true_branch,
                            false_branch,
                        } => {
                            let true_branch = self.copy(&mut copies, true_branch)?;
                            let false_branch = self.copy(&mut copies, false_branch)?;
                            Target::ConditionalBranch {
                                cond,
                                true_branch,
                                false_branch,
                            }
                        }
                        Target::Switch {
                            value,
                            cases,
                            case_default,
                        } => {
                            let case_default = self.copy(&mut copies, case_default)?;
                            let mut new_cases = BTreeMap::new();
                            for (case, to) in cases {
                                new_cases.insert(case, self.copy(&mut copies, to)?);
                            }
                            Target::Switch {
                                value,
                                cases: new_cases,
                                case_default,
                            }
                        }
                        Target::InstanceBarrier {
                            instance_count,
                            active,
                            sync,
                        } => {
                            let active = self.copy(&mut copies, active)?;
                            let sync = self.copy(&mut copies, sync)?;
                            Target::InstanceBarrier {
                                instance_count,
                                active,
                                sync,
                            }
                        }
                        Target::HullShaderWriteOutput {
                            instance_count,
                            epilogue,
                        } => Target::HullShaderWriteOutput {
                            instance_count,
                            epilogue: self.copy(&mut copies, epilogue)?,
                        },
                        Target::Call {
                            func_id,
                            return_point,
                        } => {
                            let return_point = self.copy(&mut copies, return_point)?;
                            if depth == 1 {
                                Target::UnconditionalBranch(return_point)
                            } else {
                                new_call_targets.push(Slot::Block(copy));
                                Target::Call {
                                    func_id,
                                    return_point,
                                }
                            }
                        }
                    };
                    self.at(Slot::Block(copy))?.target = new;
                }
            }
            call_targets = new_call_targets;
        }
        Ok(())
    }

    /// The copy of `original` for this call, made (instructions, name, an undefined end) if new.
    fn copy(&mut self, copies: &mut Copies, original: BlockId) -> Result<BlockId, Error> {
        if let Some(&copy) = copies.made.get(&original) {
            return Ok(copy);
        }
        if self.blocks.len() >= MAX_BLOCKS {
            return Err(TOO_MANY_BLOCKS);
        }
        let block = self.at(Slot::Block(original))?;
        let (name, instructions) = (block.debug_name, block.instructions.clone());
        let copy = self.fresh(name);
        self.at(Slot::Block(copy))?.instructions = instructions;
        copies.made.insert(original, copy);
        copies.work.push(original);
        Ok(copy)
    }
}

/// One call's copies: original to copy, and the originals whose ends are still to redirect.
#[derive(Default)]
struct Copies {
    made: BTreeMap<BlockId, BlockId>,
    work: Vec<BlockId>,
}

/// A 5.1 declaration's lower bound (second index) and size (to the third, `u32::MAX` open).
const fn range(o: &tokens::Operand) -> (u32, u32) {
    let lower_bound = o.index[1].reg;
    let upper = o.index[2].reg;
    let size = if upper == u32::MAX {
        u32::MAX
    } else {
        upper.wrapping_sub(lower_bound).wrapping_add(1)
    };
    (lower_bound, size)
}

const fn binding(o: &tokens::Operand, lower_bound: u32, size: u32, space: u32) -> ResourceRange {
    ResourceRange {
        range_id: o.index[0].reg,
        lower_bound,
        size,
        space,
    }
}

/// `to_shader_resource_type`.
const fn resource_type_of(dimension: u32) -> Result<ResourceType, Error> {
    Ok(match dimension {
        resource_dimension::UNKNOWN
        | resource_dimension::BUFFER
        | resource_dimension::RAW_BUFFER
        | resource_dimension::STRUCTURED_BUFFER => ResourceType::TextureBuffer,
        resource_dimension::TEXTURE1D => ResourceType::Texture1D,
        resource_dimension::TEXTURE2D => ResourceType::Texture2D,
        resource_dimension::TEXTURE2DMS => ResourceType::Texture2DMultisampled,
        resource_dimension::TEXTURE3D => ResourceType::Texture3D,
        resource_dimension::TEXTURECUBE => ResourceType::TextureCube,
        resource_dimension::TEXTURE1DARRAY => ResourceType::Texture1DArray,
        resource_dimension::TEXTURE2DARRAY => ResourceType::Texture2DArray,
        resource_dimension::TEXTURE2DMSARRAY => ResourceType::Texture2DMultisampledArray,
        resource_dimension::TEXTURECUBEARRAY => ResourceType::TextureCubeArray,
        _ => return Err(BAD_RESOURCE),
    })
}

/// `to_shader_scaler_type`: mixed reads as unsigned, as the C++'s.
const fn scaler_type_of(ty: u32) -> Result<ScalerDataType, Error> {
    Ok(match ty {
        return_type::UNORM | return_type::SNORM | return_type::FLOAT => ScalerDataType::Float,
        return_type::SINT => ScalerDataType::Int,
        return_type::UINT | return_type::MIXED => ScalerDataType::Uint,
        _ => return Err(BAD_RESOURCE),
    })
}

#[cfg(test)]
mod tests;
