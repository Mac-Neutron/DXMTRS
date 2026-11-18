use mtld3d_core::{dirty_rect::DirtyRect, pipeline_key::PipelineKey};

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

/// A read of a multisampled back buffer resolves it first, as Present does, then reads that.
///
/// After a draw, the resolve folds into the draw's pass before the copy, and the copy reads the
/// presented texture; a Present after the read resolves again, in a pass of its own.
#[test]
fn a_multisampled_back_buffer_is_read_resolved() {
    const DEVICE: u64 = 0x3a5;
    let mut core = core_with_samples(DEVICE, 4);
    core.drain();
    let made = back_buffers(&dxmt_fakes::take_calls(DEVICE));
    let (presented, companion) = (made[0].clone(), made[1].clone());
    let binds = ff_binds();
    let four = PipelineKey {
        samples: 4,
        ..key(0xf)
    };
    assert_eq!(core.draw(&four, &triangle(&binds)), Ok(()));
    let mut out = [0_u8; 8];
    assert!(core.read_presented(texels(3, 2, 2, 1), &mut out, 8));
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(targets(&calls)[0].0, companion, "{calls:#?}");
    let resolved = at(&calls, &format!("  resolve0 {presented}"));
    let copy = at(
        &calls,
        &format!("    copy_to_buffer {presented} level=0 slice=0 size=2x1 bytes_per_row=8"),
    );
    assert!(resolved < copy, "{calls:#?}");
    core.present(true, 0.0);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let drawn = assert_resolved_then_presented(&calls, &companion, &presented);
    assert_eq!(drawn, 0, "a resolve pass of its own: {calls:#?}");
}

/// A read of a single-sample back buffer resolves nothing: the drawn texture is the one shown.
#[test]
fn a_single_sample_back_buffer_is_read_as_drawn() {
    const DEVICE: u64 = 0x3a6;
    let mut core = core_with_samples(DEVICE, 1);
    core.drain();
    let drawn = back_buffers(&dxmt_fakes::take_calls(DEVICE))[0].clone();
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    let mut out = [0_u8; 4];
    assert!(core.read_presented(texels(3, 2, 1, 1), &mut out, 4));
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("  resolve0 ") || c.contains(" store=3 ")),
        "{calls:#?}"
    );
    at(
        &calls,
        &format!("    copy_to_buffer {drawn} level=0 slice=0 size=1x1 bytes_per_row=4"),
    );
}

/// A read the back buffer cannot answer is refused, and nothing is recorded, a resolve included.
///
/// An empty rect, one past an edge or past `u32::MAX`, a pitch under the rect's row and a short
/// destination, on a single-sample and a multisampled back buffer; and a device without one.
#[test]
fn a_refused_read_records_nothing() {
    for (device, samples) in [(0x3a7, 1), (0x3a8, 4)] {
        let mut core = core_with_samples(device, samples);
        core.drain();
        let _ = dxmt_fakes::take_calls(device);
        let mut out = vec![0_u8; 64 * 4 * 32];
        for (what, rect, len, pitch) in [
            ("an empty width", texels(0, 0, 0, 4), 256 * 4, 256),
            ("an empty height", texels(0, 0, 4, 0), 256 * 4, 256),
            ("past the right edge", texels(60, 0, 8, 1), 256, 256),
            ("past the bottom edge", texels(0, 30, 1, 4), 256 * 4, 256),
            ("past u32::MAX", texels(u32::MAX, 0, 2, 1), 256, 256),
            ("a pitch under the row", texels(0, 0, 4, 2), 256, 15),
            ("a short destination", texels(0, 0, 4, 2), 31, 16),
        ] {
            assert!(
                !core.read_presented(rect, &mut out[..len], pitch),
                "{what}, {samples} samples"
            );
        }
        assert!(core.readback.is_none(), "{samples} samples");
        core.drain();
        let calls = dxmt_fakes::take_calls(device);
        assert!(calls.is_empty(), "{samples} samples: {calls:#?}");
    }
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { winemetal_sys::Obj::adopt(0x3a9) }.expect("a non-zero handle");
    let mut core = crate::device::CoreDevice::new(mtl, crate::device::queue_profile());
    assert!(!core.read_presented(texels(0, 0, 1, 1), &mut [0_u8; 4], 4));
}

/// A write of the back buffer from rows wider than its row is one copy of the whole, packed.
///
/// The fewest bytes the rows take are enough; the copy's rows are the back buffer's 256 bytes.
#[test]
fn a_back_buffer_write_packs_its_rows() {
    const DEVICE: u64 = 0x3b1;
    let mut core = core_with_samples(DEVICE, 1);
    core.drain();
    let presented = back_buffers(&dxmt_fakes::take_calls(DEVICE))[0].clone();
    let pitch = 64 * 4 + 12;
    assert!(core.write_presented(
        DirtyRect::full(64, 32),
        &vec![0_u8; pitch * 31 + 64 * 4],
        pitch
    ));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let copies = copies_to_texture(&calls);
    assert_eq!(copies.len(), 1, "{calls:#?}");
    let copy = &calls[copies[0]];
    assert!(
        copy.starts_with(&format!(
            "  blit copy_to_texture {presented} level=0 slice=0 origin=0,0,0 size=64x32x1 "
        )),
        "{copy}"
    );
    assert!(
        copy.ends_with(" bytes_per_row=256 bytes_per_image=8192"),
        "{copy}"
    );
}

