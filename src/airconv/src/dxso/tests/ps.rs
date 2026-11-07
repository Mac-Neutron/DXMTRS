//! D3D9 pixel shaders, shader models 2 and 3.
//!
//! The lowering's IR for each feature, a metallib from each, and the arguments each binds.

use airconv_ir::{
    binding::{FunctionInput, FunctionOutput, FunctionSignatureDesc, Interpolation, TextureKind},
    dxso::{
        lower::{self, LowerError, Lowered, range},
        parse,
        spec::{StageSpec, VariantFlags, VariantKey},
    },
    handlers::SignatureHandler,
    ir::{
        DstOperand, FloatBinaryOp, FloatComparison, FloatUnaryOp, Instruction, IntegerBinaryOp,
        SrcOperand, Target,
    },
};

use super::{
    ADDR, ALL, CONST, CONSTBOOL, CONSTINT, INPUT, PREDICATE, PREDICATED, SAMPLER, TEMP, TEXCOORD,
    XXXX, XYZW, bindings, count, d, dcl, dcl_sampler, ins, instructions, op, program, reads_range,
    reads_row, s, sources, translate,
};

const PS_2_0: u32 = 0xffff_0200;
const PS_3_0: u32 = 0xffff_0300;

const COLOROUT: u32 = 8;
const DEPTHOUT: u32 = 9;
const MISCTYPE: u32 = 17;

const NORMAL: u32 = 3;
const TANGENT: u32 = 6;
const COLOR: u32 = 10;

const TEXKILL: u16 = 65;
const DSX: u16 = 91;
const DSY: u16 = 92;
const TEXLD_PROJECT: u32 = 1 << 16;
const TEXLD_BIAS: u32 = 1 << 17;

/// The input registers the lowering reads the varyings from.
const IN_POSITION: u32 = 0;
const IN_TEXCOORD: u32 = 2;
const IN_FOG: u32 = 20;
const IN_FOG_Z: u32 = 21;
const IN_FACE: u32 = 22;
const IN_POINT_COORD: u32 = 23;
const IN_EXTRA: u32 = 24;

fn plowered(tokens: &[u32], key: VariantKey) -> Lowered {
    let program = parse(tokens).unwrap_or_else(|e| panic!("parse: {e:?}"));
    lower::pixel(&program, &key).unwrap_or_else(|e| panic!("lower: {e:?}"))
}

fn prefused(tokens: &[u32], key: VariantKey) -> LowerError {
    let program = parse(tokens).unwrap_or_else(|e| panic!("parse: {e:?}"));
    match lower::pixel(&program, &key) {
        Ok(_) => panic!("lowered"),
        Err(e) => e,
    }
}

fn pmetallib(tokens: &[u32], key: VariantKey) {
    match translate(tokens, &StageSpec::Ps(key), c"main") {
        Ok(bytes) => assert_eq!(bytes.get(..4), Some(&b"MTLB"[..]), "a metallib"),
        Err(e) => panic!("translate: {}", String::from_utf8_lossy(e.text())),
    }
}

/// The arguments `bindings::pixel` declares: buffer slots, then texture slots with their kind.
fn arguments(tokens: &[u32], key: VariantKey) -> (Vec<u32>, Vec<(u32, TextureKind)>) {
    let program = parse(tokens).unwrap_or_else(|e| panic!("{e:?}"));
    let mut desc = FunctionSignatureDesc::default();
    let _ = bindings::pixel(&lower::ps_bindings(&program, &key), &mut desc);
    let mut buffers = Vec::new();
    let mut textures = Vec::new();
    for input in desc.inputs() {
        match input {
            FunctionInput::Buffer(b) => buffers.push(b.location_index),
            FunctionInput::Texture(t) => textures.push((t.location_index, t.ty.kind)),
            _ => {}
        }
    }
    (buffers, textures)
}

fn key(f: impl FnOnce(&mut VariantKey)) -> VariantKey {
    let mut k = VariantKey::default();
    f(&mut k);
    k
}

fn flags(f: VariantFlags) -> VariantKey {
    key(|k| k.flags = f)
}

/// Whether `op` reads input register `reg`.
fn reads_input(op: &SrcOperand, reg: u32) -> bool {
    matches!(op, SrcOperand::Input { regid, .. } if *regid == reg)
}

/// Whether `op` reads input register `reg`'s lane `c` in every lane.
fn reads_input_lane(op: &SrcOperand, reg: u32, c: u8) -> bool {
    matches!(op, SrcOperand::Input { regid, common } if *regid == reg && common.swizzle.0 == [c; 4])
}

fn any_read(l: &Lowered, f: impl Fn(&SrcOperand) -> bool) -> bool {
    instructions(l).flat_map(sources).any(f)
}

/// The fragment inputs the signature declares, by `user` name, with their interpolation.
fn stage_in(l: &Lowered) -> Vec<(String, Interpolation)> {
    l.io.func_signature
        .inputs()
        .iter()
        .filter_map(|i| match i {
            FunctionInput::FragmentStageIn {
                user,
                interpolation,
                ..
            } => Some((user.clone(), interpolation.clone())),
            _ => None,
        })
        .collect()
}

fn declares(l: &Lowered, f: impl Fn(&FunctionInput) -> bool) -> bool {
    l.io.func_signature.inputs().iter().any(f)
}

fn outputs(l: &Lowered) -> &[FunctionOutput] {
    l.io.func_signature.outputs()
}

fn render_targets(l: &Lowered) -> Vec<u32> {
    outputs(l)
        .iter()
        .filter_map(|o| match o {
            FunctionOutput::RenderTarget { index, .. } => Some(*index),
            _ => None,
        })
        .collect()
}

/// A temporary or output destination's lanes, 0 for any other.
fn mask(dst: &DstOperand) -> u32 {
    match dst {
        DstOperand::Temp { common, .. } | DstOperand::Output { common, .. } => common.mask,
        _ => 0,
    }
}

fn discards(l: &Lowered) -> usize {
    count(l, |i| matches!(i, Instruction::PixelDiscard))
}

fn mov(dst: u32, src: u32) -> Vec<u32> {
    vec![ins(op::MOV, 2), dst, src]
}

fn texld(dst: u32, coord: u32, slot: u32) -> Vec<u32> {
    vec![ins(op::TEXLD, 3), dst, coord, s(SAMPLER, slot, XYZW)]
}

