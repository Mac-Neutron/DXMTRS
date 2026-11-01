//! Tests of the LLVM layer against the text LLVM 23's C++ builder makes.
//!
//! The expected lines are what `IRBuilder` prints for the same calls
//! (`L23/lib/IR/IRBuilder.cpp`, `L23/include/llvm/IR/IRBuilder.h`): the integer-index overloads
//! make `i64` constants, `CreateConstGEP1_32` an `i32`, `CreateStructGEP` `inbounds nuw 0, <n>`,
//! `CreateVectorSplat` a poison vector and two named instructions.

use core::{ffi::CStr, slice};

#[cfg(feature = "oracle")]
use super::builder::NO_NAME;
use super::{Builder, Context, Func, Module, Ty, TyKind, Val, name, sys};
use crate::api::Error;

impl Val<'_> {
    /// The value's name, any bytes (`Value::getName`).
    #[must_use]
    pub fn name(self) -> Vec<u8> {
        let mut len = 0;
        // SAFETY: a live value; `len` is writable.
        let name = unsafe { sys::get_value_name2(self.raw, &raw mut len) };
        if name.is_null() {
            return Vec::new();
        }
        // SAFETY: LLVM answers `len` bytes at `name`, alive while the value keeps its name.
        unsafe { slice::from_raw_parts(name.cast::<u8>(), len) }.to_vec()
    }

    /// Names the value, any bytes (`Value::setName`; a constant takes no name).
    pub fn set_name(self, name: &[u8]) {
        // SAFETY: a live value; `name` holds its length.
        unsafe { sys::set_value_name2(self.raw, name.as_ptr().cast(), name.len()) };
    }
}

#[cfg(feature = "oracle")]
impl<'cx> Builder<'cx> {
    /// `unreachable`.
    pub fn unreachable(&self) -> Val<'cx> {
        // SAFETY: the builder is live.
        let raw = unsafe { sys::build_unreachable(self.raw) };
        self.made("unreachable", raw, self.tcx().void())
    }

    /// `sdiv`.
    #[must_use]
    pub fn sdiv(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_s_div(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("sdiv", raw, lhs.ty())
    }

    /// `srem`.
    #[must_use]
    pub fn srem(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_s_rem(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("srem", raw, lhs.ty())
    }

    /// `frem`.
    #[must_use]
    pub fn frem(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Val<'cx> {
        // SAFETY: as `add`.
        let raw = unsafe { sys::build_f_rem(self.raw, lhs.raw(), rhs.raw(), NO_NAME.as_ptr()) };
        self.made("frem", raw, lhs.ty())
    }

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

    /// `sitofp`.
    #[must_use]
    pub fn sitofp(&self, v: Val<'cx>, to: Ty) -> Val<'cx> {
        // SAFETY: as `cast`.
        let raw = unsafe {
            sys::build_si_to_fp(self.raw, v.raw(), self.tcx().lower(to), NO_NAME.as_ptr())
        };
        self.made("sitofp", raw, to)
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
}

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

/// A function `f` of `params` named `names`, with the builder at the end of its entry block.
fn function<'cx>(
    cx: &'cx Context,
    m: &Module<'cx>,
    params: &[Ty],
    names: &[&[u8]],
) -> (Builder<'cx>, Vec<Val<'cx>>) {
    let tcx = cx.tcx();
    let f = ok(m.add_function(b"f", tcx.function(tcx.void(), params, false)));
    let entry = f.append_block(cx, c"entry").unwrap();
    let b = Builder::new(cx);
    b.position_at_end(entry);
    let args: Vec<Val<'cx>> = (0..params.len())
        .map(|i| f.param(tcx, u32::try_from(i).unwrap()).unwrap())
        .collect();
    for (arg, n) in args.iter().zip(names) {
        arg.set_name(n);
    }
    (b, args)
}

/// The value's text, named `r`.
fn text(v: Val<'_>) -> String {
    v.set_name(b"r");
    String::from_utf8(v.print()).unwrap().trim().to_owned()
}

fn module_text(m: &Module<'_>) -> String {
    String::from_utf8(m.print()).unwrap()
}

#[test]
fn vector_splat_names_and_poison() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (b, args) = function(&cx, &m, &[tcx.float()], &[b"x"]);
    let splat = b.vector_splat(4, args[0], c"s");
    b.ret_void();
    assert_eq!(splat.ty(), tcx.vector(tcx.float(), 4));
    let text = module_text(&m);
    assert!(
        text.contains("  %s.splatinsert = insertelement <4 x float> poison, float %x, i32 0\n"),
        "{text}"
    );
    assert!(
        text.contains(
            "  %s.splat = shufflevector <4 x float> %s.splatinsert, <4 x float> poison, <4 x i32> zeroinitializer\n"
        ),
        "{text}"
    );
}

#[test]
fn extract_element_u64_is_i64() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (b, args) = function(&cx, &m, &[tcx.vector(tcx.float(), 4)], &[b"v"]);
    let r = b.extract_element_u64(args[0], 2);
    assert_eq!(text(r), "%r = extractelement <4 x float> %v, i64 2");
    assert_eq!(r.ty(), tcx.float());
}

#[test]
fn insert_element_u64_is_i64() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (b, args) = function(
        &cx,
        &m,
        &[tcx.vector(tcx.float(), 4), tcx.float()],
        &[b"v", b"x"],
    );
    let r = b.insert_element_u64(args[0], args[1], 2);
    assert_eq!(
        text(r),
        "%r = insertelement <4 x float> %v, float %x, i64 2"
    );
    assert_eq!(r.ty(), tcx.vector(tcx.float(), 4));
}

