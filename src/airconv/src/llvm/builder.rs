//! The instruction builder: every build method takes and returns typed values.
//!
//! [`Builder`] is LLVM's `IRBuilder<>` (the C API's builder is one), so each method folds
//! constants and applies the current fast-math flags exactly as the C++'s `Create*` of the same
//! name. Where the C API has no form of an `IRBuilder` call the method rebuilds it from that
//! call's own steps (`vector_splat`, `zext_or_trunc`, …), and the integer-index forms the C API
//! lacks are explicit: `extract_element_u64` makes an `i64` index as `IRBuilder`'s `uint64_t`
//! overload does, `const_gep1_32` an `i32` one. Values are unnamed, as the C++'s calls leave
//! them, except where a method takes a name. Every value a method makes passes through one place
//! (`Builder::made`), where its `Ty` is attached.

use core::{cell::Cell, cmp::Ordering, ffi::CStr};

use super::{Block, Context, Func, Ty, TyCx, TyKind, Val, len_u32, sys};

/// The fast-math flags of floating-point instructions, `FastMathFlags`' bits.
#[derive(Clone, Copy)]
pub struct FastMath(u32);

impl FastMath {
    /// No flag.
    pub const NONE: Self = Self(0);
    /// `reassoc`.
    pub const ALLOW_REASSOC: Self = Self(1);
    /// `nnan`.
    pub const NO_NANS: Self = Self(1 << 1);
    /// `ninf`.
    pub const NO_INFS: Self = Self(1 << 2);
    /// `nsz`.
    pub const NO_SIGNED_ZEROS: Self = Self(1 << 3);
    /// `arcp`.
    pub const ALLOW_RECIPROCAL: Self = Self(1 << 4);
    /// `contract`.
    pub const ALLOW_CONTRACT: Self = Self(1 << 5);
    /// `afn`.
    pub const APPROX_FUNC: Self = Self(1 << 6);

    /// The flags as `FastMathFlags`' bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// The flags of both.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// These flags without `other`'s.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Whether every flag of `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Puts the builder's fast-math flags back when dropped (`IRBuilder::FastMathFlagGuard`).
pub struct FastMathGuard<'b, 'cx> {
    builder: &'b Builder<'cx>,
    saved: FastMath,
}

impl Drop for FastMathGuard<'_, '_> {
    fn drop(&mut self) {
        self.builder.set_fast_math(self.saved);
    }
}

/// An instruction builder of a context, positioned where its instructions go.
pub struct Builder<'cx> {
    raw: sys::BuilderRef,
    cx: &'cx Context,
    fast_math: Cell<FastMath>,
}

impl Drop for Builder<'_> {
    fn drop(&mut self) {
        // SAFETY: the builder is live and owned here.
        unsafe { sys::dispose_builder(self.raw) };
    }
}

/// The empty name, the C++'s default.
const NO_NAME: &CStr = c"";

impl<'cx> Builder<'cx> {
    /// A builder of `cx`, not yet positioned, with no fast-math flag.
    #[must_use]
    pub fn new(cx: &'cx Context) -> Self {
        // SAFETY: the context is live.
        let raw = unsafe { sys::create_builder_in_context(cx.raw()) };
        Self {
            raw,
            cx,
            fast_math: Cell::new(FastMath::NONE),
        }
    }

    /// The `LLVMBuilderRef`.
    #[must_use]
    pub const fn raw(&self) -> sys::BuilderRef {
        self.raw
    }

