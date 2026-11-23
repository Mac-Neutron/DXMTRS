//! Unit tests for DXSO sampler and parser metadata helpers.

use super::declared_ps_samplers;
use crate::dxso::{DxsoWarning, VsSamplerKinds, ir::TextureType, parse};

const VS_HEADER: u32 = 0xFFFE_0200;
const PS3_HEADER: u32 = 0xFFFF_0300;
const END_TOKEN: u32 = 0x0000_FFFF;
const SWIZ_IDENTITY: u8 = 0xE4;

const TYPE_TEMP: u32 = 0;
const TYPE_INPUT: u32 = 1;
const TYPE_CONST: u32 = 2;
const TYPE_RASTOUT: u32 = 4;
const TYPE_COLOROUT: u32 = 8;

/// `dcl_<usage>_<index>` token: bit 31 set + usage bits 0..4 + index bits 16..19.
fn dcl_usage_token(usage: u8, index: u8) -> u32 {
    0x8000_0000 | (u32::from(usage) & 0x1F) | ((u32::from(index) & 0xF) << 16)
}

const DCL_TEXCOORD: u8 = 5;

const OP_MOV: u16 = 1;
const OP_DCL: u16 = 31;
const OP_DEF: u16 = 81;
const OP_TEX: u16 = 66;
const OP_TEXLDL: u16 = 95;

fn reg_bits(reg_type: u32, index: u16) -> u32 {
    let low = reg_type & 0x7;
    let high = (reg_type >> 3) & 0x3;
    // Bit 31 marks every dst/src parameter token in real D3D9 bytecode (all
    // shader models). SM1 operand counting keys off it, so the helpers must
    // set it to model the stream a shader compiler actually emits.
    0x8000_0000 | (low << 28) | (high << 11) | u32::from(index)
}

fn opcode_token(opcode: u16, token_count: u32) -> u32 {
    u32::from(opcode) | (token_count << 24)
}

fn dst_token(reg_type: u32, index: u16, write_mask: u8, saturate: bool) -> u32 {
    let mut t = reg_bits(reg_type, index);
    t |= (u32::from(write_mask) & 0xF) << 16;
    if saturate {
        t |= 1 << 20;
    }
    t
}

fn src_token(reg_type: u32, index: u16, swizzle: u8, modifier: u8) -> u32 {
    let mut t = reg_bits(reg_type, index);
    t |= u32::from(swizzle) << 16;
    t |= (u32::from(modifier) & 0xF) << 24;
    t
}

fn red_constant_ps() -> Vec<u32> {
    // def c0, 1, 0, 0, 1; mov oC0, c0;
    vec![
        0xFFFF_0200,
        opcode_token(OP_DEF, 5),
        dst_token(TYPE_CONST, 0, 0xF, false),
        f32::to_bits(1.0),
        f32::to_bits(0.0),
        f32::to_bits(0.0),
        f32::to_bits(1.0),
        opcode_token(OP_MOV, 2),
        dst_token(TYPE_COLOROUT, 0, 0xF, false),
        src_token(TYPE_CONST, 0, SWIZ_IDENTITY, 0),
        END_TOKEN,
    ]
}

/// A two-target `ps_3_0` shader writing `oC0` and `oC1` from `c0` and `c1`.
///
/// The two-target shape a deferred renderer's G-buffer pass uses.
fn two_target_ps() -> Vec<u32> {
    vec![
        PS3_HEADER,
        opcode_token(OP_DEF, 5),
        dst_token(TYPE_CONST, 0, 0xF, false),
        f32::to_bits(0.0),
        f32::to_bits(1.0),
        f32::to_bits(0.0),
        f32::to_bits(0.0),
        opcode_token(OP_DEF, 5),
        dst_token(TYPE_CONST, 1, 0xF, false),
        f32::to_bits(0.0),
        f32::to_bits(0.0),
        f32::to_bits(1.0),
        f32::to_bits(0.0),
        opcode_token(OP_MOV, 2),
        dst_token(TYPE_COLOROUT, 0, 0xF, false),
        src_token(TYPE_CONST, 0, SWIZ_IDENTITY, 0),
        opcode_token(OP_MOV, 2),
        dst_token(TYPE_COLOROUT, 1, 0xF, false),
        src_token(TYPE_CONST, 1, SWIZ_IDENTITY, 0),
        END_TOKEN,
    ]
}

