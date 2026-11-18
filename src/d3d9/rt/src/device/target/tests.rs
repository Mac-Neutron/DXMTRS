use std::sync::Arc;

use dxmt_core::{
    deptrack::Access,
    encoder::stage,
    resource::{Texture, TextureAllocationFlags, TextureViewKey, texture_info},
};
use mtld3d_core::{
    ids::TextureId,
    pipeline_key::{CoreExtra, PipelineKey},
};
use winemetal_sys::wire::{
    WMT_LOAD_ACTION_LOAD, WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB,
    WMT_PIXEL_FORMAT_R32_FLOAT, WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_PIXEL_FORMAT_RGBA8_UNORM_S_RGB,
    WMT_PIXEL_FORMAT_RGBA16_FLOAT, WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_STORE_ACTION_STORE,
    WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW,
    WMT_TEXTURE_USAGE_RENDER_TARGET, WMT_TEXTURE_USAGE_SHADER_READ, WMTTextureInfo,
};

use super::{Resolved, TargetRef};
use crate::device::{
    CoreDevice, CoreTextureDesc, CoreTextureFlags, DepthRef, DrawData, DrawRefused, Stage,
    TextureKind, UploadRegion,
    fixture::{
        at, core_on_fakes, depth_desc, ff_binds, key, key_extras, line, ms_desc, passes,
        target_desc, targets, textures_made, triangle,
    },
};

/// A core device on `device` and its back buffer's handle, the calls so far taken.
fn core_and_back_buffer(device: u64) -> (CoreDevice, String) {
    let mut core = core_on_fakes(device);
    core.drain();
    let made = textures_made(&dxmt_fakes::take_calls(device));
    (core, made[0].clone())
}

/// A stored BGRA8 render-target texture of `kind`, and its handle; its creation's calls taken.
fn stored(
    core: &mut CoreDevice,
    device: u64,
    id: u64,
    kind: TextureKind,
    size: u32,
    levels: u32,
) -> (TextureId, String) {
    let desc = target_desc(WMT_PIXEL_FORMAT_BGRA8_UNORM, kind, size, levels, 4);
    stored_desc(core, device, id, &desc)
}

/// A stored texture of `desc`, and its handle; its creation's calls taken.
fn stored_desc(
    core: &mut CoreDevice,
    device: u64,
    id: u64,
    desc: &CoreTextureDesc,
) -> (TextureId, String) {
    let id = TextureId::from_raw(id);
    core.texture_create(id, desc);
    core.drain();
    let made = textures_made(&dxmt_fakes::take_calls(device));
    (id, made[0].clone())
}

/// The whole of a 2D texture's level 0.
const fn level0(id: TextureId) -> TargetRef {
    TargetRef::Texture {
        id,
        level: 0,
        face: None,
    }
}

/// A render-target texture is made with the render-target usage and every level zeroed.
///
/// The zeroing is the core initializer's (a cleared pass per subresource, committed ahead of the
/// next chunk), not passes of the chunk being recorded.
#[test]
fn a_render_target_texture_has_the_usage_and_starts_zeroed() {
    const DEVICE: u64 = 0xf3;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    core.texture_create(
        TextureId::from_raw(1),
        &target_desc(WMT_PIXEL_FORMAT_BGRA8_UNORM, TextureKind::Tex2d, 64, 2, 4),
    );
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let made = calls
        .iter()
        .find(|c| c.starts_with("MTLDevice_newTexture "))
        .unwrap_or_else(|| panic!("{calls:#?}"));
    let bits = made
        .rsplit_once("bits=0x")
        .and_then(|(_, hex)| u32::from_str_radix(hex, 16).ok())
        .unwrap_or_else(|| panic!("{made}"));
    assert_ne!(
        (bits >> WMTTextureInfo::USAGE_SHIFT) & WMT_TEXTURE_USAGE_RENDER_TARGET,
        0,
        "{made}"
    );
    let zeroed = calls
        .iter()
        .filter(|c| *c == "  color0 load=2 store=1 clear=0,0,0,0")
        .count();
    assert_eq!(zeroed, 2, "{calls:#?}");
    let sizes = targets(&calls)
        .into_iter()
        .map(|(_, size)| size)
        .collect::<Vec<_>>();
    assert_eq!(sizes, ["64x64", "32x32"], "{calls:#?}");
}

/// A change of target ends the open pass; setting the current target again does not.
#[test]
fn set_target_closes_the_open_pass() {
    const DEVICE: u64 = 0xf4;
    let (mut core, back_buffer) = core_and_back_buffer(DEVICE);
    let (id, texture) = stored(&mut core, DEVICE, 1, TextureKind::Tex2d, 64, 1);
    core.open_render_pass();
    assert!(core.set_target(level0(id)));
    core.open_render_pass();
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let handles = targets(&calls)
        .into_iter()
        .map(|(handle, _)| handle)
        .collect::<Vec<_>>();
    assert_eq!(handles, [back_buffer, texture.clone()], "{calls:#?}");
    core.open_render_pass();
    assert!(core.set_target(level0(id)));
    core.open_render_pass();
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        targets(&calls),
        [(texture, "64x64".to_owned())],
        "{calls:#?}"
    );
}

/// A Clear of a texture target becomes the load action of the next pass on it.
#[test]
fn a_clear_folds_into_the_targets_pass() {
    const DEVICE: u64 = 0xf5;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (id, texture) = stored(&mut core, DEVICE, 1, TextureKind::Tex2d, 64, 1);
    assert!(core.set_target(level0(id)));
    core.clear(0, [1.0, 0.0, 0.0, 1.0], false);
    core.open_render_pass();
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        targets(&calls),
        [(texture, "64x64".to_owned())],
        "{calls:#?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c == "  color0 load=2 store=1 clear=1,0,0,1"),
        "{calls:#?}"
    );
}

/// A level and a cube face resolve to a view of that one subresource, at the level's size.
///
/// What names no render-target subresource does not resolve and leaves the target as it was.
#[test]
fn a_level_and_a_face_have_their_own_views() {
    const DEVICE: u64 = 0xf6;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (flat, flat_handle) = stored(&mut core, DEVICE, 1, TextureKind::Tex2d, 64, 3);
    let (cube, cube_handle) = stored(&mut core, DEVICE, 2, TextureKind::Cube, 16, 1);
    let level1 = TargetRef::Texture {
        id: flat,
        level: 1,
        face: None,
    };
    assert!(core.set_target(level1));
    assert_eq!(core.target_size(), Some((32, 32)));
    assert_eq!(core.target_format(), Some(WMT_PIXEL_FORMAT_BGRA8_UNORM));
    core.open_render_pass();
    assert!(core.set_target(TargetRef::Texture {
        id: cube,
        level: 0,
        face: Some(3),
    }));
    assert_eq!(core.target_size(), Some((16, 16)));
    core.open_render_pass();
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let view = |parent: &str, mip: u32, slice: u32| {
        let want = format!(
            " of {parent} format={WMT_PIXEL_FORMAT_BGRA8_UNORM} type={WMT_TEXTURE_TYPE2D} mip={mip} \
             slice={slice}"
        );
        calls
            .iter()
            .filter_map(|c| c.strip_prefix("MTLTexture_newTextureView "))
            .find_map(|c| c.strip_suffix(want.as_str()))
            .unwrap_or_else(|| panic!("no view{want}: {calls:#?}"))
            .to_owned()
    };
    assert_eq!(
        targets(&calls),
        [
            (view(&flat_handle, 1, 0), "32x32".to_owned()),
            (view(&cube_handle, 0, 3), "16x16".to_owned()),
        ],
        "{calls:#?}"
    );
    let plain = TextureId::from_raw(3);
    let mut desc = target_desc(WMT_PIXEL_FORMAT_BGRA8_UNORM, TextureKind::Tex2d, 8, 1, 4);
    desc.flags.remove(CoreTextureFlags::RENDER_TARGET);
    core.texture_create(plain, &desc);
    let current = core.target();
    for (what, target) in [
        (
            "a face of a 2D texture",
            TargetRef::Texture {
                id: flat,
                level: 0,
                face: Some(0),
            },
        ),
        ("a cube without a face", level0(cube)),
        (
            "a face past the sixth",
            TargetRef::Texture {
                id: cube,
                level: 0,
                face: Some(6),
            },
        ),
        (
            "a level past the last",
            TargetRef::Texture {
                id: flat,
                level: 3,
                face: None,
            },
        ),
        ("an id the store lacks", level0(TextureId::from_raw(9))),
        ("a texture that is no render target", level0(plain)),
    ] {
        assert!(!core.set_target(target), "{what}");
        assert_eq!(core.target(), current, "{what}");
    }
    assert_eq!(core.target_size(), Some((16, 16)));
}

