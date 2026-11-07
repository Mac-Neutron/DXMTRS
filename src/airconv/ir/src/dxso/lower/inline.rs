//! Structured flow control and the call inliner: each brace `emit.rs` opens is a block boundary.
//!
//! `if`/`ifc`/`else`/`endif`, `loop aL, iN`/`rep iN`/`endloop`/`endrep` and the three `break`s
//! keep one stack of open constructs, as the MSL's braces nest. `call l#` lowers the label's
//! body in place, and `callnz l#, b#|p0` does so inside an `if` on the condition's lane x
//! (`expand_subroutine`): the body runs with the caller's open loops, so an `aL` read there is
//! the calling loop's. A call to an undefined label, or to a label already being expanded, is
//! skipped as the MSL skips it; nesting past [`MAX_CALL_DEPTH`] is refused.

use super::{
    Counter, Frame, Lower, LowerError, MAX_CALL_DEPTH,
    ops::{compare, to_int},
    regs::{MASK_X, fdst, fsplat, ftemp, ibin, icmp, idst, isplat, itemp, lane, mov},
};
use crate::{
    dxso::{ir::Instruction, opcode::Opcode},
    ir::{
        ConversionOp, FloatComparison, InstConvert, Instruction as Ir, IntegerBinaryOp,
        IntegerComparison, SrcOperand as Src, Target,
    },
};

