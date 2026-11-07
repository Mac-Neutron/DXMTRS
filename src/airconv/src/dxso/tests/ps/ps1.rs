//! D3D9 pixel shaders 1.0-1.4.
//!
//! The `t#` file, the texture-addressing ops, bump env, the modifiers and the `[-1, 1]` constant
//! clamp, each as `emit.rs` writes it.
//!
//! An SM1 stream carries no instruction length: the parser counts the operand tokens by their
//! bit 31, so these programs write their opcode tokens with a length of 0.

use airconv_ir::{
    binding::{FunctionOutput, TextureKind},
    dxso::{
        lower::{LowerError, Lowered, range},
        spec::{VariantFlags, VariantKey},
    },
    ir::{FloatBinaryOp, FloatComparison, Instruction, SrcOperand},
};

use super::{
    super::{
        ADDR, ALL, CONST, INPUT, TEMP, XXXX, XYZW, count, d, def, ins, instructions, op, program,
        reads_row, s,
    },
    IN_TEXCOORD, any_read, arguments, discards, flags, key, outputs, plowered, pmetallib, prefused,
    reads_input, reads_input_lane,
};

const PS_1_1: u32 = 0xffff_0101;
const PS_1_2: u32 = 0xffff_0102;
const PS_1_3: u32 = 0xffff_0103;
const PS_1_4: u32 = 0xffff_0104;
const PS_2_0: u32 = 0xffff_0200;

const WWWW: u8 = 0xff;
const RGB: u8 = 0b0111;
const ALPHA: u8 = 0b1000;
const COISSUE: u32 = 1 << 30;
const SATURATE: u32 = 1 << 20;
const PHASE: u32 = 0x0000_fffd;

mod sm1 {
    pub const TEXCOORD: u16 = 64;
    pub const TEXKILL: u16 = 65;
    pub const TEX: u16 = 66;
    pub const TEXBEM: u16 = 67;
    pub const TEXBEML: u16 = 68;
    pub const TEXREG2AR: u16 = 69;
    pub const TEXREG2GB: u16 = 70;
    pub const TEXM3X2PAD: u16 = 71;
    pub const TEXM3X2TEX: u16 = 72;
    pub const TEXM3X3PAD: u16 = 73;
    pub const TEXM3X3TEX: u16 = 74;
    pub const TEXM3X3SPEC: u16 = 76;
    pub const TEXM3X3VSPEC: u16 = 77;
    pub const CND: u16 = 80;
    pub const TEXREG2RGB: u16 = 82;
    pub const TEXDP3TEX: u16 = 83;
    pub const TEXM3X2DEPTH: u16 = 84;
    pub const TEXDP3: u16 = 85;
    pub const TEXM3X3: u16 = 86;
    pub const TEXDEPTH: u16 = 87;
    pub const BEM: u16 = 89;
}

/// An SM1 instruction: its opcode token (no length) and its operands.
fn i1(opcode: u16, operands: &[u32]) -> Vec<u32> {
    let mut t = vec![ins(opcode, 0)];
    t.extend_from_slice(operands);
    t
}

/// `t<n>` as a destination and as a source.
const fn td(n: u32) -> u32 {
    d(ADDR, n, ALL)
}
const fn ts(n: u32) -> u32 {
    s(ADDR, n, XYZW)
}
const fn rd(n: u32) -> u32 {
    d(TEMP, n, ALL)
}
const fn rs(n: u32) -> u32 {
    s(TEMP, n, XYZW)
}

/// The samples of the program, by slot.
fn sampled_slots(l: &Lowered) -> Vec<u32> {
    let mut slots: Vec<u32> = instructions(l)
        .filter_map(|i| match i {
            Instruction::Sample(x) => Some(x.src_resource.range_id),
            Instruction::SampleBias(x) => Some(x.src_resource.range_id),
            Instruction::SampleLod(x) => Some(x.src_resource.range_id),
            Instruction::SampleCompare(x) => Some(x.src_resource.range_id),
            Instruction::SampleCompareLevel(x) => Some(x.src_resource.range_id),
            _ => None,
        })
        .collect();
    slots.sort_unstable();
    slots
}