/// Forgetting the current target's texture falls back to the back buffer and ends its pass.
#[test]
fn forgetting_the_current_target_falls_back_to_the_back_buffer() {
    const DEVICE: u64 = 0xf7;
    let (mut core, back_buffer) = core_and_back_buffer(DEVICE);
    let (id, texture) = stored(&mut core, DEVICE, 1, TextureKind::Tex2d, 64, 1);
    assert!(core.set_target(level0(id)));
    core.open_render_pass();
    core.forget_texture(id);
    assert_eq!(core.target(), TargetRef::BackBuffer);
    assert_eq!(core.target_size(), Some((64, 32)));
    core.open_render_pass();
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let handles = targets(&calls)
        .into_iter()
        .map(|(handle, _)| handle)
        .collect::<Vec<_>>();
    assert_eq!(handles, [texture, back_buffer], "{calls:#?}");
}

/// Without a back buffer the back-buffer target does not resolve.
#[test]
fn no_back_buffer_is_no_target() {
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { winemetal_sys::Obj::adopt(0xf8) }.expect("a non-zero handle");
    let core = CoreDevice::new(mtl, crate::device::queue_profile());
    assert_eq!(core.target(), TargetRef::BackBuffer);
    assert_eq!(core.target_size(), None);
    assert_eq!(core.target_format(), None);
}

const BGRA8: u32 = WMT_PIXEL_FORMAT_BGRA8_UNORM;
const SRGB: u32 = WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB;

/// The fixture's draw with a key that names `format` as the colour target's.
fn draw_in(core: &mut CoreDevice, format: u32) {
    let binds = ff_binds();
    let key = PipelineKey {
        color_format: format,
        ..key(0xf)
    };
    assert_eq!(core.draw(&key, &triangle(&binds)), Ok(()));
}

/// Whether `handle` is a view of `texture` in `format`, by the views `calls` show made.
fn is_view(calls: &[String], handle: &str, texture: &str, format: u32) -> bool {
    let made = format!("MTLTexture_newTextureView {handle} of {texture} format={format} ");
    calls.iter().any(|c| c.starts_with(&made))
}

/// A draw whose key names the sRGB twin draws to the back buffer's sRGB view (DXMTRS C4b-3d3).
///
/// Its pipeline names that format; a key with the stored format draws to the texture itself.
#[test]
fn srgb_write_attaches_the_twin() {
    const DEVICE: u64 = 0x320;
    let (mut core, back_buffer) = core_and_back_buffer(DEVICE);
    assert_eq!(core.target_desc(false), Some(((64, 32), [BGRA8, 0, 0, 0])));
    assert_eq!(core.target_desc(true), Some(((64, 32), [SRGB, 0, 0, 0])));
    assert_eq!(core.target_format(), Some(BGRA8));
    draw_in(&mut core, SRGB);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let drawn = targets(&calls);
    assert_eq!(drawn.len(), 1, "{calls:#?}");
    assert!(
        is_view(&calls, &drawn[0].0, &back_buffer, SRGB),
        "{calls:#?}"
    );
    assert!(
        calls.contains(&format!("  pso color0={SRGB}")),
        "{calls:#?}"
    );
    draw_in(&mut core, BGRA8);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        targets(&calls),
        [(back_buffer, "64x32".to_owned())],
        "{calls:#?}"
    );
}

/// A draw on the other view of the target ends the open pass; one on the same view does not.
#[test]
fn a_format_switch_ends_the_pass() {
    const DEVICE: u64 = 0x321;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    for format in [BGRA8, SRGB, SRGB, BGRA8] {
        draw_in(&mut core, format);
    }
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&calls).len(), 3, "{calls:#?}");
}

/// A target made with no sRGB twin keeps its format under sRGB write; one made with it takes it.
#[test]
fn a_target_without_a_twin_keeps_its_format() {
    const DEVICE: u64 = 0x322;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (plain, _) = stored(&mut core, DEVICE, 1, TextureKind::Tex2d, 64, 1);
    assert!(core.set_target(level0(plain)));
    assert_eq!(core.target_desc(true), Some(((64, 64), [BGRA8, 0, 0, 0])));
    let twin = TextureId::from_raw(2);
    let mut desc = target_desc(BGRA8, TextureKind::Tex2d, 64, 1, 4);
    desc.flags.insert(CoreTextureFlags::VIEW_TWINS);
    desc.srgb_format = SRGB;
    core.texture_create(twin, &desc);
    assert!(core.set_target(level0(twin)));
    assert_eq!(core.target_desc(true), Some(((64, 64), [SRGB, 0, 0, 0])));
    assert_eq!(core.target_desc(false), Some(((64, 64), [BGRA8, 0, 0, 0])));
}

/// A Clear and a draw made under sRGB write are one pass: the Clear is its load action.
#[test]
fn a_clear_and_its_pass_share_the_view() {
    const DEVICE: u64 = 0x323;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    core.clear(0, [0.0, 0.0, 1.0, 1.0], true);
    draw_in(&mut core, SRGB);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    assert_eq!(
        line(all[0], "  color0 "),
        Some("load=2 store=1 clear=0,0,1,1"),
        "{calls:#?}"
    );
}

/// A Clear on one view and a draw on the other are two passes: the draw loads what was cleared.
///
/// One more pass than a Clear and a draw of one state (ruling 7: C4d's to remove).
#[test]
fn a_clear_on_the_other_view_is_its_own_pass() {
    const DEVICE: u64 = 0x324;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    core.clear(0, [0.0, 0.0, 1.0, 1.0], false);
    draw_in(&mut core, SRGB);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let all = passes(&calls);
    assert_eq!(all.len(), 2, "{calls:#?}");
    assert_eq!(
        line(all[0], "  color0 "),
        Some("load=2 store=1 clear=0,0,1,1"),
        "{calls:#?}"
    );
    let drawn = line(all[1], "  color0 ").unwrap_or_default();
    assert!(drawn.starts_with("load=1 store=1"), "{calls:#?}");
}

/// A read-back after a draw through the sRGB view reads the stored texture, after that pass.
#[test]
fn read_back_reads_the_stored_view() {
    const DEVICE: u64 = 0x325;
    let (mut core, back_buffer) = core_and_back_buffer(DEVICE);
    draw_in(&mut core, SRGB);
    let mut out = vec![0_u8; 64 * 4 * 32];
    assert!(core.read_back(TargetRef::BackBuffer, &mut out, 64 * 4));
    let calls = dxmt_fakes::take_calls(DEVICE);
    let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    assert!(render < blit, "{calls:#?}");
    let copied = calls[blit..]
        .iter()
        .find_map(|c| c.trim_start().strip_prefix("copy_to_buffer "))
        .unwrap_or_else(|| panic!("{calls:#?}"));
    assert!(copied.starts_with(&format!("{back_buffer} ")), "{calls:#?}");
}

const RGBA8: u32 = WMT_PIXEL_FORMAT_RGBA8_UNORM;
const RGBA8_SRGB: u32 = WMT_PIXEL_FORMAT_RGBA8_UNORM_S_RGB;
const RGBA16F: u32 = WMT_PIXEL_FORMAT_RGBA16_FLOAT;
const R32F: u32 = WMT_PIXEL_FORMAT_R32_FLOAT;

/// An extra of `format` written through `write_mask`, unblended.
const fn extra(format: u32, write_mask: u8) -> CoreExtra {
    CoreExtra {
        format,
        write_mask,
        blend: None,
    }
}

/// A square `size` one-level 2D render target of `format`, `texel_bytes` a texel.
const fn flat(format: u32, size: u32, texel_bytes: u32) -> CoreTextureDesc {
    target_desc(format, TextureKind::Tex2d, size, 1, texel_bytes)
}

/// `desc` with an sRGB twin of `srgb_format`.
const fn twinned(mut desc: CoreTextureDesc, srgb_format: u32) -> CoreTextureDesc {
    desc.flags = desc.flags.union(CoreTextureFlags::VIEW_TWINS);
    desc.srgb_format = srgb_format;
    desc
}