#[test]
fn const_gep1_32_is_i32() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (b, args) = function(&cx, &m, &[tcx.ptr(tcx.float(), 0)], &[b"p"]);
    let r = b.const_gep1_32(tcx.float(), args[0], 3);
    assert_eq!(text(r), "%r = getelementptr float, ptr %p, i32 3");
    assert_eq!(r.ty(), tcx.ptr(tcx.float(), 0));
}

#[test]
fn const_gep1_64_is_i64() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (b, args) = function(&cx, &m, &[tcx.ptr(tcx.float(), 0)], &[b"p"]);
    let r = b.const_gep1_64(tcx.float(), args[0], 3);
    assert_eq!(text(r), "%r = getelementptr float, ptr %p, i64 3");
}

#[test]
fn const_inbounds_gep1_32_is_inbounds_i32() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (b, args) = function(&cx, &m, &[tcx.ptr(tcx.float(), 2)], &[b"p"]);
    let r = b.const_inbounds_gep1_32(tcx.float(), args[0], 3);
    assert_eq!(
        text(r),
        "%r = getelementptr inbounds float, ptr addrspace(2) %p, i32 3"
    );
    assert_eq!(r.ty(), tcx.ptr(tcx.float(), 2));
}

#[test]
fn struct_gep_is_inbounds_i32_i32() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let pair = tcx.literal_struct(&[tcx.int(32), tcx.float()], false);
    let (b, args) = function(&cx, &m, &[tcx.ptr(pair, 0)], &[b"s"]);
    let r = b.struct_gep(pair, args[0], 1);
    assert_eq!(
        text(r),
        "%r = getelementptr inbounds nuw { i32, float }, ptr %s, i32 0, i32 1"
    );
    assert_eq!(r.ty(), tcx.ptr(tcx.float(), 0));
}

#[test]
fn get_or_insert_keeps_first_attributes() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let ty = tcx.function(tcx.void(), &[], false);
    let first = ok(m.get_or_insert_function(
        b"air.f",
        ty,
        &[(sys::ATTRIBUTE_FUNCTION_INDEX, b"nounwind")],
    ));
    let again = ok(m.get_or_insert_function(
        b"air.f",
        ty,
        &[(sys::ATTRIBUTE_FUNCTION_INDEX, b"readnone")],
    ));
    assert_eq!(first.val().raw(), again.val().raw());
    let text = module_text(&m);
    assert!(text.contains("declare void @air.f() #0\n"), "{text}");
    assert!(text.contains("attributes #0 = { nounwind }\n"), "{text}");
}

#[test]
fn get_or_insert_other_type_rebinds_callee() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let takes_int = tcx.function(tcx.void(), &[tcx.int(32)], false);
    let takes_float = tcx.function(tcx.void(), &[tcx.float()], false);
    let f = ok(m.get_or_insert_function(b"air.g", takes_int, &[]));
    let cast = ok(m.get_or_insert_function(b"air.g", takes_float, &[]));
    assert!(f.is_function());
    assert!(cast.is_function());
    assert_eq!(cast.fn_ty(), takes_float);
    assert_eq!(cast.val().ty(), tcx.ptr(takes_float, 0));
    let text = String::from_utf8(cast.val().print()).unwrap();
    assert_eq!(text, "declare void @air.g(i32)\n");
    // SAFETY: a live module, a type of its context and a NUL-terminated name.
    let _ = unsafe { sys::add_global(m.raw(), tcx.lower(tcx.int(32)), c"air.h".as_ptr()) };
    let non_fn = ok(m.get_or_insert_function(b"air.h", takes_float, &[]));
    assert!(!non_fn.is_function());
}

#[test]
fn name_with_nul_is_an_error() {
    let error = name(b"a\0b").unwrap_err();
    assert_eq!(error.text(), b"Invalid DXBC bytecode: a name holds a NUL");
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let ty = tcx.function(tcx.void(), &[], false);
    let Err(error) = m.get_or_insert_function(b"air.\0f", ty, &[]) else {
        panic!("a name with a NUL was taken");
    };
    assert_eq!(error.text(), b"Invalid DXBC bytecode: a name holds a NUL");
    // The length-taking form keeps every byte.
    let f = ok(m.add_function(b"g", ty));
    f.val().set_name(b"g\0h\xff");
    assert_eq!(f.val().name(), b"g\0h\xff");
}

