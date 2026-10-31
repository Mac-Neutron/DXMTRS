//! The DXBC instruction decoder: one instruction from its tokens.
//!
//! Ported from `A/dxbc_instructions.cpp` (`readInstruction`, `readSrcOperand`, `readCondition`
//! and their helpers) over [`tokens::Instruction`]. The readers record what an instruction uses
//! in [`ShaderInfo`] as they go: a view read, written, sampled or compared, a counter, an `msad`.
//!
//! The release build compiles the C++'s assertions out, and where one guarded nothing the C++
//! read an undefined value or fell off a function's end; those are errors here. An opcode
//! `readInstruction` has no case for stays a [`Instruction::Nop`] with the C++'s stdout line.

use std::io::{self, Write as _};

use crate::{
    container::Error,
    ir::{
        AtomicBinaryOp, AtomicDst, AtomicDstOperandUav, AtomicOperandTgsm, Condition, ConversionOp,
        DstOperand, DstOperandCommon, FloatBinaryOp, FloatComparison, FloatUnaryOp, InputAttribute,
        InstAtomicBinOp, InstAtomicImmCmpExchange, InstAtomicImmDecrement, InstAtomicImmIncrement,
        InstBitFieldInsert, InstBufferInfo, InstCalcLod, InstConvert, InstDotProduct,
        InstExtractBits, InstFloatBinaryOp, InstFloatCompare, InstFloatMad, InstFloatUnaryOp,
        InstGather, InstGatherCompare, InstIntegerBinaryOp, InstIntegerBinaryOpWithTwoDst,
        InstIntegerCompare, InstIntegerMad, InstIntegerUnaryOp, InstInterpolateCentroid,
        InstInterpolateOffset, InstInterpolateSample, InstLoad, InstLoadRaw, InstLoadStructured,
        InstLoadUavTyped, InstMaskedSumOfAbsDiff, InstMov, InstMovConditional,
        InstPartialDerivative, InstResourceInfo, InstSample, InstSampleBias, InstSampleCompare,
        InstSampleDerivative, InstSampleInfo, InstSampleLod, InstSamplePos, InstSinCos,
        InstStoreRaw, InstStoreStructured, InstStoreUavTyped, InstSwapConditional, InstSync,
        Instruction, InstructionCommon, IntegerBinaryOp, IntegerBinaryOpWithTwoDst,
        IntegerComparison, IntegerUnaryOp, OperandDataType, OperandIndex, ResInfoModifier,
        ResourceOrUav, SWIZZLE_IDENTITY, SrcOperand, SrcOperandCommon, SrcOperandResource,
        SrcOperandSampler, SrcOperandTgsm, SrcOperandUav, Swizzle, TypelessSrc, UavBoundary,
    },
    shader_info::{
        ScalerDataType, ShaderFlags, ShaderInfo, ShaderResourceViewInfo, UavFlags,
        UnorderedAccessViewInfo,
    },
    tokens::{self, Operand, instruction, opcode, operand, operand_type},
};

/// An operand of a type its instruction does not take (the C++ fell off the reader's end).
pub const UNEXPECTED_OPERAND: Error =
    Error::fail("Invalid DXBC bytecode: an operand of an unexpected type");
/// A 64-bit register index (the C++ fell off `readOperandIndex`'s end).
pub const INDEX_64: Error = Error::fail("Invalid DXBC bytecode: a 64-bit operand index");
/// A resource indexed through an interface (the C++ asserted it unsupported and fell off).
pub const INTERFACE_RESOURCE: Error = Error::fail("Invalid DXBC bytecode: an interface resource");