fn divides(l: &Lowered) -> usize {
    count(
        l,
        |i| matches!(i, Instruction::FloatBinaryOp(x) if x.op == FloatBinaryOp::Div),
    )
}

fn saturating_movs(l: &Lowered) -> usize {
    count(l, |i| matches!(i, Instruction::Mov(m) if m.common.saturate))
}

fn dots(l: &Lowered, dim: u8) -> usize {
    count(
        l,
        |i| matches!(i, Instruction::DotProduct(x) if x.dimension == dim),
    )
}

/// Whether `op` is the four float constants `v`.
fn is_imm(op: &SrcOperand, v: [f32; 4]) -> bool {
    matches!(op, SrcOperand::Immediate32 { value, .. } if *value == v.map(f32::to_bits))
}

/// Whether `op` is one float constant in every lane.
fn is_splat(op: &SrcOperand, v: f32) -> bool {
    is_imm(op, [v; 4])
}

fn textures(t: &[u32], k: VariantKey) -> Vec<(u32, TextureKind)> {
    arguments(t, k).1
}

fn buffers(t: &[u32], k: VariantKey) -> Vec<u32> {
    arguments(t, k).0
}

fn writes_depth(l: &Lowered) -> bool {
    outputs(l)
        .iter()
        .any(|o| matches!(o, FunctionOutput::Depth { .. }))
}

/// `ps_1_1`: `tex t0; texcoord t1; mov r0, t0; add r0, r0, t1`.
fn tex_texcoord() -> Vec<u32> {
    program(
        PS_1_1,
        &[
            i1(sm1::TEX, &[td(0)]),
            i1(sm1::TEXCOORD, &[td(1)]),
            i1(op::MOV, &[rd(0), ts(0)]),
            i1(op::ADD, &[rd(0), rs(0), ts(1)]),
        ],
    )
}

#[test]
fn tex_texcoord_ps_1_1_to_1_3() {
    let t = tex_texcoord();
    let l = plowered(&t, VariantKey::default());
    // `t[0..8]` seeded from the texture coordinates.
    for i in 0..8 {
        assert!(any_read(&l, |o| reads_input(o, IN_TEXCOORD + i)), "t{i}");
    }
    // `tex t0` samples stage 0 at `t0`; the slot is synthesized, 2D.
    assert_eq!(sampled_slots(&l), [0]);
    assert_eq!(
        textures(&t, VariantKey::default()),
        [(0, TextureKind::Texture2d)]
    );
    assert_eq!(buffers(&t, VariantKey::default()), [15]);
    // `texcoord t1`: `float4(saturate(in.texcoord1).xyz, 1.0)`, the varying itself.
    assert_eq!(
        count(&l, |i| matches!(i, Instruction::Mov(m)
            if m.common.saturate && reads_input(&m.src, IN_TEXCOORD + 1))),
        1
    );
    // `oC0 = r[0]`.
    assert!(instructions(&l).any(|i| matches!(i, Instruction::Mov(m)
        if matches!(m.src, SrcOperand::Temp { regid: 0, .. }))));
    assert_eq!(divides(&l), 0);
    pmetallib(&t, VariantKey::default());

    // `D3DTTFF_PROJECTED` divides the coordinate by its `.w` (a zero `.w` reads the origin),
    // unless the stage's texture is a cube.
    let projected = key(|k| k.tt_projected_mask = 1);
    let l = plowered(&t, projected);
    assert_eq!(divides(&l), 1);
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatCompare(c)
        if c.cmp == FloatComparison::NotEqual && is_splat(&c.src1, 0.0)))
    );
    pmetallib(&t, projected);
    let cube = key(|k| {
        k.tt_projected_mask = 1;
        k.cube_sampler_mask = 1;
    });
    assert_eq!(divides(&plowered(&t, cube)), 0);
    assert_eq!(textures(&t, cube), [(0, TextureKind::TextureCube)]);
    pmetallib(&t, cube);

    // `ps_1_3`'s `texkill t2` tests the iterated coordinate, `.xyz`.
    let k = program(
        PS_1_3,
        &[
            i1(sm1::TEX, &[td(2)]),
            i1(sm1::TEXKILL, &[td(2)]),
            i1(op::MOV, &[rd(0), ts(2)]),
        ],
    );
    let l = plowered(&k, VariantKey::default());
    assert_eq!(discards(&l), 1);
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatCompare(c)
        if c.cmp == FloatComparison::LessThan && reads_input(&c.src0, IN_TEXCOORD + 2)))
    );
    assert_eq!(
        count(&l, |i| matches!(i, Instruction::IntegerBinaryOp(_))),
        2,
        "x | y | z"
    );
    pmetallib(&k, VariantKey::default());

    // The MSL declares `t[8]`.
    let past = program(PS_1_1, &[i1(op::MOV, &[rd(0), ts(8)])]);
    assert!(matches!(
        prefused(&past, VariantKey::default()),
        LowerError::Unsupported { opcode: 1, .. }
    ));
}

