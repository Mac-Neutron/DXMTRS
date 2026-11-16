use mtld3d_core::pipeline_key::PipelineKey;

use crate::device::fixture::{
    at, core_on_fakes, ff_binds, key, line, passes, returns_within_10s, targets, textures_made,
    triangle,
};

/// A visible Present presents the back buffer in the chunk it commits, then counts the frame.
#[test]
fn present_records_present_signals_commits_then_bounds() {
    const DEVICE: u64 = 0xa3;
    let mut core = core_on_fakes(DEVICE);
    core.clear(0, [0.0, 1.0, 0.0, 1.0], false);
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
            core.clear(0, [0.0, 0.0, 0.0, 1.0], false);
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

/// A core device on the made-up handle `handle` with a 64x32 back buffer of `samples`.
fn core_with_samples(handle: u64, samples: u8) -> crate::device::CoreDevice {
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { winemetal_sys::Obj::adopt(handle) }.expect("a non-zero handle");
    let mut core = crate::device::CoreDevice::new(mtl, crate::device::queue_profile());
    core.attach(1, 64, 32, samples);
    core
}

/// The frame `calls` shows: one pass resolving `companion` into `presented`, then the present.
///
/// Answers the draws that pass records (the present's own pass draws too).
fn assert_resolved_then_presented(calls: &[String], companion: &str, presented: &str) -> usize {
    let found = passes(calls);
    let resolving = found
        .iter()
        .filter(|pass| line(pass, "  resolve0 ").is_some())
        .collect::<Vec<_>>();
    assert_eq!(resolving.len(), 1, "{calls:#?}");
    let color = line(resolving[0], "  color0 ").unwrap_or_default();
    assert!(color.starts_with("load=1 store=3 "), "{calls:#?}");
    assert_eq!(
        line(resolving[0], "  resolve0 "),
        Some(presented),
        "{calls:#?}"
    );
    assert_eq!(targets(calls)[0].0, companion, "{calls:#?}");
    let resolved = calls
        .iter()
        .position(|c| c.starts_with("  resolve0 "))
        .unwrap_or(usize::MAX);
    let present = at(calls, "MTLCommandBuffer_presentDrawable");
    assert!(resolved < present, "{calls:#?}");
    resolving[0]
        .iter()
        .filter(|c| c.starts_with("  render 19 "))
        .count()
}

/// A multisampled back buffer is drawn in its companion and resolved into the presented one.
///
/// A frame that draws: the draw's pass names the companion and Present folds the resolve into
/// it (its store action, the presented texture its resolve view), before the present. A frame
/// that draws nothing: a resolve pass of its own, before the present.
#[test]
fn a_multisampled_back_buffer_presents_resolved() {
    const DEVICE: u64 = 0x38e;
    let mut core = core_with_samples(DEVICE, 4);
    core.drain();
    let made = back_buffers(&dxmt_fakes::take_calls(DEVICE));
    assert_eq!(made.len(), 2, "{made:?}");
    let (presented, companion) = (made[0].clone(), made[1].clone());
    assert_eq!(core.target_samples(), 4);
    let binds = ff_binds();
    let four = PipelineKey {
        samples: 4,
        ..key(0xf)
    };
    assert_eq!(core.draw(&four, &triangle(&binds)), Ok(()));
    core.present(true, 0.0);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let drawn = assert_resolved_then_presented(&calls, &companion, &presented);
    assert_eq!(drawn, 1, "folded into the draw's pass: {calls:#?}");
    core.present(true, 0.0);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let drawn = assert_resolved_then_presented(&calls, &companion, &presented);
    assert_eq!(drawn, 0, "a resolve pass of its own: {calls:#?}");
}

/// A single-sample back buffer is drawn and presented as before: one texture, no resolve.
#[test]
fn a_single_sample_back_buffer_is_as_before() {
    const DEVICE: u64 = 0x38f;
    let mut core = core_with_samples(DEVICE, 1);
    core.drain();
    let made = back_buffers(&dxmt_fakes::take_calls(DEVICE));
    assert_eq!(made.len(), 1, "{made:?}");
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.present(true, 0.0);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(targets(&calls)[0].0, made[0], "{calls:#?}");
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("  resolve0 ") || c.contains(" store=3 ")),
        "{calls:#?}"
    );
    at(&calls, "MTLCommandBuffer_presentDrawable");
}

/// The handles of the 64x32 textures `calls` shows made, in order: the presenter makes its own.
fn back_buffers(calls: &[String]) -> Vec<String> {
    textures_made(
        &calls
            .iter()
            .filter(|c| c.contains(" size=64x32x1 "))
            .cloned()
            .collect::<Vec<_>>(),
    )
}
