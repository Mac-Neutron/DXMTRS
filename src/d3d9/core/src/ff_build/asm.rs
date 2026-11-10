//! Typed DXSO assembly for the fixed-function builders.
//!
//! [`Asm`] collects one SM3 program (declarations, `def` literals, instructions) and serializes
//! it with `airconv_ir::dxso_write`. Registers are the IR's [`Register`]; [`src`] and [`dst`] turn
//! one into an operand from an assembly-style swizzle or mask (`"xy"` reads `.xyyy`, the last
//! component replicated), and the free functions beside them apply the source modifiers
//! (`_comp`, `_bias`, `_bx2`, `_x2`, `_dw`, negation, `_abs`, relative addressing) and the
//! destination ones (`_sat`, `_x2`/`_x4`/`_d2` as a shift).
//!
//! Allocation is the builder's, so a key always builds the same tokens:
//!
//! - **Literals** are `def`s from a base above every constant row the stage reads (`c256` in a
//!   vertex program, `c16` in a pixel one). Scalars are packed four to a register and shared by
//!   value, so few `def`s exist: every relative `c#` read compares its index against each one.
//! - **Temporaries** are a stack: [`Asm::mark`] and [`Asm::release`] free everything taken since
//!   the mark. Nothing caps them here; the lowering refuses `r32` and up ([`TEMP_LIMIT`]), and the
//!   builders' tests hold their peak under it.
//! - **Outputs** of a vertex program are taken from `o0` up, past the clip basis register
//!   `o31` ([`VS_CLIP_BASIS_OUTPUT`]), which only [`Asm::output_at`] names.

use std::collections::BTreeMap;

use airconv_ir::{
    dxso::{
        DeclUsage, Declaration, RegKind, ShaderType, TextureType,
        ir::{
            DefConstant, DstMods, DstOperand, InstrFlags, Instruction, Register, RelativeAddr,
            SrcModifier, SrcOperand, Swizzle, WriteMask,
        },
        lower::{VS_CLIP_BASIS_OUTPUT, VS_WINDOW, VS_WINDOW_POS_FIXUP},
        opcode::Opcode,
    },
    dxso_write::{self, ProgramParts},
};

/// The temporaries the lowering accepts, `r0` to `r31`.
pub const TEMP_LIMIT: u16 = 32;
/// The first `def` register of a vertex program, past the fixed-function rows `c0`-`c255`.
pub const VS_DEF_BASE: u16 = 256;
/// The first `def` register of a pixel program, past the fixed-function rows it reads.
pub const PS_DEF_BASE: u16 = 16;

/// The address register `a0`.
pub const A0: Register = reg(RegKind::Addr, 0);
/// The window register the lowering reads as `PosFixup` row 0, `(x, y, z, w)`.
pub const POS_FIXUP: Register = c(VS_WINDOW_POS_FIXUP);

/// One program being assembled.
pub struct Asm {
    shader_type: ShaderType,
    declarations: Vec<Declaration>,
    defs: Vec<DefConstant>,
    def_base: u16,
    /// Lanes of the last `def` holding a literal; the rest are free for the next scalar.
    def_lanes: usize,
    instructions: Vec<Instruction>,
    next_temp: u16,
    peak_temps: u16,
    next_output: u16,
}

impl Asm {
    /// An empty shader model 3 program of `shader_type` whose `def`s start at `c<def_base>`.
    #[must_use]
    pub const fn new(shader_type: ShaderType, def_base: u16) -> Self {
        Self {
            shader_type,
            declarations: Vec::new(),
            defs: Vec::new(),
            def_base,
            def_lanes: 0,
            instructions: Vec::new(),
            next_temp: 0,
            peak_temps: 0,
            next_output: 0,
        }
    }

    /// `dcl_<usage><usage_index> v<index>`, the whole register.
    pub fn input(&mut self, usage: DeclUsage, usage_index: u32, index: u16) -> Register {
        let r = reg(RegKind::Input, index);
        self.declarations.push(Declaration::Semantic {
            usage,
            usage_index,
            reg: r,
            mask: WriteMask::ALL,
        });
        r
    }

