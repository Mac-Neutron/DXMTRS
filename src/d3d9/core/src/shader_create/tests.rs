use mtld3d_shared::shader_create::{DeclUsage, ShaderStage, ShaderUsage};

use super::{CoreShaderMeta, ShaderCreateError, core_meta, parse_shader};
use crate::ids::ProgramId;

#[test]
fn malformed_and_wrong_stage_are_rejected() {
    assert!(matches!(
        parse_shader(&ShaderStage::Vertex, &[]),
        Err(ShaderCreateError::Parse(_))
    ));
    assert!(matches!(
        parse_shader(&ShaderStage::Vertex, &[0xffff_0300, 0x0000_ffff]),
        Err(ShaderCreateError::WrongStage)
    ));
    assert!(matches!(
        parse_shader(&ShaderStage::Pixel, &[0xfffe_0300, 0x0000_ffff]),
        Err(ShaderCreateError::WrongStage)
    ));
}

#[test]
fn constant_register_boundaries_are_preserved() {
    for (stage, header, last) in [
        (ShaderStage::Vertex, 0xfffe_0200, 255),
        (ShaderStage::Pixel, 0xffff_0200, 31),
        (ShaderStage::Pixel, 0xffff_0300, 223),
    ] {
        let mut tokens = [
            header,
            0x0200_0001,
            0x800f_0000,
            0xa0e4_0000 | last,
            0x0000_ffff,
        ];
        let shader = parse_shader(&stage, &tokens).expect("last constant is valid");
        assert_eq!(shader.max_const_used, last + 1);
        tokens[3] += 1;
        assert!(matches!(
            parse_shader(&stage, &tokens),
            Err(ShaderCreateError::ConstantRegisterLimit)
        ));
    }
}

#[test]
fn pixel_position_zero_is_rejected_but_other_indices_are_valid() {
    let mut tokens = [0xffff_0300, 0x0200_001f, 0, 0x900f_0000, 0x0000_ffff];
    assert!(matches!(
        parse_shader(&ShaderStage::Pixel, &tokens),
        Err(ShaderCreateError::InvalidPixelInput)
    ));
    tokens[2] = 1 << 16;
    assert!(parse_shader(&ShaderStage::Pixel, &tokens).is_ok());
}

#[test]
fn vertex_metadata_keeps_semantics_in_declaration_order() {
    let tokens = [
        0xfffe_0300,
        0x0200_001f,
        0x0002_0005,
        0x900f_0003,
        0x0200_001f,
        0x0000_0000,
        0x900f_0001,
        0x0000_ffff,
    ];
    let shader = parse_shader(&ShaderStage::Vertex, &tokens).expect("valid vertex shader");
    assert_eq!(shader.id, ProgramId::from_tokens(&tokens));
    assert_eq!(shader.program.bytecode().as_ref(), tokens);
    assert_eq!(shader.input_semantics.len(), 2);
    assert_eq!(shader.input_semantics[0].usage, DeclUsage::Texcoord);
    assert_eq!(shader.input_semantics[0].usage_index, 2);
    assert_eq!(shader.input_semantics[0].register_index, 3);
    assert_eq!(shader.input_semantics[1].usage, DeclUsage::Position);
    assert!(!shader.usage.contains(ShaderUsage::AUTOMATIC_FOG));
}

#[test]
fn pixel_metadata_preserves_fog_and_output_mask() {
    let mut tokens = [
        0xffff_0200,
        0x0200_0001,
        0x800f_0802,
        0xa0e4_0000,
        0x0000_ffff,
    ];
    let shader = parse_shader(&ShaderStage::Pixel, &tokens).expect("valid pixel shader");
    assert!(shader.usage.contains(ShaderUsage::AUTOMATIC_FOG));
    assert_eq!(shader.color_out_mask, 4);
    assert!(shader.input_semantics.is_empty());
    tokens[0] = 0xffff_0300;
    assert!(
        !parse_shader(&ShaderStage::Pixel, &tokens)
            .expect("valid SM3 shader")
            .usage
            .contains(ShaderUsage::AUTOMATIC_FOG)
    );
}

