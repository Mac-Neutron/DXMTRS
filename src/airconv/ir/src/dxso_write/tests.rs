//! Unit tests for the DXSO writer.
//!
//! Hand-written streams in every operand form the parser keeps, one per shader model family, go
//! through `parse` and back: `parse ∘ write` keeps every field, and a canonical stream is written
//! back token for token. A sweep through `write_parts` covers every register kind, source
//! modifier, declaration usage, texture type and instruction opcode.

use super::*;
use crate::dxso::{
    ir::{CmpFunc, DeclUsage, RelativeAddr, TextureType},
    parse,
};

const END: u32 = 0x0000_FFFF;
const XYZW: u32 = 0xE4;
const XXXX: u32 = 0x00;
const YYYY: u32 = 0x55;
const ZZZZ: u32 = 0xAA;
const WWWW: u32 = 0xFF;
const WZYX: u32 = 0x1B;

// Register types (`D3DSPR_*`, as the parser maps them).
const R: u32 = 0;
const V: u32 = 1;
const C: u32 = 2;
const A: u32 = 3;
const RAST: u32 = 4;
const ATTR: u32 = 5;
const OUT: u32 = 6;
const I: u32 = 7;
const OC: u32 = 8;
const ODEPTH: u32 = 9;
const S: u32 = 10;
const T11: u32 = 11;
const B: u32 = 14;
const AL: u32 = 15;
const H: u32 = 16;
const MISC: u32 = 17;
const L: u32 = 18;
const P: u32 = 19;

// Opcodes (`D3DSIO_*`).
const NOP: u32 = 0;
const MOV: u32 = 1;
const ADD: u32 = 2;
const MUL: u32 = 5;
const RCP: u32 = 6;
const DP4: u32 = 9;
const M4X4: u32 = 20;
const CALL: u32 = 25;
const CALLNZ: u32 = 26;
const LOOP: u32 = 27;
const RET: u32 = 28;
const ENDLOOP: u32 = 29;
const LABEL: u32 = 30;
const DCL: u32 = 31;
const POW: u32 = 32;
const NRM: u32 = 36;
const SINCOS: u32 = 37;
const REP: u32 = 38;
const ENDREP: u32 = 39;
const IF: u32 = 40;
const IFC: u32 = 41;
const ELSE: u32 = 42;
const ENDIF: u32 = 43;
const BREAKC: u32 = 45;
const MOVA: u32 = 46;
const DEFB: u32 = 47;
const DEFI: u32 = 48;
const TEXKILL: u32 = 65;
const TEXLD: u32 = 66;
const CND: u32 = 80;
const DEF: u32 = 81;
const CMP: u32 = 88;
const BEM: u32 = 89;
const DP2ADD: u32 = 90;
const DSX: u32 = 91;
const DSY: u32 = 92;
const TEXLDD: u32 = 93;
const SETP: u32 = 94;
const TEXLDL: u32 = 95;
const BREAKP: u32 = 96;
const PHASE: u32 = 0xFFFD;

// Token bits.
const REL: u32 = 1 << 13;
const SAT: u32 = 1 << 20;
const PP: u32 = 2 << 20;
const CENTROID: u32 = 4 << 20;
const PRED: u32 = 1 << 28;
const COISSUE: u32 = 1 << 30;
const PROJECT: u32 = 1 << 16;
const BIAS: u32 = 2 << 16;

const fn vs(major: u32, minor: u32) -> u32 {
    0xFFFE_0000 | (major << 8) | minor
}

const fn ps(major: u32, minor: u32) -> u32 {
    0xFFFF_0000 | (major << 8) | minor
}

/// An instruction token with its operand-token count (0 for SM1).
const fn op(code: u32, len: u32) -> u32 {
    code | (len << 24)
}

const fn reg(ty: u32, index: u32) -> u32 {
    0x8000_0000 | ((ty & 7) << 28) | ((ty >> 3) << 11) | index
}

const fn d(ty: u32, index: u32, mask: u32) -> u32 {
    reg(ty, index) | (mask << 16)
}

const fn s(ty: u32, index: u32, swizzle: u32) -> u32 {
    reg(ty, index) | (swizzle << 16)
}

/// A source modifier's bits (`D3DSPSM_*`).
const fn m(modifier: u32) -> u32 {
    modifier << 24
}

