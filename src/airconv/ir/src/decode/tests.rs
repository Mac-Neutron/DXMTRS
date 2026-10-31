use super::*;
use crate::{
    ir::NO_PHASE,
    tokens::{Program, program_type, test_part},
};

/// `r0.xy` as a destination, `r1.yzwx`, `t2`, `s0`.
const DST_R0_XY: [u32; 2] = [0x0010_0032, 0];
const SRC_R1_YZWX: [u32; 2] = [0x0010_0396, 1];
const T2: [u32; 2] = [0x0010_7e46, 2];
const S0: [u32; 2] = [0x0010_600a, 0];

/// The one instruction of a compute program holding `opcode` and `operands`.
fn decode(opcode: u32, operands: &[u32], info: &mut ShaderInfo) -> Result<Instruction, Error> {
    let length = u32::try_from(operands.len() + 1).unwrap();
    let words = [&[opcode | (length << 24)][..], operands].concat();
    let part = test_part(program_type::COMPUTE, false, &words);
    let inst = Program::new(&part)
        .unwrap()
        .instructions()
        .next()
        .unwrap()?;
    read_instruction(&inst, info, NO_PHASE)
}

#[test]
fn unhandled_opcode_is_a_nop() {
    // `dadd`: the C++ prints "unhandled dxbc instruction 192" and moves on.
    let inst = tokens::Instruction {
        opcode: opcode::DADD,
        num_operands: 3,
        ..tokens::Instruction::default()
    };
    let mut info = ShaderInfo::default();
    assert_eq!(
        read_instruction(&inst, &mut info, NO_PHASE),
        Ok(Instruction::Nop)
    );
}

#[test]
fn temps_named_raise_the_temp_count() {
    let mut info = ShaderInfo::default();
    let src_r7 = [0x0010_0396, 7];
    decode(opcode::MOV, &[DST_R0_XY, src_r7].concat(), &mut info).unwrap();
    assert_eq!(info.temp_register_count, 8);
}

#[test]
fn sampling_records_the_view_and_its_operands() {
    let mut info = ShaderInfo::default();
    let inst = decode(
        opcode::SAMPLE,
        &[DST_R0_XY, SRC_R1_YZWX, T2, S0].concat(),
        &mut info,
    )
    .unwrap();
    let Instruction::Sample(sample) = inst else {
        panic!("{inst:?}");
    };
    assert_eq!(sample.src_resource.range_id, 2);
    assert_eq!(sample.src_resource.read_swizzle, SWIZZLE_IDENTITY);
    assert_eq!(
        sample.dst,
        DstOperand::Temp {
            common: DstOperandCommon {
                mask: 0b11,
                write_type: OperandDataType::Float,
            },
            regid: 0,
            phase: NO_PHASE,
        }
    );
    assert!(info.srv_map[&2].sampled);
}

#[test]
fn one_component_and_mask_mode_swizzles_read_the_shared_word() {
    // vThreadIDInGroupFlattened has one component and no selection: the C++ fell off
    // `readSrcOperandSwizzle` (an undefined value); the word under it is 0.
    let flat = Operand {
        ty: operand_type::INPUT_THREAD_ID_IN_GROUP_FLATTENED,
        num_components: operand::ONE_COMPONENT,
        ..Operand::default()
    };
    let src = read_src_operand(&flat, NO_PHASE, OperandDataType::Integer).unwrap();
    assert!(
        matches!(src, SrcOperand::Attribute { common, .. } if common.swizzle == Swizzle([0; 4]))
    );
    // A four-component temp in mask mode reads its mask's bytes.
    let masked = Operand {
        ty: operand_type::TEMP,
        num_components: operand::FOUR_COMPONENT,
        mask_or_swizzle: 0xf0,
        index_dimension: operand::INDEX_1D,
        ..Operand::default()
    };
    let src = read_src_operand(&masked, NO_PHASE, OperandDataType::Float).unwrap();
    assert!(
        matches!(src, SrcOperand::Temp { common, .. } if common.swizzle == Swizzle([0xf0, 0, 0, 0]))
    );
}

#[test]
fn operands_the_cxx_could_not_read_are_errors() {
    let mut info = ShaderInfo::default();
    // A resource as a `mov`'s destination: `readDstOperand` had no case and returned nothing.
    assert_eq!(
        decode(opcode::MOV, &[T2, SRC_R1_YZWX].concat(), &mut info),
        Err(UNEXPECTED_OPERAND)
    );
    // The rasterizer as a source: `readSrcOperand` asserted and fell off its end.
    let rasterizer = [0x0000_ee46];
    assert_eq!(
        decode(
            opcode::MOV,
            &[&DST_R0_XY[..], &rasterizer].concat(),
            &mut info
        ),
        Err(UNEXPECTED_OPERAND)
    );
    // A 64-bit immediate index.
    let x_imm64 = [0x0220_3e46, 0, 1, 0];
    assert_eq!(
        decode(opcode::MOV, &[&DST_R0_XY[..], &x_imm64].concat(), &mut info),
        Err(INDEX_64)
    );
}
