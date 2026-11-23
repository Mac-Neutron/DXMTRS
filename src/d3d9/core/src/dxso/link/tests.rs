//! Unit tests for semantic linkage between SM3 vertex and pixel shaders.

use airconv_ir::dxso::link::Semantic;

use super::{LinkInputs, SemanticSet};
use crate::dxso::{ir::DeclUsage, parse};

const VS3_HEADER: u32 = 0xFFFE_0300;
const PS3_HEADER: u32 = 0xFFFF_0300;
const END_TOKEN: u32 = 0x0000_FFFF;
const DCL: u32 = 0x0200_001F;
/// `D3DSPR_OUTPUT` (type 6) destination register token without mask or index.
const OUTPUT: u32 = 0xE000_0000;
/// `D3DSPR_INPUT` (type 1) destination register token without mask or index.
const INPUT: u32 = 0x9000_0000;

const POSITION: u32 = 0;
const NORMAL: u32 = 3;
const PSIZE: u32 = 4;
const TEXCOORD: u32 = 5;
const TANGENT: u32 = 6;
const COLOR: u32 = 10;
const FOG: u32 = 11;

/// `dcl_<usage><index> <kind>N.<mask>` as its three tokens.
const fn dcl(usage: u32, index: u32, kind: u32, reg: u32, mask: u32) -> [u32; 3] {
    [
        DCL,
        0x8000_0000 | (index << 16) | usage,
        kind | (mask << 16) | reg,
    ]
}

fn program(header: u32, dcls: &[[u32; 3]]) -> crate::dxso::DxsoProgram {
    let mut tokens = vec![header];
    for d in dcls {
        tokens.extend_from_slice(d);
    }
    tokens.push(END_TOKEN);
    parse(&tokens).expect("dcl-only program parses")
}

#[test]
fn members_are_named_after_the_semantic() {
    let cases = [
        (DeclUsage::Position, 0, "position", false),
        (DeclUsage::Position, 1, "position1", false),
        (DeclUsage::Position, 2, "position2", true),
        (DeclUsage::Texcoord, 15, "texcoord15", false),
        (DeclUsage::Color, 1, "color1", false),
        (DeclUsage::Color, 2, "color2", true),
        (DeclUsage::Fog, 0, "fog", false),
        (DeclUsage::Fog, 1, "fog1", true),
        (DeclUsage::PSize, 0, "psize0", false),
        (DeclUsage::Normal, 0, "normal0", true),
        (DeclUsage::BlendIndices, 3, "blendindices3", true),
        (DeclUsage::PositionT, 0, "positiont0", true),
        (DeclUsage::Sample, 15, "sample15", true),
    ];
    for (usage, index, member, extra) in cases {
        let semantic = Semantic::new(usage, index);
        assert_eq!(semantic.member(), member);
        assert_eq!(semantic.is_extra(), extra, "{member}");
        assert_eq!(Semantic::from_code(semantic.code()), semantic, "{member}");
    }
}

#[test]
fn a_vertex_shader_outputs_only_its_declared_extras() {
    let vs = program(
        VS3_HEADER,
        &[
            dcl(POSITION, 0, INPUT, 0, 0xF),
            dcl(POSITION, 0, OUTPUT, 0, 0xF),
            dcl(NORMAL, 0, OUTPUT, 1, 0x7),
            dcl(COLOR, 2, OUTPUT, 2, 0xF),
            dcl(TEXCOORD, 3, OUTPUT, 3, 0x3),
            dcl(PSIZE, 0, OUTPUT, 4, 0x1),
            dcl(FOG, 0, OUTPUT, 5, 0x1),
        ],
    );
    let outputs = SemanticSet::vs_outputs(&vs);
    let listed: Vec<_> = outputs.iter().map(Semantic::member).collect();
    assert_eq!(
        listed,
        ["normal0", "color2"],
        "the vertex input is no output"
    );

    let sm2 = parse(&[0xFFFE_0200, END_TOKEN]).expect("vs_2_0");
    assert!(SemanticSet::vs_outputs(&sm2).is_empty());
}

#[test]
fn input_order_fixes_the_mask_bits() {
    let ps = program(
        PS3_HEADER,
        &[
            dcl(TEXCOORD, 0, INPUT, 0, 0xF),
            dcl(TANGENT, 0, INPUT, 1, 0xF),
            dcl(NORMAL, 0, INPUT, 2, 0xF),
            dcl(NORMAL, 0, INPUT, 3, 0xF),
            dcl(COLOR, 2, INPUT, 4, 0xF),
        ],
    );
    let inputs = LinkInputs::ps_inputs(&ps).0;
    let order: Vec<_> = inputs.iter().map(Semantic::member).collect();
    assert_eq!(order, ["tangent0", "normal0", "color2"]);

    let vs = program(
        VS3_HEADER,
        &[
            dcl(COLOR, 2, OUTPUT, 1, 0xF),
            dcl(TANGENT, 0, OUTPUT, 2, 0xF),
        ],
    );
    assert_eq!(inputs.mask_against(&SemanticSet::vs_outputs(&vs)), 0b101);
    assert_eq!(inputs.mask_against(&SemanticSet::default()), 0);
}
