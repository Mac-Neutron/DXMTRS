//! D3D9 registers in the DXBC temporary file, and the IR operands and instructions they read.
//!
//! `r0`-`r31` are temporaries 0-31, `a0` (integers) 32, `p0` 33 (each lane all ones when set,
//! as `FloatCompare` writes it); everything else the lowering keeps (output mirrors, loop
//! counters, scratch) is numbered after them. A source operand lowers to a DXBC operand that
//! reads the D3D9 value as a `float4`, swizzled and modified as `emit.rs`'s `load_src`; a
//! write goes through [`Lower::store`], `store_dst`'s order: scale, saturate, mask, predicate.

use super::{Lower, LowerError, Support, modifier_support, range, rel_addr_support};
use crate::{
    dxso::ir::{DstMods, DstOperand, RegKind, SrcModifier, SrcOperand},
    ir::{
        ConversionOp, DstOperand as Dst, DstOperandCommon, FloatBinaryOp, FloatComparison,
        FloatUnaryOp, InstConvert, InstDotProduct, InstFloatBinaryOp, InstFloatCompare,
        InstFloatMad, InstFloatUnaryOp, InstIntegerBinaryOp, InstIntegerCompare, InstMov,
        InstMovConditional, Instruction, InstructionCommon, IntegerBinaryOp, IntegerComparison,
        NO_PHASE, OperandDataType, OperandIndex, SrcOperand as Src, SrcOperandCommon, Swizzle,
    },
};

/// `r0`-`r31`: the temporaries `emit.rs` declares (`float4 r[32]`).
pub const TEMPS: u16 = 32;
/// The address register `a0`, integers.
pub const A0: u32 = 32;
/// The predicate register `p0`, all ones per set lane.
pub const P0: u32 = 33;
/// The first temporary the lowering allocates.
pub const FIRST_FREE: u32 = 34;

pub const MASK_X: u32 = 0b1;
pub const MASK_Y: u32 = 0b10;
pub const MASK_Z: u32 = 0b100;
pub const MASK_W: u32 = 0b1000;
pub const MASK_ALL: u32 = 0b1111;

/// `1.0f`'s bits: a set predicate lane masked by them reads 1.0.
const ONE_BITS: u32 = 0x3f80_0000;

const fn common(swizzle: [u8; 4], read_type: OperandDataType) -> SrcOperandCommon {
    SrcOperandCommon {
        swizzle: Swizzle(swizzle),
        abs: false,
        neg: false,
        read_type,
    }
}

const IDENTITY: [u8; 4] = [0, 1, 2, 3];

/// Temporary `reg` read as floats.
pub const fn ftemp(reg: u32) -> Src {
    Src::Temp {
        common: common(IDENTITY, OperandDataType::Float),
        regid: reg,
        phase: NO_PHASE,
    }
}

/// Temporary `reg` read as integers.
pub const fn itemp(reg: u32) -> Src {
    Src::Temp {
        common: common(IDENTITY, OperandDataType::Integer),
        regid: reg,
        phase: NO_PHASE,
    }
}

/// Four float constants.
pub fn fimm(v: [f32; 4]) -> Src {
    Src::Immediate32 {
        common: common(IDENTITY, OperandDataType::Float),
        value: v.map(f32::to_bits),
    }
}

/// One float constant in every lane.
pub fn fsplat(v: f32) -> Src {
    fimm([v; 4])
}

/// Four integer constants.
pub const fn iimm(value: [u32; 4]) -> Src {
    Src::Immediate32 {
        common: common(IDENTITY, OperandDataType::Integer),
        value,
    }
}

/// One integer constant in every lane.
pub const fn isplat(v: u32) -> Src {
    iimm([v; 4])
}

/// Row `row` of constant-buffer range `range`.
pub const fn cbuf(range: u32, row: u32, read_type: OperandDataType) -> Src {
    cbuf_at(range, OperandIndex::Immediate(row), read_type)
}

/// The constant-buffer row `index` names.
pub const fn cbuf_at(range: u32, index: OperandIndex, read_type: OperandDataType) -> Src {
    Src::ConstantBuffer {
        common: common(IDENTITY, read_type),
        rangeid: range,
        rangeindex: OperandIndex::Immediate(0),
        regindex: index,
    }
}

