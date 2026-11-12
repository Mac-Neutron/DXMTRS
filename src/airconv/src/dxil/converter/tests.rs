//! The DXIL converter: concurrent compiles, a static global's name and the 64-bit atomics.
//!
//! The corpus itself is compared with the C++'s by the lane runs of `dxil-translate` (C1, C2,
//! C3, C8, C11, with the root signatures the tool gives them); these pin what a lane run cannot
//! see.

use airconv_ir::{
    container::{self, Container, fourcc},
    dxil_part, metallib,
};

use super::{convert, move_constants_to_constant_space, rebuilt};
use crate::{
    abi::ArgChain,
    api::Error,
    entry::{compile, initialize},
    llvm::{Builder, Context, sys},
    module,
    shader::Shader,
    writer,
};

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

/// `cache.cs.dxil` initialized once and compiled on eight threads at once (Review Focus 2).
///
/// D3D12's workers may compile one shader handle concurrently: every metallib equals the one a
/// single compile makes.
#[test]
fn compile_on_eight_threads_matches_one() {
    let bytes = include_bytes!("../../../ir/tests/data/cache.cs.dxil");
    // SAFETY: no reflection is asked for.
    let shader = ok(unsafe { initialize(bytes, core::ptr::null_mut()) });
    let compile_once = || {
        // SAFETY: a null chain is the empty one.
        let args = unsafe { ArgChain::walk(core::ptr::null()) };
        ok(compile(&shader, &args, c"main"))
    };
    let single = compile_once();
    let shared = std::thread::scope(|s| {
        let mut threads = Vec::new();
        for _ in 0..8 {
            threads.push(s.spawn(compile_once));
        }
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(shared.iter().all(|metallib| *metallib == single));
    // A compute function, its bindings (none without a root signature) in a metallib.
    let (bitcode, entries) = metallib::parse(&single).unwrap();
    assert_eq!(entries.len(), 1);
    let cx = Context::new();
    let text =
        String::from_utf8(ok(module::parse(&cx, &bitcode, b"", b"", module::Layout::Keep)).print())
            .unwrap();
    assert!(text.contains("!air.kernel"));
}

/// A mutable static's name reaches the error as its bytes, not as UTF-8 (P15).
#[test]
fn static_global_name_keeps_its_bytes() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    let i32_ty = cx.tcx().int(32);
    // SAFETY: a live module, a type of its context and a NUL-terminated name.
    let global = unsafe {
        sys::add_global_in_address_space(m.raw(), cx.tcx().lower(i32_ty), c"".as_ptr(), 0)
    };
    let name = b"g\xff";
    // SAFETY: the global just made; the name holds its length.
    unsafe { sys::set_value_name2(global, name.as_ptr().cast(), name.len()) };
    // SAFETY: as above; a constant of its value type.
    unsafe { sys::set_initializer(global, cx.const_int(i32_ty, 7, false).raw()) };
    let Err(error) = move_constants_to_constant_space(&m) else {
        panic!("a mutable static moved");
    };
    assert_eq!(
        error.text(),
        b"DXIL: static (non-const) global g\xff not supported"
    );
}

/// 64-bit groupshared atomics (`gsatomic64.dxil`) lower on LLVM 23's opaque pointers (P4).
#[test]
fn groupshared64_atomics_lower_on_opaque_pointers() {
    let bytes = include_bytes!("../../../ir/tests/data/gsatomic64.dxil");
    // SAFETY: no reflection is asked for.
    let shader = ok(unsafe { initialize(bytes, core::ptr::null_mut()) });
    // SAFETY: a null chain is the empty one.
    let args = unsafe { ArgChain::walk(core::ptr::null()) };
    let metallib = ok(compile(&shader, &args, c"main"));
    let (bitcode, entries) = metallib::parse(&metallib).unwrap();
    assert_eq!(entries.len(), 1);
    let cx = Context::new();
    let text =
        String::from_utf8(ok(module::parse(&cx, &bitcode, b"", b"", module::Layout::Keep)).print())
            .unwrap();
    assert!(text.contains("!air.kernel"), "{text}");
    assert!(!text.contains("atomicrmw"), "{text}");
    assert!(
        !text
            .lines()
            .any(|l| l.contains("cmpxchg") && l.contains("i64")),
        "{text}"
    );
}

/// Every 64-bit texel atomic's write (`atomic64typed.dxil`) is built on its section's read.
///
/// So the write can't overtake the read: an Exchange's value doesn't depend on the read, Metal's
/// texture fence orders only a write before a later read, and a read before a later write to the
/// same texel was measured unordered (its old came back as its own new value about once in 50
/// dispatches). The read is of the write's texel, and no lock acquire (a `cmpxchg`) lies between.
#[test]
fn texel64_atomic_writes_depend_on_their_read() {
    let bytes = include_bytes!("../../../ir/tests/data/atomic64typed.dxil");
    // SAFETY: no reflection is asked for.
    let shader = ok(unsafe { initialize(bytes, core::ptr::null_mut()) });
    // The lock table comes with a root signature: RTS0 1.0, one table of t0-t6, u0-u14 and b0
    // (range type, count, base, space, offset).
    let mut rts0: Vec<u32> = vec![1, 1, 24, 0, 0, 0, 0, 0, 36, 3, 44];
    rts0.extend([0, 7, 0, 0, 0, 1, 15, 0, 0, 7, 2, 1, 0, 0, 22]);
    let part: Vec<u8> = rts0.iter().flat_map(|w| w.to_le_bytes()).collect();
    let container = container::build(&[(fourcc::RTS0, &part)]).unwrap();
    let root = crate::abi::Sm50ShaderRootSignatureData {
        next: core::ptr::null_mut(),
        r#type: crate::abi::SM50_SHADER_ROOT_SIGNATURE,
        bytecode: container.as_ptr().cast(),
        bytecode_length: container.len(),
    };
    // SAFETY: a chain of one node, which outlives it, as is its bytecode.
    let args = unsafe { ArgChain::walk((&raw const root).cast()) };
    let metallib = ok(compile(&shader, &args, c"main"));
    let (bitcode, _) = metallib::parse(&metallib).unwrap();
    let cx = Context::new();
    let text =
        String::from_utf8(ok(module::parse(&cx, &bitcode, b"", b"", module::Layout::Keep)).print())
            .unwrap();
    let lines: Vec<&str> = text.lines().collect();
    // `%name` -> (its line, its definition).
    let defs: rustc_hash::FxHashMap<&str, (usize, &str)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.trim_start().split_once(" = ").map(|(n, d)| (n, (i, d))))
        .filter(|(name, _)| name.starts_with('%'))
        .collect();
    let writes: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].contains("call void @air.write_texture_2d"))
        .collect();
    assert!(!writes.is_empty(), "{text}");
    for at in writes {
        let write = lines[at];
        // (handle, coordinate, texel, ...)
        let wargs = call_args(write);
        // The texel's lane .z (which an R32G32 write ignores), traced back through
        // insertelement, shufflevector and extractvalue: it must be a texture read's.
        let (mut value, mut lane) = (last(wargs[2]), 2);
        let (read_at, read) = loop {
            let &(_, def) = defs
                .get(value)
                .unwrap_or_else(|| panic!("{write}: lane {lane} is the constant {value}"));
            let ops = operands(def);
            if def.starts_with("insertelement") {
                let index: usize = last(ops[2])
                    .parse()
                    .unwrap_or_else(|_| panic!("{write}: {def} inserts at a variable index"));
                assert_ne!(index, lane, "{write}: lane {lane} inserted by {def}");
                value = last(ops[0]);
            } else if def.starts_with("shufflevector") {
                let mask: Vec<u32> = ops[2]
                    .trim_start_matches("<4 x i32> <")
                    .trim_end_matches('>')
                    .split(", ")
                    .map(|m| last(m).parse().unwrap())
                    .collect();
                let m = mask[lane];
                (value, lane) = (last(ops[usize::from(m >= 4)]), (m % 4) as usize);
            } else if def.starts_with("extractvalue") {
                break defs.get(last(ops[0])).copied().unwrap_or((0, ""));
            } else {
                panic!("{write}: lane {lane} comes from {value} = {def}");
            }
        };
        assert!(read.contains("@air.read_texture_2d"), "{write}: {read}");
        assert_eq!(
            call_args(read)[1],
            wargs[1],
            "{write} reads another texel: {read}"
        );
        assert!(
            read_at < at && !lines[read_at..at].iter().any(|l| l.contains("cmpxchg")),
            "{write}: its read {read} is of another lock section"
        );
    }
}