/// A destination shift's bits, `-8..=7`.
const fn shift(n: i32) -> u32 {
    (n.cast_unsigned() & 0xF) << 24
}

/// A `dcl` usage token for a semantic.
const fn usage(usage: u32, index: u32) -> u32 {
    0x8000_0000 | usage | (index << 16)
}

/// A `dcl` usage token for a sampler of texture type `ty`.
const fn sampler(ty: u32) -> u32 {
    0x8000_0000 | (ty << 27)
}

const fn f(x: f32) -> u32 {
    x.to_bits()
}

/// `vs_1_1`: implicit relative reads, 17 operand tokens (SM1 has no count), `rcp` before `end`.
#[rustfmt::skip]
const VS11: &[u32] = &[
    vs(1, 1),
    op(DCL, 0), usage(0, 0), d(V, 0, 0xF),
    op(DCL, 0), usage(2, 0), d(V, 1, 0xF),
    op(DEF, 0), d(C, 95, 0xF), f(1.0), f(0.5), f(-2.0), f(0.0),
    op(MOV, 0), d(A, 0, 0x1), s(V, 1, XXXX),
    op(DP4, 0), d(RAST, 0, 0x1), s(V, 0, XYZW), s(C, 10, XYZW) | REL,
    op(MOV, 0), d(ATTR, 0, 0xF), s(C, 95, XYZW) | m(1),
    op(MOV, 0), d(OUT, 3, 0x3), s(V, 0, WZYX),
    op(ADD, 0), d(R, 2, 0xF),
    s(R, 0, XYZW), s(R, 0, XYZW), s(R, 0, XYZW), s(R, 0, XYZW), s(R, 0, XYZW), s(R, 0, XYZW),
    s(R, 0, XYZW), s(R, 0, XYZW), s(R, 0, XYZW), s(R, 0, XYZW), s(R, 0, XYZW), s(R, 0, XYZW),
    s(R, 0, XYZW), s(R, 0, XYZW), s(R, 0, XYZW), s(R, 0, XYZW),
    op(RCP, 0), d(R, 1, 0x8), s(V, 0, WWWW),
    END,
];

/// `ps_1_4`: coissue, shifts, the SM1 source modifiers, `texld` and `texkill` of `t#`, `bem`.
#[rustfmt::skip]
const PS14: &[u32] = &[
    ps(1, 4),
    op(DEF, 0), d(C, 0, 0xF), f(0.25), f(0.5), f(0.75), f(1.0),
    op(TEXLD, 0), d(R, 0, 0xF), s(A, 0, XYZW),
    op(TEXKILL, 0), d(A, 1, 0xF),
    op(MUL, 0), d(R, 0, 0x7) | shift(1), s(R, 0, XYZW) | m(2), s(A, 1, XYZW) | m(7),
    op(MOV, 0) | COISSUE, d(R, 0, 0x8) | shift(-1), s(C, 0, XYZW) | m(5),
    op(CND, 0), d(R, 0, 0xF) | SAT, s(R, 0, WWWW) | m(6), s(R, 1, XYZW) | m(3), s(C, 0, XYZW) | m(8),
    op(BEM, 0), d(R, 1, 0x3), s(R, 0, XYZW) | m(4), s(R, 2, XYZW) | m(9),
    op(MOV, 0), d(R, 0, 0xF), s(A, 2, XYZW) | m(10),
    END,
];

