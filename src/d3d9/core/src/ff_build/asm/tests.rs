use airconv_ir::{
    dxso::{
        DxsoProgram,
        lower::{self, LowerError, range},
        parse,
        spec::{VariantKey, VsSpec},
    },
    dxso_write::write,
};

use super::*;

fn parsed(a: &Asm) -> DxsoProgram {
    let tokens = a.finish();
    let program = parse(&tokens).expect("the assembled program parses");
    assert_eq!(write(&program), tokens, "the stream is canonical");
    program
}

fn lower_vs(a: &Asm, clip_plane_count: u8) -> Result<(), LowerError> {
    let spec = VsSpec {
        provided_input_mask: a.input_mask(),
        clip_plane_count,
        ..VsSpec::default()
    };
    lower::vertex(&parsed(a), &spec).map(|_| ())
}

fn lower_ps(a: &Asm) -> Result<(), LowerError> {
    lower::pixel(&parsed(a), &VariantKey::default()).map(|_| ())
}

/// `dcl_position v0`, `dcl_position o0`, `mov o0, v0`.
fn passthrough_vs() -> (Asm, Register, Register) {
    let mut a = Asm::new(ShaderType::Vertex, VS_DEF_BASE);
    let v0 = a.input(DeclUsage::Position, 0, 0);
    let o0 = a.output(DeclUsage::Position, 0, "xyzw");
    a.op(Opcode::Mov, dst(o0, "xyzw"), [src(v0, "xyzw")]);
    (a, v0, o0)
}

#[test]
fn registers_swizzles_and_masks() {
    let r = c(3);
    assert_eq!(src(r, "x").swizzle, Swizzle([0; 4]));
    assert_eq!(src(r, "xy").swizzle, Swizzle([0, 1, 1, 1]));
    assert_eq!(src(r, "wzyx").swizzle, Swizzle([3, 2, 1, 0]));
    assert_eq!(src(r, "rgba").swizzle, Swizzle::IDENTITY);
    assert_eq!(src(r, "xyzw").modifier, SrcModifier::None);
    assert_eq!(dst(r, "xz").write_mask, WriteMask(0b0101));
    assert_eq!(dst(r, "xyzw").write_mask, WriteMask::ALL);
    assert_eq!(vs_draw(2), c(2034));
    assert_eq!(POS_FIXUP, c(2044));
    assert_eq!(bump_env(3), reg(RegKind::Output, 3));
    assert_eq!(color_out(0), reg(RegKind::ColorOut, 0));
    assert_eq!(A0, reg(RegKind::Addr, 0));
    let (a, ..) = passthrough_vs();
    lower_vs(&a, 0).expect("a passthrough lowers");
}

#[test]
#[should_panic(expected = "in order")]
fn a_mask_out_of_order_panics() {
    let _ = dst(c(0), "yx");
}

#[test]
fn source_modifiers() {
    let s = src(c(0), "xyzw");
    assert_eq!(neg(s).modifier, SrcModifier::Neg);
    assert_eq!(neg(neg(s)).modifier, SrcModifier::None);
    assert_eq!(neg(abs(s)).modifier, SrcModifier::AbsNeg);
    assert_eq!(abs(neg(s)).modifier, SrcModifier::Abs);
    assert_eq!(comp(s).modifier, SrcModifier::Comp);
    assert_eq!(bias(s).modifier, SrcModifier::Bias);
    assert_eq!(neg(bias(s)).modifier, SrcModifier::BiasNeg);
    assert_eq!(bx2(s).modifier, SrcModifier::Sign);
    assert_eq!(x2(s).modifier, SrcModifier::X2);
    assert_eq!(dw(s).modifier, SrcModifier::Dw);
    assert_eq!(dz(s).modifier, SrcModifier::Dz);
    let r = rel(src(c(95), "xyzw"), 'y');
    assert_eq!(
        r.rel_addr,
        Some(RelativeAddr {
            reg: A0,
            swizzle: Swizzle([1; 4]),
        })
    );

    // A relative read through a0's second lane, with a `def` present (its overlay runs).
    let mut a = Asm::new(ShaderType::Vertex, VS_DEF_BASE);
    let v0 = a.input(DeclUsage::Position, 0, 0);
    let v13 = a.input(DeclUsage::BlendIndices, 0, 13);
    let o0 = a.output(DeclUsage::Position, 0, "xyzw");
    let four = a.lit(4.0);
    let t = a.temp();
    a.op(Opcode::Mul, dst(t, "xyzw"), [src(v13, "xyzw"), four]);
    a.op(Opcode::MovA, dst(A0, "xyzw"), [src(t, "xyzw")]);
    a.op(Opcode::Dp4, dst(o0, "x"), [src(v0, "xyzw"), r]);
    a.op(Opcode::Mov, dst(o0, "yzw"), [neg(abs(src(v0, "xyzw")))]);
    let program = parsed(&a);
    assert_eq!(program.instructions[2].srcs[1].rel_addr, r.rel_addr);
    lower_vs(&a, 0).expect("a relative read lowers");

    // The pixel modifiers.
    let mut a = Asm::new(ShaderType::Pixel, PS_DEF_BASE);
    let v0 = a.input(DeclUsage::Color, 0, 0);
    let v1 = a.input(DeclUsage::Color, 1, 1);
    let t = a.temp();
    a.op(Opcode::Mov, dst(t, "xyzw"), [comp(src(v0, "xyzw"))]);
    a.op(
        Opcode::Add,
        dst(t, "xyzw"),
        [bias(src(v1, "xyzw")), bx2(src(t, "xyzw"))],
    );
    a.op(
        Opcode::Mul,
        dst(t, "xyzw"),
        [x2(src(t, "xyzw")), dw(src(v1, "xyzw"))],
    );
    a.op(Opcode::Mov, dst(color_out(0), "xyzw"), [src(t, "xyzw")]);
    lower_ps(&a).expect("the pixel modifiers lower");
}