/// `readInstruction`: the instruction an opcode outside the control-flow reader's cases makes.
///
/// Every temp register an operand names raises the shader's temp count first (the C++'s "ENB"
/// workaround), for every opcode.
///
/// # Errors
///
/// [`UNEXPECTED_OPERAND`], [`INDEX_64`] or [`INTERFACE_RESOURCE`] where the C++ read nothing
/// defined.
pub fn read_instruction(
    inst: &tokens::Instruction,
    info: &mut ShaderInfo,
    phase: u32,
) -> Result<Instruction, Error> {
    use OperandDataType::{Float, Half16X16, Integer};
    let count = usize::try_from(inst.num_operands).unwrap_or(usize::MAX);
    for o in inst.operands.iter().take(count) {
        if o.ty == operand_type::TEMP {
            info.temp_register_count = info.temp_register_count.max(o.index[0].reg.wrapping_add(1));
        }
    }
    let [o0, o1, o2, o3, o4, o5, o6, o7] = &inst.operands;
    let dst = |o: &Operand, ty| read_dst(o, phase, ty);
    let src = |o: &Operand, ty| read_src_operand(o, phase, ty);
    let refactoring = info.flags.contains(ShaderFlags::REFACTORING_ALLOWED);
    let common = || InstructionCommon {
        saturate: inst.saturate,
        precise_mask: if refactoring {
            u8::try_from(inst.precise_mask).unwrap_or(0)
        } else {
            0b1111
        },
    };
    let op = inst.opcode;
    // The `_s` (feedback) forms put the feedback destination first.
    let sparse = matches!(
        op,
        opcode::SAMPLE_CLAMP_FEEDBACK
            | opcode::SAMPLE_B_CLAMP_FEEDBACK
            | opcode::SAMPLE_D_CLAMP_FEEDBACK
            | opcode::SAMPLE_L_FEEDBACK
            | opcode::SAMPLE_C_CLAMP_FEEDBACK
            | opcode::SAMPLE_C_LZ_FEEDBACK
            | opcode::GATHER4_FEEDBACK
            | opcode::GATHER4_C_FEEDBACK
            | opcode::GATHER4_PO_FEEDBACK
            | opcode::GATHER4_PO_C_FEEDBACK
    );
    // `m_Operands[n + sparse]`.
    let at = |n: usize| -> &Operand {
        let all = [o0, o1, o2, o3, o4, o5, o6, o7];
        all.get(n + usize::from(sparse)).copied().unwrap_or(o7)
    };
    let feedback = || -> Result<Option<DstOperand>, Error> {
        if sparse {
            Ok(Some(dst(o1, Integer)?))
        } else {
            Ok(None)
        }
    };
    let offsets = inst.texel_offset.map(i32::from);
    let unary = |op| -> Result<Instruction, Error> {
        Ok(Instruction::FloatUnaryOp(InstFloatUnaryOp {
            common: common(),
            op,
            dst: dst(o0, Float)?,
            src: src(o1, Float)?,
        }))
    };
    let binary = |op| -> Result<Instruction, Error> {
        Ok(Instruction::FloatBinaryOp(InstFloatBinaryOp {
            common: common(),
            op,
            dst: dst(o0, Float)?,
            src0: src(o1, Float)?,
            src1: src(o2, Float)?,
        }))
    };
    let dot = |dimension| -> Result<Instruction, Error> {
        Ok(Instruction::DotProduct(InstDotProduct {
            common: common(),
            dst: dst(o0, Float)?,
            src0: src(o1, Float)?,
            src1: src(o2, Float)?,
            dimension,
        }))
    };
    let float_compare = |cmp| -> Result<Instruction, Error> {
        Ok(Instruction::FloatCompare(InstFloatCompare {
            common: common(),
            cmp,
            dst: dst(o0, Integer)?,
            src0: src(o1, Float)?,
            src1: src(o2, Float)?,
        }))
    };
    let integer_compare = |cmp| -> Result<Instruction, Error> {
        Ok(Instruction::IntegerCompare(InstIntegerCompare {
            cmp,
            dst: dst(o0, Integer)?,
            src0: src(o1, Integer)?,
            src1: src(o2, Integer)?,
        }))
    };
    let integer_unary = |op| -> Result<Instruction, Error> {
        Ok(Instruction::IntegerUnaryOp(InstIntegerUnaryOp {
            op,
            dst: dst(o0, Integer)?,
            src: src(o1, Integer)?,
        }))
    };
    let integer_binary = |op| -> Result<Instruction, Error> {
        Ok(Instruction::IntegerBinaryOp(InstIntegerBinaryOp {
            op,
            dst: dst(o0, Integer)?,
            src0: src(o1, Integer)?,
            src1: src(o2, Integer)?,
        }))
    };
    let two_dst = |op| -> Result<Instruction, Error> {
        Ok(Instruction::IntegerBinaryOpWithTwoDst(
            InstIntegerBinaryOpWithTwoDst {
                common: common(),
                op,
                dst_hi: dst(o0, Integer)?,
                dst_low: dst(o1, Integer)?,
                src0: src(o2, Integer)?,
                src1: src(o3, Integer)?,
            },
        ))
    };
    let convert = |op, dst_type, src_type| -> Result<Instruction, Error> {
        Ok(Instruction::Convert(InstConvert {
            op,
            dst: dst(o0, dst_type)?,
            src: src(o1, src_type)?,
        }))
    };
    let derivative = |ddy, coarse| -> Result<Instruction, Error> {
        Ok(Instruction::PartialDerivative(InstPartialDerivative {
            common: common(),
            dst: dst(o0, Float)?,
            src: src(o1, Float)?,
            ddy,
            coarse,
        }))
    };
    let side_effect = || DstOperand::SideEffect {
        common: DstOperandCommon {
            mask: 0b1111,
            write_type: Integer,
        },
    };
    // `atomic_*`: no result; `imm_atomic_*`: the original value in the first operand.
    let atomic = |op, imm: bool| -> Result<InstAtomicBinOp, Error> {
        let (dst_original, target, address, value) = if imm {
            (dst(o0, Integer)?, o1, o2, o3)
        } else {
            (side_effect(), o0, o1, o2)
        };
        Ok(InstAtomicBinOp {
            op,
            dst: read_atomic_dst(target, phase)?,
            dst_address: src(address, Integer)?,
            src: src(value, Integer)?,
            dst_original,
        })
    };
    let eval_reg = |info: &mut ShaderInfo| {
        let regid = o1.index[0].reg;
        // The C++'s `1 << regid` shifted by the register mod 32 on this hardware.
        info.pull_mode_reg_mask |= 1u32.wrapping_shl(regid);
        regid
    };

    Ok(match op {
        opcode::MOV => Instruction::Mov(InstMov {
            common: common(),
            dst: dst(o0, Float)?,
            src: src(o1, Float)?,
        }),
        opcode::MOVC => Instruction::MovConditional(InstMovConditional {
            common: common(),
            dst: dst(o0, Float)?,
            src_cond: src(o1, Integer)?,
            src0: src(o2, Float)?,
            src1: src(o3, Float)?,
        }),
        opcode::SWAPC => Instruction::SwapConditional(InstSwapConditional {
            dst0: dst(o0, Float)?,
            dst1: dst(o1, Float)?,
            src_cond: src(o2, Integer)?,
            src0: src(o3, Float)?,
            src1: src(o4, Float)?,
        }),
        opcode::SAMPLE | opcode::SAMPLE_CLAMP_FEEDBACK => {
            let inst = InstSample {
                dst: dst(o0, Float)?,
                src_address: src(at(1), Float)?,
                src_resource: read_resource(at(2), phase)?,
                src_sampler: read_sampler(at(3), phase)?,
                offsets,
                min_lod_clamp: optional(sparse, at(4), phase)?,
                feedback: feedback()?,
            };
            srv(info, inst.src_resource.range_id).sampled = true;
            Instruction::Sample(inst)
        }
        opcode::SAMPLE_B | opcode::SAMPLE_B_CLAMP_FEEDBACK => {
            let inst = InstSampleBias {
                dst: dst(o0, Float)?,
                src_address: src(at(1), Float)?,
                src_resource: read_resource(at(2), phase)?,
                src_sampler: read_sampler(at(3), phase)?,
                src_bias: src(at(4), Float)?,
                offsets,
                min_lod_clamp: optional(sparse, at(5), phase)?,
                feedback: feedback()?,
            };
            srv(info, inst.src_resource.range_id).sampled = true;
            Instruction::SampleBias(inst)
        }
        opcode::SAMPLE_D | opcode::SAMPLE_D_CLAMP_FEEDBACK => {
            let inst = InstSampleDerivative {
                dst: dst(o0, Float)?,
                src_address: src(at(1), Float)?,
                src_resource: read_resource(at(2), phase)?,
                src_sampler: read_sampler(at(3), phase)?,
                src_x_derivative: src(at(4), Float)?,
                src_y_derivative: src(at(5), Float)?,
                offsets,
                min_lod_clamp: optional(sparse, at(6), phase)?,
                feedback: feedback()?,
            };
            srv(info, inst.src_resource.range_id).sampled = true;
            Instruction::SampleDerivative(inst)
        }
        opcode::SAMPLE_L | opcode::SAMPLE_L_FEEDBACK => {
            let inst = InstSampleLod {
                dst: dst(o0, Float)?,
                src_address: src(at(1), Float)?,
                src_resource: read_resource(at(2), phase)?,
                src_sampler: read_sampler(at(3), phase)?,
                src_lod: src(at(4), Float)?,
                offsets,
                feedback: feedback()?,
            };
            srv(info, inst.src_resource.range_id).sampled = true;
            Instruction::SampleLod(inst)
        }
        opcode::SAMPLE_C_LZ
        | opcode::SAMPLE_C
        | opcode::SAMPLE_C_CLAMP_FEEDBACK
        | opcode::SAMPLE_C_LZ_FEEDBACK => {
            let inst = InstSampleCompare {
                dst: dst(o0, Float)?,
                src_address: src(at(1), Float)?,
                src_resource: read_resource(at(2), phase)?,
                src_sampler: read_sampler(at(3), phase)?,
                src_reference: src(at(4), Float)?,
                offsets,
                min_lod_clamp: optional(op == opcode::SAMPLE_C_CLAMP_FEEDBACK, at(5), phase)?,
                feedback: feedback()?,
                level_zero: op == opcode::SAMPLE_C_LZ || op == opcode::SAMPLE_C_LZ_FEEDBACK,
            };
            srv(info, inst.src_resource.range_id).compared = true;
            Instruction::SampleCompare(inst)
        }
        opcode::GATHER4 | opcode::GATHER4_FEEDBACK => {
            let src_resource = read_resource(at(2), phase)?;
            let ty = typed(&srv(info, src_resource.range_id).scaler_type);
            let inst = InstGather {
                dst: dst(o0, ty)?,
                src_address: src(at(1), Float)?,
                src_resource,
                src_sampler: read_sampler(at(3), phase)?,
                offset: SrcOperand::Immediate32 {
                    common: identity(Integer),
                    value: [offsets[0].cast_unsigned(), offsets[1].cast_unsigned(), 0, 0],
                },
                feedback: feedback()?,
            };
            srv(info, inst.src_resource.range_id).sampled = true;
            Instruction::Gather(inst)
        }
        opcode::GATHER4_C | opcode::GATHER4_C_FEEDBACK => {
            let inst = InstGatherCompare {
                dst: dst(o0, Float)?,
                src_address: src(at(1), Float)?,
                src_resource: read_resource(at(2), phase)?,
                src_sampler: read_sampler(at(3), phase)?,
                src_reference: src(at(4), Float)?,
                offset: SrcOperand::Immediate32 {
                    common: identity(Integer),
                    value: [offsets[0].cast_unsigned(), offsets[1].cast_unsigned(), 0, 0],
                },
                feedback: feedback()?,
            };
            srv(info, inst.src_resource.range_id).compared = true;
            Instruction::GatherCompare(inst)
        }
        opcode::GATHER4_PO | opcode::GATHER4_PO_FEEDBACK => {
            let src_resource = read_resource(at(3), phase)?;
            let ty = typed(&srv(info, src_resource.range_id).scaler_type);
            let inst = InstGather {
                dst: dst(o0, ty)?,
                src_address: src(at(1), Float)?,
                src_resource,
                src_sampler: read_sampler(at(4), phase)?,
                offset: src(at(2), Integer)?,
                feedback: feedback()?,
            };
            srv(info, inst.src_resource.range_id).sampled = true;
            Instruction::Gather(inst)
        }
        opcode::GATHER4_PO_C | opcode::GATHER4_PO_C_FEEDBACK => {
            let inst = InstGatherCompare {
                dst: dst(o0, Float)?,
                src_address: src(at(1), Float)?,
                src_resource: read_resource(at(3), phase)?,
                src_sampler: read_sampler(at(4), phase)?,
                src_reference: src(at(5), Float)?,
                offset: src(at(2), Integer)?,
                feedback: feedback()?,
            };
            srv(info, inst.src_resource.range_id).compared = true;
            Instruction::GatherCompare(inst)
        }
        opcode::SAMPLE_INFO => {
            let uint_result = inst.word() == instruction::RETURN_UINT;
            let src = if o1.ty == operand_type::RASTERIZER {
                None
            } else {
                let resource = read_resource(o1, phase)?;
                srv(info, resource.range_id).read = true;
                Some(resource)
            };
            Instruction::SampleInfo(InstSampleInfo {
                dst: dst(o0, if uint_result { Integer } else { Float })?,
                src,
                uint_result,
                read_swizzle: swizzle(o1),
            })
        }
        opcode::SAMPLE_POS => {
            info.flags.insert(ShaderFlags::USE_SAMPLEPOS);
            let resource = if o1.ty == operand_type::RASTERIZER {
                None
            } else {
                let resource = read_resource(o1, phase)?;
                srv(info, resource.range_id).read = true;
                Some(resource)
            };
            Instruction::SamplePos(InstSamplePos {
                dst: dst(o0, Float)?,
                src: resource,
                src_sample_index: src(o2, Integer)?,
                read_swizzle: swizzle(o1),
            })
        }
        opcode::BUFINFO => {
            let inst = InstBufferInfo {
                dst: dst(o0, Integer)?,
                src: read_resource_or_uav(o1, phase)?,
            };
            mark_read(info, &inst.src);
            Instruction::BufferInfo(inst)
        }
        opcode::RESINFO => {
            let modifier = match inst.word() {
                instruction::RESINFO_RETURN_UINT => ResInfoModifier::Uint,
                instruction::RESINFO_RETURN_RCPFLOAT => ResInfoModifier::Rcp,
                _ => ResInfoModifier::None,
            };
            let inst = InstResourceInfo {
                dst: dst(
                    o0,
                    if modifier == ResInfoModifier::Uint {
                        Integer
                    } else {
                        Float
                    },
                )?,
                src_mip_level: src(o1, Integer)?,
                src_resource: read_resource_or_uav(o2, phase)?,
                modifier,
            };
            mark_read(info, &inst.src_resource);
            Instruction::ResourceInfo(inst)
        }
        opcode::LD | opcode::LD_MS => {
            let src_resource = read_resource(o2, phase)?;
            let ty = typed(&srv(info, src_resource.range_id).scaler_type);
            let inst = InstLoad {
                dst: dst(o0, ty)?,
                src_address: src(o1, Integer)?,
                src_resource,
                src_sample_index: if op == opcode::LD_MS {
                    Some(src(o3, Integer)?)
                } else {
                    None
                },
                offsets,
            };
            srv(info, inst.src_resource.range_id).read = true;
            Instruction::Load(inst)
        }
        opcode::LD_UAV_TYPED => {
            let src_uav = read_uav(o2, phase)?;
            let ty = typed(&uav(info, src_uav.range_id).scaler_type);
            let inst = InstLoadUavTyped {
                dst: dst(o0, ty)?,
                src_address: src(o1, Integer)?,
                src_uav,
            };
            uav(info, inst.src_uav.range_id)
                .flags
                .insert(UavFlags::READ);
            Instruction::LoadUavTyped(inst)
        }
        opcode::STORE_UAV_TYPED => {
            let target = read_dst_uav(o0, phase)?;
            let ty = typed(&uav(info, target.range_id).scaler_type);
            let inst = InstStoreUavTyped {
                dst: target,
                src_address: src(o1, Integer)?,
                src: src(o2, ty)?,
            };
            uav(info, inst.dst.range_id).flags.insert(UavFlags::WRITTEN);
            Instruction::StoreUavTyped(inst)
        }
        opcode::LD_RAW => {
            let src_byte_offset = src(o1, Integer)?;
            let inst = InstLoadRaw {
                dst: dst(o0, Integer)?,
                opt_flag_offset_is_vec4_aligned: vec4_aligned(&src_byte_offset),
                src_byte_offset,
                src: read_typeless(o2, phase)?,
            };
            mark_typeless_read(info, &inst.src);
            Instruction::LoadRaw(inst)
        }
        opcode::STORE_RAW => {
            let inst = InstStoreRaw {
                dst: read_atomic_dst(o0, phase)?,
                dst_byte_offset: src(o1, Integer)?,
                src: src(o2, Integer)?,
            };
            mark_written(info, &inst.dst);
            Instruction::StoreRaw(inst)
        }
        opcode::LD_STRUCTURED => {
            let src_byte_offset = src(o2, Integer)?;
            let inst = InstLoadStructured {
                dst: dst(o0, Integer)?,
                src_address: src(o1, Integer)?,
                opt_flag_offset_is_vec4_aligned: vec4_aligned(&src_byte_offset),
                src_byte_offset,
                src: read_typeless(o3, phase)?,
            };
            mark_typeless_read(info, &inst.src);
            Instruction::LoadStructured(inst)
        }
        opcode::STORE_STRUCTURED => {
            let inst = InstStoreStructured {
                dst: read_atomic_dst(o0, phase)?,
                dst_address: src(o1, Integer)?,
                dst_byte_offset: src(o2, Integer)?,
                src: src(o3, Integer)?,
            };
            mark_written(info, &inst.dst);
            Instruction::StoreStructured(inst)
        }
        opcode::DP2 => dot(2)?,
        opcode::DP3 => dot(3)?,
        opcode::DP4 => dot(4)?,
        opcode::RSQ => unary(FloatUnaryOp::Rsq)?,
        opcode::RCP => unary(FloatUnaryOp::Rcp)?,
        opcode::LOG => unary(FloatUnaryOp::Log2)?,
        opcode::EXP => unary(FloatUnaryOp::Exp2)?,
        opcode::SQRT => unary(FloatUnaryOp::Sqrt)?,
        opcode::ROUND_Z => unary(FloatUnaryOp::RoundZero)?,
        opcode::ROUND_PI => unary(FloatUnaryOp::RoundPositiveInf)?,
        opcode::ROUND_NI => unary(FloatUnaryOp::RoundNegativeInf)?,
        opcode::ROUND_NE => unary(FloatUnaryOp::RoundNearestEven)?,
        opcode::FRC => unary(FloatUnaryOp::Fraction)?,
        opcode::ADD => binary(FloatBinaryOp::Add)?,
        opcode::MUL => binary(FloatBinaryOp::Mul)?,
        opcode::DIV => binary(FloatBinaryOp::Div)?,
        opcode::MAX => binary(FloatBinaryOp::Max)?,
        opcode::MIN => binary(FloatBinaryOp::Min)?,
        opcode::MAD => Instruction::FloatMad(InstFloatMad {
            common: common(),
            dst: dst(o0, Float)?,
            src0: src(o1, Float)?,
            src1: src(o2, Float)?,
            src2: src(o3, Float)?,
        }),
        opcode::IMAD | opcode::UMAD => Instruction::IntegerMad(InstIntegerMad {
            dst: dst(o0, Integer)?,
            src0: src(o1, Integer)?,
            src1: src(o2, Integer)?,
            src2: src(o3, Integer)?,
            is_signed: op == opcode::IMAD,
        }),
        opcode::MSAD => {
            info.flags.insert(ShaderFlags::USE_MSAD);
            Instruction::MaskedSumOfAbsDiff(InstMaskedSumOfAbsDiff {
                dst: dst(o0, Integer)?,
                src0: src(o1, Integer)?,
                src1: src(o2, Integer)?,
                src2: src(o3, Integer)?,
            })
        }
        opcode::SINCOS => Instruction::SinCos(InstSinCos {
            common: common(),
            dst_sin: dst(o0, Float)?,
            dst_cos: dst(o1, Float)?,
            src: src(o2, Float)?,
        }),
        opcode::EQ => float_compare(FloatComparison::Equal)?,
        opcode::GE => float_compare(FloatComparison::GreaterEqual)?,
        opcode::LT => float_compare(FloatComparison::LessThan)?,
        opcode::NE => float_compare(FloatComparison::NotEqual)?,
        opcode::IEQ => integer_compare(IntegerComparison::Equal)?,
        opcode::INE => integer_compare(IntegerComparison::NotEqual)?,
        opcode::IGE => integer_compare(IntegerComparison::SignedGreaterEqual)?,
        opcode::ILT => integer_compare(IntegerComparison::SignedLessThan)?,
        opcode::UGE => integer_compare(IntegerComparison::UnsignedGreaterEqual)?,
        opcode::ULT => integer_compare(IntegerComparison::UnsignedLessThan)?,
        opcode::NOT => integer_unary(IntegerUnaryOp::Not)?,
        opcode::INEG => integer_unary(IntegerUnaryOp::Neg)?,
        opcode::BFREV => integer_unary(IntegerUnaryOp::ReverseBits)?,
        opcode::COUNTBITS => integer_unary(IntegerUnaryOp::CountBits)?,
        opcode::FIRSTBIT_HI => integer_unary(IntegerUnaryOp::FirstHiBit)?,
        opcode::FIRSTBIT_SHI => integer_unary(IntegerUnaryOp::FirstHiBitSigned)?,
        opcode::FIRSTBIT_LO => integer_unary(IntegerUnaryOp::FirstLowBit)?,
        opcode::BFI => Instruction::BitFieldInsert(InstBitFieldInsert {
            dst: dst(o0, Integer)?,
            src0: src(o1, Integer)?,
            src1: src(o2, Integer)?,
            src2: src(o3, Integer)?,
            src3: src(o4, Integer)?,
        }),
        opcode::UBFE | opcode::IBFE => Instruction::ExtractBits(InstExtractBits {
            dst: dst(o0, Integer)?,
            src0: src(o1, Integer)?,
            src1: src(o2, Integer)?,
            src2: src(o3, Integer)?,
            is_signed: op == opcode::IBFE,
        }),
        opcode::ISHL => integer_binary(IntegerBinaryOp::IShl)?,
        opcode::ISHR => integer_binary(IntegerBinaryOp::IShr)?,
        opcode::USHR => integer_binary(IntegerBinaryOp::UShr)?,
        opcode::XOR => integer_binary(IntegerBinaryOp::Xor)?,
        opcode::OR => integer_binary(IntegerBinaryOp::Or)?,
        opcode::AND => integer_binary(IntegerBinaryOp::And)?,
        opcode::UMIN => integer_binary(IntegerBinaryOp::UMin)?,
        opcode::UMAX => integer_binary(IntegerBinaryOp::UMax)?,
        opcode::IMIN => integer_binary(IntegerBinaryOp::IMin)?,
        opcode::IMAX => integer_binary(IntegerBinaryOp::IMax)?,
        opcode::IADD => integer_binary(IntegerBinaryOp::Add)?,
        opcode::IMUL => two_dst(IntegerBinaryOpWithTwoDst::IMul)?,
        opcode::UMUL => two_dst(IntegerBinaryOpWithTwoDst::UMul)?,
        opcode::UDIV => two_dst(IntegerBinaryOpWithTwoDst::UDiv)?,
        opcode::UADDC => two_dst(IntegerBinaryOpWithTwoDst::UAddCarry)?,
        opcode::USUBB => two_dst(IntegerBinaryOpWithTwoDst::USubBorrow)?,
        opcode::F16TOF32 => convert(ConversionOp::HalfToFloat, Float, Half16X16)?,
        opcode::F32TOF16 => convert(ConversionOp::FloatToHalf, Half16X16, Float)?,
        opcode::FTOI => convert(ConversionOp::FloatToSigned, Integer, Float)?,
        opcode::FTOU => convert(ConversionOp::FloatToUnsigned, Integer, Float)?,
        opcode::ITOF => convert(ConversionOp::SignedToFloat, Float, Integer)?,
        opcode::UTOF => convert(ConversionOp::UnsignedToFloat, Float, Integer)?,
        opcode::DERIV_RTX | opcode::DERIV_RTX_FINE => derivative(false, false)?,
        opcode::DERIV_RTY | opcode::DERIV_RTY_FINE => derivative(true, false)?,
        opcode::DERIV_RTX_COARSE => derivative(false, true)?,
        opcode::DERIV_RTY_COARSE => derivative(true, true)?,
        opcode::LOD => Instruction::CalcLod(InstCalcLod {
            dst: dst(o0, Float)?,
            src_address: src(o1, Float)?,
            src_resource: read_resource(o2, phase)?,
            src_sampler: read_sampler(o3, phase)?,
        }),
        opcode::NOP => Instruction::Nop,
        opcode::SYNC => {
            let flags = inst.word();
            Instruction::Sync(InstSync {
                uav_boundary: if flags & instruction::SYNC_UNORDERED_ACCESS_VIEW_MEMORY_GLOBAL != 0
                {
                    UavBoundary::Global
                } else if flags & instruction::SYNC_UNORDERED_ACCESS_VIEW_MEMORY_GROUP != 0 {
                    UavBoundary::Group
                } else {
                    UavBoundary::None
                },
                tgsm_memory_barrier: flags & instruction::SYNC_THREAD_GROUP_SHARED_MEMORY != 0,
                tgsm_execution_barrier: flags & instruction::SYNC_THREADS_IN_GROUP != 0,
            })
        }
        opcode::ATOMIC_AND => atomic_op(info, atomic(AtomicBinaryOp::And, false)?),
        opcode::ATOMIC_OR => atomic_op(info, atomic(AtomicBinaryOp::Or, false)?),
        opcode::ATOMIC_XOR => atomic_op(info, atomic(AtomicBinaryOp::Xor, false)?),
        opcode::ATOMIC_IADD => atomic_op(info, atomic(AtomicBinaryOp::Add, false)?),
        opcode::ATOMIC_IMAX => atomic_op(info, atomic(AtomicBinaryOp::IMax, false)?),
        opcode::ATOMIC_IMIN => atomic_op(info, atomic(AtomicBinaryOp::IMin, false)?),
        opcode::ATOMIC_UMAX => atomic_op(info, atomic(AtomicBinaryOp::UMax, false)?),
        opcode::ATOMIC_UMIN => atomic_op(info, atomic(AtomicBinaryOp::UMin, false)?),
        opcode::ATOMIC_CMP_STORE => {
            info.flags.insert(ShaderFlags::USE_CMP_EXCH);
            let inst = InstAtomicImmCmpExchange {
                dst: side_effect(),
                dst_resource: read_atomic_dst(o0, phase)?,
                dst_address: src(o1, Integer)?,
                src0: src(o2, Integer)?,
                src1: src(o3, Integer)?,
            };
            mark_read_written(info, &inst.dst_resource);
            Instruction::AtomicImmCmpExchange(inst)
        }
        opcode::IMM_ATOMIC_ALLOC | opcode::IMM_ATOMIC_CONSUME => {
            let result = dst(o0, Integer)?;
            let counter = read_dst_uav(o1, phase)?;
            uav(info, counter.range_id)
                .flags
                .insert(UavFlags::WITH_COUNTER);
            if op == opcode::IMM_ATOMIC_ALLOC {
                Instruction::AtomicImmIncrement(InstAtomicImmIncrement {
                    dst: result,
                    uav: counter,
                })
            } else {
                Instruction::AtomicImmDecrement(InstAtomicImmDecrement {
                    dst: result,
                    uav: counter,
                })
            }
        }
        opcode::IMM_ATOMIC_EXCH => atomic_op(info, atomic(AtomicBinaryOp::Xchg, true)?),
        opcode::IMM_ATOMIC_CMP_EXCH => {
            info.flags.insert(ShaderFlags::USE_CMP_EXCH);
            let inst = InstAtomicImmCmpExchange {
                dst: dst(o0, Integer)?,
                dst_resource: read_atomic_dst(o1, phase)?,
                dst_address: src(o2, Integer)?,
                src0: src(o3, Integer)?,
                src1: src(o4, Integer)?,
            };
            mark_read_written(info, &inst.dst_resource);
            Instruction::AtomicImmCmpExchange(inst)
        }
        opcode::IMM_ATOMIC_AND => atomic_op(info, atomic(AtomicBinaryOp::And, true)?),
        opcode::IMM_ATOMIC_OR => atomic_op(info, atomic(AtomicBinaryOp::Or, true)?),
        opcode::IMM_ATOMIC_XOR => atomic_op(info, atomic(AtomicBinaryOp::Xor, true)?),
        opcode::IMM_ATOMIC_IADD => atomic_op(info, atomic(AtomicBinaryOp::Add, true)?),
        opcode::IMM_ATOMIC_UMAX => atomic_op(info, atomic(AtomicBinaryOp::UMax, true)?),
        opcode::IMM_ATOMIC_UMIN => atomic_op(info, atomic(AtomicBinaryOp::UMin, true)?),
        opcode::IMM_ATOMIC_IMAX => atomic_op(info, atomic(AtomicBinaryOp::IMax, true)?),
        opcode::IMM_ATOMIC_IMIN => atomic_op(info, atomic(AtomicBinaryOp::IMin, true)?),
        opcode::EVAL_CENTROID => Instruction::InterpolateCentroid(InstInterpolateCentroid {
            regid: eval_reg(info),
            dst: dst(o0, Float)?,
            read_swizzle: swizzle(o1),
        }),
        opcode::EVAL_SAMPLE_INDEX => Instruction::InterpolateSample(InstInterpolateSample {
            regid: eval_reg(info),
            dst: dst(o0, Float)?,
            sample_index: src(o2, Integer)?,
            read_swizzle: swizzle(o1),
        }),
        opcode::EVAL_SNAPPED => Instruction::InterpolateOffset(InstInterpolateOffset {
            regid: eval_reg(info),
            dst: dst(o0, Float)?,
            offset: src(o2, Integer)?,
            read_swizzle: swizzle(o1),
        }),
        _ => {
            let _ = writeln!(io::stdout().lock(), "unhandled dxbc instruction {op}");
            Instruction::Nop
        }
    })
}