/// Target 0 a stored 64x64 BGRA8 texture, made first: the fixture's back buffer is 64x32.
fn core_on_a_64_target(device: u64) -> (CoreDevice, TextureId) {
    let (mut core, _) = core_and_back_buffer(device);
    let (t0, _) = stored(&mut core, device, 1, TextureKind::Tex2d, 64, 1);
    assert!(core.set_target(level0(t0)));
    (core, t0)
}

/// The draws `calls` shows recorded.
fn draws(calls: &[String]) -> usize {
    calls
        .iter()
        .filter(|c| c.starts_with("  render 19 "))
        .count()
}

/// The `target<slot>` handle of each render pass in `calls` that attaches `slot`, in order.
fn slot_targets(calls: &[String], slot: usize) -> Vec<String> {
    let prefix = format!("  target{slot} ");
    passes(calls)
        .into_iter()
        .filter_map(|pass| line(pass, &prefix).map(str::to_owned))
        .collect()
}

/// A draw's key names the slots its pass attaches, in its formats, with its masks.
///
/// The same targets with a key naming none of them is refused: the key decides the pass.
#[test]
fn the_key_names_the_pass() {
    const DEVICE: u64 = 0x340;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (t1, h1) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 64, 4));
    let (t2, h2) = stored_desc(&mut core, DEVICE, 3, &flat(RGBA16F, 64, 8));
    assert!(core.set_extra_target(1, Some(level0(t1))));
    assert!(core.set_extra_target(2, Some(level0(t2))));
    assert_eq!(
        core.target_desc(false),
        Some(((64, 64), [BGRA8, RGBA8, RGBA16F, 0]))
    );
    let binds = ff_binds();
    let both = key_extras([Some(extra(RGBA8, 8)), Some(extra(RGBA16F, 6)), None]);
    assert_eq!(core.draw(&both, &triangle(&binds)), Ok(()));
    assert_eq!(
        core.draw(&key(0xf), &triangle(&binds)),
        Err(DrawRefused::Target)
    );
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    for slot in [1, 2] {
        let color = line(all[0], &format!("  color{slot} ")).unwrap_or_default();
        assert!(color.starts_with("load=1 store=1"), "{calls:#?}");
    }
    assert_eq!(line(all[0], "  target1 "), Some(h1.as_str()), "{calls:#?}");
    assert_eq!(line(all[0], "  target2 "), Some(h2.as_str()), "{calls:#?}");
    assert_eq!(line(all[0], "  color3 "), None, "{calls:#?}");
    assert!(
        calls.contains(&format!("  pso color1={RGBA8} mask=8 blend=0")),
        "{calls:#?}"
    );
    assert!(
        calls.contains(&format!("  pso color2={RGBA16F} mask=6 blend=0")),
        "{calls:#?}"
    );
    assert_eq!(draws(&calls), 1, "{calls:#?}");
}

/// A key naming a format a slot does not store, or a slot with no target, records nothing.
///
/// So does a key that was right before its extra was unbound: no pass, no pipeline built.
#[test]
fn a_key_the_targets_cannot_make_is_refused() {
    const DEVICE: u64 = 0x341;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (t1, _) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 64, 4));
    assert!(core.set_extra_target(1, Some(level0(t1))));
    let binds = ff_binds();
    let refused = Err(DrawRefused::Target);
    let other_format = key_extras([Some(extra(R32F, 0xf)), None, None]);
    assert_eq!(core.draw(&other_format, &triangle(&binds)), refused);
    let unbound = key_extras([Some(extra(RGBA8, 0xf)), Some(extra(RGBA8, 0xf)), None]);
    assert_eq!(core.draw(&unbound, &triangle(&binds)), refused);
    assert_eq!(core.pipelines_len(), 0);
    let named = key_extras([Some(extra(RGBA8, 0xf)), None, None]);
    assert_eq!(core.draw(&named, &triangle(&binds)), Ok(()));
    assert!(core.set_extra_target(1, None));
    assert_eq!(core.draw(&named, &triangle(&binds)), refused);
    assert_eq!(core.pipelines_len(), 1);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(passes(&calls).len(), 1, "{calls:#?}");
    assert_eq!(draws(&calls), 1, "{calls:#?}");
}

/// An empty slot between two bound ones is attached nowhere: no pass line, no pipeline line.
#[test]
fn a_hole_stays_empty() {
    const DEVICE: u64 = 0x342;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (t2, h2) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA16F, 64, 8));
    assert!(core.set_extra_target(2, Some(level0(t2))));
    assert_eq!(
        core.target_desc(false),
        Some(((64, 64), [BGRA8, 0, RGBA16F, 0]))
    );
    let binds = ff_binds();
    let hole = key_extras([None, Some(extra(RGBA16F, 0xf)), None]);
    assert_eq!(core.draw(&hole, &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    assert_eq!(line(all[0], "  target2 "), Some(h2.as_str()), "{calls:#?}");
    assert!(line(all[0], "  color2 ").is_some(), "{calls:#?}");
    assert_eq!(line(all[0], "  color1 "), None, "{calls:#?}");
    assert_eq!(line(all[0], "  target1 "), None, "{calls:#?}");
    assert!(
        calls.contains(&format!("  pso color2={RGBA16F} mask=15 blend=0")),
        "{calls:#?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("  pso color1=")),
        "{calls:#?}"
    );
}

/// A bound extra of another size than target 0 is no member of the pass set.
#[test]
fn an_extra_of_another_size_is_not_in_the_set() {
    const DEVICE: u64 = 0x343;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (small, _) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 32, 4));
    assert!(core.set_extra_target(1, Some(level0(small))));
    assert_eq!(core.target_desc(false), Some(((64, 64), [BGRA8, 0, 0, 0])));
    let binds = ff_binds();
    let named = key_extras([Some(extra(RGBA8, 0xf)), None, None]);
    assert_eq!(
        core.draw(&named, &triangle(&binds)),
        Err(DrawRefused::Target)
    );
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
}

/// A target already in the set is no member again: target 0's own, or a lower slot's.
#[test]
fn a_duplicate_is_not_in_the_set() {
    const DEVICE: u64 = 0x344;
    let (mut core, t0) = core_on_a_64_target(DEVICE);
    assert!(core.set_extra_target(1, Some(level0(t0))));
    assert_eq!(core.target_desc(false), Some(((64, 64), [BGRA8, 0, 0, 0])));
    let (t1, _) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 64, 4));
    assert!(core.set_extra_target(1, Some(level0(t1))));
    assert!(core.set_extra_target(2, Some(level0(t1))));
    assert_eq!(
        core.target_desc(false),
        Some(((64, 64), [BGRA8, RGBA8, 0, 0]))
    );
}

/// An extra leaves the set when target 0 takes another size, and joins it at its own.
#[test]
fn a_target_0_of_another_size_drops_the_extra() {
    const DEVICE: u64 = 0x345;
    let (mut core, t0) = core_on_a_64_target(DEVICE);
    let (small, h_small) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 32, 4));
    let (u, _) = stored(&mut core, DEVICE, 3, TextureKind::Tex2d, 32, 1);
    assert!(core.set_extra_target(1, Some(level0(small))));
    assert_eq!(core.target_desc(false), Some(((64, 64), [BGRA8, 0, 0, 0])));
    assert!(core.set_target(level0(u)));
    assert_eq!(
        core.target_desc(false),
        Some(((32, 32), [BGRA8, RGBA8, 0, 0]))
    );
    let binds = ff_binds();
    let named = key_extras([Some(extra(RGBA8, 0xf)), None, None]);
    assert_eq!(core.draw(&named, &triangle(&binds)), Ok(()));
    assert!(core.set_target(level0(t0)));
    assert_eq!(core.target_desc(false), Some(((64, 64), [BGRA8, 0, 0, 0])));
    assert_eq!(
        core.draw(&named, &triangle(&binds)),
        Err(DrawRefused::Target)
    );
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(slot_targets(&calls, 1), [h_small], "{calls:#?}");
}