/// The operand's common part, `None` for an immediate (whose loads ignore it).
const fn common_of(op: &mut Src) -> Option<&mut SrcOperandCommon> {
    match op {
        Src::Immediate32 { .. } => None,
        Src::Temp { common, .. }
        | Src::IndexableTemp { common, .. }
        | Src::Input { common, .. }
        | Src::ConstantBuffer { common, .. }
        | Src::ImmediateConstantBuffer { common, .. }
        | Src::Attribute { common, .. }
        | Src::IndexableInput { common, .. }
        | Src::InputIcp { common, .. }
        | Src::InputOcp { common, .. }
        | Src::InputPc { common, .. } => Some(common),
    }
}

/// `op` read through `s`: lane `i` reads `op`'s lane `s[i]`.
pub fn swizzle(mut op: Src, s: [u8; 4]) -> Src {
    let pick = |from: &[u8; 4], i: usize| -> u8 {
        let lane = s.get(i).copied().unwrap_or(0);
        from.get(usize::from(lane & 3)).copied().unwrap_or(0)
    };
    if let Src::Immediate32 { value, .. } = &mut op {
        let old = *value;
        *value = core::array::from_fn(|i| {
            let lane = s.get(i).copied().unwrap_or(0);
            old.get(usize::from(lane & 3)).copied().unwrap_or(0)
        });
        return op;
    }
    if let Some(c) = common_of(&mut op) {
        let old = c.swizzle.0;
        c.swizzle = Swizzle(core::array::from_fn(|i| pick(&old, i)));
    }
    op
}

/// Lane `c` of `op` in every lane.
pub fn lane(op: Src, c: u8) -> Src {
    swizzle(op, [c; 4])
}

/// `-op`, a float operand.
pub fn negate(mut op: Src) -> Src {
    if let Src::Immediate32 { value, .. } = &mut op {
        *value = value.map(|v| v ^ 0x8000_0000);
        return op;
    }
    if let Some(c) = common_of(&mut op) {
        c.neg = !c.neg;
    }
    op
}

/// `abs(op)`, a float operand.
pub fn absolute(mut op: Src) -> Src {
    if let Src::Immediate32 { value, .. } = &mut op {
        *value = value.map(|v| v & 0x7fff_ffff);
        return op;
    }
    if let Some(c) = common_of(&mut op) {
        c.abs = true;
        c.neg = false;
    }
    op
}

/// Temporary `reg`'s `mask` lanes, written as floats.
pub const fn fdst(reg: u32, mask: u32) -> Dst {
    Dst::Temp {
        common: DstOperandCommon {
            mask,
            write_type: OperandDataType::Float,
        },
        regid: reg,
        phase: NO_PHASE,
    }
}

/// Temporary `reg`'s `mask` lanes, written as integers.
pub const fn idst(reg: u32, mask: u32) -> Dst {
    Dst::Temp {
        common: DstOperandCommon {
            mask,
            write_type: OperandDataType::Integer,
        },
        regid: reg,
        phase: NO_PHASE,
    }
}

/// Output register `reg`'s `mask` lanes.
pub const fn odst(reg: u32, mask: u32) -> Dst {
    Dst::Output {
        common: DstOperandCommon {
            mask,
            write_type: OperandDataType::Float,
        },
        regid: reg,
        phase: NO_PHASE,
    }
}

const fn ic(saturate: bool) -> InstructionCommon {
    InstructionCommon {
        saturate,
        precise_mask: 0,
    }
}

pub const fn mov(dst: Dst, src: Src) -> Instruction {
    mov_sat(dst, src, false)
}

pub const fn mov_sat(dst: Dst, src: Src, saturate: bool) -> Instruction {
    Instruction::Mov(InstMov {
        common: ic(saturate),
        dst,
        src,
    })
}

pub const fn fbin(op: FloatBinaryOp, dst: Dst, src0: Src, src1: Src) -> Instruction {
    Instruction::FloatBinaryOp(InstFloatBinaryOp {
        common: ic(false),
        op,
        dst,
        src0,
        src1,
    })
}

pub const fn fun(op: FloatUnaryOp, dst: Dst, src: Src) -> Instruction {
    Instruction::FloatUnaryOp(InstFloatUnaryOp {
        common: ic(false),
        op,
        dst,
        src,
    })
}

pub const fn mad(dst: Dst, src0: Src, src1: Src, src2: Src) -> Instruction {
    Instruction::FloatMad(InstFloatMad {
        common: ic(false),
        dst,
        src0,
        src1,
        src2,
    })
}

pub const fn dot(dimension: u8, dst: Dst, src0: Src, src1: Src) -> Instruction {
    Instruction::DotProduct(InstDotProduct {
        common: ic(false),
        dst,
        src0,
        src1,
        dimension,
    })
}