/// A write the back buffer cannot take is refused, and nothing is recorded.
///
/// A multisampled back buffer, a pitch under the row and bytes short of the rows; and a device
/// without a back buffer.
#[test]
fn a_refused_back_buffer_write_records_nothing() {
    let bytes = vec![0_u8; 64 * 4 * 32];
    for (what, device, samples, len, pitch) in [
        ("a multisampled back buffer", 0x3b2, 4, 64 * 4 * 32, 64 * 4),
        ("a pitch under the row", 0x3b3, 1, 64 * 4 * 32, 64 * 4 - 1),
        ("bytes short of the rows", 0x3b4, 1, 64 * 4 * 32 - 1, 64 * 4),
    ] {
        let mut core = core_with_samples(device, samples);
        core.drain();
        let _ = dxmt_fakes::take_calls(device);
        assert!(
            !core.write_presented(DirtyRect::full(64, 32), &bytes[..len], pitch),
            "{what}"
        );
        core.drain();
        let calls = dxmt_fakes::take_calls(device);
        assert!(calls.is_empty(), "{what}: {calls:#?}");
    }
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { winemetal_sys::Obj::adopt(0x3b5) }.expect("a non-zero handle");
    let mut core = crate::device::CoreDevice::new(mtl, crate::device::queue_profile());
    assert!(!core.write_presented(DirtyRect::full(64, 32), &bytes, 64 * 4));
}

/// A write of the back buffer before a Present and one after it are recorded in that order.
#[test]
fn back_buffer_writes_keep_their_order_around_a_present() {
    const DEVICE: u64 = 0x3b6;
    let mut core = core_with_samples(DEVICE, 1);
    let bytes = vec![0_u8; 64 * 4 * 32];
    assert!(core.write_presented(DirtyRect::full(64, 32), &bytes, 64 * 4));
    core.present(true, 0.0);
    assert!(core.write_presented(DirtyRect::full(64, 32), &bytes, 64 * 4));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let copies = copies_to_texture(&calls);
    let present = at(&calls, "MTLCommandBuffer_presentDrawable");
    assert_eq!(copies.len(), 2, "{calls:#?}");
    assert!(copies[0] < present && present < copies[1], "{calls:#?}");
}

/// A write of a rect off the origin, of an odd width, copies that rect from its first texel.
///
/// The copy's origin and size are the rect's, its rows the rect's row packed from rows wider
/// than it; the draw after it loads the back buffer, in a pass after the copy.
#[test]
fn a_back_buffer_rect_write_lands_at_its_origin() {
    const DEVICE: u64 = 0x420;
    let mut core = core_with_samples(DEVICE, 1);
    core.drain();
    let presented = back_buffers(&dxmt_fakes::take_calls(DEVICE))[0].clone();
    let pitch = 7 * 4 + 12;
    let bytes = vec![0_u8; pitch * 3 + 7 * 4];
    assert!(core.write_presented(texels(3, 5, 7, 4), &bytes, pitch));
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let copies = copies_to_texture(&calls);
    assert_eq!(copies.len(), 1, "{calls:#?}");
    let copy = &calls[copies[0]];
    assert!(
        copy.starts_with(&format!(
            "  blit copy_to_texture {presented} level=0 slice=0 origin=3,5,0 size=7x4x1 "
        )),
        "{copy}"
    );
    assert!(
        copy.ends_with(" bytes_per_row=28 bytes_per_image=112"),
        "{copy}"
    );
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    assert_eq!(targets(&calls)[0].0, presented, "{calls:#?}");
    let drawn = line(found[0], "  color0 ").unwrap_or_default();
    assert!(drawn.starts_with("load=1 store=1 "), "{calls:#?}");
    let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
    assert!(copies[0] < render, "{calls:#?}");
}

/// A write of a rect the back buffer does not hold is refused, and nothing is recorded.
///
/// An empty rect, one past an edge and one past `u32::MAX`, with bytes enough for the whole.
#[test]
fn a_back_buffer_write_past_an_edge_records_nothing() {
    const DEVICE: u64 = 0x421;
    let mut core = core_with_samples(DEVICE, 1);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    let bytes = vec![0_u8; 64 * 4 * 32];
    for (what, rect) in [
        ("an empty width", texels(0, 0, 0, 4)),
        ("an empty height", texels(0, 0, 4, 0)),
        ("past the right edge", texels(60, 0, 8, 1)),
        ("past the bottom edge", texels(0, 30, 1, 4)),
        ("past u32::MAX", texels(u32::MAX, 0, 2, 1)),
    ] {
        assert!(!core.write_presented(rect, &bytes, 64 * 4), "{what}");
    }
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(calls.is_empty(), "{calls:#?}");
}

/// The indices of the copies into a texture `calls` shows.
fn copies_to_texture(calls: &[String]) -> Vec<usize> {
    calls
        .iter()
        .enumerate()
        .filter(|(_, c)| c.trim_start().starts_with("blit copy_to_texture "))
        .map(|(i, _)| i)
        .collect()
}

/// The rect of `width` x `height` texels at (`x`, `y`).
const fn texels(x: u32, y: u32, width: u32, height: u32) -> DirtyRect {
    DirtyRect {
        x,
        y,
        w: width,
        h: height,
    }
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