/// `ps_3_0`: `dcl_texcoord0 v0; dcl_2d s0; texld r0, v0, s0; mov oC0, r0`.
fn sampled() -> Vec<u32> {
    program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            dcl_sampler(0, 2),
            texld(d(TEMP, 0, ALL), s(INPUT, 0, XYZW), 0),
            mov(d(COLOROUT, 0, ALL), s(TEMP, 0, XYZW)),
        ],
    )
}

#[test]
fn texld_2d_cube_volume_by_declaration() {
    let t = program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            dcl_sampler(0, 2),
            dcl_sampler(1, 3),
            dcl_sampler(2, 4),
            texld(d(TEMP, 0, ALL), s(INPUT, 0, XYZW), 0),
            texld(d(TEMP, 1, ALL), s(INPUT, 0, XYZW), 1),
            texld(d(TEMP, 2, ALL), s(INPUT, 0, XYZW), 2),
            vec![
                ins(op::ADD, 3),
                d(TEMP, 0, ALL),
                s(TEMP, 0, XYZW),
                s(TEMP, 1, XYZW),
            ],
            vec![
                ins(op::ADD, 3),
                d(COLOROUT, 0, ALL),
                s(TEMP, 0, XYZW),
                s(TEMP, 2, XYZW),
            ],
        ],
    );
    // The declarations decide which slots exist; the bound textures their kinds.
    let bound = key(|k| {
        k.cube_sampler_mask = 0b010;
        k.volume_sampler_mask = 0b100;
    });
    let l = plowered(&t, bound);
    for slot in 0..3 {
        assert_eq!(
            count(&l, |i| matches!(i, Instruction::Sample(x)
                if x.src_resource.range_id == slot && x.src_sampler.range_id == slot)),
            1,
            "slot {slot}"
        );
    }
    let (_, textures) = arguments(&t, bound);
    assert_eq!(
        textures,
        [
            (0, TextureKind::Texture2d),
            (1, TextureKind::TextureCube),
            (2, TextureKind::Texture3d)
        ]
    );
    pmetallib(&t, bound);
    // A `dcl_cube` slot bound to a 2D texture samples the 2D texture.
    let (_, textures) = arguments(&t, VariantKey::default());
    assert!(textures.iter().all(|(_, k)| *k == TextureKind::Texture2d));
    pmetallib(&t, VariantKey::default());
    // A slot with no `dcl` is refused, as is a slot past s15.
    let undeclared = program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            dcl_sampler(0, 2),
            texld(d(COLOROUT, 0, ALL), s(INPUT, 0, XYZW), 3),
        ],
    );
    assert!(matches!(
        prefused(&undeclared, VariantKey::default()),
        LowerError::Unsupported { opcode: 66, .. }
    ));
    let past = program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            dcl_sampler(16, 2),
            texld(d(COLOROUT, 0, ALL), s(INPUT, 0, XYZW), 16),
        ],
    );
    assert!(matches!(
        prefused(&past, VariantKey::default()),
        LowerError::Unsupported { .. }
    ));
}

/// `texldp r0, v0, s0; texldb r1, v0, s0; texldd r2, v0, s0, v1, v1`, summed into `oC0`.
fn tex_controls() -> Vec<u32> {
    program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            dcl(TEXCOORD, 1, d(INPUT, 1, ALL)),
            dcl_sampler(0, 2),
            vec![
                ins(op::TEXLD, 3) | TEXLD_PROJECT,
                d(TEMP, 0, ALL),
                s(INPUT, 0, XYZW),
                s(SAMPLER, 0, XYZW),
            ],
            vec![
                ins(op::TEXLD, 3) | TEXLD_BIAS,
                d(TEMP, 1, ALL),
                s(INPUT, 0, XYZW),
                s(SAMPLER, 0, XYZW),
            ],
            vec![
                ins(op::TEXLDD, 5),
                d(TEMP, 2, ALL),
                s(INPUT, 0, XYZW),
                s(SAMPLER, 0, XYZW),
                s(INPUT, 1, XYZW),
                s(INPUT, 1, XYZW),
            ],
            vec![
                ins(op::ADD, 3),
                d(TEMP, 0, ALL),
                s(TEMP, 0, XYZW),
                s(TEMP, 1, XYZW),
            ],
            vec![
                ins(op::ADD, 3),
                d(COLOROUT, 0, ALL),
                s(TEMP, 0, XYZW),
                s(TEMP, 2, XYZW),
            ],
        ],
    )
}

#[test]
fn texldp_texldb_texldd() {
    let t = tex_controls();
    let l = plowered(&t, VariantKey::default());
    // `texldp`: the coordinate divided by its `.w`, then a plain sample.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
            if b.op == FloatBinaryOp::Div && reads_input(&b.src0, IN_TEXCOORD)
                && reads_input_lane(&b.src1, IN_TEXCOORD, 3)))
    );
    assert_eq!(count(&l, |i| matches!(i, Instruction::Sample(_))), 1);
    // `texldb`: the bias is the coordinate's `.w`.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::SampleBias(b)
            if reads_input_lane(&b.src_bias, IN_TEXCOORD, 3)))
    );
    // `texldd`: the gradients as given.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::SampleDerivative(g)
            if reads_input(&g.src_x_derivative, IN_TEXCOORD + 1)
                && reads_input(&g.src_y_derivative, IN_TEXCOORD + 1)))
    );
    pmetallib(&t, VariantKey::default());
}