/// `readSrcOperand`: a source operand read as `read_type`.
///
/// # Errors
///
/// [`UNEXPECTED_OPERAND`] for a register file a source cannot name (the rasterizer, a 64-bit
/// immediate, ...), [`INDEX_64`] for a 64-bit index.
pub fn read_src_operand(
    o: &Operand,
    phase: u32,
    read_type: OperandDataType,
) -> Result<SrcOperand, Error> {
    let common = || SrcOperandCommon {
        swizzle: swizzle(o),
        abs: o.modifier & operand::MODIFIER_ABS != 0,
        neg: o.modifier & operand::MODIFIER_NEG != 0,
        read_type,
    };
    // The two attributes compilers encode in mask mode read as `xyzw`.
    let identity_common = || SrcOperandCommon {
        swizzle: SWIZZLE_IDENTITY,
        ..common()
    };
    let attribute = |attribute| SrcOperand::Attribute {
        common: common(),
        attribute,
    };
    let [i0, i1, i2] = &o.index;
    let [t0, t1, t2] = o.index_type;
    Ok(match o.ty {
        operand_type::IMMEDIATE32 => SrcOperand::Immediate32 {
            common: identity(read_type),
            value: if o.num_components == operand::FOUR_COMPONENT {
                o.value
            } else {
                [o.value[0]; 4]
            },
        },
        operand_type::TEMP => SrcOperand::Temp {
            common: common(),
            regid: i0.reg,
            phase,
        },
        operand_type::INPUT if o.index_dimension == operand::INDEX_2D => SrcOperand::InputIcp {
            common: common(),
            cpid: read_index(i0, t0, phase)?,
            regid: i1.reg,
        },
        operand_type::INPUT if t0 == operand::INDEX_IMMEDIATE32 => SrcOperand::Input {
            common: common(),
            regid: i0.reg,
        },
        operand_type::INPUT => SrcOperand::IndexableInput {
            common: common(),
            regindex: read_index(i0, t0, phase)?,
        },
        operand_type::INPUT_CONTROL_POINT => SrcOperand::InputIcp {
            common: common(),
            cpid: read_index(i0, t0, phase)?,
            regid: i1.reg,
        },
        operand_type::OUTPUT_CONTROL_POINT => SrcOperand::InputOcp {
            common: common(),
            cpid: read_index(i0, t0, phase)?,
            regid: i1.reg,
        },
        operand_type::INPUT_PATCH_CONSTANT => SrcOperand::InputPc {
            common: common(),
            regindex: read_index(i0, t0, phase)?,
        },
        operand_type::INDEXABLE_TEMP => SrcOperand::IndexableTemp {
            common: common(),
            regfile: i0.reg,
            regindex: read_index(i1, t1, phase)?,
            phase,
        },
        operand_type::CONSTANT_BUFFER if o.index_dimension == operand::INDEX_2D => {
            SrcOperand::ConstantBuffer {
                common: common(),
                rangeid: i0.reg,
                rangeindex: read_index(i0, t0, phase)?,
                regindex: read_index(i1, t1, phase)?,
            }
        }
        operand_type::CONSTANT_BUFFER => SrcOperand::ConstantBuffer {
            common: common(),
            rangeid: i0.reg,
            rangeindex: read_index(i1, t1, phase)?,
            regindex: read_index(i2, t2, phase)?,
        },
        operand_type::IMMEDIATE_CONSTANT_BUFFER => SrcOperand::ImmediateConstantBuffer {
            common: common(),
            regindex: read_index(i0, t0, phase)?,
        },
        operand_type::INPUT_THREAD_GROUP_ID => attribute(InputAttribute::ThreadGroupId),
        operand_type::INPUT_THREAD_ID => attribute(InputAttribute::ThreadId),
        operand_type::INPUT_THREAD_ID_IN_GROUP => attribute(InputAttribute::ThreadIdInGroup),
        operand_type::INPUT_THREAD_ID_IN_GROUP_FLATTENED => {
            attribute(InputAttribute::ThreadIdInGroupFlatten)
        }
        operand_type::INPUT_COVERAGE_MASK => attribute(InputAttribute::CoverageMask),
        operand_type::OUTPUT_CONTROL_POINT_ID => SrcOperand::Attribute {
            common: identity_common(),
            attribute: InputAttribute::OutputControlPointId,
        },
        operand_type::INPUT_FORK_INSTANCE_ID => attribute(InputAttribute::ForkInstanceId),
        operand_type::INPUT_JOIN_INSTANCE_ID => attribute(InputAttribute::JoinInstanceId),
        operand_type::INPUT_DOMAIN_POINT => attribute(InputAttribute::Domain),
        operand_type::INPUT_PRIMITIVEID => SrcOperand::Attribute {
            common: identity_common(),
            attribute: InputAttribute::PrimitiveId,
        },
        operand_type::INPUT_GS_INSTANCE_ID => attribute(InputAttribute::GsInstanceId),
        _ => return Err(UNEXPECTED_OPERAND),
    })
}