/// A `ps_3_0` wrapping one sampling instruction against `s0` and `t0`.
fn ps3_sampling_program(op: u16, extra_srcs: &[u32]) -> Vec<u32> {
    let mut bc = vec![
        PS3_HEADER,
        opcode_token(OP_DCL, 2),
        0x9000_0000,
        dst_token(10 /* TYPE_SAMPLER */, 0, 0xF, false),
        opcode_token(OP_DCL, 2),
        dcl_usage_token(DCL_TEXCOORD, 0),
        dst_token(TYPE_INPUT, 0, 0xF, false),
        opcode_token(
            op,
            u32::try_from(3 + extra_srcs.len()).expect("operand count fits u32"),
        ),
        dst_token(TYPE_TEMP, 0, 0xF, false),
        src_token(TYPE_INPUT, 0, SWIZ_IDENTITY, 0),
        src_token(10 /* TYPE_SAMPLER */, 0, SWIZ_IDENTITY, 0),
    ];
    bc.extend_from_slice(extra_srcs);
    bc.extend_from_slice(&[
        opcode_token(OP_MOV, 2),
        dst_token(TYPE_COLOROUT, 0, 0xF, false),
        src_token(TYPE_TEMP, 0, SWIZ_IDENTITY, 0),
        END_TOKEN,
    ]);
    bc
}

#[test]
fn uses_relative_const_addressing_is_false_for_static_reads() {
    // vs_2_0 { dcl_position v0; mov r0, c[5]; mov oPos, r0; }
    let bc = vec![
        VS_HEADER,
        opcode_token(OP_DCL, 2),
        0x0000_0000,
        dst_token(TYPE_INPUT, 0, 0xF, false),
        opcode_token(OP_MOV, 2),
        dst_token(TYPE_TEMP, 0, 0xF, false),
        src_token(TYPE_CONST, 5, SWIZ_IDENTITY, 0),
        opcode_token(OP_MOV, 2),
        dst_token(TYPE_RASTOUT, 0, 0xF, false),
        src_token(TYPE_TEMP, 0, SWIZ_IDENTITY, 0),
        END_TOKEN,
    ];
    let vs = parse(&bc).expect("VS parse");
    assert!(!vs.uses_relative_const_addressing());
    // Plain static read still reports the right max so the fast path
    // keeps the short CB upload.
    assert_eq!(vs.max_const_reg(), Some(5));
}

#[test]
fn constant_register_limits_reject_out_of_range_files() {
    // Addressing a constant register past the model's file must be caught
    // so CreateShader returns INVALIDCALL. The bytecode is hand-assembled
    // to use the out-of-range indices the native assembler refuses.

    // vs_1_1 { def c255; add r0, v0, c255; mov oPos, r0 } - c255 is the last
    // in-range vertex float constant.
    let vs_float_in_range = [
        0xFFFE_0101_u32,
        0x0000_001F,
        0x8000_0000,
        0x900F_0000,
        0x0000_0051,
        0xA00F_00FF,
        0x3F80_0000,
        0x3F80_0000,
        0x3F80_0000,
        0x3F80_0000,
        0x0000_0002,
        0x800F_0000,
        0x90E4_0000,
        0xA0E4_00FF,
        0x0000_0001,
        0xC00F_0000,
        0x80E4_0000,
        0x0000_FFFF,
    ];
    assert!(
        !parse(&vs_float_in_range)
            .expect("vs_float_in_range parse")
            .violates_constant_register_limits(),
        "c255 is the last in-range vertex float constant"
    );

    // Same shader at c256 - one past the 256-entry vertex float file.
    let vs_float_over = [
        0xFFFE_0101_u32,
        0x0000_001F,
        0x8000_0000,
        0x900F_0000,
        0x0000_0051,
        0xA00F_0100,
        0x3F80_0000,
        0x3F80_0000,
        0x3F80_0000,
        0x3F80_0000,
        0x0000_0002,
        0x800F_0000,
        0x90E4_0000,
        0xA0E4_0100,
        0x0000_0001,
        0xC00F_0000,
        0x80E4_0000,
        0x0000_FFFF,
    ];
    assert!(
        parse(&vs_float_over)
            .expect("vs_float_over parse")
            .violates_constant_register_limits(),
        "c256 overflows the vertex float file"
    );

    // vs_3_0 { defi i16; rep i16; add r0,r0,v0; endrep; mov o0,r0 } - i16 is one
    // past the 16-entry integer file.
    let vs_int_over = [
        0xFFFE_0300_u32,
        0x0200_001F,
        0x8000_0000,
        0x900F_0000,
        0x0200_001F,
        0x8000_0000,
        0xE00F_0000,
        0x0500_0030,
        0xF00F_0010,
        0x0000_0001,
        0x0000_0001,
        0x0000_0001,
        0x0000_0001,
        0x0100_0026,
        0xF0E4_0010,
        0x0300_0002,
        0x800F_0000,
        0x80E4_0000,
        0x90E4_0000,
        0x0000_0027,
        0x0200_0001,
        0xE00F_0000,
        0x80E4_0000,
        0x0000_FFFF,
    ];
    assert!(
        parse(&vs_int_over)
            .expect("vs_int_over parse")
            .violates_constant_register_limits(),
        "i16 overflows the integer constant file"
    );
}