#[test]
fn depth_compare_and_raw_depth_red_masks() {
    let t = sampled();
    // A depth slot compares against the saturated `.z`, at level 0, and broadcasts.
    let depth = key(|k| k.depth_sampler_mask = 1);
    let l = plowered(&t, depth);
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::SampleCompare(c) if c.level_zero)
        ),
        1
    );
    assert_eq!(count(&l, |i| matches!(i, Instruction::Sample(_))), 0);
    assert!(instructions(&l).any(|i| matches!(i, Instruction::Mov(m)
            if m.common.saturate && reads_input(&m.src, IN_TEXCOORD))));
    assert!(instructions(&l).any(|i| matches!(i, Instruction::SampleCompare(c)
        if matches!(&c.src_reference, SrcOperand::Temp { common, .. } if common.swizzle.0 == [2; 4]))));
    assert_eq!(arguments(&t, depth).1, [(0, TextureKind::Depth2d)]);
    pmetallib(&t, depth);
    // A readable raw depth slot samples level 0 without a comparison and broadcasts.
    let raw = key(|k| {
        k.depth_sampler_mask = 1;
        k.depth_fetch_mask = 1;
    });
    let l = plowered(&t, raw);
    assert_eq!(count(&l, |i| matches!(i, Instruction::SampleCompare(_))), 0);
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::SampleLod(s)
        if matches!(s.src_lod, SrcOperand::Immediate32 { value: [0, 0, 0, 0], .. })))
    );
    assert!(!instructions(&l).any(|i| matches!(i, Instruction::Mov(m)
        if mask(&m.dst) == 0b1110 && matches!(m.src, SrcOperand::Immediate32 { .. }))));
    pmetallib(&t, raw);
    // DF16/DF24 fill green, blue and alpha with 0, 0, 1.
    let red = key(|k| {
        k.depth_sampler_mask = 1;
        k.depth_fetch_mask = 1;
        k.raw_depth_red_mask = 1;
    });
    let l = plowered(&t, red);
    let fill = [0.0_f32, 0.0, 0.0, 1.0].map(f32::to_bits);
    assert!(instructions(&l).any(|i| matches!(i, Instruction::Mov(m)
        if mask(&m.dst) == 0b1110
            && matches!(m.src, SrcOperand::Immediate32 { value, .. } if value == fill))));
    pmetallib(&t, red);
    // Without the depth bit the slot samples colour.
    let l = plowered(&t, VariantKey::default());
    assert_eq!(count(&l, |i| matches!(i, Instruction::Sample(_))), 1);
}

/// `texldl r0, v0, s0; texldd r1, v0, s0, v1, v1; texld r2, v0, s0`, summed into `oC0`.
fn explicit_levels() -> Vec<u32> {
    program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            dcl(TEXCOORD, 1, d(INPUT, 1, ALL)),
            dcl_sampler(0, 2),
            vec![
                ins(op::TEXLDL, 3),
                d(TEMP, 0, ALL),
                s(INPUT, 0, XYZW),
                s(SAMPLER, 0, XYZW),
            ],
            vec![
                ins(op::TEXLDD, 5),
                d(TEMP, 1, ALL),
                s(INPUT, 0, XYZW),
                s(SAMPLER, 0, XYZW),
                s(INPUT, 1, XYZW),
                s(INPUT, 1, XYZW),
            ],
            texld(d(TEMP, 2, ALL), s(INPUT, 0, XYZW), 0),
            vec![
                ins(op::ADD, 3),
                d(TEMP, 0, ALL),
                s(TEMP, 0, XYZW),
                s(TEMP, 1, XYZW),
            ],
            vec![
                ins(op::ADD, 3),
                d(COLOROUT, 0, ALL),
                s(TEMP, 0, XYZW),
                s(TEMP, 2, XYZW),
            ],
        ],
    )
}

#[test]
fn depth_compare_texldl_texldd_and_lod_bias_level() {
    let t = explicit_levels();
    let depth = key(|k| k.depth_sampler_mask = 1);
    let l = plowered(&t, depth);
    // `texldl`: the coordinate's `.w` as the level; `texldd`: the gradients; `texld`: level 0.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::SampleCompareLevel(c)
        if reads_input_lane(&c.src_lod, IN_TEXCOORD, 3)))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::SampleCompareGrad(c)
        if reads_input(&c.src_x_derivative, IN_TEXCOORD + 1)
            && reads_input(&c.src_y_derivative, IN_TEXCOORD + 1)))
    );
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::SampleCompare(c) if c.level_zero)
        ),
        1
    );
    pmetallib(&t, depth);
    // Under the LOD table: `texldl` at `max(w + row.z, row.w)`, `texld` pinned at
    // `max(row.w, 0)`, the gradients scaled by `row.y`.
    let biased = key(|k| {
        k.depth_sampler_mask = 1;
        k.flags = VariantFlags::LOD_BIAS;
    });
    let l = plowered(&t, biased);
    assert_eq!(
        count(&l, |i| matches!(i, Instruction::SampleCompareLevel(_))),
        2
    );
    assert_eq!(count(&l, |i| matches!(i, Instruction::SampleCompare(_))), 0);
    let row = |op: &SrcOperand, c: u8| {
        reads_row(op, range::LOD_BIAS, 0)
            && matches!(op, SrcOperand::ConstantBuffer { common, .. } if common.swizzle.0 == [c; 4])
    };
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Max && row(&b.src0, 3)
            && matches!(b.src1, SrcOperand::Immediate32 { value: [0, 0, 0, 0], .. })))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Max && row(&b.src1, 3)))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Add && row(&b.src1, 2)))
    );
    assert_eq!(
        count(&l, |i| matches!(i, Instruction::FloatBinaryOp(b)
            if b.op == FloatBinaryOp::Mul && row(&b.src1, 1))),
        2
    );
    pmetallib(&t, biased);
    // A gradient against a depth slot bound as a cube does not compile in the MSL.
    let cube = key(|k| {
        k.depth_sampler_mask = 1;
        k.cube_sampler_mask = 1;
    });
    assert!(matches!(
        prefused(&t, cube),
        LowerError::Unsupported { opcode: 93, .. }
    ));
}