#[test]
fn ty_lowers_to_llvm_type_identity() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let c = cx.raw();
    let float = tcx.float();
    let i32_ty = tcx.int(32);
    let expected = |ty: Ty, raw: sys::TypeRef| assert_eq!(tcx.lower(ty), raw, "{:?}", tcx.kind(ty));
    // SAFETY: the context is live and every argument a type of it.
    expected(tcx.void(), unsafe { sys::void_type_in_context(c) });
    // SAFETY: as above.
    expected(tcx.int(1), unsafe { sys::int1_type_in_context(c) });
    // SAFETY: as above.
    expected(i32_ty, unsafe { sys::int32_type_in_context(c) });
    // SAFETY: as above.
    expected(tcx.int(64), unsafe { sys::int64_type_in_context(c) });
    // SAFETY: as above.
    expected(tcx.half(), unsafe { sys::half_type_in_context(c) });
    // SAFETY: as above.
    expected(float, unsafe { sys::float_type_in_context(c) });
    // SAFETY: as above.
    expected(tcx.double(), unsafe { sys::double_type_in_context(c) });
    // SAFETY: as above.
    expected(tcx.label(), unsafe { sys::label_type_in_context(c) });
    // SAFETY: as above.
    expected(tcx.metadata(), unsafe { sys::metadata_type_in_context(c) });
    // SAFETY: as above.
    expected(tcx.vector(float, 4), unsafe {
        sys::vector_type(tcx.lower(float), 4)
    });
    // SAFETY: as above.
    expected(tcx.array(i32_ty, 8), unsafe {
        sys::array_type(tcx.lower(i32_ty), 8)
    });
    // SAFETY: as above.
    expected(tcx.ptr(float, 1), unsafe {
        sys::pointer_type_in_context(c, 1)
    });
    let mut params = [tcx.lower(i32_ty), tcx.lower(float)];
    // SAFETY: as above; `params` holds two types.
    let function = unsafe { sys::function_type(tcx.lower(float), params.as_mut_ptr(), 2, 0) };
    expected(tcx.function(float, &[i32_ty, float], false), function);
    // SAFETY: as above; `params` holds two types.
    let literal = unsafe { sys::struct_type_in_context(c, params.as_mut_ptr(), 2, 0) };
    expected(tcx.literal_struct(&[i32_ty, float], false), literal);
    let named = tcx.named_struct(c"struct._texture_2d_t");
    // SAFETY: the context is live; the name is NUL-terminated.
    expected(named, unsafe {
        sys::get_type_by_name2(c, c"struct._texture_2d_t".as_ptr())
    });
    assert_eq!(tcx.named_struct(c"struct._texture_2d_t"), named);
}

#[test]
fn lift_gives_back_the_same_ty() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let node = tcx.create_struct(c"node", None);
    tcx.set_body(node, &[tcx.ptr(node, 0), tcx.vector(tcx.half(), 2)], false);
    let ty = tcx.function(
        node,
        &[tcx.array(tcx.int(8), 4), tcx.literal_struct(&[], true)],
        true,
    );
    // SAFETY: a type of this context.
    assert_eq!(unsafe { tcx.lift(tcx.lower(ty)) }, Some(ty));
    // A second table over the same context builds every part from LLVM alone, the recursive
    // struct included; each part lowers to the same LLVM type only if it was read right.
    let other = super::TyCx::new(cx.raw());
    // SAFETY: as above.
    let lifted = unsafe { other.lift(tcx.lower(ty)) }.unwrap();
    assert_eq!(other.lower(lifted), tcx.lower(ty));
    let node = other.ret(lifted).unwrap();
    let fields = other.fields(node).unwrap();
    assert_eq!(other.addrspace(fields[0]), Some(0));
    assert_eq!(other.pointee(fields[0]), None);
    assert_eq!(other.lower(node), tcx.lower(tcx.named_struct(c"node")));
}

/// Defines `void @f()` in `m`, its body a `ret void`.
fn defines_f(m: &Module<'_>) {
    let tcx = m.cx().tcx();
    let f: Func<'_> = ok(m.add_function(b"f", tcx.function(tcx.void(), &[], false)));
    let b = Builder::new(m.cx());
    b.position_at_end(f.append_block(m.cx(), c"").unwrap());
    b.ret_void();
}

#[test]
fn diagnostic_handler_catches_link_error() {
    let cx = Context::new();
    let dest = Module::new(&cx, c"dest");
    let src = Module::new(&cx, c"src");
    defines_f(&dest);
    defines_f(&src);
    // SAFETY: two modules of one context; the link takes `src`.
    let failed = unsafe { sys::link_modules2(dest.raw(), src.into_raw()) };
    assert_eq!(failed, 1);
    let error = cx.take_error().unwrap();
    assert_eq!(
        error.text(),
        b"LLVM error: Linking globals named 'f': symbol multiply defined!"
    );
    assert!(cx.take_error().is_none());
}

#[test]
fn diagnostic_warning_does_not_fail() {
    let cx = Context::new();
    let dest = Module::new(&cx, c"dest");
    let src = Module::new(&cx, c"src");
    // SAFETY: live modules; NUL-terminated triples.
    unsafe { sys::set_target(dest.raw(), c"air64-apple-macosx14.0.0".as_ptr()) };
    // SAFETY: as above.
    unsafe { sys::set_target(src.raw(), c"x86_64-apple-macosx14.0.0".as_ptr()) };
    defines_f(&src);
    // SAFETY: two modules of one context; the link takes `src`.
    let failed = unsafe { sys::link_modules2(dest.raw(), src.into_raw()) };
    assert_eq!(failed, 0);
    assert!(cx.take_error().is_none());
    let printed = cx.printed();
    assert_eq!(printed.len(), 1, "{printed:?}");
    let line = String::from_utf8(printed[0].clone()).unwrap();
    assert!(
        line.starts_with("warning: Linking two modules of different target triples: "),
        "{line}"
    );
}

#[test]
fn constant_of_the_wrong_kind_is_undef_and_an_error() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let (float, int) = (tcx.float(), tcx.int(32));
    // The kinds each maker takes still make their constants, with no error.
    assert_eq!(cx.const_int(int, 7, false).print(), b"i32 7");
    assert_eq!(
        cx.const_real(tcx.vector(float, 2), 1.0).print(),
        b"<2 x float> splat (float 1.000000e+00)"
    );
    assert_eq!(cx.all_ones(float).print(), b"float -nan(0x3FFFFF)");
    assert!(cx.take_error().is_none());
    // Another kind: `undef`, and the compile's next check fails it.
    let wrong = |v: Val<'_>| {
        assert!(
            v.print().ends_with(b" undef"),
            "{}",
            String::from_utf8_lossy(&v.print())
        );
        let error = cx.take_error().expect("an error per wrong constant");
        assert_eq!(error.text(), b"LLVM error: a constant of the wrong type");
    };
    wrong(cx.const_int(float, 1, false));
    wrong(cx.const_int(tcx.vector(int, 2), 1, false));
    wrong(cx.const_real(int, 1.0));
    wrong(cx.all_ones(tcx.void()));
    wrong(cx.null(tcx.void()));
}