#[test]
fn texcrd_texld_phase_ps_1_4() {
    // `texcrd r0, t0; texld r1, t1; phase; texld r2, r0; texkill r1; add r0, r1, r2`.
    let t = program(
        PS_1_4,
        &[
            i1(sm1::TEXCOORD, &[d(TEMP, 0, RGB), ts(0)]),
            i1(sm1::TEX, &[rd(1), ts(1)]),
            vec![PHASE],
            i1(sm1::TEX, &[rd(2), rs(0)]),
            i1(sm1::TEXKILL, &[rd(1)]),
            i1(op::ADD, &[rd(0), rs(1), rs(2)]),
        ],
    );
    let l = plowered(&t, VariantKey::default());
    // `texcrd` is not clamped; `texld rN` samples stage N at its source.
    assert_eq!(saturating_movs(&l), 0);
    assert_eq!(sampled_slots(&l), [1, 2]);
    assert_eq!(
        textures(&t, VariantKey::default()),
        [(1, TextureKind::Texture2d), (2, TextureKind::Texture2d)]
    );
    // `texkill r1` tests `r1.xyz`.
    assert_eq!(discards(&l), 1);
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatCompare(c)
        if c.cmp == FloatComparison::LessThan
            && matches!(c.src0, SrcOperand::Temp { regid: 1, .. })))
    );
    pmetallib(&t, VariantKey::default());
    // `ps_1_4` projects through `_dz`/`_dw`, never by the stage flag.
    let projected = key(|k| k.tt_projected_mask = 0xff);
    assert_eq!(divides(&plowered(&t, projected)), 0);
    pmetallib(&t, projected);
    // A depth slot compares, a fetch4 slot gathers, as a shader model 2 sample does.
    let depth = key(|k| k.depth_sampler_mask = 0b100);
    let l = plowered(&t, depth);
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::SampleCompare(x) if x.src_resource.range_id == 2)
        ),
        1
    );
    pmetallib(&t, depth);
    let fetch4 = key(|k| k.fetch4_mask = 0b10);
    assert_eq!(
        count(&plowered(&t, fetch4), |i| matches!(
            i,
            Instruction::Gather(_)
        )),
        1
    );
    pmetallib(&t, fetch4);
}

/// `ps_1_1`: `tex t0; texbem t1, t0; texbeml t2, t0; add r0, t1, t2`.
fn bumps() -> Vec<u32> {
    program(
        PS_1_1,
        &[
            i1(sm1::TEX, &[td(0)]),
            i1(sm1::TEXBEM, &[td(1), ts(0)]),
            i1(sm1::TEXBEML, &[td(2), ts(0)]),
            i1(op::ADD, &[rd(0), ts(1), ts(2)]),
        ],
    )
}

fn reads_bump_row(l: &Lowered, row: u32) -> bool {
    any_read(l, |o| reads_row(o, range::BUMP_ENV, row))
}

