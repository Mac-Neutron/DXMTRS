//! DXIL's operations lowered to AIR.
//!
//! Ported from `A/dxil/dxil_lower.cpp` (`Lowering`): each `dx.op` call of the moved DXIL body is
//! replaced, one at a time, with AIR made at its place in the function being built, through the
//! DXBC path's [`Converter`] (its builders, bindings, IO map and helpers). The math is
//! `lower_math.rs`, the resources `lower_resources.rs`; here are the dispatch, the IO, the IDs,
//! control, the wave and quad operations, the 64-bit lock path and the groupshared 64-bit
//! atomics. The parsed module's values are lifted to the context's [`Ty`]s as they are read.
//! Values are made in the C++'s order (clang evaluates a call's arguments and a braced list left
//! to right). Where the C++ dereferenced a null (an IO map value the stage lacks, an empty
//! emit), the lowering answers an `invalid operation` error instead (P15).

use super::{
    initialize::DxilShader,
    metadata::{self, HandleInfo, const_u32, op_code},
    types::{EntryInfo, SignatureElement, UNPACKED_INSTANCE_ID, UNPACKED_VERTEX_ID, op},
};
use crate::{
    abi::SM50_SHADER_METAL_320,
    air::builder::{AirBuilder, IntUnOp, MemFlags, ThreadScope},
    api::Error,
    dxbc::converter::{Converter, invalid, req},
    dxil::types::{component_type, semantic_kind, shader_kind},
    llvm::{Block, Builder, Context, Ty, TyCx, TyKind, Val, sys},
};

/// The function's attribute index (`~0U`).
const FN: sys::AttributeIndex = sys::ATTRIBUTE_FUNCTION_INDEX;

/// The lowering of one DXIL body's calls (`Lowering`).
///
/// `vertex_outputs`: for a geometry shader, the vertex shader whose outputs its inputs read
/// (from the payload).
pub struct Lowering<'a, 'cx> {
    pub conv: &'a Converter<'a, 'cx>,
    pub dxil: &'a DxilShader,
    pub entry: &'a EntryInfo,
    pub vertex_outputs: Option<&'a EntryInfo>,
}

impl<'a, 'cx> Lowering<'a, 'cx> {
    /// Lowers and erases `call` (`Lower`); handle-producing calls are kept for [`Self::cleanup`].
    ///
    /// # Errors
    ///
    /// The op's refusal (`DXIL: <callee> (<opcode>) not supported`), the C++'s other `DXIL:`
    /// texts, and P15's for what the C++ dereferenced as null.
    ///
    /// # Safety
    ///
    /// `call` is erased unless it makes a handle or the lowering fails: no copy of it is used
    /// after a call that succeeds.
    pub unsafe fn lower(&self, call: Val<'cx>) -> Result<(), Error> {
        let opcode = op_code(call.raw());
        if matches!(
            opcode,
            op::CREATE_HANDLE
                | op::CREATE_HANDLE_FROM_BINDING
                | op::CREATE_HANDLE_FROM_HEAP
                | op::ANNOTATE_HANDLE
        ) {
            // Resolved at each use, erased by `cleanup`.
            return Ok(());
        }
        let value = self.lowered(opcode, call)?;
        // SAFETY: the caller's contract: nothing uses `call` after this.
        unsafe { replace(call, value) }
    }

    /// `call`'s replacement, made before it: `None` for a call that answers no value.
    fn lowered(&self, opcode: u32, call: Val<'cx>) -> Result<Option<Val<'cx>>, Error> {
        self.b().position_before(call);
        if (op::F_ABS..=op::DOT4).contains(&opcode)
            || opcode == op::MAKE_DOUBLE
            || opcode == op::SPLIT_DOUBLE
            || (op::BITCAST_I16_TO_F16..=op::LEGACY_F16_TO_F32).contains(&opcode)
            || (op::DOT2_ADD_HALF..=op::DOT4_ADD_U8_PACKED).contains(&opcode)
            || opcode == op::UNPACK4X8
            || opcode == op::PACK4X8
        {
            return self.lower_math(opcode, call);
        }
        if opcode == op::RENDER_TARGET_GET_SAMPLE_POSITION
            || opcode == op::RENDER_TARGET_GET_SAMPLE_COUNT
        {
            // Inside the resource range, but they take no handle.
            return self.lower_other(opcode, call);
        }
        if (op::C_BUFFER_LOAD..=op::ATOMIC_COMPARE_EXCHANGE).contains(&opcode)
            || opcode == op::CALCULATE_LOD
            || opcode == op::RAW_BUFFER_LOAD
            || opcode == op::RAW_BUFFER_STORE
            || opcode == op::SAMPLE_CMP_GRAD
            || opcode == op::SAMPLE_CMP_BIAS
        {
            return self.lower_resource(opcode, call);
        }
        self.lower_other(opcode, call)
    }

    /// Erases the handle-producing calls once nothing uses them (`Cleanup`).
    ///
    /// An `annotateHandle` uses a `createHandle*`: the walk repeats until nothing changes.
    ///
    /// # Safety
    ///
    /// `function` is a live function, and no `Val` of a handle-producing call of it is used after
    /// the call.
    pub unsafe fn cleanup(function: sys::ValueRef) {
        let mut erased = true;
        while erased {
            erased = false;
            // SAFETY: the function being built.
            let first = unsafe { sys::get_first_basic_block(function) };
            for bb in walk_blocks(first) {
                // SAFETY: a block of the function.
                let mut inst = unsafe { sys::get_first_instruction(bb) };
                while !inst.is_null() {
                    let this = inst;
                    // SAFETY: an instruction of the block.
                    inst = unsafe { sys::get_next_instruction(this) };
                    if !metadata::is_call(this) || has_uses(this) {
                        continue;
                    }
                    if matches!(
                        op_code(this),
                        op::CREATE_HANDLE
                            | op::CREATE_HANDLE_FROM_BINDING
                            | op::CREATE_HANDLE_FROM_HEAP
                            | op::ANNOTATE_HANDLE
                    ) {
                        // SAFETY: an unused instruction of the function.
                        unsafe { sys::instruction_erase_from_parent(this) };
                        erased = true;
                    }
                }
            }
        }
    }

    /// A 64-bit groupshared `atomicrmw` or `cmpxchg` under the threadgroup lock array `locks`.
    ///
    /// `GroupShared64`: lock `(byte offset >> 3) & (N - 1)` of the array, so a qword always takes
    /// one lock (an addrspace(3) `ptrtoint` is 0x10000 + the byte offset, which keeps that). The
    /// old value is a plain threadgroup load under the lock; a `cmpxchg` stores only on a match,
    /// and its `{old, success}` is rebuilt for its `extractvalue` users. No i64 atomic reaches
    /// Metal. Replaces and erases `inst`.
    ///
    /// # Errors
    ///
    /// What the lock path refuses (none in practice).
    ///
    /// # Safety
    ///
    /// No copy of `inst` is used after a call that succeeds.
    pub unsafe fn group_shared64(&self, inst: Val<'cx>, locks: Val<'cx>) -> Result<(), Error> {
        let b = self.b();
        let tcx = self.tcx();
        b.position_before(inst);
        // SAFETY: an instruction of the module.
        let rmw = !unsafe { sys::is_a_atomic_rmw_inst(inst.raw()) }.is_null();
        let ptr = self.operand(inst, 0)?;
        // SAFETY: an `atomicrmw` or a `cmpxchg` (Initialize's scan found it).
        let align = unsafe { sys::get_alignment(inst.raw()) };
        let i64_ty = tcx.int(64);
        let locks_ty = tcx
            .pointee(locks.ty())
            .ok_or_else(|| invalid("a lock array is not a pointer"))?;
        let n = tcx.count(locks_ty).unwrap_or(0);
        let key = b.and(
            b.lshr(b.trunc(b.ptrtoint(ptr, i64_ty), tcx.int(32)), self.int(3)),
            self.int(n.wrapping_sub(1)),
        );
        let lock = b.inbounds_gep(locks_ty, locks, &[self.int(0), key]);
        let compare = if rmw {
            None
        } else {
            Some(self.operand(inst, 1)?)
        };
        let val_ty = compare.map_or_else(|| inst.ty(), Val::ty);
        let old = self.under_lock(
            inst,
            lock,
            MemFlags::THREADGROUP,
            ThreadScope::Threadgroup,
            |at| {
                let value = b.aligned_load(val_ty, ptr, align);
                if rmw {
                    let v = self.operand(inst, 1)?;
                    let pick = |pred| b.select(b.icmp(pred, value, v), value, v);
                    // SAFETY: an `atomicrmw`.
                    let next = match unsafe { sys::get_atomic_rmw_bin_op(inst.raw()) } {
                        sys::ATOMIC_RMW_BIN_OP_ADD => b.add(value, v),
                        sys::ATOMIC_RMW_BIN_OP_AND => b.and(value, v),
                        sys::ATOMIC_RMW_BIN_OP_OR => b.or(value, v),
                        sys::ATOMIC_RMW_BIN_OP_XOR => b.xor(value, v),
                        sys::ATOMIC_RMW_BIN_OP_MAX => pick(sys::INT_SGT),
                        sys::ATOMIC_RMW_BIN_OP_MIN => pick(sys::INT_SLT),
                        sys::ATOMIC_RMW_BIN_OP_U_MAX => pick(sys::INT_UGT),
                        sys::ATOMIC_RMW_BIN_OP_U_MIN => pick(sys::INT_ULT),
                        // Xchg; Initialize refused the rest.
                        _ => v,
                    };
                    b.aligned_store(next, ptr, align);
                    return Ok(value);
                }
                // A cmpxchg stores only on a match: the section's block splits around the store.
                let compare = req(compare, "a cmpxchg of no compare value")?;
                let matched = b.icmp(sys::INT_EQ, value, compare);
                let cur = self.insert_block()?;
                let tail = split(cur, at);
                let store = tail.insert_before(self.cx(), c"");
                // SAFETY: `cur`'s terminator is the branch the split just made, held by nothing.
                unsafe { erase_terminator(cur) };
                b.position_at_end(cur);
                b.cond_br(matched, store, tail);
                b.position_at_end(store);
                b.aligned_store(self.operand(inst, 2)?, ptr, align);
                b.br(tail);
                self.position_at_start(tail)?;
                Ok(value)
            },
        )?;
        let result = compare.map_or(old, |compare| {
            let pair = b.insert_value(self.cx().undef(inst.ty()), old, 0);
            b.insert_value(pair, b.icmp(sys::INT_EQ, old, compare), 1)
        });
        // SAFETY: the instruction and its replacement, of one type.
        unsafe { sys::replace_all_uses_with(inst.raw(), result.raw()) };
        // SAFETY: an instruction nothing uses now.
        unsafe { sys::instruction_erase_from_parent(inst.raw()) };
        Ok(())
    }

    // The lowering's tools (`Replace`, `Aggregate`, `Cast`, …).

    /// The instruction builder.
    pub const fn b(&self) -> &'a Builder<'cx> {
        self.conv.ir
    }

    /// The AIR builder.
    pub const fn air(&self) -> &'a AirBuilder<'a, 'cx> {
        self.conv.air
    }

