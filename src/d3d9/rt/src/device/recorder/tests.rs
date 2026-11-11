use crate::device::fixture::{at, core_on_fakes};

/// A whole-target Clear reaches Metal as a render pass that clears colour target 0.
#[test]
fn clear_becomes_the_pass_load_action() {
    const DEVICE: u64 = 0xa1;
    let mut core = core_on_fakes(DEVICE);
    core.clear([0.0, 0.0, 1.0, 1.0]);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let encoder = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
    assert_eq!(
        calls[encoder + 1],
        "  color0 load=2 store=1 clear=0,0,1,1",
        "{calls:#?}"
    );
}

/// A Clear behind an open render pass ends that pass first (the core's `begin` would drop it).
#[test]
fn clear_behind_an_open_pass_closes_it() {
    const DEVICE: u64 = 0xa2;
    let mut core = core_on_fakes(DEVICE);
    core.open_render_pass();
    core.clear([1.0, 0.0, 0.0, 1.0]);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let encoders = calls
        .iter()
        .filter(|c| *c == "MTLCommandBuffer_renderCommandEncoder")
        .count();
    let ends = calls
        .iter()
        .filter(|c| *c == "MTLCommandEncoder_endEncoding")
        .count();
    assert!(encoders >= 1 && encoders == ends, "{calls:#?}");
    assert!(
        calls
            .iter()
            .any(|c| c == "  color0 load=2 store=1 clear=1,0,0,1"),
        "{calls:#?}"
    );
}
