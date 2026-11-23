//! Unit tests for D3D9 to Metal blend-state translation.

use mtld3d_types::{
    D3DBLEND_BOTHINVSRCALPHA, D3DBLEND_DESTALPHA, D3DBLEND_INVDESTALPHA, D3DBLEND_INVSRCALPHA,
    D3DBLEND_ONE, D3DBLEND_SRCALPHA, D3DBLEND_ZERO, D3DBLENDOP_MAX, D3DBLENDOP_REVSUBTRACT,
    D3DRS_ALPHABLENDENABLE, D3DRS_BLENDOP, D3DRS_BLENDOPALPHA, D3DRS_DESTBLEND,
    D3DRS_DESTBLENDALPHA, D3DRS_SEPARATEALPHABLENDENABLE, D3DRS_SRCBLEND, D3DRS_SRCBLENDALPHA,
    RENDER_STATE_COUNT,
};

use super::*;

/// The render states at their defaults with `changes` (state, value) applied.
fn states(changes: &[(u32, u32)]) -> [u32; RENDER_STATE_COUNT] {
    let mut rs = mtld3d_types::render_state_defaults();
    for &(state, value) in changes {
        rs[state as usize] = value;
    }
    rs
}

#[test]
fn core_blend_follows_effective_blend() {
    use BlendFactor::{
        DestinationAlpha, One, OneMinusDestinationAlpha, OneMinusSourceAlpha, SourceAlpha, Zero,
    };
    let on = (D3DRS_ALPHABLENDENABLE, 1);
    let alpha = [
        (D3DRS_SRCBLEND, D3DBLEND_SRCALPHA),
        (D3DRS_DESTBLEND, D3DBLEND_INVSRCALPHA),
    ];
    // Off, with factors left behind: none.
    assert_eq!(core_blend(&states(&alpha), true), None);
    // On: the colour equation is the alpha one too.
    let blend = core_blend(&states(&[on, alpha[0], alpha[1]]), true).expect("blending");
    assert_eq!(
        (blend.src, blend.dst, blend.op),
        (SourceAlpha, OneMinusSourceAlpha, BlendOperation::Add)
    );
    assert_eq!(
        (blend.src_alpha, blend.dst_alpha, blend.op_alpha),
        (SourceAlpha, OneMinusSourceAlpha, BlendOperation::Add)
    );
    // Separate alpha: its own equation, and only with the state on.
    let separate = [
        on,
        alpha[0],
        alpha[1],
        (D3DRS_SRCBLENDALPHA, D3DBLEND_ONE),
        (D3DRS_DESTBLENDALPHA, D3DBLEND_ZERO),
        (D3DRS_BLENDOPALPHA, D3DBLENDOP_REVSUBTRACT),
    ];
    assert_eq!(core_blend(&states(&separate), true), Some(blend));
    let mut with = separate.to_vec();
    with.push((D3DRS_SEPARATEALPHABLENDENABLE, 1));
    let blend = core_blend(&states(&with), true).expect("blending");
    assert_eq!(
        (blend.src, blend.src_alpha, blend.dst_alpha, blend.op_alpha),
        (SourceAlpha, One, Zero, BlendOperation::ReverseSubtract)
    );
    // A BOTH* source sets the destination; MIN and MAX read no factor.
    let both = [
        on,
        (D3DRS_SRCBLEND, D3DBLEND_BOTHINVSRCALPHA),
        (D3DRS_DESTBLEND, D3DBLEND_ONE),
    ];
    let blend = core_blend(&states(&both), true).expect("blending");
    assert_eq!((blend.src, blend.dst), (OneMinusSourceAlpha, SourceAlpha));
    let max = [on, alpha[0], alpha[1], (D3DRS_BLENDOP, D3DBLENDOP_MAX)];
    let blend = core_blend(&states(&max), true).expect("blending");
    assert_eq!(
        (blend.src, blend.dst, blend.op),
        (One, One, BlendOperation::Max)
    );
    // Destination alpha, in the colour and the alpha equation: read on a target that stores
    // alpha, 1 on one that does not.
    let dest = [
        on,
        (D3DRS_SRCBLEND, D3DBLEND_DESTALPHA),
        (D3DRS_DESTBLEND, D3DBLEND_INVDESTALPHA),
    ];
    let blend = core_blend(&states(&dest), true).expect("blending");
    assert_eq!(
        (blend.src, blend.dst, blend.src_alpha, blend.dst_alpha),
        (
            DestinationAlpha,
            OneMinusDestinationAlpha,
            DestinationAlpha,
            OneMinusDestinationAlpha
        )
    );
    let blend = core_blend(&states(&dest), false).expect("blending");
    assert_eq!(
        (blend.src, blend.dst, blend.src_alpha, blend.dst_alpha),
        (One, Zero, One, Zero)
    );
    // A value outside its enum reads as D3D9's default.
    let wild = [
        on,
        (D3DRS_SRCBLEND, 0x1234_5678),
        (D3DRS_DESTBLEND, 99),
        (D3DRS_BLENDOP, 77),
    ];
    let blend = core_blend(&states(&wild), true).expect("blending");
    assert_eq!(
        (blend.src, blend.dst, blend.op),
        (One, Zero, BlendOperation::Add)
    );
}

#[test]
fn reads_blend_color_cases() {
    let on = (D3DRS_ALPHABLENDENABLE, 1);
    let reads = |changes: &[(u32, u32)]| {
        core_blend(&states(changes), true)
            .expect("blending")
            .reads_blend_color()
    };
    assert!(!reads(&[on]));
    assert!(reads(&[
        on,
        (D3DRS_SRCBLEND, mtld3d_types::D3DBLEND_BLENDFACTOR)
    ]));
    assert!(reads(&[
        on,
        (D3DRS_DESTBLEND, mtld3d_types::D3DBLEND_INVBLENDFACTOR)
    ]));
    // In the alpha equation alone: only with separate alpha on.
    let alpha = (D3DRS_SRCBLENDALPHA, mtld3d_types::D3DBLEND_BLENDFACTOR);
    assert!(!reads(&[on, alpha]));
    assert!(reads(&[on, alpha, (D3DRS_SEPARATEALPHABLENDENABLE, 1)]));
}