pub const fn fcmp(cmp: FloatComparison, dst: Dst, src0: Src, src1: Src) -> Instruction {
    Instruction::FloatCompare(InstFloatCompare {
        common: ic(false),
        cmp,
        dst,
        src0,
        src1,
    })
}

pub const fn icmp(cmp: IntegerComparison, dst: Dst, src0: Src, src1: Src) -> Instruction {
    Instruction::IntegerCompare(InstIntegerCompare {
        cmp,
        dst,
        src0,
        src1,
    })
}

pub const fn ibin(op: IntegerBinaryOp, dst: Dst, src0: Src, src1: Src) -> Instruction {
    Instruction::IntegerBinaryOp(InstIntegerBinaryOp {
        op,
        dst,
        src0,
        src1,
    })
}

/// `dst = cond != 0 ? src0 : src1`, per lane.
pub const fn movc(dst: Dst, cond: Src, src0: Src, src1: Src) -> Instruction {
    Instruction::MovConditional(InstMovConditional {
        common: ic(false),
        dst,
        src_cond: cond,
        src0,
        src1,
    })
}

pub const fn conv(op: ConversionOp, dst: Dst, src: Src) -> Instruction {
    Instruction::Convert(InstConvert { op, dst, src })
}

/// A register write's target.
pub enum Target {
    /// A float temporary: an `r#` or an output's mirror.
    Temp(u32),
    /// A write `emit.rs` sinks into a local nothing reads.
    Discard,
}