impl Lower<'_> {
    /// A branch condition: `value.x != 0.0`.
    fn nonzero(&mut self, value: Src) -> Result<crate::ir::Condition, LowerError> {
        let c = self.mask(FloatComparison::NotEqual, lane(value, 0), fsplat(0.0))?;
        Ok(crate::ir::Condition {
            operand: lane(c, 0),
            test_nonzero: true,
        })
    }

    /// The condition of `ifc`/`breakc`: the comparison of the two sources' lane x.
    fn compared(&mut self, inst: &Instruction) -> Result<crate::ir::Condition, LowerError> {
        let srcs = self.sources(inst)?;
        let (a, b) = (lane(self.arg(&srcs, 0)?, 0), lane(self.arg(&srcs, 1)?, 0));
        let (cmp, a, b) = compare(inst.cmp_func, a, b);
        let c = self.mask(cmp, a, b)?;
        Ok(crate::ir::Condition {
            operand: lane(c, 0),
            test_nonzero: true,
        })
    }

    /// Opens an `if` on `cond`.
    fn branch_if(&mut self, cond: crate::ir::Condition) -> Result<(), LowerError> {
        let then = self.block("if_true")?;
        let alternative = self.block("end_if")?;
        self.end_block(Target::ConditionalBranch {
            cond,
            true_branch: then,
            false_branch: alternative,
        })?;
        self.current = then;
        self.frames.push(Frame::If {
            alternative,
            end: None,
        });
        Ok(())
    }

    /// `if s` (`s.x != 0.0`) and `ifc_<cmp> s0, s1`.
    pub fn open_if(&mut self, inst: &Instruction) -> Result<(), LowerError> {
        let cond = if inst.opcode == Opcode::Ifc {
            self.compared(inst)?
        } else {
            let srcs = self.sources(inst)?;
            let value = self.arg(&srcs, 0)?;
            self.nonzero(value)?
        };
        self.branch_if(cond)
    }

    /// `else`.
    pub fn open_else(&mut self) -> Result<(), LowerError> {
        let Some(Frame::If {
            alternative,
            end: None,
        }) = self.frames.last()
        else {
            return Err(self.refuse("an else outside an if"));
        };
        let alternative = *alternative;
        let end = self.block("end_if")?;
        self.end_block(Target::UnconditionalBranch(end))?;
        self.current = alternative;
        if let Some(Frame::If { end: slot, .. }) = self.frames.last_mut() {
            *slot = Some(end);
        }
        Ok(())
    }

    /// `endif`.
    pub fn end_if(&mut self) -> Result<(), LowerError> {
        let Some(Frame::If { alternative, end }) = self.frames.pop() else {
            return Err(self.refuse("an endif that closes no if"));
        };
        let end = end.unwrap_or(alternative);
        self.end_block(Target::UnconditionalBranch(end))?;
        self.current = end;
        Ok(())
    }

    /// The loop's header: `header` tests `test` (an integer mask, lane x) into body or exit.
    fn open_body(
        &mut self,
        header: crate::ir::BlockId,
        test: Src,
        counter: Counter,
    ) -> Result<(), LowerError> {
        let body = self.block("loop_body")?;
        let exit = self.block("end_loop")?;
        self.end_block(Target::ConditionalBranch {
            cond: crate::ir::Condition {
                operand: test,
                test_nonzero: true,
            },
            true_branch: body,
            false_branch: exit,
        })?;
        self.current = body;
        self.frames.push(Frame::Loop {
            header,
            exit,
            counter,
        });
        Ok(())
    }

    /// Ends the current block with a jump to a new header block and moves there.
    fn header(&mut self) -> Result<crate::ir::BlockId, LowerError> {
        let header = self.block("loop_header")?;
        self.end_block(Target::UnconditionalBranch(header))?;
        self.current = header;
        Ok(header)
    }

    /// `loop aL, iN`: `for (aL = i.y, step = i.z, end = i.x; end-- > 0; aL += step)`.
    pub fn open_loop(&mut self, inst: &Instruction) -> Result<(), LowerError> {
        let srcs = self.sources(inst)?;
        let i = self.arg(&srcs, 1)?;
        let (al, step, count) = (self.perm(), self.perm(), self.perm());
        self.emit(to_int(al, i.clone(), 1))?;
        self.emit(to_int(step, i.clone(), 2))?;
        self.emit(to_int(count, i, 0))?;
        let header = self.header()?;
        let t = self.scratch();
        self.emit(icmp(
            IntegerComparison::SignedLessThan,
            idst(t, MASK_X),
            isplat(0),
            lane(itemp(count), 0),
        ))?;
        self.emit(ibin(
            IntegerBinaryOp::Add,
            idst(count, MASK_X),
            lane(itemp(count), 0),
            isplat(u32::MAX),
        ))?;
        self.open_body(header, lane(itemp(t), 0), Counter::Al { al, step })
    }

    /// `rep iN`: `for (int r = 0; r < i.x; ++r)`, the count compared as a float.
    pub fn open_rep(&mut self, inst: &Instruction) -> Result<(), LowerError> {
        let srcs = self.sources(inst)?;
        let i = self.arg(&srcs, 0)?;
        let (limit, count) = (self.perm(), self.perm());
        self.emit(mov(fdst(limit, MASK_X), lane(i, 0)))?;
        self.emit(mov(idst(count, MASK_X), isplat(0)))?;
        let header = self.header()?;
        let f = self.scratch();
        self.emit(Ir::Convert(InstConvert {
            op: ConversionOp::SignedToFloat,
            dst: fdst(f, MASK_X),
            src: lane(itemp(count), 0),
        }))?;
        let t = self.mask(
            FloatComparison::LessThan,
            lane(ftemp(f), 0),
            lane(ftemp(limit), 0),
        )?;
        self.open_body(header, lane(t, 0), Counter::Rep { count })
    }

    /// `endloop`/`endrep`: the counters step and the loop goes back to its test.
    pub fn end_loop(&mut self) -> Result<(), LowerError> {
        let Some(Frame::Loop {
            header,
            exit,
            counter,
        }) = self.frames.pop()
        else {
            return Err(self.refuse("an endloop that closes no loop"));
        };
        let step = match counter {
            Counter::Al { al, step } => ibin(
                IntegerBinaryOp::Add,
                idst(al, MASK_X),
                lane(itemp(al), 0),
                lane(itemp(step), 0),
            ),
            Counter::Rep { count } => ibin(
                IntegerBinaryOp::Add,
                idst(count, MASK_X),
                lane(itemp(count), 0),
                isplat(1),
            ),
        };
        self.emit(step)?;
        self.end_block(Target::UnconditionalBranch(header))?;
        self.current = exit;
        Ok(())
    }

    /// `break`, `breakc_<cmp> s0, s1` and `breakp p0.<c>`: out of the innermost loop.
    pub fn break_loop(&mut self, inst: &Instruction) -> Result<(), LowerError> {
        let Some(exit) = self.frames.iter().rev().find_map(|f| match f {
            Frame::Loop { exit, .. } => Some(*exit),
            Frame::If { .. } => None,
        }) else {
            return Err(self.refuse("a break outside a loop"));
        };
        let cond = match inst.opcode {
            Opcode::BreakC => self.compared(inst)?,
            Opcode::BreakP => {
                let srcs = self.sources(inst)?;
                let value = self.arg(&srcs, 0)?;
                self.nonzero(value)?
            }
            _ => {
                self.end_block(Target::UnconditionalBranch(exit))?;
                // What follows a `break` in the same scope runs never.
                self.current = self.block("after_break")?;
                return Ok(());
            }
        };
        let rest = self.block("break_not_taken")?;
        self.end_block(Target::ConditionalBranch {
            cond,
            true_branch: exit,
            false_branch: rest,
        })?;
        self.current = rest;
        Ok(())
    }

    /// `call l#` and `callnz l#, s` (`if (s.x != 0.0) { body }`).
    pub fn call(&mut self, inst: &Instruction) -> Result<(), LowerError> {
        let srcs = self.sources(inst)?;
        let opcode = self.opcode;
        let label = inst.srcs.first().map(|s| u32::from(s.reg.index));
        let conditional = inst.opcode == Opcode::CallNz;
        if conditional {
            let value = self.arg(&srcs, 1)?;
            let cond = self.nonzero(value)?;
            self.branch_if(cond)?;
        }
        if let Some(label) = label {
            self.expand(label)?;
        }
        self.opcode = opcode;
        if conditional {
            self.end_if()?;
        }
        Ok(())
    }

    /// The body of `label` lowered here; skipped when undefined or already being expanded.
    fn expand(&mut self, label: u32) -> Result<(), LowerError> {
        let program = self.program;
        let Some(body) = program.subroutines.get(&label) else {
            return Ok(());
        };
        if self.calls.contains(&label) {
            return Ok(());
        }
        if self.calls.len() >= MAX_CALL_DEPTH {
            return Err(self.refuse("calls nested deeper than 32"));
        }
        self.calls.push(label);
        self.body(body)?;
        self.calls.pop();
        Ok(())
    }
}