/// `readCondition`: operand `index` of a conditional instruction, as an integer test.
///
/// # Errors
///
/// As [`read_src_operand`].
pub fn read_condition(
    inst: &tokens::Instruction,
    index: usize,
    phase: u32,
) -> Result<Condition, Error> {
    let o = inst.operands.get(index).ok_or(UNEXPECTED_OPERAND)?;
    Ok(Condition {
        operand: read_src_operand(o, phase, OperandDataType::Integer)?,
        test_nonzero: inst.word() == instruction::TEST_NONZERO,
    })
}

/// `readSrcOperandSwizzle`: the operand's swizzle.
///
/// Swizzle and select-1 modes both leave the four source components in the word the C++'s
/// union shares with the write mask. In mask mode, and on a one-component operand, the C++ fell
/// off the function's end with an undefined value; the same word's bytes are read here. The
/// consumers ignore the swizzle of a scalar.
const fn swizzle(o: &Operand) -> Swizzle {
    Swizzle(o.swizzle())
}

const fn identity(read_type: OperandDataType) -> SrcOperandCommon {
    SrcOperandCommon {
        swizzle: SWIZZLE_IDENTITY,
        abs: false,
        neg: false,
        read_type,
    }
}

/// `readOperandIndex`.
fn read_index(index: &tokens::Index, rep: u32, phase: u32) -> Result<OperandIndex, Error> {
    match rep {
        operand::INDEX_IMMEDIATE32 => Ok(OperandIndex::Immediate(index.reg)),
        operand::INDEX_RELATIVE => Ok(read_relative(index, phase, 0)),
        operand::INDEX_IMMEDIATE32_PLUS_RELATIVE => Ok(read_relative(index, phase, index.reg)),
        _ => Err(INDEX_64),
    }
}