#[test]
fn vertex_constant_usage_flags_are_independent() {
    let tokens = [
        0xfffe_0300,
        0x0300_0001,
        0x800f_0000,
        0xa0e4_2005,
        0xb000_0000,
        0x0200_0001,
        0x800f_0000,
        0xf0e4_0000,
        0x0200_0001,
        0x800f_0000,
        0xe0e4_0803,
        0x0000_ffff,
    ];
    let shader = parse_shader(&ShaderStage::Vertex, &tokens).expect("valid constant references");
    assert!(shader.usage.contains(ShaderUsage::RELATIVE_CONST));
    assert!(shader.usage.contains(ShaderUsage::INT_CONST));
    assert!(shader.usage.contains(ShaderUsage::BOOL_CONST));
    assert!(!shader.usage.contains(ShaderUsage::BUMP_ENV));
}

#[test]
fn pixel_relative_constant_read_is_reported() {
    // ps_3_0: defi i0, 4, 0, 1, 0; mov r0, c1; loop aL, i0;
    // add r0, r0, c[aL + 2]; endloop; mov oC0, r0
    let tokens = [
        0xffff_0300,
        0x0500_0030,
        0xf00f_0000,
        4,
        0,
        1,
        0,
        0x0200_0001,
        0x800f_0000,
        0xa0e4_0001,
        0x0200_001b,
        0xf0e4_0800,
        0xf0e4_0000,
        0x0400_0002,
        0x800f_0000,
        0x80e4_0000,
        0xa0e4_2002,
        0xf000_0800,
        0x0000_001d,
        0x0200_0001,
        0x800f_0800,
        0x80e4_0000,
        0x0000_ffff,
    ];
    let shader = parse_shader(&ShaderStage::Pixel, &tokens).expect("valid relative read");
    assert!(shader.usage.contains(ShaderUsage::RELATIVE_CONST));
    assert_eq!(shader.max_const_used, 3);
}

#[test]
fn pixel_integer_and_boolean_files_start_at_ps_2_x() {
    // ps_2_x { defb b0, true; if b0; mov oC0, c0; endif }
    let bool_branch = [
        0xffff_0201,
        0x0200_002f,
        0xe00f_0800,
        1,
        0x0100_0028,
        0xe0e4_0800,
        0x0200_0001,
        0x800f_0800,
        0xa0e4_0000,
        0x0000_002b,
        0x0000_ffff,
    ];
    // ps_2_x { defi i0, 2, 0, 0, 0; mov r0, c0; rep i0; add r0, r0, c1; endrep;
    // mov oC0, r0 }
    let int_loop = [
        0xffff_0201,
        0x0500_0030,
        0xf00f_0000,
        2,
        0,
        0,
        0,
        0x0200_0001,
        0x800f_0000,
        0xa0e4_0000,
        0x0100_0026,
        0xf0e4_0000,
        0x0300_0002,
        0x800f_0000,
        0x80e4_0000,
        0xa0e4_0001,
        0x0000_0027,
        0x0200_0001,
        0x800f_0800,
        0x80e4_0000,
        0x0000_ffff,
    ];
    for mut tokens in [bool_branch.to_vec(), int_loop.to_vec()] {
        assert!(parse_shader(&ShaderStage::Pixel, &tokens).is_ok());
        tokens[0] = 0xffff_0200;
        assert!(matches!(
            parse_shader(&ShaderStage::Pixel, &tokens),
            Err(ShaderCreateError::ConstantRegisterLimit)
        ));
    }
}

/// `ps_2_0 { dcl t0; dcl_2d s0; texld r0, t0, s0; mov oC0, r0 }`.
const PS_2_0_TEXLD: [u32; 15] = [
    0xffff_0200, // ps_2_0
    0x0200_001f,
    0x8000_0000,
    0xb00f_0000, // dcl t0
    0x0200_001f,
    0x9000_0000,
    0xa00f_0800, // dcl_2d s0
    0x0300_0042,
    0x800f_0000,
    0xb0e4_0000,
    0xa0e4_0800, // texld r0, t0, s0
    0x0200_0001,
    0x800f_0800,
    0x80e4_0000, // mov oC0, r0
    0x0000_ffff,
];

/// `ps_1_1 { tex t0; mov r0, t0 }`: SM1 declares no sampler, its `tex` samples stage 0.
const PS_1_1_TEX: [u32; 7] = [
    0xffff_0101, // ps_1_1
    0x0100_0042,
    0xb00f_0000, // tex t0
    0x0200_0001,
    0x800f_0000,
    0xb0e4_0000, // mov r0, t0
    0x0000_ffff,
];

