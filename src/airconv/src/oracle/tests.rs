//! Tests of the type oracle.
//!
//! The creation check through the builder, the derivation on modules built here, and the
//! derivation over every module of the frozen C++ baseline.

use std::{
    env, fs,
    path::{Path, PathBuf},
    ptr,
};

use super::{derive, untyped};
use crate::{
    api::Error,
    llvm::{Builder, Context, Func, Module, Ty, Val, sys},
};

unsafe extern "C" {
    /// `LLVMParseBitcodeInContext2` (`llvm-c/BitReader.h`): the module in `buf`, read whole.
    ///
    /// Declared here, for the baseline test only, until `llvm/sys.rs` declares it or L3's
    /// `airconv_shim_parse_bitcode` reads bitcode; both parse through `parseBitcodeFile`.
    #[link_name = "LLVMParseBitcodeInContext2"]
    fn parse_bitcode_in_context2(
        cx: sys::ContextRef,
        buf: sys::MemoryBufferRef,
        out: *mut sys::ModuleRef,
    ) -> sys::Bool;
}

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

/// The lines as text.
fn text(lines: Vec<Vec<u8>>) -> Vec<String> {
    lines
        .into_iter()
        .map(|l| String::from_utf8(l).unwrap())
        .collect()
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
fn derive_types_a_gep_chain() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (f32, i64) = (tcx.float(), tcx.int(64));
    let row = tcx.array(f32, 4);
    let s = tcx.literal_struct(&[tcx.int(32), row], false);
    let (b, args, _) = function(&cx, &m, tcx.void(), &[tcx.ptr(s, 1)], &[b"s"]);
    // The argument is typed by the GEP over it, each GEP's result by its indexed type.
    let field = b.struct_gep(s, args[0], 1);
    let index = [cx.const_int(i64, 0, false), cx.const_int(i64, 2, false)];
    let element = b.inbounds_gep(row, field, &index);
    let slot = b.alloca(s, c"slot");
    let next = b.const_gep1_32(s, slot, 1);
    let inner = b.struct_gep(s, next, 1);
    let _ = b.load(f32, element);
    let _ = b.load(row, inner);
    b.ret_void();
    assert_eq!(text(ok(untyped(&m))), Vec::<String>::new());
}

#[test]
fn derive_reports_an_untypable_value() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (b, args, _) = function(&cx, &m, tcx.void(), &[tcx.ptr(tcx.float(), 0)], &[b"p"]);
    // A cast changes the pointee, so it follows nothing; with no use, nothing types it, and the
    // argument's only use is the cast.
    b.bitcast(args[0], tcx.ptr(tcx.int(32), 0)).set_name(b"c");
    b.ret_void();
    assert_eq!(
        text(ok(untyped(&m))),
        [
            "oracle-untyped f argument 0 float*",
            "oracle-untyped f %c = bitcast float* %p to i32*",
        ]
    );
}

#[test]
fn derive_follows_select_and_phi_to_a_fixpoint() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (f32, pf) = (tcx.float(), tcx.ptr(tcx.float(), 0));
    let params = [tcx.int(1), pf, tcx.ptr(tcx.int(32), 0)];
    let (b, args, func) = function(&cx, &m, tcx.void(), &params, &[b"c", b"a", b"q"]);
    let (cond, start, ints) = (args[0], args[1], args[2]);
    let entry = b.insert_block().unwrap();
    let body = func.append_block(&cx, c"loop").unwrap();
    let exit = func.append_block(&cx, c"exit").unwrap();
    let slot = b.alloca(f32, c"x");
    b.br(body);
    b.position_at_end(body);
    // %a is reached only through the phi, after the select below has typed its result.
    let iv = b.phi(pf);
    let step = b.select(cond, iv, slot);
    b.add_incoming(iv, start, entry);
    b.add_incoming(iv, step, body);
    b.cond_br(cond, body, exit);
    b.position_at_end(exit);
    let cast = b.bitcast(ints, pf);
    b.store(cx.const_real(f32, 0.0), cast);
    b.ret_void();
    assert_eq!(text(ok(untyped(&m))), ["oracle-untyped f argument 2 i32*"]);
}