impl Lower<'_> {
    /// `src` as a `float4`, swizzled and modified (`load_src`).
    ///
    /// # Errors
    ///
    /// A register or modifier the stage cannot read.
    pub fn src(&mut self, src: &SrcOperand) -> Result<Src, LowerError> {
        let base = match src.rel_addr {
            Some(rel) => self.relative(src, rel.reg.kind, rel.swizzle.0)?,
            None => self.read(src.reg.kind, src.reg.index)?,
        };
        self.modify(swizzle(base, src.swizzle.0), src.modifier)
    }

    /// Register `kind`/`index` as a `float4` (`register_read_expr`).
    fn read(&mut self, kind: RegKind, index: u16) -> Result<Src, LowerError> {
        Ok(match kind {
            RegKind::Temp if index < TEMPS => ftemp(u32::from(index)),
            RegKind::Temp => return Err(self.refuse("a temporary register above r31")),
            RegKind::Input | RegKind::Addr | RegKind::ColorOut | RegKind::MiscType
                if !self.is_vertex() =>
            {
                self.ps_read(kind, index)?
            }
            RegKind::Input => self.read_input(index)?,
            // `ps_1_x` clamps to `[-1, 1]`: a `def` where it is defined, a `c#` at each read.
            RegKind::Const => match self.defs.get(&index).copied() {
                Some(value) if self.sm1() => fimm(value.map(|v| v.clamp(-1.0, 1.0))),
                Some(value) => fimm(value),
                None => {
                    let c = cbuf(range::FLOAT, u32::from(index), OperandDataType::Float);
                    if self.sm1() {
                        let s = self.scratch();
                        self.emit(fbin(FloatBinaryOp::Max, fdst(s, MASK_ALL), c, fsplat(-1.0)))?;
                        self.emit(fbin(
                            FloatBinaryOp::Min,
                            fdst(s, MASK_ALL),
                            ftemp(s),
                            fsplat(1.0),
                        ))?;
                        ftemp(s)
                    } else {
                        c
                    }
                }
            },
            // `float4(a)`.
            RegKind::Addr if self.is_vertex() => {
                let s = self.scratch();
                self.emit(conv(
                    ConversionOp::SignedToFloat,
                    fdst(s, MASK_ALL),
                    itemp(A0),
                ))?;
                ftemp(s)
            }
            // `float4(aL_n)`; outside a loop `float4(0)`.
            RegKind::Loop => match self.loop_counter() {
                Some(al) => {
                    let s = self.scratch();
                    self.emit(conv(
                        ConversionOp::SignedToFloat,
                        fdst(s, MASK_ALL),
                        lane(itemp(al), 0),
                    ))?;
                    ftemp(s)
                }
                None => fsplat(0.0),
            },
            // `float4(iN)`: a `defi` local or the stage's integer file.
            RegKind::ConstInt => {
                let ints = self.defi.get(&index).map_or_else(
                    || cbuf(range::INT, u32::from(index), OperandDataType::Integer),
                    |value| iimm(value.map(i32::cast_unsigned)),
                );
                let s = self.scratch();
                self.emit(conv(ConversionOp::SignedToFloat, fdst(s, MASK_ALL), ints))?;
                ftemp(s)
            }
            // `float4(float(bN))` or `float4(float((b >> N) & 1))`.
            RegKind::ConstBool => {
                if let Some(value) = self.defb.get(&index) {
                    return Ok(fsplat(if *value { 1.0 } else { 0.0 }));
                }
                let s = self.scratch();
                let word = lane(cbuf(range::BOOL, 0, OperandDataType::Integer), 0);
                self.emit(ibin(
                    IntegerBinaryOp::UShr,
                    idst(s, MASK_X),
                    word,
                    isplat(u32::from(index)),
                ))?;
                self.emit(ibin(
                    IntegerBinaryOp::And,
                    idst(s, MASK_X),
                    itemp(s),
                    isplat(1),
                ))?;
                let f = self.scratch();
                self.emit(conv(
                    ConversionOp::UnsignedToFloat,
                    fdst(f, MASK_ALL),
                    lane(itemp(s), 0),
                ))?;
                ftemp(f)
            }
            // `float4(p0)`.
            RegKind::Predicate => {
                let s = self.scratch();
                self.emit(ibin(
                    IntegerBinaryOp::And,
                    idst(s, MASK_ALL),
                    itemp(P0),
                    isplat(ONE_BITS),
                ))?;
                ftemp(s)
            }
            // The operand of `call`/`callnz`: never read as a value.
            RegKind::Label => fsplat(0.0),
            // `vPos`/`vFace` read without their dcl: `float4(0)` (always so in a vertex shader).
            RegKind::MiscType if self.is_vertex() => fsplat(0.0),
            _ => return Err(self.refuse("a register the stage cannot read")),
        })
    }

    /// `c[a0.<c> + N]` or `c[aL + N]`, with the `def` overlay when the program has `def`s.
    ///
    /// Without `def`s the raw index reads the buffer, as the MSL; with them the unclamped
    /// index selects a `def`'s value and the buffer is read at the index clamped to 0-255
    /// (`mtld3d_const_rel`).
    fn relative(
        &mut self,
        src: &SrcOperand,
        via: RegKind,
        rel: [u8; 4],
    ) -> Result<Src, LowerError> {
        match rel_addr_support(self.stage, via) {
            Some(Support::Lowered) => {}
            Some(Support::Unsupported(why)) => return Err(self.refuse(why)),
            _ => return Err(self.refuse("a relative address the table has no row for")),
        }
        if src.reg.kind != RegKind::Const {
            return Err(self.refuse("relative addressing on a register other than c#"));
        }
        let offset = u32::from(src.reg.index);
        // The dynamic index: `a0`'s first swizzled lane, the innermost `aL`, or none.
        let dynamic = match via {
            RegKind::Addr => Some((A0, rel.first().copied().unwrap_or(0) & 3)),
            _ => self.loop_counter().map(|al| (al, 0)),
        };
        if self.defs.is_empty() {
            let index = match dynamic {
                Some((regid, component)) => OperandIndex::Temp {
                    regid,
                    phase: NO_PHASE,
                    component,
                    offset,
                },
                None => OperandIndex::Immediate(offset),
            };
            return Ok(cbuf_at(range::FLOAT, index, OperandDataType::Float));
        }
        let Some((regid, component)) = dynamic else {
            // `0 + N`: a constant index.
            return Ok(self.defs.get(&src.reg.index).map_or_else(
                || cbuf(range::FLOAT, offset.min(255), OperandDataType::Float),
                |value| fimm(*value),
            ));
        };
        let index = self.scratch();
        self.emit(ibin(
            IntegerBinaryOp::Add,
            idst(index, MASK_X),
            lane(itemp(regid), component),
            isplat(offset),
        ))?;
        let clamped = self.scratch();
        self.emit(ibin(
            IntegerBinaryOp::IMax,
            idst(clamped, MASK_X),
            lane(itemp(index), 0),
            isplat(0),
        ))?;
        self.emit(ibin(
            IntegerBinaryOp::IMin,
            idst(clamped, MASK_X),
            lane(itemp(clamped), 0),
            isplat(255),
        ))?;
        let value = self.scratch();
        let row = OperandIndex::Temp {
            regid: clamped,
            phase: NO_PHASE,
            component: 0,
            offset: 0,
        };
        self.emit(mov(
            fdst(value, MASK_ALL),
            cbuf_at(range::FLOAT, row, OperandDataType::Float),
        ))?;
        let hit = self.scratch();
        let defs: Vec<(u16, [f32; 4])> = self.defs.iter().map(|(k, v)| (*k, *v)).collect();
        for (reg, def) in defs {
            self.emit(icmp(
                IntegerComparison::Equal,
                idst(hit, MASK_X),
                lane(itemp(index), 0),
                isplat(u32::from(reg)),
            ))?;
            self.emit(movc(
                fdst(value, MASK_ALL),
                lane(itemp(hit), 0),
                fimm(def),
                ftemp(value),
            ))?;
        }
        Ok(ftemp(value))
    }

    /// A source modifier applied to a swizzled value (`apply_src_modifier`).
    fn modify(&mut self, value: Src, modifier: SrcModifier) -> Result<Src, LowerError> {
        match modifier_support(self.stage, modifier) {
            Some(Support::Lowered) => {}
            Some(Support::Unsupported(why)) => return Err(self.refuse(why)),
            _ => return Err(self.refuse("a source modifier the table has no row for")),
        }
        let s = match modifier {
            SrcModifier::None => return Ok(value),
            SrcModifier::Neg => return Ok(negate(value)),
            SrcModifier::Abs => return Ok(absolute(value)),
            SrcModifier::AbsNeg => return Ok(negate(absolute(value))),
            _ => self.scratch(),
        };
        let d = fdst(s, MASK_ALL);
        let inst = match modifier {
            // `x - 0.5`, `-(x - 0.5)`.
            SrcModifier::Bias | SrcModifier::BiasNeg => {
                fbin(FloatBinaryOp::Add, d, value, fsplat(-0.5))
            }
            // `2x - 1` and `1 - 2x`, fused as Metal's fast math fuses them.
            SrcModifier::Sign => mad(d, value, fsplat(2.0), fsplat(-1.0)),
            SrcModifier::SignNeg => mad(d, value, fsplat(-2.0), fsplat(1.0)),
            // `1 - x`.
            SrcModifier::Comp => fbin(FloatBinaryOp::Add, d, fsplat(1.0), negate(value)),
            SrcModifier::X2 => fbin(FloatBinaryOp::Mul, d, fsplat(2.0), value),
            SrcModifier::X2Neg => fbin(FloatBinaryOp::Mul, d, fsplat(-2.0), value),
            // `x / x.z`, `x / x.w`.
            SrcModifier::Dz | SrcModifier::Dw => {
                let c = if modifier == SrcModifier::Dz { 2 } else { 3 };
                fbin(FloatBinaryOp::Div, d, value.clone(), lane(value, c))
            }
            // `float4(!bool4(x))`: lanes equal to zero read 1.0.
            _ => {
                let cmp = self.scratch();
                self.emit(fcmp(
                    FloatComparison::Equal,
                    idst(cmp, MASK_ALL),
                    value,
                    fsplat(0.0),
                ))?;
                ibin(
                    IntegerBinaryOp::And,
                    idst(s, MASK_ALL),
                    itemp(cmp),
                    isplat(ONE_BITS),
                )
            }
        };
        self.emit(inst)?;
        Ok(if modifier == SrcModifier::BiasNeg {
            negate(ftemp(s))
        } else {
            ftemp(s)
        })
    }

    /// Writes `value` to `dst`, scaled, saturated, masked and predicated (`store_dst`).
    ///
    /// # Errors
    ///
    /// A register the stage cannot write, and a destination with no lane.
    pub fn store(
        &mut self,
        dst: DstOperand,
        value: Src,
        predicate: Option<&SrcOperand>,
    ) -> Result<(), LowerError> {
        let target = match self.target(dst.reg.kind, dst.reg.index)? {
            Target::Temp(t) => t,
            Target::Discard => return Ok(()),
        };
        let mask = u32::from(dst.write_mask.0 & 0xf);
        if mask == 0 {
            return Err(self.refuse("a destination with no lane"));
        }
        let mut value = value;
        if dst.shift_scale != 0 {
            let s = self.scratch();
            let scale = 2.0_f32.powi(i32::from(dst.shift_scale));
            self.emit(fbin(
                FloatBinaryOp::Mul,
                fdst(s, MASK_ALL),
                value,
                fsplat(scale),
            ))?;
            value = ftemp(s);
        }
        let saturate = dst.mods.contains(DstMods::SATURATE);
        let Some(predicate) = predicate else {
            return self.emit(mov_sat(fdst(target, mask), value, saturate));
        };
        if saturate {
            let s = self.scratch();
            self.emit(mov_sat(fdst(s, MASK_ALL), value, true))?;
            value = ftemp(s);
        }
        let cond = swizzle(itemp(P0), predicate.swizzle.0);
        let (on, off) = if predicate.modifier == SrcModifier::Not {
            (ftemp(target), value)
        } else {
            (value, ftemp(target))
        };
        self.emit(movc(fdst(target, mask), cond, on, off))
    }

    /// Writes integer `value` to the integer temporary `target`'s `mask` lanes under `p0`.
    ///
    /// # Errors
    ///
    /// A destination with no lane.
    pub fn store_int(
        &mut self,
        target: u32,
        mask: u32,
        value: Src,
        predicate: Option<&SrcOperand>,
    ) -> Result<(), LowerError> {
        if mask == 0 {
            return Err(self.refuse("a destination with no lane"));
        }
        let Some(predicate) = predicate else {
            return self.emit(mov(idst(target, mask), value));
        };
        let cond = swizzle(itemp(P0), predicate.swizzle.0);
        let (on, off) = if predicate.modifier == SrcModifier::Not {
            (itemp(target), value)
        } else {
            (value, itemp(target))
        };
        self.emit(movc(idst(target, mask), cond, on, off))
    }

    /// Where a write to `kind`/`index` lands (`register_write_target`).
    ///
    /// # Errors
    ///
    /// A register the stage has no local for.
    pub fn target(&self, kind: RegKind, index: u16) -> Result<Target, LowerError> {
        match kind {
            RegKind::Temp if index < TEMPS => Ok(Target::Temp(u32::from(index))),
            RegKind::Temp => Err(self.refuse("a temporary register above r31")),
            RegKind::Predicate => Err(self.refuse("a p0 write other than setp's")),
            _ if self.is_vertex() => self.vs_target(kind, index),
            _ => self.ps_target(kind, index),
        }
    }

    /// `a0`'s `mask` lanes from `value`, rounded (`mova`) or floored (`mov a0`, `vs_1_1`).
    ///
    /// # Errors
    ///
    /// A destination with no lane.
    pub fn write_address(
        &mut self,
        mask: u32,
        value: Src,
        round: bool,
        predicate: Option<&SrcOperand>,
    ) -> Result<(), LowerError> {
        let r = if round {
            self.round(value)?
        } else {
            let s = self.scratch();
            self.emit(fun(
                FloatUnaryOp::RoundNegativeInf,
                fdst(s, MASK_ALL),
                value,
            ))?;
            s
        };
        let i = self.scratch();
        self.emit(conv(
            ConversionOp::FloatToSigned,
            idst(i, MASK_ALL),
            ftemp(r),
        ))?;
        self.store_int(A0, mask, itemp(i), predicate)
    }

    /// MSL's `round`: to the nearest integer, halves away from zero, into a scratch.
    fn round(&mut self, value: Src) -> Result<u32, LowerError> {
        let t = self.scratch();
        self.emit(fun(
            FloatUnaryOp::RoundZero,
            fdst(t, MASK_ALL),
            value.clone(),
        ))?;
        // `x - trunc(x)` is exact; a fraction of at least one half moves away from zero.
        let frac = self.scratch();
        self.emit(fbin(
            FloatBinaryOp::Add,
            fdst(frac, MASK_ALL),
            value,
            negate(ftemp(t)),
        ))?;
        let up = self.scratch();
        self.emit(fcmp(
            FloatComparison::GreaterEqual,
            idst(up, MASK_ALL),
            ftemp(frac),
            fsplat(0.5),
        ))?;
        let down = self.scratch();
        self.emit(fcmp(
            FloatComparison::GreaterEqual,
            idst(down, MASK_ALL),
            fsplat(-0.5),
            ftemp(frac),
        ))?;
        let step = self.scratch();
        self.emit(movc(
            fdst(step, MASK_ALL),
            itemp(down),
            fsplat(-1.0),
            fsplat(0.0),
        ))?;
        self.emit(movc(
            fdst(step, MASK_ALL),
            itemp(up),
            fsplat(1.0),
            ftemp(step),
        ))?;
        let r = self.scratch();
        self.emit(fbin(
            FloatBinaryOp::Add,
            fdst(r, MASK_ALL),
            ftemp(t),
            ftemp(step),
        ))?;
        Ok(r)
    }
}