/// `readOperandRelativeIndex`: a register file the C++ has no case for reads as index 0.
fn read_relative(index: &tokens::Index, phase: u32, offset: u32) -> OperandIndex {
    let component = u8::try_from(index.rel_component).unwrap_or(0);
    match index.rel_type {
        operand_type::TEMP => OperandIndex::Temp {
            regid: index.rel_index,
            phase,
            component,
            offset,
        },
        operand_type::OUTPUT_CONTROL_POINT_ID => OperandIndex::ControlPointId { offset },
        operand_type::INDEXABLE_TEMP => OperandIndex::IndexableTemp {
            regfile: index.rel_index,
            regid: index.rel_index1,
            phase,
            component,
            offset,
        },
        _ => OperandIndex::Immediate(0),
    }
}

/// `readDstOperand`: `mask` is the token's write mask shifted down (x in bit 0).
fn read_dst(o: &Operand, phase: u32, write_type: OperandDataType) -> Result<DstOperand, Error> {
    let masked = DstOperandCommon {
        mask: o.write_mask() >> 4,
        write_type,
    };
    let scalar = DstOperandCommon {
        mask: 1,
        write_type,
    };
    let [i0, i1, _] = &o.index;
    Ok(match o.ty {
        operand_type::TEMP => DstOperand::Temp {
            common: masked,
            regid: i0.reg,
            phase,
        },
        operand_type::INDEXABLE_TEMP => DstOperand::IndexableTemp {
            common: masked,
            regfile: i0.reg,
            regindex: read_index(i1, o.index_type[1], phase)?,
            phase,
        },
        operand_type::OUTPUT if o.index_type[0] == operand::INDEX_IMMEDIATE32 => {
            DstOperand::Output {
                common: masked,
                regid: i0.reg,
                phase,
            }
        }
        operand_type::OUTPUT => DstOperand::IndexableOutput {
            common: masked,
            regindex: read_index(i0, o.index_type[0], phase)?,
            phase,
        },
        operand_type::OUTPUT_DEPTH
        | operand_type::OUTPUT_DEPTH_GREATER_EQUAL
        | operand_type::OUTPUT_DEPTH_LESS_EQUAL => DstOperand::OutputDepth { common: scalar },
        operand_type::OUTPUT_COVERAGE_MASK => DstOperand::OutputCoverageMask { common: scalar },
        operand_type::OUTPUT_STENCIL_REF => DstOperand::OutputStencilRef { common: scalar },
        operand_type::NULL => DstOperand::Null {
            common: DstOperandCommon {
                mask: 0,
                write_type,
            },
        },
        _ => return Err(UNEXPECTED_OPERAND),
    })
}

