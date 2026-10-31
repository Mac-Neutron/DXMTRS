//! The DXIL converter: concurrent compiles and a static global's name.
//!
//! The corpus itself is compared with the C++'s by the lane runs of `dxil-translate` (C1, C2,
//! C3, C8, C11, with the root signatures the tool gives them); these pin what a lane run cannot
//! see.

use airconv_ir::metallib;

use super::move_constants_to_constant_space;
use crate::{
    abi::ArgChain,
    api::Error,
    entry::{compile, initialize},
    llvm::{Context, sys},
    module,
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
