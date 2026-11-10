//! DXSO tokens from the IR: the inverse of [`crate::dxso::parse`].
//!
//! For a program `P` the parser produced, `parse(&write(&P))` equals `P` in every public field
//! (`bytecode` aside, which is the stream itself). For a stream `T` already in the canonical form
//! below, `write(&parse(T)?) == T`. The writer reads the fields, never `bytecode`: a builder that
//! cannot construct a [`DxsoProgram`] (its `bytecode` field is private to `dxso`) passes the parts
//! to [`write_parts`] instead, and gets the tokens to parse.
//!
//! Building a program: every IR type is plain public data, so a builder writes struct literals;
//! [`src`], [`dst`] and [`instruction`] give the common default (`.xyzw`, no modifier, direct; no
//! flags) and struct update syntax the rest. Literals go in [`ProgramParts::def_constants`].
//!
//! Canonical form: the version token; the declarations; the `def`, `defi` and `defb` constants,
//! in that order; the main instructions; when there are subroutines, a `ret` closing main and
//! then, per label in ascending order, `label lN`, its body and `ret`; the end token. Every
//! operand token has bit 31 set, as a shader compiler writes it (SM1's operand count reads it).
//! SM2 and later carry the operand-token count in bits 24-27 of every instruction token; SM1
//! carries none, and its relative source (`c[a0.x + N]`) is the bit alone, with no address token
//! (the parser reads it as `a0.x`, whatever `rel_addr` names). A sampler `dcl` and the `def`
//! family write mask `.xyzw`, which the parser does not keep. `label` and a predicate are written
//! in SM2 form in any model.
//!
//! What the stream cannot hold is the caller's to avoid (debug builds assert it); the writer keeps
//! only the bits each field has: a register or label index is 11 bits, a `dcl` usage index 4,
//! `shift_scale` -8..=7, an SM2 instruction 15 operand tokens (SM1 has no count to overflow).
//! The instruction lists hold no `dcl`, `def`, `defi`, `defb`, `label`, `ret`, `phase`, comment
//! or end token: those are the writer's (or, in a stream, dropped by the parser). `RegKind` names
//! the parser's register types: a `vs_3_0` output `oN` in a compiler's stream is token type 6,
//! [`RegKind::TexcoordOut`], and type 11 is [`RegKind::Output`] (the lowering takes either as a
//! declared output). `predicate` is written whenever it is `Some` and bit 28 only from
//! `PREDICATED`: without the flag the parser reads the predicate as the first source.
//! `TEX_PROJECTED` and `TEX_BIASED` hold only on an SM2+ `texld`, and `cmp_func` only on `ifc`,
//! `breakc` and `setp`; elsewhere the parser drops them.

use std::collections::BTreeMap;

use crate::dxso::{
    ir::{
        Declaration, DefBoolConstant, DefConstant, DefIntConstant, DstMods, DstOperand,
        DxsoProgram, InstrFlags, Instruction, RegKind, Register, ShaderType, SrcModifier,
        SrcOperand, Swizzle, TextureType, WriteMask,
    },
    opcode::Opcode,
};

/// Bit 31, set on every operand token.
const OPERAND: u32 = 0x8000_0000;
/// The register index bits of an operand token.
const INDEX: u32 = 0x7FF;
/// The relative-addressing bit of a source token.
const RELATIVE: u32 = 1 << 13;

/// The parts of a program, borrowed, as [`write_parts`] serializes them.
///
/// The fields are [`DxsoProgram`]'s, which a builder outside `dxso` cannot construct.
pub struct ProgramParts<'a> {
    pub shader_type: ShaderType,
    pub major: u8,
    pub minor: u8,
    pub declarations: &'a [Declaration],
    pub def_constants: &'a [DefConstant],
    pub def_int_constants: &'a [DefIntConstant],
    pub def_bool_constants: &'a [DefBoolConstant],
    pub instructions: &'a [Instruction],
    /// Subroutine bodies by label index (11 bits); empty for a program without `call`.
    pub subroutines: &'a BTreeMap<u32, Vec<Instruction>>,
}

/// The canonical token stream of a parsed program.
#[must_use]
pub fn write(program: &DxsoProgram) -> Vec<u32> {
    write_parts(&ProgramParts {
        shader_type: program.shader_type,
        major: program.major,
        minor: program.minor,
        declarations: &program.declarations,
        def_constants: &program.def_constants,
        def_int_constants: &program.def_int_constants,
        def_bool_constants: &program.def_bool_constants,
        instructions: &program.instructions,
        subroutines: &program.subroutines,
    })
}

