//! D3D blend, comparison, stencil, address and write-mask enums as Metal's values.
//!
//! D3D11 and D3D12 number these enums alike. Every function answers `None` for a value outside the
//! enum, so each front end chooses its own answer there: D3D11 used a table lookup falling back
//! to 0, D3D12 a `match` falling back to `ZERO`, `NEVER`, `KEEP`'s neighbour or `ADD`.

use winemetal_wire::{
    WMT_BLEND_FACTOR_BLEND_ALPHA, WMT_BLEND_FACTOR_BLEND_COLOR, WMT_BLEND_FACTOR_DESTINATION_ALPHA,
    WMT_BLEND_FACTOR_DESTINATION_COLOR, WMT_BLEND_FACTOR_ONE,
    WMT_BLEND_FACTOR_ONE_MINUS_BLEND_ALPHA, WMT_BLEND_FACTOR_ONE_MINUS_BLEND_COLOR,
    WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_ALPHA, WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_COLOR,
    WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA, WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_COLOR,
    WMT_BLEND_FACTOR_ONE_MINUS_SOURCE1_ALPHA, WMT_BLEND_FACTOR_ONE_MINUS_SOURCE1_COLOR,
    WMT_BLEND_FACTOR_SOURCE_ALPHA, WMT_BLEND_FACTOR_SOURCE_ALPHA_SATURATED,
    WMT_BLEND_FACTOR_SOURCE_COLOR, WMT_BLEND_FACTOR_SOURCE1_ALPHA, WMT_BLEND_FACTOR_SOURCE1_COLOR,
    WMT_BLEND_FACTOR_ZERO, WMT_BLEND_OPERATION_ADD, WMT_BLEND_OPERATION_MAX,
    WMT_BLEND_OPERATION_MIN, WMT_BLEND_OPERATION_REVERSE_SUBTRACT, WMT_BLEND_OPERATION_SUBTRACT,
    WMT_COLOR_WRITE_MASK_ALPHA, WMT_COLOR_WRITE_MASK_BLUE, WMT_COLOR_WRITE_MASK_GREEN,
    WMT_COLOR_WRITE_MASK_NONE, WMT_COLOR_WRITE_MASK_RED, WMT_COMPARE_FUNCTION_ALWAYS,
    WMT_COMPARE_FUNCTION_EQUAL, WMT_COMPARE_FUNCTION_GREATER, WMT_COMPARE_FUNCTION_GREATER_EQUAL,
    WMT_COMPARE_FUNCTION_LESS, WMT_COMPARE_FUNCTION_LESS_EQUAL, WMT_COMPARE_FUNCTION_NEVER,
    WMT_COMPARE_FUNCTION_NOT_EQUAL, WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR,
    WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE, WMT_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE,
    WMT_SAMPLER_ADDRESS_MODE_MIRROR_REPEAT, WMT_SAMPLER_ADDRESS_MODE_REPEAT,
    WMT_STENCIL_OPERATION_DECREMENT_CLAMP, WMT_STENCIL_OPERATION_DECREMENT_WRAP,
    WMT_STENCIL_OPERATION_INCREMENT_CLAMP, WMT_STENCIL_OPERATION_INCREMENT_WRAP,
    WMT_STENCIL_OPERATION_INVERT, WMT_STENCIL_OPERATION_KEEP, WMT_STENCIL_OPERATION_REPLACE,
    WMT_STENCIL_OPERATION_ZERO, WMTDepthStencilInfo, WMTStencilInfo,
};