/// `vs_2_0`: `defi`/`defb`, explicit relative reads on `a0` and `aL`, static flow, subroutines.
#[rustfmt::skip]
const VS20: &[u32] = &[
    vs(2, 0),
    op(DCL, 2), usage(0, 0), d(V, 0, 0xF),
    op(DCL, 2), usage(5, 1), d(V, 1, 0x3),
    op(DEF, 5), d(C, 4, 0xF), f(2.0), f(-0.0), f(f32::MAX), f(1e-30),
    op(DEFI, 5), d(I, 0, 0xF), 3, 0, 1, (-1i32).cast_unsigned(),
    op(DEFB, 2), d(B, 0, 0xF), 1,
    op(DEFB, 2), d(B, 1, 0xF), 0,
    op(MOVA, 2), d(A, 0, 0x3), s(V, 1, XXXX),
    op(MOV, 3), d(R, 0, 0xF), s(C, 4, XYZW) | REL, s(A, 0, YYYY),
    op(LOOP, 2), s(AL, 0, XYZW), s(I, 0, XYZW),
    op(ADD, 4), d(R, 0, 0xF), s(R, 0, XYZW), s(C, 0, XYZW) | REL, s(AL, 0, XXXX),
    op(ENDLOOP, 0),
    op(IF, 1), s(B, 0, XXXX),
    op(CALL, 1), s(L, 0, XYZW),
    op(ELSE, 0),
    op(CALLNZ, 2), s(L, 1, XYZW), s(B, 1, XXXX),
    op(ENDIF, 0),
    op(REP, 1), s(I, 0, XYZW),
    op(SINCOS, 4), d(R, 1, 0x3), s(R, 0, XXXX), s(C, 5, XYZW), s(C, 6, XYZW),
    op(ENDREP, 0),
    op(MOV, 2), d(RAST, 0, 0xF), s(R, 0, XYZW),
    op(RET, 0),
    op(LABEL, 1), s(L, 0, XYZW),
    op(MUL, 3), d(R, 0, 0xF), s(R, 0, XYZW), s(C, 4, XYZW),
    op(RET, 0),
    op(LABEL, 1), s(L, 1, XYZW),
    op(RET, 0),
    END,
];

/// `ps_2_0`: `t#` and `v#` dcls, every sampler type, `texld`/`texldp`/`texldb`, `_pp`, `oDepth`.
#[rustfmt::skip]
const PS20: &[u32] = &[
    ps(2, 0),
    op(DCL, 2), usage(0, 0), d(A, 0, 0x3),
    op(DCL, 2), usage(0, 0), d(V, 0, 0xF),
    op(DCL, 2), sampler(2), d(S, 0, 0xF),
    op(DCL, 2), sampler(3), d(S, 1, 0xF),
    op(DCL, 2), sampler(4), d(S, 2, 0xF),
    op(DCL, 2), sampler(0), d(S, 3, 0xF),
    op(DEF, 5), d(C, 0, 0xF), f(0.0), f(1.0), f(-1.0), f(0.5),
    op(TEXLD, 3), d(R, 0, 0xF), s(A, 0, XYZW), s(S, 0, XYZW),
    op(TEXLD, 3) | PROJECT, d(R, 1, 0xF), s(A, 0, XYZW), s(S, 1, XYZW),
    op(TEXLD, 3) | BIAS, d(R, 2, 0xF), s(A, 0, XYZW), s(S, 2, XYZW),
    op(TEXKILL, 1), d(R, 0, 0xF),
    op(DP2ADD, 4), d(R, 3, 0x1) | PP, s(R, 0, XYZW) | m(11), s(R, 1, XYZW) | m(12), s(C, 0, XXXX),
    op(CMP, 4), d(R, 0, 0xF), s(R, 0, XYZW) | m(1), s(R, 1, XYZW), s(R, 2, ZZZZ),
    op(MOV, 2), d(OC, 0, 0xF) | SAT, s(R, 0, XYZW),
    op(MOV, 2), d(ODEPTH, 0, 0x1), s(R, 3, XXXX),
    END,
];

