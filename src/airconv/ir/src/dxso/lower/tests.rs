//! The opcode table against the lowering: each row lowers, or is refused, as it says.

use std::{collections::BTreeMap, sync::Arc};

use super::{
    Control, LowerError, MODIFIER_ROWS, REL_ADDR_ROWS, ROWS, Support, opcode_value, pixel, vertex,
};
use crate::dxso::{
    TextureType,
    ir::{
        CmpFunc, Declaration, DefIntConstant, DstMods, DstOperand, DxsoProgram, InstrFlags,
        Instruction, RegKind, Register, RelativeAddr, ShaderType, SrcModifier, SrcOperand, Swizzle,
        WriteMask,
    },
    opcode::Opcode,
    spec::{VariantKey, VsSpec},
};

const fn reg(kind: RegKind, index: u16) -> Register {
    Register { kind, index }
}

const fn src(kind: RegKind, index: u16) -> SrcOperand {
    SrcOperand {
        reg: reg(kind, index),
        swizzle: Swizzle::IDENTITY,
        modifier: SrcModifier::None,
        rel_addr: None,
    }
}

const fn dst(kind: RegKind, index: u16, mask: u8) -> DstOperand {
    DstOperand {
        reg: reg(kind, index),
        write_mask: WriteMask(mask),
        mods: DstMods::empty(),
        shift_scale: 0,
    }
}

fn inst(opcode: Opcode, d: Option<DstOperand>, srcs: Vec<SrcOperand>) -> Instruction {
    Instruction {
        opcode,
        dst: d,
        srcs,
        flags: InstrFlags::empty(),
        predicate: None,
        cmp_func: None,
    }
}

fn bare(opcode: Opcode) -> Instruction {
    inst(opcode, None, Vec::new())
}

/// A program of `stage` running `instructions`, with sampler 0 and `i0` declared.
fn program(stage: ShaderType, instructions: Vec<Instruction>) -> DxsoProgram {
    let mut subroutines = BTreeMap::new();
    subroutines.insert(
        0,
        vec![inst(
            Opcode::Mov,
            Some(dst(RegKind::Temp, 0, 0xf)),
            vec![src(RegKind::Temp, 1)],
        )],
    );
    DxsoProgram {
        bytecode: Arc::from([]),
        shader_type: stage,
        major: 3,
        minor: 0,
        declarations: vec![Declaration::Sampler {
            texture_type: TextureType::Texture2D,
            reg: reg(RegKind::Sampler, 0),
        }],
        def_constants: Vec::new(),
        def_int_constants: vec![DefIntConstant {
            reg: reg(RegKind::ConstInt, 0),
            value: [2, 0, 1, 0],
        }],
        def_bool_constants: Vec::new(),
        instructions,
        subroutines,
    }
}

