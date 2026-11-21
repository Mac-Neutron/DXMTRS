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
    core.present(true, 0.0, true);
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
    core.present(true, 0.0, true);
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
            core.present(false, 0.0, true);
        }
        assert_eq!(core.device().queue().current_frame_seq(), 1);
        core.present(true, 0.0, true);
        assert_eq!(core.device().queue().current_frame_seq(), 2);
        core.drain();
    });
}

/// Each D3D9 presentation interval waits its number of refreshes and honours `max_fps`.
#[test]
fn present_after_follows_the_interval() {
    use super::present_after;
    assert!((present_after(0x8000_0000, 60, 0) - 0.0).abs() < f64::EPSILON);
    assert!((present_after(0x8000_0000, 60, 30) - 1.0 / 30.0).abs() < 1e-9);
    assert!((present_after(0, 60, 0) - 1.0 / 60.0).abs() < 1e-9);
    assert!((present_after(1, 120, 0) - 1.0 / 120.0).abs() < 1e-9);
    assert!((present_after(1, 60, 120) - 1.0 / 60.0).abs() < 1e-9);
    assert!((present_after(1, 60, 30) - 1.0 / 30.0).abs() < 1e-9);
    assert!((present_after(2, 60, 0) - 2.0 / 60.0).abs() < 1e-9);
    assert!((present_after(2, 60, 120) - 2.0 / 60.0).abs() < 1e-9);
    assert!((present_after(4, 60, 0) - 3.0 / 60.0).abs() < 1e-9);
    assert!((present_after(8, 60, 0) - 4.0 / 60.0).abs() < 1e-9);
    assert!((present_after(1, 0, 0) - 1.0 / 60.0).abs() < 1e-9);
    assert!((present_after(2, 0, 0) - 2.0 / 60.0).abs() < 1e-9);
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
    core.present(true, 0.0, true);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let drawn = assert_resolved_then_presented(&calls, &companion, &presented);
    assert_eq!(drawn, 1, "folded into the draw's pass: {calls:#?}");
    core.present(true, 0.0, true);
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
    core.present(true, 0.0, true);
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
    core.present(true, 0.0, true);
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
/// A pitch under the row, bytes short of the rows and a rect past an edge on both single-sample
/// and multisampled back buffers; and a device without a back buffer.
#[test]
fn a_refused_back_buffer_write_records_nothing() {
    let bytes = vec![0_u8; 64 * 4 * 32];
    for (what, device, samples, rect, len, pitch) in [
        (
            "a pitch under the row",
            0x3b2,
            1,
            DirtyRect::full(64, 32),
            64 * 4 * 32,
            64 * 4 - 1,
        ),
        (
            "bytes short of the rows",
            0x3b3,
            1,
            DirtyRect::full(64, 32),
            64 * 4 * 32 - 1,
            64 * 4,
        ),
        (
            "a multisampled pitch under the row",
            0x3b4,
            4,
            DirtyRect::full(64, 32),
            64 * 4 * 32,
            64 * 4 - 1,
        ),
        (
            "a multisampled short buffer",
            0x701,
            4,
            DirtyRect::full(64, 32),
            64 * 4 * 32 - 1,
            64 * 4,
        ),
        (
            "a multisampled rect past the right edge",
            0x702,
            4,
            texels(60, 0, 8, 4),
            64 * 4 * 32,
            64 * 4,
        ),
    ] {
        let mut core = core_with_samples(device, samples);
        core.drain();
        let _ = dxmt_fakes::take_calls(device);
        assert!(!core.write_presented(rect, &bytes[..len], pitch), "{what}");
        core.drain();
        let calls = dxmt_fakes::take_calls(device);
        assert!(calls.is_empty(), "{what}: {calls:#?}");
    }
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { winemetal_sys::Obj::adopt(0x3b5) }.expect("a non-zero handle");
    let mut core = crate::device::CoreDevice::new(mtl, crate::device::queue_profile());
    assert!(!core.write_presented(DirtyRect::full(64, 32), &bytes, 64 * 4));
}

/// A multisampled back buffer write stretches into the companion.
///
/// Uploads into a 1x scratch texture, stretches into the companion, and forgets the scratch.
#[test]
fn a_multisampled_back_buffer_write_stretches_into_companion() {
    const DEVICE: u64 = 0x703;
    let mut core = core_with_samples(DEVICE, 4);
    core.drain();
    let made = back_buffers(&dxmt_fakes::take_calls(DEVICE));
    assert_eq!(made.len(), 2, "{made:?}");
    let (presented, companion) = (made[0].clone(), made[1].clone());

    let sub_pitch = 8 * 4 + 16;
    let sub_bytes = vec![0x7f_u8; sub_pitch * 4 + 8 * 4];
    assert!(core.write_presented(texels(4, 6, 8, 5), &sub_bytes, sub_pitch));
    assert!(
        core.textures.is_empty(),
        "transient scratch must be forgotten immediately"
    );
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let scratches = textures_made(&calls);
    assert_eq!(scratches.len(), 1, "{calls:#?}");
    let scratch_line = calls
        .iter()
        .find(|c| c.starts_with(&format!("MTLDevice_newTexture {}", scratches[0])))
        .expect("scratch texture line");
    assert!(scratch_line.contains(" size=8x5x1 "), "{scratch_line}");
    let copies = copies_to_texture(&calls);
    assert_eq!(copies.len(), 1, "{calls:#?}");
    assert!(
        calls[copies[0]].starts_with(&format!(
            "  blit copy_to_texture {} level=0 slice=0 origin=0,0,0 size=8x5x1 ",
            scratches[0]
        )),
        "{}",
        calls[copies[0]]
    );
    assert!(
        calls[copies[0]].ends_with(" bytes_per_row=32 bytes_per_image=160"),
        "{}",
        calls[copies[0]]
    );
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    let color = line(found[0], "  color0 ").unwrap_or_default();
    assert!(color.starts_with("load=1 store=1 "), "{calls:#?}");
    assert_eq!(targets(&calls)[0].0, companion, "{calls:#?}");
    assert!(
        found[0]
            .iter()
            .any(|c| c.contains("viewports[0].origin_x=0x40800000")
                && c.contains("viewports[0].origin_y=0x40c00000")
                && c.contains("viewports[0].width=0x41000000")
                && c.contains("viewports[0].height=0x40a00000")),
        "{calls:#?}"
    );
    assert!(copies[0] < at(&calls, "MTLCommandBuffer_renderCommandEncoder"));

    // Full-rect write followed by Present resolves the companion into the presented texture.
    let full_bytes = vec![0x33_u8; 64 * 4 * 32];
    assert!(core.write_presented(DirtyRect::full(64, 32), &full_bytes, 64 * 4));
    assert!(core.textures.is_empty());
    core.present(true, 0.0, true);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let drawn = assert_resolved_then_presented(&calls, &companion, &presented);
    assert_eq!(drawn, 1, "resolve folded into stretch pass: {calls:#?}");
    core.present(true, 0.0, true);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let drawn = assert_resolved_then_presented(&calls, &companion, &presented);
    assert_eq!(drawn, 0, "resolve pass of its own: {calls:#?}");
}

/// A write of the back buffer before a Present and one after it are recorded in that order.
#[test]
fn back_buffer_writes_keep_their_order_around_a_present() {
    const DEVICE: u64 = 0x3b6;
    let mut core = core_with_samples(DEVICE, 1);
    let bytes = vec![0_u8; 64 * 4 * 32];
    assert!(core.write_presented(DirtyRect::full(64, 32), &bytes, 64 * 4));
    core.present(true, 0.0, true);
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

/// A resize drains the work recorded before it, then makes the back buffer alone, at its size.
///
/// The Clear's chunk is committed before the new texture is made; nothing else is made (a second
/// presenter would make its gamma table), and the old back buffer is held by nothing.
#[test]
fn a_resize_drains_then_makes_the_back_buffer_alone() {
    const DEVICE: u64 = 0x480;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    let held = core.back_buffer.as_ref().expect("a back buffer");
    let old = std::sync::Arc::downgrade(&held.texture);
    core.clear(0, [0.0, 1.0, 0.0, 1.0], false);
    assert!(core.resize(128, 64, 1));
    let calls = dxmt_fakes::take_calls(DEVICE);
    let commit = at(&calls, "MTLCommandBuffer_commit");
    let made = made_at(&calls);
    assert_eq!(made.len(), 1, "{calls:#?}");
    assert!(made[0].1.contains(" size=128x64x1 "), "{calls:#?}");
    assert!(commit < made[0].0, "{calls:#?}");
    assert!(old.upgrade().is_none(), "the old back buffer is held");
}

/// After a resize a draw targets the new back buffer and a Present shows it at the new size.
///
/// Twice: each resize's texture is the one the draw's pass attaches, at its size, and the one
/// the present samples, into the layer at the size the presenter was last given (its viewport).
#[test]
fn a_resized_back_buffer_is_drawn_and_presented() {
    const DEVICE: u64 = 0x481;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    let binds = ff_binds();
    for (width, height) in [(128_u16, 64_u16), (96, 48)] {
        assert!(core.resize(u32::from(width), u32::from(height), 1));
        core.drain();
        let made = textures_made(&dxmt_fakes::take_calls(DEVICE));
        assert_eq!(made.len(), 1, "{made:?}");
        assert_eq!(
            core.target_size(),
            Some((u32::from(width), u32::from(height)))
        );
        assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
        core.present(true, 0.0, true);
        core.drain();
        let calls = dxmt_fakes::take_calls(DEVICE);
        assert_eq!(
            targets(&calls)[0],
            (made[0].clone(), format!("{width}x{height}")),
            "{calls:#?}"
        );
        assert_presented_at(&calls, &made[0], width, height);
    }
}

/// A resize to four samples makes a companion; one back to one sample drops it.
///
/// Each way only the back buffer's textures are made and target 0 takes the new count: at four
/// the draw goes to the companion, resolved into the new presented texture, which is presented;
/// at one the draw goes to the presented texture, nothing resolves, and the companion is held
/// by nothing.
#[test]
fn a_resize_changes_the_sample_count_both_ways() {
    const DEVICE: u64 = 0x482;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    let binds = ff_binds();
    assert!(core.resize(64, 32, 4));
    core.drain();
    let made = textures_made(&dxmt_fakes::take_calls(DEVICE));
    assert_eq!(made.len(), 2, "{made:?}");
    let (presented, companion) = (made[0].clone(), made[1].clone());
    assert_eq!(core.target_samples(), 4);
    let four = PipelineKey {
        samples: 4,
        ..key(0xf)
    };
    assert_eq!(core.draw(&four, &triangle(&binds)), Ok(()));
    core.present(true, 0.0, true);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let drawn = assert_resolved_then_presented(&calls, &companion, &presented);
    assert_eq!(drawn, 1, "folded into the draw's pass: {calls:#?}");
    assert_presented_at(&calls, &presented, 64, 32);
    let held = core.back_buffer.as_ref().expect("a back buffer");
    let ms = held.companion.as_ref().expect("a companion");
    let old = std::sync::Arc::downgrade(&ms.texture);
    assert!(core.resize(64, 32, 1));
    core.drain();
    let made = textures_made(&dxmt_fakes::take_calls(DEVICE));
    assert_eq!(made.len(), 1, "{made:?}");
    assert!(old.upgrade().is_none(), "the companion is held");
    assert_eq!(core.target_samples(), 1);
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.present(true, 0.0, true);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(targets(&calls)[0].0, made[0], "{calls:#?}");
    assert!(
        !calls.iter().any(|c| c.starts_with("  resolve0 ")),
        "{calls:#?}"
    );
    assert_presented_at(&calls, &made[0], 64, 32);
}

/// A device without a window resizes its back buffer alone, and still presents nothing.
#[test]
fn a_resize_without_a_window_makes_the_back_buffer() {
    const DEVICE: u64 = 0x483;
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { winemetal_sys::Obj::adopt(DEVICE) }.expect("a non-zero handle");
    let mut core = crate::device::CoreDevice::new(mtl, crate::device::queue_profile());
    core.attach(0, 64, 32, 1);
    assert!(core.resize(128, 64, 4));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let resized = calls
        .iter()
        .filter(|c| c.starts_with("MTLDevice_newTexture ") && c.contains(" size=128x64x1 "))
        .count();
    assert_eq!(resized, 2, "{calls:#?}");
    assert_eq!(core.target_size(), Some((128, 64)));
    assert_eq!(core.target_samples(), 4);
    let binds = ff_binds();
    let four = PipelineKey {
        samples: 4,
        ..key(0xf)
    };
    assert_eq!(core.draw(&four, &triangle(&binds)), Ok(()));
    core.present(true, 0.0, true);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    at(&calls, "MTLCommandBuffer_commit");
    assert!(
        !calls
            .iter()
            .any(|c| c == "MTLCommandBuffer_presentDrawable"),
        "{calls:#?}"
    );
}

/// A resize leaves an open OCCLUSION bracket open: its END after the resize is the one counted.
#[test]
fn a_resize_keeps_an_open_occlusion_bracket() {
    const DEVICE: u64 = 0x484;
    let mut core = core_on_fakes(DEVICE);
    let mut query = crate::device::Occlusion::default();
    let binds = ff_binds();
    core.occlusion_begin(&mut query);
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    assert!(core.resize(128, 64, 1));
    assert_eq!(core.open_brackets, Some(1));
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.occlusion_end(&mut query);
    assert_eq!(core.open_brackets, Some(0));
    core.drain();
}

/// A retarget drains, then makes a presenter on the new window and keeps the back buffer.
///
/// The Clear's chunk is committed before the new presenter configures the layer, with no unused
/// gamma texture made; the next Present shows the kept back buffer at its size. To window 0 it
/// answers false and the device presents nothing; to a window again it answers true and presents.
#[test]
fn a_retarget_keeps_the_back_buffer_and_makes_a_presenter() {
    const DEVICE: u64 = 0x485;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let back_buffer = back_buffers(&dxmt_fakes::take_calls(DEVICE))[0].clone();
    core.clear(0, [0.0, 1.0, 0.0, 1.0], false);
    assert!(core.retarget(2));
    let calls = dxmt_fakes::take_calls(DEVICE);
    let commit = at(&calls, "MTLCommandBuffer_commit");
    let made = made_at(&calls);
    assert!(made.is_empty(), "{calls:#?}");
    let set_props = at(
        &calls,
        "MetalLayer_setProps format=80 size=64x32 display_sync=1",
    );
    assert!(commit < set_props, "{calls:#?}");
    core.present(true, 0.0, true);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_presented_at(&calls, &back_buffer, 64, 32);
    assert!(!core.retarget(0));
    core.clear(0, [0.0, 1.0, 0.0, 1.0], false);
    core.present(true, 0.0, true);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    at(&calls, "MTLCommandBuffer_commit");
    assert!(
        !calls
            .iter()
            .any(|c| c == "MTLCommandBuffer_presentDrawable"),
        "{calls:#?}"
    );
    assert!(core.retarget(3));
    core.present(true, 0.0, true);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_presented_at(&calls, &back_buffer, 64, 32);
}

/// Zero-dimension resize rejects the back buffer, updates no layer, and bumps `target_gen`.
///
/// A valid resize afterwards restores `has_back_buffer()`, bumps `target_gen`, and resets
/// `pass_bound`.
#[test]
fn zero_resize_fails_without_layer_update_and_valid_resize_recovers() {
    const DEVICE: u64 = 0x704;
    let mut core = core_on_fakes(DEVICE);
    assert!(core.has_back_buffer());
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    assert_ne!(core.pass_bound.pso, 0, "draw populated pass_bound");
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    let gen_before_fail = core.target_gen();
    assert!(!core.resize(0, 64, 1));
    assert!(!core.has_back_buffer());
    assert_ne!(core.target_gen(), gen_before_fail);
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("MTLDevice_newTexture ") || c.contains("MetalLayer")),
        "zero resize must allocate no texture or update layer: {calls:#?}"
    );

    // Draw populates pass_bound again once back buffer is restored, and resize resets pass_bound.
    let gen_before_ok = core.target_gen();
    assert!(core.resize(128, 64, 1));
    assert!(core.has_back_buffer());
    assert_ne!(core.target_gen(), gen_before_ok);
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    assert_ne!(core.pass_bound.pso, 0);
    let gen_before_reset = core.target_gen();
    assert!(core.resize(96, 48, 1));
    assert_ne!(core.target_gen(), gen_before_reset);
    assert_eq!(
        core.pass_bound.pso, 0,
        "resize drains and resets pass_bound"
    );
    core.drain();
}