#[test]
fn fetch4_gather() {
    let t = sampled();
    let gathers = |l: &Lowered| -> Vec<u8> {
        instructions(l)
            .filter_map(|i| match i {
                Instruction::Gather(g) => Some(g.src_sampler.gather_channel),
                _ => None,
            })
            .collect()
    };
    let fetch4 = key(|k| k.fetch4_mask = 1);
    let l = plowered(&t, fetch4);
    // The size for the half-texel shift (no null-handle case, unlike `resinfo`), one gather of
    // red, no filtered sample.
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::TextureSize(s) if s.src_resource.range_id == 0)
        ),
        1
    );
    assert_eq!(count(&l, |i| matches!(i, Instruction::ResourceInfo(_))), 0);
    assert!(instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Div
            && matches!(b.src0, SrcOperand::Immediate32 { value, .. } if value == [0.5_f32.to_bits(); 4]))));
    assert_eq!(gathers(&l), [0]);
    assert_eq!(count(&l, |i| matches!(i, Instruction::Sample(_))), 0);
    pmetallib(&t, fetch4);
    // A8 gathers alpha.
    let alpha = key(|k| {
        k.fetch4_mask = 1;
        k.fetch4_alpha_mask = 1;
    });
    assert_eq!(gathers(&plowered(&t, alpha)), [3]);
    pmetallib(&t, alpha);
    // A depth slot gathers its depths, no comparison, the alpha bit ignored.
    let depth = key(|k| {
        k.fetch4_mask = 1;
        k.fetch4_alpha_mask = 1;
        k.depth_sampler_mask = 1;
    });
    let l = plowered(&t, depth);
    assert_eq!(gathers(&l), [0]);
    assert_eq!(count(&l, |i| matches!(i, Instruction::SampleCompare(_))), 0);
    pmetallib(&t, depth);
    // The MSL's float2 gather does not compile on a cube, and a volume has none.
    for k in [
        key(|k| {
            k.fetch4_mask = 1;
            k.cube_sampler_mask = 1;
        }),
        key(|k| {
            k.fetch4_mask = 1;
            k.volume_sampler_mask = 1;
        }),
    ] {
        assert!(matches!(
            prefused(&t, k),
            LowerError::Unsupported { opcode: 66, .. }
        ));
    }
    // Another slot's fetch4 bit leaves this one sampling.
    let other = key(|k| k.fetch4_mask = 0b10);
    assert_eq!(
        count(&plowered(&t, other), |i| matches!(
            i,
            Instruction::Sample(_)
        )),
        1
    );
}

/// `texld r0, v0, s3; texldb r1, v0, s3; texldl r2, v0, s3`, summed into `oC0`.
fn slot3() -> Vec<u32> {
    program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            dcl_sampler(3, 2),
            texld(d(TEMP, 0, ALL), s(INPUT, 0, XYZW), 3),
            vec![
                ins(op::TEXLD, 3) | TEXLD_BIAS,
                d(TEMP, 1, ALL),
                s(INPUT, 0, XYZW),
                s(SAMPLER, 3, XYZW),
            ],
            vec![
                ins(op::TEXLDL, 3),
                d(TEMP, 2, ALL),
                s(INPUT, 0, XYZW),
                s(SAMPLER, 3, XYZW),
            ],
            vec![
                ins(op::ADD, 3),
                d(TEMP, 0, ALL),
                s(TEMP, 0, XYZW),
                s(TEMP, 1, XYZW),
            ],
            vec![
                ins(op::ADD, 3),
                d(COLOROUT, 0, ALL),
                s(TEMP, 0, XYZW),
                s(TEMP, 2, XYZW),
            ],
        ],
    )
}

/// Whether `op` reads lane `c` of the LOD table's row `row`.
fn lod_lane(op: &SrcOperand, row: u32, c: u8) -> bool {
    reads_row(op, range::LOD_BIAS, row)
        && matches!(op, SrcOperand::ConstantBuffer { common, .. } if common.swizzle.0 == [c; 4])
}

#[test]
fn lod_bias_row() {
    let t = slot3();
    let biased = flags(VariantFlags::LOD_BIAS);
    let l = plowered(&t, biased);
    // `texld`: `bias(lod_bias[3].x)`.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::SampleBias(b)
        if lod_lane(&b.src_bias, 3, 0)))
    );
    // `texldb`: `bias(w + lod_bias[3].x)`.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Add && reads_input_lane(&b.src0, IN_TEXCOORD, 3)
            && lod_lane(&b.src1, 3, 0)))
    );
    // `texldl`: `level(max(w + lod_bias[3].z, lod_bias[3].w))`.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Add && lod_lane(&b.src1, 3, 2)))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Max && lod_lane(&b.src1, 3, 3)))
    );
    assert_eq!(count(&l, |i| matches!(i, Instruction::Sample(_))), 0);
    assert_eq!(arguments(&t, biased).0, [15, 9]);
    pmetallib(&t, biased);
}

#[test]
fn no_lod_bias_argument_without_the_flag() {
    let t = slot3();
    let l = plowered(&t, VariantKey::default());
    // No row of the table is read: the implicit sample carries no bias of its own (the
    // sampler's is a constant 0), `texldb` the coordinate's `.w`, `texldl` its level.
    assert!(!any_read(&l, |o| reads_range(o, range::LOD_BIAS)));
    assert_eq!(count(&l, |i| matches!(i, Instruction::Sample(_))), 1);
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::SampleBias(b)
        if reads_input_lane(&b.src_bias, IN_TEXCOORD, 3)))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::SampleLod(s)
        if reads_input_lane(&s.src_lod, IN_TEXCOORD, 3)))
    );
    assert_eq!(arguments(&t, VariantKey::default()).0, [15]);
    pmetallib(&t, VariantKey::default());
}

/// `ps_3_0` reading `vPos` into `oC0`.
fn vpos() -> Vec<u32> {
    program(
        PS_3_0,
        &[
            dcl(0, 0, d(MISCTYPE, 0, 0b0011)),
            mov(d(COLOROUT, 0, ALL), s(MISCTYPE, 0, XYZW)),
        ],
    )
}

/// `ps_3_0`: `rep i0` (dynamic) around `add r0, r0, c0`, then `if b0` (dynamic).
fn dynamic_constants(int: bool, bool_: bool) -> Vec<u32> {
    let mut body = vec![dcl(TEXCOORD, 0, d(INPUT, 0, ALL))];
    if int {
        body.push(vec![ins(op::REP, 1), s(CONSTINT, 0, XYZW)]);
        body.push(vec![
            ins(op::ADD, 3),
            d(TEMP, 0, ALL),
            s(TEMP, 0, XYZW),
            s(CONST, 0, XYZW),
        ]);
        body.push(vec![ins(op::ENDREP, 0)]);
    }
    if bool_ {
        body.push(vec![ins(op::IF, 1), s(CONSTBOOL, 0, XYZW)]);
        body.push(mov(d(TEMP, 0, ALL), s(INPUT, 0, XYZW)));
        body.push(vec![ins(op::ENDIF, 0)]);
    }
    body.push(mov(d(COLOROUT, 0, ALL), s(TEMP, 0, XYZW)));
    program(PS_3_0, &body)
}