/// `readSrcOperandResource`: the range is the first index.
///
/// The register is the first index too (5.0) or the second (5.1).
fn read_resource(o: &Operand, phase: u32) -> Result<SrcOperandResource, Error> {
    if o.index_type[0] != operand::INDEX_IMMEDIATE32 {
        return Err(INTERFACE_RESOURCE);
    }
    let (index, rep) = range_index(o);
    Ok(SrcOperandResource {
        range_id: o.index[0].reg,
        index: read_index(index, rep, phase)?,
        read_swizzle: swizzle(o),
    })
}

fn read_sampler(o: &Operand, phase: u32) -> Result<SrcOperandSampler, Error> {
    let (index, rep) = range_index(o);
    Ok(SrcOperandSampler {
        range_id: o.index[0].reg,
        index: read_index(index, rep, phase)?,
        gather_channel: u8::try_from(o.component).unwrap_or(0),
    })
}

fn read_uav(o: &Operand, phase: u32) -> Result<SrcOperandUav, Error> {
    let (index, rep) = range_index(o);
    Ok(SrcOperandUav {
        range_id: o.index[0].reg,
        index: read_index(index, rep, phase)?,
        read_swizzle: swizzle(o),
    })
}

fn read_dst_uav(o: &Operand, phase: u32) -> Result<AtomicDstOperandUav, Error> {
    let (index, rep) = range_index(o);
    Ok(AtomicDstOperandUav {
        range_id: o.index[0].reg,
        index: read_index(index, rep, phase)?,
        mask: o.write_mask() >> 4,
    })
}