/// `vs_3_0`: shared output registers, `o31`, a `c2032+` read, predication, comparisons, `texldl`.
#[rustfmt::skip]
const VS30: &[u32] = &[
    vs(3, 0),
    op(DCL, 2), usage(0, 0), d(V, 0, 0xF),
    op(DCL, 2), usage(5, 3), d(V, 22, 0xF),
    op(DCL, 2), usage(0, 0), d(OUT, 0, 0xF),
    op(DCL, 2), usage(0, 0), d(OUT, 31, 0xF),
    op(DCL, 2), usage(5, 0), d(OUT, 1, 0x3),
    op(DCL, 2), usage(5, 1), d(OUT, 1, 0xC),
    op(DCL, 2), usage(4, 0), d(OUT, 2, 0x1),
    op(DCL, 2), usage(11, 0), d(OUT, 3, 0x1),
    op(DCL, 2), usage(10, 15), d(OUT, 4, 0xF),
    op(DCL, 2), sampler(2), d(S, 0, 0xF),
    op(DEF, 5), d(C, 256, 0xF), f(0.5), f(-std::f32::consts::LOG2_E), f(128.0), f(0.0),
    op(MOV, 2), d(R, 0, 0xF), s(C, 2044, XYZW),
    op(SETP, 3) | (1 << 16), d(P, 0, 0x1), s(R, 0, XXXX), s(C, 256, XXXX),
    op(MOV, 3) | PRED, d(R, 1, 0xF), s(P, 0, XXXX), s(V, 0, XYZW),
    op(ADD, 4) | PRED, d(R, 1, 0x3), s(P, 0, YYYY) | m(13), s(R, 1, XYZW), s(C, 2032, XYZW),
    op(IFC, 2) | (4 << 16), s(R, 0, XXXX), s(C, 256, YYYY),
    op(BREAKC, 2) | (2 << 16), s(R, 0, XXXX), s(C, 256, XXXX),
    op(ENDIF, 0),
    op(BREAKP, 1), s(P, 0, XXXX) | m(13),
    op(TEXLDL, 3), d(R, 2, 0xF), s(V, 22, XYZW), s(S, 0, XYZW),
    op(NRM, 2), d(R, 3, 0x7), s(R, 1, XYZW),
    op(POW, 3), d(R, 3, 0x8), s(R, 3, XXXX) | m(11), s(C, 256, ZZZZ),
    op(M4X4, 3), d(OUT, 0, 0xF), s(V, 0, XYZW), s(C, 0, XYZW),
    op(MOV, 2), d(OUT, 31, 0xF), s(R, 0, XYZW),
    op(MOV, 3), d(OUT, 4, 0xF) | SAT, s(C, 0, XYZW) | REL, s(A, 0, XXXX),
    op(NOP, 0),
    END,
];

/// `ps_3_0`: `vPos`/`vFace`, `bem`, a type-11 source, `texldd`, derivatives, `_centroid`, `oC1`.
#[rustfmt::skip]
const PS30: &[u32] = &[
    ps(3, 0),
    op(DCL, 2), usage(10, 0), d(V, 0, 0xF),
    op(DCL, 2), usage(5, 2), d(V, 4, 0x3),
    op(DCL, 2), usage(0, 0), d(MISC, 0, 0x3),
    op(DCL, 2), usage(0, 0), d(MISC, 1, 0x1),
    op(DCL, 2), sampler(2), d(S, 0, 0xF),
    op(DEF, 5), d(C, 16, 0xF), f(1.0), f(0.0), f(0.0), f(1.0),
    op(BEM, 3), d(R, 0, 0x3), s(V, 4, XYZW), s(R, 1, XYZW),
    op(MUL, 3), d(R, 0, 0x4), s(R, 0, XXXX), s(T11, 3, XXXX),
    op(TEXLDD, 5), d(R, 1, 0xF), s(V, 4, XYZW), s(S, 0, XYZW), s(R, 2, XYZW), s(R, 3, XYZW),
    op(DSX, 2), d(R, 2, 0xF) | CENTROID, s(V, 4, XYZW),
    op(DSY, 2), d(H, 0, 0xF), s(V, 4, XYZW),
    op(MOV, 2), d(OC, 1, 0xF), s(MISC, 1, XXXX),
    op(MOV, 2), d(OC, 0, 0xF), s(R, 1, XYZW) | m(2),
    END,
];

const CANONICAL: [&[u32]; 6] = [VS11, PS14, VS20, PS20, VS30, PS30];

/// A comment and a `phase`, both dropped by the parser.
#[rustfmt::skip]
const PS14_COMMENTED: &[u32] = &[
    ps(1, 4),
    0x0002_FFFE, 0x1234_5678, 0x9ABC_DEF0,
    op(TEXLD, 0), d(R, 0, 0xF), s(A, 0, XYZW),
    PHASE,
    op(MOV, 0), d(R, 0, 0xF), s(R, 0, XYZW),
    END,
];