#[test]
fn ps_buffers_only_under_their_condition() {
    let t = sampled();
    let slots = |t: &[u32], k: VariantKey| arguments(t, k).0;
    // `ps_c` always, nothing else for a plain shader.
    assert_eq!(slots(&t, VariantKey::default()), [15]);
    // The alpha reference with an active alpha test only.
    for func in [0, 8] {
        assert_eq!(slots(&t, key(|k| k.alpha_func = func)), [15]);
    }
    for func in 1..8 {
        assert_eq!(slots(&t, key(|k| k.alpha_func = func)), [15, 14]);
    }
    // Fog with vertex or table fog.
    assert_eq!(slots(&t, key(|k| k.fog_mode = 1)), [15, 13]);
    assert_eq!(slots(&t, key(|k| k.fog_table_mode = 3)), [15, 13]);
    // The LOD table under its flag.
    assert_eq!(slots(&t, flags(VariantFlags::LOD_BIAS)), [15, 9]);
    // `PsDraw` for a declared `vPos` under the scale only.
    assert_eq!(slots(&vpos(), flags(VariantFlags::VPOS_SCALE)), [15, 8]);
    assert_eq!(slots(&vpos(), VariantKey::default()), [15]);
    assert_eq!(slots(&t, flags(VariantFlags::VPOS_SCALE)), [15]);
    // Dynamic `i#` and `b#`.
    assert_eq!(
        slots(&dynamic_constants(true, false), VariantKey::default()),
        [15, 11]
    );
    assert_eq!(
        slots(&dynamic_constants(false, true), VariantKey::default()),
        [15, 10]
    );
    // All of them, in `emit_ps_function`'s order.
    let all = key(|k| {
        k.alpha_func = 5;
        k.fog_mode = 1;
        k.flags = VariantFlags::LOD_BIAS | VariantFlags::VPOS_SCALE;
    });
    let mut both = vpos();
    both.pop();
    both.extend(dynamic_constants(true, true).into_iter().skip(1));
    assert_eq!(slots(&both, all), [15, 14, 13, 9, 8, 11, 10]);
    pmetallib(&both, all);
    pmetallib(&dynamic_constants(true, true), VariantKey::default());
}

#[test]
fn alpha_test_each_cmp_func() {
    let t = sampled();
    let alpha = |op: &SrcOperand| reads_range(op, range::ALPHA_REF);
    for func in 0..=9 {
        let k = key(|k| k.alpha_func = func);
        let l = plowered(&t, k);
        let compares: Vec<(FloatComparison, bool)> = instructions(&l)
            .filter_map(|i| match i {
                Instruction::FloatCompare(c) if alpha(&c.src0) || alpha(&c.src1) => {
                    Some((c.cmp.clone(), alpha(&c.src0)))
                }
                _ => None,
            })
            .collect();
        let expected = match func {
            2 => Some((FloatComparison::LessThan, false)),
            3 => Some((FloatComparison::Equal, false)),
            4 => Some((FloatComparison::GreaterEqual, true)),
            5 => Some((FloatComparison::LessThan, true)),
            6 => Some((FloatComparison::NotEqual, false)),
            7 => Some((FloatComparison::GreaterEqual, false)),
            _ => None,
        };
        assert_eq!(
            compares,
            expected.clone().into_iter().collect::<Vec<_>>(),
            "func {func}"
        );
        // NEVER discards unconditionally, the comparisons when they fail; ALWAYS, none
        // and an unknown function never.
        let want = usize::from(matches!(func, 1..=7));
        assert_eq!(discards(&l), want, "func {func}");
        let branches = l
            .cfg
            .blocks
            .iter()
            .filter(|b| matches!(&b.target, Target::ConditionalBranch { cond, .. } if !cond.test_nonzero))
            .count();
        assert_eq!(branches, usize::from(matches!(func, 2..=7)), "func {func}");
        // The compared alpha is `oC0.a`.
        if expected.is_some() {
            assert!(
                instructions(&l).any(|i| matches!(i, Instruction::FloatCompare(c)
                if matches!(if alpha(&c.src0) { &c.src1 } else { &c.src0 },
                    SrcOperand::Temp { common, .. } if common.swizzle.0 == [3; 4])))
            );
        }
        pmetallib(&t, k);
    }
}

#[test]
fn table_fog_linear_exp_exp2_z_and_w() {
    let t = sampled();
    let fog = |op: &SrcOperand, row: u32| reads_row(op, range::FOG, row);
    for mode in 1..=3 {
        for w in [false, true] {
            let k = key(|k| {
                k.fog_table_mode = mode;
                if w {
                    k.flags = VariantFlags::FOG_SOURCE_W;
                }
            });
            let l = plowered(&t, k);
            if w {
                // `1 / in.position.w`.
                assert!(
                    instructions(&l).any(|i| matches!(i, Instruction::FloatUnaryOp(u)
                    if u.op == FloatUnaryOp::Rcp && reads_input_lane(&u.src, IN_POSITION, 3)))
                );
            } else {
                // `in.fog_z + fog_data[1].w`.
                assert!(
                    instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
                    if b.op == FloatBinaryOp::Add && reads_input_lane(&b.src0, IN_FOG_Z, 0)
                        && fog(&b.src1, 1)))
                );
            }
            let exps = count(
                &l,
                |i| matches!(i, Instruction::FloatUnaryOp(u) if u.op == FloatUnaryOp::Exp2),
            );
            let divides = count(
                &l,
                |i| matches!(i, Instruction::FloatBinaryOp(b) if b.op == FloatBinaryOp::Div),
            );
            match mode {
                1 | 2 => assert_eq!((exps, divides), (1, 0), "mode {mode}"),
                _ => assert_eq!((exps, divides), (0, 1), "mode {mode}"),
            }
            // The blend toward `fog_data[0]`, alpha kept.
            assert!(
                instructions(&l).any(|i| matches!(i, Instruction::FloatMad(m)
                if fog(&m.src2, 0) && mask(&m.dst) == 0b0111))
            );
            pmetallib(&t, k);
        }
    }
    // Vertex fog blends by the interpolated factor.
    let k = key(|k| k.fog_mode = 1);
    let l = plowered(&t, k);
    assert!(instructions(&l).any(|i| matches!(i, Instruction::Mov(m)
        if m.common.saturate && reads_input_lane(&m.src, IN_FOG, 0))));
    assert!(!any_read(&l, |o| reads_input(o, IN_FOG_Z)));
    pmetallib(&t, k);
    // No fog, no `fog_data`.
    assert!(!any_read(&plowered(&t, VariantKey::default()), |o| {
        reads_range(o, range::FOG)
    }));
}

