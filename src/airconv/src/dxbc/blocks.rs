//! Basic blocks: a control-flow graph lowered into a function.
//!
//! [`convert`] is `convert_basicblocks` (`A/dxbc_converter_basicblock.cpp`): the blocks reachable
//! from the entry are made in the C++'s depth-first order (a stack: a conditional branch pushes
//! its true branch, then its false one, so the false one is made first; a switch its default,
//! then its cases in value order), then each gets its instructions and its terminator in that
//! order. The prologue and epilogue helpers of that file are the stages'.

use airconv_ir::ir::{BasicBlock, SrcOperand, Target};

use super::converter::{Converter, MASK_X};
use crate::{
    air::builder::MemFlags,
    api::Error,
    llvm::{Block, name, sys},
};

/// Lowers the blocks reachable from `blocks[0]` into the converter's function.
///
/// A `ret` branches to `return_bb`. Answers the entry's block; the builder is put back where it
/// was.
///
/// # Errors
///
/// An undefined block, a call left in the graph (the C++'s `UnsupportedFeature` texts), and
/// what an instruction's lowering refuses.
pub fn convert<'cx>(
    conv: &Converter<'_, 'cx>,
    blocks: &[BasicBlock],
    return_bb: Block<'cx>,
) -> Result<Block<'cx>, Error> {
    let ir = conv.ir;
    let cx = ir.cx();
    let mut visited: Vec<Option<Block<'cx>>> = vec![None; blocks.len()];
    let mut visit_order: Vec<usize> = Vec::new();
    let mut to_visit = vec![0_usize];
    while let Some(current) = to_visit.pop() {
        let Some(slot) = visited.get_mut(current) else {
            return Err(Error::new(b"invalid operation: a branch to no block."));
        };
        if slot.is_some() {
            continue;
        }
        let Some(block) = blocks.get(current) else {
            return Err(Error::new(b"invalid operation: a branch to no block."));
        };
        let block_name = name(block.debug_name.as_bytes())?;
        *slot = Some(
            conv.function
                .append_block(cx, &block_name)
                .ok_or_else(|| Error::new(b"invalid operation: a block of no function."))?,
        );
        visit_order.push(current);
        match &block.target {
            Target::Undefined | Target::Return => {}
            Target::UnconditionalBranch(target) => to_visit.push(target.index()),
            Target::ConditionalBranch {
                true_branch,
                false_branch,
                ..
            } => {
                to_visit.push(true_branch.index());
                to_visit.push(false_branch.index());
            }
            Target::Switch {
                cases,
                case_default,
                ..
            } => {
                to_visit.push(case_default.index());
                to_visit.extend(cases.values().map(|b| b.index()));
            }
            Target::InstanceBarrier { active, sync, .. } => {
                to_visit.push(active.index());
                to_visit.push(sync.index());
            }
            Target::HullShaderWriteOutput { epilogue, .. } => to_visit.push(epilogue.index()),
            Target::Call { return_point, .. } => to_visit.push(return_point.index()),
        }
    }
    let target = |id: usize| -> Result<Block<'cx>, Error> {
        visited
            .get(id)
            .copied()
            .flatten()
            .ok_or_else(|| Error::new(b"invalid operation: a branch to no block."))
    };

    let saved = ir.insert_block();
    for &current in &visit_order {
        let (Some(block), Some(bb)) = (blocks.get(current), target(current).ok()) else {
            continue;
        };
        ir.position_at_end(bb);
        for inst in &block.instructions {
            conv.convert(inst)?;
        }
        match &block.target {
            Target::Undefined => {
                return Err(Error::new(b"unexpected undefined basicblock"));
            }
            Target::UnconditionalBranch(t) => {
                ir.br(target(t.index())?);
            }
            Target::ConditionalBranch {
                cond,
                true_branch,
                false_branch,
            } => {
                let then = target(true_branch.index())?;
                let otherwise = target(false_branch.index())?;
                let test = load_condition(conv, &cond.operand, cond.test_nonzero)?;
                ir.cond_br(test, then, otherwise);
            }
            Target::Switch {
                value,
                cases,
                case_default,
            } => {
                let value = conv.load_operand(value, MASK_X)?;
                let count = airconv_ir::binding::count_u32(cases.len());
                let switch = ir.switch(value, target(case_default.index())?, count);
                for (&on, case) in cases {
                    ir.add_case(switch, conv.air.get_int(on), target(case.index())?);
                }
            }
            Target::Return => {
                ir.br(return_bb);
            }
            Target::InstanceBarrier {
                instance_count,
                active,
                sync,
            } => {
                let then = target(active.index())?;
                let otherwise = target(sync.index())?;
                let in_patch = conv
                    .res
                    .thread_id_in_patch
                    .ok_or_else(|| Error::new(b"invalid operation: no thread id in the patch."))?;
                let test = ir.icmp(sys::INT_ULT, in_patch, conv.air.get_int(*instance_count));
                ir.cond_br(test, then, otherwise);
            }
            Target::HullShaderWriteOutput {
                instance_count,
                epilogue,
            } => write_control_point(conv, *instance_count, target(epilogue.index())?)?,
            Target::Call { .. } => {
                return Err(Error::new(b"call terminator must be lowered"));
            }
        }
    }
    if let Some(saved) = saved {
        ir.position_at_end(saved);
    }
    target(0)
}

/// A branch condition: the operand's first component tested against zero.
fn load_condition<'cx>(
    conv: &Converter<'_, 'cx>,
    src: &SrcOperand,
    non_zero_test: bool,
) -> Result<crate::llvm::Val<'cx>, Error> {
    let element = conv.load_operand(src, MASK_X)?;
    let pred = if non_zero_test {
        sys::INT_NE
    } else {
        sys::INT_EQ
    };
    Ok(conv.ir.icmp(pred, element, conv.air.get_int(0)))
}

/// A hull shader's control points copied out by the first `instance_count` threads.
///
/// Then a threadgroup barrier and the branch to `epilogue`.
fn write_control_point<'cx>(
    conv: &Converter<'_, 'cx>,
    instance_count: u32,
    epilogue: Block<'cx>,
) -> Result<(), Error> {
    let ir = conv.ir;
    let cx = ir.cx();
    let tcx = cx.tcx();
    let missing = || Error::new(b"invalid operation: no control point pass-through.");
    let no_block = || Error::new(b"invalid operation: a block of no function.");
    let active = conv
        .function
        .append_block(cx, c"write_control_point")
        .ok_or_else(no_block)?;
    let sync = conv
        .function
        .append_block(cx, c"write_control_point_end")
        .ok_or_else(no_block)?;
    let in_patch = conv.res.thread_id_in_patch.ok_or_else(missing)?;
    let test = ir.icmp(sys::INT_ULT, in_patch, conv.air.get_int(instance_count));
    ir.cond_br(test, active, sync);
    ir.position_at_end(active);

    let ty = conv.res.hull_cp_passthrough_type.ok_or_else(missing)?;
    let mut src = conv.res.hull_cp_passthrough_src.ok_or_else(missing)?;
    let dst = conv.res.hull_cp_passthrough_dst.ok_or_else(missing)?;
    if tcx.pointee(src.ty()) != Some(ty) {
        let addrspace = tcx.addrspace(src.ty()).unwrap_or(0);
        src = ir.bitcast(src, tcx.ptr(ty, addrspace));
    }
    ir.store(ir.load(ty, src), dst);

    ir.br(sync);
    ir.position_at_end(sync);
    conv.air.create_barrier(MemFlags::THREADGROUP, false)?;
    ir.br(epilogue);
    Ok(())
}