/// The index that picks a register in a range: the first for a 1D operand, else the second.
const fn range_index(o: &Operand) -> (&tokens::Index, u32) {
    let [i0, i1, _] = &o.index;
    if o.index_dimension == operand::INDEX_1D {
        (i0, o.index_type[0])
    } else {
        (i1, o.index_type[1])
    }
}

const fn read_tgsm(o: &Operand) -> SrcOperandTgsm {
    SrcOperandTgsm {
        id: o.index[0].reg,
        read_swizzle: swizzle(o),
    }
}

fn read_atomic_dst(o: &Operand, phase: u32) -> Result<AtomicDst, Error> {
    match o.ty {
        operand_type::UNORDERED_ACCESS_VIEW => Ok(AtomicDst::Uav(read_dst_uav(o, phase)?)),
        operand_type::THREAD_GROUP_SHARED_MEMORY => Ok(AtomicDst::Tgsm(AtomicOperandTgsm {
            id: o.index[0].reg,
            mask: o.write_mask() >> 4,
        })),
        _ => Err(UNEXPECTED_OPERAND),
    }
}

fn read_typeless(o: &Operand, phase: u32) -> Result<TypelessSrc, Error> {
    match o.ty {
        operand_type::UNORDERED_ACCESS_VIEW => Ok(TypelessSrc::Uav(read_uav(o, phase)?)),
        operand_type::RESOURCE => Ok(TypelessSrc::Resource(read_resource(o, phase)?)),
        operand_type::THREAD_GROUP_SHARED_MEMORY => Ok(TypelessSrc::Tgsm(read_tgsm(o))),
        _ => Err(UNEXPECTED_OPERAND),
    }
}