/// Another target at a slot ends the pass, the same one does not; with a hole below it too.
///
/// The passes reach Metal after the optimizer: two passes that differ in any slot up to the
/// highest attached one stay two.
#[test]
fn an_extra_change_ends_the_pass() {
    const DEVICE: u64 = 0x346;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (a, ha) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 64, 4));
    let (b, hb) = stored_desc(&mut core, DEVICE, 3, &flat(RGBA8, 64, 4));
    let binds = ff_binds();
    for slot in [1, 2] {
        let mut extras = [None, None, None];
        extras[slot - 1] = Some(extra(RGBA8, 0xf));
        let named = key_extras(extras);
        assert!(core.set_extra_target(1, None));
        assert!(core.set_extra_target(slot, Some(level0(a))));
        assert_eq!(core.draw(&named, &triangle(&binds)), Ok(()));
        assert!(core.set_extra_target(slot, Some(level0(b))));
        assert_eq!(core.draw(&named, &triangle(&binds)), Ok(()));
        core.drain();
        let calls = dxmt_fakes::take_calls(DEVICE);
        assert_eq!(passes(&calls).len(), 2, "slot {slot}: {calls:#?}");
        assert_eq!(
            slot_targets(&calls, slot),
            [ha.clone(), hb.clone()],
            "slot {slot}: {calls:#?}"
        );
        assert_eq!(core.draw(&named, &triangle(&binds)), Ok(()));
        assert!(core.set_extra_target(slot, Some(level0(b))));
        assert_eq!(core.draw(&named, &triangle(&binds)), Ok(()));
        core.drain();
        let calls = dxmt_fakes::take_calls(DEVICE);
        assert_eq!(passes(&calls).len(), 1, "slot {slot}: {calls:#?}");
    }
}

/// Forgetting a texture empties every extra slot that holds it and ends the pass.
#[test]
fn forgetting_an_extra_unbinds_it() {
    const DEVICE: u64 = 0x347;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (cube, _) = stored_desc(
        &mut core,
        DEVICE,
        2,
        &target_desc(RGBA8, TextureKind::Cube, 64, 1, 4),
    );
    let (kept, h_kept) = stored_desc(&mut core, DEVICE, 3, &flat(RGBA8, 64, 4));
    let face = |face| TargetRef::Texture {
        id: cube,
        level: 0,
        face: Some(face),
    };
    assert!(core.set_extra_target(1, Some(face(0))));
    assert!(core.set_extra_target(2, Some(face(1))));
    assert!(core.set_extra_target(3, Some(level0(kept))));
    assert_eq!(
        core.target_desc(false),
        Some(((64, 64), [BGRA8, RGBA8, RGBA8, RGBA8]))
    );
    let binds = ff_binds();
    let all = key_extras([
        Some(extra(RGBA8, 0xf)),
        Some(extra(RGBA8, 0xf)),
        Some(extra(RGBA8, 0xf)),
    ]);
    assert_eq!(core.draw(&all, &triangle(&binds)), Ok(()));
    core.forget_texture(cube);
    assert_eq!(
        core.target_desc(false),
        Some(((64, 64), [BGRA8, 0, 0, RGBA8]))
    );
    let third = key_extras([None, None, Some(extra(RGBA8, 0xf))]);
    assert_eq!(core.draw(&third, &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 2, "{calls:#?}");
    assert_eq!(line(found[1], "  target1 "), None, "{calls:#?}");
    assert_eq!(line(found[1], "  target2 "), None, "{calls:#?}");
    assert_eq!(
        slot_targets(&calls, 3),
        [h_kept.clone(), h_kept],
        "{calls:#?}"
    );
}

/// Under sRGB write the set attaches every member's twin when every one has one; none otherwise.
#[test]
fn srgb_twins_are_all_or_nothing() {
    const DEVICE: u64 = 0x348;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t0, _) = stored_desc(&mut core, DEVICE, 1, &twinned(flat(BGRA8, 64, 4), SRGB));
    let (t1, _) = stored_desc(
        &mut core,
        DEVICE,
        2,
        &twinned(flat(RGBA8, 64, 4), RGBA8_SRGB),
    );
    let (float, _) = stored_desc(&mut core, DEVICE, 3, &flat(R32F, 64, 4));
    let (small, _) = stored_desc(&mut core, DEVICE, 4, &flat(R32F, 32, 4));
    assert!(core.set_target(level0(t0)));
    assert!(core.set_extra_target(1, Some(level0(t1))));
    assert_eq!(
        core.target_desc(true),
        Some(((64, 64), [SRGB, RGBA8_SRGB, 0, 0]))
    );
    assert_eq!(
        core.target_desc(false),
        Some(((64, 64), [BGRA8, RGBA8, 0, 0]))
    );
    assert!(core.set_extra_target(1, Some(level0(float))));
    assert_eq!(
        core.target_desc(true),
        Some(((64, 64), [BGRA8, R32F, 0, 0]))
    );
    assert!(core.set_extra_target(1, Some(level0(small))));
    assert_eq!(core.target_desc(true), Some(((64, 64), [SRGB, 0, 0, 0])));
}

/// An extra's blend reaches its colour of the pipeline; an unblended one stays off.
#[test]
fn the_blend_reaches_each_extra() {
    const DEVICE: u64 = 0x349;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (t1, _) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 64, 4));
    let (t2, _) = stored_desc(&mut core, DEVICE, 3, &flat(RGBA8, 64, 4));
    assert!(core.set_extra_target(1, Some(level0(t1))));
    assert!(core.set_extra_target(2, Some(level0(t2))));
    let mut rs = mtld3d_types::render_state_defaults();
    rs[mtld3d_types::D3DRS_ALPHABLENDENABLE as usize] = 1;
    let blend = mtld3d_core::pipeline_state::core_blend(&rs, true);
    assert!(blend.is_some());
    let blended = PipelineKey {
        blend: blend.clone(),
        extras: [
            Some(CoreExtra {
                format: RGBA8,
                write_mask: 0xf,
                blend,
            }),
            Some(extra(RGBA8, 0xf)),
            None,
        ],
        ..key(0xf)
    };
    let binds = ff_binds();
    assert_eq!(core.draw(&blended, &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        calls.contains(&format!("  pso color1={RGBA8} mask=15 blend=1")),
        "{calls:#?}"
    );
    assert!(
        calls.contains(&format!("  pso color2={RGBA8} mask=15 blend=0")),
        "{calls:#?}"
    );
}

/// The tile barrier's pipeline names each attached slot's format, a hole none, and one sample.
///
/// Under sRGB write with two twinned members both name the twins.
#[test]
fn the_tile_barrier_names_each_slot() {
    const DEVICE: u64 = 0x34a;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (x, _) = stored(&mut core, DEVICE, 1, TextureKind::Tex2d, 64, 2);
    let (e, _) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA16F, 64, 8));
    let (y, _) = stored_desc(
        &mut core,
        DEVICE,
        3,
        &twinned(target_desc(BGRA8, TextureKind::Tex2d, 64, 2, 4), SRGB),
    );
    let (f, _) = stored_desc(
        &mut core,
        DEVICE,
        4,
        &twinned(flat(RGBA8, 64, 4), RGBA8_SRGB),
    );
    let binds = ff_binds();
    // A draw that samples target 0's texture, whose level 0 its pass attaches.
    let sampling = |core: &mut CoreDevice, id: TextureId, key: &PipelineKey| {
        let bind = core
            .texture_bind(Stage::Pixel, 0, id, BGRA8, None)
            .expect("a stored texture");
        let textures = [bind];
        let data = DrawData {
            textures: &textures,
            ..triangle(&binds)
        };
        assert_eq!(core.draw(key, &data), Ok(()));
        core.drain();
        dxmt_fakes::take_calls(DEVICE)
    };
    assert!(core.set_target(level0(x)));
    assert!(core.set_extra_target(2, Some(level0(e))));
    let hole = key_extras([None, Some(extra(RGBA16F, 0xf)), None]);
    let calls = sampling(&mut core, x, &hole);
    let tile = |calls: &[String], slot: usize| {
        calls
            .iter()
            .find_map(|c| c.strip_prefix(&format!("  tile color{slot}=")))
            .map(str::to_owned)
    };
    assert_eq!(tile(&calls, 0), Some(BGRA8.to_string()), "{calls:#?}");
    assert_eq!(tile(&calls, 1), None, "{calls:#?}");
    assert_eq!(tile(&calls, 2), Some(RGBA16F.to_string()), "{calls:#?}");
    assert!(calls.contains(&"  tile samples=1".to_owned()), "{calls:#?}");
    assert!(core.set_target(level0(y)));
    assert!(core.set_extra_target(2, None));
    assert!(core.set_extra_target(1, Some(level0(f))));
    assert_eq!(
        core.target_desc(true),
        Some(((64, 64), [SRGB, RGBA8_SRGB, 0, 0]))
    );
    let twins = PipelineKey {
        color_format: SRGB,
        ..key_extras([Some(extra(RGBA8_SRGB, 0xf)), None, None])
    };
    let calls = sampling(&mut core, y, &twins);
    assert_eq!(tile(&calls, 0), Some(SRGB.to_string()), "{calls:#?}");
    assert_eq!(tile(&calls, 1), Some(RGBA8_SRGB.to_string()), "{calls:#?}");
}