    /// `dcl_<type> s<slot>`.
    pub fn sampler(&mut self, texture_type: TextureType, slot: u16) -> Register {
        let r = reg(RegKind::Sampler, slot);
        self.declarations.push(Declaration::Sampler {
            texture_type,
            reg: r,
        });
        r
    }

    /// The next vertex output `o#` from `o0` up, declared `dcl_<usage><usage_index> o#.<mask>`.
    ///
    /// `o31`, the clip basis, is skipped.
    pub fn output(&mut self, usage: DeclUsage, usage_index: u32, mask: &str) -> Register {
        if self.next_output == VS_CLIP_BASIS_OUTPUT {
            self.next_output += 1;
        }
        let index = self.next_output;
        self.next_output = index.saturating_add(1);
        self.output_at(index, usage, usage_index, mask)
    }

    /// The vertex output `o<index>`, declared `dcl_<usage><usage_index> o<index>.<mask>`.
    pub fn output_at(
        &mut self,
        index: u16,
        usage: DeclUsage,
        usage_index: u32,
        mask: &str,
    ) -> Register {
        let r = reg(RegKind::TexcoordOut, index);
        self.declarations.push(Declaration::Semantic {
            usage,
            usage_index,
            reg: r,
            mask: write_mask(mask),
        });
        r
    }

    /// A fresh temporary, held until a [`Self::release`] to a mark taken before it.
    pub fn temp(&mut self) -> Register {
        let index = self.next_temp;
        self.next_temp = index.saturating_add(1);
        self.peak_temps = self.peak_temps.max(self.next_temp);
        reg(RegKind::Temp, index)
    }

    /// The temporaries held now, for [`Self::release`].
    #[must_use]
    pub const fn mark(&self) -> u16 {
        self.next_temp
    }

    /// Frees every temporary taken since `mark`.
    pub fn release(&mut self, mark: u16) {
        self.next_temp = self.next_temp.min(mark);
    }

    /// The most temporaries held at once.
    #[must_use]
    pub const fn peak_temps(&self) -> u16 {
        self.peak_temps
    }

    /// A literal: the lane of a `def` holding `value` (by bits), replicated.
    pub fn lit(&mut self, value: f32) -> SrcOperand {
        if let Some((r, lane)) = self.find(value) {
            return src_lanes(r, [lane; 4]);
        }
        if self.defs.is_empty() || self.def_lanes == 4 {
            self.push_def([0.0; 4]);
            self.def_lanes = 0;
        }
        let lane = self.def_lanes;
        if let Some(def) = self.defs.last_mut() {
            def.value[lane] = value;
        }
        self.def_lanes = lane + 1;
        let r = self.def_reg(self.defs.len() - 1);
        src_lanes(r, [u8::try_from(lane).unwrap_or(0); 4])
    }

    /// A vector literal: one `def` holding every component, swizzled.
    pub fn vec4(&mut self, value: [f32; 4]) -> SrcOperand {
        for i in 0..self.defs.len() {
            let lanes = value.map(|v| self.lane_of(i, v));
            if let [Some(x), Some(y), Some(z), Some(w)] = lanes {
                return src_lanes(self.def_reg(i), [x, y, z, w]);
            }
        }
        self.push_def(value);
        self.def_lanes = 4;
        src(self.def_reg(self.defs.len() - 1), "xyzw")
    }

    /// `opcode dst, srcs…`.
    pub fn op(&mut self, opcode: Opcode, dst: DstOperand, srcs: impl Into<Vec<SrcOperand>>) {
        self.instructions
            .push(dxso_write::instruction(opcode, Some(dst), srcs));
    }

    /// `texld dst, coord, s<n>`, `flags` selecting `texldp` or `texldb`.
    pub fn texld(
        &mut self,
        dst: DstOperand,
        coord: SrcOperand,
        sampler: Register,
        flags: InstrFlags,
    ) {
        let mut inst =
            dxso_write::instruction(Opcode::TexLd, Some(dst), [coord, src(sampler, "xyzw")]);
        inst.flags = flags;
        self.instructions.push(inst);
    }

    /// The `v#` registers below 16 the program declares, bit `i` for `vi`.
    #[must_use]
    pub fn input_mask(&self) -> u16 {
        self.declarations
            .iter()
            .filter_map(|decl| match decl {
                Declaration::Semantic { reg, .. } if reg.kind == RegKind::Input => {
                    1u16.checked_shl(u32::from(reg.index))
                }
                _ => None,
            })
            .fold(0, |mask, bit| mask | bit)
    }