/// The smallest instruction run that exercises `opcode` under `control`.
fn sample(opcode: Opcode, control: &Control) -> Vec<Instruction> {
    let r = |i| src(RegKind::Temp, i);
    let r0 = Some(dst(RegKind::Temp, 0, 0xf));
    let mut main = match opcode {
        Opcode::Nop | Opcode::Else | Opcode::EndIf | Opcode::EndLoop | Opcode::EndRep => Vec::new(),
        Opcode::M4x4 | Opcode::M4x3 | Opcode::M3x4 | Opcode::M3x3 | Opcode::M3x2 => {
            vec![inst(opcode, r0, vec![r(1), src(RegKind::Const, 0)])]
        }
        Opcode::Call => vec![inst(opcode, None, vec![src(RegKind::Label, 0)])],
        Opcode::CallNz => vec![inst(
            opcode,
            None,
            vec![src(RegKind::Label, 0), src(RegKind::ConstBool, 0)],
        )],
        Opcode::SetP => vec![inst(
            opcode,
            Some(dst(RegKind::Predicate, 0, 0xf)),
            vec![r(1), r(2)],
        )],
        Opcode::MovA => vec![inst(opcode, Some(dst(RegKind::Addr, 0, 0x1)), vec![r(1)])],
        Opcode::If
        | Opcode::Ifc
        | Opcode::Loop
        | Opcode::Rep
        | Opcode::Break
        | Opcode::BreakC
        | Opcode::BreakP => vec![inst(opcode, None, vec![r(1), r(2), r(3)])],
        Opcode::TexLd | Opcode::TexLdL | Opcode::TexLdD => vec![inst(
            opcode,
            r0,
            vec![r(1), src(RegKind::Sampler, 0), r(2), r(3)],
        )],
        _ => vec![inst(opcode, r0, vec![r(1), r(2), r(3)])],
    };
    for i in &mut main {
        match control {
            Control::Projected => i.flags = InstrFlags::TEX_PROJECTED,
            Control::Biased => i.flags = InstrFlags::TEX_BIASED,
            Control::Coissue => i.flags = InstrFlags::COISSUE,
            Control::Cmp(func) => i.cmp_func = *func,
            Control::None => {}
        }
    }
    // Flow control inside the construct it belongs to.
    let open_loop = || {
        inst(
            Opcode::Loop,
            None,
            vec![src(RegKind::Loop, 0), src(RegKind::ConstInt, 0)],
        )
    };
    let open_if = || inst(Opcode::If, None, vec![src(RegKind::ConstBool, 0)]);
    match opcode {
        Opcode::Loop => main.push(bare(Opcode::EndLoop)),
        Opcode::EndLoop => main = vec![open_loop(), bare(Opcode::EndLoop)],
        Opcode::Rep => main.push(bare(Opcode::EndRep)),
        Opcode::EndRep => {
            main = vec![
                inst(Opcode::Rep, None, vec![src(RegKind::ConstInt, 0)]),
                bare(Opcode::EndRep),
            ];
        }
        Opcode::If | Opcode::Ifc => main.push(bare(Opcode::EndIf)),
        Opcode::Else => main = vec![open_if(), bare(Opcode::Else), bare(Opcode::EndIf)],
        Opcode::EndIf => main = vec![open_if(), bare(Opcode::EndIf)],
        Opcode::Break | Opcode::BreakC | Opcode::BreakP => {
            main.insert(0, open_loop());
            main.push(bare(Opcode::EndLoop));
        }
        _ => {}
    }
    if opcode == Opcode::Break {
        main = vec![open_loop(), bare(Opcode::Break), bare(Opcode::EndLoop)];
    }
    if opcode == Opcode::BreakP
        && let Some(i) = main.get_mut(1)
    {
        i.srcs = vec![src(RegKind::Predicate, 0)];
    }
    main
}

fn lower(stage: ShaderType, instructions: Vec<Instruction>) -> Result<(), LowerError> {
    let p = program(stage, instructions);
    match stage {
        ShaderType::Vertex => vertex(&p, &VsSpec::default()).map(drop),
        ShaderType::Pixel => pixel(&p, &VariantKey::default()).map(drop),
    }
}

/// Whether the stage did what the row says.
fn agrees(support: &Support, result: &Result<(), LowerError>, opcode: u16) -> bool {
    match (support, result) {
        (Support::Lowered, Ok(())) | (Support::Parser, _) => true,
        (Support::Unsupported(_), Err(LowerError::Unsupported { opcode: o, .. })) => {
            // A pixel shader is refused before any instruction.
            *o == opcode || *o == 0
        }
        _ => false,
    }
}