/// Asserts `calls` present `presented` into the layer at `width` x `height`.
///
/// The present's pass samples it at index 0, and its viewport is the layer's drawable size: the
/// size the presenter was last given.
fn assert_presented_at(calls: &[String], presented: &str, width: u16, height: u16) {
    let texture = winemetal_sys::wire::WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE;
    let source = format!("  render {texture} texture={presented} index=0");
    let found = passes(calls);
    let pass = found
        .iter()
        .find(|pass| pass.contains(&source))
        .unwrap_or_else(|| panic!("no present of {presented}: {calls:#?}"));
    let (zero, one) = (hex(0.0), hex(1.0));
    let (across, down) = (hex(f32::from(width)), hex(f32::from(height)));
    let viewport = format!(
        "  render 13 viewports=48B viewport_count=1 viewports[0].origin_x={zero} \
         viewports[0].origin_y={zero} viewports[0].width={across} \
         viewports[0].height={down} viewports[0].znear={zero} viewports[0].zfar={one}"
    );
    assert!(pass.contains(&viewport), "{calls:#?}");
}

/// Each texture `calls` shows made: its call's index and its line.
fn made_at(calls: &[String]) -> Vec<(usize, &String)> {
    calls
        .iter()
        .enumerate()
        .filter(|(_, c)| c.starts_with("MTLDevice_newTexture "))
        .collect()
}

