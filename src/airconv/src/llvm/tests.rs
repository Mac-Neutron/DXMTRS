//! Tests of the LLVM layer against the text LLVM 15's C++ builder makes.
//!
//! The expected lines are what `IRBuilder` prints for the same calls
//! (`L15/lib/IR/IRBuilder.cpp:1149-1161`, `L15/include/llvm/IR/IRBuilder.h:2284-2309`): the
//! integer-index overloads make `i64` constants, `CreateConstGEP1_32` an `i32`, `CreateStructGEP`
//! `inbounds i32 0, i32 <n>`, `CreateVectorSplat` a poison vector and two named instructions.

use core::slice;

#[cfg(feature = "oracle")]
use super::builder::NO_NAME;
use super::{Builder, Context, Func, Module, Ty, Val, name, sys, take_message};
use crate::api::Error;

impl Module<'_> {
    /// The module's text (`Module::print`).
    #[must_use]
    pub fn print(&self) -> Vec<u8> {
        // SAFETY: the module is live; the answer is a message disposed of below.
        let text = unsafe { sys::print_module_to_string(self.raw) };
        // SAFETY: the message just made.
        unsafe { take_message(text) }
    }
}

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

impl<'cx> Builder<'cx> {
    /// `unreachable`.
    pub fn unreachable(&self) -> Val<'cx> {
        // SAFETY: the builder is live.
        let raw = unsafe { sys::build_unreachable(self.raw) };
        self.made("unreachable", raw, self.tcx().void())
    }
}

#[cfg(feature = "oracle")]
impl<'cx> Builder<'cx> {
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
    assert_eq!(text(r), "%r = getelementptr float, float* %p, i32 3");
    assert_eq!(r.ty(), tcx.ptr(tcx.float(), 0));
}

#[test]
fn const_gep1_64_is_i64() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (b, args) = function(&cx, &m, &[tcx.ptr(tcx.float(), 0)], &[b"p"]);
    let r = b.const_gep1_64(tcx.float(), args[0], 3);
    assert_eq!(text(r), "%r = getelementptr float, float* %p, i64 3");
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
        "%r = getelementptr inbounds float, float addrspace(2)* %p, i32 3"
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
        "%r = getelementptr inbounds { i32, float }, { i32, float }* %s, i32 0, i32 1"
    );
    assert_eq!(r.ty(), tcx.ptr(tcx.float(), 0));
}

#[test]
fn get_or_insert_keeps_first_attributes() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let ty = tcx.function(tcx.void(), &[], false);
    let nounwind = cx.enum_attribute(b"nounwind", 0).unwrap();
    let readnone = cx.enum_attribute(b"readnone", 0).unwrap();
    let first =
        ok(m.get_or_insert_function(b"air.f", ty, &[(sys::ATTRIBUTE_FUNCTION_INDEX, nounwind)]));
    let again =
        ok(m.get_or_insert_function(b"air.f", ty, &[(sys::ATTRIBUTE_FUNCTION_INDEX, readnone)]));
    assert_eq!(first.val().raw(), again.val().raw());
    let text = module_text(&m);
    assert!(text.contains("declare void @air.f() #0\n"), "{text}");
    assert!(text.contains("attributes #0 = { nounwind }\n"), "{text}");
}

#[test]
fn get_or_insert_other_type_is_a_bitcast() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let takes_int = tcx.function(tcx.void(), &[tcx.int(32)], false);
    let takes_float = tcx.function(tcx.void(), &[tcx.float()], false);
    let f = ok(m.get_or_insert_function(b"air.g", takes_int, &[]));
    let cast = ok(m.get_or_insert_function(b"air.g", takes_float, &[]));
    assert!(f.is_function());
    assert!(!cast.is_function());
    assert_eq!(cast.fn_ty(), takes_float);
    assert_eq!(cast.val().ty(), tcx.ptr(takes_float, 0));
    let text = String::from_utf8(cast.val().print()).unwrap();
    assert_eq!(
        text,
        "void (float)* bitcast (void (i32)* @air.g to void (float)*)"
    );
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
        sys::pointer_type(tcx.lower(float), 1)
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
        tcx.ptr(node, 3),
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
    let ret = other.ret(lifted).unwrap();
    assert_eq!(other.addrspace(ret), Some(3));
    let node = other.pointee(ret).unwrap();
    let fields = other.fields(node).unwrap();
    assert_eq!(other.pointee(fields[0]), Some(node));
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
        b"<2 x float> <float 1.000000e+00, float 1.000000e+00>"
    );
    assert_eq!(cx.all_ones(float).print(), b"float 0xFFFFFFFFE0000000");
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