    /// The context.
    #[must_use]
    pub const fn cx(&self) -> &'cx Context {
        self.cx
    }

    const fn tcx(&self) -> &'cx TyCx {
        self.cx.tcx()
    }

    /// The value `raw` that `op` made, of type `ty`.
    ///
    /// Every value a method makes comes through here. With the feature `oracle`, `ty` is checked
    /// against the value's LLVM type, and a difference is kept as the context's first error (the
    /// slot of P5's handler), which every check of the context answers.
    fn made(&self, op: &'static str, raw: sys::ValueRef, ty: Ty) -> Val<'cx> {
        // SAFETY: `raw` is what the builder of this context just answered, of type `ty` (each
        // caller derives it as `IRBuilder` types the instruction).
        let v = unsafe { self.cx.val(raw, ty) };
        #[cfg(feature = "oracle")]
        if let Some(text) = crate::oracle::check(self.cx, op, v) {
            self.cx.diagnostics().error.borrow_mut().get_or_insert(text);
        }
        #[cfg(not(feature = "oracle"))]
        let _ = op;
        v
    }

    /// Positions the builder at the end of `block`.
    pub fn position_at_end(&self, block: Block<'cx>) {
        // SAFETY: the builder and the block are live.
        unsafe { sys::position_builder_at_end(self.raw, block.raw()) };
    }

    /// Positions the builder before the instruction `inst`.
    pub fn position_before(&self, inst: Val<'cx>) {
        // SAFETY: the builder is live; `inst` is an instruction in a block (the caller's value).
        unsafe { sys::position_builder_before(self.raw, inst.raw()) };
    }

    /// The block the builder inserts into.
    #[must_use]
    pub fn insert_block(&self) -> Option<Block<'cx>> {
        // SAFETY: the builder is live.
        let raw = unsafe { sys::get_insert_block(self.raw) };
        // SAFETY: a block of this context.
        (!raw.is_null()).then(|| unsafe { Block::new(raw) })
    }

    /// The current fast-math flags.
    #[must_use]
    pub const fn fast_math(&self) -> FastMath {
        self.fast_math.get()
    }

    /// Sets the fast-math flags of the floating-point instructions made from now on.
    ///
    /// `IRBuilder::setFastMathFlags`, through the shim.
    pub fn set_fast_math(&self, flags: FastMath) {
        self.fast_math.set(flags);
        // SAFETY: the builder is live.
        unsafe { sys::airconv_shim_set_fast_math_flags(self.raw, flags.bits()) };
    }

    /// Sets `flags` until the guard is dropped, which puts the current ones back.
    #[must_use]
    pub fn fast_math_guard(&self, flags: FastMath) -> FastMathGuard<'_, 'cx> {
        let saved = self.fast_math();
        self.set_fast_math(flags);
        FastMathGuard {
            builder: self,
            saved,
        }
    }

    // Arithmetic: the result has the left operand's type.

    /// `add`.
    #[must_use]
    pub fn add(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: the builder and the operands are live, of this context.
        let raw = unsafe { sys::build_add(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("add", raw, lhs.ty())
    }

    /// `sub`.
    #[must_use]
    pub fn sub(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_sub(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("sub", raw, lhs.ty())
    }

    /// `mul`.
    #[must_use]
    pub fn mul(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_mul(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("mul", raw, lhs.ty())
    }

    /// `udiv`.
    #[must_use]
    pub fn udiv(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_u_div(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("udiv", raw, lhs.ty())
    }

    /// `sdiv`.
    #[must_use]
    pub fn sdiv(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_s_div(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("sdiv", raw, lhs.ty())
    }

    /// `urem`.
    #[must_use]
    pub fn urem(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_u_rem(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("urem", raw, lhs.ty())
    }

    /// `srem`.
    #[must_use]
    pub fn srem(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_s_rem(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("srem", raw, lhs.ty())
    }

    /// `shl`.
    #[must_use]
    pub fn shl(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_shl(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("shl", raw, lhs.ty())
    }

    /// `lshr`.
    #[must_use]
    pub fn lshr(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_l_shr(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("lshr", raw, lhs.ty())
    }

    /// `ashr`.
    #[must_use]
    pub fn ashr(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_a_shr(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("ashr", raw, lhs.ty())
    }

    /// `and`.
    #[must_use]
    pub fn and(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_and(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("and", raw, lhs.ty())
    }

    /// `or`.
    #[must_use]
    pub fn or(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_or(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("or", raw, lhs.ty())
    }

    /// `xor`.
    #[must_use]
    pub fn xor(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_xor(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("xor", raw, lhs.ty())
    }

    /// `fadd`.
    #[must_use]
    pub fn fadd(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_f_add(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("fadd", raw, lhs.ty())
    }

    /// `fsub`.
    #[must_use]
    pub fn fsub(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_f_sub(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("fsub", raw, lhs.ty())
    }

    /// `fmul`.
    #[must_use]
    pub fn fmul(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_f_mul(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("fmul", raw, lhs.ty())
    }

    /// `fdiv`.
    #[must_use]
    pub fn fdiv(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_f_div(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("fdiv", raw, lhs.ty())
    }

    /// `frem`.
    #[must_use]
    pub fn frem(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_f_rem(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("frem", raw, lhs.ty())
    }

    /// The binary operation `op` (`IRBuilder::CreateBinOp`).
    ///
    /// `op` is an `Instruction::BinaryOps` opcode (`sys::ADD`, …).
    #[must_use]
    pub fn binop(&self, op: sys::Opcode, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`; `op` is a binary opcode (the caller's).
        let raw =
            unsafe { sys::build_bin_op(self.raw, op, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("binop", raw, lhs.ty())
    }

    /// `sub 0, v`.
    #[must_use]
    pub fn neg(&self, v: Val<'cx>) -> Val<'cx> {
        // SAFETY: the builder and the operand are live, of this context.
        let raw = unsafe { sys::build_neg(self.raw, v.raw(), NO_NAME.as_ptr()) };
        self.made("neg", raw, v.ty())
    }

    /// `fneg`.
    #[must_use]
    pub fn fneg(&self, v: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `neg`.
        let raw = unsafe { sys::build_f_neg(self.raw, v.raw(), NO_NAME.as_ptr()) };
        self.made("fneg", raw, v.ty())
    }

    /// `xor v, -1`.
    #[must_use]
    pub fn not(&self, v: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `neg`.
        let raw = unsafe { sys::build_not(self.raw, v.raw(), NO_NAME.as_ptr()) };
        self.made("not", raw, v.ty())
    }

    // Casts: the result has the destination type (a no-op cast answers `v`).

    /// The cast `op` (`IRBuilder::CreateCast`), an `Instruction::CastOps` opcode.
    #[must_use]
    pub fn cast(&self, op: sys::Opcode, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: the builder and the operand are live, `to` lowers to a type of this context;
        // `op` is a cast opcode (the caller's).
        let raw = unsafe {
            sys::build_cast(
                self.raw,
                op,
                v.raw(),
                self.tcx().lower(to),
                NO_NAME.as_ptr(),
            )
        };
        self.made("cast", raw, to)
    }

    /// `trunc`.
    #[must_use]
    pub fn trunc(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw =
            unsafe { sys::build_trunc(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr()) };
        self.made("trunc", raw, to)
    }

    /// `zext`.
    #[must_use]
    pub fn zext(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw =
            unsafe { sys::build_z_ext(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr()) };
        self.made("zext", raw, to)
    }

    /// `sext`.
    #[must_use]
    pub fn sext(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw =
            unsafe { sys::build_s_ext(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr()) };
        self.made("sext", raw, to)
    }

    /// `fptrunc`.
    #[must_use]
    pub fn fptrunc(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_fp_trunc(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("fptrunc", raw, to)
    }

    /// `fpext`.
    #[must_use]
    pub fn fpext(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw =
            unsafe { sys::build_fp_ext(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr()) };
        self.made("fpext", raw, to)
    }

    /// `fptoui`.
    #[must_use]
    pub fn fptoui(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_fp_to_ui(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("fptoui", raw, to)
    }

    /// `fptosi`.
    #[must_use]
    pub fn fptosi(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_fp_to_si(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("fptosi", raw, to)
    }

    /// `uitofp`.
    #[must_use]
    pub fn uitofp(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_ui_to_fp(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("uitofp", raw, to)
    }

    /// `sitofp`.
    #[must_use]
    pub fn sitofp(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_si_to_fp(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("sitofp", raw, to)
    }

    /// `ptrtoint`.
    #[must_use]
    pub fn ptrtoint(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_ptr_to_int(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("ptrtoint", raw, to)
    }

    /// `inttoptr`.
    #[must_use]
    pub fn inttoptr(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_int_to_ptr(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("inttoptr", raw, to)
    }

    /// `bitcast`.
    #[must_use]
    pub fn bitcast(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_bit_cast(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("bitcast", raw, to)
    }

    /// `addrspacecast`.
    #[must_use]
    pub fn addrspacecast(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_addr_space_cast(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("addrspacecast", raw, to)
    }

    /// `IRBuilder::CreatePointerCast`: a bitcast or an address-space cast.
    #[must_use]
    pub fn pointer_cast(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_pointer_cast(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("pointer_cast", raw, to)
    }

    /// `IRBuilder::CreateFPCast`: an extension or a truncation.
    #[must_use]
    pub fn fp_cast(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_fp_cast(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("fp_cast", raw, to)
    }

    /// `IRBuilder::CreateIntCast`: an extension (by sign when `signed`) or a truncation.
    #[must_use]
    pub fn int_cast(&self, v: Val<'cx>, to: Ty, signed: bool) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_int_cast2(
                self.raw,
                v.raw(),
                self.tcx().lower(to),
                signed.into(),
                NO_NAME.as_ptr(),
            )
        };
        self.made("int_cast", raw, to)
    }

    /// `IRBuilder::CreateZExtOrTrunc`: by the scalar widths, `v` itself when they are equal.
    #[must_use]
    pub fn zext_or_trunc(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        let (from, into) = (self.tcx().scalar_bits(v.ty()), self.tcx().scalar_bits(to));
        match from.cmp(&into) {
            Ordering::Less => self.zext(v, to),
            Ordering::Greater => self.trunc(v, to),
            Ordering::Equal => v,
        }
    }

    /// `IRBuilder::CreateBitOrPointerCast`.
    ///
    /// `v` itself for its own type, else `ptrtoint`, `inttoptr` or `bitcast` by the kinds.
    #[must_use]
    pub fn bit_or_pointer_cast(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        if v.ty() == to {
            return v;
        }
        match (self.tcx().kind(v.ty()), self.tcx().kind(to)) {
            (TyKind::Ptr { .. }, TyKind::Int(_)) => self.ptrtoint(v, to),
            (TyKind::Int(_), TyKind::Ptr { .. }) => self.inttoptr(v, to),
            _ => self.bitcast(v, to),
        }
    }

    // Comparisons: `i1`, or a vector of them for vector operands.

    fn bool_like(&self, v: Val<'cx>) -> Ty {
        let i1 = self.tcx().int(1);
        match self.tcx().kind(v.ty()) {
            TyKind::Vector(_, n) => self.tcx().vector(i1, n),
            _ => i1,
        }
    }

    /// `icmp pred` (`sys::INT_EQ`, …).
    #[must_use]
    pub fn icmp(&self, pred: sys::IntPredicate, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`; `pred` is an integer predicate (the caller's).
        let raw =
            unsafe { sys::build_i_cmp(self.raw, pred, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("icmp", raw, self.bool_like(lhs))
    }

    /// `fcmp pred` (`sys::REAL_OEQ`, …).
    #[must_use]
    pub fn fcmp(&self, pred: sys::RealPredicate, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`; `pred` is a floating-point predicate (the caller's).
        let raw =
            unsafe { sys::build_f_cmp(self.raw, pred, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("fcmp", raw, self.bool_like(lhs))
    }

    /// `icmp eq v, null`.
    #[must_use]
    pub fn is_null(&self, v: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `neg`.
        let raw = unsafe { sys::build_is_null(self.raw, v.raw(), NO_NAME.as_ptr()) };
        self.made("is_null", raw, self.bool_like(v))
    }

    /// `icmp ne v, null`.
    #[must_use]
    pub fn is_not_null(&self, v: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `neg`.
        let raw = unsafe { sys::build_is_not_null(self.raw, v.raw(), NO_NAME.as_ptr()) };
        self.made("is_not_null", raw, self.bool_like(v))
    }

    /// `IRBuilder::CreateIsNotNeg`: `icmp sgt v, -1`.
    #[must_use]
    pub fn is_not_neg(&self, v: Val<'cx>) -> Val<'cx> {
        self.icmp(sys::INT_SGT, v, self.cx.all_ones(v.ty()))
    }

    /// `IRBuilder::CreateLogicalAnd`: `select a, b, false`.
    #[must_use]
    pub fn logical_and(&self, a: Val<'cx>, b: Val<'cx>) -> Val<'cx> {
        self.select(a, b, self.cx.null(b.ty()))
    }

    /// `select`.
    #[must_use]
    pub fn select(&self, cond: Val<'cx>, then: Val<'cx>, otherwise: Val<'cx>) -> Val<'cx> {
        // SAFETY: the builder and the operands are live, of this context.
        let raw = unsafe {
            sys::build_select(
                self.raw,
                cond.raw(),
                then.raw(),
                otherwise.raw(),
                NO_NAME.as_ptr(),
            )
        };
        self.made("select", raw, then.ty())
    }

    // Memory.

    /// `alloca ty`, named `name`, in address space 0 (AIR's data layout names no other).
    #[must_use]
    pub fn alloca(&self, ty: Ty, name: &CStr) -> Val<'cx> {
        // SAFETY: the builder is live, `ty` lowers to a type of its context; `name` is
        // NUL-terminated.
        let raw = unsafe { sys::build_alloca(self.raw, self.tcx().lower(ty), name.as_ptr()) };
        self.made("alloca", raw, self.tcx().ptr(ty, 0))
    }

    /// `load ty, ptr`, at `ty`'s ABI alignment.
    #[must_use]
    pub fn load(&self, ty: Ty, ptr: Val<'cx>) -> Val<'cx> {
        // SAFETY: the builder and `ptr` are live, `ty` lowers to a type of their context.
        let raw = unsafe {
            sys::build_load2(self.raw, self.tcx().lower(ty), ptr.raw(), NO_NAME.as_ptr())
        };
        self.made("load", raw, ty)
    }

    /// `load ty, ptr, align`.
    #[must_use]
    pub fn aligned_load(&self, ty: Ty, ptr: Val<'cx>, align: u32) -> Val<'cx> {
        let load = self.load(ty, ptr);
        self.set_alignment(load, align);
        load
    }

    /// `store v, ptr`, at `v`'s ABI alignment; answers the store.
    pub fn store(&self, v: Val<'cx>, ptr: Val<'cx>) -> Val<'cx> {
        // SAFETY: the builder and the operands are live, of this context.
        let raw = unsafe { sys::build_store(self.raw, v.raw(), ptr.raw()) };
        self.made("store", raw, self.tcx().void())
    }

    /// `store v, ptr, align`; answers the store.
    pub fn aligned_store(&self, v: Val<'cx>, ptr: Val<'cx>, align: u32) -> Val<'cx> {
        let store = self.store(v, ptr);
        self.set_alignment(store, align);
        store
    }

    /// Sets the alignment of a load, store, alloca, atomic or global; anything else is left alone.
    pub fn set_alignment(&self, v: Val<'cx>, align: u32) {
        let raw = v.raw();
        let kinds: [unsafe extern "C" fn(sys::ValueRef) -> sys::ValueRef; 6] = [
            sys::is_a_load_inst,
            sys::is_a_store_inst,
            sys::is_a_alloca_inst,
            sys::is_a_atomic_rmw_inst,
            sys::is_a_atomic_cmp_xchg_inst,
            sys::is_a_global_object,
        ];
        let aligned = kinds.iter().any(|is_a| {
            // SAFETY: a live value.
            !unsafe { is_a(raw) }.is_null()
        });
        if aligned {
            // SAFETY: one of the values `LLVMSetAlignment` takes.
            unsafe { sys::set_alignment(raw, align) };
        }
    }

    /// The type of a GEP over `source` with `indices` from `ptr`.
    ///
    /// A pointer, in `ptr`'s address space, to the indexed type.
    fn gep_ty(&self, source: Ty, ptr: Val<'cx>, indices: &[Val<'cx>]) -> Ty {
        let tcx = self.tcx();
        let constants: Vec<Option<u64>> = indices.iter().map(|i| i.const_u64()).collect();
        let pointee = tcx.indexed(source, &constants).unwrap_or(source);
        tcx.ptr(pointee, tcx.addrspace(ptr.ty()).unwrap_or(0))
    }

    /// `getelementptr source, ptr, indices…`.
    #[must_use]
    pub fn gep(&self, source: Ty, ptr: Val<'cx>, indices: &[Val<'cx>]) -> Val<'cx> {
        let mut raws: Vec<sys::ValueRef> = indices.iter().map(|i| i.raw()).collect();
        // SAFETY: the builder and the operands are live, `source` lowers to a type of their
        // context; `raws` holds its length.
        let raw = unsafe {
            sys::build_gep2(
                self.raw,
                self.tcx().lower(source),
                ptr.raw(),
                raws.as_mut_ptr(),
                len_u32(raws.len()),
                NO_NAME.as_ptr(),
            )
        };
        self.made("gep", raw, self.gep_ty(source, ptr, indices))
    }

    /// `getelementptr inbounds source, ptr, indices…`.
    #[must_use]
    pub fn inbounds_gep(&self, source: Ty, ptr: Val<'cx>, indices: &[Val<'cx>]) -> Val<'cx> {
        let mut raws: Vec<sys::ValueRef> = indices.iter().map(|i| i.raw()).collect();
        // SAFETY: as `gep`.
        let raw = unsafe {
            sys::build_in_bounds_gep2(
                self.raw,
                self.tcx().lower(source),
                ptr.raw(),
                raws.as_mut_ptr(),
                len_u32(raws.len()),
                NO_NAME.as_ptr(),
            )
        };
        self.made("inbounds_gep", raw, self.gep_ty(source, ptr, indices))
    }

    /// `IRBuilder::CreateStructGEP`: `getelementptr inbounds source, ptr, i32 0, i32 index`.
    #[must_use]
    pub fn struct_gep(&self, source: Ty, ptr: Val<'cx>, index: u32) -> Val<'cx> {
        // SAFETY: as `gep`.
        let raw = unsafe {
            sys::build_struct_gep2(
                self.raw,
                self.tcx().lower(source),
                ptr.raw(),
                index,
                NO_NAME.as_ptr(),
            )
        };
        let i32_ty = self.tcx().int(32);
        let indices = [
            self.cx.const_int(i32_ty, 0, false),
            self.cx.const_int(i32_ty, u64::from(index), false),
        ];
        self.made("struct_gep", raw, self.gep_ty(source, ptr, &indices))
    }

    /// `IRBuilder::CreateConstGEP1_32`: `getelementptr source, ptr, i32 index`.
    #[must_use]
    pub fn const_gep1_32(&self, source: Ty, ptr: Val<'cx>, index: u32) -> Val<'cx> {
        let index = self
            .cx
            .const_int(self.tcx().int(32), u64::from(index), false);
        self.gep(source, ptr, &[index])
    }

    /// `IRBuilder::CreateConstGEP1_64`: `getelementptr source, ptr, i64 index`.
    #[must_use]
    pub fn const_gep1_64(&self, source: Ty, ptr: Val<'cx>, index: u64) -> Val<'cx> {
        let index = self.cx.const_int(self.tcx().int(64), index, false);
        self.gep(source, ptr, &[index])
    }

    /// `IRBuilder::CreateConstInBoundsGEP1_32`: `getelementptr inbounds source, ptr, i32 index`.
    #[must_use]
    pub fn const_inbounds_gep1_32(&self, source: Ty, ptr: Val<'cx>, index: u32) -> Val<'cx> {
        let index = self
            .cx
            .const_int(self.tcx().int(32), u64::from(index), false);
        self.inbounds_gep(source, ptr, &[index])
    }

    // Vectors and aggregates.

    /// `extractelement vec, index` with the value `index`.
    #[must_use]
    pub fn extract_element(&self, vec: Val<'cx>, index: Val<'cx>) -> Val<'cx> {
        // SAFETY: the builder and the operands are live, of this context.
        let raw = unsafe {
            sys::build_extract_element(self.raw, vec.raw(), index.raw(), NO_NAME.as_ptr())
        };
        let ty = self.tcx().element(vec.ty()).unwrap_or_else(|| vec.ty());
        self.made("extract_element", raw, ty)
    }

    /// `extractelement vec, i64 index` (`IRBuilder`'s `uint64_t` overload).
    #[must_use]
    pub fn extract_element_u64(&self, vec: Val<'cx>, index: u64) -> Val<'cx> {
        self.extract_element(vec, self.cx.const_int(self.tcx().int(64), index, false))
    }

    /// `insertelement vec, element, index` with the value `index`.
    #[must_use]
    pub fn insert_element(&self, vec: Val<'cx>, element: Val<'cx>, index: Val<'cx>) -> Val<'cx> {
        // SAFETY: the builder and the operands are live, of this context.
        let raw = unsafe {
            sys::build_insert_element(
                self.raw,
                vec.raw(),
                element.raw(),
                index.raw(),
                NO_NAME.as_ptr(),
            )
        };
        self.made("insert_element", raw, vec.ty())
    }

    /// `insertelement vec, element, i64 index` (`IRBuilder`'s `uint64_t` overload).
    #[must_use]
    pub fn insert_element_u64(&self, vec: Val<'cx>, element: Val<'cx>, index: u64) -> Val<'cx> {
        self.insert_element(
            vec,
            element,
            self.cx.const_int(self.tcx().int(64), index, false),
        )
    }

    /// `shufflevector a, b, mask`; a mask element of -1 is `undef`.
    #[must_use]
    pub fn shuffle_vector(&self, a: Val<'cx>, b: Val<'cx>, mask: &[i32]) -> Val<'cx> {
        let i32_ty = self.tcx().int(32);
        let elements: Vec<Val<'cx>> = mask
            .iter()
            .map(|&m| {
                u64::try_from(m).map_or_else(
                    |_| self.cx.undef(i32_ty),
                    |m| self.cx.const_int(i32_ty, m, false),
                )
            })
            .collect();
        let mask = self.cx.const_vector(&elements);
        // SAFETY: the builder and the operands are live, of this context; `mask` a constant
        // vector of `i32`.
        let raw = unsafe {
            sys::build_shuffle_vector(self.raw, a.raw(), b.raw(), mask.raw(), NO_NAME.as_ptr())
        };
        let element = self.tcx().element(a.ty()).unwrap_or_else(|| a.ty());
        self.made(
            "shuffle_vector",
            raw,
            self.tcx().vector(element, len_u32(elements.len())),
        )
    }

    /// `shufflevector v, poison, mask` (`IRBuilder`'s one-vector form).
    #[must_use]
    pub fn shuffle_vector1(&self, v: Val<'cx>, mask: &[i32]) -> Val<'cx> {
        self.shuffle_vector(v, self.cx.poison(v.ty()), mask)
    }

    /// `IRBuilder::CreateVectorSplat`.
    ///
    /// `v` inserted at `i32 0` of a poison vector of `len` (`<name>.splatinsert`), shuffled
    /// with a zero mask (`<name>.splat`).
    #[must_use]
    pub fn vector_splat(&self, len: u32, v: Val<'cx>, name: &CStr) -> Val<'cx> {
        let vec_ty = self.tcx().vector(v.ty(), len);
        let zero = self.cx.const_int(self.tcx().int(32), 0, false);
        let named = |suffix: &[u8]| [name.to_bytes(), suffix, b"\0"].concat();
        let insert_name = named(b".splatinsert");
        let insert_name = CStr::from_bytes_with_nul(&insert_name).unwrap_or_default();
        // SAFETY: the builder and the operands are live, of this context; the name is
        // NUL-terminated.
        let raw = unsafe {
            sys::build_insert_element(
                self.raw,
                self.cx.poison(vec_ty).raw(),
                v.raw(),
                zero.raw(),
                insert_name.as_ptr(),
            )
        };
        let inserted = self.made("vector_splat", raw, vec_ty);
        let mask = self.cx.null(self.tcx().vector(self.tcx().int(32), len));
        let splat_name = named(b".splat");
        let splat_name = CStr::from_bytes_with_nul(&splat_name).unwrap_or_default();
        // SAFETY: as above; `mask` a constant vector of `i32`.
        let raw = unsafe {
            sys::build_shuffle_vector(
                self.raw,
                inserted.raw(),
                self.cx.poison(vec_ty).raw(),
                mask.raw(),
                splat_name.as_ptr(),
            )
        };
        self.made("vector_splat", raw, vec_ty)
    }

    /// `extractvalue agg, index`.
    #[must_use]
    pub fn extract_value(&self, agg: Val<'cx>, index: u32) -> Val<'cx> {
        // SAFETY: the builder and the operand are live, of this context.
        let raw = unsafe { sys::build_extract_value(self.raw, agg.raw(), index, NO_NAME.as_ptr()) };
        let tcx = self.tcx();
        let ty = tcx
            .field(agg.ty(), index)
            .or_else(|| tcx.element(agg.ty()))
            .unwrap_or_else(|| agg.ty());
        self.made("extract_value", raw, ty)
    }

    /// `insertvalue agg, v, index`.
    #[must_use]
    pub fn insert_value(&self, agg: Val<'cx>, v: Val<'cx>, index: u32) -> Val<'cx> {
        // SAFETY: the builder and the operands are live, of this context.
        let raw = unsafe {
            sys::build_insert_value(self.raw, agg.raw(), v.raw(), index, NO_NAME.as_ptr())
        };
        self.made("insert_value", raw, agg.ty())
    }

    /// `phi ty` with no incoming value yet.
    #[must_use]
    pub fn phi(&self, ty: Ty) -> Val<'cx> {
        // SAFETY: the builder is live, `ty` lowers to a type of its context.
        let raw = unsafe { sys::build_phi(self.raw, self.tcx().lower(ty), NO_NAME.as_ptr()) };
        self.made("phi", raw, ty)
    }

    /// Adds `value` from `block` to the phi `phi`.
    pub fn add_incoming(&self, phi: Val<'cx>, value: Val<'cx>, block: Block<'cx>) {
        let (mut value, mut block) = (value.raw(), block.raw());
        // SAFETY: `phi` is a phi of this context, `value` and `block` of it; one entry each.
        unsafe { sys::add_incoming(phi.raw(), &raw mut value, &raw mut block, 1) };
    }

    /// `call callee(args…)`; the result has the callee's return type.
    #[must_use]
    pub fn call(&self, callee: Func<'cx>, args: &[Val<'cx>]) -> Val<'cx> {
        let tcx = self.tcx();
        let mut raws: Vec<sys::ValueRef> = args.iter().map(|a| a.raw()).collect();
        // SAFETY: the builder, the callee and the arguments are live, of this context; the
        // function type lowers to one of it; `raws` holds its length.
        let raw = unsafe {
            sys::build_call2(
                self.raw,
                tcx.lower(callee.fn_ty()),
                callee.val().raw(),
                raws.as_mut_ptr(),
                len_u32(raws.len()),
                NO_NAME.as_ptr(),
            )
        };
        let ret = tcx.ret(callee.fn_ty()).unwrap_or_else(|| tcx.void());
        self.made("call", raw, ret)
    }

    /// `IRBuilder::CreateBinaryIntrinsic`: a call of the intrinsic `name` (`llvm.umax`, …).
    ///
    /// The intrinsic is overloaded on `lhs`' type. `None` for a name LLVM does not know or an
    /// unpositioned builder.
    #[must_use]
    pub fn binary_intrinsic(&self, name: &[u8], lhs: Val<'cx>, rhs: Val<'cx>) -> Option<Val<'cx>> {
        let tcx = self.tcx();
        // SAFETY: `name` holds its length.
        let id = unsafe { sys::lookup_intrinsic_id(name.as_ptr().cast(), name.len()) };
        let block = self.insert_block()?;
        if id == 0 {
            return None;
        }
        // SAFETY: a block of this context.
        let function = unsafe { sys::get_basic_block_parent(block.raw()) };
        // SAFETY: the function of a block.
        let module = unsafe { sys::get_global_parent(function) };
        let mut overload = [tcx.lower(lhs.ty())];
        // SAFETY: the module is live, `id` an intrinsic, `overload` one type of its context.
        let decl = unsafe { sys::get_intrinsic_declaration(module, id, overload.as_mut_ptr(), 1) };
        // SAFETY: as above.
        let fn_raw =
            unsafe { sys::intrinsic_get_type(self.cx.raw(), id, overload.as_mut_ptr(), 1) };
        // SAFETY: a function type of this context.
        let fn_ty = unsafe { tcx.lift(fn_raw) }?;
        // SAFETY: the declaration is a function of this context, of `fn_ty`, in address space 0.
        let callee = unsafe { Func::new(decl, fn_ty, tcx.ptr(fn_ty, 0)) };
        Some(self.call(callee, &[lhs, rhs]))
    }

    // Terminators.

    /// `ret void`.
    pub fn ret_void(&self) -> Val<'cx> {
        // SAFETY: the builder is live.
        let raw = unsafe { sys::build_ret_void(self.raw) };
        self.made("ret_void", raw, self.tcx().void())
    }

    /// `ret v`.
    pub fn ret(&self, v: Val<'cx>) -> Val<'cx> {
        // SAFETY: the builder and the operand are live, of this context.
        let raw = unsafe { sys::build_ret(self.raw, v.raw()) };
        self.made("ret", raw, self.tcx().void())
    }

    /// `br dest`.
    pub fn br(&self, dest: Block<'cx>) -> Val<'cx> {
        // SAFETY: the builder and the block are live, of this context.
        let raw = unsafe { sys::build_br(self.raw, dest.raw()) };
        self.made("br", raw, self.tcx().void())
    }

    /// `br cond, then, otherwise`.
    pub fn cond_br(&self, cond: Val<'cx>, then: Block<'cx>, otherwise: Block<'cx>) -> Val<'cx> {
        // SAFETY: the builder, the condition and the blocks are live, of this context.
        let raw = unsafe { sys::build_cond_br(self.raw, cond.raw(), then.raw(), otherwise.raw()) };
        self.made("cond_br", raw, self.tcx().void())
    }

    /// `switch v, default` with room for `cases` cases.
    pub fn switch(&self, v: Val<'cx>, default: Block<'cx>, cases: u32) -> Val<'cx> {
        // SAFETY: the builder, the value and the block are live, of this context.
        let raw = unsafe { sys::build_switch(self.raw, v.raw(), default.raw(), cases) };
        self.made("switch", raw, self.tcx().void())
    }

    /// Adds the case `on` (a constant) going to `dest` to the switch `switch`.
    pub fn add_case(&self, switch: Val<'cx>, on: Val<'cx>, dest: Block<'cx>) {
        // SAFETY: `switch` is a switch of this context, `on` a constant and `dest` a block of it.
        unsafe { sys::add_case(switch.raw(), on.raw(), dest.raw()) };
    }

    /// `unreachable`.
    pub fn unreachable(&self) -> Val<'cx> {
        // SAFETY: the builder is live.
        let raw = unsafe { sys::build_unreachable(self.raw) };
        self.made("unreachable", raw, self.tcx().void())
    }

    // Atomics.

    /// `atomicrmw op ptr, v ordering`; the result has `v`'s type.
    ///
    /// Aligned to `v`'s store size, in the system scope or a single thread's.
    #[must_use]
    pub fn atomic_rmw(
        &self,
        op: sys::AtomicRMWBinOp,
        ptr: Val<'cx>,
        v: Val<'cx>,
        ordering: sys::AtomicOrdering,
        single_thread: bool,
    ) -> Val<'cx> {
        // SAFETY: the builder and the operands are live, of this context; `op` and `ordering`
        // are the C API's (the caller's).
        let raw = unsafe {
            sys::build_atomic_rmw(
                self.raw,
                op,
                ptr.raw(),
                v.raw(),
                ordering,
                single_thread.into(),
            )
        };
        self.made("atomic_rmw", raw, v.ty())
    }

    /// `cmpxchg ptr, cmp, new success failure`; the result is `{ T, i1 }`.
    #[must_use]
    pub fn atomic_cmpxchg(
        &self,
        ptr: Val<'cx>,
        cmp: Val<'cx>,
        new: Val<'cx>,
        success: sys::AtomicOrdering,
        failure: sys::AtomicOrdering,
        single_thread: bool,
    ) -> Val<'cx> {
        // SAFETY: as `atomic_rmw`.
        let raw = unsafe {
            sys::build_atomic_cmp_xchg(
                self.raw,
                ptr.raw(),
                cmp.raw(),
                new.raw(),
                success,
                failure,
                single_thread.into(),
            )
        };
        let ty = self
            .tcx()
            .literal_struct(&[cmp.ty(), self.tcx().int(1)], false);
        self.made("atomic_cmpxchg", raw, ty)
    }
}