fn print_raw_type(raw: sys::TypeRef) -> String {
    // SAFETY: `raw` is a live `LLVMTypeRef`.
    let ptr = unsafe { sys::print_type_to_string(raw) };
    // SAFETY: `print_type_to_string` returns a NUL-terminated string.
    let text = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_owned();
    // SAFETY: `ptr` was allocated by `print_type_to_string`.
    unsafe { sys::dispose_message(ptr) };
    text
}

#[test]
fn pointer_lowers_opaque_whatever_its_pointee() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let p_f = tcx.ptr(tcx.float(), 1);
    let p_i = tcx.ptr(tcx.int(32), 1);
    let p_u = tcx.ptr(tcx.unknown(), 1);
    let p_0 = tcx.ptr(tcx.float(), 0);
    assert_eq!(tcx.lower(p_f), tcx.lower(p_i));
    assert_eq!(tcx.lower(p_f), tcx.lower(p_u));
    assert_ne!(tcx.lower(p_f), tcx.lower(p_0));
    assert_eq!(print_raw_type(tcx.lower(p_f)), "ptr addrspace(1)");
    assert_eq!(print_raw_type(tcx.lower(p_0)), "ptr");
}

#[test]
fn typed_lowering_builds_typed_pointers() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let nested = tcx.ptr(tcx.ptr(tcx.float(), 1), 2);
    let raw = ok(tcx.typed(nested));
    assert_eq!(print_raw_type(raw), "typedptr(typedptr(float, 1), 2)");

    let s = tcx.create_struct(c"struct.S", Some((&[tcx.ptr(tcx.int(32), 1)], false)));
    assert_eq!(ok(tcx.typed(s)), tcx.lower(s));

    let p_f = tcx.ptr(tcx.float(), 1);
    let lit = tcx.literal_struct(&[p_f], false);
    assert_eq!(print_raw_type(ok(tcx.typed(lit))), "{ typedptr(float, 1) }");
    let arr = tcx.array(p_f, 3);
    assert_eq!(
        print_raw_type(ok(tcx.typed(arr))),
        "[3 x typedptr(float, 1)]"
    );
    let vec_ty = tcx.vector(p_f, 2);
    assert_eq!(
        print_raw_type(ok(tcx.typed(vec_ty))),
        "<2 x typedptr(float, 1)>"
    );
    let fn_ty = tcx.function(tcx.void(), &[p_f], false);
    assert_eq!(
        print_raw_type(ok(tcx.typed(fn_ty))),
        "void (typedptr(float, 1))"
    );
}

#[test]
fn typed_refuses_an_unknown_pointee() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let err = tcx.typed(tcx.unknown()).unwrap_err();
    assert_eq!(
        err.text(),
        b"AIR writer: a type the writer cannot be told (Unknown)"
    );
    let p_unk = tcx.ptr(tcx.unknown(), 1);
    let err = tcx.typed(p_unk).unwrap_err();
    assert_eq!(
        err.text(),
        b"AIR writer: a type the writer cannot be told (Unknown)"
    );
    let p_void = tcx.ptr(tcx.void(), 0);
    let err = tcx.typed(p_void).unwrap_err();
    assert!(
        err.text()
            .starts_with(b"AIR writer: a type the writer cannot be told ("),
        "{:?}",
        String::from_utf8_lossy(err.text())
    );
}

#[test]
fn lift_of_an_opaque_pointer_has_no_pointee() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let raw_ptr = tcx.lower(tcx.ptr(tcx.float(), 3));
    // SAFETY: a type of this context.
    let lifted = unsafe { tcx.lift(raw_ptr) }.unwrap();
    assert_eq!(
        tcx.kind(lifted),
        TyKind::Ptr {
            pointee: tcx.unknown(),
            addrspace: 3
        }
    );
    assert_eq!(tcx.pointee(lifted), None);
    assert_eq!(tcx.addrspace(lifted), Some(3));
    let u = cx.null(tcx.unknown());
    assert!(u.print().ends_with(b" undef"));
    assert_eq!(
        cx.take_error().unwrap().text(),
        b"LLVM error: a constant of the wrong type"
    );
}

#[test]
fn lift_keeps_identified_structs() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let f_ptr = tcx.ptr(tcx.float(), 1);
    let s = tcx.create_struct(c"struct.my_buf", Some((&[f_ptr, tcx.int(32)], false)));
    assert_eq!(tcx.named_struct(c"struct.my_buf"), s);
    let raw_s = tcx.lower(s);
    // SAFETY: a type of this context.
    let lifted = unsafe { tcx.lift(raw_s) }.unwrap();
    assert_eq!(lifted, s);
    assert_eq!(
        tcx.fields(lifted).as_deref(),
        Some(&[f_ptr, tcx.int(32)][..])
    );
    let fn_ty = tcx.function(tcx.void(), &[s], false);
    // SAFETY: a type of this context.
    let lifted_fn = unsafe { tcx.lift(tcx.lower(fn_ty)) }.unwrap();
    assert_eq!(tcx.params(lifted_fn).as_deref(), Some(&[s][..]));
}