#[test]
fn texbem_texbeml_bump_env_rows() {
    let t = bumps();
    let l = plowered(&t, VariantKey::default());
    assert_eq!(sampled_slots(&l), [0, 1, 2]);
    // Stage N's matrix is row 2N, its luminance scale and offset row 2N + 1.
    assert!(reads_bump_row(&l, 2));
    assert!(!reads_bump_row(&l, 3), "texbem reads no luminance");
    assert!(reads_bump_row(&l, 4));
    assert!(reads_bump_row(&l, 5));
    // `saturate(t0.z * lscale + loffset)`.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatMad(m)
        if reads_row(&m.src1, range::BUMP_ENV, 5) && reads_row(&m.src2, range::BUMP_ENV, 5)))
    );
    // `bump_env` at slot 12, after `fog_data`, only for a program with a bump op.
    assert_eq!(buffers(&t, VariantKey::default()), [15, 12]);
    let fogged = key(|k| k.fog_mode = 1);
    assert_eq!(buffers(&t, fogged), [15, 13, 12]);
    assert_eq!(buffers(&tex_texcoord(), VariantKey::default()), [15]);
    pmetallib(&t, VariantKey::default());
    pmetallib(&t, fogged);
    // A projected stage divides its own coordinate before the offset.
    let projected = key(|k| k.tt_projected_mask = 0b10);
    assert_eq!(divides(&plowered(&t, projected)), 1);
    pmetallib(&t, projected);
    // A cube destination keeps its own third coordinate.
    let cube = key(|k| k.cube_sampler_mask = 0b100);
    pmetallib(&t, cube);

    // `ps_1_4`: `texld r0, t0; bem r1.xy, c0, r0; texld r2, r1; mov r0, r2`, stage 1's rows.
    let b = program(
        PS_1_4,
        &[
            i1(sm1::TEX, &[rd(0), ts(0)]),
            i1(sm1::BEM, &[d(TEMP, 1, 0b0011), s(CONST, 0, XYZW), rs(0)]),
            vec![PHASE],
            i1(sm1::TEX, &[rd(2), rs(1)]),
            i1(op::MOV, &[rd(0), rs(2)]),
        ],
    );
    let l = plowered(&b, VariantKey::default());
    assert!(reads_bump_row(&l, 2));
    assert!(!reads_bump_row(&l, 3));
    assert_eq!(sampled_slots(&l), [0, 2]);
    assert_eq!(buffers(&b, VariantKey::default()), [15, 12]);
    pmetallib(&b, VariantKey::default());
}

#[test]
fn texm3x2_texm3x3_family() {
    let pad2 = |last: u16, dst: u32| {
        program(
            PS_1_3,
            &[
                i1(sm1::TEX, &[td(0)]),
                i1(sm1::TEXM3X2PAD, &[td(1), ts(0)]),
                i1(last, &[td(dst), ts(0)]),
                i1(op::MOV, &[rd(0), ts(0)]),
            ],
        )
    };
    // `texm3x2tex t2, t0`: `(t1.x, dot(t2.xyz, t0.xyz))` into stage 2.
    let t = pad2(sm1::TEXM3X2TEX, 2);
    let l = plowered(&t, VariantKey::default());
    assert_eq!(dots(&l, 3), 2);
    assert_eq!(sampled_slots(&l), [0, 2]);
    assert!(!writes_depth(&l));
    pmetallib(&t, VariantKey::default());
    // `texm3x2depth t2, t0`: the fragment depth `w != 0 ? saturate(t1.x / w) : 1`.
    let t = pad2(sm1::TEXM3X2DEPTH, 2);
    let l = plowered(&t, VariantKey::default());
    assert!(writes_depth(&l));
    assert_eq!(sampled_slots(&l), [0]);
    assert_eq!(divides(&l), 1);
    pmetallib(&t, VariantKey::default());
    assert!(!writes_depth(&plowered(
        &t,
        flags(VariantFlags::NO_DEPTH_ATTACHMENT)
    )));

    let pad3 = |last: u16, extra: &[u32]| {
        let mut operands = vec![td(3), ts(0)];
        operands.extend_from_slice(extra);
        program(
            PS_1_1,
            &[
                i1(sm1::TEX, &[td(0)]),
                i1(sm1::TEXM3X3PAD, &[td(1), ts(0)]),
                i1(sm1::TEXM3X3PAD, &[td(2), ts(0)]),
                i1(last, &operands),
                i1(op::MOV, &[rd(0), ts(3)]),
            ],
        )
    };
    // `texm3x3tex`: `(t1.x, t2.x, dot)` into stage 3.
    let t = pad3(sm1::TEXM3X3TEX, &[]);
    let l = plowered(&t, VariantKey::default());
    assert_eq!(dots(&l, 3), 3);
    assert_eq!(sampled_slots(&l), [0, 3]);
    pmetallib(&t, VariantKey::default());
    // `texm3x3`: the normal itself, `.w` 1, no sample.
    let t = pad3(sm1::TEXM3X3, &[]);
    let l = plowered(&t, VariantKey::default());
    assert_eq!(sampled_slots(&l), [0]);
    assert_eq!(
        textures(&t, VariantKey::default()),
        [(0, TextureKind::Texture2d)]
    );
    pmetallib(&t, VariantKey::default());
    // `texm3x3spec t3, t0, c0`: the eye reflected about the normal, into a cube.
    let cube = key(|k| k.cube_sampler_mask = 0b1000);
    let t = pad3(sm1::TEXM3X3SPEC, &[s(CONST, 0, XYZW)]);
    let l = plowered(&t, cube);
    assert_eq!(dots(&l, 3), 5, "three rows, dot(n, e), dot(n, n)");
    assert_eq!(divides(&l), 1);
    assert_eq!(sampled_slots(&l), [0, 3]);
    assert_eq!(
        textures(&t, cube),
        [(0, TextureKind::Texture2d), (3, TextureKind::TextureCube)]
    );
    pmetallib(&t, cube);
    // `texm3x3vspec`: the eye is the three iterated coordinates' `.w`.
    let t = pad3(sm1::TEXM3X3VSPEC, &[]);
    let l = plowered(&t, cube);
    for i in 1..4 {
        assert!(
            any_read(&l, |o| reads_input_lane(o, IN_TEXCOORD + i, 3)),
            "t{i}.w"
        );
    }
    pmetallib(&t, cube);

    // Shader model 2 has no `t[]`: the MSL's `t[1].x` does not compile.
    let sm2 = program(
        PS_2_0,
        &[
            vec![ins(sm1::TEXM3X2TEX, 2), rd(1), rs(0)],
            vec![ins(op::MOV, 2), d(8, 0, ALL), rs(1)],
        ],
    );
    assert!(matches!(
        prefused(&sm2, VariantKey::default()),
        LowerError::Unsupported { opcode: 72, .. }
    ));
}

