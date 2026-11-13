use mtld3d_core::ids::TextureId;
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_RENDER_TARGET,
    WMTTextureInfo,
};

use super::TargetRef;
use crate::device::{
    CoreDevice, TextureKind,
    fixture::{core_on_fakes, target_desc, targets, textures_made},
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
    let id = TextureId::from_raw(id);
    core.texture_create(
        id,
        &target_desc(WMT_PIXEL_FORMAT_BGRA8_UNORM, kind, size, levels, 4),
    );
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
    core.clear([1.0, 0.0, 0.0, 1.0]);
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
    desc.render_target = false;
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
