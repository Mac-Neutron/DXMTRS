use std::sync::Arc;

use dxmt_core::staging::Staging;
use dxmt_fakes::take_calls;
use mtld3d_core::{dirty_rect::DirtyRect, ids::TextureId};
use winemetal_sys::wire::WMT_PIXEL_FORMAT_RGBA16_UNORM;

use crate::device::{
    CoreDevice, TargetRef, TextureKind,
    fixture::{at, core_on_fakes, target_desc, textures_made},
    target::Shape,
};

/// The device's read-back buffer; panics when it has none.
fn readback(core: &CoreDevice) -> Arc<Staging> {
    Arc::clone(core.readback.as_ref().expect("a read-back buffer"))
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
