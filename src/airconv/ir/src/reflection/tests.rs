use super::*;
use crate::ir::{BlockId, Instruction};

/// An entry block holding `instructions` that branches to a returning block.
fn blocks(instructions: Vec<Instruction>) -> Vec<BasicBlock> {
    let mut blocks = Vec::new();
    let entry = BlockId::push(&mut blocks, "entrybb");
    let ret = BlockId::push(&mut blocks, "returnbb");
    blocks[entry.index()].instructions = instructions;
    blocks[entry.index()].target = Target::UnconditionalBranch(ret);
    blocks[ret.index()].target = Target::Return;
    blocks
}

#[test]
fn gs_blocks_with_any_instruction_are_no_pass_through() {
    assert_eq!(gs_blocks_pass_through(&blocks(vec![])), Ok(true));
    // The C++ meant an emit to pass; its `const` dispatch let no instruction pass.
    assert_eq!(
        gs_blocks_pass_through(&blocks(vec![Instruction::Emit])),
        Ok(false)
    );
    let mut cycle = blocks(vec![]);
    cycle[1].target = Target::UnconditionalBranch(BlockId::push(&mut Vec::new(), "x"));
    assert_eq!(gs_blocks_pass_through(&cycle), Err(GS_LOOPS_FOREVER));
}

#[test]
fn semantic_kind_names_depth_and_stencil_outputs() {
    assert_eq!(semantic_kind(sb_name::POSITION, b"SV_Position"), 3);
    assert_eq!(semantic_kind(0, b"Sv_DepthLessEqual"), 18);
    assert_eq!(semantic_kind(0, b"SV_STENCILREF"), 20);
    assert_eq!(semantic_kind(0, b"SV_Target"), 0);
}