/// Each bound target's Clear folds into its slot of the next pass, as that slot's load action.
///
/// With a hole below the cleared slot too: the pass's count reaches it.
#[test]
fn a_clear_reaches_each_bound_target() {
    const DEVICE: u64 = 0x34b;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (t1, _) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 64, 4));
    let (t2, _) = stored_desc(&mut core, DEVICE, 3, &flat(RGBA8, 64, 4));
    assert!(core.set_extra_target(1, Some(level0(t1))));
    assert!(core.set_extra_target(2, Some(level0(t2))));
    let colours = [
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
    ];
    for (slot, colour) in colours.into_iter().enumerate() {
        core.clear(slot, colour, false);
    }
    let binds = ff_binds();
    let both = key_extras([Some(extra(RGBA8, 0xf)), Some(extra(RGBA8, 0xf)), None]);
    assert_eq!(core.draw(&both, &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    for (slot, clear) in ["1,0,0,1", "0,1,0,1", "0,0,1,1"].into_iter().enumerate() {
        assert_eq!(
            line(found[0], &format!("  color{slot} ")),
            Some(format!("load=2 store=1 clear={clear}").as_str()),
            "{calls:#?}"
        );
    }
    assert!(core.set_extra_target(1, None));
    core.clear(2, [1.0, 1.0, 0.0, 1.0], false);
    let hole = key_extras([None, Some(extra(RGBA8, 0xf)), None]);
    assert_eq!(core.draw(&hole, &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    assert_eq!(
        line(found[0], "  color2 "),
        Some("load=2 store=1 clear=1,1,0,1"),
        "{calls:#?}"
    );
}

/// A Clear of a bound target outside the pass set is a pass of its own, on that target's view.
///
/// The draw after it attaches target 0 alone.
#[test]
fn a_clear_of_a_target_outside_the_set_is_its_own_pass() {
    const DEVICE: u64 = 0x34c;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t0, h0) = stored(&mut core, DEVICE, 1, TextureKind::Tex2d, 64, 1);
    let (small, h_small) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 32, 4));
    assert!(core.set_target(level0(t0)));
    assert!(core.set_extra_target(1, Some(level0(small))));
    core.clear(1, [0.0, 1.0, 0.0, 1.0], false);
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 2, "{calls:#?}");
    let handles = targets(&calls)
        .into_iter()
        .map(|(handle, _)| handle)
        .collect::<Vec<_>>();
    assert_eq!(handles, [h_small, h0], "{calls:#?}");
    assert_eq!(
        line(found[0], "  color0 "),
        Some("load=2 store=1 clear=0,1,0,1"),
        "{calls:#?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("  color1 ")),
        "{calls:#?}"
    );
}

/// Under sRGB write a Clear stores its colour encoded exactly once.
///
/// A target with a twin, alone, is cleared through the twin with the linear colour. With an
/// extra that has none, both are cleared through their stored views with the encoded colour,
/// and both Clears fold into the pass of the set.
#[test]
fn a_clear_encodes_once() {
    const DEVICE: u64 = 0x34d;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t0, h0) = stored_desc(&mut core, DEVICE, 1, &twinned(flat(BGRA8, 64, 4), SRGB));
    let (float, h_float) = stored_desc(&mut core, DEVICE, 2, &flat(R32F, 64, 4));
    assert!(core.set_target(level0(t0)));
    let linear = [0.5, 0.5, 0.5, 1.0];
    core.clear(0, linear, true);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    assert_eq!(
        line(found[0], "  color0 "),
        Some("load=2 store=1 clear=0.5,0.5,0.5,1"),
        "{calls:#?}"
    );
    let cleared = targets(&calls);
    assert!(is_view(&calls, &cleared[0].0, &h0, SRGB), "{calls:#?}");
    assert!(core.set_extra_target(1, Some(level0(float))));
    core.clear(0, linear, true);
    core.clear(1, linear, true);
    let binds = ff_binds();
    let named = key_extras([Some(extra(R32F, 0xf)), None, None]);
    assert_eq!(core.draw(&named, &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    let [r, g, b, a] = mtld3d_core::convert::linear_to_srgb_rgba(linear).map(f64::from);
    let encoded = format!("load=2 store=1 clear={r},{g},{b},{a}");
    assert_eq!(
        line(found[0], "  color0 "),
        Some(encoded.as_str()),
        "{calls:#?}"
    );
    assert_eq!(
        line(found[0], "  color1 "),
        Some(encoded.as_str()),
        "{calls:#?}"
    );
    assert_eq!(targets(&calls)[0].0, h0, "{calls:#?}");
    assert_eq!(
        line(found[0], "  target1 "),
        Some(h_float.as_str()),
        "{calls:#?}"
    );
}

/// A Clear of rects at a slot is a quad clear on that slot's view alone.
#[test]
fn a_rect_clear_reaches_one_slot() {
    const DEVICE: u64 = 0x34e;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (t1, h1) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 64, 4));
    assert!(core.set_extra_target(1, Some(level0(t1))));
    core.clear_rects(1, &[[8, 8, 16, 16]], [0.0, 1.0, 0.0, 1.0], false);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    assert_eq!(
        targets(&calls).first().map(|(handle, _)| handle.as_str()),
        Some(h1.as_str()),
        "{calls:#?}"
    );
    assert_eq!(line(found[0], "  color1 "), None, "{calls:#?}");
    let rects = found[0].iter().filter(|c| c.trim() == "render 36").count();
    assert_eq!(rects, 1, "{calls:#?}");
}

/// The size of the target bound at each slot, in the pass set or not; none for an empty slot.
#[test]
fn clear_size_lists_bound_slots() {
    const DEVICE: u64 = 0x34f;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (small, _) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 32, 4));
    let (wide, _) = stored_desc(&mut core, DEVICE, 3, &flat(RGBA8, 64, 4));
    assert!(core.set_extra_target(1, Some(level0(small))));
    assert!(core.set_extra_target(3, Some(level0(wide))));
    assert_eq!(
        [0, 1, 2, 3, 4].map(|slot| core.clear_size(slot)),
        [Some((64, 64)), Some((32, 32)), None, Some((64, 64)), None]
    );
}

/// A slot that is not 1-3, or a target that does not resolve, is refused and changes nothing.
#[test]
fn an_extra_that_cannot_be_bound_is_refused() {
    const DEVICE: u64 = 0x350;
    let (mut core, _) = core_on_a_64_target(DEVICE);
    let (kept, _) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 64, 4));
    assert!(core.set_extra_target(1, Some(level0(kept))));
    let unknown = level0(TextureId::from_raw(9));
    assert!(!core.set_extra_target(0, Some(level0(kept))));
    assert!(!core.set_extra_target(4, Some(level0(kept))));
    assert!(!core.set_extra_target(1, Some(unknown)));
    assert_eq!(core.clear_size(1), Some((64, 64)));
    assert_eq!(
        core.target_desc(false),
        Some(((64, 64), [BGRA8, RGBA8, 0, 0]))
    );
}