fn read_resource_or_uav(o: &Operand, phase: u32) -> Result<ResourceOrUav, Error> {
    match o.ty {
        operand_type::UNORDERED_ACCESS_VIEW => Ok(ResourceOrUav::Uav(read_uav(o, phase)?)),
        operand_type::RESOURCE => Ok(ResourceOrUav::Resource(read_resource(o, phase)?)),
        _ => Err(UNEXPECTED_OPERAND),
    }
}

/// The `_cl` forms' minimum LOD clamp, unless the operand is null.
fn optional(clamp: bool, o: &Operand, phase: u32) -> Result<Option<SrcOperand>, Error> {
    if clamp && o.ty != operand_type::NULL {
        Ok(Some(read_src_operand(o, phase, OperandDataType::Float)?))
    } else {
        Ok(None)
    }
}

/// A typed load's or store's value type: integer for an integer view, float otherwise.
const fn typed(scaler: &ScalerDataType) -> OperandDataType {
    match scaler {
        ScalerDataType::Uint | ScalerDataType::Int => OperandDataType::Integer,
        ScalerDataType::Float | ScalerDataType::Double => OperandDataType::Float,
    }
}

/// An immediate byte offset that is a multiple of 16.
const fn vec4_aligned(offset: &SrcOperand) -> bool {
    matches!(offset, SrcOperand::Immediate32 { value, .. } if value[0].trailing_zeros() >= 4)
}

/// `srvMap[id]`: the view, made with zeroed fields when no declaration made it.
fn srv(info: &mut ShaderInfo, id: u32) -> &mut ShaderResourceViewInfo {
    info.srv_map.entry(id).or_default()
}

/// `uavMap[id]`, as [`srv`].
fn uav(info: &mut ShaderInfo, id: u32) -> &mut UnorderedAccessViewInfo {
    info.uav_map.entry(id).or_default()
}

fn mark_read(info: &mut ShaderInfo, view: &ResourceOrUav) {
    match view {
        ResourceOrUav::Resource(resource) => srv(info, resource.range_id).read = true,
        ResourceOrUav::Uav(view) => uav(info, view.range_id).flags.insert(UavFlags::READ),
    }
}

fn mark_typeless_read(info: &mut ShaderInfo, view: &TypelessSrc) {
    match view {
        TypelessSrc::Resource(resource) => srv(info, resource.range_id).read = true,
        TypelessSrc::Uav(view) => uav(info, view.range_id).flags.insert(UavFlags::READ),
        TypelessSrc::Tgsm(_) => {}
    }
}

fn mark_written(info: &mut ShaderInfo, target: &AtomicDst) {
    if let AtomicDst::Uav(view) = target {
        uav(info, view.range_id).flags.insert(UavFlags::WRITTEN);
    }
}

fn mark_read_written(info: &mut ShaderInfo, target: &AtomicDst) {
    if let AtomicDst::Uav(view) = target {
        let flags = &mut uav(info, view.range_id).flags;
        flags.insert(UavFlags::READ);
        flags.insert(UavFlags::WRITTEN);
    }
}

fn atomic_op(info: &mut ShaderInfo, inst: InstAtomicBinOp) -> Instruction {
    mark_read_written(info, &inst.dst);
    Instruction::AtomicBinOp(inst)
}

#[cfg(test)]
mod tests;
