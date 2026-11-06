use winemetal_wire::{WMT_PIXEL_FORMAT_INVALID, WMT_PIXEL_FORMAT_RGBA8_UNORM};

use super::{
    Decomposed, LinearLayout, calc_subresource, clamped_mip_count, decompose_subresource,
    full_mip_count, linear_layout, linear_row_pitch, mip_count_wh, mip_count_whd, mip_extent,
    mip_extent_wrapping,
};
use crate::format::{MTL_DXGI_FORMAT_BC, MTL_DXGI_FORMAT_DEPTH_PLANER, MtlDxgiFormatDesc};

const fn format(pixel_format: u32, bytes_per_texel: u32, flag: u16) -> MtlDxgiFormatDesc {
    MtlDxgiFormatDesc {
        pixel_format,
        attribute_format: 0,
        bytes_per_texel,
        planar_count: 1,
        flag,
    }
}

#[test]
fn extents_halve_to_one() {
    assert_eq!(
        [0, 1, 2, 3, 4].map(|level| mip_extent(256, level)),
        [256, 128, 64, 32, 16]
    );
    assert_eq!(mip_extent(5, 1), 2);
    assert_eq!(mip_extent(256, 8), 1);
    assert_eq!(mip_extent(256, 9), 1);
    assert_eq!(mip_extent(0, 0), 1);
}

/// Past 31 the two rules part: D3D11 answers 1, the hardware's shift wraps to the low five bits.
#[test]
fn the_two_extent_rules_part_at_level_32() {
    assert_eq!(mip_extent(256, 32), 1);
    assert_eq!(mip_extent_wrapping(256, 32), 256);
    assert_eq!(mip_extent_wrapping(256, 33), 128);
    for level in 0..32 {
        assert_eq!(mip_extent(1000, level), mip_extent_wrapping(1000, level));
    }
}

#[test]
fn mip_counts_by_rule() {
    assert_eq!(full_mip_count(0), 0);
    assert_eq!(full_mip_count(1), 1);
    assert_eq!(full_mip_count(256), 9);
    assert_eq!(mip_count_wh(256, 64), 9);
    // A 3D texture 4 by 4 by 64: D3D11's rule ignores the depth, D3D12's does not.
    assert_eq!(mip_count_wh(4, 4), 3);
    assert_eq!(mip_count_whd(4, 4, 64), 7);
    assert_eq!(clamped_mip_count(0, 0, 0, 0), 1);
    assert_eq!(clamped_mip_count(256, 256, 1, 0), 9);
    assert_eq!(clamped_mip_count(256, 256, 1, 4), 4);
    assert_eq!(clamped_mip_count(256, 256, 1, 20), 9);
}

#[test]
fn subresources_compose_and_decompose() {
    assert_eq!(calc_subresource(2, 3, 5), 17);
    assert_eq!(
        decompose_subresource(17, 5, 4),
        Decomposed {
            mip_slice: 2,
            array_slice: 3,
            plane_slice: 0
        }
    );
    // The second plane of a two-plane format: subresource 20 with 5 mips and 4 slices.
    assert_eq!(
        decompose_subresource(20, 5, 4),
        Decomposed {
            mip_slice: 0,
            array_slice: 0,
            plane_slice: 1
        }
    );
    let none = Decomposed {
        mip_slice: 0,
        array_slice: 0,
        plane_slice: 0,
    };
    assert_eq!(decompose_subresource(9, 0, 4), none);
    assert_eq!(
        decompose_subresource(9, 5, 0),
        Decomposed {
            mip_slice: 4,
            array_slice: 0,
            plane_slice: 0
        }
    );
    assert_eq!(calc_subresource(u32::MAX, 1, 1), u32::MAX.wrapping_add(1));
}

#[test]
fn linear_layouts_align_rows() {
    let rgba = format(WMT_PIXEL_FORMAT_RGBA8_UNORM, 4, 0);
    // 5 texels of 4 bytes: 20 bytes, to 16 when aligned and to 4 otherwise.
    assert_eq!(linear_row_pitch(&rgba, 5, true), Some(32));
    assert_eq!(linear_row_pitch(&rgba, 5, false), Some(20));
    assert_eq!(
        linear_layout(&rgba, 5, 3, 2, true),
        Some(LinearLayout {
            bytes_per_row: 32,
            bytes_per_image: 96,
            bytes_per_slice: 192
        })
    );
    let depth = format(
        WMT_PIXEL_FORMAT_RGBA8_UNORM,
        4,
        MTL_DXGI_FORMAT_DEPTH_PLANER,
    );
    assert_eq!(linear_row_pitch(&depth, 5, true), Some(20));
}

#[test]
fn block_compressed_layouts_count_whole_blocks() {
    // BC1: an 8-byte block. 6 by 5 texels cover 8 by 8: two blocks across, two down.
    let bc1 = format(WMT_PIXEL_FORMAT_RGBA8_UNORM, 8, MTL_DXGI_FORMAT_BC);
    assert_eq!(
        linear_layout(&bc1, 6, 5, 1, true),
        Some(LinearLayout {
            bytes_per_row: 16,
            bytes_per_image: 32,
            bytes_per_slice: 32
        })
    );
}

#[test]
fn a_format_without_a_layout_has_none() {
    assert_eq!(
        linear_layout(&format(WMT_PIXEL_FORMAT_INVALID, 4, 0), 4, 4, 1, true),
        None
    );
    assert_eq!(
        linear_row_pitch(&format(WMT_PIXEL_FORMAT_RGBA8_UNORM, 0, 0), 4, true),
        None
    );
}
