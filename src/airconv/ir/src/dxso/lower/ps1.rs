//! The pixel stage, shader models 1.0-1.4: the `t#` file, the texture-addressing and bump ops.
//!
//! `emit.rs`'s SM1 paths: `t0`-`t7` are a read-write file seeded from texture coordinates 0-7
//! (`float4 t[8]`), the colour is whatever `r0` holds at the end, float constants read clamped
//! to `[-1, 1]` (a `def` where it is defined, a `c#` at each read; no result is clamped), and
//! each sampling op samples the stage its destination names, whose 2D slot `ps_bindings`
//! synthesizes. Each op below is its `translate_instruction` arm: `texcoord`/`texcrd`, `tex`/
//! `texld`, `texbem`/`texbeml`/`bem` on the stage's two `bump_env` rows, the `texm3x2`/`texm3x3`
//! family, `texreg2ar`/`gb`/`rgb`, `texdp3`/`texdp3tex` and the two depth writes. A sample goes
//! through `sample_or_compare` with no level control, as Task 5's `ps_texture` takes it.

use super::{
    Lower, LowerError, range,
    regs::{
        MASK_ALL, MASK_W, MASK_X, MASK_Y, MASK_Z, Target, cbuf, dot, fbin, fdst, fsplat, ftemp,
        lane, mad, mov, mov_sat, movc, negate, swizzle,
    },
};
use crate::{
    dxso::{
        TextureType,
        ir::{DstOperand, Instruction, Register, SrcModifier, SrcOperand, Swizzle},
        opcode::Opcode,
    },
    ir::{FloatBinaryOp, FloatComparison, OperandDataType, SrcOperand as Src},
};

/// The `t#` registers `emit.rs` declares (`float4 t[8]`).
pub const T_REGS: u16 = 8;

const XY: u32 = MASK_X | MASK_Y;
const XYZ: u32 = MASK_X | MASK_Y | MASK_Z;
const ZW: u32 = MASK_Z | MASK_W;
const YZW: u32 = MASK_Y | MASK_Z | MASK_W;

/// An SM1 op that samples the stage its destination names (`sm1_op_samples`).
#[must_use]
pub const fn samples(opcode: Opcode) -> bool {
    matches!(
        opcode,
        Opcode::TexLd
            | Opcode::TexBem
            | Opcode::TexBemL
            | Opcode::TexM3x2Tex
            | Opcode::TexM3x3Tex
            | Opcode::TexM3x3Spec
            | Opcode::TexM3x3VSpec
            | Opcode::TexReg2Ar
            | Opcode::TexReg2Gb
            | Opcode::TexReg2Rgb
            | Opcode::TexDp3Tex
    )
}

/// Whether [`Lower::ps1`] lowers `opcode` in a model `major` pixel shader.
#[must_use]
pub const fn lowers(opcode: Opcode, major: u8) -> bool {
    matches!(
        opcode,
        Opcode::TexCoord
            | Opcode::TexBem
            | Opcode::TexBemL
            | Opcode::TexReg2Ar
            | Opcode::TexReg2Gb
            | Opcode::TexM3x2Pad
            | Opcode::TexM3x2Tex
            | Opcode::TexM3x3Pad
            | Opcode::TexM3x3Tex
            | Opcode::TexM3x3Spec
            | Opcode::TexM3x3VSpec
            | Opcode::TexReg2Rgb
            | Opcode::TexDp3Tex
            | Opcode::TexM3x2Depth
            | Opcode::TexDp3
            | Opcode::TexM3x3
            | Opcode::TexDepth
            | Opcode::Bem
    ) || (major == 1 && matches!(opcode, Opcode::TexLd))
}

/// Row `row` of `bump_env`.
const fn bump_row(row: u32) -> Src {
    cbuf(range::BUMP_ENV, row, OperandDataType::Float)
}

