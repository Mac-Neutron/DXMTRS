//! DXIL's math operations lowered to AIR.
//!
//! Ported from `A/dxil/dxil_lower_math.cpp` (`Lowering::LowerMath`): Metal's fast variants, as
//! airconv's DXBC path uses them, and the integer, bit, dot, pack and conversion operations.

use super::{
    lower::{Lowering, unsupported},
    types::op,
};
use crate::{
    air::builder::{FpBinOp, FpUnOp, IntBinOp, IntUnOp},
    api::Error,
    llvm::{TyKind, Val, sys},
};

/// The function's attribute index (`~0U`).
const FN: sys::AttributeIndex = sys::ATTRIBUTE_FUNCTION_INDEX;
/// `FLT_MIN`, the smallest normal `float`.
const FLT_MIN: f64 = 1.175_494_350_822_287_5e-38;
/// The smallest normal `half`.
const HALF_MIN: f64 = 6.103_515_625e-05;

impl<'cx> Lowering<'_, 'cx> {
    /// The math operations (`LowerMath`).
    ///
    /// # Errors
    ///
    /// The op's refusal for one it does not lower, what the AIR builder refuses.
    pub fn lower_math(&self, opcode: u32, call: Val<'cx>) -> Result<Option<Val<'cx>>, Error> {
        let b = self.b();
        let air = self.air();
        let cx = self.cx();
        let tcx = self.tcx();
        let arg = |i| self.arg(call, i);
        let ty = call.ty();
        let done = |v: Val<'cx>| -> Result<Option<Val<'cx>>, Error> { Ok(Some(v)) };
        let un = |op: FpUnOp| done(air.create_fp_un_op(op, arg(1)?, true)?);
        let i32_ty = tcx.int(32);
        let int = |v: u32| self.int(v);

        match opcode {
            op::F_ABS => un(FpUnOp::Fabs),
            op::SATURATE => un(FpUnOp::Saturate),
            op::COS => un(FpUnOp::Cos),
            op::SIN => un(FpUnOp::Sin),
            op::TAN => un(FpUnOp::Tan),
            op::ACOS => un(FpUnOp::Acos),
            op::ASIN => un(FpUnOp::Asin),
            op::ATAN => un(FpUnOp::Atan),
            op::HCOS => un(FpUnOp::Cosh),
            op::HSIN => un(FpUnOp::Sinh),
            op::HTAN => un(FpUnOp::Tanh),
            // DXIL's Exp and Log are base 2.
            op::EXP => un(FpUnOp::Exp2),
            op::LOG => un(FpUnOp::Log2),
            op::FRC => un(FpUnOp::Fract),
            op::SQRT => un(FpUnOp::Sqrt),
            op::RSQRT => un(FpUnOp::Rsqrt),
            op::ROUND_NE => un(FpUnOp::Rint),
            op::ROUND_NI => un(FpUnOp::Floor),
            op::ROUND_PI => un(FpUnOp::Ceil),
            op::ROUND_Z => un(FpUnOp::Trunc),

            op::IS_NAN => done(air.create_is_nan(arg(1)?)),
            op::IS_INF | op::IS_FINITE | op::IS_NORMAL => {
                let x = air.create_fp_un_op(FpUnOp::Fabs, arg(1)?, false)?;
                let inf = cx.const_real(x.ty(), f64::INFINITY);
                if opcode == op::IS_INF {
                    return done(b.fcmp(sys::REAL_OEQ, x, inf));
                }
                if opcode == op::IS_FINITE {
                    return done(b.fcmp(sys::REAL_OLT, x, inf));
                }
                let half = tcx.kind(x.ty()) == TyKind::Half;
                let min_normal = cx.const_real(x.ty(), if half { HALF_MIN } else { FLT_MIN });
                let normal = b.fcmp(sys::REAL_OGE, x, min_normal);
                done(b.and(normal, b.fcmp(sys::REAL_OLT, x, inf)))
            }

            op::BFREV => done(air.create_int_un_op(IntUnOp::ReverseBits, arg(1)?)?),
            op::COUNTBITS => {
                let count = air.create_int_un_op(IntUnOp::Popcount, arg(1)?)?;
                done(b.zext_or_trunc(count, ty))
            }
            // FirstbitLo counts from the least significant bit; FirstbitHi/SHi count from the
            // most significant one (DXC turns them into HLSL's firstbithigh itself). ~0u when no
            // bit qualifies.
            op::FIRSTBIT_LO | op::FIRSTBIT_HI | op::FIRSTBIT_SHI => {
                let mut x = arg(1)?;
                let zero = cx.const_int(x.ty(), 0, false);
                if opcode == op::FIRSTBIT_SHI {
                    // The first bit that differs from the sign.
                    let negative = b.icmp(sys::INT_SLT, x, zero);
                    x = b.select(negative, b.not(x), x);
                }
                let count = air.create_count_zero(x, opcode == op::FIRSTBIT_LO)?;
                let zeros = b.zext_or_trunc(count, ty);
                let none = b.icmp(sys::INT_EQ, x, zero);
                done(b.select(none, cx.const_int(ty, u32::MAX.into(), false), zeros))
            }

            op::F_MAX => done(air.create_fp_bin_op(FpBinOp::Fmax, arg(1)?, arg(2)?, true)?),
            op::F_MIN => done(air.create_fp_bin_op(FpBinOp::Fmin, arg(1)?, arg(2)?, true)?),
            op::I_MAX => done(air.create_int_bin_op(IntBinOp::Max, arg(1)?, arg(2)?, true)?),
            op::I_MIN => done(air.create_int_bin_op(IntBinOp::Min, arg(1)?, arg(2)?, true)?),
            op::U_MAX => done(air.create_int_bin_op(IntBinOp::Max, arg(1)?, arg(2)?, false)?),
            op::U_MIN => done(air.create_int_bin_op(IntBinOp::Min, arg(1)?, arg(2)?, false)?),

            // -> {hi, lo}
            op::I_MUL | op::U_MUL => {
                let wide = tcx.int(64);
                let extend = |v| {
                    if opcode == op::I_MUL {
                        b.sext(v, wide)
                    } else {
                        b.zext(v, wide)
                    }
                };
                let a = extend(arg(1)?);
                let c = extend(arg(2)?);
                let p = b.mul(a, c);
                let hi = b.trunc(b.lshr(p, cx.const_int(wide, 32, false)), i32_ty);
                let lo = b.trunc(p, i32_ty);
                done(self.aggregate(ty, &[Some(hi), Some(lo)]))
            }
            // -> {quotient, remainder}; D3D: division by zero gives ~0u for both
            op::U_DIV => {
                let zero = b.icmp(sys::INT_EQ, arg(2)?, int(0));
                let safe = b.select(zero, int(1), arg(2)?);
                let all = int(u32::MAX);
                let quotient = b.select(zero, all, b.udiv(arg(1)?, safe));
                let remainder = b.select(zero, all, b.urem(arg(1)?, safe));
                done(self.aggregate(ty, &[Some(quotient), Some(remainder)]))
            }
            // -> {result, carry/borrow}
            op::U_ADDC | op::U_SUBB => {
                let name: &[u8] = if opcode == op::U_ADDC {
                    b"llvm.uadd.with.overflow"
                } else {
                    b"llvm.usub.with.overflow"
                };
                let r = b
                    .binary_intrinsic(name, arg(1)?, arg(2)?)
                    .ok_or_else(|| unsupported(call))?;
                let result = b.extract_value(r, 0);
                let carry_ty = tcx.field(ty, 1).unwrap_or(i32_ty);
                let carry = b.zext_or_trunc(b.extract_value(r, 1), carry_ty);
                done(self.aggregate(ty, &[Some(result), Some(carry)]))
            }

            // Fused unless precise (DXC marks those; Metal Shader Converter fuses the same ones).
            op::F_MAD => {
                if !self.precise(call) {
                    return done(air.create_fma(arg(1)?, arg(2)?, arg(3)?)?);
                }
                done(b.fadd(b.fmul(arg(1)?, arg(2)?), arg(3)?))
            }
            op::FMA => done(air.create_fma(arg(1)?, arg(2)?, arg(3)?)?),
            op::I_MAD | op::U_MAD => done(b.add(b.mul(arg(1)?, arg(2)?), arg(3)?)),
            // (width, offset, value), as D3D's ibfe/ubfe
            op::IBFE | op::UBFE => {
                let width = b.and(arg(1)?, int(31));
                let offset = b.and(arg(2)?, int(31));
                let value = arg(3)?;
                let fits = b.icmp(sys::INT_ULT, b.add(width, offset), int(32));
                let shifted = b.shl(value, b.sub(int(32), b.add(width, offset)));
                let signed = opcode == op::IBFE;
                let shift = |x, by| if signed { b.ashr(x, by) } else { b.lshr(x, by) };
                let extracted = shift(shifted, b.sub(int(32), width));
                let tail = shift(value, offset);
                let field = b.select(fits, extracted, tail);
                done(b.select(b.icmp(sys::INT_EQ, width, int(0)), int(0), field))
            }
            // (width, offset, insert, base)
            op::BFI => {
                let width = b.and(arg(1)?, int(31));
                let offset = b.and(arg(2)?, int(31));
                let mask = b.shl(b.sub(b.shl(int(1), width), int(1)), offset);
                let base = b.and(arg(4)?, b.not(mask));
                done(b.or(base, b.and(b.shl(arg(3)?, offset), mask)))
            }

            op::DOT2 | op::DOT3 | op::DOT4 => {
                let n = opcode - op::DOT2 + 2;
                let vec_ty = tcx.vector(ty, n);
                let (mut a, mut c) = (cx.undef(vec_ty), cx.undef(vec_ty));
                for i in 0..n {
                    a = b.insert_element_u64(a, arg(1 + i)?, i.into());
                    c = b.insert_element_u64(c, arg(1 + n + i)?, i.into());
                }
                done(air.create_dot_product(a, c)?)
            }

            // (lo, hi)
            op::MAKE_DOUBLE => {
                let pair = b.insert_element_u64(cx.undef(air.int_ty(2)), arg(1)?, 0);
                done(b.bitcast(b.insert_element_u64(pair, arg(2)?, 1), ty))
            }
            // -> {lo, hi}
            op::SPLIT_DOUBLE => {
                let v = b.bitcast(arg(1)?, air.int_ty(2));
                let lo = b.extract_element_u64(v, 0);
                let hi = b.extract_element_u64(v, 1);
                done(self.aggregate(ty, &[Some(lo), Some(hi)]))
            }
            op::BITCAST_I16_TO_F16
            | op::BITCAST_F16_TO_I16
            | op::BITCAST_I32_TO_F32
            | op::BITCAST_F32_TO_I32 => done(b.bitcast(arg(1)?, ty)),
            // float -> low 16 bits of an i32, rounded toward zero (as D3DMetal)
            op::LEGACY_F32_TO_F16 => {
                let nearest = b.fptrunc(arg(1)?, tcx.half());
                let widened = b.fpext(nearest, tcx.float());
                let grown = air.create_fp_un_op(FpUnOp::Fabs, widened, false)?;
                let source = air.create_fp_un_op(FpUnOp::Fabs, arg(1)?, false)?;
                let grew = b.fcmp(sys::REAL_OGT, grown, source);
                let bits = b.bitcast(nearest, tcx.int(16));
                // One unit toward zero.
                let lower = b.sub(bits, cx.const_int(tcx.int(16), 1, false));
                done(b.zext(b.select(grew, lower, bits), ty))
            }
            op::LEGACY_F16_TO_F32 => {
                let bits = b.trunc(arg(1)?, tcx.int(16));
                done(b.fpext(b.bitcast(bits, tcx.half()), ty))
            }

            // (acc, ax, ay, bx, by): acc + a.x*b.x + a.y*b.y, in float
            op::DOT2_ADD_HALF => {
                let f = |i| Ok::<_, Error>(b.fpext(arg(i)?, tcx.float()));
                let x = b.fmul(f(2)?, f(4)?);
                let y = b.fmul(f(3)?, f(5)?);
                done(b.fadd(arg(1)?, b.fadd(x, y)))
            }
            // (acc, a, b): acc + sum of the byte products
            op::DOT4_ADD_I8_PACKED | op::DOT4_ADD_U8_PACKED => {
                let signed = opcode == op::DOT4_ADD_I8_PACKED;
                let mut sum = arg(1)?;
                for k in 0..4 {
                    let byte = |x: Val<'cx>| {
                        let t = b.trunc(b.lshr(x, int(8 * k)), tcx.int(8));
                        if signed {
                            b.sext(t, i32_ty)
                        } else {
                            b.zext(t, i32_ty)
                        }
                    };
                    let product = b.mul(byte(arg(2)?), byte(arg(3)?));
                    sum = b.add(sum, product);
                }
                done(sum)
            }
            // (mode: 0 unsigned, 1 signed, packed) -> four bytes, extended to i16 or i32
            op::UNPACK4X8 => {
                let signed = arg(1)?.const_u64().ok_or_else(|| unsupported(call))? == 1;
                let element = tcx.field(ty, 0).ok_or_else(|| unsupported(call))?;
                let packed = arg(2)?;
                let mut bytes = Vec::with_capacity(4);
                for k in 0..4 {
                    let t = b.trunc(
                        b.lshr(packed, cx.const_int(packed.ty(), 8 * k, false)),
                        tcx.int(8),
                    );
                    bytes.push(Some(if signed {
                        b.sext(t, element)
                    } else {
                        b.zext(t, element)
                    }));
                }
                done(self.aggregate(ty, &bytes))
            }
            // (mode: 0 truncate, 1 clamp to [0, 255], 2 clamp to [-128, 127], x, y, z, w) -> i32
            op::PACK4X8 => {
                let mode = arg(1)?.const_u64().ok_or_else(|| unsupported(call))?;
                let mut packed = int(0);
                for k in 0..4 {
                    let mut v = arg(2 + k)?;
                    if mode != 0 {
                        let (lo, hi): (i64, i64) = if mode == 1 { (0, 255) } else { (-128, 127) };
                        let lo = cx.const_int(v.ty(), lo.cast_unsigned(), true);
                        let hi = cx.const_int(v.ty(), hi.cast_unsigned(), true);
                        v = b.select(b.icmp(sys::INT_SLT, v, lo), lo, v);
                        v = b.select(b.icmp(sys::INT_SGT, v, hi), hi, v);
                    }
                    let byte = b.zext(b.trunc(v, tcx.int(8)), i32_ty);
                    packed = b.or(packed, b.shl(byte, int(8 * k)));
                }
                done(packed)
            }
            // (ref, src, accum): airconv's msad library, linked when use_msad is set (as the DXBC
            // msad lowering)
            op::MSAD => {
                if tcx.kind(ty) != TyKind::Int(32) {
                    return Err(unsupported(call));
                }
                let attrs: Vec<_> = [&b"readnone"[..], b"nounwind", b"willreturn"]
                    .iter()
                    .filter_map(|name| cx.enum_attribute(name, 0).map(|a| (FN, a)))
                    .collect();
                let fn_ty = tcx.function(ty, &[ty, ty, ty], false);
                let callee =
                    air.module()
                        .get_or_insert_function(b"dxmt.msad.i32", fn_ty, &attrs)?;
                done(b.call(callee, &[arg(1)?, arg(2)?, arg(3)?]))
            }
            _ => Err(unsupported(call)),
        }
    }

    /// Whether `call` carries `!dx.precise` (`getMetadata("dx.precise")`).
    ///
    /// The kind is looked up (and so registered in the context, which the bitcode lists) only
    /// for an instruction with metadata, as `Instruction::getMetadata` does.
    fn precise(&self, call: Val<'cx>) -> bool {
        // SAFETY: an instruction of the module.
        if unsafe { sys::has_metadata(call.raw()) } == 0 {
            return false;
        }
        let name: &[u8; 10] = b"dx.precise";
        // SAFETY: the context is live; the name holds its length.
        let kind =
            unsafe { sys::get_md_kind_id_in_context(self.cx().raw(), name.as_ptr().cast(), 10) };
        // SAFETY: an instruction of the module and a metadata kind of its context.
        !unsafe { sys::get_metadata(call.raw(), kind) }.is_null()
    }
}
