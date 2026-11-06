//! The old tables are the oracles.
//!
//! `d3d11/src/state.rs`, `d3d12/src/pipeline/graphics.rs` and `d3d12/src/sampler.rs`.

use winemetal_wire::{
    WMT_COLOR_WRITE_MASK_ALPHA, WMT_COLOR_WRITE_MASK_BLUE, WMT_COLOR_WRITE_MASK_GREEN,
    WMT_COLOR_WRITE_MASK_NONE, WMT_COLOR_WRITE_MASK_RED,
};

use super::{
    address_mode, blend_alpha_factor, blend_factor, blend_factor_is_dual_source, blend_op,
    color_write_mask, compare_func, stencil_op,
};

const D3D11_BLEND_FACTOR_MAP: [u8; 20] = [
    0, 0, 1, 2, 3, 4, 5, 8, 9, 6, 7, 10, 0, 0, 11, 12, 15, 16, 17, 18,
];
const D3D11_BLEND_ALPHA_FACTOR_MAP: [u8; 20] = [
    0, 0, 1, 2, 3, 4, 5, 8, 9, 6, 7, 10, 0, 0, 13, 14, 15, 16, 17, 18,
];
const D3D11_COLOR_WRITE_MASK_MAP: [u8; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
const D3D11_BLEND_OP_MAP: [u8; 6] = [0, 0, 1, 2, 3, 4];
const D3D11_COMPARE_FUNC_MAP: [u8; 9] = [0, 0, 1, 2, 3, 4, 5, 6, 7];
const D3D11_STENCIL_OP_MAP: [u8; 9] = [1, 0, 1, 2, 3, 4, 5, 6, 7];
const D3D11_ADDRESS_MODE_MAP: [u8; 6] = [0, 2, 3, 0, 5, 1];

fn check_table(table: &[u8], f: fn(u32) -> Option<u8>) {
    for (index, expected) in table.iter().enumerate() {
        assert_eq!(
            f(u32::try_from(index).unwrap()),
            Some(*expected),
            "value {index}"
        );
    }
    assert_eq!(f(u32::try_from(table.len()).unwrap()), None);
    assert_eq!(f(1000), None);
}

#[test]
fn d3d11_tables() {
    check_table(&D3D11_BLEND_OP_MAP, blend_op);
    check_table(&D3D11_COMPARE_FUNC_MAP, compare_func);
    check_table(&D3D11_STENCIL_OP_MAP, stencil_op);
    check_table(&D3D11_COLOR_WRITE_MASK_MAP, color_write_mask);
    // The address map's 0 is not a mode: d3d11's table answers clamp-to-edge for it.
    for (index, expected) in D3D11_ADDRESS_MODE_MAP.iter().enumerate().skip(1) {
        assert_eq!(
            address_mode(u32::try_from(index).unwrap()),
            Some(*expected),
            "mode {index}"
        );
    }
    assert_eq!(address_mode(0), None);
    assert_eq!(address_mode(6), None);
    // D3D11_BLEND ends at 19; 20 and 21 are D3D12's.
    for index in 0..20 {
        let i = index as usize;
        assert_eq!(
            blend_factor(index),
            Some(D3D11_BLEND_FACTOR_MAP[i]),
            "factor {index}"
        );
        assert_eq!(
            blend_alpha_factor(index),
            Some(D3D11_BLEND_ALPHA_FACTOR_MAP[i]),
            "alpha factor {index}"
        );
    }
    assert_eq!(blend_factor(22), None);
    assert_eq!(blend_alpha_factor(22), None);
}

/// d3d12's `match`, with its `_ => ZERO`, for the values `D3D12_BLEND` has.
fn d3d12_blend_factor(factor: u32) -> u8 {
    match factor {
        2 => 1,
        3 => 2,
        4 => 3,
        5 => 4,
        6 => 5,
        7 => 8,
        8 => 9,
        9 => 6,
        10 => 7,
        11 => 10,
        14 => 11,
        15 => 12,
        16 => 15,
        17 => 16,
        18 => 17,
        19 => 18,
        20 => 13,
        21 => 14,
        _ => 0,
    }
}

#[test]
fn d3d12_blend_factors() {
    for factor in 0..=21 {
        let alpha = match factor {
            14 => 13,
            15 => 14,
            other => d3d12_blend_factor(other),
        };
        assert_eq!(
            blend_factor(factor),
            Some(d3d12_blend_factor(factor)),
            "{factor}"
        );
        assert_eq!(blend_alpha_factor(factor), Some(alpha), "alpha {factor}");
    }
    let dual: Vec<u32> = (0..30)
        .filter(|f| blend_factor_is_dual_source(*f))
        .collect();
    assert_eq!(dual, [16, 17, 18, 19]);
}

#[test]
fn color_write_mask_is_the_four_bits_reversed() {
    let one_bit = [
        (1, WMT_COLOR_WRITE_MASK_RED),
        (2, WMT_COLOR_WRITE_MASK_GREEN),
        (4, WMT_COLOR_WRITE_MASK_BLUE),
        (8, WMT_COLOR_WRITE_MASK_ALPHA),
        (0, WMT_COLOR_WRITE_MASK_NONE),
    ];
    for (mask, expected) in one_bit {
        assert_eq!(color_write_mask(mask), Some(expected));
    }
}