/// `ps_3_0` writing `oC0` and `oC1` from `v0`.
fn two_targets() -> Vec<u32> {
    program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            mov(d(COLOROUT, 0, ALL), s(INPUT, 0, XYZW)),
            mov(d(COLOROUT, 1, ALL), s(INPUT, 0, XYZW)),
        ],
    )
}

#[test]
fn srgb_write() {
    let t = two_targets();
    let srgb = key(|k| {
        k.flags = VariantFlags::SRGB_WRITE;
        k.color_out_mask = 0b11;
    });
    let l = plowered(&t, srgb);
    // Each exported target: `c <= 0.0031308` selects `12.92 c`, else `1.055 pow(c, 1/2.4) - 0.055`.
    let knee = 0.003_130_8_f32.to_bits();
    assert_eq!(
        count(&l, |i| matches!(i, Instruction::FloatCompare(c)
            if c.cmp == FloatComparison::GreaterEqual
                && matches!(c.src0, SrcOperand::Immediate32 { value: [v, ..], .. } if v == knee))),
        2
    );
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::FloatUnaryOp(u) if u.op == FloatUnaryOp::Log2)
        ),
        2
    );
    // The alpha stays linear.
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::MovConditional(m) if mask(&m.dst) == 0b0111)
        ),
        2
    );
    pmetallib(&t, srgb);
    let plain = key(|k| k.color_out_mask = 0b11);
    let l = plowered(&t, plain);
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::FloatUnaryOp(u) if u.op == FloatUnaryOp::Log2)
        ),
        0
    );
}

#[test]
fn flat_shade_colors() {
    let t = program(
        PS_3_0,
        &[
            dcl(COLOR, 0, d(INPUT, 0, ALL)),
            dcl(COLOR, 1, d(INPUT, 1, ALL)),
            dcl(TEXCOORD, 0, d(INPUT, 2, ALL)),
            vec![
                ins(op::ADD, 3),
                d(TEMP, 0, ALL),
                s(INPUT, 0, XYZW),
                s(INPUT, 1, XYZW),
            ],
            vec![
                ins(op::ADD, 3),
                d(COLOROUT, 0, ALL),
                s(TEMP, 0, XYZW),
                s(INPUT, 2, XYZW),
            ],
        ],
    );
    let interpolation = |l: &Lowered, user: &str| {
        stage_in(l)
            .into_iter()
            .find(|(u, _)| u == user)
            .map(|(_, i)| i)
    };
    let flat = flags(VariantFlags::FLAT_SHADE);
    let l = plowered(&t, flat);
    assert_eq!(interpolation(&l, "d3d9_160"), Some(Interpolation::Flat));
    assert_eq!(interpolation(&l, "d3d9_161"), Some(Interpolation::Flat));
    assert_eq!(
        interpolation(&l, "d3d9_80"),
        Some(Interpolation::CenterPerspective)
    );
    assert_eq!(
        interpolation(&l, "d3d9_fog_z"),
        Some(Interpolation::CenterNoPerspective)
    );
    pmetallib(&t, flat);
    let l = plowered(&t, VariantKey::default());
    assert_eq!(
        interpolation(&l, "d3d9_160"),
        Some(Interpolation::CenterPerspective)
    );
    // `ps_2_0`'s `dcl v0` is the diffuse colour.
    let sm2 = program(
        PS_2_0,
        &[
            dcl(0, 0, d(INPUT, 0, ALL)),
            mov(d(COLOROUT, 0, ALL), s(INPUT, 0, XYZW)),
        ],
    );
    let l = plowered(&sm2, flat);
    assert!(any_read(&l, |o| reads_input(o, 18)));
    pmetallib(&sm2, flat);
}

#[test]
fn point_sprite_texcoords() {
    // `ps_3_0`: texcoord0 and texcoord1 share v0, texcoord8 in v1.
    let t = program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, 0b0011)),
            dcl(TEXCOORD, 1, d(INPUT, 0, 0b1100)),
            dcl(TEXCOORD, 8, d(INPUT, 1, ALL)),
            vec![
                ins(op::ADD, 3),
                d(COLOROUT, 0, ALL),
                s(INPUT, 0, XYZW),
                s(INPUT, 1, XYZW),
            ],
        ],
    );
    let sprite = flags(VariantFlags::POINT_SPRITE);
    let l = plowered(&t, sprite);
    assert!(declares(&l, |i| matches!(i, FunctionInput::PointCoord)));
    // Texture coordinates 0-7 are the point coordinate; 8-15 stay the vertex's.
    assert!(any_read(&l, |o| reads_input(o, IN_POINT_COORD)));
    for set in 0..8 {
        assert!(
            !any_read(&l, |o| reads_input(o, IN_TEXCOORD + set)),
            "set {set}"
        );
    }
    assert!(any_read(&l, |o| reads_input(o, IN_TEXCOORD + 8)));
    pmetallib(&t, sprite);
    let l = plowered(&t, VariantKey::default());
    assert!(!declares(&l, |i| matches!(i, FunctionInput::PointCoord)));
    assert!(any_read(&l, |o| reads_input(o, IN_TEXCOORD)));
    assert!(any_read(&l, |o| reads_input(o, IN_TEXCOORD + 1)));
    pmetallib(&t, VariantKey::default());
    // `ps_2_0`'s `t0`.
    let sm2 = program(
        PS_2_0,
        &[
            dcl(0, 0, d(ADDR, 0, ALL)),
            mov(d(COLOROUT, 0, ALL), s(ADDR, 0, XYZW)),
        ],
    );
    let l = plowered(&sm2, sprite);
    assert!(!any_read(&l, |o| reads_input(o, IN_TEXCOORD)));
    pmetallib(&sm2, sprite);
}