/// Out of order: an instruction before a `def`, a `dcl` after it, main resuming after a body.
#[rustfmt::skip]
const VS20_UNORDERED: &[u32] = &[
    vs(2, 0),
    op(MOV, 2), d(RAST, 0, 0xF), s(V, 0, XYZW),
    op(DEF, 5), d(C, 1, 0xF), f(1.0), f(2.0), f(3.0), f(4.0),
    op(DCL, 2), usage(0, 0), d(V, 0, 0xF),
    op(LABEL, 1), s(L, 2, XYZW),
    op(MOV, 2), d(R, 0, 0xF), s(C, 1, XYZW),
    op(RET, 0),
    op(CALL, 1), s(L, 2, XYZW),
    END,
];

/// A centroid `dcl` (its modifier bits are not kept) and `texld` with both control bits set.
#[rustfmt::skip]
const PS30_DROPPED_BITS: &[u32] = &[
    ps(3, 0),
    op(DCL, 2), usage(5, 0), d(V, 0, 0xF) | CENTROID,
    op(DCL, 2), sampler(2), d(S, 0, 0xF),
    op(TEXLD, 3) | PROJECT | BIAS, d(R, 0, 0xF), s(V, 0, XYZW), s(S, 0, XYZW),
    op(MOV, 2), d(OC, 0, 0xF), s(R, 0, XYZW),
    END,
];

const NOT_CANONICAL: [&[u32]; 3] = [PS14_COMMENTED, VS20_UNORDERED, PS30_DROPPED_BITS];

fn def_bits(defs: &[DefConstant]) -> Vec<(Register, [u32; 4])> {
    defs.iter()
        .map(|d| (d.reg, d.value.map(f32::to_bits)))
        .collect()
}

/// Asserts that `b` has `a`'s parts, field by field, `def` floats by bits.
fn same_parts(a: &ProgramParts, b: &DxsoProgram) {
    assert_eq!(a.shader_type, b.shader_type);
    assert_eq!((a.major, a.minor), (b.major, b.minor));
    assert_eq!(a.declarations, b.declarations.as_slice());
    assert_eq!(def_bits(a.def_constants), def_bits(&b.def_constants));
    assert_eq!(a.def_int_constants, b.def_int_constants.as_slice());
    assert_eq!(a.def_bool_constants, b.def_bool_constants.as_slice());
    assert_eq!(a.instructions, b.instructions.as_slice());
    assert_eq!(a.subroutines, &b.subroutines);
}

fn parts(p: &DxsoProgram) -> ProgramParts<'_> {
    ProgramParts {
        shader_type: p.shader_type,
        major: p.major,
        minor: p.minor,
        declarations: &p.declarations,
        def_constants: &p.def_constants,
        def_int_constants: &p.def_int_constants,
        def_bool_constants: &p.def_bool_constants,
        instructions: &p.instructions,
        subroutines: &p.subroutines,
    }
}

#[test]
fn round_trips_hand_programs() {
    for (n, stream) in CANONICAL.iter().chain(&NOT_CANONICAL).enumerate() {
        let program = parse(stream).unwrap_or_else(|e| panic!("stream {n}: {e:?}"));
        let written = write(&program);
        let back = parse(&written).unwrap_or_else(|e| panic!("stream {n} written: {e:?}"));
        same_parts(&parts(&program), &back);
    }
}

#[test]
fn writes_canonical_streams_back_identically() {
    for (n, stream) in CANONICAL.iter().enumerate() {
        let program = parse(stream).expect("a hand stream parses");
        assert_eq!(write(&program), *stream, "stream {n}");
    }
    // Not from `bytecode`: what the parser drops is gone, and the result is a fixpoint.
    for (n, stream) in NOT_CANONICAL.iter().enumerate() {
        let written = write(&parse(stream).expect("a hand stream parses"));
        assert_ne!(written, *stream, "stream {n}");
        assert!(
            !written.contains(&0x1234_5678),
            "stream {n} kept the comment"
        );
        assert_eq!(
            write(&parse(&written).expect("written stream parses")),
            written,
            "stream {n}"
        );
    }
}

/// Every opcode `from_u16` names that an instruction list may hold.
fn instruction_opcodes() -> Vec<Opcode> {
    (0..=u16::MAX)
        .filter_map(Opcode::from_u16)
        .filter(|op| {
            !matches!(
                op,
                Opcode::Dcl
                    | Opcode::Def
                    | Opcode::DefI
                    | Opcode::DefB
                    | Opcode::Label
                    | Opcode::Ret
                    | Opcode::Phase
                    | Opcode::Comment
                    | Opcode::End
            )
        })
        .collect()
}