#[test]
fn texreg2ar_gb_rgb() {
    let t = program(
        PS_1_2,
        &[
            i1(sm1::TEX, &[td(0)]),
            i1(sm1::TEXREG2AR, &[td(1), ts(0)]),
            i1(sm1::TEXREG2GB, &[td(2), ts(0)]),
            i1(sm1::TEXREG2RGB, &[td(3), ts(0)]),
            i1(op::ADD, &[rd(0), ts(1), ts(2)]),
            i1(op::ADD, &[rd(0), rs(0), ts(3)]),
        ],
    );
    let volume = key(|k| k.volume_sampler_mask = 0b1000);
    let l = plowered(&t, volume);
    assert_eq!(sampled_slots(&l), [0, 1, 2, 3]);
    // `(t0.a, t0.r)`, `(t0.g, t0.b)` from `t0`'s mirror.
    let picks = |sw: [u8; 2]| {
        instructions(&l).any(|i| {
            matches!(i, Instruction::Mov(m)
            if matches!(&m.src, SrcOperand::Temp { common, .. }
                if common.swizzle.0[..2] == sw))
        })
    };
    assert!(picks([3, 0]));
    assert!(picks([1, 2]));
    assert_eq!(
        textures(&t, volume),
        [
            (0, TextureKind::Texture2d),
            (1, TextureKind::Texture2d),
            (2, TextureKind::Texture2d),
            (3, TextureKind::Texture3d)
        ]
    );
    pmetallib(&t, volume);
    pmetallib(&t, VariantKey::default());
}