/// An instruction's operands, split at the commas outside `<>`, `{}` and `()`.
fn operands(def: &str) -> Vec<&str> {
    split_top(def.split_once(' ').map_or("", |(_, b)| b))
}

/// A call's arguments: inside the parentheses after its callee.
fn call_args(call: &str) -> Vec<&str> {
    let after = &call[call.find('@').unwrap_or(0)..];
    let open = after.find('(').unwrap_or_else(|| panic!("{call}"));
    let mut depth = 0i32;
    let close = after[open..]
        .char_indices()
        .find(|&(_, c)| {
            depth += match c {
                '(' => 1,
                ')' => -1,
                _ => 0,
            };
            depth == 0
        })
        .map_or(after.len(), |(i, _)| open + i);
    split_top(&after[open + 1..close])
}

/// `s` split at the commas outside `<>`, `{}` and `()`, each part trimmed.
fn split_top(s: &str) -> Vec<&str> {
    let (mut out, mut depth, mut from) = (Vec::new(), 0i32, 0);
    for (i, c) in s.char_indices() {
        match c {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[from..i].trim());
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[from..].trim());
    out
}

/// An operand's last token: its value without its type.
fn last(operand: &str) -> &str {
    operand.rsplit(' ').next().unwrap_or(operand)
}

/// DXC's `.hca` pattern (`buffers.dxil`) retargets and erases a no-op bitcast.
#[test]
fn hca_noop_bitcast_is_retargeted_not_rebuilt() {
    let bytes = include_bytes!("../../../ir/tests/data/buffers.dxil");
    let container = Container::parse(bytes).unwrap();
    let part = dxil_part::find(&container).unwrap().unwrap();
    let raw_cx = Context::new();
    let raw_m = ok(module::parse(
        &raw_cx,
        part.bitcode,
        b"dxil",
        b"DXIL: can't read the bitcode: ",
        module::Layout::Dxil,
    ));
    // SAFETY: a live module and NUL-terminated names.
    let hca0 = unsafe { sys::get_named_global(raw_m.raw(), c"kNeighbor.i.0.hca".as_ptr()) };
    assert!(!hca0.is_null());
    // SAFETY: as above.
    let main_fn = unsafe { sys::get_named_function(raw_m.raw(), c"main".as_ptr()) };
    // SAFETY: `@main` has an entry block and slot 125 is the reader's dead `bitcast ptr %10 to ptr`
    // instruction from `llvm.lifetime.start.p0i8`; point it at `@kNeighbor.i.0.hca` so `retarget`
    // sees `bitcast ptr @kNeighbor.i.0.hca to ptr`.
    let dead_cast = unsafe { sys::airconv_shim_local_slot(main_fn, 125) };
    assert!(!dead_cast.is_null());
    // SAFETY: `dead_cast` is a `BitCastInst` of the module.
    assert!(!unsafe { sys::is_a_bit_cast_inst(dead_cast) }.is_null());
    // SAFETY: `dead_cast` has operand 0 of type `ptr`, matching `hca0`.
    unsafe { sys::set_operand(dead_cast, 0, hca0) };
    let before_text = String::from_utf8(raw_m.print()).unwrap();
    assert!(
        before_text.contains("bitcast ptr @kNeighbor.i.0.hca to ptr"),
        "{before_text}"
    );
    ok(move_constants_to_constant_space(&raw_m));
    let moved_text = String::from_utf8(raw_m.print()).unwrap();
    assert!(
        moved_text.contains("@kNeighbor.i.0.hca = internal addrspace(2) constant [4 x i32]"),
        "{moved_text}"
    );
    assert!(
        moved_text.contains("@kNeighbor.i.1.hca = internal addrspace(2) constant [4 x i32]"),
        "{moved_text}"
    );
    assert!(
        !moved_text.contains("bitcast ptr @kNeighbor"),
        "{moved_text}"
    );
    assert_eq!(
        moved_text
            .lines()
            .filter(|l| l.contains(" = load ") && l.contains(", ptr addrspace(2) "))
            .count(),
        4,
        "{moved_text}"
    );

    // SAFETY: no reflection is asked for.
    let shader = ok(unsafe { initialize(bytes, core::ptr::null_mut()) });
    let Shader::Dxil(dxil) = &shader else {
        panic!("expected a DXIL shader");
    };
    // SAFETY: a null chain is the empty one.
    let args = unsafe { ArgChain::walk(core::ptr::null()) };
    let conv_cx = Context::new();
    let conv_m = ok(convert(&dxil.ir, b"main", &conv_cx, &args));
    let metallib = ok(writer::write(&conv_m, &[]));
    let (bitcode, entries) = metallib::parse(&metallib).unwrap();
    assert_eq!(entries.len(), 1);
    let out_cx = Context::new();
    let out_text = String::from_utf8(
        ok(module::parse(
            &out_cx,
            &bitcode,
            b"",
            b"",
            module::Layout::Keep,
        ))
        .print(),
    )
    .unwrap();
    assert!(
        out_text.contains("@kNeighbor.i.0.hca = internal addrspace(2) constant [4 x i32]"),
        "{out_text}"
    );
    assert!(
        out_text.contains("@kNeighbor.i.1.hca = internal addrspace(2) constant [4 x i32]"),
        "{out_text}"
    );
    assert_eq!(
        out_text
            .lines()
            .filter(|l| l.contains(" = load ") && l.contains(", ptr addrspace(2) "))
            .count(),
        4,
        "{out_text}"
    );
}

/// `rebuilt` refuses a builder closure that folds to its `freeze` placeholder.
#[test]
fn rebuilt_refuses_a_folded_cast() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    let tcx = cx.tcx();
    let i32_ty = tcx.int(32);
    let fn_ty = tcx.function(tcx.void(), &[], false);
    let f = ok(m.add_function(b"f", fn_ty));
    let bb = f.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(bb);
    let old = b.ret_void();
    let to = cx.const_int(i32_ty, 7, false);
    let Err(err) = rebuilt(&cx, old.raw(), to.raw(), |_, placeholder| placeholder) else {
        panic!("a folded cast was accepted");
    };
    assert!(
        String::from_utf8_lossy(err.text()).contains("placeholder"),
        "{}",
        String::from_utf8_lossy(err.text())
    );
    let text = String::from_utf8(m.print()).unwrap();
    assert!(text.contains("freeze i32 7"), "{text}");
    assert!(text.contains("ret void"), "{text}");
}
