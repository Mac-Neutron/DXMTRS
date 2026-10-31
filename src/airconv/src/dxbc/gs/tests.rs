//! The geometry pipeline's warp table and its refusal of shaders of other stages.
//!
//! The functions themselves are compared with the C++'s by the lane runs of `dxil-translate`
//! (`C6`'s `gs`, `gspt` and `gsadj` pipelines).

use super::{convert_geometry, convert_vertex, warp_counts};
use crate::{abi::ArgChain, api::Error, entry::initialize, llvm::Context, module, shader::Shader};

/// The table's rows, from `get_vertex_primitive_count_in_warp`.
#[test]
fn warp_counts_follow_the_table() {
    // Point, line, triangle and their adjacencies, a list and a strip.
    assert_eq!(warp_counts(1, false), (32, 32, 1));
    assert_eq!(warp_counts(1, true), (32, 32, 1));
    assert_eq!(warp_counts(2, false), (32, 16, 2));
    assert_eq!(warp_counts(2, true), (31, 31, 2));
    assert_eq!(warp_counts(3, false), (30, 10, 3));
    assert_eq!(warp_counts(3, true), (30, 30, 3));
    assert_eq!(warp_counts(6, false), (32, 8, 4));
    assert_eq!(warp_counts(6, true), (29, 29, 4));
    assert_eq!(warp_counts(7, false), (30, 5, 6));
    assert_eq!(warp_counts(7, true), (28, 14, 6));
    // The patches: the explicit rows (1 to 16 control points), then one patch a warp.
    assert_eq!(warp_counts(8, false), (32, 32, 1));
    assert_eq!(warp_counts(12, true), (30, 6, 5));
    assert_eq!(warp_counts(16, false), (27, 3, 9));
    assert_eq!(warp_counts(23, false), (32, 2, 16));
    assert_eq!(warp_counts(24, false), (17, 1, 17));
    assert_eq!(warp_counts(39, false), (32, 1, 32));
    // Undefined and past the last patch.
    assert_eq!(warp_counts(0, false), (0, 0, 0));
    assert_eq!(warp_counts(4, false), (0, 0, 0));
    assert_eq!(warp_counts(40, false), (0, 0, 0));
}

/// A pixel shader as the vertex or the geometry shader is an error, not a lowering.
#[test]
fn a_pipeline_of_other_stages_is_refused() {
    let bytes = include_bytes!("../../../ir/tests/data/render.ps.dxbc");
    // SAFETY: no reflection is asked for.
    let Ok(Shader::Dxbc(pixel)) = (unsafe { initialize(bytes, core::ptr::null_mut()) }) else {
        panic!("render.ps.dxbc does not initialize");
    };
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    // SAFETY: a null chain is the empty one.
    let args = unsafe { ArgChain::walk(core::ptr::null()) };
    let refused = |result: Result<(), Error>| {
        let error = result.expect_err("a pipeline of pixel shaders compiled");
        assert!(
            error
                .text()
                .starts_with(b"invalid operation: a geometry pipeline")
        );
    };
    refused(convert_geometry(
        &pixel.ir,
        &pixel.ir,
        b"gs",
        &m,
        &args,
        |_, _| panic!("the body ran"),
    ));
    refused(convert_vertex(
        &pixel.ir,
        &pixel.ir,
        b"vs",
        &m,
        &args,
        |_, _| panic!("the body ran"),
    ));
}
