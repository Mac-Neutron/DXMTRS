use crate::device::fixture::{at, core_on_fakes};

/// The read-back blit waits for the render pass that wrote the back buffer.
///
/// It comes after that pass, waits on a fence the pass updates, and the call returns once its
/// chunk is done.
#[test]
fn readback_waits_on_the_render_pass_then_its_seq() {
    const DEVICE: u64 = 0xa6;
    let mut core = core_on_fakes(DEVICE);
    core.clear([0.0, 0.0, 1.0, 1.0]);
    let mut out = vec![0_u8; 64 * 4 * 32];
    let seq = core.device().queue().current_seq();
    assert!(core.read_back(&mut out, 64 * 4));
    assert!(core.device().queue().coherent_seq() >= seq);
    let calls = dxmt_fakes::take_calls(DEVICE);
    let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    assert!(render < blit, "{calls:#?}");
    // The fences the clear's render pass updates (record 34), and those the blit waits for (6)
    // before its texture-to-buffer copy (3).
    let updated = calls[render..blit]
        .iter()
        .filter_map(|c| c.strip_prefix("  render 34 fence="))
        .filter_map(|c| c.split(' ').next())
        .collect::<Vec<_>>();
    let copy = blit
        + calls[blit..]
            .iter()
            .position(|c| c == "  blit 3")
            .unwrap_or_else(|| panic!("no texture-to-buffer copy in {calls:#?}"));
    let waits = calls[blit..copy]
        .iter()
        .filter_map(|c| c.strip_prefix("  blit 6 fence="))
        .collect::<Vec<_>>();
    assert!(
        waits.iter().any(|w| updated.contains(w)),
        "the blit waits on {waits:?}, the render pass updates {updated:?}: {calls:#?}"
    );
}

/// A destination too small for the back buffer at its pitch is refused.
#[test]
fn readback_refuses_a_short_destination() {
    let mut core = core_on_fakes(0xa7);
    let mut out = vec![0_u8; 16];
    assert!(!core.read_back(&mut out, 64 * 4));
}