#[test]
fn color_out_mask_reports_written_targets() {
    let ps = parse(&two_target_ps()).expect("ps parse");
    assert_eq!(ps.color_out_mask(), (0b11, vec![]));
    let single = parse(&red_constant_ps()).expect("ps parse");
    assert_eq!(single.color_out_mask(), (0b1, vec![]));
    // A write beyond oC3 is outside the D3D9 limit: it is not in the mask, and is returned as a
    // warning.
    let mut beyond = two_target_ps();
    let oc1_dst = dst_token(TYPE_COLOROUT, 1, 0xF, false);
    let slot = beyond
        .iter()
        .position(|&t| t == oc1_dst)
        .expect("oC1 dst present");
    beyond[slot] = dst_token(TYPE_COLOROUT, 4, 0xF, false);
    assert_eq!(
        parse(&beyond).expect("ps parse").color_out_mask(),
        (0b1, vec![DxsoWarning::ColorOutBeyondTargets(4)])
    );
}

// An unbound-sampler pixel shader: `dcl_<dim> s0; dcl t0; texld r0, t0, s0;
// mov oC0, r0`. The dcl token's high nibble selects the sampler dimension.
const UNBOUND_PS_2D: [u32; 12] = [
    0xffff_0200,
    0x0200_001f,
    0x9000_0000,
    0xa00f_0800,
    0x0200_001f,
    0x8000_0000,
    0xb00f_0000,
    0x0300_0042,
    0x800f_0000,
    0xb0e4_0000,
    0xa0e4_0800,
    0x0000_ffff,
];

#[test]
fn declared_ps_samplers_reads_the_sampler_dimension() {
    // 2D (0x90000000), cube (0x98000000), volume (0xa0000000): the dcl token's
    // high nibble carries the sampler dimensionality.
    for (dcl, expected) in [
        (0x9000_0000u32, TextureType::Texture2D),
        (0x9800_0000, TextureType::TextureCube),
        (0xa000_0000, TextureType::Texture3D),
    ] {
        let mut bc = UNBOUND_PS_2D;
        bc[2] = dcl;
        let prog = parse(&bc).expect("unbound-sampler ps_2_0 should parse");
        let samplers = declared_ps_samplers(&prog);
        assert_eq!(samplers.len(), 1, "one declared sampler");
        assert_eq!(
            samplers.get(&0),
            Some(&expected),
            "sampler s0 dimensionality"
        );
    }
}

#[test]
fn explicit_lod_samplers_name_the_texldl_slots() {
    let texldl = parse(&ps3_sampling_program(OP_TEXLDL, &[])).expect("PS3 parse");
    assert_eq!(super::explicit_lod_samplers(&texldl), 1);
    let texld = parse(&ps3_sampling_program(OP_TEX, &[])).expect("PS3 parse");
    assert_eq!(super::explicit_lod_samplers(&texld), 0);
}

#[test]
fn vertex_sampler_kinds_reads_one_slot_at_a_time() {
    let mut kinds = VsSamplerKinds::default();
    kinds.set_slot(1, true, false);
    kinds.set_slot(3, false, true);
    assert_eq!(
        kinds.kind(0),
        TextureType::Texture2D,
        "untouched slot is 2D"
    );
    assert_eq!(kinds.kind(1), TextureType::Texture3D, "slot 1 volume");
    assert_eq!(
        kinds.kind(2),
        TextureType::Texture2D,
        "untouched slot is 2D"
    );
    assert_eq!(kinds.kind(3), TextureType::TextureCube, "slot 3 cube");
    assert_eq!(
        kinds.kind(4),
        TextureType::Texture2D,
        "D3D9 defines four vertex fetch slots; past them reads 2D"
    );
    kinds.set_slot(1, false, false);
    assert_eq!(
        kinds.kind(1),
        TextureType::Texture2D,
        "unbinding a slot returns it to 2D"
    );
    assert_eq!(
        kinds,
        VsSamplerKinds {
            cube_mask: 0b1000,
            ..VsSamplerKinds::default()
        }
    );
}