#[test]
fn destination_modifiers() {
    let d = dst(c(0), "xyzw");
    assert!(sat(d).mods.contains(DstMods::SATURATE));
    assert_eq!(shift(d, 1).shift_scale, 1);
    assert_eq!(shift(sat(d), -1).shift_scale, -1);
    assert!(shift(sat(d), 2).mods.contains(DstMods::SATURATE));

    let mut a = Asm::new(ShaderType::Pixel, PS_DEF_BASE);
    let v0 = a.input(DeclUsage::Color, 0, 0);
    let t = a.temp();
    a.op(
        Opcode::Mul,
        shift(dst(t, "xyzw"), 2),
        [src(v0, "xyzw"), src(v0, "xyzw")],
    );
    a.op(
        Opcode::Mov,
        sat(dst(color_out(0), "xyzw")),
        [src(t, "xyzw")],
    );
    let program = parsed(&a);
    assert_eq!(program.instructions[0].dst.map(|d| d.shift_scale), Some(2));
    lower_ps(&a).expect("a shifted and a saturated write lower");
}

#[test]
fn defs_pack_lanes_from_the_base() {
    let (mut a, v0, o0) = passthrough_vs();
    let one = a.lit(1.0);
    let half = a.lit(0.5);
    assert_eq!((one.reg, one.swizzle), (c(256), Swizzle([0; 4])));
    assert_eq!((half.reg, half.swizzle), (c(256), Swizzle([1; 4])));
    assert_eq!(a.lit(1.0), one, "a literal is shared by value");
    // Zero's bits are not negative zero's.
    let zero = a.lit(0.0);
    let negative_zero = a.lit(-0.0);
    assert_eq!(zero.swizzle, Swizzle([2; 4]));
    assert_eq!(negative_zero.swizzle, Swizzle([3; 4]));
    let fifth = a.lit(39.0);
    assert_eq!((fifth.reg, fifth.swizzle), (c(257), Swizzle([0; 4])));
    // A vector whose components one def holds reads that def.
    let v = a.vec4([0.5, 1.0, 0.0, 1.0]);
    assert_eq!((v.reg, v.swizzle), (c(256), Swizzle([1, 0, 2, 0])));
    let w = a.vec4([9.0, 8.0, 7.0, 6.0]);
    assert_eq!((w.reg, w.swizzle), (c(258), Swizzle::IDENTITY));
    assert_eq!(a.lit(7.0).swizzle, Swizzle([2; 4]));
    a.op(Opcode::Mad, dst(o0, "xyzw"), [src(v0, "xyzw"), one, w]);
    let program = parsed(&a);
    let defs: Vec<(u16, [u32; 4])> = program
        .def_constants
        .iter()
        .map(|d| (d.reg.index, d.value.map(f32::to_bits)))
        .collect();
    assert_eq!(
        defs,
        [
            (256, [1.0, 0.5, 0.0, -0.0].map(f32::to_bits)),
            (257, [39.0, 0.0, 0.0, 0.0].map(f32::to_bits)),
            (258, [9.0, 8.0, 7.0, 6.0].map(f32::to_bits)),
        ]
    );
    lower_vs(&a, 0).expect("defs above the rows lower");

    let mut a = Asm::new(ShaderType::Pixel, PS_DEF_BASE);
    assert_eq!(a.lit(2.0).reg, c(16));
}