    /// The program's canonical token stream.
    #[must_use]
    pub fn finish(&self) -> Vec<u32> {
        dxso_write::write_parts(&ProgramParts {
            shader_type: self.shader_type,
            major: 3,
            minor: 0,
            declarations: &self.declarations,
            def_constants: &self.defs,
            def_int_constants: &[],
            def_bool_constants: &[],
            instructions: &self.instructions,
            subroutines: &BTreeMap::new(),
        })
    }

    fn def_reg(&self, i: usize) -> Register {
        c(self
            .def_base
            .saturating_add(u16::try_from(i).unwrap_or(u16::MAX)))
    }

    fn push_def(&mut self, value: [f32; 4]) {
        let r = self.def_reg(self.defs.len());
        self.defs.push(DefConstant { reg: r, value });
    }

    /// The lane of `def` `i` already holding `value`.
    fn lane_of(&self, i: usize, value: f32) -> Option<u8> {
        let filled = if i + 1 == self.defs.len() {
            self.def_lanes
        } else {
            4
        };
        let def = self.defs.get(i)?;
        (0u8..)
            .zip(&def.value[..filled])
            .find(|(_, v)| v.to_bits() == value.to_bits())
            .map(|(lane, _)| lane)
    }

    fn find(&self, value: f32) -> Option<(Register, u8)> {
        (0..self.defs.len())
            .find_map(|i| self.lane_of(i, value).map(|lane| (self.def_reg(i), lane)))
    }
}

/// Register `kind` `index`.
#[must_use]
pub const fn reg(kind: RegKind, index: u16) -> Register {
    Register { kind, index }
}

/// The constant register `c<index>`.
#[must_use]
pub const fn c(index: u16) -> Register {
    reg(RegKind::Const, index)
}

/// The window register the lowering reads as `VsDraw` row `row`.
///
/// Row 0 is `point`, 1 `point_scale`, 2-5 `inv_view`, 6-11 `clip`.
#[must_use]
pub const fn vs_draw(row: u16) -> Register {
    c(VS_WINDOW + row)
}

/// A pixel source the lowering reads as `bump_env` row `row` (token type 11).
#[must_use]
pub const fn bump_env(row: u16) -> Register {
    reg(RegKind::Output, row)
}

/// The pixel colour output `oC<index>`.
#[must_use]
pub const fn color_out(index: u16) -> Register {
    reg(RegKind::ColorOut, index)
}

/// `r` read through `swizzle`, one to four of `xyzw` (or `rgba`), the last one replicated.
///
/// # Panics
///
/// On an empty swizzle, one longer than four, or a character outside `xyzwrgba`.
#[must_use]
pub fn src(r: Register, swizzle: &str) -> SrcOperand {
    let lanes: Vec<u8> = swizzle.chars().map(component).collect();
    assert!(
        (1..=4).contains(&lanes.len()),
        "a swizzle names one to four components: {swizzle:?}"
    );
    let last = lanes.last().copied().unwrap_or(0);
    src_lanes(
        r,
        core::array::from_fn(|i| lanes.get(i).copied().unwrap_or(last)),
    )
}

/// `r` written through `mask`, `xyzw`'s lanes in order (`"xz"`).
///
/// # Panics
///
/// On a mask that is empty, out of order, or names a character outside `xyzwrgba`.
#[must_use]
pub fn dst(r: Register, mask: &str) -> DstOperand {
    DstOperand {
        reg: r,
        write_mask: write_mask(mask),
        mods: DstMods::empty(),
        shift_scale: 0,
    }
}

/// `-s`.
///
/// # Panics
///
/// On a modifier with no negated form (`_comp`, `_dz`, `_dw`, `!`).
#[must_use]
pub fn neg(s: SrcOperand) -> SrcOperand {
    let modifier = match s.modifier {
        SrcModifier::None => SrcModifier::Neg,
        SrcModifier::Neg => SrcModifier::None,
        SrcModifier::Abs => SrcModifier::AbsNeg,
        SrcModifier::AbsNeg => SrcModifier::Abs,
        SrcModifier::Bias => SrcModifier::BiasNeg,
        SrcModifier::BiasNeg => SrcModifier::Bias,
        SrcModifier::Sign => SrcModifier::SignNeg,
        SrcModifier::SignNeg => SrcModifier::Sign,
        SrcModifier::X2 => SrcModifier::X2Neg,
        SrcModifier::X2Neg => SrcModifier::X2,
        other => panic!("no negated form of {other:?}"),
    };
    SrcOperand { modifier, ..s }
}