impl Lower<'_> {
    /// Whether this lowers a `ps_1_x` program, whose `t#` file and constant clamp `emit.rs` keeps.
    #[must_use]
    pub const fn sm1(&self) -> bool {
        !self.is_vertex() && self.program.major == 1
    }

    /// `t[index]`'s mirror.
    ///
    /// # Errors
    ///
    /// A program other than `ps_1_x`, whose MSL has no `t` array, and an index past `t7`.
    pub fn t_reg(&self, index: u16) -> Result<u32, LowerError> {
        if !self.sm1() {
            return Err(self.refuse("t[] outside ps_1_x (the MSL declares no t array there)"));
        }
        self.ps
            .t
            .get(usize::from(index))
            .copied()
            .ok_or_else(|| self.refuse("a texture register above t7 (the MSL declares t[8])"))
    }

    /// Register `reg` as `register_read_expr` reads it: no swizzle, no modifier.
    fn register(&mut self, reg: Register) -> Result<Src, LowerError> {
        self.src(&SrcOperand {
            reg,
            swizzle: Swizzle::IDENTITY,
            modifier: SrcModifier::None,
            rel_addr: None,
        })
    }

    /// Whether stage `n` divides its coordinate by `.w` (`is_tt_projected`): a cube does not.
    fn projected(&self, n: u16) -> bool {
        n < 8
            && self.ps.key.tt_projected_mask & (1 << n) != 0
            && self.ps.samplers.get(&n) != Some(&TextureType::TextureCube)
    }

    /// `(c.w != 0 ? c.xyz / c.w : 0, 1)`, the stage's projective divide.
    fn project(&mut self, c: Src) -> Result<Src, LowerError> {
        let w = lane(c.clone(), 3);
        let q = self.binary(FloatBinaryOp::Div, c, w.clone())?;
        let nonzero = self.mask(FloatComparison::NotEqual, w, fsplat(0.0))?;
        let s = self.scratch();
        self.emit(movc(fdst(s, XYZ), nonzero, q, fsplat(0.0)))?;
        self.emit(mov(fdst(s, MASK_W), fsplat(1.0)))?;
        Ok(ftemp(s))
    }

    /// Stage `slot` sampled at `coord` (`sample_or_compare(ctx, slot, coord, None, None)`).
    fn sample1(&mut self, inst: &Instruction, slot: u16, coord: Src) -> Result<Src, LowerError> {
        if !self.ps.samplers.contains_key(&slot) {
            return Err(self.refuse("a sample from a sampler slot with no dcl"));
        }
        self.ps_texture(inst, &[Some(coord)], slot, [0, 1, 2, 3])
    }

    /// The MSL declares `bump_env` only when a bump op is in the program's main body.
    fn bump_env(&self) -> Result<(), LowerError> {
        if self.ps.ranges.contains(&range::BUMP_ENV) {
            Ok(())
        } else {
            Err(self.refuse("a bump op only in a subroutine (the MSL declares no bump_env)"))
        }
    }

    /// `dot(a.xyz, b.xyz)` into a scratch's lanes `mask`.
    fn dot3_into(&mut self, s: u32, mask: u32, a: Src, b: Src) -> Result<(), LowerError> {
        self.emit(dot(3, fdst(s, mask), a, b))
    }

    /// A `ps_1_x` texture-addressing or bump op, or `ps_1_x`'s `tex`/`texld`.
    ///
    /// # Errors
    ///
    /// What `emit.rs` refuses or its MSL does not compile: an op without its destination or
    /// source, a `t[]` it names outside `ps_1_x` or past `t7`, a texture coordinate past 15.
    pub fn ps1(&mut self, inst: &Instruction) -> Result<(), LowerError> {
        let srcs = self.sources(inst)?;
        let Some(dst) = inst.dst else {
            return Err(self.refuse("a ps_1_x op without its destination"));
        };
        let n = dst.reg.index;
        let value = match inst.opcode {
            // ps_1_4 `texcrd rN, tM`: the source as is; earlier models `texcoord tN`:
            // `float4(saturate(in.texcoordN).xyz, 1.0)`.
            Opcode::TexCoord => {
                if self.program.minor >= 4 {
                    self.arg(&srcs, 0)?
                } else {
                    let c = self.texcoord(n)?;
                    let s = self.scratch();
                    self.emit(mov_sat(fdst(s, XYZ), c, true))?;
                    self.emit(mov(fdst(s, MASK_W), fsplat(1.0)))?;
                    ftemp(s)
                }
            }
            // ps_1_4 `texld rN, s` samples stage N at `s`; earlier `tex tN` at `tN`, divided
            // under `D3DTTFF_PROJECTED`.
            Opcode::TexLd => {
                let coord = if self.program.minor >= 4 {
                    self.arg(&srcs, 0)?
                } else {
                    let raw = self.register(dst.reg)?;
                    if self.projected(n) {
                        self.project(raw)?
                    } else {
                        raw
                    }
                };
                self.sample1(inst, n, coord)?
            }
            Opcode::TexBem | Opcode::TexBemL => return self.texbem(inst, dst, &srcs),
            // `bem rN, s0, s1`: `(s0.xy + M_N * s1.xy, 0, 0)`.
            Opcode::Bem => {
                self.bump_env()?;
                let (s0, s1) = (self.arg(&srcs, 0)?, self.arg(&srcs, 1)?);
                let m = bump_row(u32::from(n) * 2);
                let s = self.scratch();
                self.emit(mad(
                    fdst(s, XY),
                    swizzle(m.clone(), [0, 1, 0, 1]),
                    lane(s1.clone(), 0),
                    s0,
                ))?;
                self.emit(mad(
                    fdst(s, XY),
                    swizzle(m, [2, 3, 2, 3]),
                    lane(s1, 1),
                    ftemp(s),
                ))?;
                self.emit(mov(fdst(s, ZW), fsplat(0.0)))?;
                ftemp(s)
            }
            // A pad row: `(dot(tN.xyz, s.xyz), 0, 0, 0)`.
            Opcode::TexM3x2Pad | Opcode::TexM3x3Pad => {
                let coord = self.register(dst.reg)?;
                let s = self.scratch();
                self.dot3_into(s, MASK_X, coord, self.arg(&srcs, 0)?)?;
                self.emit(mov(fdst(s, YZW), fsplat(0.0)))?;
                ftemp(s)
            }
            // `texm3x2tex tM, s`: stage M at `(t[M-1].x, dot(tM.xyz, s.xyz))`.
            Opcode::TexM3x2Tex => {
                let coord = self.register(dst.reg)?;
                let u = self.t_reg(n.saturating_sub(1))?;
                let s = self.scratch();
                self.emit(mov(fdst(s, MASK_X), lane(ftemp(u), 0)))?;
                self.dot3_into(s, MASK_Y, coord, self.arg(&srcs, 0)?)?;
                self.emit(mov(fdst(s, ZW), fsplat(0.0)))?;
                self.sample1(inst, n, ftemp(s))?
            }
            Opcode::TexM3x2Depth => return self.texm3x2depth(dst, &srcs),
            Opcode::TexM3x3 | Opcode::TexM3x3Tex | Opcode::TexM3x3Spec | Opcode::TexM3x3VSpec => {
                self.texm3x3(inst, dst, &srcs)?
            }
            // `texreg2ar`/`gb`/`rgb tN, s`: stage N at `(s.a, s.r)`, `(s.g, s.b)`, `s.rgb`.
            Opcode::TexReg2Ar | Opcode::TexReg2Gb | Opcode::TexReg2Rgb => {
                let a = self.arg(&srcs, 0)?;
                let s = self.scratch();
                match inst.opcode {
                    Opcode::TexReg2Ar => {
                        self.emit(mov(fdst(s, XY), swizzle(a, [3, 0, 3, 0])))?;
                        self.emit(mov(fdst(s, ZW), fsplat(0.0)))?;
                    }
                    Opcode::TexReg2Gb => {
                        self.emit(mov(fdst(s, XY), swizzle(a, [1, 2, 1, 2])))?;
                        self.emit(mov(fdst(s, ZW), fsplat(0.0)))?;
                    }
                    _ => {
                        self.emit(mov(fdst(s, XYZ), a))?;
                        self.emit(mov(fdst(s, MASK_W), fsplat(0.0)))?;
                    }
                }
                self.sample1(inst, n, ftemp(s))?
            }
            // `texdp3 tN, s`: `dot(tN.xyz, s.xyz)` in every lane.
            Opcode::TexDp3 => {
                let coord = self.register(dst.reg)?;
                let s = self.scratch();
                self.dot3_into(s, MASK_ALL, coord, self.arg(&srcs, 0)?)?;
                ftemp(s)
            }
            // `texdp3tex tN, s`: stage N at `(dot(tN.xyz, s.xyz), 0, 0, 0)`.
            Opcode::TexDp3Tex => {
                let coord = self.register(dst.reg)?;
                let s = self.scratch();
                self.dot3_into(s, MASK_X, coord, self.arg(&srcs, 0)?)?;
                self.emit(mov(fdst(s, YZW), fsplat(0.0)))?;
                self.sample1(inst, n, ftemp(s))?
            }
            // `texdepth rN`: the depth `saturate(r.x / min(r.y, 1))`.
            Opcode::TexDepth => {
                let r = self.register(dst.reg)?;
                let depth = self.depth_mirror()?;
                let s = self.scratch();
                self.emit(fbin(
                    FloatBinaryOp::Min,
                    fdst(s, MASK_X),
                    lane(r.clone(), 1),
                    fsplat(1.0),
                ))?;
                self.emit(fbin(
                    FloatBinaryOp::Div,
                    fdst(s, MASK_X),
                    lane(r, 0),
                    lane(ftemp(s), 0),
                ))?;
                return self.emit(mov_sat(fdst(depth, MASK_ALL), lane(ftemp(s), 0), true));
            }
            _ => return Err(self.refuse("an opcode with no ps_1_x lowering")),
        };
        self.store(dst, value, inst.predicate.as_ref())
    }

    /// `_depth_storage`, which every program with a depth op declares.
    fn depth_mirror(&self) -> Result<u32, LowerError> {
        self.ps
            .depth
            .ok_or_else(|| self.refuse("a depth write with no depth output"))
    }

    /// `texbem`/`texbeml tN, tM`: stage N at `tN.xy + M_N * tM.xy`, its own `.zw`.
    ///
    /// The store ignores the predicate, as the MSL's; `texbeml` then scales the written
    /// register's `.rgb`, whatever the mask, by `saturate(tM.z * lscale + loffset)`, `tM` read
    /// again after the store.
    fn texbem(
        &mut self,
        inst: &Instruction,
        dst: DstOperand,
        srcs: &[Option<Src>],
    ) -> Result<(), LowerError> {
        self.bump_env()?;
        let n = dst.reg.index;
        let coord = self.register(dst.reg)?;
        let bump = self.arg(srcs, 0)?;
        let base = if self.projected(n) {
            self.project(coord)?
        } else {
            coord
        };
        // `bx + m00 * b.x + m10 * b.y`, `by + m01 * b.x + m11 * b.y`, fused left to right.
        let m = bump_row(u32::from(n) * 2);
        let s = self.scratch();
        self.emit(mad(
            fdst(s, XY),
            swizzle(m.clone(), [0, 1, 0, 1]),
            lane(bump.clone(), 0),
            base.clone(),
        ))?;
        self.emit(mad(
            fdst(s, XY),
            swizzle(m, [2, 3, 2, 3]),
            lane(bump, 1),
            ftemp(s),
        ))?;
        self.emit(mov(fdst(s, ZW), base))?;
        let sampled = self.sample1(inst, n, ftemp(s))?;
        self.store(dst, sampled, None)?;
        if inst.opcode != Opcode::TexBemL {
            return Ok(());
        }
        let Target::Temp(target) = self.target(dst.reg.kind, n)? else {
            return Ok(());
        };
        let Some(source) = inst.srcs.first() else {
            return Err(self.refuse("an instruction without its source operand"));
        };
        let bump = self.src(source)?;
        let lum = bump_row(u32::from(n) * 2 + 1);
        let l = self.scratch();
        self.emit(mad(
            fdst(l, MASK_X),
            lane(bump, 2),
            lane(lum.clone(), 0),
            lane(lum, 1),
        ))?;
        self.emit(mov_sat(fdst(l, MASK_X), lane(ftemp(l), 0), true))?;
        self.emit(fbin(
            FloatBinaryOp::Mul,
            fdst(target, XYZ),
            ftemp(target),
            lane(ftemp(l), 0),
        ))
    }

    /// `texm3x2depth tM, s`: the depth `w != 0 ? saturate(t[M-1].x / w) : 1`, `w` this row.
    fn texm3x2depth(&mut self, dst: DstOperand, srcs: &[Option<Src>]) -> Result<(), LowerError> {
        let coord = self.register(dst.reg)?;
        let z = self.t_reg(dst.reg.index.saturating_sub(1))?;
        let depth = self.depth_mirror()?;
        let w = self.scratch();
        self.dot3_into(w, MASK_X, coord, self.arg(srcs, 0)?)?;
        let q = self.scratch();
        self.emit(fbin(
            FloatBinaryOp::Div,
            fdst(q, MASK_X),
            lane(ftemp(z), 0),
            lane(ftemp(w), 0),
        ))?;
        self.emit(mov_sat(fdst(q, MASK_X), lane(ftemp(q), 0), true))?;
        let nonzero = self.mask(FloatComparison::NotEqual, lane(ftemp(w), 0), fsplat(0.0))?;
        self.emit(movc(
            fdst(depth, MASK_ALL),
            lane(nonzero, 0),
            lane(ftemp(q), 0),
            fsplat(1.0),
        ))
    }

    /// The `texm3x3` family on the normal `(t[M-2].x, t[M-1].x, dot(tM.xyz, s.xyz))`.
    ///
    /// `texm3x3` is the normal with `.w` 1, `texm3x3tex` samples stage M with it, `spec` and
    /// `vspec` reflect the eye (`s1`, or the three iterated coordinates' `.w`) about it,
    /// `2 dot(n, e) / dot(n, n) * n - e`, and sample stage M there.
    fn texm3x3(
        &mut self,
        inst: &Instruction,
        dst: DstOperand,
        srcs: &[Option<Src>],
    ) -> Result<Src, LowerError> {
        let stage = dst.reg.index;
        let coord = self.register(dst.reg)?;
        let (row0, row1) = (
            self.t_reg(stage.saturating_sub(2))?,
            self.t_reg(stage.saturating_sub(1))?,
        );
        let normal = self.scratch();
        self.emit(mov(fdst(normal, MASK_X), lane(ftemp(row0), 0)))?;
        self.emit(mov(fdst(normal, MASK_Y), lane(ftemp(row1), 0)))?;
        self.dot3_into(normal, MASK_Z, coord, self.arg(srcs, 0)?)?;
        match inst.opcode {
            Opcode::TexM3x3 => {
                self.emit(mov(fdst(normal, MASK_W), fsplat(1.0)))?;
                return Ok(ftemp(normal));
            }
            Opcode::TexM3x3Tex => {
                self.emit(mov(fdst(normal, MASK_W), fsplat(0.0)))?;
                return self.sample1(inst, stage, ftemp(normal));
            }
            _ => {}
        }
        let eye = if inst.opcode == Opcode::TexM3x3Spec {
            self.arg(srcs, 1)?
        } else {
            let eye = self.scratch();
            for (lane_mask, index) in [
                (MASK_X, stage.saturating_sub(2)),
                (MASK_Y, stage.saturating_sub(1)),
                (MASK_Z, stage),
            ] {
                let coordinate = self.texcoord(index)?;
                self.emit(mov(fdst(eye, lane_mask), lane(coordinate, 3)))?;
            }
            ftemp(eye)
        };
        let ne = self.scratch();
        self.dot3_into(ne, MASK_X, ftemp(normal), eye.clone())?;
        self.emit(fbin(
            FloatBinaryOp::Mul,
            fdst(ne, MASK_X),
            fsplat(2.0),
            lane(ftemp(ne), 0),
        ))?;
        let nn = self.scratch();
        self.dot3_into(nn, MASK_X, ftemp(normal), ftemp(normal))?;
        self.emit(fbin(
            FloatBinaryOp::Div,
            fdst(ne, MASK_X),
            lane(ftemp(ne), 0),
            lane(ftemp(nn), 0),
        ))?;
        let reflected = self.scratch();
        self.emit(mad(
            fdst(reflected, XYZ),
            lane(ftemp(ne), 0),
            ftemp(normal),
            negate(eye),
        ))?;
        self.emit(mov(fdst(reflected, MASK_W), fsplat(0.0)))?;
        self.sample1(inst, stage, ftemp(reflected))
    }
}