#[test]
fn texdp3_texdp3tex_texdepth() {
    // `tex t0; texdp3tex t1, t0; texdp3 t2, t0; add r0, t1, t2`.
    let t = program(
        PS_1_2,
        &[
            i1(sm1::TEX, &[td(0)]),
            i1(sm1::TEXDP3TEX, &[td(1), ts(0)]),
            i1(sm1::TEXDP3, &[td(2), ts(0)]),
            i1(op::ADD, &[rd(0), ts(1), ts(2)]),
        ],
    );
    let l = plowered(&t, VariantKey::default());
    assert_eq!(dots(&l, 3), 2);
    assert_eq!(sampled_slots(&l), [0, 1]);
    assert_eq!(
        textures(&t, VariantKey::default()),
        [(0, TextureKind::Texture2d), (1, TextureKind::Texture2d)]
    );
    pmetallib(&t, VariantKey::default());

    // `ps_1_4`: `texcrd r5, t0; phase; texdepth r5; mov r0, c0`: `saturate(r5.x / min(r5.y, 1))`.
    let t = program(
        PS_1_4,
        &[
            i1(sm1::TEXCOORD, &[rd(5), ts(0)]),
            vec![PHASE],
            i1(sm1::TEXDEPTH, &[rd(5)]),
            i1(op::MOV, &[rd(0), s(CONST, 0, XYZW)]),
        ],
    );
    let l = plowered(&t, VariantKey::default());
    assert!(writes_depth(&l));
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(x)
        if x.op == FloatBinaryOp::Min && is_splat(&x.src1, 1.0)))
    );
    assert_eq!(divides(&l), 1);
    assert_eq!(saturating_movs(&l), 1);
    pmetallib(&t, VariantKey::default());
    let none = flags(VariantFlags::NO_DEPTH_ATTACHMENT);
    assert!(!writes_depth(&plowered(&t, none)));
    pmetallib(&t, none);
}

/// Whether `l` compares `0.5 < src` on temporary 0 read through `swizzle`.
fn half_compares(l: &Lowered, swizzle: [u8; 4]) -> usize {
    count(l, |i| {
        matches!(i, Instruction::FloatCompare(c)
        if c.cmp == FloatComparison::LessThan
            && is_splat(&c.src0, 0.5)
            && matches!(&c.src1, SrcOperand::Temp { regid: 0, common, .. }
                if common.swizzle.0 == swizzle))
    })
}

#[test]
fn cnd_r0a_half_rule() {
    let cnd = |header: u32, flags: u32, mask: u8, sw: u8| {
        program(
            header,
            &[
                i1(op::MOV, &[rd(0), s(CONST, 0, XYZW)]),
                vec![
                    ins(sm1::CND, 0) | flags,
                    d(TEMP, 1, mask),
                    s(TEMP, 0, sw),
                    s(CONST, 1, XYZW),
                    s(CONST, 2, XYZW),
                ],
                i1(op::MOV, &[rd(0), rs(1)]),
            ],
        )
    };
    // `ps_1_1`: `r0.a > 0.5 ? s1 : s2`, broadcast.
    let t = cnd(PS_1_1, 0, ALL, WWWW);
    assert_eq!(
        half_compares(&plowered(&t, VariantKey::default()), [3; 4]),
        1
    );
    pmetallib(&t, VariantKey::default());
    // A co-issued alpha `cnd` still compares; a co-issued colour one is `s1`.
    let t = cnd(PS_1_1, COISSUE, ALPHA, WWWW);
    assert_eq!(
        half_compares(&plowered(&t, VariantKey::default()), [3; 4]),
        1
    );
    pmetallib(&t, VariantKey::default());
    let t = cnd(PS_1_1, COISSUE, RGB, WWWW);
    let l = plowered(&t, VariantKey::default());
    assert_eq!(count(&l, |i| matches!(i, Instruction::FloatCompare(_))), 0);
    pmetallib(&t, VariantKey::default());
    // `ps_1_4` compares per lane.
    let t = cnd(PS_1_4, 0, ALL, XYZW);
    assert_eq!(
        half_compares(&plowered(&t, VariantKey::default()), [0, 1, 2, 3]),
        1
    );
    pmetallib(&t, VariantKey::default());
}

