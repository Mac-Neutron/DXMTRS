use crate::device::fixture::{at, core_on_fakes, ff_binds, key, line, passes, triangle};

/// A whole-target Clear reaches Metal as a render pass that clears colour target 0.
#[test]
fn clear_becomes_the_pass_load_action() {
    const DEVICE: u64 = 0xa1;
    let mut core = core_on_fakes(DEVICE);
    core.clear([0.0, 0.0, 1.0, 1.0], false);
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
    core.clear([1.0, 0.0, 0.0, 1.0], false);
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

/// A Clear of rects is a pass of its own between the draws around it: a scissored triangle a rect.
#[test]
fn a_rect_clear_is_a_pass_of_its_own() {
    const DEVICE: u64 = 0x300;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    let binds = ff_binds();
    assert_eq!(core.draw_up(&key(0xf), &triangle(&binds)), Ok(()));
    core.clear_rects(&[[1, 2, 3, 4], [8, 8, 16, 16]], [0.0, 1.0, 0.0, 1.0], false);
    assert_eq!(core.draw_up(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let all = passes(&calls);
    assert_eq!(all.len(), 3, "{calls:#?}");
    let rects = |pass: &[String]| pass.iter().filter(|c| c.trim() == "render 36").count();
    assert_eq!(
        (rects(all[0]), rects(all[1]), rects(all[2])),
        (0, 2, 0),
        "{calls:#?}"
    );
    // The quads load what the first draw stored: nothing of a whole clear's load action.
    let color = line(all[1], "  color0 ").unwrap_or_default();
    assert!(color.starts_with("load=1 store=1"), "{calls:#?}");
}

/// A Clear of no rect records nothing, and leaves the open pass open.
#[test]
fn an_empty_list_records_nothing() {
    const DEVICE: u64 = 0x301;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    core.clear_rects(&[], [0.0, 1.0, 0.0, 1.0], false);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(passes(&calls).is_empty(), "{calls:#?}");
}
