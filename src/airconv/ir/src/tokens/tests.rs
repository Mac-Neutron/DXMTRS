use super::*;

const MOV: u32 = opcode::MOV;
const RET: u32 = opcode::RET | (1 << 24);
/// `r0.xy` as a destination: four components, mask mode, temp, one immediate index.
const DST_R0_XY: [u32; 2] = [0x0010_0032, 0];
/// `r1.yzwx`: swizzle mode.
const SRC_R1_YZWX: [u32; 2] = [0x0010_0396, 1];
/// `r1.z`: select-1 mode.
const SRC_R1_Z: [u32; 2] = [0x0010_002a, 1];

fn parse(part: &[u8]) -> Result<Vec<Instruction>, Error> {
    Program::new(part)?.instructions().collect()
}

fn with_length(opcode: u32, words: &[u32]) -> Vec<u32> {
    let length = u32::try_from(words.len() + 1).unwrap();
    [&[opcode | (length << 24)][..], words].concat()
}

#[test]
fn token_walk_stops_at_blob_end() {
    // The length token claims ten tokens; the part holds three.
    let mut part = test_part(program_type::COMPUTE, false, &[RET]);
    part[4..8].copy_from_slice(&10u32.to_le_bytes());
    assert_eq!(Program::new(&part).unwrap_err(), PROGRAM_PAST_PART);
    // No room for the header.
    assert_eq!(Program::new(&part[..6]).unwrap_err(), PROGRAM_PAST_PART);
    // The length fits, but the destination's index token is past the part.
    let part = test_part(
        program_type::COMPUTE,
        false,
        &[MOV | (5 << 24), DST_R0_XY[0]],
    );
    assert_eq!(parse(&part).unwrap_err(), INSTRUCTION_PAST_PART);
}

#[test]
fn shorter_declared_length_ends_the_walk() {
    let mut part = test_part(program_type::COMPUTE, false, &[RET, RET, RET]);
    part[4..8].copy_from_slice(&3u32.to_le_bytes());
    assert_eq!(parse(&part).unwrap().len(), 1);
}

#[test]
fn walk_moves_on_by_the_length_token() {
    // A `nop` that says it is three tokens long, two words of anything, then `ret`.
    let part = test_part(
        program_type::COMPUTE,
        false,
        &[opcode::NOP | (3 << 24), 0xdead_beef, 0xffff_ffff, RET],
    );
    let ops: Vec<u32> = parse(&part).unwrap().iter().map(|i| i.opcode).collect();
    assert_eq!(ops, [opcode::NOP, opcode::RET]);
}

#[test]
fn zero_length_and_unknown_opcodes_are_errors() {
    let part = test_part(program_type::COMPUTE, false, &[opcode::NOP]);
    assert_eq!(parse(&part).unwrap_err(), EMPTY_INSTRUCTION);
    let part = test_part(
        program_type::COMPUTE,
        false,
        &[opcode::NUM_OPCODES | (1 << 24)],
    );
    assert_eq!(parse(&part).unwrap_err(), UNKNOWN_OPCODE);
}

#[test]
fn operand_word_keeps_mask_swizzle_and_select() {
    let words: Vec<u32> = [
        with_length(MOV, &[DST_R0_XY, SRC_R1_YZWX].concat()),
        with_length(MOV, &[DST_R0_XY, SRC_R1_Z].concat()),
    ]
    .concat();
    let part = test_part(program_type::COMPUTE, false, &words);
    let insts = parse(&part).unwrap();
    let [dst, swizzled, ..] = &insts[0].operands;
    assert_eq!(dst.write_mask(), 0x30);
    assert_eq!(dst.swizzle(), [0x30, 0, 0, 0]);
    assert_eq!(swizzled.swizzle(), [1, 2, 3, 0]);
    assert_eq!(swizzled.index[0].reg, 1);
    let selected = &insts[1].operands[1];
    assert_eq!((selected.component, selected.swizzle()), (2, [2, 2, 2, 2]));
    // A third selection mode does not exist.
    let bad = with_length(MOV, &[DST_R0_XY, [0x0010_000e, 1]].concat());
    let part = test_part(program_type::COMPUTE, false, &bad);
    assert_eq!(parse(&part).unwrap_err(), BAD_SELECTION);
}

#[test]
fn texel_offsets_are_four_bit_signed() {
    // `sample_aoffimmi(-1, 7, -8)`: an extended sample-controls token, then the operands.
    let ext = 1 | (0xf << 9) | (7 << 13) | (8 << 17);
    let operands = [DST_R0_XY, SRC_R1_YZWX, [0x0010_7e46, 0], [0x0010_600a, 0]].concat();
    let length = u32::try_from(operands.len() + 2).unwrap();
    let words = [
        &[opcode::SAMPLE | 0x8000_0000 | (length << 24), ext][..],
        &operands,
    ]
    .concat();
    let part = test_part(program_type::PIXEL, false, &words);
    assert_eq!(parse(&part).unwrap()[0].texel_offset, [-1, 7, -8]);
}

/// A source whose register index is relative `levels` times over: `v[x0[x0[... r0.x]]]`.
fn nested_source(levels: u32) -> Vec<u32> {
    let mut held = vec![0x0010_000a, 0];
    for _ in 1..levels {
        // x0[<held>].x: indexable temp, 2D, an immediate then a relative index.
        held = [vec![0x0420_300a, 0], held].concat();
    }
    // v[<held>]: input, 1D, relative.
    [vec![0x0090_1e46], held].concat()
}

#[test]
fn relative_indices_nest_eight_deep() {
    let deep = with_length(MOV, &[DST_R0_XY.to_vec(), nested_source(8)].concat());
    let part = test_part(program_type::COMPUTE, false, &deep);
    assert_eq!(
        parse(&part).unwrap()[0].operands[1].index[0].rel_type,
        operand_type::INDEXABLE_TEMP
    );
    let deeper = with_length(MOV, &[DST_R0_XY.to_vec(), nested_source(9)].concat());
    let part = test_part(program_type::COMPUTE, false, &deeper);
    assert_eq!(parse(&part).unwrap_err(), INDEX_NESTING);
}

#[test]
fn custom_data_keeps_its_words_and_bounds() {
    let icb = opcode::CUSTOMDATA | (instruction::CUSTOMDATA_DCL_IMMEDIATE_CONSTANT_BUFFER << 11);
    let part = test_part(program_type::COMPUTE, false, &[icb, 4, 7, 9, RET]);
    let insts = parse(&part).unwrap();
    assert!(matches!(&insts[0].payload, Payload::CustomData { class: 3, data } if data == &[7, 9]));
    assert_eq!(insts[1].opcode, opcode::RET);
    // Its length token claims words past the part.
    let part = test_part(program_type::COMPUTE, false, &[icb, 40, 7]);
    assert_eq!(parse(&part).unwrap_err(), INSTRUCTION_PAST_PART);
}

#[test]
fn declarations_read_their_payloads() {
    // dcl_resource_texture2d (float,float,float,float) t3, space 2 (5.1: a 3D index and a space).
    let words = [
        opcode::DCL_RESOURCE | (3 << 11) | (7 << 24),
        (operand_type::RESOURCE << 12) | (3 << 20),
        3,
        3,
        3,
        0x5555,
        2,
        RET,
    ];
    let part = test_part(program_type::PIXEL, true, &words);
    let insts = parse(&part).unwrap();
    assert!(matches!(
        insts[0].payload,
        Payload::Resource {
            dimension: 3,
            return_type: [5, 5, 5, 5],
            space: 2,
            ..
        }
    ));
}
