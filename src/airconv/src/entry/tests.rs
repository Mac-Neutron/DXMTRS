//! Tests of the entry points that span a whole compile.
//!
//! The baseline comparison reads `AIRCONV_STEP4_BASE`, step 4's frozen C++ outputs: unset, it
//! prints why and checks only what needs no baseline; set, a missing file fails it.

use std::{env, fs, path::PathBuf};

use airconv_ir::metallib;

use super::{compile, initialize};
use crate::{abi::ArgChain, api::Error, llvm::Context, module};

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

/// `sync.dxbc` (C4's barrier rows) compiled as `main` with no argument, as the tool does.
///
/// The module its metallib carries prints as the C++'s did.
#[test]
fn compute_shader_matches_the_baseline() {
    let bytes = include_bytes!("../../ir/tests/data/sync.dxbc");
    // SAFETY: no reflection is asked for.
    let shader = ok(unsafe { initialize(bytes, core::ptr::null_mut()) });
    // SAFETY: a null chain is the empty one.
    let args = unsafe { ArgChain::walk(core::ptr::null()) };
    let metallib = ok(compile(&shader, &args, c"main"));
    let (bitcode, entries) = metallib::parse(&metallib).unwrap();
    assert_eq!(entries.len(), 1);
    let cx = Context::new();
    let text = String::from_utf8(ok(module::parse(&cx, &bitcode, b"", b"")).print()).unwrap();
    assert!(text.contains("call void @air.wg.barrier(i32 7, i32 1)"));
    let Some(base) = env::var_os("AIRCONV_STEP4_BASE") else {
        eprintln!("AIRCONV_STEP4_BASE is not set: the module is not compared with the C++'s");
        return;
    };
    let path = PathBuf::from(base).join("C4.dis/sync.dxbc.ll");
    let baseline = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(text == baseline, "{}", path.display());
}

/// `render.ps.dxbc` initialized once and compiled on eight threads at once (Review Focus 2).
///
/// D3D11's workers compile variants of one shader handle concurrently: every metallib equals
/// the one a single compile makes.
#[test]
fn compile_on_eight_threads_matches_one() {
    let bytes = include_bytes!("../../ir/tests/data/render.ps.dxbc");
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
}