/// The canonical token stream of a program given by its parts.
#[must_use]
pub fn write_parts(parts: &ProgramParts) -> Vec<u32> {
    let sm2 = parts.major >= 2;
    let header = match parts.shader_type {
        ShaderType::Vertex => 0xFFFE_0000,
        ShaderType::Pixel => 0xFFFF_0000,
    };
    let mut out = vec![header | (u32::from(parts.major) << 8) | u32::from(parts.minor)];
    // `dcl` and the `def` family have fixed operand counts; SM2+ states them anyway.
    let fixed =
        |opcode: Opcode, count: u32| opcode_value(opcode) | if sm2 { count << 24 } else { 0 };
    for decl in parts.declarations {
        out.push(fixed(Opcode::Dcl, 2));
        match *decl {
            Declaration::Semantic {
                usage,
                usage_index,
                reg,
                mask,
            } => {
                debug_assert!(usage_index <= 0xF, "a dcl usage index is 4 bits");
                out.push(OPERAND | u32::from(usage as u8) | ((usage_index & 0xF) << 16));
                out.push(dst_token(dst(reg.kind, reg.index, mask)));
            }
            Declaration::Sampler { texture_type, reg } => {
                let ty = match texture_type {
                    TextureType::Unknown => 0,
                    TextureType::Texture2D => 2,
                    TextureType::TextureCube => 3,
                    TextureType::Texture3D => 4,
                };
                out.push(OPERAND | (ty << 27));
                out.push(dst_token(dst(reg.kind, reg.index, WriteMask::ALL)));
            }
        }
    }
    for def in parts.def_constants {
        out.push(fixed(Opcode::Def, 5));
        out.push(dst_token(dst(def.reg.kind, def.reg.index, WriteMask::ALL)));
        out.extend(def.value.map(f32::to_bits));
    }
    for def in parts.def_int_constants {
        out.push(fixed(Opcode::DefI, 5));
        out.push(dst_token(dst(def.reg.kind, def.reg.index, WriteMask::ALL)));
        out.extend(def.value.map(i32::cast_unsigned));
    }
    for def in parts.def_bool_constants {
        out.push(fixed(Opcode::DefB, 2));
        out.push(dst_token(dst(def.reg.kind, def.reg.index, WriteMask::ALL)));
        out.push(u32::from(def.value));
    }
    for inst in parts.instructions {
        write_instruction(&mut out, inst, sm2);
    }
    if !parts.subroutines.is_empty() {
        let ret = opcode_value(Opcode::Ret);
        out.push(ret);
        for (&label, body) in parts.subroutines {
            debug_assert!(label <= INDEX, "a label index is 11 bits");
            // The parser reads `label`'s count in every model.
            out.push(opcode_value(Opcode::Label) | (1 << 24));
            out.push(src_token(
                RegKind::Label,
                label,
                Swizzle::IDENTITY,
                SrcModifier::None,
            ));
            for inst in body {
                write_instruction(&mut out, inst, sm2);
            }
            out.push(ret);
        }
    }
    out.push(opcode_value(Opcode::End));
    out
}

/// A source reading `kind` register `index` whole: `.xyzw`, no modifier, direct.
///
/// The other fields are set with struct update syntax:
/// `SrcOperand { modifier: SrcModifier::Neg, ..src(RegKind::Temp, 0) }`.
#[must_use]
pub const fn src(kind: RegKind, index: u16) -> SrcOperand {
    SrcOperand {
        reg: Register { kind, index },
        swizzle: Swizzle::IDENTITY,
        modifier: SrcModifier::None,
        rel_addr: None,
    }
}

/// A destination writing the `mask` lanes of `kind` register `index`, with no modifier or shift.
#[must_use]
pub const fn dst(kind: RegKind, index: u16, mask: WriteMask) -> DstOperand {
    DstOperand {
        reg: Register { kind, index },
        write_mask: mask,
        mods: DstMods::empty(),
        shift_scale: 0,
    }
}

/// An instruction with no flags, predicate or comparison.
///
/// `srcs` takes an array or a `Vec`: `instruction(Opcode::Add, Some(d), [a, b])`.
#[must_use]
pub fn instruction(
    opcode: Opcode,
    dst: Option<DstOperand>,
    srcs: impl Into<Vec<SrcOperand>>,
) -> Instruction {
    Instruction {
        opcode,
        dst,
        srcs: srcs.into(),
        flags: InstrFlags::empty(),
        predicate: None,
        cmp_func: None,
    }
}

/// The `D3DSIO_*` value of `opcode`, from [`Opcode::from_u16`]'s table.
fn opcode_value(opcode: Opcode) -> u32 {
    // The table's values are 0..=96 and 0xFFFD..=0xFFFF; a test checks every value inverts.
    (0..=96)
        .chain(0xFFFD..=0xFFFF)
        .find(|&v| Opcode::from_u16(v) == Some(opcode))
        .map_or(0, u32::from)
}

