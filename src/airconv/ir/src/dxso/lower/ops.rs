//! The instructions that compute a value, each as `emit.rs`'s `translate_instruction` writes it.
//!
//! An instruction's sources are read first (`load_src`), its value computed into a scratch
//! temporary in as many IR instructions as the MSL expression takes, then stored through the
//! destination path. Scalar instructions (`rcp`, `rsq`, `exp`, `log`, `pow`, ...) read lane x
//! of their swizzled source and broadcast, as the MSL's `float4(f((src).x))`.

use super::{
    Lower, LowerError, MAX_DXSO_INSTRUCTIONS, Support, control, opcode_value, range,
    regs::{
        MASK_ALL, MASK_W, MASK_X, MASK_Y, MASK_Z, absolute, cbuf, conv, dot, fbin, fcmp, fdst,
        fsplat, ftemp, fun, ibin, idst, itemp, lane, mad, mov, movc, negate, swizzle,
    },
    support,
};
use crate::{
    dxso::{
        TextureType,
        ir::{CmpFunc, InstrFlags, Instruction, RegKind, Register, SrcOperand},
        opcode::Opcode,
    },
    ir::{
        ConversionOp, FloatBinaryOp, FloatComparison, FloatUnaryOp, InstSample, InstSampleBias,
        InstSampleDerivative, InstSampleLod, Instruction as Ir, IntegerBinaryOp, OperandDataType,
        OperandIndex, SrcOperand as Src, SrcOperandResource, SrcOperandSampler, Swizzle,
    },
};

/// An instruction that lacks a source operand its opcode reads.
const MISSING: &str = "an instruction without its source operand";

/// The DXBC comparison and operand order of a D3D9 comparison (`ifc`, `breakc`, `setp`).
///
/// `>` and `<=` swap their operands (both ordered, as the MSL's); an unknown function is
/// `==`, the MSL's default.
pub const fn compare(func: Option<CmpFunc>, a: Src, b: Src) -> (FloatComparison, Src, Src) {
    match func {
        Some(CmpFunc::Gt) => (FloatComparison::LessThan, b, a),
        Some(CmpFunc::Ge) => (FloatComparison::GreaterEqual, a, b),
        Some(CmpFunc::Lt) => (FloatComparison::LessThan, a, b),
        Some(CmpFunc::Ne) => (FloatComparison::NotEqual, a, b),
        Some(CmpFunc::Le) => (FloatComparison::GreaterEqual, b, a),
        Some(CmpFunc::Eq) | None => (FloatComparison::Equal, a, b),
    }
}

