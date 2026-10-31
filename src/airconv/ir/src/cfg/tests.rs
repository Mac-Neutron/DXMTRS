use super::*;
use crate::{
    container::{Container, fourcc},
    ir::{AtomicDst, AtomicOperandTgsm, InputAttribute, InstStoreStructured, SrcOperand},
    shader_info::UavFlags,
    tokens::test_part,
};

/// A sink that takes every declaration.
struct Accept;

impl DeclSink for Accept {
    fn signature(
        &mut self,
        _decl: &tokens::Instruction,
        _phase: u32,
        _info: &mut ShaderInfo,
    ) -> Result<(), Error> {
        Ok(())
    }
}

fn walk(part: &[u8]) -> Result<Cfg, Error> {
    read(&Program::new(part)?, &mut Accept)
}

fn compute(words: &[u32]) -> Result<Cfg, Error> {
    walk(&test_part(program_type::COMPUTE, false, words))
}

/// Each block's name and end, with the ends' conditions and switch values left out.
fn shape(cfg: &Cfg) -> Vec<(&'static str, String)> {
    cfg.blocks
        .iter()
        .map(|block| {
            let end = match &block.target {
                Target::ConditionalBranch {
                    true_branch,
                    false_branch,
                    ..
                } => format!("if {} else {}", true_branch.index(), false_branch.index()),
                Target::UnconditionalBranch(to) => format!("to {}", to.index()),
                Target::Switch {
                    cases,
                    case_default,
                    ..
                } => {
                    let cases: Vec<String> = cases
                        .iter()
                        .map(|(v, to)| format!("{v}:{}", to.index()))
                        .collect();
                    format!(
                        "switch {} default {}",
                        cases.join(","),
                        case_default.index()
                    )
                }
                other => format!("{other:?}"),
            };
            (block.debug_name, end)
        })
        .collect()
}

// `ir/tests/data/sync.dxbc`, read once from the C++ by `$S/p2/cfgdump` (P2's scratch dumper over
// `read_control_flow` at 0439fa9, built NDEBUG): two blocks, the entry's 23 instructions and the
// uses below.
#[test]
fn cfg_of_sync_dxbc() {
    let bytes = include_bytes!("../../tests/data/sync.dxbc");
    let container = Container::parse(bytes).unwrap();
    let part = container
        .find(fourcc::SHEX)
        .or_else(|| container.find(fourcc::SHDR))
        .unwrap();
    let cfg = walk(part).unwrap();
    assert_eq!(
        shape(&cfg),
        [
            ("entrybb", "to 1".to_owned()),
            ("returnbb", "Return".to_owned())
        ]
    );
    let kinds: Vec<&str> = cfg.blocks[0]
        .instructions
        .iter()
        .map(|inst| match inst {
            Instruction::StoreStructured(_) => "store_structured",
            Instruction::Sync(_) => "sync",
            Instruction::IntegerUnaryOp(_) => "ineg",
            Instruction::IntegerBinaryOp(_) => "int",
            Instruction::LoadStructured(_) => "ld_structured",
            Instruction::StoreUavTyped(_) => "store_uav_typed",
            Instruction::Mov(_) => "mov",
            Instruction::LoadUavTyped(_) => "ld_uav_typed",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "store_structured",
            "sync",
            "sync",
            "ineg",
            "int",
            "ld_structured",
            "store_uav_typed",
            "sync",
            "sync",
            "int",
            "int",
            "mov",
            "mov",
            "mov",
            "ld_uav_typed",
            "int",
            "store_structured",
            "sync",
            "sync",
            "int",
            "int",
            "ld_structured",
            "store_uav_typed"
        ]
    );
    // The first store: g0's first word, at vThreadIDInGroupFlattened.
    let Instruction::StoreStructured(InstStoreStructured {
        dst, dst_address, ..
    }) = &cfg.blocks[0].instructions[0]
    else {
        panic!();
    };
    assert!(matches!(
        dst,
        AtomicDst::Tgsm(AtomicOperandTgsm { id: 0, mask: 1 })
    ));
    assert!(matches!(
        dst_address,
        SrcOperand::Attribute {
            attribute: InputAttribute::ThreadIdInGroupFlatten,
            ..
        }
    ));
    let uav = &cfg.info.uav_map[&0];
    assert_eq!(uav.flags, {
        let mut flags = UavFlags::READ;
        flags.insert(UavFlags::WRITTEN);
        flags
    });
    assert_eq!(uav.scaler_type, ScalerDataType::Uint);
    assert_eq!(
        cfg.info.tgsm_map[&0],
        ThreadgroupBufferInfo {
            size_in_uint: 64,
            size: 64,
            stride: 4,
            structured: true,
        }
    );
    assert_eq!(cfg.info.temp_register_count, 1);
    assert_eq!(cfg.decls.threadgroup_size, [64, 1, 1]);
    assert_eq!(cfg.decls.max_workgroup_size, 64);
}

