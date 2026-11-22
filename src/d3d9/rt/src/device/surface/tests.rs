use std::sync::Arc;

use dxmt_core::staging::Staging;
use dxmt_fakes::take_calls;
use mtld3d_core::{dirty_rect::DirtyRect, ids::TextureId};
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE, WMT_PIXEL_FORMAT_BGRA8_UNORM,
    WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_PIXEL_FORMAT_R32_FLOAT, WMT_PIXEL_FORMAT_RGBA8_UNORM,
    WMT_PIXEL_FORMAT_RGBA16_UNORM,
};

use crate::device::{
    CoreDevice, CoreTextureDesc, CoreTextureFlags, TargetRef, TextureKind, UploadRegion,
    fixture::{
        at, core_on_fakes, depth_desc, ff_binds, key, ms_desc, target_desc, targets, textures_made,
        triangle,
    },
    target::Shape,
};

const BGRA8: u32 = WMT_PIXEL_FORMAT_BGRA8_UNORM;

/// The device's read-back buffer; panics when it has none.
fn readback(core: &CoreDevice) -> Arc<Staging> {
    Arc::clone(core.readback.as_ref().expect("a read-back buffer"))
}

/// A stored texture of `desc` under `id`; its creation's calls (a target's zeroing) are taken.
fn stored(core: &mut CoreDevice, device: u64, id: u64, desc: &CoreTextureDesc) -> TextureId {
    let id = TextureId::from_raw(id);
    core.texture_create(id, desc);
    core.drain();
    let _ = take_calls(device);
    id
}

/// A stored square `size` BGRA8 2D render-target texture of `levels` levels, as [`stored`] makes.
fn target_2d(core: &mut CoreDevice, device: u64, id: u64, size: u32, levels: u32) -> TextureId {
    let desc = target_desc(BGRA8, TextureKind::Tex2d, size, levels, 4);
    stored(core, device, id, &desc)
}

/// A square `size` one-level BGRA8 texture that is no render target, of `kind`.
const fn plain_desc(kind: TextureKind, size: u32) -> CoreTextureDesc {
    CoreTextureDesc {
        flags: CoreTextureFlags::empty(),
        ..target_desc(BGRA8, kind, size, 1, 4)
    }
}

/// Level `level` of the 2D texture `id`.
const fn level_of(id: TextureId, level: u32) -> TargetRef {
    TargetRef::Texture {
        id,
        level,
        face: None,
    }
}

/// The fences (`#<n>`) `lines`' records starting with `prefix` (`  blit 7 fence=`) name.
fn fences<'a>(lines: &'a [String], prefix: &str) -> Vec<&'a str> {
    lines
        .iter()
        .filter_map(|c| c.strip_prefix(prefix))
        .filter_map(|c| c.split(' ').next())
        .collect()
}

/// How many texture-to-texture copies (`  blit 4`) `calls` holds.
fn copies(calls: &[String]) -> usize {
    calls.iter().filter(|c| *c == "  blit 4").count()
}

/// A rect of the back buffer is copied at its size and row, after the open pass ends.
///
/// The render pass open at the read (a Clear folded into it) ends before the blit, the copy is
/// the rect's size at its row from the back buffer's texture, and the call returns once its
/// chunk is done.
#[test]
fn a_rect_read_copies_its_size_after_the_open_pass() {
    const DEVICE: u64 = 0x3a0;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let back_buffer = textures_made(&take_calls(DEVICE))[0].clone();
    core.clear(0, [0.0, 0.0, 1.0, 1.0], false);
    core.open_render_pass();
    let mut out = [0_u8; 3 * 4 * 2];
    let seq = core.device().queue().current_seq();
    let rect = DirtyRect {
        x: 8,
        y: 4,
        w: 3,
        h: 2,
    };
    assert!(core.read_presented(rect, &mut out, 3 * 4));
    assert!(core.device().queue().coherent_seq() >= seq);
    let calls = take_calls(DEVICE);
    let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
    let ended = render
        + calls[render..]
            .iter()
            .position(|c| c == "MTLCommandEncoder_endEncoding")
            .unwrap_or_else(|| panic!("the render pass never ends: {calls:#?}"));
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    let copy = at(
        &calls,
        &format!("    copy_to_buffer {back_buffer} level=0 slice=0 size=3x2 bytes_per_row=12"),
    );
    assert!(ended < blit && blit < copy, "{calls:#?}");
}