impl Lower<'_> {
    /// Lowers `insts` in order.
    ///
    /// # Errors
    ///
    /// The first instruction the stage refuses.
    pub fn body(&mut self, insts: &[Instruction]) -> Result<(), LowerError> {
        for inst in insts {
            self.instruction(inst)?;
        }
        Ok(())
    }

    /// Lowers one instruction, after checking the opcode table.
    pub fn instruction(&mut self, inst: &Instruction) -> Result<(), LowerError> {
        self.release();
        self.opcode = opcode_value(inst.opcode);
        // Every instruction counts, those that emit nothing (`call`, `nop`) too, so a call tree
        // cannot walk without bound.
        self.lowered += 1;
        if self.lowered > MAX_DXSO_INSTRUCTIONS {
            return Err(self.refuse("a program that expands past 65536 instructions"));
        }
        match support(self.stage, inst.opcode, &control(inst)) {
            Some(Support::Lowered) => {}
            Some(Support::Unsupported(why)) => return Err(self.refuse(why)),
            Some(Support::Parser) => return Err(self.refuse("a token the parser consumes")),
            None => return Err(self.refuse("an opcode and control with no row")),
        }
        match inst.opcode {
            Opcode::Nop => return Ok(()),
            Opcode::If | Opcode::Ifc => return self.open_if(inst),
            Opcode::Else => return self.open_else(),
            Opcode::EndIf => return self.end_if(),
            Opcode::Loop => return self.open_loop(inst),
            Opcode::Rep => return self.open_rep(inst),
            Opcode::EndLoop | Opcode::EndRep => return self.end_loop(),
            Opcode::Break | Opcode::BreakC | Opcode::BreakP => return self.break_loop(inst),
            Opcode::Call | Opcode::CallNz => return self.call(inst),
            Opcode::SetP => return self.setp(inst),
            Opcode::MovA => return self.mova(inst),
            _ => {}
        }
        let srcs = self.sources(inst)?;
        let value = self.value(inst, &srcs)?;
        let Some(dst) = inst.dst else {
            return Ok(());
        };
        // `vs_1_1`'s `mov a0` (and any vertex write of `a0` but `mova`) floors, unpredicated.
        if dst.reg.kind == RegKind::Addr && self.is_vertex() {
            return self.write_address(u32::from(dst.write_mask.0 & 0xf), value, false, None);
        }
        self.store(dst, value, inst.predicate.as_ref())
    }

    /// Every source but a sampler and `loop`'s `aL`, read (`loads_source`).
    pub fn sources(&mut self, inst: &Instruction) -> Result<Vec<Option<Src>>, LowerError> {
        let mut out = Vec::with_capacity(inst.srcs.len());
        for (i, s) in inst.srcs.iter().enumerate() {
            let skip = s.reg.kind == RegKind::Sampler || (inst.opcode == Opcode::Loop && i == 0);
            out.push(if skip { None } else { Some(self.src(s)?) });
        }
        Ok(out)
    }

    /// Source `i` as read, or the error of an instruction that lacks it.
    pub fn arg(&self, srcs: &[Option<Src>], i: usize) -> Result<Src, LowerError> {
        srcs.get(i)
            .cloned()
            .flatten()
            .ok_or_else(|| self.refuse(MISSING))
    }

    /// A scratch holding `op(a, b)` in every lane.
    fn binary(&mut self, op: FloatBinaryOp, a: Src, b: Src) -> Result<Src, LowerError> {
        let s = self.scratch();
        self.emit(fbin(op, fdst(s, MASK_ALL), a, b))?;
        Ok(ftemp(s))
    }

    /// A scratch holding `op(a)` in every lane.
    fn unary(&mut self, op: FloatUnaryOp, a: Src) -> Result<Src, LowerError> {
        let s = self.scratch();
        self.emit(fun(op, fdst(s, MASK_ALL), a))?;
        Ok(ftemp(s))
    }

    /// A scratch holding the `dim`-wide dot product of `a` and `b` in every lane.
    fn dot(&mut self, dim: u8, a: Src, b: Src) -> Result<Src, LowerError> {
        let s = self.scratch();
        self.emit(dot(dim, fdst(s, MASK_ALL), a, b))?;
        Ok(ftemp(s))
    }

    /// A scratch holding `cond ? a : b` per lane, `cond` an integer mask.
    fn select(&mut self, cond: Src, a: Src, b: Src) -> Result<Src, LowerError> {
        let s = self.scratch();
        self.emit(movc(fdst(s, MASK_ALL), cond, a, b))?;
        Ok(ftemp(s))
    }

    /// A scratch holding the comparison's mask per lane.
    pub fn mask(&mut self, cmp: FloatComparison, a: Src, b: Src) -> Result<Src, LowerError> {
        let s = self.scratch();
        self.emit(fcmp(cmp, idst(s, MASK_ALL), a, b))?;
        Ok(itemp(s))
    }

    /// `pow(base, e)` as Metal's fast math computes it: `exp2(e * log2(base))`.
    fn pow(&mut self, base: Src, e: Src) -> Result<Src, LowerError> {
        let l = self.unary(FloatUnaryOp::Log2, base)?;
        let p = self.binary(FloatBinaryOp::Mul, l, e)?;
        self.unary(FloatUnaryOp::Exp2, p)
    }

    /// The instruction's value as a `float4` operand.
    fn value(&mut self, inst: &Instruction, srcs: &[Option<Src>]) -> Result<Src, LowerError> {
        let opcode = self.opcode;
        let a = |i: usize| {
            srcs.get(i)
                .cloned()
                .flatten()
                .ok_or(LowerError::Unsupported {
                    opcode,
                    what: MISSING,
                })
        };
        Ok(match inst.opcode {
            Opcode::Mov => a(0)?,
            Opcode::Add => self.binary(FloatBinaryOp::Add, a(0)?, a(1)?)?,
            Opcode::Sub => self.binary(FloatBinaryOp::Add, a(0)?, negate(a(1)?))?,
            Opcode::Mul => self.binary(FloatBinaryOp::Mul, a(0)?, a(1)?)?,
            Opcode::Mad => {
                let s = self.scratch();
                self.emit(mad(fdst(s, MASK_ALL), a(0)?, a(1)?, a(2)?))?;
                ftemp(s)
            }
            Opcode::Dp3 => self.dot(3, a(0)?, a(1)?)?,
            Opcode::Dp4 => self.dot(4, a(0)?, a(1)?)?,
            Opcode::Rcp => self.unary(FloatUnaryOp::Rcp, lane(a(0)?, 0))?,
            Opcode::Rsq => self.unary(FloatUnaryOp::Rsq, absolute(lane(a(0)?, 0)))?,
            Opcode::Min => self.binary(FloatBinaryOp::Min, a(0)?, a(1)?)?,
            Opcode::Max => self.binary(FloatBinaryOp::Max, a(0)?, a(1)?)?,
            Opcode::Frc => self.unary(FloatUnaryOp::Fraction, a(0)?)?,
            // `mix(s2, s1, s0)` = `s2 + (s1 - s2) * s0`.
            Opcode::Lrp => {
                let (s0, s1, s2) = (a(0)?, a(1)?, a(2)?);
                let d = self.binary(FloatBinaryOp::Add, s1, negate(s2.clone()))?;
                let s = self.scratch();
                self.emit(mad(fdst(s, MASK_ALL), d, s0, s2))?;
                ftemp(s)
            }
            // `dot == 0 ? s : s * rsqrt(dot)`, every lane scaled.
            Opcode::Nrm => {
                let s = a(0)?;
                let d = self.dot(3, s.clone(), s.clone())?;
                let r = self.unary(FloatUnaryOp::Rsq, d.clone())?;
                let scaled = self.binary(FloatBinaryOp::Mul, s.clone(), r)?;
                let zero = self.mask(FloatComparison::Equal, d, fsplat(0.0))?;
                self.select(zero, s, scaled)?
            }
            Opcode::Abs => absolute(a(0)?),
            Opcode::Pow => {
                let (b, e) = (absolute(lane(a(0)?, 0)), lane(a(1)?, 0));
                self.pow(b, e)?
            }
            // `vs_1_1`: `(2^floor(x), x - floor(x), 2^x, 1)`.
            Opcode::ExpP if self.is_vertex() && self.program.major == 1 => {
                let x = lane(a(0)?, 0);
                let f = self.unary(FloatUnaryOp::RoundNegativeInf, x.clone())?;
                let s = self.scratch();
                self.emit(fun(FloatUnaryOp::Exp2, fdst(s, MASK_X), f.clone()))?;
                self.emit(fbin(
                    FloatBinaryOp::Add,
                    fdst(s, MASK_Y),
                    x.clone(),
                    negate(f),
                ))?;
                self.emit(fun(FloatUnaryOp::Exp2, fdst(s, MASK_Z), x))?;
                self.emit(mov(fdst(s, MASK_W), fsplat(1.0)))?;
                ftemp(s)
            }
            Opcode::Exp | Opcode::ExpP => self.unary(FloatUnaryOp::Exp2, lane(a(0)?, 0))?,
            Opcode::Log | Opcode::LogP => {
                self.unary(FloatUnaryOp::Log2, absolute(lane(a(0)?, 0)))?
            }
            // `float4(cross(s0.xyz, s1.xyz), 0)`.
            Opcode::Crs => {
                let (s0, s1) = (a(0)?, a(1)?);
                let t = self.binary(
                    FloatBinaryOp::Mul,
                    swizzle(s0.clone(), [2, 0, 1, 0]),
                    swizzle(s1.clone(), [1, 2, 0, 0]),
                )?;
                let s = self.scratch();
                self.emit(mad(
                    fdst(s, MASK_X | MASK_Y | MASK_Z),
                    swizzle(s0, [1, 2, 0, 0]),
                    swizzle(s1, [2, 0, 1, 0]),
                    negate(t),
                ))?;
                self.emit(mov(fdst(s, MASK_W), fsplat(0.0)))?;
                ftemp(s)
            }
            // `sign`: 1 above zero, -1 below, the zero itself, 0 for a NaN.
            Opcode::Sgn => {
                let x = a(0)?;
                let ordered = self.mask(FloatComparison::Equal, x.clone(), x.clone())?;
                let r = self.select(ordered, x.clone(), fsplat(0.0))?;
                let below = self.mask(FloatComparison::LessThan, x.clone(), fsplat(0.0))?;
                let r = self.select(below, fsplat(-1.0), r)?;
                let above = self.mask(FloatComparison::LessThan, fsplat(0.0), x)?;
                self.select(above, fsplat(1.0), r)?
            }
            // `float4(dot(s0.xy, s1.xy) + s2.x)`.
            Opcode::Dp2Add => {
                let d = self.dot(2, a(0)?, a(1)?)?;
                self.binary(FloatBinaryOp::Add, d, lane(a(2)?, 0))?
            }
            Opcode::Lit => self.lit(a(0)?)?,
            // `(1, s0.y * s1.y, s0.z, s1.w)`.
            Opcode::Dst => {
                let (s0, s1) = (a(0)?, a(1)?);
                let s = self.scratch();
                self.emit(mov(fdst(s, MASK_X), fsplat(1.0)))?;
                self.emit(fbin(
                    FloatBinaryOp::Mul,
                    fdst(s, MASK_Y),
                    s0.clone(),
                    s1.clone(),
                ))?;
                self.emit(mov(fdst(s, MASK_Z), s0))?;
                self.emit(mov(fdst(s, MASK_W), s1))?;
                ftemp(s)
            }
            Opcode::Cnd => self.cnd(inst, a(0)?, a(1)?, a(2)?)?,
            // `float4(cos(s.x), sin(s.x), 0, 0)`; the SM2 coefficient sources are ignored.
            Opcode::SinCos => {
                let x = lane(a(0)?, 0);
                let s = self.scratch();
                self.emit(Ir::SinCos(crate::ir::InstSinCos {
                    common: crate::ir::InstructionCommon {
                        saturate: false,
                        precise_mask: 0,
                    },
                    dst_sin: fdst(s, MASK_Y),
                    dst_cos: fdst(s, MASK_X),
                    src: x,
                }))?;
                self.emit(mov(fdst(s, MASK_Z | MASK_W), fsplat(0.0)))?;
                ftemp(s)
            }
            Opcode::M4x4 => self.matrix(inst, &a(0)?, 4, 4)?,
            Opcode::M4x3 => self.matrix(inst, &a(0)?, 4, 3)?,
            Opcode::M3x4 => self.matrix(inst, &a(0)?, 3, 4)?,
            Opcode::M3x3 => self.matrix(inst, &a(0)?, 3, 3)?,
            Opcode::M3x2 => self.matrix(inst, &a(0)?, 3, 2)?,
            // `select(s2, s1, s0 >= 0)`.
            Opcode::Cmp => {
                let c = self.mask(FloatComparison::GreaterEqual, a(0)?, fsplat(0.0))?;
                self.select(c, a(1)?, a(2)?)?
            }
            // `1 - step(s1, s0)` and `step(s1, s0)`: `step` is 0 only where `s0 < s1`.
            Opcode::Slt | Opcode::Sge => {
                let below = self.mask(FloatComparison::LessThan, a(0)?, a(1)?)?;
                let (on, off) = if inst.opcode == Opcode::Slt {
                    (1.0, 0.0)
                } else {
                    (0.0, 1.0)
                };
                self.select(below, fsplat(on), fsplat(off))?
            }
            Opcode::TexLd | Opcode::TexLdL | Opcode::TexLdD => self.texture(inst, srcs)?,
            _ => return Err(self.refuse("an opcode with no lowering")),
        })
    }

    /// `lit`: `(1, max(x, 0), x > 0 && y > 0 ? pow(y, clamp(w, ±127.9961)) : 0, 1)`.
    fn lit(&mut self, src: Src) -> Result<Src, LowerError> {
        let (lx, ly, lw) = (lane(src.clone(), 0), lane(src.clone(), 1), lane(src, 3));
        let power = self.binary(FloatBinaryOp::Max, lw, fsplat(-127.9961))?;
        let power = self.binary(FloatBinaryOp::Min, power, fsplat(127.9961))?;
        let specular = self.pow(ly.clone(), power)?;
        let x_above = self.mask(FloatComparison::LessThan, fsplat(0.0), lx.clone())?;
        let y_above = self.mask(FloatComparison::LessThan, fsplat(0.0), ly)?;
        let both = self.scratch();
        self.emit(ibin(
            IntegerBinaryOp::And,
            idst(both, MASK_ALL),
            x_above,
            y_above,
        ))?;
        let out = self.scratch();
        self.emit(mov(fdst(out, MASK_X | MASK_W), fsplat(1.0)))?;
        self.emit(fbin(FloatBinaryOp::Max, fdst(out, MASK_Y), lx, fsplat(0.0)))?;
        self.emit(movc(fdst(out, MASK_Z), itemp(both), specular, fsplat(0.0)))?;
        Ok(ftemp(out))
    }

    /// `cnd`: `ps_1_4` per lane, every other model on `s0.x`; a co-issued non-alpha one is `s1`.
    fn cnd(&mut self, inst: &Instruction, s0: Src, s1: Src, s2: Src) -> Result<Src, LowerError> {
        if self.program.major == 1 && self.program.minor >= 4 {
            let c = self.mask(FloatComparison::LessThan, fsplat(0.5), s0)?;
            return self.select(c, s1, s2);
        }
        let alpha_only = inst.dst.is_some_and(|d| d.write_mask.0 == 0b1000);
        if inst.flags.contains(InstrFlags::COISSUE) && !alpha_only {
            return Ok(s1);
        }
        let c = self.mask(FloatComparison::LessThan, fsplat(0.5), lane(s0, 0))?;
        self.select(c, s1, s2)
    }

    /// `m<cols>x<rows>`: lane `i` is the dot product with row `i` (`c[n + i]`), the rest 0.
    fn matrix(
        &mut self,
        inst: &Instruction,
        v: &Src,
        cols: u8,
        rows: u8,
    ) -> Result<Src, LowerError> {
        let Some(start) = inst.srcs.get(1) else {
            return Err(self.refuse(MISSING));
        };
        let s = self.scratch();
        for i in 0..rows {
            let Some(index) = start.reg.index.checked_add(u16::from(i)) else {
                return Err(self.refuse("a matrix past the last register"));
            };
            let row = SrcOperand {
                reg: Register {
                    kind: start.reg.kind,
                    index,
                },
                ..*start
            };
            let m = self.src(&row)?;
            self.emit(dot(cols, fdst(s, 1 << i), v.clone(), m))?;
        }
        let rest = MASK_ALL & !((1u32 << rows) - 1);
        if rest != 0 {
            self.emit(mov(fdst(s, rest), fsplat(0.0)))?;
        }
        Ok(ftemp(s))
    }

    /// `texld`/`texldp`/`texldb`, `texldl` and `texldd` on a declared sampler slot.
    fn texture(&mut self, inst: &Instruction, srcs: &[Option<Src>]) -> Result<Src, LowerError> {
        if self.program.major < 2 && inst.opcode == Opcode::TexLd {
            return Err(self.refuse("tex in shader model 1 (it reads pixel texture coordinates)"));
        }
        let Some(sampler) = inst.srcs.get(1) else {
            return Err(self.refuse(MISSING));
        };
        let slot = sampler.reg.index;
        if !self.samplers().contains_key(&slot) {
            return Err(self.refuse("a sample from a sampler slot with no dcl"));
        }
        let coord = self.arg(srcs, 0)?;
        let src_resource = SrcOperandResource {
            range_id: u32::from(slot),
            index: OperandIndex::Immediate(0),
            read_swizzle: Swizzle(sampler.swizzle.0),
        };
        let src_sampler = SrcOperandSampler {
            range_id: u32::from(slot),
            index: OperandIndex::Immediate(0),
            gather_channel: 0,
        };
        let s = self.scratch();
        let dst = fdst(s, MASK_ALL);
        let inst = match inst.opcode {
            Opcode::TexLdL => Ir::SampleLod(InstSampleLod {
                dst,
                src_lod: self.explicit_level(slot, coord.clone())?,
                src_address: coord,
                src_resource,
                src_sampler,
                offsets: [0; 3],
                feedback: None,
            }),
            Opcode::TexLdD => Ir::SampleDerivative(InstSampleDerivative {
                dst,
                src_address: coord,
                src_resource,
                src_sampler,
                src_x_derivative: self.arg(srcs, 2)?,
                src_y_derivative: self.arg(srcs, 3)?,
                offsets: [0; 3],
                min_lod_clamp: None,
                feedback: None,
            }),
            _ => {
                // `texldp`: the coordinate divided by its `.w` first; `texldb`: the bias is the
                // unprojected coordinate's `.w` (`emit.rs` applies both when both are set).
                let src_address = if inst.flags.contains(InstrFlags::TEX_PROJECTED) {
                    self.binary(FloatBinaryOp::Div, coord.clone(), lane(coord.clone(), 3))?
                } else {
                    coord.clone()
                };
                if inst.flags.contains(InstrFlags::TEX_BIASED) {
                    Ir::SampleBias(InstSampleBias {
                        dst,
                        src_bias: lane(coord, 3),
                        src_address,
                        src_resource,
                        src_sampler,
                        offsets: [0; 3],
                        min_lod_clamp: None,
                        feedback: None,
                    })
                } else {
                    Ir::Sample(InstSample {
                        dst,
                        src_address,
                        src_resource,
                        src_sampler,
                        offsets: [0; 3],
                        min_lod_clamp: None,
                        feedback: None,
                    })
                }
            }
        };
        self.emit(inst)?;
        Ok(ftemp(s))
    }

    /// `texldl`'s level: `coord.w`, or `max(coord.w + lod.x, lod.y)` under the vertex LOD table.
    fn explicit_level(&mut self, slot: u16, coord: Src) -> Result<Src, LowerError> {
        if !(self.is_vertex() && self.vs.lod_table) {
            return Ok(lane(coord, 3));
        }
        // Row `slot` of a `float2` table: half of `int4` row `slot / 2`.
        let row = cbuf(range::VS_LOD, u32::from(slot / 2), OperandDataType::Float);
        let first = u8::try_from((slot % 2) * 2).unwrap_or(0);
        let l = self.scratch();
        self.emit(fbin(
            FloatBinaryOp::Add,
            fdst(l, MASK_X),
            lane(coord, 3),
            lane(row.clone(), first),
        ))?;
        self.emit(fbin(
            FloatBinaryOp::Max,
            fdst(l, MASK_X),
            lane(ftemp(l), 0),
            lane(row, first + 1),
        ))?;
        Ok(lane(ftemp(l), 0))
    }

    /// The stage's declared sampler slots, typed by the bound texture.
    pub const fn samplers(&self) -> &std::collections::BTreeMap<u16, TextureType> {
        &self.vs.samplers
    }

    /// `setp_<cmp> p0, s0, s1`: the comparison per lane into `p0`, masked and predicated.
    fn setp(&mut self, inst: &Instruction) -> Result<(), LowerError> {
        let srcs = self.sources(inst)?;
        let (cmp, a, b) = compare(inst.cmp_func, self.arg(&srcs, 0)?, self.arg(&srcs, 1)?);
        let c = self.mask(cmp, a, b)?;
        let mask = inst
            .dst
            .map_or(MASK_ALL, |d| u32::from(d.write_mask.0 & 0xf));
        self.store_int(super::regs::P0, mask, c, inst.predicate.as_ref())
    }

    /// `mova a0, s`: rounded, masked and predicated (a vertex shader's `a` only).
    fn mova(&mut self, inst: &Instruction) -> Result<(), LowerError> {
        if !self.is_vertex() {
            return Err(self.refuse("mova outside a vertex shader"));
        }
        let srcs = self.sources(inst)?;
        let value = self.arg(&srcs, 0)?;
        let Some(dst) = inst.dst else {
            return Err(self.refuse("mova without a destination"));
        };
        self.write_address(
            u32::from(dst.write_mask.0 & 0xf),
            value,
            true,
            inst.predicate.as_ref(),
        )
    }
}

/// Integer `float -> int` of lane `c` of `value` into the integer temporary `reg`'s lane x.
pub fn to_int(reg: u32, value: Src, c: u8) -> Ir {
    conv(
        ConversionOp::FloatToSigned,
        idst(reg, MASK_X),
        lane(value, c),
    )
}