#[test]
fn air_writer_mirror_layouts() {
    use std::mem::{offset_of, size_of};
    assert_eq!(size_of::<sys::AirValueType>(), 16);
    assert_eq!(offset_of!(sys::AirValueType, value), 0);
    assert_eq!(offset_of!(sys::AirValueType, ty), 8);

    assert_eq!(size_of::<sys::AirStructBody>(), 16);
    assert_eq!(offset_of!(sys::AirStructBody, struct_ty), 0);
    assert_eq!(offset_of!(sys::AirStructBody, body), 8);

    assert_eq!(size_of::<sys::AirTypes>(), 48);
    assert_eq!(offset_of!(sys::AirTypes, values), 0);
    assert_eq!(offset_of!(sys::AirTypes, num_values), 8);
    assert_eq!(offset_of!(sys::AirTypes, structs), 16);
    assert_eq!(offset_of!(sys::AirTypes, num_structs), 24);
    assert_eq!(offset_of!(sys::AirTypes, typed_refs), 32);
    assert_eq!(offset_of!(sys::AirTypes, num_typed_refs), 40);

    assert_eq!(size_of::<sys::AirWriteStats>(), 24);
    assert_eq!(offset_of!(sys::AirWriteStats, inst_casts), 0);
    assert_eq!(offset_of!(sys::AirWriteStats, const_casts), 4);
    assert_eq!(offset_of!(sys::AirWriteStats, use_typed), 8);
    assert_eq!(offset_of!(sys::AirWriteStats, attrs_dropped), 12);
    assert_eq!(offset_of!(sys::AirWriteStats, flags_dropped), 16);
    assert_eq!(offset_of!(sys::AirWriteStats, refs_filled), 20);
}