/// A rect's rows land `dst_pitch` apart, each the rect's row of the read-back buffer.
///
/// The fake copies nothing, so the bytes written into the buffer's mapping are what a 2x2 rect
/// at (5, 3) reads: two 8-byte rows, read 8 bytes apart and written 20 apart, the 12 bytes
/// between them untouched. The buffer is the one the larger whole read before made.
#[test]
fn a_rect_lands_row_by_row() {
    const DEVICE: u64 = 0x3a1;
    let mut core = core_on_fakes(DEVICE);
    let mut whole = vec![0_u8; 64 * 4 * 32];
    assert!(core.read_back(TargetRef::BackBuffer, &mut whole, 64 * 4));
    let staging = readback(&core);
    let mapped = staging.mapped_immediate_memory();
    assert!(!mapped.is_null());
    // SAFETY: the fake buffer's memory, 8192 bytes (the whole read's), which nothing else
    // reaches while this slice lives: the fake's copies write nothing.
    let mapping = unsafe { core::slice::from_raw_parts_mut(mapped, 16) };
    for (byte, value) in mapping.iter_mut().zip(1_u8..) {
        *byte = value;
    }
    let mut out = [0xee_u8; 20 + 8];
    let rect = DirtyRect {
        x: 5,
        y: 3,
        w: 2,
        h: 2,
    };
    assert!(core.read_presented(rect, &mut out, 20));
    assert!(Arc::ptr_eq(&staging, &readback(&core)));
    assert_eq!(out[..8], [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(out[8..20], [0xee; 12]);
    assert_eq!(out[20..], [9, 10, 11, 12, 13, 14, 15, 16]);
}

/// The read-back buffer only grows.
///
/// A rect read before any other makes one of its bytes and is copied; a larger whole read then
/// makes a longer one, which a smaller read after it keeps.
#[test]
fn the_read_back_buffer_only_grows() {
    const DEVICE: u64 = 0x3a2;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let back_buffer = textures_made(&take_calls(DEVICE))[0].clone();
    let rect = DirtyRect {
        x: 1,
        y: 1,
        w: 3,
        h: 2,
    };
    let mut out = vec![0_u8; 64 * 4 * 32];
    assert!(core.read_presented(rect, &mut out, 12));
    let calls = take_calls(DEVICE);
    at(
        &calls,
        &format!("    copy_to_buffer {back_buffer} level=0 slice=0 size=3x2 bytes_per_row=12"),
    );
    let first = readback(&core);
    assert!(first.length() >= 24);
    assert!(core.read_back(TargetRef::BackBuffer, &mut out, 64 * 4));
    let grown = readback(&core);
    assert!(!Arc::ptr_eq(&first, &grown));
    assert!(grown.length() >= 64 * 4 * 32);
    assert!(core.read_presented(rect, &mut out, 12));
    assert!(Arc::ptr_eq(&grown, &readback(&core)));
}

/// The read-back blit waits for the render pass that wrote the back buffer.
///
/// It comes after that pass, waits on a fence the pass updates, and the call returns once its
/// chunk is done.
#[test]
fn readback_waits_on_the_render_pass_then_its_seq() {
    const DEVICE: u64 = 0xa6;
    let mut core = core_on_fakes(DEVICE);
    core.clear(0, [0.0, 0.0, 1.0, 1.0], false);
    let mut out = vec![0_u8; 64 * 4 * 32];
    let seq = core.device().queue().current_seq();
    assert!(core.read_back(TargetRef::BackBuffer, &mut out, 64 * 4));
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

/// A destination too small for the back buffer at its pitch is refused, as is a texel of no bytes.
#[test]
fn readback_refuses_a_short_destination() {
    let mut core = core_on_fakes(0xa7);
    let mut out = vec![0_u8; 16];
    assert!(!core.read_back(TargetRef::BackBuffer, &mut out, 64 * 4));
    let shape = core.shape(TargetRef::BackBuffer).expect("the back buffer");
    let empty = Shape {
        texel_bytes: 0,
        ..shape
    };
    let whole = DirtyRect::full(shape.width, shape.height);
    assert!(super::rect_bytes(empty, whole, 64 * 4 * 32, 64 * 4).is_none());
}

/// A read-back reads the target it names, in rows of that target's texels.
///
/// An RGBA16 16x16 texture's rows are 128 bytes; a pitch under that is refused, and the back
/// buffer is still read at 4 bytes a texel while the texture is the current target.
#[test]
fn read_back_reads_the_named_target_at_its_texel_size() {
    const DEVICE: u64 = 0xfc;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let back_buffer = textures_made(&dxmt_fakes::take_calls(DEVICE))[0].clone();
    let id = TextureId::from_raw(1);
    core.texture_create(
        id,
        &target_desc(WMT_PIXEL_FORMAT_RGBA16_UNORM, TextureKind::Tex2d, 16, 1, 8),
    );
    core.drain();
    let texture = textures_made(&dxmt_fakes::take_calls(DEVICE))[0].clone();
    let target = TargetRef::Texture {
        id,
        level: 0,
        face: None,
    };
    assert!(core.set_target(target));
    let mut out = vec![0_u8; 128 * 16];
    assert!(!core.read_back(target, &mut out, 127));
    assert!(core.read_back(target, &mut out, 128));
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        calls.contains(&format!(
            "    copy_to_buffer {texture} level=0 slice=0 size=16x16 bytes_per_row=128"
        )),
        "{calls:#?}"
    );
    let mut out = vec![0_u8; 64 * 4 * 32];
    assert!(core.read_back(TargetRef::BackBuffer, &mut out, 64 * 4));
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        calls.contains(&format!(
            "    copy_to_buffer {back_buffer} level=0 slice=0 size=64x32 bytes_per_row=256"
        )),
        "{calls:#?}"
    );
    let unknown = TargetRef::Texture {
        id: TextureId::from_raw(7),
        level: 0,
        face: None,
    };
    assert!(!core.read_back(unknown, &mut out, 64 * 4));
}

/// A read-back of a level waits for the pass that wrote that level.
///
/// The copy names the level, and its blit waits on a fence the level's pass updates.
#[test]
fn a_read_back_waits_on_its_subresource() {
    const DEVICE: u64 = 0xfd;
    let mut core = core_on_fakes(DEVICE);
    let id = TextureId::from_raw(1);
    core.texture_create(
        id,
        &target_desc(
            winemetal_sys::wire::WMT_PIXEL_FORMAT_BGRA8_UNORM,
            TextureKind::Tex2d,
            16,
            2,
            4,
        ),
    );
    core.drain();
    let texture = textures_made(&dxmt_fakes::take_calls(DEVICE))
        .last()
        .expect("the texture")
        .clone();
    let level1 = TargetRef::Texture {
        id,
        level: 1,
        face: None,
    };
    assert!(core.set_target(level1));
    core.clear(0, [0.0, 1.0, 0.0, 1.0], false);
    let mut out = vec![0_u8; 8 * 4 * 8];
    assert!(core.read_back(level1, &mut out, 8 * 4));
    let calls = dxmt_fakes::take_calls(DEVICE);
    let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    assert!(render < blit, "{calls:#?}");
    let updated = calls[render..blit]
        .iter()
        .filter_map(|c| c.strip_prefix("  render 34 fence="))
        .filter_map(|c| c.split(' ').next())
        .collect::<Vec<_>>();
    let copy = at(
        &calls,
        &format!("    copy_to_buffer {texture} level=1 slice=0 size=8x8 bytes_per_row=32"),
    );
    let waits = calls[blit..copy]
        .iter()
        .filter_map(|c| c.strip_prefix("  blit 6 fence="))
        .collect::<Vec<_>>();
    assert!(
        waits.iter().any(|w| updated.contains(w)),
        "the blit waits on {waits:?}, the level's pass updates {updated:?}: {calls:#?}"
    );
}

/// A copy's record names each end's handle, level and slice, the source's rect and the origin.
///
/// Sentinel handles and a distinct value in every field: the source's never lands in the
/// destination's, and the size is the rect's, one image deep.
#[test]
fn a_blit_record_names_each_end() {
    let shape = |level, slice| Shape {
        width: 64,
        height: 64,
        pixel_format: BGRA8,
        srgb_format: 0,
        texel_bytes: 4,
        level,
        slice,
        samples: 1,
    };
    let record = super::blit_record(
        (0xa1, &shape(2, 5)),
        [3, 7, 11, 13],
        (0xb2, &shape(1, 4)),
        [17, 19],
    );
    assert_eq!(
        (record.r#type, record.reserved),
        (WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE, [0; 3])
    );
    assert_eq!(
        (record.src, record.src_level, record.src_slice),
        (0xa1, 2, 5)
    );
    let origin = record.src_origin;
    assert_eq!((origin.x, origin.y, origin.z), (3, 7, 0));
    let size = record.src_size;
    assert_eq!((size.width, size.height, size.depth), (11, 13, 1));
    assert_eq!(
        (record.dst, record.dst_level, record.dst_slice),
        (0xb2, 1, 4)
    );
    let origin = record.dst_origin;
    assert_eq!((origin.x, origin.y, origin.z), (17, 19, 0));
}

/// A copy it cannot make is refused, and nothing is recorded.
///
/// Each case is sound but in the one it names. A rect inside level 0 that leaves level 1 is
/// copied from level 0: the rect is in the level's texels.
#[test]
fn a_refused_copy_records_nothing() {
    const DEVICE: u64 = 0x430;
    let mut core = core_on_fakes(DEVICE);
    let mips = target_2d(&mut core, DEVICE, 1, 64, 2);
    let (level0, level1) = (level_of(mips, 0), level_of(mips, 1));
    let flat = plain_desc(TextureKind::Tex2d, 64);
    let plain = level_of(stored(&mut core, DEVICE, 2, &flat), 0);
    let d32f = depth_desc(WMT_PIXEL_FORMAT_DEPTH32_FLOAT, 64, 64, 1);
    let depth = level_of(stored(&mut core, DEVICE, 3, &d32f), 0);
    let volume_desc = CoreTextureDesc {
        depth: 4,
        ..plain_desc(TextureKind::Volume, 16)
    };
    let volume = level_of(stored(&mut core, DEVICE, 4, &volume_desc), 0);
    let ms = level_of(stored(&mut core, DEVICE, 5, &ms_desc(BGRA8, 64, 4)), 0);
    let rgba_desc = target_desc(WMT_PIXEL_FORMAT_RGBA8_UNORM, TextureKind::Tex2d, 64, 1, 4);
    let rgba = level_of(stored(&mut core, DEVICE, 6, &rgba_desc), 0);
    let (bb, unknown) = (TargetRef::BackBuffer, level_of(TextureId::from_raw(9), 0));
    let (rect, zero) = ([0, 0, 8, 8], [0, 0]);
    for (what, src, src_rect, dst, origin) in [
        ("an unknown source", unknown, rect, bb, zero),
        ("an unknown destination", bb, rect, unknown, zero),
        ("a depth source", depth, rect, bb, zero),
        ("a volume source", volume, rect, bb, zero),
        ("a non-target destination", bb, rect, plain, zero),
        ("a multisampled texture source", ms, rect, bb, zero),
        ("a multisampled destination", bb, rect, ms, zero),
        ("unequal formats", rgba, rect, bb, zero),
        ("no width", bb, [0, 0, 0, 8], level0, zero),
        ("no height", bb, [0, 0, 8, 0], level0, zero),
        ("a rect past level 1", level1, [20, 20, 20, 20], bb, zero),
        ("an origin past level 1", bb, rect, level1, [28, 0]),
        ("an origin past u32", bb, rect, level0, [u32::MAX, 0]),
        ("overlapping rects", level0, [0, 0, 32, 32], level0, [9, 9]),
        ("identical rects", level0, [8, 8, 16, 16], level0, [8, 8]),
        ("back-buffer overlap", bb, [0, 0, 32, 16], bb, [8, 4]),
    ] {
        assert!(!core.copy(src, src_rect, dst, origin), "{what}");
        core.drain();
        let calls = take_calls(DEVICE);
        assert!(calls.is_empty(), "{what}: {calls:#?}");
    }
    assert!(core.copy(level0, [20, 20, 20, 20], bb, zero));
}

/// A copy ends the open render pass and is one copy in a blit pass after the pass's end.
#[test]
fn a_copy_ends_the_open_render_pass() {
    const DEVICE: u64 = 0x431;
    let mut core = core_on_fakes(DEVICE);
    let target = level_of(target_2d(&mut core, DEVICE, 1, 64, 1), 0);
    core.clear(0, [0.0, 0.0, 1.0, 1.0], false);
    core.open_render_pass();
    assert!(core.copy(TargetRef::BackBuffer, [8, 4, 13, 9], target, [5, 6]));
    assert!(!core.render_open && core.blit_open);
    core.drain();
    let calls = take_calls(DEVICE);
    let end = |from: usize| {
        from + calls[from..]
            .iter()
            .position(|c| c == "MTLCommandEncoder_endEncoding")
            .unwrap_or_else(|| panic!("no end after {from}: {calls:#?}"))
    };
    let render_end = end(at(&calls, "MTLCommandBuffer_renderCommandEncoder"));
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    let copy = at(&calls, "  blit 4");
    let order = [render_end, blit, copy, end(blit)];
    assert!(order.is_sorted(), "{order:?}: {calls:#?}");
    assert_eq!(copies(&calls), 1, "{calls:#?}");
}

/// A multisampled back buffer is copied from as Present shows it: resolved, then copied.
///
/// Its companion is resolved into the presented texture, a pass of its own, and the one copy, in
/// a blit pass after that resolve, reads the presented texture.
#[test]
fn a_multisampled_back_buffer_is_copied_resolved() {
    const DEVICE: u64 = 0x436;
    let mut core = core_on_fakes(DEVICE);
    let target = level_of(target_2d(&mut core, DEVICE, 1, 64, 1), 0);
    core.attach(0, 64, 32, 4);
    core.drain();
    let made = textures_made(&take_calls(DEVICE));
    let (presented, companion) = (made[0].clone(), made[1].clone());
    assert!(core.copy(TargetRef::BackBuffer, [8, 4, 13, 9], target, [5, 6]));
    core.drain();
    let calls = take_calls(DEVICE);
    // The resolve pass's one target; a resolve names no pass size.
    let attached = targets(&calls)
        .into_iter()
        .map(|(handle, _)| handle)
        .collect::<Vec<_>>();
    assert_eq!(attached, [companion], "{calls:#?}");
    let resolved = at(&calls, &format!("  resolve0 {presented}"));
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    let copy = at(&calls, "  blit 4");
    assert!(resolved < blit && blit < copy, "{calls:#?}");
    assert_eq!(copies(&calls), 1, "{calls:#?}");
}

/// A copy from a texture that is no target follows the upload before it, in its blit pass.
#[test]
fn a_copy_from_a_texture_no_target_follows_its_upload() {
    const DEVICE: u64 = 0x432;
    let mut core = core_on_fakes(DEVICE);
    let plain = stored(&mut core, DEVICE, 1, &plain_desc(TextureKind::Tex2d, 16));
    let region = UploadRegion {
        level: 0,
        slice: 0,
        x: 0,
        y: 0,
        z: 0,
        width: 16,
        height: 16,
        depth: 1,
    };
    assert!(core.texture_upload(plain, region, &[0; 16 * 16 * 4], 16 * 4, 16 * 16 * 4));
    let source = level_of(plain, 0);
    assert!(core.copy(source, [2, 3, 9, 7], TargetRef::BackBuffer, [30, 20]));
    core.drain();
    let calls = take_calls(DEVICE);
    let blits = calls
        .iter()
        .filter(|c| *c == "MTLCommandBuffer_blitCommandEncoder")
        .count();
    assert_eq!(blits, 1, "{calls:#?}");
    let upload = calls
        .iter()
        .position(|c| c.starts_with("  blit copy_to_texture "))
        .unwrap_or_else(|| panic!("no upload in {calls:#?}"));
    assert!(upload < at(&calls, "  blit 4"), "{calls:#?}");
    assert_eq!(copies(&calls), 1, "{calls:#?}");
}

/// One texture copies into itself where the texels differ, one copy each.
///
/// Level 0 into level 1, two disjoint rects of one level, two of the back buffer.
#[test]
fn one_texture_copies_into_other_texels_of_itself() {
    const DEVICE: u64 = 0x433;
    let mut core = core_on_fakes(DEVICE);
    let mips = target_2d(&mut core, DEVICE, 1, 64, 2);
    let (level0, level1) = (level_of(mips, 0), level_of(mips, 1));
    let bb = TargetRef::BackBuffer;
    for (what, src, src_rect, dst, origin) in [
        ("two levels", level0, [8, 8, 20, 20], level1, [3, 4]),
        ("one level", level0, [0, 0, 16, 16], level0, [32, 32]),
        ("the back buffer", bb, [0, 0, 16, 16], bb, [40, 8]),
    ] {
        assert!(core.copy(src, src_rect, dst, origin), "{what}");
        core.drain();
        let calls = take_calls(DEVICE);
        assert_eq!(copies(&calls), 1, "{what}: {calls:#?}");
    }
}

/// A copy from a level waits for the pass that wrote that level.
///
/// A Clear of level 1 of a render-target texture, then a copy from that level into the back
/// buffer: the copy's blit pass waits on a fence the Clear's pass updates.
#[test]
fn a_copy_waits_on_its_source_subresource() {
    const DEVICE: u64 = 0x434;
    let mut core = core_on_fakes(DEVICE);
    let level1 = level_of(target_2d(&mut core, DEVICE, 1, 64, 2), 1);
    assert!(core.set_target(level1));
    core.clear(0, [0.0, 1.0, 0.0, 1.0], false);
    assert!(core.copy(level1, [2, 3, 10, 9], TargetRef::BackBuffer, [5, 6]));
    core.drain();
    let calls = take_calls(DEVICE);
    let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    assert!(render < blit, "{calls:#?}");
    // The fences the Clear's pass updates (record 34), and those the blit waits for (6) before
    // its texture-to-texture copy (4).
    let updated = fences(&calls[render..blit], "  render 34 fence=");
    let waits = fences(&calls[blit..at(&calls, "  blit 4")], "  blit 6 fence=");
    assert!(
        waits.iter().any(|w| updated.contains(w)),
        "the copy waits on {waits:?}, the level's Clear updates {updated:?}: {calls:#?}"
    );
}

/// A draw on a copy's destination waits for the copy.
///
/// A copy into level 1 of a render-target texture, and one into face 3 of a cube, each then
/// drawn on: the draw's pass waits on a fence the copy's blit pass updates. Each target holds
/// the draw's viewport.
#[test]
fn a_draw_on_a_copied_subresource_waits_on_the_copy() {
    const DEVICE: u64 = 0x435;
    let mut core = core_on_fakes(DEVICE);
    let level1 = level_of(target_2d(&mut core, DEVICE, 1, 128, 2), 1);
    let cube_desc = target_desc(BGRA8, TextureKind::Cube, 64, 1, 4);
    let face3 = TargetRef::Texture {
        id: stored(&mut core, DEVICE, 2, &cube_desc),
        level: 0,
        face: Some(3),
    };
    let binds = ff_binds();
    for (what, dst) in [("level 1", level1), ("face 3", face3)] {
        let copied = core.copy(TargetRef::BackBuffer, [4, 2, 16, 16], dst, [7, 9]);
        assert!(copied, "{what}");
        assert!(core.set_target(dst), "{what}");
        assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()), "{what}");
        core.drain();
        let calls = take_calls(DEVICE);
        let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
        let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
        assert!(blit < render, "{what}: {calls:#?}");
        // The fences the copy's blit pass updates (record 7), and those the draw's pass waits
        // for (33).
        let updated = fences(&calls[blit..render], "  blit 7 fence=");
        let waits = fences(&calls[render..], "  render 33 fence=");
        assert!(
            waits.iter().any(|w| updated.contains(w)),
            "{what}: the draw waits on {waits:?}, the copy updates {updated:?}: {calls:#?}"
        );
    }
}

