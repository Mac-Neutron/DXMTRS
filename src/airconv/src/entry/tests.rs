//! Tests of the entry points that span a whole compile.

use super::{compile, initialize};
use crate::{abi::ArgChain, api::Error};

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
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