#[test]
fn attribute_tables_round_trip_to_the_cxx() {
    use std::collections::BTreeMap;
    const FN: sys::AttributeIndex = sys::ATTRIBUTE_FUNCTION_INDEX;
    type Attrs = &'static [(sys::AttributeIndex, &'static [u8])];

    struct Row<'a> {
        name: &'static [u8],
        ty: Ty,
        attrs: Attrs,
        sig: &'a str,
        pre_fn_attrs: &'a str,
        post_fn_attrs: &'a str,
    }

    // Every `Attrs` constant in `air/builder.rs` and every inline attribute table in `dxbc/` and
    // `dxil/`, paired with the declaration's pre-pass attribute set (LLVM 23's reader upgrade of
    // the table itself, matching `$W/in/helpers/air_{msad,samplepos,tessellation}.air`) and its
    // post-pass attribute set (matching `$W/llvm23/bin/llvm-dis` of the C++ baseline metallib
    // declaring the same `air.*` / `dxmt.*` name).
    const ATOMIC: Attrs = &[(1, b"nocapture"), (FN, b"nounwind"), (FN, b"willreturn")];
    const SAMPLE: Attrs = &[
        (1, b"nocapture"),
        (1, b"readonly"),
        (2, b"nocapture"),
        (2, b"readonly"),
        (FN, b"convergent"),
        (FN, b"nounwind"),
        (FN, b"willreturn"),
        (FN, b"argmemonly"),
        (FN, b"readonly"),
    ];
    const SAMPLE_EXPLICIT: Attrs = &[
        (1, b"nocapture"),
        (1, b"readonly"),
        (2, b"nocapture"),
        (2, b"readonly"),
        (FN, b"nounwind"),
        (FN, b"willreturn"),
        (FN, b"argmemonly"),
        (FN, b"readonly"),
    ];
    const READ: Attrs = &[
        (1, b"nocapture"),
        (1, b"readonly"),
        (FN, b"nounwind"),
        (FN, b"willreturn"),
        (FN, b"argmemonly"),
        (FN, b"readonly"),
    ];
    const WRITE: Attrs = &[
        (1, b"nocapture"),
        (FN, b"nounwind"),
        (FN, b"willreturn"),
        (FN, b"argmemonly"),
    ];
    const TEXTURE_FENCE: Attrs = &[
        (1, b"nocapture"),
        (FN, b"nounwind"),
        (FN, b"willreturn"),
        (FN, b"mustprogress"),
    ];
    const PURE: Attrs = &[(FN, b"nounwind"), (FN, b"readnone"), (FN, b"willreturn")];
    const PLAIN: Attrs = &[(FN, b"nounwind"), (FN, b"willreturn")];
    const BARRIER: Attrs = &[(FN, b"convergent"), (FN, b"nounwind"), (FN, b"willreturn")];
    const COHERENT_LOAD: Attrs = &[
        (1, b"nocapture"),
        (1, b"readonly"),
        (FN, b"argmemonly"),
        (FN, b"nofree"),
        (FN, b"nosync"),
        (FN, b"nounwind"),
        (FN, b"readonly"),
        (FN, b"willreturn"),
    ];
    const COHERENT_STORE: Attrs = &[
        (2, b"nocapture"),
        (2, b"writeonly"),
        (FN, b"argmemonly"),
        (FN, b"nofree"),
        (FN, b"nosync"),
        (FN, b"nounwind"),
        (FN, b"willreturn"),
    ];
    const INTERPOLATE_SAMPLE: Attrs = &[
        (1, b"nocapture"),
        (1, b"readonly"),
        (FN, b"nounwind"),
        (FN, b"willreturn"),
        (FN, b"argmemonly"),
    ];
    const MESH_PROPERTIES: Attrs = &[
        (1, b"nocapture"),
        (FN, b"nounwind"),
        (FN, b"willreturn"),
        (FN, b"argmemonly"),
        (FN, b"mustprogress"),
    ];
    const DXBC_ATOMIC_FENCE: Attrs = &[
        (FN, b"nounwind"),
        (FN, b"willreturn"),
        (FN, b"mustprogress"),
    ];
    const DXBC_MSAD_SAMPLEPOS: Attrs = &[(FN, b"nounwind"), (FN, b"readnone")];
    const DXBC_HULL_DOMAIN_PURE: Attrs =
        &[(FN, b"nounwind"), (FN, b"willreturn"), (FN, b"readnone")];
    const DXBC_DOMAIN_GEN_PRIM: Attrs = &[(FN, b"nounwind"), (FN, b"willreturn")];
    const DXBC_UNPACK: Attrs = &[
        (FN, b"nounwind"),
        (FN, b"readnone"),
        (FN, b"willreturn"),
        (FN, b"mustprogress"),
        (FN, b"nofree"),
        (FN, b"nosync"),
    ];
    const DXIL_SIMD: Attrs = &[
        (FN, b"nounwind"),
        (FN, b"willreturn"),
        (FN, b"convergent"),
        (FN, b"mustprogress"),
    ];

    let cx = Context::new();
    let tcx = cx.tcx();
    let m = Module::new(&cx, c"attrs");
    let void = tcx.void();
    let i1_ty = tcx.int(1);
    let i8_ty = tcx.int(8);
    let i32_ty = tcx.int(32);
    let f32_ty = tcx.float();
    let vec2_int = tcx.vector(i32_ty, 2);
    let vec4_int = tcx.vector(i32_ty, 4);
    let vec2_float = tcx.vector(f32_ty, 2);
    let vec4_float = tcx.vector(f32_ty, 4);
    let p1 = tcx.ptr(i32_ty, 1);
    let p2 = tcx.ptr(i32_ty, 2);
    let p7 = tcx.ptr(i32_ty, 7);
    let sample_ret = tcx.literal_struct(&[vec4_float, i8_ty], false);
    let read_ret = tcx.literal_struct(&[vec4_int, i8_ty], false);

    let rows = [
        // C2.out/atomics.dxil.metallib
        Row {
            name: b"air.atomic.global.add.u.i32",
            ty: tcx.function(i32_ty, &[p1, i32_ty, i32_ty, i32_ty, i1_ty], false),
            attrs: ATOMIC,
            sig: "i32 @air.atomic.global.add.u.i32(ptr addrspace(1) captures(none), i32, i32, i32, i1)",
            pre_fn_attrs: "nounwind willreturn",
            post_fn_attrs: "mustprogress nounwind willreturn",
        },
        // C1.out/lod.ps.dxil.metallib
        Row {
            name: b"air.sample_texture_2d.v4f32",
            ty: tcx.function(
                sample_ret,
                &[
                    p1, p2, vec2_float, i1_ty, vec2_int, i1_ty, f32_ty, f32_ty, i32_ty,
                ],
                false,
            ),
            attrs: SAMPLE,
            sig: "{ <4 x float>, i8 } @air.sample_texture_2d.v4f32(ptr addrspace(1) readonly captures(none), ptr addrspace(2) readonly captures(none), <2 x float>, i1, <2 x i32>, i1, float, float, i32)",
            pre_fn_attrs: "convergent nounwind willreturn memory(argmem: read)",
            post_fn_attrs: "convergent mustprogress nofree nounwind willreturn memory(argmem: read)",
        },
        // C1.out/lod.ps.dxil.metallib
        Row {
            name: b"air.sample_texture_2d_grad.v4f32",
            ty: tcx.function(
                sample_ret,
                &[
                    p1, p2, vec2_float, vec2_float, vec2_float, f32_ty, i1_ty, vec2_int, i32_ty,
                ],
                false,
            ),
            attrs: SAMPLE_EXPLICIT,
            sig: "{ <4 x float>, i8 } @air.sample_texture_2d_grad.v4f32(ptr addrspace(1) readonly captures(none), ptr addrspace(2) readonly captures(none), <2 x float>, <2 x float>, <2 x float>, float, i1, <2 x i32>, i32)",
            pre_fn_attrs: "nounwind willreturn memory(argmem: read)",
            post_fn_attrs: "mustprogress nofree nounwind willreturn memory(argmem: read)",
        },
        // C2.out/atomics.dxil.metallib
        Row {
            name: b"air.read_texture_buffer_1d.u.v4i32",
            ty: tcx.function(read_ret, &[p1, i32_ty, i32_ty], false),
            attrs: READ,
            sig: "{ <4 x i32>, i8 } @air.read_texture_buffer_1d.u.v4i32(ptr addrspace(1) readonly captures(none), i32, i32)",
            pre_fn_attrs: "nounwind willreturn memory(argmem: read)",
            post_fn_attrs: "mustprogress nofree nounwind willreturn memory(argmem: read)",
        },
        // C1.out/null.cs.dxil.metallib
        Row {
            name: b"air.write_texture_1d.u.v4i32",
            ty: tcx.function(void, &[p1, i32_ty, vec4_int, i32_ty], false),
            attrs: WRITE,
            sig: "void @air.write_texture_1d.u.v4i32(ptr addrspace(1) captures(none), i32, <4 x i32>, i32)",
            pre_fn_attrs: "nounwind willreturn memory(argmem: readwrite)",
            post_fn_attrs: "mustprogress nounwind willreturn memory(argmem: readwrite)",
        },
        // C2.out/atomics.dxil.metallib
        Row {
            name: b"air.fence_texture_buffer_1d",
            ty: tcx.function(void, &[p1], false),
            attrs: TEXTURE_FENCE,
            sig: "void @air.fence_texture_buffer_1d(ptr addrspace(1) captures(none))",
            pre_fn_attrs: "mustprogress nounwind willreturn",
            post_fn_attrs: "mustprogress nounwind willreturn",
        },
        // C1.out/lod.ps.dxil.metallib
        Row {
            name: b"air.exp2.f32",
            ty: tcx.function(f32_ty, &[f32_ty], false),
            attrs: PURE,
            sig: "float @air.exp2.f32(float)",
            pre_fn_attrs: "nounwind willreturn memory(none)",
            post_fn_attrs: "mustprogress nofree nosync nounwind willreturn memory(none)",
        },
        // C1.out/lod.ps.dxil.metallib
        Row {
            name: b"air.dfdx.f32",
            ty: tcx.function(f32_ty, &[f32_ty], false),
            attrs: PLAIN,
            sig: "float @air.dfdx.f32(float)",
            pre_fn_attrs: "nounwind willreturn",
            post_fn_attrs: "mustprogress nounwind willreturn",
        },
        // C2.out/atomics.dxil.metallib
        Row {
            name: b"air.wg.barrier",
            ty: tcx.function(void, &[i32_ty, i32_ty], false),
            attrs: BARRIER,
            sig: "void @air.wg.barrier(i32, i32)",
            pre_fn_attrs: "convergent nounwind willreturn",
            post_fn_attrs: "convergent mustprogress nounwind willreturn",
        },
        // C8p.out/845.metallib
        Row {
            name: b"air.load.device.coherent.i32.p1i32",
            ty: tcx.function(i32_ty, &[p1], false),
            attrs: COHERENT_LOAD,
            sig: "i32 @air.load.device.coherent.i32.p1i32(ptr addrspace(1) readonly captures(none))",
            pre_fn_attrs: "nofree nosync nounwind willreturn memory(argmem: read)",
            post_fn_attrs: "mustprogress nofree nosync nounwind willreturn memory(argmem: read)",
        },
        // C11.out/cs-386b45646daa96df.dxil.metallib
        Row {
            name: b"air.store.device.coherent.i32.p1i32",
            ty: tcx.function(void, &[i32_ty, p1], false),
            attrs: COHERENT_STORE,
            sig: "void @air.store.device.coherent.i32.p1i32(i32, ptr addrspace(1) writeonly captures(none))",
            pre_fn_attrs: "nofree nosync nounwind willreturn memory(argmem: readwrite)",
            post_fn_attrs: "mustprogress nofree nosync nounwind willreturn memory(argmem: readwrite)",
        },
        // C1.out/cmp68.ps.dxil.metallib
        Row {
            name: b"air.interpolate_at_sample.v2f32",
            ty: tcx.function(vec2_float, &[p1, i32_ty, i1_ty], false),
            attrs: INTERPOLATE_SAMPLE,
            sig: "<2 x float> @air.interpolate_at_sample.v2f32(ptr addrspace(1) readonly captures(none), i32, i1)",
            pre_fn_attrs: "nounwind willreturn memory(argmem: readwrite)",
            post_fn_attrs: "mustprogress nounwind willreturn memory(argmem: readwrite)",
        },
        // C1.out/triangle2.gs.dxil.metallib
        Row {
            name: b"air.set_mesh_properties",
            ty: tcx.function(void, &[p7, i32_ty], false),
            attrs: MESH_PROPERTIES,
            sig: "void @air.set_mesh_properties(ptr addrspace(7) captures(none), i32)",
            pre_fn_attrs: "mustprogress nounwind willreturn memory(argmem: readwrite)",
            post_fn_attrs: "mustprogress nounwind willreturn memory(argmem: readwrite)",
        },
        // C2.out/atomics.dxil.metallib (dxbc ATOMIC_FENCE & dxil fence)
        Row {
            name: b"air.atomic.fence",
            ty: tcx.function(void, &[i32_ty, i32_ty, i32_ty], false),
            attrs: DXBC_ATOMIC_FENCE,
            sig: "void @air.atomic.fence(i32, i32, i32)",
            pre_fn_attrs: "mustprogress nounwind willreturn",
            post_fn_attrs: "mustprogress nounwind willreturn",
        },
        // dxbc call_msad & replay sample_pos
        Row {
            name: b"dxmt.sample_pos.i32",
            ty: tcx.function(vec2_float, &[i32_ty, i32_ty], false),
            attrs: DXBC_MSAD_SAMPLEPOS,
            sig: "<2 x float> @dxmt.sample_pos.i32(i32, i32)",
            pre_fn_attrs: "nounwind memory(none)",
            post_fn_attrs: "nofree nosync nounwind memory(none)",
        },
        // $W/in/helpers/air_msad.air (#5) & C6-320.out/tessl.hs.dxbc#i0t64.1.metallib (#0)
        Row {
            name: b"dxmt.msad.i32",
            ty: tcx.function(i32_ty, &[i32_ty, vec2_int, i32_ty], false),
            attrs: DXBC_HULL_DOMAIN_PURE,
            sig: "i32 @dxmt.msad.i32(i32, <2 x i32>, i32)",
            pre_fn_attrs: "nounwind willreturn memory(none)",
            post_fn_attrs: "mustprogress nofree nosync nounwind willreturn memory(none)",
        },
        // $W/in/helpers/air_tessellation.air (#18) & C6-320.out/tessl.hs.dxbc#i0t64.1.metallib (#1)
        Row {
            name: b"dxmt.domain_generate_primitives.tri.even",
            ty: tcx.function(void, &[p7, i32_ty], false),
            attrs: DXBC_DOMAIN_GEN_PRIM,
            sig: "void @dxmt.domain_generate_primitives.tri.even(ptr addrspace(7), i32)",
            pre_fn_attrs: "nounwind willreturn",
            post_fn_attrs: "mustprogress nounwind willreturn",
        },
        // C5-320.out/patch.vs.dxbc#fmt39.metallib
        Row {
            name: b"air.unpack.snorm4x8.v4f32",
            ty: tcx.function(vec4_float, &[i32_ty], false),
            attrs: DXBC_UNPACK,
            sig: "<4 x float> @air.unpack.snorm4x8.v4f32(i32)",
            pre_fn_attrs: "mustprogress nofree nosync nounwind willreturn memory(none)",
            post_fn_attrs: "mustprogress nofree nosync nounwind willreturn memory(none)",
        },
        // C1.out/helper.ps.dxil.metallib
        Row {
            name: b"air.simd_ballot",
            ty: tcx.function(vec4_int, &[i1_ty], false),
            attrs: DXIL_SIMD,
            sig: "<4 x i32> @air.simd_ballot(i1)",
            pre_fn_attrs: "convergent mustprogress nounwind willreturn",
            post_fn_attrs: "convergent mustprogress nounwind willreturn",
        },
    ];

    let caller = ok(m.add_function(b"caller", tcx.function(void, &[], false)));
    let b = Builder::new(&cx);
    b.position_at_end(caller.append_block(&cx, c"entry").unwrap());
    for (i, row) in rows.iter().enumerate() {
        let f = ok(m.get_or_insert_function(row.name, row.ty, row.attrs));
        let args: Vec<_> = tcx
            .params(row.ty)
            .unwrap()
            .iter()
            .map(|&p| cx.null(p))
            .collect();
        let ret = b.call(f, &args);
        if ret.ty() != void {
            let sink_name = std::ffi::CString::new(format!("sink{i}")).unwrap();
            // SAFETY: a live module, a type of its context and a NUL-terminated name.
            let raw = unsafe { sys::add_global(m.raw(), tcx.lower(ret.ty()), sink_name.as_ptr()) };
            // SAFETY: a global of this context, a pointer to `ret`'s type in address space 0.
            let global = unsafe { cx.val(raw, tcx.ptr(ret.ty(), 0)) };
            b.store(ret, global);
        }
    }
    b.ret_void();

    let parse_decls =
        |text: &str| -> (BTreeMap<String, (String, String)>, BTreeMap<String, String>) {
            let mut decls = BTreeMap::new();
            let mut groups = BTreeMap::new();
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("declare ") {
                    let (before_hash, group) = rest.rsplit_once(" #").expect(line);
                    let sig = before_hash
                        .strip_suffix(" local_unnamed_addr")
                        .unwrap_or(before_hash);
                    let at = sig.find('@').unwrap();
                    let paren = sig[at..].find('(').unwrap() + at;
                    let name = &sig[at + 1..paren];
                    decls.insert(name.to_owned(), (sig.to_owned(), group.to_owned()));
                } else if let Some(rest) = line.strip_prefix("attributes #") {
                    let (id, body) = rest.split_once(" = { ").expect(line);
                    let body = body.strip_suffix(" }").expect(line);
                    groups.insert(id.to_owned(), body.to_owned());
                }
            }
            (decls, groups)
        };

    let verify_ok = |m: &Module<'_>| {
        // SAFETY: a live module; no message out-pointer needed.
        let status = unsafe {
            sys::verify_module(m.raw(), sys::RETURN_STATUS_ACTION, core::ptr::null_mut())
        };
        assert_eq!(status, 0);
    };
    verify_ok(&m);
    let pre_text = String::from_utf8(m.print()).unwrap();
    let (pre_decls, pre_groups) = parse_decls(&pre_text);
    for row in &rows {
        let name = std::str::from_utf8(row.name).unwrap();
        let (sig, grp) = pre_decls
            .get(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(sig, row.sig, "pre-pass sig for {name}");
        assert_eq!(
            pre_groups.get(grp).map(String::as_str),
            Some(row.pre_fn_attrs),
            "pre-pass attrs for {name}"
        );
    }

    crate::module::optimize(&m);
    verify_ok(&m);
    let post_text = String::from_utf8(m.print()).unwrap();
    let (post_decls, post_groups) = parse_decls(&post_text);
    for row in &rows {
        let name = std::str::from_utf8(row.name).unwrap();
        let (sig, grp) = post_decls
            .get(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(sig, row.sig, "post-pass sig for {name}");
        assert_eq!(
            post_groups.get(grp).map(String::as_str),
            Some(row.post_fn_attrs),
            "post-pass attrs for {name}"
        );
    }
}

#[test]
fn unknown_attribute_name_is_an_error() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = Module::new(&cx, c"m");
    let fn_ty = tcx.function(tcx.void(), &[], false);
    let err = m
        .get_or_insert_function(
            b"air.bad",
            fn_ty,
            &[(sys::ATTRIBUTE_FUNCTION_INDEX, b"nosuchattr")],
        )
        .err()
        .expect("unknown attribute must fail");
    assert_eq!(err.text(), b"LLVM error: no attribute nosuchattr");
    // SAFETY: live module; NUL-terminated name.
    let f = unsafe { sys::get_named_function(m.raw(), c"air.bad".as_ptr()) };
    assert!(f.is_null());
}