/// `vs_3_0 { dcl_position v0; dcl_position o0; dcl_2d s0; mov o0, v0 }`.
const VS_3_0_SAMPLER: [u32; 14] = [
    0xfffe_0300, // vs_3_0
    0x0200_001f,
    0x8000_0000,
    0x900f_0000, // dcl_position v0
    0x0200_001f,
    0x8000_0000,
    0xe00f_0000, // dcl_position o0
    0x0200_001f,
    0x9000_0000,
    0xa00f_0800, // dcl_2d s0
    0x0200_0001,
    0xe00f_0000,
    0x90e4_0000, // mov o0, v0
    0x0000_ffff,
];

/// `ps_2_0 { mov oC0, c0 }`.
const PS_2_0_C0: [u32; 5] = [
    0xffff_0200, // ps_2_0
    0x0200_0001,
    0x800f_0800,
    0xa0e4_0000, // mov oC0, c0
    0x0000_ffff,
];

/// `ps_1_4 { bem r0.xy, r0, r1 }`: reads the bump-env rows, samples nothing.
const PS_1_4_BEM: [u32; 6] = [
    0xffff_0104, // ps_1_4
    0x0300_0059,
    0x8003_0000,
    0x80e4_0000,
    0x80e4_0001, // bem r0.xy, r0, r1
    0x0000_ffff,
];

/// `ps_3_0 { dcl_normal v0; mov oC0, v0 }`.
const PS_3_0_NORMAL: [u32; 8] = [
    0xffff_0300, // ps_3_0
    0x0200_001f,
    0x8000_0003,
    0x900f_0000, // dcl_normal v0
    0x0200_0001,
    0x800f_0800,
    0x90e4_0000, // mov oC0, v0
    0x0000_ffff,
];

/// `vs_3_0 { dcl_position v0; dcl_position o0; dcl_normal o2; mov o0, v0; mov o2, v0 }`.
const VS_3_0_NORMAL: [u32; 17] = [
    0xfffe_0300, // vs_3_0
    0x0200_001f,
    0x8000_0000,
    0x900f_0000, // dcl_position v0
    0x0200_001f,
    0x8000_0000,
    0xe00f_0000, // dcl_position o0
    0x0200_001f,
    0x8000_0003,
    0xe00f_0002, // dcl_normal o2
    0x0200_0001,
    0xe00f_0000,
    0x90e4_0000, // mov o0, v0
    0x0200_0001,
    0xe00f_0002,
    0x90e4_0000, // mov o2, v0
    0x0000_ffff,
];

/// What the core derives once from `tokens`, created as `stage`.
fn meta(stage: &ShaderStage, tokens: &[u32]) -> CoreShaderMeta {
    core_meta(
        &parse_shader(stage, tokens)
            .expect("a valid program")
            .program,
    )
}

/// The declared sampler slots: a `ps_2_0` `texld` of s0 declares s0 alone; a plain program none.
#[test]
fn core_meta_masks_the_declared_samplers() {
    let texld = meta(&ShaderStage::Pixel, &PS_2_0_TEXLD);
    assert_eq!(texld.sampler_mask, 1);
    assert_eq!(texld.explicit_lod_mask, 0);
    assert_eq!(meta(&ShaderStage::Pixel, &PS_2_0_C0).sampler_mask, 0);
}

#[test]
fn core_meta_flags_a_declared_sampler() {
    assert!(meta(&ShaderStage::Pixel, &PS_2_0_TEXLD).samples());
    assert!(meta(&ShaderStage::Pixel, &PS_1_1_TEX).samples());
    assert!(meta(&ShaderStage::Vertex, &VS_3_0_SAMPLER).samples());
    let plain = meta(&ShaderStage::Pixel, &PS_2_0_C0);
    assert!(!plain.samples());
    assert!(!plain.bump_env());
}

#[test]
fn core_meta_flags_bump_env() {
    let bem = meta(&ShaderStage::Pixel, &PS_1_4_BEM);
    assert!(bem.bump_env());
    assert!(!bem.samples());
}

#[test]
fn core_meta_links_a_ps_3_0_input() {
    let ps = meta(&ShaderStage::Pixel, &PS_3_0_NORMAL);
    let vs = meta(&ShaderStage::Vertex, &VS_3_0_NORMAL);
    assert_eq!(ps.link_inputs.mask_against(&vs.link_outputs), 1);
    let plain = meta(&ShaderStage::Pixel, &PS_2_0_C0);
    assert!(plain.link_inputs.is_empty());
}
