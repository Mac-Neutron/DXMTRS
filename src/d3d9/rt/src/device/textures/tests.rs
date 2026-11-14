use mtld3d_core::ids::TextureId;
use winemetal_sys::wire::{WMT_PIXEL_FORMAT_BC1_RGBA, WMT_PIXEL_FORMAT_BGRA8_UNORM};

use super::{CoreTextureDesc, TextureKind, UploadRegion};
use crate::device::{
    CoreDevice,
    fixture::{at, core_on_fakes},
    queue_profile_for,
};

/// A 2D BGRA8 texture of `levels` levels, `width` x `height`.
fn bgra(width: u32, height: u32, levels: u32) -> CoreTextureDesc {
    CoreTextureDesc {
        pixel_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
        kind: TextureKind::Tex2d,
        width,
        height,
        depth: 1,
        levels,
        view_twins: false,
        srgb_format: 0,
        block: (1, 1, 4),
        render_target: false,
    }
}

/// The whole of level `level`, slice `slice`.
const fn whole(level: u32, slice: u32) -> UploadRegion {
    UploadRegion {
        level,
        slice,
        x: 0,
        y: 0,
        z: 0,
        width: u32::MAX,
        height: u32::MAX,
        depth: u32::MAX,
    }
}

/// Every captured line that starts with `prefix`.
fn lines<'a>(calls: &'a [String], prefix: &str) -> Vec<&'a str> {
    calls
        .iter()
        .filter(|c| c.trim_start().starts_with(prefix))
        .map(String::as_str)
        .collect()
}

/// A texture is made once; two uploads to it are two copies in one blit pass.
#[test]
fn create_once_upload_twice() {
    const DEVICE: u64 = 0xd1;
    let mut core = core_on_fakes(DEVICE);
    let id = TextureId::from_raw(1);
    core.texture_create(id, &bgra(4, 4, 1));
    let bytes = [0x11u8; 4 * 4 * 4];
    assert!(core.texture_upload(id, whole(0, 0), &bytes, 16, 64));
    assert!(core.texture_upload(id, whole(0, 0), &bytes, 16, 64));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let made = lines(&calls, "MTLDevice_newTexture");
    assert_eq!(
        made.iter().filter(|c| c.contains("size=4x4x1")).count(),
        1,
        "{calls:#?}"
    );
    let copies = lines(&calls, "blit copy_to_texture");
    assert_eq!(copies.len(), 2, "{calls:#?}");
    assert!(copies[0].contains("size=4x4x1"), "{copies:?}");
    assert!(
        copies[0].contains("bytes_per_row=16 bytes_per_image=64"),
        "{copies:?}"
    );
    assert_eq!(
        lines(&calls, "MTLCommandBuffer_blitCommandEncoder").len(),
        1,
        "{calls:#?}"
    );
}

/// Clear, upload, Clear: a render pass, a blit pass, a render pass, in record order.
///
/// A pass with no work is dropped, so Clears stand for the draws.
#[test]
fn blit_closes_before_render() {
    const DEVICE: u64 = 0xd2;
    let mut core = core_on_fakes(DEVICE);
    let id = TextureId::from_raw(2);
    core.texture_create(id, &bgra(2, 2, 1));
    core.clear([1.0, 0.0, 0.0, 1.0], false);
    assert!(core.texture_upload(id, whole(0, 0), &[0u8; 16], 8, 16));
    core.clear([0.0, 1.0, 0.0, 1.0], false);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    let renders: Vec<usize> = calls
        .iter()
        .enumerate()
        .filter(|(_, c)| *c == "MTLCommandBuffer_renderCommandEncoder")
        .map(|(i, _)| i)
        .collect();
    assert!(renders.iter().any(|&r| r < blit), "{calls:#?}");
    assert!(renders.iter().any(|&r| r > blit), "{calls:#?}");
}

/// A BC1 mip of 2x2 texels uploads one 4x4 block: size clamped to the mip edge, 8 bytes a row.
#[test]
fn bc_small_mip_extents() {
    const DEVICE: u64 = 0xd3;
    let mut core = core_on_fakes(DEVICE);
    let id = TextureId::from_raw(3);
    let desc = CoreTextureDesc {
        pixel_format: WMT_PIXEL_FORMAT_BC1_RGBA,
        block: (4, 4, 8),
        render_target: false,
        ..bgra(8, 8, 3)
    };
    core.texture_create(id, &desc);
    assert!(core.texture_upload(id, whole(2, 0), &[0u8; 8], 8, 8));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let copy = lines(&calls, "blit copy_to_texture");
    assert_eq!(copy.len(), 1, "{calls:#?}");
    assert!(copy[0].contains("level=2"), "{copy:?}");
    assert!(copy[0].contains("size=2x2x1"), "{copy:?}");
    assert!(
        copy[0].contains("bytes_per_row=8 bytes_per_image=8"),
        "{copy:?}"
    );
}