/// Metal's `WMTBlendFactor` for a `D3D*_BLEND` (0 to 21), `ZERO` for the unused 0, 12 and 13.
///
/// 20 and 21 (`ALPHA_FACTOR`, `INV_ALPHA_FACTOR`) are D3D12's: `D3D11_BLEND` ends at 19, and a
/// D3D11 caller refuses 20 and above as it always has. `BLEND_FACTOR` (14) and `INV_BLEND_FACTOR`
/// (15) are the blend colour here; [`blend_alpha_factor`] gives the alpha channel's.
#[must_use]
pub const fn blend_factor(factor: u32) -> Option<u8> {
    Some(match factor {
        0 | 1 | 12 | 13 => WMT_BLEND_FACTOR_ZERO,
        2 => WMT_BLEND_FACTOR_ONE,
        3 => WMT_BLEND_FACTOR_SOURCE_COLOR,
        4 => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_COLOR,
        5 => WMT_BLEND_FACTOR_SOURCE_ALPHA,
        6 => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA,
        7 => WMT_BLEND_FACTOR_DESTINATION_ALPHA,
        8 => WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_ALPHA,
        9 => WMT_BLEND_FACTOR_DESTINATION_COLOR,
        10 => WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_COLOR,
        11 => WMT_BLEND_FACTOR_SOURCE_ALPHA_SATURATED,
        14 => WMT_BLEND_FACTOR_BLEND_COLOR,
        15 => WMT_BLEND_FACTOR_ONE_MINUS_BLEND_COLOR,
        16 => WMT_BLEND_FACTOR_SOURCE1_COLOR,
        17 => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE1_COLOR,
        18 => WMT_BLEND_FACTOR_SOURCE1_ALPHA,
        19 => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE1_ALPHA,
        20 => WMT_BLEND_FACTOR_BLEND_ALPHA,
        21 => WMT_BLEND_FACTOR_ONE_MINUS_BLEND_ALPHA,
        _ => return None,
    })
}

/// Metal's `WMTBlendFactor` for a `D3D*_BLEND` in the alpha channel.
///
/// [`blend_factor`], except that `BLEND_FACTOR` (14) and `INV_BLEND_FACTOR` (15) read the blend
/// colour's alpha.
#[must_use]
pub const fn blend_alpha_factor(factor: u32) -> Option<u8> {
    match factor {
        14 => Some(WMT_BLEND_FACTOR_BLEND_ALPHA),
        15 => Some(WMT_BLEND_FACTOR_ONE_MINUS_BLEND_ALPHA),
        other => blend_factor(other),
    }
}

/// Whether a `D3D*_BLEND` reads the second pixel shader output (`SRC1_COLOR` to `INV_SRC1_ALPHA`).
#[must_use]
pub const fn blend_factor_is_dual_source(factor: u32) -> bool {
    matches!(factor, 16..=19)
}

/// Metal's `WMTBlendOperation` for a `D3D*_BLEND_OP` (0 to 5), `ADD` for the unused 0.
#[must_use]
pub const fn blend_op(op: u32) -> Option<u8> {
    Some(match op {
        0 | 1 => WMT_BLEND_OPERATION_ADD,
        2 => WMT_BLEND_OPERATION_SUBTRACT,
        3 => WMT_BLEND_OPERATION_REVERSE_SUBTRACT,
        4 => WMT_BLEND_OPERATION_MIN,
        5 => WMT_BLEND_OPERATION_MAX,
        _ => return None,
    })
}

/// Metal's `WMTCompareFunction` for a `D3D*_COMPARISON_FUNC` (0 to 8), `NEVER` for the unused 0.
#[must_use]
pub const fn compare_func(func: u32) -> Option<u8> {
    Some(match func {
        0 | 1 => WMT_COMPARE_FUNCTION_NEVER,
        2 => WMT_COMPARE_FUNCTION_LESS,
        3 => WMT_COMPARE_FUNCTION_EQUAL,
        4 => WMT_COMPARE_FUNCTION_LESS_EQUAL,
        5 => WMT_COMPARE_FUNCTION_GREATER,
        6 => WMT_COMPARE_FUNCTION_NOT_EQUAL,
        7 => WMT_COMPARE_FUNCTION_GREATER_EQUAL,
        8 => WMT_COMPARE_FUNCTION_ALWAYS,
        _ => return None,
    })
}

/// Metal's `WMTStencilOperation` for a `D3D*_STENCIL_OP` (0 to 8), `ZERO` for the unused 0.
#[must_use]
pub const fn stencil_op(op: u32) -> Option<u8> {
    Some(match op {
        0 | 2 => WMT_STENCIL_OPERATION_ZERO,
        1 => WMT_STENCIL_OPERATION_KEEP,
        3 => WMT_STENCIL_OPERATION_REPLACE,
        4 => WMT_STENCIL_OPERATION_INCREMENT_CLAMP,
        5 => WMT_STENCIL_OPERATION_DECREMENT_CLAMP,
        6 => WMT_STENCIL_OPERATION_INVERT,
        7 => WMT_STENCIL_OPERATION_INCREMENT_WRAP,
        8 => WMT_STENCIL_OPERATION_DECREMENT_WRAP,
        _ => return None,
    })
}

