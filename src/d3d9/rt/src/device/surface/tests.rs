use mtld3d_core::ids::TextureId;
use winemetal_sys::wire::WMT_PIXEL_FORMAT_RGBA16_UNORM;

use crate::device::{
    TargetRef, TextureKind,
    fixture::{at, core_on_fakes, target_desc, textures_made},
};

/// The read-back blit waits for the render pass that wrote the back buffer.
///
/// It comes after that pass, waits on a fence the pass updates, and the call returns once its
/// chunk is done.
#[test]
fn readback_waits_on_the_render_pass_then_its_seq() {
    const DEVICE: u64 = 0xa6;
    let mut core = core_on_fakes(DEVICE);
    core.clear([0.0, 0.0, 1.0, 1.0], false);
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

/// A destination too small for the back buffer at its pitch is refused.
#[test]
fn readback_refuses_a_short_destination() {
    let mut core = core_on_fakes(0xa7);
    let mut out = vec![0_u8; 16];
    assert!(!core.read_back(TargetRef::BackBuffer, &mut out, 64 * 4));
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
    core.clear([0.0, 1.0, 0.0, 1.0], false);
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