const R0X: [u32; 2] = [0x0010_000a, 0];
const MOV: [u32; 5] = [opcode::MOV | (5 << 24), 0x0010_0032, 0, 0x0010_0396, 1];
const L0: [u32; 2] = [0x0010_a000, 0];

const fn op(opcode: u32, length: u32) -> u32 {
    opcode | (length << 24)
}

// The C++'s blocks for this program, printed by `$S/p2/cfgdump` (as above): if/else around a loop
// whose body calls `l0`, a switch with a case and a default, then `l0` with a nested `ret`.
#[test]
fn control_flow_matches_the_cxx() {
    let words = [
        &[op(opcode::DCL_TEMPS, 2), 2][..],
        &[op(opcode::IF, 3) | (1 << 18)],
        &R0X,
        &MOV,
        &[
            op(opcode::ELSE, 1),
            op(opcode::LOOP, 1),
            op(opcode::BREAKC, 3),
        ],
        &R0X,
        &[op(opcode::CALL, 3)],
        &L0,
        &[
            op(opcode::ENDLOOP, 1),
            op(opcode::ENDIF, 1),
            op(opcode::SWITCH, 3),
        ],
        &R0X,
        &[op(opcode::CASE, 3), 0x4001, 1],
        &MOV,
        &[
            op(opcode::BREAK, 1),
            op(opcode::DEFAULT, 1),
            op(opcode::BREAK, 1),
            op(opcode::ENDSWITCH, 1),
            op(opcode::RET, 1),
            op(opcode::LABEL, 3),
        ],
        &L0,
        &[op(opcode::IF, 3)],
        &R0X,
        &[op(opcode::RET, 1), op(opcode::ENDIF, 1)],
        &MOV,
        &[op(opcode::RET, 1)],
    ]
    .concat();
    let cfg = compute(&words).unwrap();
    let expected = [
        ("entrybb", "if 2 else 3"),
        ("returnbb", "Return"),
        ("if_true", "to 4"),
        ("if_alternative", "to 5"),
        ("end_if", "switch 1:10 default 11"),
        ("loop_entrance", "if 6 else 7"),
        ("end_loop", "to 4"),
        ("after_breakc", "to 16"),
        ("after_call", "to 5"),
        ("end_switch", "to 1"),
        ("switch_case", "to 9"),
        ("case_default", "to 9"),
        ("func_body", "if 14 else 15"),
        ("func_end", "Return"),
        ("if_true", "to 13"),
        ("end_if", "to 13"),
        // The inlined copy of `l0`, its returns sent to the call's return point.
        ("func_body", "if 17 else 18"),
        ("if_true", "to 19"),
        ("end_if", "to 19"),
        ("func_end", "to 8"),
    ];
    let shape = shape(&cfg);
    let got: Vec<(&str, &str)> = shape.iter().map(|(n, e)| (*n, e.as_str())).collect();
    assert_eq!(got, expected);
    assert_eq!(cfg.blocks[18].instructions.len(), 1);
    // `breakc_z`: the test is for zero.
    let Target::ConditionalBranch { cond, .. } = &cfg.blocks[5].target else {
        panic!();
    };
    assert!(!cond.test_nonzero);
}