/// Metal's `WMTSamplerAddressMode` for a `D3D*_TEXTURE_ADDRESS_MODE`.
///
/// `WRAP` (1) to `MIRROR_ONCE` (5); nothing for 0 and values above 5.
#[must_use]
pub const fn address_mode(mode: u32) -> Option<u8> {
    match mode {
        1 => Some(WMT_SAMPLER_ADDRESS_MODE_REPEAT),
        2 => Some(WMT_SAMPLER_ADDRESS_MODE_MIRROR_REPEAT),
        3 => Some(WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE),
        4 => Some(WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR),
        5 => Some(WMT_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE),
        _ => None,
    }
}

/// Metal's `WMTColorWriteMask` for a `D3D*_COLOR_WRITE_ENABLE` mask.
///
/// D3D's red is 1, green 2, blue 4 and alpha 8. Nothing for a mask above 15.
#[must_use]
pub const fn color_write_mask(mask: u32) -> Option<u8> {
    const RGB: [u8; 4] = [
        WMT_COLOR_WRITE_MASK_RED,
        WMT_COLOR_WRITE_MASK_GREEN,
        WMT_COLOR_WRITE_MASK_BLUE,
        WMT_COLOR_WRITE_MASK_ALPHA,
    ];
    if mask > 15 {
        return None;
    }
    let mut out = WMT_COLOR_WRITE_MASK_NONE;
    let mut bit = 0;
    while bit < 4 {
        if mask & (1 << bit) != 0 {
            out |= RGB[bit];
        }
        bit += 1;
    }
    Some(out)
}

/// One face's stencil operations, as D3D11 and D3D12 both describe them.
pub struct StencilFace {
    pub fail_op: u32,
    pub depth_fail_op: u32,
    pub pass_op: u32,
    pub func: u32,
}

/// A depth-stencil description, as D3D11 and D3D12 both give it, its `BOOL`s read.
pub struct DepthStencilDesc {
    pub depth_enable: bool,
    /// `DEPTH_WRITE_MASK_ALL`.
    pub depth_write: bool,
    pub depth_func: u32,
    pub stencil_enable: bool,
    pub read_mask: u8,
    pub write_mask: u8,
    pub front: StencilFace,
    pub back: StencilFace,
}

/// Metal's description of `desc`.
///
/// Depth off is `ALWAYS` without writes, stencil off a zeroed face. A comparison outside the enum
/// is `NEVER`; a stencil operation outside it is `stencil_op_fallback`, which differs per API
/// (D3D11 `KEEP`, D3D12 `ZERO`).
#[must_use]
pub const fn depth_stencil_info(
    desc: &DepthStencilDesc,
    stencil_op_fallback: u8,
) -> WMTDepthStencilInfo {
    const OFF: WMTStencilInfo = WMTStencilInfo {
        enabled: 0,
        depth_stencil_pass_op: 0,
        stencil_fail_op: 0,
        depth_fail_op: 0,
        stencil_compare_function: 0,
        write_mask: 0,
        read_mask: 0,
    };
    const fn or(mapped: Option<u8>, fallback: u8) -> u8 {
        match mapped {
            Some(value) => value,
            None => fallback,
        }
    }
    const fn face(desc: &DepthStencilDesc, face: &StencilFace, fallback: u8) -> WMTStencilInfo {
        WMTStencilInfo {
            enabled: 1,
            depth_stencil_pass_op: or(stencil_op(face.pass_op), fallback),
            stencil_fail_op: or(stencil_op(face.fail_op), fallback),
            depth_fail_op: or(stencil_op(face.depth_fail_op), fallback),
            stencil_compare_function: or(compare_func(face.func), WMT_COMPARE_FUNCTION_NEVER),
            write_mask: desc.write_mask,
            read_mask: desc.read_mask,
        }
    }
    let mut info = WMTDepthStencilInfo {
        depth_compare_function: WMT_COMPARE_FUNCTION_ALWAYS,
        depth_write_enabled: 0,
        front_stencil: OFF,
        back_stencil: OFF,
    };
    if desc.depth_enable {
        info.depth_compare_function = or(compare_func(desc.depth_func), WMT_COMPARE_FUNCTION_NEVER);
        info.depth_write_enabled = if desc.depth_write { 1 } else { 0 };
    }
    if desc.stencil_enable {
        info.front_stencil = face(desc, &desc.front, stencil_op_fallback);
        info.back_stencil = face(desc, &desc.back, stencil_op_fallback);
    }
    info
}

#[cfg(test)]
mod tests;
