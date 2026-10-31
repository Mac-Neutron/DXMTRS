//! Tests of the type oracle: the creation check through the builder.

use crate::{
    api::Error,
    llvm::{Builder, Context, Func, Module, Ty, Val, sys},
};

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

/// A function `name` of `params` named `names`, with the builder at the end of its entry block.
fn function<'cx>(
    cx: &'cx Context,
    m: &Module<'cx>,
    ret: Ty,
    params: &[Ty],
    names: &[&[u8]],
) -> (Builder<'cx>, Vec<Val<'cx>>, Func<'cx>) {
    let tcx = cx.tcx();
    let f = ok(m.add_function(b"f", tcx.function(ret, params, false)));
    let entry = f.append_block(cx, c"entry").unwrap();
    let b = Builder::new(cx);
    b.position_at_end(entry);
    let args: Vec<Val<'cx>> = (0..params.len())
        .map(|i| f.param(tcx, u32::try_from(i).unwrap()).unwrap())
        .collect();
    for (arg, n) in args.iter().zip(names) {
        arg.set_name(n);
    }
    (b, args, f)
}

#[test]
fn creation_check_fails_the_compile() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (b, args, _) = function(&cx, &m, tcx.void(), &[tcx.int(32)], &[b"x"]);
    // SAFETY: breaks `val`'s contract on purpose (an i32 said to be an i64), which is what the
    // oracle catches; the value is only handed to the builder.
    let wrong = unsafe { cx.val(args[0].raw(), tcx.int(64)) };
    let _ = b.add(wrong, wrong);
    let _ = b.sub(wrong, wrong);
    let _ = b.add(args[0], args[0]);
    b.ret_void();
    let Some(error) = cx.take_error() else {
        panic!("a wrong Ty passed")
    };
    assert_eq!(
        String::from_utf8_lossy(error.text()),
        "LLVM error: oracle: add made i32, Ty says i64"
    );
    assert!(cx.take_error().is_none());
}

#[test]
fn every_builder_method_agrees_with_llvm() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (i1, i32, i64, f32) = (tcx.int(1), tcx.int(32), tcx.int(64), tcx.float());
    let (v4, vi4, pf) = (tcx.vector(f32, 4), tcx.vector(i32, 4), tcx.ptr(f32, 0));
    let agg = tcx.literal_struct(&[i32, f32], false);
    let params = [i32, i32, f32, f32, v4, vi4, pf, i1, agg, tcx.ptr(i32, 1)];
    let names: [&[u8]; 10] = [b"a", b"b", b"f", b"g", b"v", b"w", b"p", b"c", b"s", b"q"];
    let (b, args, func) = function(&cx, &m, tcx.void(), &params, &names);
    let [ia, ib, fx, fy, vf, vi, ptr, cond, pair, global] = args[..] else {
        panic!("ten parameters")
    };
    for op in [
        Builder::add,
        Builder::sub,
        Builder::mul,
        Builder::udiv,
        Builder::sdiv,
        Builder::urem,
        Builder::srem,
        Builder::shl,
        Builder::lshr,
        Builder::ashr,
        Builder::and,
        Builder::or,
        Builder::xor,
    ] {
        let _ = op(&b, ia, ib);
    }
    for op in [
        Builder::fadd,
        Builder::fsub,
        Builder::fmul,
        Builder::fdiv,
        Builder::frem,
    ] {
        let _ = op(&b, fx, fy);
    }
    let _ = (b.binop(sys::ADD, ia, ib), b.neg(ia), b.fneg(fx), b.not(ia));
    let _ = (b.cast(sys::Z_EXT, ia, i64), b.trunc(ia, tcx.int(16)));
    let _ = (b.zext(ia, i64), b.sext(ia, i64), b.fptrunc(fx, tcx.half()));
    let _ = (
        b.fpext(fx, tcx.double()),
        b.fptoui(fx, i32),
        b.fptosi(fx, i32),
    );
    let _ = (b.uitofp(ia, f32), b.sitofp(ia, f32), b.ptrtoint(ptr, i64));
    let _ = (
        b.inttoptr(b.zext(ia, i64), pf),
        b.bitcast(ptr, tcx.ptr(i32, 0)),
    );
    let _ = (
        b.addrspacecast(ptr, tcx.ptr(f32, 1)),
        b.pointer_cast(ptr, tcx.ptr(i64, 0)),
    );
    let _ = (b.fp_cast(fx, tcx.double()), b.int_cast(ia, i64, true));
    let _ = (b.zext_or_trunc(ia, i64), b.bit_or_pointer_cast(ptr, i64));
    let _ = (b.icmp(sys::INT_EQ, ia, ib), b.icmp(sys::INT_SLT, vi, vi));
    let _ = (b.fcmp(sys::REAL_OEQ, fx, fy), b.fcmp(sys::REAL_OEQ, vf, vf));
    let _ = (b.is_null(ptr), b.is_not_null(ptr), b.is_not_neg(ia));
    let _ = (b.logical_and(cond, cond), b.select(cond, ia, ib));
    let slot = b.alloca(agg, c"slot");
    let _ = (b.load(f32, ptr), b.aligned_load(f32, ptr, 4));
    let _ = (b.store(fx, ptr), b.aligned_store(fx, ptr, 4));
    let zero = cx.const_int(i32, 0, false);
    let _ = (
        b.gep(f32, ptr, &[ia]),
        b.inbounds_gep(agg, slot, &[zero, zero]),
    );
    let _ = (b.struct_gep(agg, slot, 1), b.const_gep1_32(f32, ptr, 1));
    let _ = (
        b.const_gep1_64(f32, ptr, 1),
        b.const_inbounds_gep1_32(f32, ptr, 1),
    );
    let _ = (b.extract_element(vf, ia), b.extract_element_u64(vf, 1));
    let _ = (
        b.insert_element(vf, fx, ia),
        b.insert_element_u64(vf, fx, 1),
    );
    let _ = (
        b.shuffle_vector(vf, vf, &[0, 5, -1]),
        b.shuffle_vector1(vf, &[1, 0]),
    );
    let _ = (
        b.vector_splat(4, fx, c"s"),
        b.extract_value(pair, 1),
        b.insert_value(pair, fx, 1),
    );
    let callee = ok(m.add_function(b"g", tcx.function(f32, &[f32], false)));
    let _ = (
        b.call(callee, &[fx]),
        b.binary_intrinsic(b"llvm.umax", ia, ib).unwrap(),
    );
    let add = sys::ATOMIC_RMW_BIN_OP_ADD;
    let order = sys::ATOMIC_ORDERING_MONOTONIC;
    let _ = b.atomic_rmw(add, global, ia, order, false);
    let _ = b.atomic_cmpxchg(global, ia, ib, order, order, false);
    let [next, cases, end, dead] =
        [c"next", c"cases", c"end", c"dead"].map(|n| func.append_block(&cx, n).unwrap());
    b.br(next);
    b.position_at_end(next);
    let _ = b.phi(f32);
    b.cond_br(cond, cases, end);
    b.position_at_end(cases);
    let switch = b.switch(ia, end, 1);
    b.add_case(switch, zero, dead);
    b.position_at_end(dead);
    b.unreachable();
    b.position_at_end(end);
    b.ret_void();
    let (b2, args2, _) = function(&cx, &m, f32, &[f32], &[b"x"]);
    b2.ret(args2[0]);
    if let Some(error) = cx.take_error() {
        panic!("{}", String::from_utf8_lossy(error.text()));
    }
}