#[test]
fn temps_are_a_stack_with_a_peak() {
    let (mut a, v0, o0) = passthrough_vs();
    assert_eq!(a.temp(), reg(RegKind::Temp, 0));
    assert_eq!(a.temp(), reg(RegKind::Temp, 1));
    let mark = a.mark();
    assert_eq!(a.temp(), reg(RegKind::Temp, 2));
    a.release(mark);
    assert_eq!(a.temp(), reg(RegKind::Temp, 2));
    a.release(0);
    assert_eq!(a.temp(), reg(RegKind::Temp, 0));
    assert_eq!(a.peak_temps(), 3);

    // The cap is the lowering's: r31 lowers, r32 is refused.
    let mut last = reg(RegKind::Temp, 0);
    for _ in 1..TEMP_LIMIT {
        last = a.temp();
    }
    assert_eq!(last, reg(RegKind::Temp, 31));
    a.op(Opcode::Mov, dst(last, "xyzw"), [src(v0, "xyzw")]);
    a.op(Opcode::Mov, dst(o0, "xyzw"), [src(last, "xyzw")]);
    lower_vs(&a, 0).expect("r31 lowers");
    let past = a.temp();
    assert_eq!(a.peak_temps(), TEMP_LIMIT + 1);
    a.op(Opcode::Mov, dst(past, "xyzw"), [src(v0, "xyzw")]);
    assert!(lower_vs(&a, 0).is_err(), "r32 is refused");
}

#[test]
fn outputs_skip_the_clip_basis() {
    let mut a = Asm::new(ShaderType::Vertex, VS_DEF_BASE);
    a.input(DeclUsage::Position, 0, 0);
    a.input(DeclUsage::Normal, 0, 15);
    let indices: Vec<u16> = (0..32)
        .map(|i| a.output(DeclUsage::Texcoord, i % 16, "xyzw").index)
        .collect();
    let want: Vec<u16> = (0..31).chain([32]).collect();
    assert_eq!(indices, want);
    assert_eq!(a.input_mask(), 1 | 1 << 15);

    let mut a = Asm::new(ShaderType::Vertex, VS_DEF_BASE);
    let v = a.input(DeclUsage::Position, 0, 0);
    let o0 = a.output(DeclUsage::Position, 0, "xyzw");
    let basis = a.output_at(VS_CLIP_BASIS_OUTPUT, DeclUsage::Position, 0, "xyzw");
    let fog = a.output(DeclUsage::Fog, 0, "x");
    assert_eq!(
        [o0, basis, fog],
        [
            reg(RegKind::TexcoordOut, 0),
            reg(RegKind::TexcoordOut, 31),
            reg(RegKind::TexcoordOut, 1),
        ]
    );
    a.op(Opcode::Mov, dst(o0, "xyzw"), [src(v, "xyzw")]);
    a.op(Opcode::Mov, dst(basis, "xyzw"), [src(v, "xyzw")]);
    a.op(Opcode::Mov, dst(fog, "x"), [src(v, "x")]);
    lower_vs(&a, 1).expect("a clip basis lowers");
}

#[test]
fn pixel_declarations_texld_and_bump_rows() {
    let mut a = Asm::new(ShaderType::Pixel, PS_DEF_BASE);
    let v0 = a.input(DeclUsage::Color, 0, 0);
    let t0 = a.input(DeclUsage::Texcoord, 0, 2);
    let t1 = a.input(DeclUsage::Texcoord, 1, 3);
    let s0 = a.sampler(TextureType::Texture2D, 0);
    let s1 = a.sampler(TextureType::TextureCube, 1);
    let r = a.temp();
    let b = a.temp();
    a.texld(
        dst(r, "xyzw"),
        src(t0, "xyzw"),
        s0,
        InstrFlags::TEX_PROJECTED,
    );
    a.op(Opcode::Mov, dst(b, "zw"), [src(t1, "xyzw")]);
    a.op(Opcode::Bem, dst(b, "xy"), [src(t1, "xyzw"), src(r, "xyzw")]);
    a.texld(dst(r, "xyzw"), dw(src(b, "xyzw")), s1, InstrFlags::empty());
    a.op(
        Opcode::Mul,
        dst(r, "xyzw"),
        [src(r, "xyzw"), src(bump_env(3), "x")],
    );
    a.op(
        Opcode::Mul,
        sat(dst(color_out(0), "xyzw")),
        [src(r, "xyzw"), src(v0, "xyzw")],
    );
    let program = parsed(&a);
    assert_eq!(program.instructions[0].flags, InstrFlags::TEX_PROJECTED);
    let bindings = lower::ps_bindings(&program, &VariantKey::default());
    assert!(bindings.ranges.contains(&range::BUMP_ENV));
    assert_eq!(
        bindings.samplers.keys().copied().collect::<Vec<_>>(),
        [0, 1]
    );
    lower_ps(&a).expect("texld, bem and a bump row lower");
}
