use winemetal_wire::{
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_R32_FLOAT, WMT_PIXEL_FORMAT_R32_UINT,
    WMT_PIXEL_FORMAT_RG32_UINT, WMT_PIXEL_FORMAT_RGBA16_FLOAT, WMT_TEXTURE_TYPE_CUBE,
    WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D,
};

use super::{Dimension, atomic_ok, info_bits, lossless, texture_type};
use crate::settings::Compression;

#[test]
fn shapes_pick_their_type() {
    let t1 = |a| texture_type(Dimension::Texture1D, a, 1, false);
    let t2 = |a, s, c| texture_type(Dimension::Texture2D, a, s, c);
    assert_eq!(
        [t1(1), t1(0), t1(2)],
        [
            WMT_TEXTURE_TYPE2D,
            WMT_TEXTURE_TYPE2D,
            WMT_TEXTURE_TYPE2D_ARRAY
        ]
    );
    assert_eq!(t2(6, 4, true), WMT_TEXTURE_TYPE_CUBE);
    assert_eq!(t2(11, 1, true), WMT_TEXTURE_TYPE_CUBE);
    assert_eq!(t2(12, 1, true), WMT_TEXTURE_TYPE_CUBE_ARRAY);
    assert_eq!(t2(1, 4, false), WMT_TEXTURE_TYPE2D_MULTISAMPLE);
    assert_eq!(t2(2, 4, false), WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY);
    assert_eq!(t2(2, 1, false), WMT_TEXTURE_TYPE2D_ARRAY);
    assert_eq!(t2(1, 0, false), WMT_TEXTURE_TYPE2D);
    assert_eq!(
        texture_type(Dimension::Texture3D, 9, 4, true),
        WMT_TEXTURE_TYPE3D
    );
    assert_eq!(info_bits(0x103, 0x1ff, 4, 0x13), 0x1304_ff03);
}

#[test]
fn atomics_and_compression_as_the_front_ends() {
    let mut asked = false;
    assert!(atomic_ok(WMT_PIXEL_FORMAT_R32_UINT, || {
        asked = true;
        false
    }));
    assert!(!asked, "only RG32Uint asks for Apple8");
    assert!(atomic_ok(WMT_PIXEL_FORMAT_RG32_UINT, || true));
    assert!(!atomic_ok(WMT_PIXEL_FORMAT_RG32_UINT, || false));
    assert!(!atomic_ok(WMT_PIXEL_FORMAT_R32_FLOAT, || true));

    assert!(!lossless(
        Compression::Off,
        WMT_PIXEL_FORMAT_RGBA16_FLOAT,
        false
    ));
    assert!(lossless(
        Compression::RenderTargets,
        WMT_PIXEL_FORMAT_BGRA8_UNORM,
        false
    ));
    assert!(!lossless(
        Compression::RenderTargets,
        WMT_PIXEL_FORMAT_RGBA16_FLOAT,
        true
    ));
    assert!(!lossless(
        Compression::Default,
        WMT_PIXEL_FORMAT_BGRA8_UNORM,
        true
    ));
    assert!(lossless(
        Compression::Default,
        WMT_PIXEL_FORMAT_R32_UINT,
        true
    ));
    assert!(lossless(
        Compression::Default,
        WMT_PIXEL_FORMAT_RGBA16_FLOAT,
        true
    ));
}