/// A volume's region goes in z: slice 0, origin.z and size.depth from the region.
#[test]
fn volume_slice_extents() {
    const DEVICE: u64 = 0xd4;
    let mut core = core_on_fakes(DEVICE);
    let id = TextureId::from_raw(4);
    let desc = CoreTextureDesc {
        kind: TextureKind::Volume,
        depth: 4,
        ..bgra(2, 2, 1)
    };
    core.texture_create(id, &desc);
    let region = UploadRegion {
        z: 1,
        depth: 2,
        ..whole(0, 0)
    };
    assert!(core.texture_upload(id, region, &[0u8; 32], 8, 16));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let copy = lines(&calls, "blit copy_to_texture");
    assert!(
        copy[0].contains("slice=0 origin=0,0,1 size=2x2x2"),
        "{copy:?}"
    );
}

/// A source pitch wider than the row is repacked to the tight row.
#[test]
fn rows_repacked_to_tight_pitch() {
    const DEVICE: u64 = 0xd5;
    let mut core = core_on_fakes(DEVICE);
    let id = TextureId::from_raw(5);
    core.texture_create(id, &bgra(2, 2, 1));
    let mut bytes = vec![0u8; 2 * 32];
    bytes[32] = 7;
    assert!(core.texture_upload(id, whole(0, 0), &bytes, 32, 64));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let copy = lines(&calls, "blit copy_to_texture");
    assert!(
        copy[0].contains("bytes_per_row=8 bytes_per_image=16"),
        "{copy:?}"
    );
}

/// A 2 MiB upload in a 32-bit profile (1 MiB ring blocks) is one copy (an oversize block).
#[test]
fn oversize_upload_i686() {
    const DEVICE: u64 = 0xd6;
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { winemetal_sys::Obj::adopt(DEVICE) }.expect("a non-zero handle");
    let mut core = CoreDevice::new(mtl, queue_profile_for(32));
    core.attach(1, 64, 32);
    let id = TextureId::from_raw(6);
    core.texture_create(id, &bgra(1024, 512, 1));
    let bytes = vec![0u8; 1024 * 512 * 4];
    assert!(core.texture_upload(id, whole(0, 0), &bytes, 4096, 4096 * 512));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(lines(&calls, "blit copy_to_texture").len(), 1, "{calls:#?}");
}

/// Past the staged-bytes budget the chunk is committed.
///
/// Three 16 MiB uploads in a 32-bit profile (32 MiB budget) commit once, at the third.
#[test]
fn budget_commits_chunk() {
    const DEVICE: u64 = 0xd7;
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { winemetal_sys::Obj::adopt(DEVICE) }.expect("a non-zero handle");
    let mut core = CoreDevice::new(mtl, queue_profile_for(32));
    core.attach(1, 64, 32);
    let id = TextureId::from_raw(7);
    core.texture_create(id, &bgra(2048, 2048, 1));
    let bytes = vec![0u8; 2048 * 2048 * 4];
    let before = core.device().queue().current_seq();
    for _ in 0..3 {
        assert!(core.texture_upload(id, whole(0, 0), &bytes, 8192, 8192 * 2048));
    }
    assert_eq!(core.device().queue().current_seq() - before, 1);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
}

/// A generated mip chain is one blit record after the level-0 upload.
#[test]
fn generate_mips_after_upload() {
    const DEVICE: u64 = 0xd8;
    let mut core = core_on_fakes(DEVICE);
    let id = TextureId::from_raw(8);
    core.texture_create(id, &bgra(4, 4, 3));
    assert!(core.texture_upload(id, whole(0, 0), &[0u8; 64], 16, 64));
    core.texture_generate_mips(id);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let copy = calls
        .iter()
        .position(|c| c.trim_start().starts_with("blit copy_to_texture"))
        .unwrap_or_else(|| panic!("{calls:#?}"));
    let gen_at = calls
        .iter()
        .position(|c| c.trim_start().starts_with("blit generate_mipmaps"))
        .unwrap_or_else(|| panic!("{calls:#?}"));
    assert!(gen_at > copy, "{calls:#?}");
}

/// Forgetting a texture drops the store's entry; what was recorded still runs.
#[test]
fn forget_keeps_encoded() {
    const DEVICE: u64 = 0xd9;
    let mut core = core_on_fakes(DEVICE);
    let id = TextureId::from_raw(9);
    core.texture_create(id, &bgra(2, 2, 1));
    assert!(core.texture_upload(id, whole(0, 0), &[0u8; 16], 8, 16));
    core.forget_texture(id);
    assert!(!core.has_texture(id));
    assert!(!core.texture_upload(id, whole(0, 0), &[0u8; 16], 8, 16));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(lines(&calls, "blit copy_to_texture").len(), 1, "{calls:#?}");
}
