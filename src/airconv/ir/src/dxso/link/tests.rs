//! Unit tests for the data half of semantic linkage.

use super::{LinkInputs, MAX_LINKED_INPUTS, Semantic, SemanticSet, decl_passthrough_code};
use crate::dxso::{
    ir::{DeclUsage, DxsoProgram, DxsoWarning},
    parser::parse,
};

const PS3_HEADER: u32 = 0xFFFF_0300;
const END_TOKEN: u32 = 0x0000_FFFF;
const DCL: u32 = 0x0200_001F;
/// `D3DSPR_INPUT` (type 1) destination register token without mask or index.
const INPUT: u32 = 0x9000_0000;
const NORMAL: u32 = 3;

/// `dcl_normal<index> v<reg>.xyzw` as its three tokens.
const fn dcl_normal(index: u32, reg: u32) -> [u32; 3] {
    [
        DCL,
        0x8000_0000 | (index << 16) | NORMAL,
        INPUT | (0xF << 16) | reg,
    ]
}

fn ps_declaring_normals(count: u32) -> DxsoProgram {
    let mut tokens = vec![PS3_HEADER];
    for i in 0..count {
        tokens.extend_from_slice(&dcl_normal(i, i));
    }
    tokens.push(END_TOKEN);
    parse(&tokens).expect("dcl-only program parses")
}

#[test]
fn inputs_past_the_mask_width_are_not_linked() {
    let (inputs, warning) = LinkInputs::ps_inputs(&ps_declaring_normals(10));
    assert_eq!(inputs.iter().count(), MAX_LINKED_INPUTS);
    assert!(
        inputs
            .position(Semantic::new(DeclUsage::Normal, 8))
            .is_none()
    );
    assert_eq!(warning, Some(DxsoWarning::LinkedInputsOverflow));
    // Exactly the width is no overflow.
    let (inputs, warning) = LinkInputs::ps_inputs(&ps_declaring_normals(8));
    assert_eq!(inputs.iter().count(), MAX_LINKED_INPUTS);
    assert_eq!(warning, None);
}

#[test]
fn a_write_beyond_the_four_targets_is_ignored_and_reported_once() {
    // ps_3_0: mov oC5, c0 twice, and mov oC0, c0. `oC` is register type 8: bit 11 of the token.
    const PS3_MOV: u32 = 0x0200_0001;
    let dst_oc = |index: u32| 0x800F_0800 | index;
    let src_c0 = 0xA0E4_0000_u32;
    let tokens = [
        PS3_HEADER,
        PS3_MOV,
        dst_oc(5),
        src_c0,
        PS3_MOV,
        dst_oc(5),
        src_c0,
        PS3_MOV,
        dst_oc(0),
        src_c0,
        END_TOKEN,
    ];
    let ps = parse(&tokens).expect("ps parses");
    let (mask, warnings) = ps.color_out_mask();
    assert_eq!(mask, 0b1);
    assert_eq!(warnings, [DxsoWarning::ColorOutBeyondTargets(5)]);
}

#[test]
fn a_pretransformed_layout_passes_through_what_the_ff_stage_does_not_write() {
    // `D3DDECLUSAGE_*` values, written as the headers have them.
    let [position, normal, psize, texcoord, color, fog, depth, sample] =
        [0u8, 3, 4, 5, 10, 11, 12, 13];
    // The fixed-function stage writes these itself.
    for (usage, index) in [
        (position, 0),
        (9, 0),
        (psize, 0),
        (texcoord, 0),
        (texcoord, 7),
        (color, 0),
        (color, 1),
    ] {
        assert_eq!(
            decl_passthrough_code(usage, index),
            None,
            "usage {usage} index {index}"
        );
    }
    // Everything else a `ps_3_0` can declare reaches it by semantic.
    for (usage, index) in [
        (position, 1),
        (fog, 0),
        (normal, 0),
        (1, 0),
        (6, 3),
        (color, 2),
        (depth, 0),
        (sample, 15),
        (9, 1),
    ] {
        assert!(
            decl_passthrough_code(usage, index).is_some(),
            "usage {usage} index {index}"
        );
    }
    // A usage outside D3DDECLUSAGE never does.
    assert_eq!(decl_passthrough_code(sample + 1, 0), None);
}

#[test]
fn semantic_sets_hold_every_code() {
    let mut set = SemanticSet::default();
    assert!(set.is_empty());
    for code in [0u8, 63, 64, 130, 223] {
        set.insert(Semantic::from_code(code));
    }
    let listed: Vec<_> = set.iter().map(Semantic::code).collect();
    assert_eq!(listed, [0, 63, 64, 130, 223]);
    assert!(set.contains(Semantic::new(DeclUsage::Sample, 15)));
}