/// A 4-sample one-level 2D render target of `format`, `size` square, which the store does not hold.
///
/// Its usage is the one `texture_create` gives a render target with twins.
fn multisampled(core: &CoreDevice, format: u32, size: u32) -> Arc<Texture> {
    let usage = WMT_TEXTURE_USAGE_RENDER_TARGET
        | WMT_TEXTURE_USAGE_SHADER_READ
        | WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW;
    let info = WMTTextureInfo {
        pixel_format: format,
        width: size,
        height: size,
        depth: 1,
        array_length: 1,
        bits: texture_info::info_bits(WMT_TEXTURE_TYPE2D_MULTISAMPLE, 1, 4, usage),
        options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let texture = Arc::new(Texture::new(info, core.device.device()));
    texture.rename(texture.allocate(TextureAllocationFlags::GPU_PRIVATE));
    texture
}

/// Records a render pass that loads and stores `texture`'s view `key` at colour 0, drawing nothing.
fn pass_on(core: &mut CoreDevice, texture: &Arc<Texture>, key: TextureViewKey) {
    let texture = Arc::clone(texture);
    core.emit(move |enc| {
        enc.start_render_pass(0, 0, 1, None);
        let attachment =
            enc.attachment::<{ stage::PIXEL }>(&texture, key, Access::READ | Access::WRITE);
        if let Some(targets) = enc.render_targets() {
            let color = &mut targets.colors[0].target;
            color.attachment = attachment;
            color.load_action = WMT_LOAD_ACTION_LOAD;
            color.store_action = WMT_STORE_ACTION_STORE;
        }
        enc.end_pass();
    });
}

/// Records the core's resolve of `source`'s view `key` into `destination`.
fn resolve_on(
    core: &mut CoreDevice,
    source: &Arc<Texture>,
    key: TextureViewKey,
    destination: &Resolved,
) {
    let (source, texture, view) = (
        Arc::clone(source),
        Arc::clone(&destination.texture),
        destination.key,
    );
    core.emit(move |enc| enc.resolve_texture(&source, key, &texture, view));
}

/// The single render pass `calls` shows: its colour 0 stores and resolves, into the returned view.
fn resolved_pass(calls: &[String]) -> String {
    let found = passes(calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    let color = line(found[0], "  color0 ").unwrap_or_default();
    assert!(color.starts_with("load=1 store=3 "), "{calls:#?}");
    line(found[0], "  resolve0 ")
        .unwrap_or_else(|| panic!("no resolve0: {calls:#?}"))
        .to_owned()
}

/// The core makes a multisampled texture and attaches its own view to a pass (DXMTRS C4b-3d5).
///
/// Made with the multisample type and 4 samples, as the fake sees its bits; a pass on its
/// `TYPE2D_MULTISAMPLE` view, which is the texture itself, is encoded. The fakes take any view:
/// that Metal does is the validation layer's to show.
#[test]
fn dxmt_core_attaches_a_multisampled_view() {
    const DEVICE: u64 = 0x380;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let texture = multisampled(&core, BGRA8, 64);
    let calls = dxmt_fakes::take_calls(DEVICE);
    let made = calls
        .iter()
        .find(|c| c.starts_with("MTLDevice_newTexture "))
        .unwrap_or_else(|| panic!("{calls:#?}"));
    let bits = made
        .rsplit_once("bits=0x")
        .and_then(|(_, hex)| u32::from_str_radix(hex, 16).ok())
        .unwrap_or_else(|| panic!("{made}"));
    let seen = WMTTextureInfo {
        bits,
        ..*texture.info()
    };
    assert_eq!(
        (seen.r#type(), seen.sample_count()),
        (WMT_TEXTURE_TYPE2D_MULTISAMPLE, 4),
        "{made}"
    );
    let handle = textures_made(&calls)[0].clone();
    pass_on(&mut core, &texture, texture.full_view());
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    assert_eq!(
        line(found[0], "  color0 "),
        Some("load=1 store=1 clear=0,0,0,0"),
        "{calls:#?}"
    );
    assert_eq!(targets(&calls)[0].0, handle, "{calls:#?}");
}

/// The core resolves a multisampled view into a 2D view of one level, or of one cube face.
///
/// Alone, a pass of its own whose colour 0 is the source, resolved into that view; right after a
/// render pass on the source, folded into it: one pass, its store action the resolve's.
#[test]
fn dxmt_core_resolves_into_a_level_view() {
    const DEVICE: u64 = 0x381;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (flat, flat_handle) = stored(&mut core, DEVICE, 1, TextureKind::Tex2d, 64, 2);
    let (cube, cube_handle) = stored(&mut core, DEVICE, 2, TextureKind::Cube, 32, 1);
    let source = multisampled(&core, BGRA8, 32);
    let source_handle = textures_made(&dxmt_fakes::take_calls(DEVICE))[0].clone();
    let key = source.full_view();
    let level1 = TargetRef::Texture {
        id: flat,
        level: 1,
        face: None,
    };
    let face3 = TargetRef::Texture {
        id: cube,
        level: 0,
        face: Some(3),
    };
    for (target, parent, mip, slice) in [(level1, flat_handle, 1, 0), (face3, cube_handle, 0, 3)] {
        let destination = core.resolve(target).expect("a target");
        resolve_on(&mut core, &source, key, &destination);
        core.drain();
        let calls = dxmt_fakes::take_calls(DEVICE);
        let want = format!(
            " of {parent} format={BGRA8} type={WMT_TEXTURE_TYPE2D} mip={mip} slice={slice}"
        );
        let view = calls
            .iter()
            .filter_map(|c| c.strip_prefix("MTLTexture_newTextureView "))
            .find_map(|c| c.strip_suffix(want.as_str()))
            .unwrap_or_else(|| panic!("no view{want}: {calls:#?}"))
            .to_owned();
        assert_eq!(
            resolved_pass(&calls),
            view,
            "alone, mip {mip} slice {slice}"
        );
        assert_eq!(targets(&calls)[0].0, source_handle, "{calls:#?}");
        pass_on(&mut core, &source, key);
        resolve_on(&mut core, &source, key, &destination);
        core.drain();
        let calls = dxmt_fakes::take_calls(DEVICE);
        assert_eq!(
            resolved_pass(&calls),
            view,
            "folded, mip {mip} slice {slice}"
        );
        assert_eq!(targets(&calls)[0].0, source_handle, "{calls:#?}");
    }
}

/// The core resolves through sRGB twins: both views keyed in the twin's format.
///
/// The resolve names the destination's twin view, alone and folded into a pass on the source's.
#[test]
fn dxmt_core_resolves_through_a_twin() {
    const DEVICE: u64 = 0x382;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (id, handle) = stored_desc(&mut core, DEVICE, 1, &twinned(flat(BGRA8, 32, 4), SRGB));
    let source = multisampled(&core, BGRA8, 32);
    let source_handle = textures_made(&dxmt_fakes::take_calls(DEVICE))[0].clone();
    let key = source.full_view().with_format(SRGB);
    let destination = core.resolve_as(level0(id), SRGB).expect("a target");
    resolve_on(&mut core, &source, key, &destination);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let twin = resolved_pass(&calls);
    assert!(is_view(&calls, &twin, &handle, SRGB), "{calls:#?}");
    let source_twin = targets(&calls)[0].0.clone();
    assert!(
        is_view(&calls, &source_twin, &source_handle, SRGB),
        "{calls:#?}"
    );
    pass_on(&mut core, &source, key);
    resolve_on(&mut core, &source, key, &destination);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(resolved_pass(&calls), twin, "folded");
    assert_eq!(targets(&calls)[0].0, source_twin, "{calls:#?}");
}

/// The key of the fixture's draw with `samples` a texel and alpha-to-coverage `a2c`.
fn key_of_count(samples: u8, a2c: bool) -> PipelineKey {
    PipelineKey {
        samples,
        alpha_to_coverage: a2c,
        ..key(0xf)
    }
}

/// A multisampled render target is made of the multisample type, and zeroed by a pass.
#[test]
fn a_multisampled_texture_is_made_so() {
    const DEVICE: u64 = 0x383;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    core.texture_create(TextureId::from_raw(1), &ms_desc(BGRA8, 64, 4));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let made = calls
        .iter()
        .find(|c| c.starts_with("MTLDevice_newTexture "))
        .unwrap_or_else(|| panic!("{calls:#?}"));
    let bits = made
        .rsplit_once("bits=0x")
        .and_then(|(_, hex)| u32::from_str_radix(hex, 16).ok())
        .unwrap_or_else(|| panic!("{made}"));
    let field = |shift: u32| (bits >> shift) & 0xff;
    assert_eq!(
        (
            field(WMTTextureInfo::TYPE_SHIFT),
            field(WMTTextureInfo::SAMPLE_COUNT_SHIFT)
        ),
        (WMT_TEXTURE_TYPE2D_MULTISAMPLE, 4),
        "{made}"
    );
    let zeroed = calls
        .iter()
        .filter(|c| *c == "  color0 load=2 store=1 clear=0,0,0,0")
        .count();
    assert_eq!(zeroed, 1, "{calls:#?}");
}

/// A key names target 0's sample count, whatever pass is open; its pipeline and pass take it.
///
/// The pass attaches the multisampled texture itself (its one view of the multisample type).
#[test]
fn the_key_names_the_sample_count() {
    const DEVICE: u64 = 0x384;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t, h_t) = stored_desc(&mut core, DEVICE, 1, &ms_desc(BGRA8, 64, 4));
    let (u, _) = stored(&mut core, DEVICE, 2, TextureKind::Tex2d, 64, 1);
    assert!(core.set_target(level0(t)));
    assert_eq!(core.target_samples(), 4);
    let binds = ff_binds();
    let refused = Err(DrawRefused::Target);
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), refused);
    let four = key_of_count(4, false);
    assert_eq!(core.draw(&four, &triangle(&binds)), Ok(()));
    // The four-sample draw's pass is open on the same colours: no excuse for another count.
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), refused);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(draws(&calls), 1, "{calls:#?}");
    assert!(
        calls.contains(&"  pso samples=4 a2c=0".to_owned()),
        "{calls:#?}"
    );
    assert_eq!(targets(&calls)[0].0, h_t, "{calls:#?}");
    assert!(core.set_target(level0(u)));
    assert_eq!(core.target_samples(), 1);
    assert_eq!(core.draw(&four, &triangle(&binds)), refused);
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
}