#[test]
fn write_parts_sweeps_every_kind_modifier_usage_and_opcode() {
    const KINDS: [RegKind; 18] = [
        RegKind::Temp,
        RegKind::Input,
        RegKind::Const,
        RegKind::Addr,
        RegKind::RastOut,
        RegKind::AttrOut,
        RegKind::TexcoordOut,
        RegKind::ConstInt,
        RegKind::ColorOut,
        RegKind::DepthOut,
        RegKind::Sampler,
        RegKind::Output,
        RegKind::ConstBool,
        RegKind::Loop,
        RegKind::TempFloat16,
        RegKind::MiscType,
        RegKind::Label,
        RegKind::Predicate,
    ];
    const MODIFIERS: [SrcModifier; 14] = [
        SrcModifier::None,
        SrcModifier::Neg,
        SrcModifier::Bias,
        SrcModifier::BiasNeg,
        SrcModifier::Sign,
        SrcModifier::SignNeg,
        SrcModifier::Comp,
        SrcModifier::X2,
        SrcModifier::X2Neg,
        SrcModifier::Dz,
        SrcModifier::Dw,
        SrcModifier::Abs,
        SrcModifier::AbsNeg,
        SrcModifier::Not,
    ];
    const USAGES: [DeclUsage; 14] = [
        DeclUsage::Position,
        DeclUsage::BlendWeight,
        DeclUsage::BlendIndices,
        DeclUsage::Normal,
        DeclUsage::PSize,
        DeclUsage::Texcoord,
        DeclUsage::Tangent,
        DeclUsage::Binormal,
        DeclUsage::TessFactor,
        DeclUsage::PositionT,
        DeclUsage::Color,
        DeclUsage::Fog,
        DeclUsage::Depth,
        DeclUsage::Sample,
    ];
    const TEXTURES: [TextureType; 4] = [
        TextureType::Unknown,
        TextureType::Texture2D,
        TextureType::TextureCube,
        TextureType::Texture3D,
    ];
    let mut declarations: Vec<Declaration> = USAGES
        .iter()
        .zip(0u16..)
        .map(|(&usage, i)| Declaration::Semantic {
            usage,
            usage_index: u32::from(i),
            reg: Register {
                kind: RegKind::Input,
                index: i,
            },
            mask: WriteMask(u8::try_from(i % 15 + 1).expect("a mask")),
        })
        .collect();
    declarations.extend(TEXTURES.iter().zip(0u16..).map(|(&texture_type, i)| {
        Declaration::Sampler {
            texture_type,
            reg: Register {
                kind: RegKind::Sampler,
                index: i,
            },
        }
    }));
    let a0y = RelativeAddr {
        reg: Register {
            kind: RegKind::Addr,
            index: 0,
        },
        swizzle: Swizzle([1; 4]),
    };
    let mut instructions: Vec<Instruction> = KINDS
        .iter()
        .zip(MODIFIERS.iter().cycle())
        .zip(0u16..)
        .map(|((&kind, &modifier), i)| {
            let source = SrcOperand {
                swizzle: Swizzle([3, 2, 1, 0]),
                modifier,
                ..src(kind, 0x7FF - i)
            };
            instruction(Opcode::Mov, Some(dst(kind, i, WriteMask(0b0101))), [source])
        })
        .collect();
    instructions.extend(MODIFIERS.iter().map(|&modifier| {
        instruction(
            Opcode::Add,
            Some(DstOperand {
                mods: DstMods::all(),
                shift_scale: -8,
                ..dst(RegKind::Temp, 31, WriteMask::ALL)
            }),
            [
                SrcOperand {
                    modifier,
                    rel_addr: Some(a0y),
                    ..src(RegKind::Const, 2044)
                },
                src(RegKind::Temp, 0),
            ],
        )
    }));
    for opcode in instruction_opcodes() {
        let d = opcode
            .has_destination()
            .then(|| dst(RegKind::Temp, 1, WriteMask(0b1000)));
        instructions.push(instruction(opcode, d, [src(RegKind::Temp, 2)]));
    }
    let mut predicated = instruction(
        Opcode::Mov,
        Some(dst(RegKind::Temp, 0, WriteMask::ALL)),
        [src(RegKind::Input, 0)],
    );
    predicated.flags = InstrFlags::PREDICATED | InstrFlags::COISSUE;
    predicated.predicate = Some(SrcOperand {
        modifier: SrcModifier::Not,
        ..src(RegKind::Predicate, 0)
    });
    instructions.push(predicated);
    for cmp in [
        CmpFunc::Gt,
        CmpFunc::Eq,
        CmpFunc::Ge,
        CmpFunc::Lt,
        CmpFunc::Ne,
        CmpFunc::Le,
    ] {
        let mut setp = instruction(
            Opcode::SetP,
            Some(dst(RegKind::Predicate, 0, WriteMask::ALL)),
            [src(RegKind::Temp, 0), src(RegKind::Const, 1)],
        );
        setp.cmp_func = Some(cmp);
        instructions.push(setp);
    }
    for flag in [InstrFlags::TEX_PROJECTED, InstrFlags::TEX_BIASED] {
        let mut texld = instruction(
            Opcode::TexLd,
            Some(dst(RegKind::Temp, 0, WriteMask::ALL)),
            [src(RegKind::Input, 0), src(RegKind::Sampler, 0)],
        );
        texld.flags = flag;
        instructions.push(texld);
    }
    let subroutines = BTreeMap::from([
        (0, Vec::new()),
        (
            0x7FF,
            vec![instruction(
                Opcode::Mov,
                Some(dst(RegKind::Temp, 0, WriteMask::ALL)),
                [src(RegKind::Temp, 1)],
            )],
        ),
    ]);
    let def_constants = [DefConstant {
        reg: Register {
            kind: RegKind::Const,
            index: 300,
        },
        value: [f32::NAN, f32::NEG_INFINITY, -0.0, f32::MIN_POSITIVE],
    }];
    let def_int_constants = [DefIntConstant {
        reg: Register {
            kind: RegKind::ConstInt,
            index: 15,
        },
        value: [i32::MIN, -1, 0, i32::MAX],
    }];
    let def_bool_constants = [
        DefBoolConstant {
            reg: Register {
                kind: RegKind::ConstBool,
                index: 0,
            },
            value: true,
        },
        DefBoolConstant {
            reg: Register {
                kind: RegKind::ConstBool,
                index: 15,
            },
            value: false,
        },
    ];
    for shader_type in [ShaderType::Vertex, ShaderType::Pixel] {
        let program = ProgramParts {
            shader_type,
            major: 3,
            minor: 0,
            declarations: &declarations,
            def_constants: &def_constants,
            def_int_constants: &def_int_constants,
            def_bool_constants: &def_bool_constants,
            instructions: &instructions,
            subroutines: &subroutines,
        };
        let back = parse(&write_parts(&program)).expect("written parts parse");
        same_parts(&program, &back);
    }
}

