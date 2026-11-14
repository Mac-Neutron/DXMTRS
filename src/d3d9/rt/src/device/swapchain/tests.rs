use crate::device::fixture::{at, core_on_fakes, returns_within_10s};

/// A visible Present presents the back buffer in the chunk it commits, then counts the frame.
#[test]
fn present_records_present_signals_commits_then_bounds() {
    const DEVICE: u64 = 0xa3;
    let mut core = core_on_fakes(DEVICE);
    core.clear([0.0, 1.0, 0.0, 1.0], false);
    assert_eq!(core.device().queue().current_frame_seq(), 1);
    core.present(true, 0.0);
    assert_eq!(core.device().queue().current_frame_seq(), 2);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let present = at(&calls, "MTLCommandBuffer_presentDrawable");
    let commit = at(&calls, "MTLCommandBuffer_commit");
    assert!(present < commit, "{calls:#?}");
}

/// A Present behind an open render pass ends that pass first.
#[test]
fn present_closes_the_open_pass() {
    const DEVICE: u64 = 0xa4;
    let mut core = core_on_fakes(DEVICE);
    core.open_render_pass();
    core.present(true, 0.0);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    at(&calls, "MTLCommandBuffer_presentDrawable");
}

/// Hidden Presents never wait on a frame nothing signals.
///
/// They skip the present, its frame-latency signal and its bound, so `max_latency + 2` of them
/// and a visible one return.
#[test]
fn hidden_presents_never_hang() {
    const DEVICE: u64 = 0xa5;
    returns_within_10s(|| {
        let mut core = core_on_fakes(DEVICE);
        let hidden = core.device().queue().max_latency() + 2;
        for _ in 0..hidden {
            core.clear([0.0, 0.0, 0.0, 1.0], false);
            core.present(false, 0.0);
        }
        assert_eq!(core.device().queue().current_frame_seq(), 1);
        core.present(true, 0.0);
        assert_eq!(core.device().queue().current_frame_seq(), 2);
        core.drain();
    });
}

/// Each D3D9 presentation interval waits its number of refreshes; IMMEDIATE waits none.
#[test]
fn present_after_follows_the_interval() {
    use super::present_after;
    assert!((present_after(0x8000_0000, 60) - 0.0).abs() < f64::EPSILON);
    assert!((present_after(0, 60) - 1.0 / 60.0).abs() < 1e-9);
    assert!((present_after(1, 120) - 1.0 / 120.0).abs() < 1e-9);
    assert!((present_after(2, 60) - 2.0 / 60.0).abs() < 1e-9);
    assert!((present_after(4, 60) - 3.0 / 60.0).abs() < 1e-9);
    assert!((present_after(8, 60) - 4.0 / 60.0).abs() < 1e-9);
    assert!((present_after(1, 0) - 1.0 / 60.0).abs() < 1e-9);
}