#[test]
fn vpos_scale() {
    let t = vpos();
    // The position is read raw: no `1 / w` on its `.w`.
    let position_init = |l: &Lowered| {
        l.io.signature_handlers.iter().any(|h| {
            matches!(h, SignatureHandler::InitInput { reg, fix_w: false, .. } if *reg == IN_POSITION)
        })
    };
    let scaled = flags(VariantFlags::VPOS_SCALE);
    let l = plowered(&t, scaled);
    assert!(position_init(&l));
    // `float4(floor(position.xy * vpos_scale.xy), position.zw)`.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Mul && reads_input(&b.src0, IN_POSITION)
            && reads_row(&b.src1, range::PS_DRAW, 0)))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatUnaryOp(u)
        if u.op == FloatUnaryOp::RoundNegativeInf))
    );
    assert!(instructions(&l).any(|i| matches!(i, Instruction::Mov(m)
        if mask(&m.dst) == 0b1100 && reads_input(&m.src, IN_POSITION))));
    pmetallib(&t, scaled);
    // Unscaled: `position - 0.5`, every lane.
    let l = plowered(&t, VariantKey::default());
    assert!(position_init(&l));
    assert!(!any_read(&l, |o| reads_range(o, range::PS_DRAW)));
    let half = (-0.5_f32).to_bits();
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Add && reads_input(&b.src0, IN_POSITION)
            && matches!(b.src1, SrcOperand::Immediate32 { value, .. } if value == [half; 4])))
    );
    pmetallib(&t, VariantKey::default());
}

#[test]
fn vface() {
    let t = program(
        PS_3_0,
        &[
            dcl(0, 0, d(MISCTYPE, 1, 0b0001)),
            mov(d(COLOROUT, 0, ALL), s(MISCTYPE, 1, XXXX)),
        ],
    );
    let l = plowered(&t, VariantKey::default());
    assert!(declares(&l, |i| matches!(i, FunctionInput::FrontFacing)));
    // `v_face_in ? 1.0 : -1.0`.
    let (one, minus) = (1.0_f32.to_bits(), (-1.0_f32).to_bits());
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::MovConditional(m)
        if reads_input(&m.src_cond, IN_FACE)
            && matches!(m.src0, SrcOperand::Immediate32 { value: [v, ..], .. } if v == one)
            && matches!(m.src1, SrcOperand::Immediate32 { value: [v, ..], .. } if v == minus)))
    );
    pmetallib(&t, VariantKey::default());
    // Without the `dcl` the register reads zero and no front-facing input exists.
    let undeclared = program(PS_3_0, &[mov(d(COLOROUT, 0, ALL), s(MISCTYPE, 1, XXXX))]);
    let l = plowered(&undeclared, VariantKey::default());
    assert!(!declares(&l, |i| matches!(i, FunctionInput::FrontFacing)));
    pmetallib(&undeclared, VariantKey::default());
}

/// `mov r0, v0; texkill r0.<mask>; mov oC0, r0`.
fn killed(header: u32, mask: u8) -> Vec<u32> {
    let input = if header == PS_2_0 {
        dcl(0, 0, d(ADDR, 0, ALL))
    } else {
        dcl(TEXCOORD, 0, d(INPUT, 0, ALL))
    };
    let read = if header == PS_2_0 {
        s(ADDR, 0, XYZW)
    } else {
        s(INPUT, 0, XYZW)
    };
    program(
        header,
        &[
            input,
            mov(d(TEMP, 0, ALL), read),
            vec![ins(TEXKILL, 1), d(TEMP, 0, mask)],
            mov(d(COLOROUT, 0, ALL), s(TEMP, 0, XYZW)),
        ],
    )
}

#[test]
fn texkill() {
    for header in [PS_2_0, PS_3_0] {
        let t = killed(header, 0b0111);
        let l = plowered(&t, VariantKey::default());
        // `any(r0.xyz < 0)`: one comparison, the three lanes or-ed, a discard on a branch.
        assert!(
            instructions(&l).any(|i| matches!(i, Instruction::FloatCompare(c)
            if c.cmp == FloatComparison::LessThan))
        );
        assert_eq!(
            count(
                &l,
                |i| matches!(i, Instruction::IntegerBinaryOp(b) if b.op == IntegerBinaryOp::Or)
            ),
            2
        );
        assert_eq!(discards(&l), 1);
        let discard_block = l
            .cfg
            .blocks
            .iter()
            .position(|b| b.instructions.contains(&Instruction::PixelDiscard));
        assert!(l.cfg.blocks.iter().any(|b| matches!(&b.target,
            Target::ConditionalBranch { cond, true_branch, .. }
                if cond.test_nonzero && Some(true_branch.index()) == discard_block)));
        pmetallib(&t, VariantKey::default());
    }
    // No lane, no test.
    let none = killed(PS_3_0, 0);
    assert_eq!(discards(&plowered(&none, VariantKey::default())), 0);
}

#[test]
fn predicated_texld_is_a_select() {
    let t = program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            dcl_sampler(0, 2),
            vec![
                ins(op::SETP, 3) | (1 << 16),
                d(PREDICATE, 0, ALL),
                s(INPUT, 0, XYZW),
                s(CONST, 0, XYZW),
            ],
            vec![
                ins(op::TEXLD, 4) | PREDICATED,
                d(TEMP, 0, ALL),
                s(PREDICATE, 0, XYZW),
                s(INPUT, 0, XYZW),
                s(SAMPLER, 0, XYZW),
            ],
            mov(d(COLOROUT, 0, ALL), s(TEMP, 0, XYZW)),
        ],
    );
    let l = plowered(&t, VariantKey::default());
    // One block: the sample runs unconditionally, the write selects on p0 (temporary 33).
    assert!(
        l.cfg
            .blocks
            .iter()
            .all(|b| !matches!(b.target, Target::ConditionalBranch { .. }))
    );
    let block = l
        .cfg
        .blocks
        .iter()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Sample(_)))
        })
        .unwrap_or_else(|| panic!("no sample"));
    let sample = block
        .instructions
        .iter()
        .position(|i| matches!(i, Instruction::Sample(_)));
    let select = block.instructions.iter().position(|i| {
        matches!(i, Instruction::MovConditional(m)
            if matches!(m.src_cond, SrcOperand::Temp { regid: 33, .. }))
    });
    assert!(sample.is_some() && select.is_some() && sample < select);
    pmetallib(&t, VariantKey::default());
}