#[test]
fn derive_types_callees_and_atomic_addresses() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let (i32, callee) = (tcx.int(32), tcx.function(tcx.void(), &[], false));
    let (pi, pcallee) = (tcx.ptr(i32, 1), tcx.ptr(callee, 0));
    let params = [pi, pi, tcx.ptr(pcallee, 0), i32];
    let (b, args, _) = function(&cx, &m, tcx.void(), &params, &[b"r", b"t", b"slot", b"x"]);
    let loaded = b.load(pcallee, args[2]);
    // SAFETY: the loaded value is a pointer to `callee`, of type `pcallee`.
    let _ = b.call(unsafe { Func::new(loaded.raw(), callee, pcallee) }, &[]);
    let order = sys::ATOMIC_ORDERING_MONOTONIC;
    let _ = b.atomic_rmw(sys::ATOMIC_RMW_BIN_OP_ADD, args[0], args[3], order, false);
    let _ = b.atomic_cmpxchg(args[1], args[3], args[3], order, order, false);
    b.ret_void();
    assert_eq!(text(ok(untyped(&m))), Vec::<String>::new());
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
    let Err(error) = derive(&m) else {
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

/// The entries of `dir`, sorted.
fn sorted(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
}

/// The untyped lines of the module in `bitcode`, read into a fresh context.
fn untyped_of(bitcode: &[u8], what: &Path) -> Vec<Vec<u8>> {
    let cx = Context::new();
    // SAFETY: `bitcode` holds its length; the name is NUL-terminated; the buffer is a copy.
    let buf = unsafe {
        sys::create_memory_buffer_with_memory_range_copy(
            bitcode.as_ptr().cast(),
            bitcode.len(),
            c"".as_ptr(),
        )
    };
    let mut raw = ptr::null_mut();
    // SAFETY: the context and the buffer are live; `raw` is writable.
    let failed = unsafe { parse_bitcode_in_context2(cx.raw(), buf, &raw mut raw) } != 0;
    // SAFETY: the buffer made above; the module read from it holds none of it.
    unsafe { sys::dispose_memory_buffer(buf) };
    if failed {
        let why = cx
            .take_error()
            .map(|e| e.text().to_vec())
            .unwrap_or_default();
        panic!("{}: {}", what.display(), String::from_utf8_lossy(&why));
    }
    // SAFETY: a module of `cx` that nothing else owns.
    let m = unsafe { Module::from_raw(&cx, raw) };
    untyped(&m)
        .unwrap_or_else(|e| panic!("{}: {}", what.display(), String::from_utf8_lossy(e.text())))
}

/// The derivation over every module the C++ wrote (`$AIRCONV_STEP4_BASE/<c>.out/*.metallib`).
///
/// No fact is wrong, and each corpus' untyped lines (`LC_ALL=C sort -u`) are written to
/// `untyped/<c>.txt` beside the baseline folder when that file does not exist, or compared with
/// it: a line it lacks fails, a line only it holds is printed.
#[test]
fn derive_over_every_baseline_module() {
    let Some(base) = env::var_os("AIRCONV_STEP4_BASE") else {
        eprintln!("AIRCONV_STEP4_BASE is not set: no baseline module is derived");
        return;
    };
    let base = Path::new(&base);
    let lists = base.parent().unwrap_or(base).join("untyped");
    let mut modules = 0;
    for out in sorted(base) {
        if !out.is_dir() || out.extension().is_none_or(|ext| ext != "out") {
            continue;
        }
        let mut lines = Vec::new();
        for path in sorted(&out) {
            if path.extension().is_none_or(|ext| ext != "metallib") {
                continue;
            }
            let bytes = fs::read(&path).unwrap();
            let (bitcode, _) = airconv_ir::metallib::parse(&bytes)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            lines.extend(untyped_of(&bitcode, &path));
            modules += 1;
        }
        lines.sort();
        lines.dedup();
        let corpus = out.file_stem().unwrap().to_string_lossy().into_owned();
        let list = lists.join(format!("{corpus}.txt"));
        if !list.exists() {
            fs::create_dir_all(&lists).unwrap();
            fs::write(
                &list,
                lines
                    .iter()
                    .flat_map(|l| [&l[..], b"\n"].concat())
                    .collect::<Vec<u8>>(),
            )
            .unwrap_or_else(|e| panic!("{}: {e}", list.display()));
            eprintln!(
                "{corpus}: {} untyped, written to {}",
                lines.len(),
                list.display()
            );
            continue;
        }
        let kept = fs::read(&list).unwrap();
        let kept: Vec<&[u8]> = kept
            .split(|&c| c == b'\n')
            .filter(|l| !l.is_empty())
            .collect();
        let new: Vec<String> = lines
            .iter()
            .filter(|l| kept.binary_search(&&l[..]).is_err())
            .map(|l| String::from_utf8_lossy(l).into_owned())
            .collect();
        assert!(
            new.is_empty(),
            "{corpus}: new untyped values:\n{}",
            new.join("\n")
        );
        for gone in kept
            .iter()
            .filter(|k| lines.binary_search_by(|l| l[..].cmp(k)).is_err())
        {
            eprintln!(
                "{corpus}: no longer untyped: {}",
                String::from_utf8_lossy(gone)
            );
        }
        eprintln!("{corpus}: {} untyped, as {}", lines.len(), list.display());
    }
    assert!(modules > 0, "no metallib under {}", base.display());
    eprintln!("derived {modules} modules");
}