/// An extra of another sample count than target 0 is no member of the pass set.
#[test]
fn an_extra_of_another_count_is_not_in_the_set() {
    const DEVICE: u64 = 0x385;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (ms0, _) = stored_desc(&mut core, DEVICE, 1, &ms_desc(BGRA8, 64, 4));
    let (ms1, _) = stored_desc(&mut core, DEVICE, 2, &ms_desc(RGBA8, 64, 4));
    let (one0, _) = stored(&mut core, DEVICE, 3, TextureKind::Tex2d, 64, 1);
    let (one1, _) = stored_desc(&mut core, DEVICE, 4, &flat(RGBA8, 64, 4));
    for (what, t0, t1, slot1) in [
        ("4 beside 1", ms0, one1, 0),
        ("1 beside 4", one0, ms1, 0),
        ("4 beside 4", ms0, ms1, RGBA8),
    ] {
        assert!(core.set_target(level0(t0)), "{what}");
        assert!(core.set_extra_target(1, Some(level0(t1))), "{what}");
        assert_eq!(
            core.target_desc(false),
            Some(((64, 64), [BGRA8, slot1, 0, 0])),
            "{what}"
        );
    }
}

/// A depth-stencil of another sample count than target 0 is left out of the draw's pass.
///
/// A key naming its format is refused; one naming none draws without it. Equal counts attach.
#[test]
fn a_depth_of_another_count_is_left_out() {
    const DEVICE: u64 = 0x386;
    const D32F_S8: u32 = winemetal_sys::wire::WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t, _) = stored_desc(&mut core, DEVICE, 1, &ms_desc(BGRA8, 64, 4));
    let (u, _) = stored(&mut core, DEVICE, 2, TextureKind::Tex2d, 64, 1);
    let ms_depth = CoreTextureDesc {
        samples: 4,
        ..depth_desc(D32F_S8, 64, 64, 1)
    };
    let (d4, _) = stored_desc(&mut core, DEVICE, 3, &ms_depth);
    let (d1, _) = stored_desc(&mut core, DEVICE, 4, &depth_desc(D32F_S8, 64, 64, 1));
    let binds = ff_binds();
    let named = |samples| PipelineKey {
        depth_format: D32F_S8,
        ..key_of_count(samples, false)
    };
    for (what, target, depth, samples) in [
        ("4-sample depth, 1-sample target", u, d4, 1),
        ("1-sample depth, 4-sample target", t, d1, 4),
    ] {
        assert!(core.set_target(level0(target)), "{what}");
        assert!(
            core.set_depth(Some(DepthRef {
                id: depth,
                level: 0
            })),
            "{what}"
        );
        assert_eq!(
            core.draw(&named(samples), &triangle(&binds)),
            Err(DrawRefused::Depth),
            "{what}"
        );
        assert_eq!(
            core.draw(&key_of_count(samples, false), &triangle(&binds)),
            Ok(()),
            "{what}"
        );
        core.drain();
        let calls = dxmt_fakes::take_calls(DEVICE);
        let found = passes(&calls);
        assert_eq!(found.len(), 1, "{what}: {calls:#?}");
        assert_eq!(line(found[0], "  depth "), None, "{what}: {calls:#?}");
    }
    assert!(core.set_depth(Some(DepthRef { id: d4, level: 0 })));
    assert_eq!(core.depth_samples(), Some(4));
    assert_eq!(core.draw(&named(4), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(line(passes(&calls)[0], "  depth ").is_some(), "{calls:#?}");
}

/// Alpha-to-coverage is the pipeline's: two keys differing in it build two pipelines.
#[test]
fn alpha_to_coverage_is_the_pipelines() {
    const DEVICE: u64 = 0x387;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t, _) = stored_desc(&mut core, DEVICE, 1, &ms_desc(BGRA8, 64, 4));
    assert!(core.set_target(level0(t)));
    let binds = ff_binds();
    for a2c in [true, false] {
        assert_eq!(core.draw(&key_of_count(4, a2c), &triangle(&binds)), Ok(()));
    }
    assert_eq!(core.pipelines_len(), 2);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    for line in ["  pso samples=4 a2c=1", "  pso samples=4 a2c=0"] {
        assert!(calls.contains(&line.to_owned()), "{line}: {calls:#?}");
    }
}

/// A whole Clear of a multisampled target folds into its pass; a rect Clear is a 4-sample quad.
#[test]
fn a_clear_of_a_multisampled_target() {
    const DEVICE: u64 = 0x388;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t, _) = stored_desc(&mut core, DEVICE, 1, &ms_desc(BGRA8, 64, 4));
    assert!(core.set_target(level0(t)));
    core.clear(0, [1.0, 0.0, 0.0, 1.0], false);
    let binds = ff_binds();
    assert_eq!(
        core.draw(&key_of_count(4, false), &triangle(&binds)),
        Ok(())
    );
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    assert_eq!(
        line(found[0], "  color0 "),
        Some("load=2 store=1 clear=1,0,0,1"),
        "{calls:#?}"
    );
    core.clear_rects(0, &[[8, 8, 16, 16]], [0.0, 1.0, 0.0, 1.0], false);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "{calls:#?}");
    assert_eq!(line(found[0], "  samples="), Some("4"), "{calls:#?}");
    assert!(
        calls.contains(&"  pso samples=4 a2c=0".to_owned()),
        "{calls:#?}"
    );
}

/// A multisampled target is not read back: nothing is recorded.
#[test]
fn a_multisampled_target_is_not_read_back() {
    const DEVICE: u64 = 0x389;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t, _) = stored_desc(&mut core, DEVICE, 1, &ms_desc(BGRA8, 64, 4));
    let mut out = vec![0_u8; 64 * 4 * 64];
    assert!(!core.read_back(level0(t), &mut out, 64 * 4));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        !calls.contains(&"MTLCommandBuffer_blitCommandEncoder".to_owned()),
        "{calls:#?}"
    );
}

/// A Clear does not reach a bound target of target 0's size and another sample count.
///
/// One of another size is cleared apart whatever its count, and one of target 0's count is.
#[test]
fn a_clear_skips_a_same_size_target_of_another_count() {
    const DEVICE: u64 = 0x38a;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t, _) = stored_desc(&mut core, DEVICE, 1, &ms_desc(BGRA8, 64, 4));
    let (same, _) = stored_desc(&mut core, DEVICE, 2, &flat(RGBA8, 64, 4));
    let (small, _) = stored_desc(&mut core, DEVICE, 3, &flat(RGBA8, 32, 4));
    let (equal, _) = stored_desc(&mut core, DEVICE, 4, &ms_desc(RGBA8, 64, 4));
    assert!(core.set_target(level0(t)));
    for (what, extra, reached) in [
        ("same size, another count", same, None),
        ("another size, another count", small, Some((32, 32))),
        ("same size, same count", equal, Some((64, 64))),
    ] {
        assert!(core.set_extra_target(1, Some(level0(extra))), "{what}");
        assert_eq!(core.clear_size(1), reached, "{what}");
        assert_eq!(core.clear_size(0), Some((64, 64)), "{what}");
    }
}