#[test]
fn sm1_relative_reads_need_no_address_token() {
    let a0x = RelativeAddr {
        reg: Register {
            kind: RegKind::Addr,
            index: 0,
        },
        swizzle: Swizzle([0; 4]),
    };
    let instructions = [instruction(
        Opcode::Mov,
        Some(dst(RegKind::RastOut, 0, WriteMask::ALL)),
        [SrcOperand {
            rel_addr: Some(a0x),
            ..src(RegKind::Const, 8)
        }],
    )];
    let program = ProgramParts {
        shader_type: ShaderType::Vertex,
        major: 1,
        minor: 1,
        declarations: &[],
        def_constants: &[],
        def_int_constants: &[],
        def_bool_constants: &[],
        instructions: &instructions,
        subroutines: &BTreeMap::new(),
    };
    let tokens = write_parts(&program);
    assert_eq!(
        tokens,
        [
            vs(1, 1),
            op(MOV, 0),
            d(RAST, 0, 0xF),
            s(C, 8, XYZW) | REL,
            END
        ]
    );
    same_parts(&program, &parse(&tokens).expect("written parts parse"));
}

#[test]
fn opcode_values_invert_from_u16() {
    for v in 0..=u16::MAX {
        if let Some(opcode) = Opcode::from_u16(v) {
            assert_eq!(opcode_value(opcode), u32::from(v), "{opcode:?}");
        }
    }
}
