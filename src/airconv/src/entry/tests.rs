//! Tests of the entry points that span a whole compile.

use super::{compile, initialize};
use crate::{
    abi::{ArgChain, SM50_SHADER_COMMON, SM50_SHADER_METAL_320, Sm50ShaderCommonData},
    api::Error,
};

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

/// `render.ps.dxbc` initialized once and compiled on eight threads at once (Review Focus 2).
///
/// D3D11's workers compile variants of one shader handle concurrently: two chains (null, and
/// `COMMON` at Metal 320) alternate across the eight threads, and each thread's metallib equals
/// its own single-thread output.
#[test]
fn compile_on_eight_threads_matches_one() {
    let bytes = include_bytes!("../../ir/tests/data/render.ps.dxbc");
    // SAFETY: no reflection is asked for.
    let shader = ok(unsafe { initialize(bytes, core::ptr::null_mut()) });
    let compile_null = || {
        // SAFETY: a null chain is the empty one.
        let args = unsafe { ArgChain::walk(core::ptr::null()) };
        ok(compile(&shader, &args, c"main"))
    };
    let compile_320 = || {
        let common = Sm50ShaderCommonData {
            next: core::ptr::null_mut(),
            r#type: SM50_SHADER_COMMON,
            metal_version: SM50_SHADER_METAL_320,
            flags: 0,
        };
        // SAFETY: `common` is a valid single-node argument chain.
        let args = unsafe { ArgChain::walk((&raw const common).cast()) };
        ok(compile(&shader, &args, c"main"))
    };
    let single_null = compile_null();
    let single_320 = compile_320();
    let shared = std::thread::scope(|s| {
        let mut threads = Vec::new();
        for i in 0..8 {
            if i % 2 == 0 {
                threads.push(s.spawn(compile_null));
            } else {
                threads.push(s.spawn(compile_320));
            }
        }
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect::<Vec<_>>()
    });
    for (i, metallib) in shared.iter().enumerate() {
        let expected = if i % 2 == 0 {
            &single_null
        } else {
            &single_320
        };
        assert_eq!(metallib, expected, "thread {i}");
    }
}