#[test]
fn hull_phases_end_with_the_control_point_copy() {
    // hs_control_point_phase; ret; hs_fork_phase; dcl_hs_fork_phase_instance_count 3; ret
    let words = [
        op(opcode::DCL_OUTPUT_CONTROL_POINT_COUNT, 1) | (4 << 11),
        op(opcode::HS_CONTROL_POINT_PHASE, 1),
        op(opcode::RET, 1),
        op(opcode::HS_FORK_PHASE, 1),
        op(opcode::DCL_HS_FORK_PHASE_INSTANCE_COUNT, 2),
        3,
        op(opcode::RET, 1),
    ];
    let cfg = walk(&test_part(program_type::HULL, false, &words)).unwrap();
    let shape = shape(&cfg);
    let got: Vec<(&str, &str)> = shape.iter().map(|(n, e)| (*n, e.as_str())).collect();
    assert_eq!(
        got,
        [
            (
                "entrybb",
                "InstanceBarrier { instance_count: 4, active: BlockId(2), sync: BlockId(3) }"
            ),
            ("returnbb", "Return"),
            ("control_point_active", "to 3"),
            (
                "control_point_end",
                "InstanceBarrier { instance_count: 3, active: BlockId(4), sync: BlockId(5) }"
            ),
            ("fork_join_active", "to 5"),
            // No phase reads the control points and the control point phase wrote them out.
            ("fork_join_end", "to 1"),
        ]
    );
    assert_eq!(cfg.info.phases.len(), 1);
    assert_eq!(cfg.decls.hull_maximum_threads_per_patch, 4);
}

// As above: a `ret` ends its hull shader phase only, so a second `ret` goes to the shader's end
// and what follows lands in the return block.
#[test]
fn hull_phase_ret_pops_the_phase() {
    let words = [
        &[
            op(opcode::DCL_OUTPUT_CONTROL_POINT_COUNT, 1) | (4 << 11),
            op(opcode::HS_CONTROL_POINT_PHASE, 1),
            op(opcode::RET, 1),
            op(opcode::RET, 1),
        ][..],
        &MOV,
    ]
    .concat();
    let cfg = walk(&test_part(program_type::HULL, false, &words)).unwrap();
    let shape = shape(&cfg);
    let got: Vec<(&str, &str)> = shape.iter().map(|(n, e)| (*n, e.as_str())).collect();
    assert_eq!(
        got,
        [
            (
                "entrybb",
                "InstanceBarrier { instance_count: 4, active: BlockId(2), sync: BlockId(3) }"
            ),
            ("returnbb", "Return"),
            ("control_point_active", "to 3"),
            ("control_point_end", "to 1"),
        ]
    );
    assert!(matches!(
        cfg.blocks[1].instructions[..],
        [Instruction::Mov(_)]
    ));
    assert!(matches!(
        cfg.blocks[3].instructions[..],
        [Instruction::Sync(_)]
    ));
}

// As above: of two `case 1`, the first keeps the value (`std::map::insert`).
#[test]
fn first_case_of_a_value_stays() {
    let words = [
        &[op(opcode::SWITCH, 3)][..],
        &R0X,
        &[op(opcode::CASE, 3), 0x4001, 1],
        &MOV,
        &[op(opcode::BREAK, 1), op(opcode::CASE, 3), 0x4001, 1],
        &[
            op(opcode::BREAK, 1),
            op(opcode::ENDSWITCH, 1),
            op(opcode::RET, 1),
        ],
    ]
    .concat();
    let cfg = compute(&words).unwrap();
    assert_eq!(
        shape(&cfg)[0],
        ("entrybb", "switch 1:3 default 2".to_owned())
    );
    assert_eq!(cfg.blocks[3].instructions.len(), 1);
}