    /// The context.
    pub const fn cx(&self) -> &'cx Context {
        self.conv.ir.cx()
    }

    /// The context's types.
    pub const fn tcx(&self) -> &'cx TyCx {
        self.cx().tcx()
    }

    /// The `i32` constant `v`.
    pub fn int(&self, v: u32) -> Val<'cx> {
        self.air().get_int(v)
    }

    /// The parsed module's value `raw` with its type.
    ///
    /// A pointer lifts with [`TyKind::Unknown`](crate::llvm::TyKind::Unknown) as its pointee.
    ///
    /// # Errors
    ///
    /// A value of a type the translator never makes (a token, bfloat, a scalable vector).
    pub fn val(&self, raw: sys::ValueRef) -> Result<Val<'cx>, Error> {
        lift(self.cx(), raw)
    }

    /// Argument `i` of the `dx.op` call `call`.
    ///
    /// # Errors
    ///
    /// The call's refusal when it has no such argument (the C++ read past its operands).
    pub fn arg(&self, call: Val<'cx>, i: u32) -> Result<Val<'cx>, Error> {
        let raw = metadata::arg(call.raw(), i);
        if raw.is_null() {
            return Err(unsupported(call));
        }
        self.val(raw)
    }

    /// Operand `i` of the instruction `inst`.
    ///
    /// # Errors
    ///
    /// An instruction with no such operand.
    pub fn operand(&self, inst: Val<'cx>, i: u32) -> Result<Val<'cx>, Error> {
        // SAFETY: an instruction of the module.
        if i >= u32::try_from(unsafe { sys::get_num_operands(inst.raw()) }).unwrap_or(0) {
            return Err(invalid("an operand past the instruction's"));
        }
        // SAFETY: as above, with more than `i` operands.
        self.val(unsafe { sys::get_operand(inst.raw(), i) })
    }

    /// `Aggregate`: an aggregate of `ty` from `fields` with `insertvalue`; `None` stays undef.
    pub fn aggregate(&self, ty: Ty, fields: &[Option<Val<'cx>>]) -> Val<'cx> {
        let b = self.b();
        let mut agg = self.cx().undef(ty);
        for (i, field) in (0_u32..).zip(fields) {
            if let Some(field) = *field {
                agg = b.insert_value(agg, field, i);
            }
        }
        agg
    }

    /// `Cast`: a scalar reinterpreted or converted to `ty`.
    ///
    /// The same width: bitcast; float to float: fp cast; ints: trunc or zext; across kinds,
    /// through the source's (from a float) or the destination's (to a float) own-width int.
    pub fn cast(&self, value: Val<'cx>, ty: Ty) -> Val<'cx> {
        let b = self.b();
        let tcx = self.tcx();
        let from = value.ty();
        if from == ty {
            return value;
        }
        let (from_bits, to_bits) = (primitive_bits(tcx, from), primitive_bits(tcx, ty));
        if from_bits == to_bits {
            return b.bitcast(value, ty);
        }
        let (from_fp, to_fp) = (is_fp(tcx, from), is_fp(tcx, ty));
        if from_fp && to_fp {
            return b.fp_cast(value, ty);
        }
        if tcx.int_bits(from).is_some() && tcx.int_bits(ty).is_some() {
            return b.zext_or_trunc(value, ty);
        }
        if from_fp {
            return b.zext_or_trunc(b.bitcast(value, tcx.int(from_bits)), ty);
        }
        b.bitcast(b.zext_or_trunc(value, tcx.int(to_bits)), ty)
    }

    /// A Metal simdgroup function call, marked convergent like Metal's (`Simd`).
    ///
    /// Never `readnone` or speculatable: the 64-bit lock path's election (`air.simd_is_first`
    /// behind a branch) and its fence ordering rely on that.
    ///
    /// # Errors
    ///
    /// P15's error for a name holding a NUL (none here).
    pub fn simd(&self, name: &[u8], ret: Ty, args: &[Val<'cx>]) -> Result<Val<'cx>, Error> {
        let cx = self.cx();
        let attrs: &[(sys::AttributeIndex, &[u8])] = &[
            (FN, b"convergent"),
            (FN, b"mustprogress"),
            (FN, b"nounwind"),
            (FN, b"willreturn"),
        ];
        let params: Vec<Ty> = args.iter().map(|a| a.ty()).collect();
        let fn_ty = self.tcx().function(ret, &params, false);
        let callee = self
            .air()
            .module()
            .get_or_insert_function(name, fn_ty, attrs)?;
        let call = self.b().call(callee, args);
        let convergent = cx.enum_attribute(b"convergent", 0)?;
        // SAFETY: a call just made; an attribute of its context.
        unsafe { sys::add_call_site_attribute(call.raw(), FN, convergent) };
        Ok(call)
    }

    /// The block the builder inserts into.
    ///
    /// # Errors
    ///
    /// An unpositioned builder (an internal inconsistency).
    pub fn insert_block(&self) -> Result<Block<'cx>, Error> {
        self.b()
            .insert_block()
            .ok_or_else(|| invalid("an unpositioned builder"))
    }

    /// Positions the builder before the first instruction of `block` (`&*block->begin()`).
    ///
    /// # Errors
    ///
    /// An empty block (an internal inconsistency).
    pub fn position_at_start(&self, block: Block<'cx>) -> Result<(), Error> {
        // SAFETY: a block of this context.
        let first = unsafe { sys::get_first_instruction(block.raw()) };
        if first.is_null() {
            return Err(invalid("an empty block"));
        }
        self.b().position_before(self.val(first)?);
        Ok(())
    }

    /// Function argument `index` of the function being built.
    ///
    /// # Errors
    ///
    /// No such argument (the C++'s `getArg` past them, or of `~0u`).
    pub fn param(&self, index: u32) -> Result<Val<'cx>, Error> {
        self.conv
            .function
            .param(self.tcx(), index)
            .ok_or_else(|| invalid("a function argument the stage did not define"))
    }

    /// The handle's resource and its register, as the binding maps take them.
    ///
    /// # Errors
    ///
    /// `ResolveHandle`'s refusals; a handle call without its register argument.
    pub fn handle(&self, call: Val<'cx>, i: u32) -> Result<Handle<'a, 'cx>, Error> {
        let HandleInfo {
            cls,
            range,
            index,
            resource,
        } = metadata::resolve_handle(self.entry, metadata::arg(call.raw(), i))?;
        let index = if index.is_null() {
            return Err(unsupported(call));
        } else {
            self.val(index)?
        };
        let resource = self
            .entry
            .resources
            .get(resource)
            .ok_or_else(|| invalid("a handle of no resource"))?;
        Ok(Handle {
            cls,
            range,
            index,
            resource,
        })
    }

    /// The 64-bit lock path (spec 3.8), from 32-bit atomics, fences and simdgroup votes.
    ///
    /// `UnderLock`: never Metal's 64-bit atomics, which fail or crash its compiler service.
    /// Phase 1: every lane tries the lock once, and the winners run the critical section and
    /// release inside that branch, with no loop. Phase 2: while any lane of the simdgroup is
    /// pending, the first pending lane spins until it holds the lock, runs the section and
    /// releases it. `air.simd_is_first` is reached by pending lanes only (a branch, not an
    /// `and`: it names the first active lane, so a finished lane could be elected and let none
    /// in). No spin cap: a cap would drop operations in a legitimate storm. Acquire: a relaxed
    /// cmpxchg, then a `seq_cst` fence; release: a `seq_cst` fence, then a relaxed store of 0.
    ///
    /// The block is split before `at` (the builder's insertion point); `critical` reads the
    /// location, computes, writes and answers the old value, given the section's branch, which
    /// it leaves the builder before. Answers the old value; the builder ends before `at`.
    ///
    /// # Errors
    ///
    /// What `critical` answers.
    pub fn under_lock(
        &self,
        at: Val<'cx>,
        lock: Val<'cx>,
        flags: MemFlags,
        scope: ThreadScope,
        mut critical: impl FnMut(Val<'cx>) -> Result<Val<'cx>, Error>,
    ) -> Result<Val<'cx>, Error> {
        let b = self.b();
        let cx = self.cx();
        let tcx = self.tcx();
        let relaxed = sys::ATOMIC_ORDERING_MONOTONIC;
        let i1 = tcx.int(1);
        let scope = scope as u32;
        let acquire = || {
            let pair = b.atomic_cmpxchg(lock, self.int(0), self.int(1), relaxed, relaxed, false);
            b.extract_value(pair, 1)
        };
        // The section in `bb`, which then branches to `next` (built first: `critical` may split
        // blocks before it). Answers the old value and the block that branches to `next`.
        let mut section = |bb: Block<'cx>, next: Block<'cx>| -> Result<_, Error> {
            let br = branch_at_end(cx, bb, next);
            b.position_before(br);
            self.fence(flags, scope)?;
            let old = critical(br)?;
            self.fence(flags, scope)?;
            let store = b.aligned_store(self.int(0), lock, 4);
            // SAFETY: a store just made.
            unsafe { sys::set_ordering(store.raw(), relaxed) };
            Ok((old, self.insert_block()?))
        };
        let head = self.insert_block()?;
        let done = split(head, at);
        // SAFETY: `head`'s terminator is the branch the split just made, held by nothing.
        unsafe { erase_terminator(head) };
        let block = || done.insert_before(cx, c"");
        let (first, join, looped, pending_bb, elect, spin, locked, latch) = (
            block(),
            block(),
            block(),
            block(),
            block(),
            block(),
            block(),
            block(),
        );
        b.position_at_end(head);
        let won = acquire();
        let lost = b.not(won);
        b.cond_br(won, first, join);
        let (old1, first_end) = section(first, join)?;
        let ty = old1.ty();
        b.position_at_end(join);
        let old_join = b.phi(ty);
        b.add_incoming(old_join, old1, first_end);
        b.add_incoming(old_join, cx.null(ty), head);
        b.br(looped);
        b.position_at_end(looped);
        let pending = b.phi(i1);
        let old = b.phi(ty);
        let any = self.simd(b"air.simd_any", i1, &[pending])?;
        b.cond_br(any, pending_bb, done);
        b.position_at_end(pending_bb);
        b.cond_br(pending, elect, latch);
        b.position_at_end(elect);
        let is_first = self.simd(b"air.simd_is_first", i1, &[])?;
        b.cond_br(is_first, spin, latch);
        b.position_at_end(spin);
        b.cond_br(acquire(), locked, spin);
        let (old2, locked_end) = section(locked, latch)?;
        b.position_at_end(latch);
        let pending_next = b.phi(i1);
        let old_next = b.phi(ty);
        b.add_incoming(pending_next, self.air().get_bool(false), locked_end);
        b.add_incoming(old_next, old2, locked_end);
        for bb in [pending_bb, elect] {
            b.add_incoming(pending_next, pending, bb);
            b.add_incoming(old_next, old, bb);
        }
        b.br(looped);
        b.add_incoming(pending, lost, join);
        b.add_incoming(pending, pending_next, latch);
        b.add_incoming(old, old_join, join);
        b.add_incoming(old, old_next, latch);
        self.position_at_start(done)?;
        Ok(old)
    }

    /// `air.atomic.fence(flags, 5, scope)`, sequentially consistent, at any scope value.
    ///
    /// `CreateAtomicFence`; the barrier's `Device | Threadgroup` (3) is no single
    /// [`ThreadScope`] (lane a's request, D1), so the declaration is made here.
    fn fence(&self, flags: MemFlags, scope: u32) -> Result<(), Error> {
        let i32_ty = self.tcx().int(32);
        let attrs: &[(sys::AttributeIndex, &[u8])] = &[
            (FN, b"mustprogress"),
            (FN, b"nounwind"),
            (FN, b"willreturn"),
        ];
        let fn_ty = self
            .tcx()
            .function(self.tcx().void(), &[i32_ty, i32_ty, i32_ty], false);
        let callee =
            self.air()
                .module()
                .get_or_insert_function(b"air.atomic.fence", fn_ty, attrs)?;
        let _ = self.b().call(
            callee,
            &[self.int(flags.bits()), self.int(5), self.int(scope)],
        );
        Ok(())
    }

    // IO.

    /// The value of an input in no register (`UnpackedInput`), `None` for any other.
    fn unpacked_input(&self, element_id: u32) -> Option<Val<'cx>> {
        let res = self.conv.res;
        match *self.dxil.io.unpacked_inputs.get(&element_id)? {
            UNPACKED_VERTEX_ID => res.vertex_id,
            UNPACKED_INSTANCE_ID => res.instance_id,
            arg => self.conv.function.param(self.tcx(), arg),
        }
    }

    /// The unpacked input of semantic `kind` (`UnpackedInputOfKind`).
    fn unpacked_input_of_kind(&self, kind: u32) -> Option<Val<'cx>> {
        let e = self.entry.inputs.iter().find(|e| e.kind == kind)?;
        self.unpacked_input(e.id)
    }

    /// Element `e`'s row `row`, column `col` in a register file (`RegisterElementPointer`).
    fn register_element_pointer(
        &self,
        file: Option<Val<'cx>>,
        e: &SignatureElement,
        row: Val<'cx>,
        col: u32,
    ) -> Result<Val<'cx>, Error> {
        let b = self.b();
        let file = req(file, "no register file")?;
        let array_ty = self
            .tcx()
            .pointee(file.ty())
            .ok_or_else(|| invalid("a register file is not a pointer"))?;
        let reg = b.add(self.int(e.start_row.cast_unsigned()), row);
        let col = e.start_col.cast_unsigned().wrapping_add(col);
        Ok(b.gep(array_ty, file, &[self.int(0), reg, self.int(col)]))
    }

    /// The vertex shader output register and column holding geometry input `e`'s row and column.
    ///
    /// `VertexOutputPointer`: the vertex shader element with the same semantic (position by
    /// kind; others by name and index). Rows are consecutive semantic indices on both sides, so
    /// a dynamic row keeps its offset.
    fn vertex_output_pointer(
        &self,
        e: &SignatureElement,
        row: Val<'cx>,
        col: u32,
        vertex: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let b = self.b();
        let first = e.semantic_indices.first().copied().unwrap_or(0);
        let arbitrary = e.kind == semantic_kind::ARBITRARY;
        for f in self.vertex_outputs.map_or(&[][..], |v| &v.outputs) {
            if f.start_row < 0
                || f.kind != e.kind
                || (arbitrary && !f.name.eq_ignore_ascii_case(&e.name))
            {
                continue;
            }
            for r in 0..f.rows {
                let same = usize::try_from(r)
                    .ok()
                    .and_then(|r| f.semantic_indices.get(r))
                    .is_some_and(|&index| index == first);
                if arbitrary && !same {
                    continue;
                }
                let input = req(self.conv.res.input.ptr_int4, "no input register file")?;
                let array_ty = self
                    .tcx()
                    .pointee(input.ty())
                    .ok_or_else(|| invalid("a register file is not a pointer"))?;
                let reg = b.add(self.int(f.start_row.cast_unsigned().wrapping_add(r)), row);
                let col = f.start_col.cast_unsigned().wrapping_add(col);
                return Ok(b.gep(array_ty, input, &[self.int(0), vertex, reg, self.int(col)]));
            }
        }
        Err(Error::new(
            &[
                &b"DXIL: geometry shader input "[..],
                &e.name,
                b" isn't a vertex shader output",
            ]
            .concat(),
        ))
    }

    // Wave operations. A pixel shader whose entry lacks [WaveOpsIncludeHelperLanes] leaves its
    // helper lanes out of them (D3D's Wave_Ops_Include_Helper_Lanes.md), and Metal's simdgroup
    // functions count them.

    /// Whether helper lanes are left out of wave operations (`HelpersLeftOut`).
    pub const fn helpers_left_out(&self) -> bool {
        self.entry.kind == shader_kind::PIXEL && !self.dxil.include_helper_lanes
    }

    /// The lanes taking part in a wave operation here (`WaveLanes`).
    ///
    /// The `i64` ballot of the non-helper lanes when helpers are left out, `None` otherwise
    /// (nothing is masked). Made at each operation: divergence changes it.
    fn wave_lanes(&self) -> Result<Option<Val<'cx>>, Error> {
        if !self.helpers_left_out() {
            return Ok(None);
        }
        let participates = self.participates()?;
        Ok(Some(self.simd(
            b"air.simd_ballot.i64",
            self.tcx().int(64),
            &[participates],
        )?))
    }

    /// `i1`: this lane takes part (`Participates`).
    ///
    /// When helpers are left out: a non-helper lane, or any lane of a simdgroup with no active
    /// non-helper lane; true otherwise. A simdgroup whose active lanes are all helpers (its
    /// covered lanes have left a loop, or a discard demoted them) runs the operation over them
    /// all, as Metal does: D3D leaves a helper's wave results undefined, but a loop waiting on a
    /// wave result must still end for it. Masked to nothing, a waterfall loop's helpers never
    /// leave it (a GPU hang).
    pub fn participates(&self) -> Result<Val<'cx>, Error> {
        let b = self.b();
        let tcx = self.tcx();
        if !self.helpers_left_out() {
            return Ok(self.air().get_bool(true));
        }
        let helper = self.simd(b"air.simd_is_helper_thread", tcx.int(1), &[])?;
        let live = b.not(helper);
        let ballot = self.simd(b"air.simd_ballot.i64", tcx.int(64), &[live])?;
        let none = b.icmp(
            sys::INT_EQ,
            ballot,
            self.cx().const_int(tcx.int(64), 0, false),
        );
        Ok(b.or(live, none))
    }

    /// `v`, or `identity` for a lane left out (`masked`).
    ///
    /// The result is then as the other lanes make it.
    fn masked(&self, v: Val<'cx>, identity: Val<'cx>) -> Result<Val<'cx>, Error> {
        if !self.helpers_left_out() {
            return Ok(v);
        }
        let participates = self.participates()?;
        Ok(self.b().select(participates, v, identity))
    }

    /// The lowest lane taking part (`first_lane`); lanes are 32 wide (`d3d12_device.cpp`).
    fn first_lane(&self, lanes: Val<'cx>) -> Result<Val<'cx>, Error> {
        let low = self.b().trunc(lanes, self.tcx().int(32));
        self.air().create_count_zero(low, true)
    }

    /// `v` of the first lane taking part (`read_first`).
    fn read_first(&self, v: Val<'cx>) -> Result<Val<'cx>, Error> {
        let suffix = simd_suffix(self.tcx(), v.ty(), false);
        if let Some(lanes) = self.wave_lanes()? {
            let first = self.first_lane(lanes)?;
            let lane = self.b().trunc(first, self.tcx().int(16));
            return self.simd(
                &[b"air.simd_shuffle", &suffix[..]].concat(),
                v.ty(),
                &[v, lane],
            );
        }
        self.simd(
            &[b"air.simd_broadcast_first", &suffix[..]].concat(),
            v.ty(),
            &[v],
        )
    }

    // Compute shaders, which have no air.dfdx (spec 3.3).

    /// The derivative of the float `v` (scalar or vector) across its quad (`QuadDerivative`).
    ///
    /// Read with `air.quad_shuffle`. Quad lane l is Metal's (simd lane & 3), never D3D's
    /// flattened ID: coarse lanes 1 - 0 (x) and 2 - 0 (y); fine (l | 1) - (l & 2) and
    /// (l | 2) - (l & 1).
    pub fn quad_derivative(&self, v: Val<'cx>, y: bool, fine: bool) -> Result<Val<'cx>, Error> {
        let b = self.b();
        let tcx = self.tcx();
        let ty = v.ty();
        if let TyKind::Vector(_, n) = tcx.kind(ty) {
            // Per component: the quad shuffles here take scalars.
            let mut d = self.cx().undef(ty);
            for i in 0..u64::from(n) {
                let component = b.extract_element_u64(v, i);
                let derivative = self.quad_derivative(component, y, fine)?;
                d = b.insert_element_u64(d, derivative, i);
            }
            return Ok(d);
        }
        let (mut from, mut to) = (self.int(0), self.int(if y { 2 } else { 1 }));
        if fine {
            // The lane index is defined for these ops in a compute shader (`add_wave_inputs`).
            let l = b.and(self.param(self.dxil.lane_index_arg)?, self.int(3));
            from = b.and(l, self.int(if y { 1 } else { 2 }));
            to = b.or(l, self.int(if y { 2 } else { 1 }));
        }
        let name = [b"air.quad_shuffle", &simd_suffix(tcx, ty, false)[..]].concat();
        let read = |lane: Val<'cx>| self.simd(&name, ty, &[v, b.trunc(lane, tcx.int(16))]);
        let a = read(to)?;
        Ok(b.fsub(a, read(from)?))
    }

    /// `(ddx, ddy)` of `coord`, coarse (`CoarseGradients`).
    ///
    /// The gradients implicit-LOD sampling in a compute shader uses.
    pub fn coarse_gradients(&self, coord: Val<'cx>) -> Result<(Val<'cx>, Val<'cx>), Error> {
        let ddx = self.quad_derivative(coord, false, false)?;
        Ok((ddx, self.quad_derivative(coord, true, false)?))
    }

    /// IO, IDs, control and the wave and quad operations (`LowerOther`).
    fn lower_other(&self, opcode: u32, call: Val<'cx>) -> Result<Option<Val<'cx>>, Error> {
        let b = self.b();
        let air = self.air();
        let cx = self.cx();
        let tcx = self.tcx();
        let res = self.conv.res;
        let i1 = tcx.int(1);
        let i16_ty = tcx.int(16);
        let i32_ty = tcx.int(32);
        let i64_ty = tcx.int(64);
        let arg = |i| self.arg(call, i);
        let component = |vec: Option<Val<'cx>>| -> Result<Option<Val<'cx>>, Error> {
            vec.map(|vec| Ok(b.extract_element(vec, arg(1)?)))
                .transpose()
        };
        let suffix = |ty, is_signed| simd_suffix(tcx, ty, is_signed);
        let named = |base: &[u8], ty| [base, &suffix(ty, false)[..]].concat();
        match opcode {
            // (93, i32 component)
            op::THREAD_ID => Ok(component(res.thread_id_arg)?),
            op::GROUP_ID => Ok(component(res.thread_group_id_arg)?),
            op::THREAD_ID_IN_GROUP => Ok(component(res.thread_id_in_group_arg)?),
            op::FLATTENED_THREAD_ID_IN_GROUP => Ok(res.thread_id_in_group_flat_arg),
            // (4, i32 inputSigId, i32 row, i8 col, i32 gsVertexAxis)
            op::LOAD_INPUT => {
                let id = const_arg(call, 1, 0);
                let e = self
                    .entry
                    .inputs
                    .iter()
                    .find(|e| e.id == id)
                    .ok_or_else(|| Error::new(b"DXIL: loadInput names an undeclared input"))?;
                if let Some(v) = self.unpacked_input(e.id) {
                    return Ok(Some(self.cast(v, call.ty())));
                }
                let ptr = if self.entry.kind == shader_kind::GEOMETRY {
                    self.vertex_output_pointer(e, arg(2)?, const_arg(call, 3, 0), arg(4)?)?
                } else {
                    self.register_element_pointer(
                        res.input.ptr_int4,
                        e,
                        arg(2)?,
                        const_arg(call, 3, 0),
                    )?
                };
                // Signature registers hold 32-bit values: a half element reads a float and
                // narrows it; i16 truncates (`cast`).
                let word = b.load(i32_ty, ptr);
                let ty = call.ty();
                let value = match tcx.kind(ty) {
                    TyKind::Int(1) => b.icmp(sys::INT_NE, word, self.int(0)),
                    TyKind::Half => b.fptrunc(b.bitcast(word, tcx.float()), ty),
                    _ => self.cast(word, ty),
                };
                Ok(Some(value))
            }
            // (5, i32 outputSigId, i32 row, i8 col, T value)
            op::STORE_OUTPUT => {
                let id = const_arg(call, 1, 0);
                let e = self
                    .entry
                    .outputs
                    .iter()
                    .find(|e| e.id == id)
                    .ok_or_else(|| Error::new(b"DXIL: storeOutput names an undeclared output"))?;
                let value = arg(4)?;
                match e.kind {
                    semantic_kind::DEPTH
                    | semantic_kind::DEPTH_LESS_EQUAL
                    | semantic_kind::DEPTH_GREATER_EQUAL => {
                        if let Some(depth) = res.depth_output_reg {
                            b.store(self.cast(value, tcx.float()), depth);
                        }
                    }
                    semantic_kind::COVERAGE => {
                        let mask = self.cast(value, i32_ty);
                        b.store(mask, req(res.coverage_mask_reg, "no coverage output")?);
                    }
                    semantic_kind::STENCIL_REF => {
                        let stencil = self.cast(value, i32_ty);
                        b.store(stencil, req(res.stencil_ref_reg, "no stencil output")?);
                    }
                    _ => {
                        // As 32-bit values (`LoadInput`): a half widened to a float, an I16
                        // element sign-extended, U16 zero-extended.
                        let ty = value.ty();
                        let word = match tcx.kind(ty) {
                            TyKind::Int(1) => b.sext(value, i32_ty),
                            TyKind::Int(16) if e.ty == component_type::I16 => b.sext(value, i32_ty),
                            TyKind::Half => b.bitcast(b.fpext(value, tcx.float()), i32_ty),
                            _ => self.cast(value, i32_ty),
                        };
                        let ptr = self.register_element_pointer(
                            res.output.ptr_int4,
                            e,
                            arg(2)?,
                            const_arg(call, 3, 0),
                        )?;
                        b.store(word, ptr);
                    }
                }
                Ok(None)
            }
            // (82, i1 condition)
            op::DISCARD => {
                let cond = arg(1)?;
                if cond.const_u64() == Some(0) {
                    return Ok(None);
                }
                // SAFETY: a condition and the call it splits before, of the function built.
                let then = unsafe {
                    sys::airconv_shim_split_block_and_insert_if_then(cond.raw(), call.raw(), 0)
                };
                b.position_before(self.val(then)?);
                air.create_discard()?;
                Ok(None)
            }
            op::DERIV_COARSE_X | op::DERIV_COARSE_Y | op::DERIV_FINE_X | op::DERIV_FINE_Y => {
                let y = opcode == op::DERIV_COARSE_Y || opcode == op::DERIV_FINE_Y;
                let value = if self.entry.kind == shader_kind::COMPUTE {
                    let fine = opcode == op::DERIV_FINE_X || opcode == op::DERIV_FINE_Y;
                    self.quad_derivative(arg(1)?, y, fine)?
                } else {
                    air.create_derivative(arg(1)?, y)?
                };
                Ok(Some(value))
            }
            // The input primitive's index in the draw (`dxbc/gs.rs`).
            op::PRIMITIVE_ID if self.entry.kind == shader_kind::GEOMETRY => Ok(res.patch_id),
            // (100)
            op::GS_INSTANCE_ID => Ok(res.gs_instance_id),
            // (97/98/99, i8 stream): stream 0 only (Initialize refuses others).
            op::EMIT_STREAM | op::CUT_STREAM | op::EMIT_THEN_CUT_STREAM => {
                let geometry = self
                    .conv
                    .geometry
                    .ok_or_else(|| invalid("emit outside a geometry shader"))?;
                if opcode != op::CUT_STREAM {
                    geometry.emit()?;
                }
                if opcode != op::EMIT_STREAM {
                    geometry.cut()?;
                }
                Ok(None)
            }
            op::SAMPLE_INDEX | op::COVERAGE | op::PRIMITIVE_ID => {
                let kind = match opcode {
                    op::SAMPLE_INDEX => semantic_kind::SAMPLE_INDEX,
                    op::COVERAGE => semantic_kind::COVERAGE,
                    _ => semantic_kind::PRIMITIVE_ID,
                };
                let v = self
                    .unpacked_input_of_kind(kind)
                    .ok_or_else(|| unsupported(call))?;
                Ok(Some(self.cast(v, call.ty())))
            }
            op::WAVE_IS_FIRST_LANE => {
                let value = match self.wave_lanes()? {
                    Some(lanes) => {
                        let lane = self.param(self.dxil.lane_index_arg)?;
                        b.icmp(sys::INT_EQ, lane, self.first_lane(lanes)?)
                    }
                    None => self.simd(b"air.simd_is_first", i1, &[])?,
                };
                Ok(Some(value))
            }
            op::WAVE_GET_LANE_INDEX | op::WAVE_GET_LANE_COUNT => {
                let index = if opcode == op::WAVE_GET_LANE_INDEX {
                    self.dxil.lane_index_arg
                } else {
                    self.dxil.lane_count_arg
                };
                Ok(Some(self.param(index)?))
            }
            op::WAVE_ANY_TRUE | op::WAVE_ALL_TRUE => {
                let any = opcode == op::WAVE_ANY_TRUE;
                let v = self.masked(arg(1)?, air.get_bool(!any))?;
                let name: &[u8] = if any {
                    b"air.simd_any"
                } else {
                    b"air.simd_all"
                };
                Ok(Some(self.simd(name, i1, &[v])?))
            }
            // (115, T)
            op::WAVE_ACTIVE_ALL_EQUAL => {
                let v = arg(1)?;
                let first = self.read_first(v)?;
                let same = if is_fp(tcx, v.ty()) {
                    b.fcmp(sys::REAL_OEQ, v, first)
                } else {
                    b.icmp(sys::INT_EQ, v, first)
                };
                let all = self.simd(
                    b"air.simd_all",
                    i1,
                    &[self.masked(same, air.get_bool(true))?],
                )?;
                Ok(Some(all))
            }
            // (116, i1) -> %dx.types.fouri32 {lanes 0-31, lanes 32-63, 0, 0}
            op::WAVE_ACTIVE_BALLOT => {
                let v = self.masked(arg(1)?, air.get_bool(false))?;
                let ballot = self.simd(b"air.simd_ballot.i64", i64_ty, &[v])?;
                let low = b.trunc(ballot, i32_ty);
                let high = b.trunc(b.lshr(ballot, cx.const_int(i64_ty, 32, false)), i32_ty);
                let value = self.aggregate(
                    call.ty(),
                    &[Some(low), Some(high), Some(self.int(0)), Some(self.int(0))],
                );
                Ok(Some(value))
            }
            // (122, T, i32 quadLane)
            op::QUAD_READ_LANE_AT => {
                let v = arg(1)?;
                let lane = b.trunc(arg(2)?, i16_ty);
                let value = self.simd(&named(b"air.quad_shuffle", v.ty()), v.ty(), &[v, lane])?;
                Ok(Some(value))
            }
            // (123, T, i8 op): read across X (0), Y (1) or the diagonal (2): the quad lane XOR 1,
            // 2 or 3.
            op::QUAD_OP => {
                let v = arg(1)?;
                let mask = const_arg(call, 2, 0).wrapping_add(1);
                let mask = cx.const_int(i16_ty, mask.into(), false);
                let name = named(b"air.quad_shuffle_xor", v.ty());
                Ok(Some(self.simd(&name, v.ty(), &[v, mask])?))
            }
            // (117, T, i32 lane)
            op::WAVE_READ_LANE_AT => {
                let v = arg(1)?;
                let lane = b.trunc(arg(2)?, i16_ty);
                let value = self.simd(&named(b"air.simd_shuffle", v.ty()), v.ty(), &[v, lane])?;
                Ok(Some(value))
            }
            // (118, T)
            op::WAVE_READ_LANE_FIRST => Ok(Some(self.read_first(arg(1)?)?)),
            // (119/121, T, i8 op: 0 sum, 1 product, 2 min, 3 max, i8 sign: 0 signed, 1 unsigned)
            op::WAVE_ACTIVE_OP | op::WAVE_PREFIX_OP => {
                let v = arg(1)?;
                let kind = const_arg(call, 2, 0);
                let is_signed = const_arg(call, 3, 0) == 0;
                let active = opcode == op::WAVE_ACTIVE_OP;
                let base: &[u8] = match (active, kind) {
                    (true, 0) => b"air.simd_sum",
                    (true, 1) => b"air.simd_product",
                    (true, 2) => b"air.simd_min",
                    (true, 3) => b"air.simd_max",
                    (false, 0) => b"air.simd_prefix_exclusive_sum",
                    (false, 1) => b"air.simd_prefix_exclusive_product",
                    _ => return Err(unsupported(call)),
                };
                let ty = v.ty();
                let name = [base, &suffix(ty, is_signed)[..]].concat();
                // The identity of each: x op identity = x (a float sum's is -0.0, for x = -0.0
                // too).
                let fp = is_fp(tcx, ty);
                let bits = tcx.scalar_bits(ty);
                let int = |value: u64| cx.const_int(ty, value, false);
                let sign_bit = 1_u64.checked_shl(bits.wrapping_sub(1)).unwrap_or(0);
                let identity = match (kind, fp) {
                    (0, true) => cx.const_real(ty, -0.0),
                    (0, false) => int(0),
                    (1, true) => cx.const_real(ty, 1.0),
                    (1, false) => int(1),
                    (2, true) => cx.const_real(ty, f64::INFINITY),
                    (2, false) if is_signed => int(sign_bit.wrapping_sub(1)),
                    (2, false) => cx.all_ones(ty),
                    (_, true) => cx.const_real(ty, f64::NEG_INFINITY),
                    (_, false) if is_signed => int(sign_bit),
                    (_, false) => int(0),
                };
                let value = self.simd(&name, ty, &[self.masked(v, identity)?])?;
                Ok(Some(value))
            }
            // (120, T, i8 op: 0 and, 1 or, 2 xor)
            op::WAVE_ACTIVE_BIT => {
                let v = arg(1)?;
                let base: &[u8] = match const_arg(call, 2, 0) {
                    0 => b"air.simd_and",
                    1 => b"air.simd_or",
                    2 => b"air.simd_xor",
                    _ => return Err(unsupported(call)),
                };
                let identity = if base == b"air.simd_and" {
                    cx.all_ones(v.ty())
                } else {
                    cx.null(v.ty())
                };
                let value =
                    self.simd(&named(base, v.ty()), v.ty(), &[self.masked(v, identity)?])?;
                Ok(Some(value))
            }
            // (135/136, i1): lanes (below this one) where the value is true
            op::WAVE_ALL_BIT_COUNT | op::WAVE_PREFIX_BIT_COUNT => {
                let v = self.masked(arg(1)?, air.get_bool(false))?;
                let mut ballot = self.simd(b"air.simd_ballot.i64", i64_ty, &[v])?;
                if opcode == op::WAVE_PREFIX_BIT_COUNT {
                    let lane = b.zext(self.param(self.dxil.lane_index_arg)?, i64_ty);
                    let one = cx.const_int(i64_ty, 1, false);
                    ballot = b.and(ballot, b.sub(b.shl(one, lane), one));
                }
                let lo = air.create_int_un_op(IntUnOp::Popcount, b.trunc(ballot, i32_ty))?;
                let high = b.trunc(b.lshr(ballot, cx.const_int(i64_ty, 32, false)), i32_ty);
                let hi = air.create_int_un_op(IntUnOp::Popcount, high)?;
                Ok(Some(b.add(lo, hi)))
            }
            // (165, T) -> %dx.types.fouri32 {lanes 0-31, 0, 0, 0}: the lanes whose value equals
            // this lane's. A loop over the distinct values: each pass reads the first remaining
            // lane's value, and the lanes holding it leave with their ballot. That first lane
            // always leaves, a NaN too: the loop ends, and D3D's own-lane bit is set.
            op::WAVE_MATCH => {
                let v = arg(1)?;
                let ty = v.ty();
                if tcx.kind(ty) == TyKind::Double {
                    // No f64 simdgroup functions in Metal (unreachable while DoubleOps is FALSE).
                    return Err(unsupported(call));
                }
                // SAFETY: a call of the function built.
                let head = unsafe { sys::get_instruction_parent(call.raw()) };
                // SAFETY: a block of this context.
                let head = unsafe { Block::new(head) };
                let done = split(head, call);
                let looped = done.insert_before(cx, c"");
                // SAFETY: the split left `head` its branch to `done`.
                let branch = unsafe { sys::get_basic_block_terminator(head.raw()) };
                // SAFETY: an unconditional branch, whose successor 0 is `done`.
                unsafe { sys::set_successor(branch, 0, looped.raw()) };
                b.position_at_end(looped);
                let first = self.simd(&named(b"air.simd_broadcast_first", ty), ty, &[v])?;
                let same = if is_fp(tcx, ty) {
                    let equal = b.fcmp(sys::REAL_OEQ, v, first);
                    b.or(equal, self.simd(b"air.simd_is_first", i1, &[])?)
                } else {
                    b.icmp(sys::INT_EQ, v, first)
                };
                let lanes = self.simd(
                    b"air.simd_ballot.i64",
                    i64_ty,
                    &[self.masked(same, air.get_bool(false))?],
                )?;
                b.cond_br(same, done, looped);
                b.position_before(call);
                let value = self.aggregate(
                    call.ty(),
                    &[
                        Some(b.trunc(lanes, i32_ty)),
                        Some(self.int(0)),
                        Some(self.int(0)),
                        Some(self.int(0)),
                    ],
                );
                Ok(Some(value))
            }
            // (166, T, i32 mask0-3, i8 op: 0 sum, 1 and, 2 or, 3 xor, 4 product, i8 sign),
            // (167, i1, i32 mask0-3)
            op::WAVE_MULTI_PREFIX_OP | op::WAVE_MULTI_PREFIX_BIT_COUNT => {
                // The lanes below this one in its partition, mask0 (lanes are 32 wide); D3D
                // ignores the inactive and helper ones.
                let lane = self.param(self.dxil.lane_index_arg)?;
                let below = b.and(arg(2)?, b.sub(b.shl(self.int(1), lane), self.int(1)));
                if opcode == op::WAVE_MULTI_PREFIX_BIT_COUNT {
                    let v = self.masked(arg(1)?, air.get_bool(false))?;
                    let ballot = self.simd(b"air.simd_ballot.i64", i64_ty, &[v])?;
                    let bits = b.and(b.trunc(ballot, i32_ty), below);
                    let count = air.create_int_un_op(IntUnOp::Popcount, bits)?;
                    return Ok(Some(count));
                }
                let v = arg(1)?;
                let ty = v.ty();
                let fp = is_fp(tcx, ty);
                let kind = const_arg(call, 6, u32::MAX);
                if kind > 4 || (fp && (1..=3).contains(&kind)) || tcx.kind(ty) == TyKind::Double {
                    return Err(unsupported(call));
                }
                // The sign operand changes nothing: a sum or a product wraps the same either way.
                let bin = match (fp, kind) {
                    (false, 0) => sys::ADD,
                    (false, 1) => sys::AND,
                    (false, 2) => sys::OR,
                    (false, 3) => sys::XOR,
                    (false, _) => sys::MUL,
                    (true, 0) => sys::F_ADD,
                    (true, _) => sys::F_MUL,
                };
                // The empty prefix: 0 (sum, or, xor; +0.0 for floats), all ones (and), 1
                // (product).
                let mut acc = match kind {
                    1 => cx.all_ones(ty),
                    4 if fp => cx.const_real(ty, 1.0),
                    4 => cx.const_int(ty, 1, false),
                    _ => cx.null(ty),
                };
                let participates = self.participates()?;
                let ballot = self.simd(b"air.simd_ballot.i64", i64_ty, &[participates])?;
                let from = b.and(below, b.trunc(ballot, i32_ty));
                // ponytail: 32 shuffles a call; a partitioned scan if a shader ever leans on it
                let name = named(b"air.simd_shuffle", ty);
                for j in 0..32_u32 {
                    let x = self.simd(&name, ty, &[v, cx.const_int(i16_ty, j.into(), false)])?;
                    let bit = b.and(from, self.int(1 << j));
                    let taken = b.icmp(sys::INT_NE, bit, self.int(0));
                    acc = b.select(taken, b.binop(bin, acc, x), acc);
                }
                Ok(Some(acc))
            }
            // (244, i32 MemoryTypeFlags, i32 SemanticFlags), (245, handle, i32 SemanticFlags):
            // the handle is a UAV, (80, i32 mode): 1 = sync threadgroup, 2 = UAV fence global,
            // 4 = UAV fence threadgroup, 8 = TGSM fence. As airconv's DXBC sync lowering, at any
            // Metal version, as Metal Shader Converter does at Metal 3.1: a fence without the
            // sync bit (GroupMemoryBarrier 8, AllMemoryBarrier 10, DeviceMemoryBarrier 2) orders
            // memory but doesn't wait for the group, so a fence alone; a synced UAV barrier
            // (DeviceMemoryBarrierWithGroupSync 3, AllMemoryBarrierWithGroupSync 11) a fence at
            // its scope, then the barrier, whose memory flags order only within the group.
            op::BARRIER_BY_MEMORY_TYPE | op::BARRIER_BY_MEMORY_HANDLE | op::BARRIER => {
                let mut mode = const_arg(call, 1, 0);
                if opcode != op::BARRIER {
                    // Shader Model 6.8's Barrier() as the op-80 mode it spells (DXC 1.9's
                    // DxilConstants.h): memory UAV 1, group shared 2 (node memory 4, 8 ignored:
                    // no node shader reaches the translator); semantics group sync 1, group
                    // scope 2, device scope 4.
                    let mem = if opcode == op::BARRIER_BY_MEMORY_HANDLE {
                        1
                    } else {
                        mode
                    };
                    let sem = const_arg(call, 2, 0);
                    let uav = if mem & 1 == 0 {
                        0
                    } else if sem & 4 == 0 {
                        4
                    } else {
                        2
                    };
                    mode = (sem & 1) | uav | if mem & 2 == 0 { 0 } else { 8 };
                }
                let sync = mode & 1 != 0;
                let non_execution = self.conv.metal_version >= SM50_SHADER_METAL_320;
                let mut flags = if mode & 8 == 0 {
                    MemFlags::NONE
                } else {
                    MemFlags::THREADGROUP
                };
                // ThreadScope::Device | ThreadScope::Threadgroup, or Threadgroup.
                let scope = if mode & 2 == 0 { 1 } else { 3 };
                if mode & 6 != 0 {
                    flags = flags.with(MemFlags::DEVICE).with(MemFlags::TEXTURE);
                }
                if !sync {
                    if flags.bits() != 0 {
                        self.fence(flags, scope)?;
                    }
                } else if mode & 6 != 0 {
                    self.fence(flags, scope)?;
                    air.create_barrier(if non_execution { MemFlags::NONE } else { flags }, false)?;
                } else {
                    air.create_barrier(flags, false)?;
                }
                Ok(None)
            }
            // (221): only a pixel shader has helper lanes
            op::IS_HELPER_LANE => {
                let value = if self.entry.kind == shader_kind::PIXEL {
                    self.simd(b"air.simd_is_helper_thread", i1, &[])?
                } else {
                    air.get_bool(false)
                };
                Ok(Some(value))
            }
            // (222, i1, i8 op: 0 any, 1 all)
            op::QUAD_VOTE => {
                let name: &[u8] = match const_arg(call, 2, u32::MAX) {
                    0 => b"air.quad_any",
                    1 => b"air.quad_all",
                    _ => return Err(unsupported(call)),
                };
                Ok(Some(self.simd(name, i1, &[arg(1)?])?))
            }
            // (77)
            op::RENDER_TARGET_GET_SAMPLE_COUNT => Ok(Some(air.create_get_num_samples()?)),
            // (76, i32 index) -> {f32, f32}: D3D's standard positions (`air_samplepos`)
            op::RENDER_TARGET_GET_SAMPLE_POSITION => {
                let count = air.create_get_num_samples()?;
                let pos = self.conv.sample_pos(count, arg(1)?)?;
                let value = self.aggregate(
                    call.ty(),
                    &[
                        Some(b.extract_element_u64(pos, 0)),
                        Some(b.extract_element_u64(pos, 1)),
                    ],
                );
                Ok(Some(value))
            }
            // (138): ViewInstancingTier is NOT_SUPPORTED, so the only view is 0
            op::VIEW_ID => Ok(Some(self.int(0))),
            // (256) -> i32: the draw's StartVertexLocation, or BaseVertexLocation when indexed;
            // (257) -> i32: its StartInstanceLocation. `[[base_vertex]]` and `[[base_instance]]`
            // in a vertex function, the draw's arguments in a geometry pipeline's object
            // function (`dxbc/gs.rs`); no other stage has them.
            op::START_VERTEX_LOCATION | op::START_INSTANCE_LOCATION => {
                let v = if opcode == op::START_VERTEX_LOCATION {
                    res.base_vertex_id
                } else {
                    res.base_instance_id
                };
                let v = v.ok_or_else(|| unsupported(call))?;
                Ok(Some(self.cast(v, call.ty())))
            }
            _ => Err(unsupported(call)),
        }
    }
}

/// The parsed module's value `raw` of `cx`, with its type.
///
/// A pointer lifts with [`TyKind::Unknown`](crate::llvm::TyKind::Unknown) as its pointee.
///
/// # Errors
///
/// A value of a type the translator never makes (a token, bfloat, a scalable vector).
pub fn lift(cx: &Context, raw: sys::ValueRef) -> Result<Val<'_>, Error> {
    // SAFETY: a value of the module, of this context.
    let ty = unsafe { sys::type_of(raw) };
    // SAFETY: a type of this context.
    let ty = unsafe { cx.tcx().lift(ty) }
        .ok_or_else(|| Error::new(b"DXIL: a value of a type the translator does not make"))?;
    // SAFETY: a live value of this context, of the type just lifted.
    Ok(unsafe { cx.val(raw, ty) })
}

/// Argument `i`'s constant value, `otherwise` for anything else (`ConstantU32`).
pub fn const_arg(call: Val<'_>, i: u32, otherwise: u32) -> u32 {
    const_u32(metadata::arg(call.raw(), i), otherwise)
}

/// `Unsupported`: `DXIL: <callee> (<opcode>) not supported`.
pub fn unsupported(call: Val<'_>) -> Error {
    // SAFETY: a call of the module.
    let callee = unsafe { sys::get_called_value(call.raw()) };
    Error::new(
        &[
            &b"DXIL: "[..],
            &metadata::value_name(callee),
            b" (",
            op_code(call.raw()).to_string().as_bytes(),
            b") not supported",
        ]
        .concat(),
    )
}

/// `Replace`: `call`'s uses take `value`, and `call` is erased.
///
/// # Errors
///
/// No value for a call something uses (the C++ erased it with its uses left).
///
/// # Safety
///
/// No copy of `call` is used after a call that succeeds.
unsafe fn replace<'cx>(call: Val<'cx>, value: Option<Val<'cx>>) -> Result<(), Error> {
    match value {
        // SAFETY: the call and its replacement, of one context.
        Some(v) => unsafe { sys::replace_all_uses_with(call.raw(), v.raw()) },
        None if has_uses(call.raw()) => {
            return Err(invalid("a value its lowering did not make"));
        }
        None => {}
    }
    // SAFETY: a call nothing uses now.
    unsafe { sys::instruction_erase_from_parent(call.raw()) };
    Ok(())
}

/// Splits `block` before its instruction `at` (`splitBasicBlock`); answers the new block.
pub fn split<'cx>(block: Block<'cx>, at: Val<'cx>) -> Block<'cx> {
    // SAFETY: `at` is an instruction of `block` (the caller's).
    let rest = unsafe { sys::airconv_shim_split_block(block.raw(), at.raw()) };
    // SAFETY: the block the split made, of this context.
    unsafe { Block::new(rest) }
}

/// A handle's resource: its class, range ID, register and declaration (`HandleInfo`).
pub struct Handle<'a, 'cx> {
    pub cls: u32,
    pub range: u32,
    /// The absolute register, as airconv's binding maps take it.
    pub index: Val<'cx>,
    pub resource: &'a super::types::Resource,
}

/// `.f32`, `.f16`, `.s.i32`, `.u.i16`, … for a simdgroup function's value type (`SimdSuffix`).
pub fn simd_suffix(tcx: &TyCx, ty: Ty, is_signed: bool) -> Vec<u8> {
    match tcx.kind(ty) {
        TyKind::Float => b".f32".to_vec(),
        TyKind::Half => b".f16".to_vec(),
        kind => {
            let bits = if let TyKind::Int(bits) = kind {
                bits
            } else {
                0
            };
            let sign: &[u8] = if is_signed { b".s" } else { b".u" };
            [sign, b".i", bits.to_string().as_bytes()].concat()
        }
    }
}

/// `Type::getPrimitiveSizeInBits`: a scalar's or a fixed vector's bits, 0 for anything else.
pub fn primitive_bits(tcx: &TyCx, ty: Ty) -> u32 {
    match tcx.kind(ty) {
        TyKind::Int(bits) => bits,
        TyKind::Half => 16,
        TyKind::Float => 32,
        TyKind::Double => 64,
        TyKind::Vector(element, n) => n.wrapping_mul(primitive_bits(tcx, element)),
        _ => 0,
    }
}

/// `Type::isFloatingPointTy`: `half`, `float` or `double`.
pub fn is_fp(tcx: &TyCx, ty: Ty) -> bool {
    matches!(tcx.kind(ty), TyKind::Half | TyKind::Float | TyKind::Double)
}

/// Whether anything uses the value `v` (`!use_empty()`).
pub fn has_uses(v: sys::ValueRef) -> bool {
    // SAFETY: a live value.
    !unsafe { sys::get_first_use(v) }.is_null()
}

/// The blocks from `first` along the function's list.
pub fn walk_blocks(first: sys::BasicBlockRef) -> impl Iterator<Item = sys::BasicBlockRef> {
    core::iter::successors((!first.is_null()).then_some(first), |&bb| {
        // SAFETY: a block of a live function.
        let next = unsafe { sys::get_next_basic_block(bb) };
        (!next.is_null()).then_some(next)
    })
}

/// Erases `block`'s terminator (`getTerminator()->eraseFromParent()`); none is left alone.
///
/// # Safety
///
/// No `Val` of the terminator is used after the call.
pub unsafe fn erase_terminator(block: Block<'_>) {
    // SAFETY: a live block.
    let term = unsafe { sys::get_basic_block_terminator(block.raw()) };
    if !term.is_null() {
        // SAFETY: the block's terminator; the block is reterminated by the caller.
        unsafe { sys::instruction_erase_from_parent(term) };
    }
}

/// `BranchInst::Create(next, bb)`: a branch at the end of `bb`, made with no builder state.
///
/// A fresh builder, so the branch carries no debug location (as the C++'s constructor).
pub fn branch_at_end<'cx>(cx: &'cx Context, bb: Block<'cx>, next: Block<'cx>) -> Val<'cx> {
    let b = Builder::new(cx);
    b.position_at_end(bb);
    b.br(next)
}