/// `|s|`, of `s` or `-s`.
///
/// # Panics
///
/// On any other modifier.
#[must_use]
pub fn abs(s: SrcOperand) -> SrcOperand {
    match s.modifier {
        SrcModifier::None | SrcModifier::Neg | SrcModifier::Abs | SrcModifier::AbsNeg => {}
        other => panic!("no absolute value of {other:?}"),
    }
    SrcOperand {
        modifier: SrcModifier::Abs,
        ..s
    }
}

/// `1 - s` (`_comp`).
#[must_use]
pub fn comp(s: SrcOperand) -> SrcOperand {
    modified(s, SrcModifier::Comp)
}

/// `s - 0.5` (`_bias`).
#[must_use]
pub fn bias(s: SrcOperand) -> SrcOperand {
    modified(s, SrcModifier::Bias)
}

/// `2 * (s - 0.5)` (`_bx2`).
#[must_use]
pub fn bx2(s: SrcOperand) -> SrcOperand {
    modified(s, SrcModifier::Sign)
}

/// `2 * s` (`_x2`).
#[must_use]
pub fn x2(s: SrcOperand) -> SrcOperand {
    modified(s, SrcModifier::X2)
}

/// `s / s.w` (`_dw`).
#[must_use]
pub fn dw(s: SrcOperand) -> SrcOperand {
    modified(s, SrcModifier::Dw)
}

/// `s / s.z` (`_dz`).
#[must_use]
pub fn dz(s: SrcOperand) -> SrcOperand {
    modified(s, SrcModifier::Dz)
}

/// `s` read at `c[a0.<lane> + n]`, `n` being its register index.
///
/// # Panics
///
/// On a lane outside `xyzw`.
#[must_use]
pub fn rel(s: SrcOperand, lane: char) -> SrcOperand {
    SrcOperand {
        rel_addr: Some(RelativeAddr {
            reg: A0,
            swizzle: Swizzle([component(lane); 4]),
        }),
        ..s
    }
}

/// `d_sat`: the result clamped to `[0, 1]`.
#[must_use]
pub fn sat(d: DstOperand) -> DstOperand {
    DstOperand {
        mods: d.mods | DstMods::SATURATE,
        ..d
    }
}

/// The result scaled by `2^shift` (`_x2` 1, `_x4` 2, `_d2` -1).
#[must_use]
pub const fn shift(d: DstOperand, shift: i8) -> DstOperand {
    DstOperand {
        shift_scale: shift,
        ..d
    }
}

const fn src_lanes(r: Register, lanes: [u8; 4]) -> SrcOperand {
    SrcOperand {
        reg: r,
        swizzle: Swizzle(lanes),
        modifier: SrcModifier::None,
        rel_addr: None,
    }
}

/// `s` with `modifier`, from no modifier.
fn modified(s: SrcOperand, modifier: SrcModifier) -> SrcOperand {
    assert!(
        s.modifier == SrcModifier::None,
        "{modifier:?} over {:?}",
        s.modifier
    );
    SrcOperand { modifier, ..s }
}

fn component(c: char) -> u8 {
    match c {
        'x' | 'r' => 0,
        'y' | 'g' => 1,
        'z' | 'b' => 2,
        'w' | 'a' => 3,
        _ => panic!("not a component: {c:?}"),
    }
}

fn write_mask(mask: &str) -> WriteMask {
    let mut bits = 0u8;
    let mut last = None;
    for lane in mask.chars().map(component) {
        assert!(
            last < Some(lane),
            "a mask names its lanes in order: {mask:?}"
        );
        last = Some(lane);
        bits |= 1 << lane;
    }
    assert!(bits != 0, "a mask names a lane");
    WriteMask(bits)
}

#[cfg(test)]
mod tests;