#[test]
fn every_opcode_has_a_lowering_or_is_unsupported() {
    // Every opcode `opcode.rs` decodes has a row, at its own token value.
    for v in 0..=u16::MAX {
        let Some(op) = Opcode::from_u16(v) else {
            continue;
        };
        assert_eq!(opcode_value(op), v, "{op:?}");
        assert!(ROWS.iter().any(|r| r.opcode == op), "{op:?} has no row");
    }
    // Each control of the opcodes that read one, and no pair twice.
    let cmps = [
        None,
        Some(CmpFunc::Gt),
        Some(CmpFunc::Eq),
        Some(CmpFunc::Ge),
        Some(CmpFunc::Lt),
        Some(CmpFunc::Ne),
        Some(CmpFunc::Le),
    ];
    for op in [Opcode::Ifc, Opcode::BreakC, Opcode::SetP] {
        for func in cmps {
            let n = ROWS
                .iter()
                .filter(|r| r.opcode == op && r.control == Control::Cmp(func))
                .count();
            assert_eq!(n, 1, "{op:?} {func:?}");
        }
    }
    for control in [Control::None, Control::Projected, Control::Biased] {
        assert!(
            ROWS.iter()
                .any(|r| r.opcode == Opcode::TexLd && r.control == control)
        );
    }
    assert!(
        ROWS.iter()
            .any(|r| r.opcode == Opcode::Cnd && r.control == Control::Coissue)
    );
    for (i, a) in ROWS.iter().enumerate() {
        for b in ROWS.iter().skip(i + 1) {
            assert!(
                !(a.opcode == b.opcode && a.control == b.control),
                "{:?} {:?} twice",
                a.opcode,
                a.control
            );
        }
    }

    // Each row lowers, or is refused naming its opcode, in each stage.
    for row in ROWS {
        let code = opcode_value(row.opcode);
        for (stage, support) in [(ShaderType::Vertex, &row.vs), (ShaderType::Pixel, &row.ps)] {
            if *support == Support::Parser {
                continue;
            }
            let result = lower(stage, sample(row.opcode, &row.control));
            assert!(
                agrees(support, &result, code),
                "{stage:?} {:?} {:?}: {support:?} but {result:?}",
                row.opcode,
                row.control
            );
            // A lowered instruction with a destination lowers predicated too.
            let mut predicated = sample(row.opcode, &row.control);
            let has_value = predicated.iter().any(|i| i.dst.is_some());
            if *support == Support::Lowered && has_value {
                for i in &mut predicated {
                    i.flags.insert(InstrFlags::PREDICATED);
                    i.predicate = Some(src(RegKind::Predicate, 0));
                }
                assert_eq!(
                    lower(stage, predicated),
                    Ok(()),
                    "predicated {:?}",
                    row.opcode
                );
            }
        }
    }

    // Every source modifier: `mov r0, r1<modifier>`.
    let modifiers = [
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
    for m in modifiers {
        let Some((_, vs, ps)) = MODIFIER_ROWS.iter().find(|(r, ..)| *r == m) else {
            panic!("{m:?} has no row");
        };
        let mov = || {
            let mut s = src(RegKind::Temp, 1);
            s.modifier = m;
            vec![inst(Opcode::Mov, Some(dst(RegKind::Temp, 0, 0xf)), vec![s])]
        };
        for (stage, support) in [(ShaderType::Vertex, vs), (ShaderType::Pixel, ps)] {
            let result = lower(stage, mov());
            assert!(agrees(support, &result, 1), "{stage:?} {m:?}: {result:?}");
        }
    }

    // Every register a relative address may go through: `mov r0, c[<reg> + 1]` in a loop.
    let kinds = [
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
    for kind in kinds {
        let Some((_, vs, ps)) = REL_ADDR_ROWS.iter().find(|(r, ..)| *r == kind) else {
            panic!("{kind:?} has no row");
        };
        let read = || {
            let mut s = src(RegKind::Const, 1);
            s.rel_addr = Some(RelativeAddr {
                reg: reg(kind, 0),
                swizzle: Swizzle([0; 4]),
            });
            vec![
                inst(
                    Opcode::Loop,
                    None,
                    vec![src(RegKind::Loop, 0), src(RegKind::ConstInt, 0)],
                ),
                inst(Opcode::Mov, Some(dst(RegKind::Temp, 0, 0xf)), vec![s]),
                bare(Opcode::EndLoop),
            ]
        };
        for (stage, support) in [(ShaderType::Vertex, vs), (ShaderType::Pixel, ps)] {
            let result = lower(stage, read());
            assert!(
                agrees(support, &result, 1),
                "{stage:?} {kind:?}: {result:?}"
            );
        }
    }
}
