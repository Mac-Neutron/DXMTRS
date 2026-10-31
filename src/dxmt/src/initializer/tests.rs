use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_BC1_RGBA, WMT_PIXEL_FORMAT_R8_UNORM, WMT_PIXEL_FORMAT_RGBA8_UNORM,
    WMT_TEXTURE_TYPE1D, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE3D,
};

use super::*;

/// The layout's row, row count, image and depth.
fn shape(l: &Layout) -> (usize, usize, usize, u32) {
    (l.bytes_per_row, l.rows, l.bytes_per_image, l.depth)
}

#[test]
fn upload_rows_as_cpp() {
    let data: Vec<u8> = (1..=40).collect();
    // RGBA8 3x2 with rows 16 bytes apart: each row's 12 bytes, packed.
    let l = Layout::new(
        WMT_PIXEL_FORMAT_RGBA8_UNORM,
        WMT_TEXTURE_TYPE2D,
        [3, 2, 1],
        0,
    );
    assert_eq!(shape(&l), (12, 2, 24, 1));
    assert_eq!(
        pack(&data, &l, 16, 0),
        [&data[0..12], &data[16..28]].concat()
    );
    // A pitch shorter than a row copies the pitch; the rest of the row stays 0.
    assert_eq!(
        pack(&data, &l, 8, 0),
        [&data[0..8], &[0; 4], &data[8..16], &[0; 4]].concat()
    );
    // Level 1 of 7x5 is 3x2.
    let l = Layout::new(
        WMT_PIXEL_FORMAT_RGBA8_UNORM,
        WMT_TEXTURE_TYPE2D,
        [7, 5, 1],
        1,
    );
    assert_eq!((l.width, l.height), (3, 2));
    // BC1 8x8: two rows of two 8-byte blocks; a pitch equal to the row copies the image at once.
    let l = Layout::new(WMT_PIXEL_FORMAT_BC1_RGBA, WMT_TEXTURE_TYPE2D, [8, 8, 1], 0);
    assert_eq!(shape(&l), (16, 2, 32, 1));
    assert_eq!(pack(&data, &l, 16, 0), &data[0..32]);
    // Its level 3 is 1x1: one block.
    let l = Layout::new(WMT_PIXEL_FORMAT_BC1_RGBA, WMT_TEXTURE_TYPE2D, [8, 8, 1], 3);
    assert_eq!(shape(&l), (8, 1, 8, 1));
    // A 1D texture ignores the row pitch.
    let l = Layout::new(WMT_PIXEL_FORMAT_R8_UNORM, WMT_TEXTURE_TYPE1D, [4, 1, 1], 0);
    assert_eq!(pack(&data, &l, 0, 0), &data[0..4]);
    // A 3D texture's slices are the depth pitch apart: 2x2x2 R8 at a depth pitch of 6.
    let l = Layout::new(WMT_PIXEL_FORMAT_R8_UNORM, WMT_TEXTURE_TYPE3D, [2, 2, 2], 0);
    assert_eq!(shape(&l), (2, 2, 4, 2));
    assert_eq!(pack(&data, &l, 2, 6), [&data[0..4], &data[6..10]].concat());
    // Data shorter than the pitches say is copied as far as it goes.
    assert_eq!(pack(&data[..7], &l, 2, 6), [1, 2, 3, 4, 7, 0, 0, 0]);
}

#[test]
fn depth_stencil_splits_as_cpp() {
    // D24S8 3x1: the depth is the low 24 bits over 0xffffff, the stencil the top byte; stencil
    // rows are padded to 4 bytes.
    let texels: [u32; 3] = [0xffff_ffff, 0x0100_0000, 0x7f80_0000];
    let data: Vec<u8> = texels.iter().flat_map(|t| t.to_ne_bytes()).collect();
    let (depth, stencil) = split_depth_stencil(&data, 3, 1, 12, true);
    let expected: Vec<u8> = [1.0_f32, 0.0, 8_388_608.0 / 16_777_215.0]
        .iter()
        .flat_map(|d| d.to_ne_bytes())
        .collect();
    assert_eq!(depth, expected);
    assert_eq!(stencil, [0xff, 0x01, 0x7f, 0]);
    // D32S8X24 2x2 with rows 20 bytes apart: each texel's 4 depth bytes and the byte after.
    let data: Vec<u8> = (0..40).collect();
    let (depth, stencil) = split_depth_stencil(&data, 2, 2, 20, false);
    assert_eq!(
        depth,
        [0, 1, 2, 3, 8, 9, 10, 11, 20, 21, 22, 23, 28, 29, 30, 31]
    );
    assert_eq!(stencil, [4, 12, 0, 0, 24, 32, 0, 0]);
}