#[test]
fn modifiers_bias_bx2_x2_x4_x8_d2_d4_d8_sat() {
    let shift = |n: i8| (u32::from(n.cast_unsigned()) & 0xf) << 24;
    let src_mod = |m: u32| m << 24;
    let t = program(
        PS_1_4,
        &[
            // `mov_x2 r0, v0_bias`, `mul_x4 r1, r0, v1_bx2`, `add_x8_sat r1, r1, c0_x2`.
            i1(op::MOV, &[rd(0) | shift(1), s(INPUT, 0, XYZW) | src_mod(2)]),
            i1(
                op::MUL,
                &[rd(1) | shift(2), rs(0), s(INPUT, 1, XYZW) | src_mod(4)],
            ),
            i1(
                op::ADD,
                &[
                    rd(1) | shift(3) | SATURATE,
                    rs(1),
                    s(CONST, 0, XYZW) | src_mod(7),
                ],
            ),
            // `mov_d2 r2, r1`, `mov_d4 r2, r2`, `mov_d8 r0, r2`.
            i1(op::MOV, &[rd(2) | shift(-1), rs(1)]),
            i1(op::MOV, &[rd(2) | shift(-2), rs(2)]),
            i1(op::MOV, &[rd(0) | shift(-3), rs(2)]),
        ],
    );
    let l = plowered(&t, VariantKey::default());
    // Each destination scale multiplies the result by 2^shift.
    for scale in [2.0, 4.0, 8.0, 0.5, 0.25, 0.125] {
        assert_eq!(
            count(&l, |i| matches!(i, Instruction::FloatBinaryOp(x)
                if x.op == FloatBinaryOp::Mul && is_splat(&x.src1, scale))),
            1,
            "x{scale}"
        );
    }
    // `_bias` is `x - 0.5`, `_bx2` `2x - 1`, `_x2` `2x`.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(x)
        if x.op == FloatBinaryOp::Add && is_splat(&x.src1, -0.5)))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatMad(m)
        if is_splat(&m.src1, 2.0) && is_splat(&m.src2, -1.0)))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(x)
        if x.op == FloatBinaryOp::Mul && is_splat(&x.src0, 2.0)))
    );
    // `_sat` after the scale: the store saturates what the `x8` multiply wrote.
    let order: Vec<&Instruction> = instructions(&l)
        .skip_while(|i| {
            !matches!(i, Instruction::FloatBinaryOp(x)
            if x.op == FloatBinaryOp::Mul && is_splat(&x.src1, 8.0))
        })
        .take(2)
        .collect();
    assert!(matches!(order.get(1), Some(Instruction::Mov(m)) if m.common.saturate));
    assert_eq!(saturating_movs(&l), 1);
    pmetallib(&t, VariantKey::default());
}

#[test]
fn ps_1_x_range_clamp() {
    let body = |header: u32| {
        // Shader model 2 counts each instruction's operands in its token.
        let inst = |opcode: u16, operands: &[u32]| {
            let mut t = i1(opcode, operands);
            if header >= PS_2_0 {
                t[0] = ins(opcode, 3);
            }
            t
        };
        program(
            header,
            &[
                def(1, [2.0, -3.0, 0.5, -0.25]),
                inst(op::ADD, &[rd(0), s(CONST, 0, XYZW), s(CONST, 1, XYZW)]),
                inst(op::MUL, &[rd(0), rs(0), s(CONST, 2, XXXX)]),
            ],
        )
    };
    let t = body(PS_1_1);
    let l = plowered(&t, VariantKey::default());
    // A `def` is clamped where it is defined.
    assert!(any_read(&l, |o| is_imm(o, [1.0, -1.0, 0.5, -0.25])));
    assert!(!any_read(&l, |o| is_imm(o, [2.0, -3.0, 0.5, -0.25])));
    // A plain `c#` read is `clamp(ps_c[n], -1, 1)`, before its swizzle.
    for row in [0, 2] {
        assert!(
            instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(x)
            if x.op == FloatBinaryOp::Max
                && reads_row(&x.src0, range::FLOAT, row)
                && is_splat(&x.src1, -1.0)))
        );
    }
    assert_eq!(
        count(&l, |i| matches!(i, Instruction::FloatBinaryOp(x)
            if x.op == FloatBinaryOp::Min && is_splat(&x.src1, 1.0))),
        2
    );
    // Results keep their range: `emit.rs` clamps no result.
    assert_eq!(saturating_movs(&l), 0);
    pmetallib(&t, VariantKey::default());

    // Shader model 2 keeps the full range.
    let t = body(PS_2_0);
    let l = plowered(&t, VariantKey::default());
    assert!(any_read(&l, |o| is_imm(o, [2.0, -3.0, 0.5, -0.25])));
    assert_eq!(
        count(&l, |i| matches!(i, Instruction::FloatBinaryOp(x)
            if x.op == FloatBinaryOp::Max)),
        0
    );
}