#[test]
fn read_back_logical_and_write_target_logical_resample_scaled_targets() {
    const DEVICE: u64 = 0x49a;
    let mut core = core_on_fakes(DEVICE);
    let desc = target_desc(BGRA8, TextureKind::Tex2d, 128, 1, 4);
    let id = TextureId::from_raw(0x99);
    core.texture_create(id, &desc);
    core.set_texture_logical_size(id, 256, 128);
    core.drain();
    let _ = take_calls(DEVICE);

    let target = TargetRef::Texture {
        id,
        level: 0,
        face: None,
    };
    let rows = vec![0x55_u8; 256 * 128 * 4];
    let whole_logical = DirtyRect::full(256, 128);
    assert!(core.write_target_logical(target, (256, 128), whole_logical, &rows, 256 * 4));
    core.drain();
    let write_calls = take_calls(DEVICE);
    assert!(
        write_calls
            .iter()
            .any(|c| c == "MTLCommandBuffer_renderCommandEncoder"),
        "scaled write_target_logical must stretch into physical texture: {write_calls:#?}"
    );

    let mut read_buf = vec![0_u8; 256 * 128 * 4];
    assert!(core.read_back_logical(target, 256, 128, &mut read_buf, 256 * 4));
    core.drain();
    let read_calls = take_calls(DEVICE);
    assert!(
        read_calls
            .iter()
            .any(|c| c == "MTLCommandBuffer_renderCommandEncoder"),
        "scaled read_back_logical must stretch into logical scratch first: {read_calls:#?}"
    );
    assert!(
        read_calls
            .iter()
            .any(|c| c.trim_start().starts_with("copy_to_buffer ") && c.contains("size=256x128 ")),
        "{read_calls:#?}"
    );
    assert_eq!(readback(&core).length(), 256_u64 * 128 * 4);
}

