//! The old tables are the oracles.
//!
//! `d3d11/src/state.rs`, `d3d12/src/pipeline/graphics.rs` and `d3d12/src/sampler.rs`.

use winemetal_wire::{
    WMT_COLOR_WRITE_MASK_ALPHA, WMT_COLOR_WRITE_MASK_BLUE, WMT_COLOR_WRITE_MASK_GREEN,
    WMT_COLOR_WRITE_MASK_NONE, WMT_COLOR_WRITE_MASK_RED,
};

use super::{
    DepthStencilDesc, RtBlend, StencilFace, address_mode, blend_alpha_factor, blend_factor,
    blend_factor_is_dual_source, blend_op, color_write_mask, compare_func, depth_stencil_info,
    fill_blend, stencil_op,
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

/// Depth off is `ALWAYS` without writes, stencil off a zeroed face; out-of-range answers per API.
#[test]
fn depth_stencil_info_as_both_apis() {
    use winemetal_wire::{
        WMT_COMPARE_FUNCTION_ALWAYS, WMT_COMPARE_FUNCTION_LESS, WMT_COMPARE_FUNCTION_NEVER,
        WMT_STENCIL_OPERATION_INVERT, WMT_STENCIL_OPERATION_KEEP, WMT_STENCIL_OPERATION_REPLACE,
        WMT_STENCIL_OPERATION_ZERO,
    };
    let face = |pass_op| StencilFace {
        fail_op: 3,
        depth_fail_op: 6,
        pass_op,
        func: u32::MAX,
    };
    let mut desc = DepthStencilDesc {
        depth_enable: false,
        depth_write: true,
        depth_func: 2,
        stencil_enable: false,
        read_mask: 0x5a,
        write_mask: 0xa5,
        front: face(9),
        back: face(1),
    };
    let off = depth_stencil_info(&desc, WMT_STENCIL_OPERATION_ZERO);
    assert_eq!(
        (off.depth_compare_function, off.depth_write_enabled),
        (WMT_COMPARE_FUNCTION_ALWAYS, 0)
    );
    assert_eq!(
        (off.front_stencil.enabled, off.front_stencil.read_mask),
        (0, 0)
    );

    desc.depth_enable = true;
    desc.stencil_enable = true;
    for (fallback, out_of_range) in [
        (WMT_STENCIL_OPERATION_ZERO, WMT_STENCIL_OPERATION_ZERO),
        (WMT_STENCIL_OPERATION_KEEP, WMT_STENCIL_OPERATION_KEEP),
    ] {
        let on = depth_stencil_info(&desc, fallback);
        assert_eq!(
            (on.depth_compare_function, on.depth_write_enabled),
            (WMT_COMPARE_FUNCTION_LESS, 1)
        );
        let f = on.front_stencil;
        assert_eq!(
            (
                f.enabled,
                f.stencil_fail_op,
                f.depth_fail_op,
                f.depth_stencil_pass_op
            ),
            (
                1,
                WMT_STENCIL_OPERATION_REPLACE,
                WMT_STENCIL_OPERATION_INVERT,
                out_of_range
            )
        );
        assert_eq!(f.stencil_compare_function, WMT_COMPARE_FUNCTION_NEVER);
        assert_eq!((f.read_mask, f.write_mask), (0x5a, 0xa5));
        assert_eq!(
            on.back_stencil.depth_stencil_pass_op,
            WMT_STENCIL_OPERATION_KEEP
        );
    }
}

/// Out-of-range blend values answer `ADD` and `ZERO` as both front ends did, 20 and 21 per API.
#[test]
fn fill_blend_as_both_apis() {
    use winemetal_wire::{
        WMT_BLEND_FACTOR_BLEND_ALPHA, WMT_BLEND_FACTOR_ONE_MINUS_BLEND_ALPHA,
        WMT_BLEND_FACTOR_SOURCE1_COLOR, WMT_BLEND_FACTOR_ZERO, WMT_BLEND_OPERATION_ADD,
        WMT_BLEND_OPERATION_MAX, WMTColorAttachmentBlendInfo,
    };
    let fill = |rt: &RtBlend, end| {
        let mut a = WMTColorAttachmentBlendInfo {
            pixel_format: 0,
            rgb_blend_operation: 0xa5,
            alpha_blend_operation: 0xa5,
            src_rgb_blend_factor: 0xa5,
            dst_rgb_blend_factor: 0xa5,
            src_alpha_blend_factor: 0xa5,
            dst_alpha_blend_factor: 0xa5,
            write_mask: 0xa5,
            blending_enabled: 0xa5,
        };
        fill_blend(&mut a, rt, end);
        assert_eq!((a.write_mask, a.blending_enabled), (0xa5, 0xa5));
        a
    };
    for end in [20, 22] {
        for bad in [0, 22, u32::MAX] {
            let rt = RtBlend {
                src: bad,
                dest: bad,
                op: 6,
                src_alpha: bad,
                dest_alpha: bad,
                op_alpha: u32::MAX,
            };
            let a = fill(&rt, end);
            assert_eq!(
                [
                    a.src_rgb_blend_factor,
                    a.dst_rgb_blend_factor,
                    a.src_alpha_blend_factor,
                    a.dst_alpha_blend_factor
                ],
                [WMT_BLEND_FACTOR_ZERO; 4]
            );
            assert_eq!(
                (a.rgb_blend_operation, a.alpha_blend_operation),
                (WMT_BLEND_OPERATION_ADD, WMT_BLEND_OPERATION_ADD)
            );
        }
    }
    let rt = RtBlend {
        src: 16,
        dest: 20,
        op: 5,
        src_alpha: 14,
        dest_alpha: 15,
        op_alpha: 1,
    };
    let d3d12 = fill(&rt, 22);
    assert_eq!(
        (d3d12.src_rgb_blend_factor, d3d12.dst_rgb_blend_factor),
        (WMT_BLEND_FACTOR_SOURCE1_COLOR, WMT_BLEND_FACTOR_BLEND_ALPHA)
    );
    assert_eq!(
        (d3d12.src_alpha_blend_factor, d3d12.dst_alpha_blend_factor),
        (
            WMT_BLEND_FACTOR_BLEND_ALPHA,
            WMT_BLEND_FACTOR_ONE_MINUS_BLEND_ALPHA
        )
    );
    assert_eq!(d3d12.rgb_blend_operation, WMT_BLEND_OPERATION_MAX);
    let d3d11 = fill(&rt, 20);
    assert_eq!(d3d11.dst_rgb_blend_factor, WMT_BLEND_FACTOR_ZERO);
    assert_eq!(d3d11.src_alpha_blend_factor, WMT_BLEND_FACTOR_BLEND_ALPHA);
}