/// A resolve reads the source through the view a pass or Clear last wrote it in.
///
/// After a draw through the sRGB twin and a pass on another target, a resolve pass of its own
/// names both twin views; after a Clear with sRGB write off, the stored views (the textures
/// themselves); with nothing between the draw and the resolve, it is the draw's pass's store.
#[test]
fn a_resolve_takes_the_last_view() {
    const DEVICE: u64 = 0x38b;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t, h_t) = stored_desc(&mut core, DEVICE, 1, &twinned(ms_desc(BGRA8, 64, 4), SRGB));
    let (d, h_d) = stored_desc(&mut core, DEVICE, 2, &twinned(flat(BGRA8, 64, 4), SRGB));
    let (e, _) = stored(&mut core, DEVICE, 3, TextureKind::Tex2d, 64, 1);
    let binds = ff_binds();
    let through_twin = PipelineKey {
        color_format: SRGB,
        ..key_of_count(4, false)
    };
    let other_pass = |core: &mut CoreDevice| {
        assert!(core.set_target(level0(e)));
        assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    };
    assert!(core.set_target(level0(t)));
    assert_eq!(core.draw(&through_twin, &triangle(&binds)), Ok(()));
    other_pass(&mut core);
    assert!(core.resolve_into(level0(t), level0(d)));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 3, "{calls:#?}");
    let drawn = line(found[0], "  color0 ").unwrap_or_default();
    assert!(drawn.starts_with("load=1 store=1 "), "{calls:#?}");
    let twin = line(found[2], "  resolve0 ").unwrap_or_default().to_owned();
    assert!(is_view(&calls, &twin, &h_d, SRGB), "{calls:#?}");
    let source = targets(&calls)[2].0.clone();
    assert!(is_view(&calls, &source, &h_t, SRGB), "{calls:#?}");
    assert!(core.set_target(level0(t)));
    core.clear(0, [0.0, 0.0, 0.0, 1.0], false);
    other_pass(&mut core);
    assert!(core.resolve_into(level0(t), level0(d)));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 3, "{calls:#?}");
    assert_eq!(
        line(found[2], "  resolve0 "),
        Some(h_d.as_str()),
        "{calls:#?}"
    );
    assert_eq!(targets(&calls)[2].0, h_t, "{calls:#?}");
    assert!(core.set_target(level0(t)));
    assert_eq!(core.draw(&through_twin, &triangle(&binds)), Ok(()));
    assert!(core.resolve_into(level0(t), level0(d)));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(resolved_pass(&calls), twin, "folded");
}

/// A resolve ends the destination's open pass and follows it; a later draw on it opens another.
#[test]
fn a_resolve_ends_the_destinations_pass() {
    const DEVICE: u64 = 0x38c;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t, h_t) = stored_desc(&mut core, DEVICE, 1, &ms_desc(BGRA8, 64, 4));
    let (d, h_d) = stored(&mut core, DEVICE, 2, TextureKind::Tex2d, 64, 1);
    let binds = ff_binds();
    assert!(core.set_target(level0(d)));
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    assert!(core.resolve_into(level0(t), level0(d)));
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 3, "{calls:#?}");
    let handles = targets(&calls)
        .into_iter()
        .map(|(handle, _)| handle)
        .collect::<Vec<_>>();
    assert_eq!(handles, [h_d.clone(), h_t, h_d.clone()], "{calls:#?}");
    assert_eq!(
        line(found[1], "  resolve0 "),
        Some(h_d.as_str()),
        "{calls:#?}"
    );
    assert_eq!(draws(&calls), 2, "{calls:#?}");
}

/// A resolve the core cannot record is refused, and nothing is recorded.
///
/// A single-sample source, a multisampled destination, another size, another format, and a
/// format Metal does not resolve (an integer one: the fakes answer every GPU family, and Apple9
/// resolves the 32-bit floats).
#[test]
fn a_resolve_refuses_what_it_cannot_do() {
    const DEVICE: u64 = 0x38d;
    const RGBA16_UINT: u32 = winemetal_sys::wire::WMT_PIXEL_FORMAT_RGBA16_UINT;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (ms, _) = stored_desc(&mut core, DEVICE, 1, &ms_desc(BGRA8, 64, 4));
    let (ms_too, _) = stored_desc(&mut core, DEVICE, 2, &ms_desc(BGRA8, 64, 4));
    let (one, _) = stored(&mut core, DEVICE, 3, TextureKind::Tex2d, 64, 1);
    let (one_too, _) = stored(&mut core, DEVICE, 4, TextureKind::Tex2d, 64, 1);
    let (small, _) = stored(&mut core, DEVICE, 5, TextureKind::Tex2d, 32, 1);
    let (rgba, _) = stored_desc(&mut core, DEVICE, 6, &flat(RGBA8, 64, 4));
    let (int_ms, _) = stored_desc(&mut core, DEVICE, 7, &ms_desc(RGBA16_UINT, 64, 4));
    let (int_one, _) = stored_desc(&mut core, DEVICE, 8, &flat(RGBA16_UINT, 64, 8));
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    for (what, source, destination) in [
        ("a single-sample source", one, one_too),
        ("a multisampled destination", ms, ms_too),
        ("another size", ms, small),
        ("another format", ms, rgba),
        ("a format Metal does not resolve", int_ms, int_one),
    ] {
        assert!(
            !core.resolve_into(level0(source), level0(destination)),
            "{what}"
        );
    }
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        !calls.contains(&"MTLCommandBuffer_renderCommandEncoder".to_owned()),
        "{calls:#?}"
    );
}

/// A Clear, an upload into that target, then a draw: the draw's pass loads the upload.
///
/// The Clear is a pass of its own before the upload's blit, not folded past it into the draw's
/// pass, where it would clear the uploaded block (DXMTRS C4c-3a).
#[test]
fn a_draw_after_an_upload_loads_it() {
    const DEVICE: u64 = 0x3a4;
    let (mut core, _) = core_and_back_buffer(DEVICE);
    let (t, h_t) = stored(&mut core, DEVICE, 1, TextureKind::Tex2d, 64, 1);
    assert!(core.set_target(level0(t)));
    core.clear(0, [1.0, 0.0, 0.0, 1.0], false);
    let block = UploadRegion {
        level: 0,
        slice: 0,
        x: 8,
        y: 8,
        z: 0,
        width: 8,
        height: 8,
        depth: 1,
    };
    assert!(core.texture_upload(t, block, &[0x55; 8 * 8 * 4], 8 * 4, 8 * 8 * 4));
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 2, "{calls:#?}");
    assert_eq!(
        line(found[0], "  color0 "),
        Some("load=2 store=1 clear=1,0,0,1"),
        "{calls:#?}"
    );
    let drawn = line(found[1], "  color0 ").unwrap_or_default();
    assert!(drawn.starts_with("load=1 store=1 "), "{calls:#?}");
    let copied = format!("  blit copy_to_texture {h_t} ");
    let handles = targets(&calls)
        .into_iter()
        .map(|(handle, _)| handle)
        .collect::<Vec<_>>();
    assert_eq!(handles, [h_t.clone(), h_t], "{calls:#?}");
    let upload = calls
        .iter()
        .position(|c| c.starts_with(&copied))
        .unwrap_or_else(|| panic!("no upload: {calls:#?}"));
    let renders = calls
        .iter()
        .enumerate()
        .filter(|(_, c)| *c == "MTLCommandBuffer_renderCommandEncoder")
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert!(renders[0] < upload && upload < renders[1], "{calls:#?}");
    assert_eq!(draws(&calls), 1, "{calls:#?}");
}

/// A Clear, a write of the whole back buffer, then a draw: the draw's pass loads the write.
///
/// The back buffer's twin of `a_draw_after_an_upload_loads_it`: the Clear is a pass of its own
/// before the write's copy into the back buffer's texture (DXMTRS C4c-3b).
#[test]
fn a_draw_after_a_back_buffer_write_loads_it() {
    const DEVICE: u64 = 0x3b0;
    let (mut core, back_buffer) = core_and_back_buffer(DEVICE);
    core.clear(0, [1.0, 0.0, 0.0, 1.0], false);
    assert!(core.write_presented(&[0x55; 64 * 4 * 32], 64 * 4));
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 2, "{calls:#?}");
    assert_eq!(
        line(found[0], "  color0 "),
        Some("load=2 store=1 clear=1,0,0,1"),
        "{calls:#?}"
    );
    let drawn = line(found[1], "  color0 ").unwrap_or_default();
    assert!(drawn.starts_with("load=1 store=1 "), "{calls:#?}");
    let copied = format!("  blit copy_to_texture {back_buffer} ");
    let handles = targets(&calls)
        .into_iter()
        .map(|(handle, _)| handle)
        .collect::<Vec<_>>();
    assert_eq!(handles, [back_buffer.clone(), back_buffer], "{calls:#?}");
    let write = calls
        .iter()
        .position(|c| c.starts_with(&copied))
        .unwrap_or_else(|| panic!("no write: {calls:#?}"));
    let renders = calls
        .iter()
        .enumerate()
        .filter(|(_, c)| *c == "MTLCommandBuffer_renderCommandEncoder")
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert!(renders[0] < write && write < renders[1], "{calls:#?}");
    assert_eq!(draws(&calls), 1, "{calls:#?}");
}