/// `value` as the fakes print a float: its bits in hex.
fn hex(value: f32) -> String {
    format!("0x{:08x}", value.to_bits())
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

/// Inline `[u16; 1024]` gamma binds at fragment `buffer(1)` and survives `retarget`.
#[test]
fn inline_gamma_binds_buffer_1_and_survives_retarget() {
    const DEVICE: u64 = 0x486;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    let mut lut1 = [0_u16; 1024];
    lut1[0] = 100;
    let mut lut2 = [0_u16; 1024];
    lut2[0] = 200;

    core.set_gamma_ramp(Some(lut1));
    core.present(true, 0.0, true);
    core.set_gamma_ramp(Some(lut2));
    core.present(true, 0.0, true);
    core.drain();

    let calls = dxmt_fakes::take_calls(DEVICE);
    let psos = calls
        .iter()
        .filter(|c| *c == "MTLDevice_newRenderPipelineState")
        .count();
    assert_eq!(psos, 1, "{calls:#?}");
    let gamma_binds: Vec<&String> = calls
        .iter()
        .filter(|c| c.starts_with("    fragment_bytes index=1 length=2048 fnv="))
        .collect();
    assert_eq!(gamma_binds.len(), 2, "{calls:#?}");
    assert_ne!(gamma_binds[0], gamma_binds[1], "{calls:#?}");

    // Retarget keeps the active gamma ramp.
    assert!(core.retarget(2));
    let _ = dxmt_fakes::take_calls(DEVICE);
    core.present(true, 0.0, true);
    core.drain();
    let retarget_calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        retarget_calls.contains(gamma_binds[1]),
        "gamma LUT must survive retarget: {retarget_calls:#?}"
    );

    // Clearing the ramp stops binding buffer(1).
    core.set_gamma_ramp(None);
    core.present(true, 0.0, true);
    core.drain();
    let cleared_calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        !cleared_calls
            .iter()
            .any(|c| c.contains("fragment_bytes index=1 length=2048")),
        "{cleared_calls:#?}"
    );
}