// As above: the copy of `l0` is made last in, first out: its end (12) before the inner `if`'s
// blocks (13, 14).
#[test]
fn inlined_copies_follow_the_cxx_order() {
    let words = [
        &[op(opcode::CALL, 3)][..],
        &L0,
        &[op(opcode::RET, 1), op(opcode::LABEL, 3)],
        &L0,
        &[op(opcode::IF, 3) | (1 << 18)],
        &R0X,
        &[op(opcode::IF, 3) | (1 << 18)],
        &R0X,
        &MOV,
        &[op(opcode::ENDIF, 1)],
        &MOV,
        &[op(opcode::ENDIF, 1)],
        &MOV,
        &[op(opcode::RET, 1)],
    ]
    .concat();
    let cfg = compute(&words).unwrap();
    let shape = shape(&cfg);
    let got: Vec<(&str, &str)> = shape.iter().map(|(n, e)| (*n, e.as_str())).collect();
    assert_eq!(
        got,
        [
            ("entrybb", "to 9"),
            ("returnbb", "Return"),
            ("after_call", "to 1"),
            ("func_body", "if 5 else 6"),
            ("func_end", "Return"),
            ("if_true", "if 7 else 8"),
            ("end_if", "to 4"),
            ("if_true", "to 8"),
            ("end_if", "to 6"),
            ("func_body", "if 10 else 11"),
            ("if_true", "if 13 else 14"),
            ("end_if", "to 12"),
            ("func_end", "to 2"),
            ("if_true", "to 14"),
            ("end_if", "to 11"),
        ]
    );
}

#[test]
fn what_crashed_the_cxx_is_an_error() {
    // `else` with no `if`: the C++ took the top of an empty stack.
    assert_eq!(compute(&[op(opcode::ELSE, 1)]).unwrap_err(), UNBALANCED);
    // `case` outside a switch.
    assert_eq!(
        compute(&[op(opcode::CASE, 3), 0x4001, 1]).unwrap_err(),
        UNBALANCED
    );
    // A phase count outside a phase.
    assert_eq!(
        compute(&[op(opcode::DCL_HS_FORK_PHASE_INSTANCE_COUNT, 2), 1]).unwrap_err(),
        UNBALANCED
    );
    // `callc`: `std::get<BasicBlockCall>` of a conditional branch aborted.
    let callc = [&[op(opcode::CALLC, 5) | (1 << 18)][..], &R0X, &L0].concat();
    assert_eq!(compute(&callc).unwrap_err(), CONDITIONAL_CALL);
    // `call` to a label nobody declares: `std::map::at` threw.
    let call = [&[op(opcode::CALL, 3)][..], &L0, &[op(opcode::RET, 1)]].concat();
    assert_eq!(compute(&call).unwrap_err(), UNDECLARED_LABEL);
    // A cbuffer declared with a 1D index: the C++ left its range uninitialised.
    let cb_1d = [op(opcode::DCL_CONSTANT_BUFFER, 3), 0x0010_8000, 0];
    assert_eq!(compute(&cb_1d).unwrap_err(), BAD_DECLARATION);
}

#[test]
fn inlining_stops_at_the_block_limit() {
    // l0 calls l1 twice, l1 calls l2 twice, ... 21 levels: 2^21 copies of the last body.
    let mut words = vec![op(opcode::CALL, 3), 0x0010_a000, 0, op(opcode::RET, 1)];
    for label in 0..21 {
        words.extend([op(opcode::LABEL, 3), 0x0010_a000, label]);
        for _ in 0..2 {
            words.extend([op(opcode::CALL, 3), 0x0010_a000, label + 1]);
        }
        words.push(op(opcode::RET, 1));
    }
    words.extend([op(opcode::LABEL, 3), 0x0010_a000, 21]);
    words.extend(MOV);
    words.push(op(opcode::RET, 1));
    assert_eq!(compute(&words).unwrap_err(), TOO_MANY_BLOCKS);
}

#[test]
fn immediate_constant_buffer_keeps_whole_vectors() {
    let icb = opcode::CUSTOMDATA | (instruction::CUSTOMDATA_DCL_IMMEDIATE_CONSTANT_BUFFER << 11);
    let cfg = compute(&[icb, 7, 1, 2, 3, 4, 5, op(opcode::RET, 1)]).unwrap();
    assert_eq!(cfg.info.imm_constant_buffer_data, [[1, 2, 3, 4]]);
}