#[test]
fn r32_float_logical_read_and_write_use_nearest_sampler() {
    const DEVICE_WRITE: u64 = 0x437;
    const DEVICE_READ: u64 = 0x438;
    const DEVICE_MSAA: u64 = 0x439;
    let desc = target_desc(WMT_PIXEL_FORMAT_R32_FLOAT, TextureKind::Tex2d, 128, 1, 4);

    let mut core_w = core_on_fakes(DEVICE_WRITE);
    let id_w = TextureId::from_raw(0x9a);
    core_w.texture_create(id_w, &desc);
    core_w.set_texture_logical_size(id_w, 256, 128);
    core_w.drain();
    let _ = take_calls(DEVICE_WRITE);

    let target_w = level_of(id_w, 0);
    let rows = vec![0_u8; 256 * 128 * 4];
    let whole_logical = DirtyRect::full(256, 128);
    assert!(core_w.write_target_logical(target_w, (256, 128), whole_logical, &rows, 256 * 4));
    core_w.drain();
    let write_calls = take_calls(DEVICE_WRITE);
    let write_samplers: Vec<_> = write_calls
        .iter()
        .filter(|c| c.starts_with("MTLDevice_newSamplerState "))
        .collect();
    assert!(!write_samplers.is_empty(), "{write_calls:#?}");
    assert!(
        write_samplers
            .iter()
            .all(|c| c.contains("min=0 mag=0 ") && !c.contains("min=1") && !c.contains("mag=1")),
        "write_target_logical on R32Float must use nearest sampler: {write_samplers:#?}"
    );

    let mut core_r = core_on_fakes(DEVICE_READ);
    let id_r = TextureId::from_raw(0x9b);
    core_r.texture_create(id_r, &desc);
    core_r.set_texture_logical_size(id_r, 256, 128);
    core_r.drain();
    let _ = take_calls(DEVICE_READ);

    let target_r = level_of(id_r, 0);
    let mut read_buf = vec![0_u8; 256 * 128 * 4];
    assert!(core_r.read_back_logical(target_r, 256, 128, &mut read_buf, 256 * 4));
    core_r.drain();
    let read_calls = take_calls(DEVICE_READ);
    let read_samplers: Vec<_> = read_calls
        .iter()
        .filter(|c| c.starts_with("MTLDevice_newSamplerState "))
        .collect();
    assert!(!read_samplers.is_empty(), "{read_calls:#?}");
    assert!(
        read_samplers
            .iter()
            .all(|c| c.contains("min=0 mag=0 ") && !c.contains("min=1") && !c.contains("mag=1")),
        "read_back_logical on R32Float must use nearest sampler: {read_samplers:#?}"
    );

    let mut core_ms = core_on_fakes(DEVICE_MSAA);
    let id_ms = stored(&mut core_ms, DEVICE_MSAA, 0x9c, &ms_desc(BGRA8, 64, 4));
    let rows_ms = vec![0_u8; 64 * 64 * 4];
    assert!(core_ms.write_target_logical(
        level_of(id_ms, 0),
        (64, 64),
        DirtyRect::full(64, 64),
        &rows_ms,
        64 * 4,
    ));
    core_ms.drain();
    let ms_calls = take_calls(DEVICE_MSAA);
    let ms_samplers: Vec<_> = ms_calls
        .iter()
        .filter(|c| c.starts_with("MTLDevice_newSamplerState "))
        .collect();
    assert!(!ms_samplers.is_empty(), "{ms_calls:#?}");
    assert!(
        ms_samplers
            .iter()
            .all(|c| c.contains("min=0 mag=0 ") && !c.contains("min=1") && !c.contains("mag=1")),
        "1:1 MSAA write_target_logical must use nearest sampler: {ms_samplers:#?}"
    );
}