/// HDR two-step EDR switches layer props/colorspace in order and preserves HDR on `resize`.
#[test]
fn hdr_two_step_edr_switching_and_resize_preservation() {
    use winemetal_sys::{
        Obj,
        wire::{WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_RGBA16_FLOAT, WMTEDRValue},
    };

    use crate::device::{CoreDevice, CoreOptions, queue_profile};

    const DEVICE: u64 = 0x487;
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { Obj::adopt(DEVICE) }.expect("a non-zero handle");
    let mut core = CoreDevice::with_options(mtl, queue_profile(), CoreOptions::HDR_ENABLE);
    core.attach(1, 64, 32, 1);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    // Step 1: potential > 1.0, current == 1.0 -> RGBA16Float then HDR_scRGB, Passthrough (no buffer(0)).
    dxmt_fakes::set_edr_value(
        DEVICE,
        Some(WMTEDRValue {
            maximum_edr_color_component_value: 1.0,
            maximum_potential_edr_color_component_value: 2.0,
        }),
    );
    dxmt_fakes::set_drawable_format(DEVICE, Some(WMT_PIXEL_FORMAT_RGBA16_FLOAT));
    core.present(true, 0.0, true);
    core.drain();
    let step1 = dxmt_fakes::take_calls(DEVICE);
    let set_props = at(
        &step1,
        "MetalLayer_setProps format=115 size=64x32 display_sync=1",
    );
    let set_cs = at(&step1, "MetalLayer_setColorSpace 5");
    assert!(set_props < set_cs, "{step1:#?}");
    assert!(
        !step1
            .iter()
            .any(|c| c.contains("fragment_bytes index=0 length=16")),
        "{step1:#?}"
    );

    // Step 2: current == 2.0 -> no layer change, encodes BT.2446 uniforms at buffer(0).
    dxmt_fakes::set_edr_value(
        DEVICE,
        Some(WMTEDRValue {
            maximum_edr_color_component_value: 2.0,
            maximum_potential_edr_color_component_value: 2.0,
        }),
    );
    core.present(true, 0.0, true);
    core.drain();
    let step2 = dxmt_fakes::take_calls(DEVICE);
    assert!(
        !step2.iter().any(|c| c.starts_with("MetalLayer_")),
        "{step2:#?}"
    );
    assert!(
        step2
            .iter()
            .any(|c| c.starts_with("    fragment_bytes index=0 length=16 fnv=")),
        "{step2:#?}"
    );

    // Resize while in HDR preserves format=115 (RGBA16Float).
    assert!(core.resize(128, 64, 1));
    let resize_calls = dxmt_fakes::take_calls(DEVICE);
    at(
        &resize_calls,
        "MetalLayer_setProps format=115 size=128x64 display_sync=1",
    );

    // Step 3: potential drops to 1.0 -> sRGB colorspace then BGRA8Unorm props.
    dxmt_fakes::set_edr_value(
        DEVICE,
        Some(WMTEDRValue {
            maximum_edr_color_component_value: 1.0,
            maximum_potential_edr_color_component_value: 1.0,
        }),
    );
    dxmt_fakes::set_drawable_format(DEVICE, Some(WMT_PIXEL_FORMAT_BGRA8_UNORM));
    core.present(true, 0.0, true);
    core.drain();
    let step3 = dxmt_fakes::take_calls(DEVICE);
    let set_cs = at(&step3, "MetalLayer_setColorSpace 0");
    let set_props = at(
        &step3,
        "MetalLayer_setProps format=80 size=128x64 display_sync=1",
    );
    assert!(set_cs < set_props, "{step3:#?}");

    dxmt_fakes::set_edr_value(DEVICE, None);
    dxmt_fakes::set_drawable_format(DEVICE, None);
}

/// `present_after` and `display_sync` propagate to the layer and command buffer.
#[test]
fn present_after_and_display_sync_reach_layer_and_cmdbuf() {
    const DEVICE: u64 = 0x488;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);

    core.present(true, 1.0 / 30.0, false);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    at(
        &calls,
        "MetalLayer_setProps format=80 size=64x32 display_sync=0",
    );
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("MTLCommandBuffer_presentDrawableAfterMinimumDuration after=")),
        "{calls:#?}"
    );
}