/// The `D3DSPR_*` register type the parser reads as `kind`.
const fn reg_type(kind: RegKind) -> u32 {
    match kind {
        RegKind::Temp => 0,
        RegKind::Input => 1,
        RegKind::Const => 2,
        RegKind::Addr => 3,
        RegKind::RastOut => 4,
        RegKind::AttrOut => 5,
        RegKind::TexcoordOut => 6,
        RegKind::ConstInt => 7,
        RegKind::ColorOut => 8,
        RegKind::DepthOut => 9,
        RegKind::Sampler => 10,
        RegKind::Output => 11,
        RegKind::ConstBool => 14,
        RegKind::Loop => 15,
        RegKind::TempFloat16 => 16,
        RegKind::MiscType => 17,
        RegKind::Label => 18,
        RegKind::Predicate => 19,
    }
}

/// The `D3DSPSM_*` value the parser reads as `modifier`.
const fn modifier_value(modifier: SrcModifier) -> u32 {
    match modifier {
        SrcModifier::None => 0,
        SrcModifier::Neg => 1,
        SrcModifier::Bias => 2,
        SrcModifier::BiasNeg => 3,
        SrcModifier::Sign => 4,
        SrcModifier::SignNeg => 5,
        SrcModifier::Comp => 6,
        SrcModifier::X2 => 7,
        SrcModifier::X2Neg => 8,
        SrcModifier::Dz => 9,
        SrcModifier::Dw => 10,
        SrcModifier::Abs => 11,
        SrcModifier::AbsNeg => 12,
        SrcModifier::Not => 13,
    }
}

/// An operand token's register bits: the type split over bits 28-30 and 11-12, the index.
fn reg_bits(kind: RegKind, index: u32) -> u32 {
    debug_assert!(index <= INDEX, "a register index is 11 bits");
    let ty = reg_type(kind);
    OPERAND | ((ty & 7) << 28) | ((ty >> 3) << 11) | (index & INDEX)
}

fn swizzle_bits(swizzle: Swizzle) -> u32 {
    let [x, y, z, w] = swizzle.0.map(|c| u32::from(c & 3));
    x | (y << 2) | (z << 4) | (w << 6)
}

fn dst_token(d: DstOperand) -> u32 {
    debug_assert!((-8..=7).contains(&d.shift_scale), "a shift is 4 bits");
    reg_bits(d.reg.kind, u32::from(d.reg.index))
        | (u32::from(d.write_mask.0 & 0xF) << 16)
        | (u32::from(d.mods.bits() & 0xF) << 20)
        | (u32::from(d.shift_scale.cast_unsigned() & 0xF) << 24)
}

fn src_token(kind: RegKind, index: u32, swizzle: Swizzle, modifier: SrcModifier) -> u32 {
    reg_bits(kind, index) | (swizzle_bits(swizzle) << 16) | (modifier_value(modifier) << 24)
}

/// Writes a source; an SM2-form relative read is followed by its address token.
fn write_src(out: &mut Vec<u32>, s: &SrcOperand, sm1: bool) {
    let token = src_token(s.reg.kind, u32::from(s.reg.index), s.swizzle, s.modifier);
    let Some(rel) = s.rel_addr else {
        out.push(token);
        return;
    };
    out.push(token | RELATIVE);
    if !sm1 {
        let address = src_token(
            rel.reg.kind,
            u32::from(rel.reg.index),
            rel.swizzle,
            SrcModifier::None,
        );
        out.push(address);
    }
}

fn write_instruction(out: &mut Vec<u32>, inst: &Instruction, sm2: bool) {
    let at = out.len();
    out.push(0);
    if let Some(d) = &inst.dst {
        out.push(dst_token(*d));
    }
    if let Some(p) = &inst.predicate {
        write_src(out, p, false);
    }
    for s in &inst.srcs {
        write_src(out, s, !sm2);
    }
    let mut token = opcode_value(inst.opcode);
    if sm2 {
        let count = out.len() - at - 1;
        debug_assert!(
            count <= 15,
            "an SM2 instruction holds at most 15 operand tokens"
        );
        token |= u32::try_from(count & 0xF).unwrap_or(0) << 24;
    }
    if let Some(cmp) = inst.cmp_func {
        token |= (cmp as u32) << 16;
    }
    if inst.flags.contains(InstrFlags::TEX_PROJECTED) {
        token |= 1 << 16;
    }
    if inst.flags.contains(InstrFlags::TEX_BIASED) {
        token |= 2 << 16;
    }
    if inst.flags.contains(InstrFlags::PREDICATED) {
        token |= 1 << 28;
    }
    if inst.flags.contains(InstrFlags::COISSUE) {
        token |= 1 << 30;
    }
    if let Some(slot) = out.get_mut(at) {
        *slot = token;
    }
}

#[cfg(test)]
mod tests;