#[test]
fn odepth() {
    let t = program(
        PS_2_0,
        &[
            mov(d(COLOROUT, 0, ALL), s(CONST, 0, XYZW)),
            mov(d(DEPTHOUT, 0, ALL), s(CONST, 1, XXXX)),
        ],
    );
    let depth = |l: &Lowered| {
        outputs(l)
            .iter()
            .any(|o| matches!(o, FunctionOutput::Depth { .. }))
    };
    let l = plowered(&t, VariantKey::default());
    assert!(depth(&l));
    assert!(
        l.io.signature_handlers
            .iter()
            .any(|h| matches!(h, SignatureHandler::PopOutput { mask: 0b1, .. }))
    );
    pmetallib(&t, VariantKey::default());
    // Computed but not exported without a depth attachment.
    let none = flags(VariantFlags::NO_DEPTH_ATTACHMENT);
    assert!(!depth(&plowered(&t, none)));
    pmetallib(&t, none);
    // A shader without `oDepth` exports none.
    assert!(!depth(&plowered(&sampled(), VariantKey::default())));
    // `oDepth` is no source.
    let read = program(
        PS_2_0,
        &[
            mov(d(DEPTHOUT, 0, ALL), s(CONST, 1, XXXX)),
            mov(d(COLOROUT, 0, ALL), s(DEPTHOUT, 0, XYZW)),
        ],
    );
    assert!(matches!(
        prefused(&read, VariantKey::default()),
        LowerError::Unsupported { opcode: 1, .. }
    ));
}

#[test]
fn mrt_color_out_mask() {
    let t = program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            mov(d(COLOROUT, 0, ALL), s(INPUT, 0, XYZW)),
            mov(d(COLOROUT, 1, ALL), s(INPUT, 0, XYZW)),
            mov(d(COLOROUT, 2, ALL), s(INPUT, 0, XYZW)),
        ],
    );
    // Exported: written and attached, target 0 always.
    for (mask, targets) in [
        (0b0000_u8, vec![0]),
        (0b0011, vec![0, 1]),
        (0b0111, vec![0, 1, 2]),
        (0b1101, vec![0, 2]),
    ] {
        let k = key(|k| k.color_out_mask = mask);
        assert_eq!(render_targets(&plowered(&t, k)), targets, "mask {mask:#b}");
        pmetallib(&t, k);
    }
    // `oC3` is read back only when written.
    let read = program(PS_3_0, &[mov(d(COLOROUT, 0, ALL), s(COLOROUT, 3, XYZW))]);
    assert!(matches!(
        prefused(&read, VariantKey::default()),
        LowerError::Unsupported { opcode: 1, .. }
    ));
}

#[test]
fn sample_mask_output() {
    let t = sampled();
    let masked = key(|k| {
        k.flags = VariantFlags::SAMPLE_MASK;
        k.sample_mask = 5;
    });
    let l = plowered(&t, masked);
    assert!(
        outputs(&l)
            .iter()
            .any(|o| matches!(o, FunctionOutput::CoverageMask))
    );
    assert!(instructions(&l).any(|i| matches!(i, Instruction::Mov(m)
        if matches!(m.src, SrcOperand::Immediate32 { value: [5, ..], .. }))));
    pmetallib(&t, masked);
    let l = plowered(&t, key(|k| k.sample_mask = 5));
    assert!(
        !outputs(&l)
            .iter()
            .any(|o| matches!(o, FunctionOutput::CoverageMask))
    );
}

#[test]
fn dsx_dsy() {
    let t = program(
        PS_3_0,
        &[
            dcl(TEXCOORD, 0, d(INPUT, 0, ALL)),
            vec![ins(DSX, 2), d(TEMP, 0, ALL), s(INPUT, 0, XYZW)],
            vec![ins(DSY, 2), d(TEMP, 1, ALL), s(INPUT, 0, XYZW)],
            vec![
                ins(op::ADD, 3),
                d(COLOROUT, 0, ALL),
                s(TEMP, 0, XYZW),
                s(TEMP, 1, XYZW),
            ],
        ],
    );
    let l = plowered(&t, VariantKey::default());
    for ddy in [false, true] {
        assert_eq!(
            count(&l, |i| matches!(i, Instruction::PartialDerivative(p)
                if p.ddy == ddy && reads_input(&p.src, IN_TEXCOORD))),
            1,
            "ddy {ddy}"
        );
    }
    pmetallib(&t, VariantKey::default());
}

#[test]
fn linked_extras_only_when_vs_writes_them() {
    let t = program(
        PS_3_0,
        &[
            dcl(NORMAL, 0, d(INPUT, 0, ALL)),
            dcl(TANGENT, 0, d(INPUT, 1, ALL)),
            vec![
                ins(op::ADD, 3),
                d(COLOROUT, 0, ALL),
                s(INPUT, 0, XYZW),
                s(INPUT, 1, XYZW),
            ],
        ],
    );
    let users = |l: &Lowered| -> Vec<String> { stage_in(l).into_iter().map(|(u, _)| u).collect() };
    // Only NORMAL0 linked: its member exists, TANGENT0 reads zero.
    let normal = key(|k| k.linked_input_mask = 0b01);
    let l = plowered(&t, normal);
    let names = users(&l);
    assert!(names.iter().any(|u| u == "d3d9_48"));
    assert!(!names.iter().any(|u| u == "d3d9_96"));
    assert!(any_read(&l, |o| reads_input(o, IN_EXTRA)));
    assert!(!any_read(&l, |o| reads_input(o, IN_EXTRA + 1)));
    pmetallib(&t, normal);
    // Both linked, in declaration order.
    let both = key(|k| k.linked_input_mask = 0b11);
    let l = plowered(&t, both);
    let names = users(&l);
    let extras: Vec<&String> = names
        .iter()
        .filter(|u| *u == "d3d9_48" || *u == "d3d9_96")
        .collect();
    assert_eq!(extras, ["d3d9_48", "d3d9_96"]);
    assert!(any_read(&l, |o| reads_input(o, IN_EXTRA + 1)));
    pmetallib(&t, both);
    // None linked: no extra member at all.
    let l = plowered(&t, VariantKey::default());
    assert!(!users(&l).iter().any(|u| u == "d3d9_48" || u == "d3d9_96"));
    pmetallib(&t, VariantKey::default());
}

mod ps1;
