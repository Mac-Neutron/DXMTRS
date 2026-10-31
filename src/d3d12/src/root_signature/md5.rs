//! The DXBC container's checksum: MD5's rounds with the container's own last block.
//!
//! Ported from `src/util/util_md5.cpp` (`Hasher` and `hashDxbcBinary`), whose original source is
//! dxbc-spirv's `util/util_md5.cpp` and `dxbc/dxbc_container.cpp`, MIT License, Copyright (c) 2025
//! Philip Rebohle. The hash covers the container from its version field on (offset 20) and does
//! not finalize as MD5 does: the bit count and a derived number frame the last block instead.

/// MD5's per-round left rotations.
const SHIFTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];
/// MD5's per-round constants.
const CONSTANTS: [u32; 64] = [
    0xd76a_a478,
    0xe8c7_b756,
    0x2420_70db,
    0xc1bd_ceee,
    0xf57c_0faf,
    0x4787_c62a,
    0xa830_4613,
    0xfd46_9501,
    0x6980_98d8,
    0x8b44_f7af,
    0xffff_5bb1,
    0x895c_d7be,
    0x6b90_1122,
    0xfd98_7193,
    0xa679_438e,
    0x49b4_0821,
    0xf61e_2562,
    0xc040_b340,
    0x265e_5a51,
    0xe9b6_c7aa,
    0xd62f_105d,
    0x0244_1453,
    0xd8a1_e681,
    0xe7d3_fbc8,
    0x21e1_cde6,
    0xc337_07d6,
    0xf4d5_0d87,
    0x455a_14ed,
    0xa9e3_e905,
    0xfcef_a3f8,
    0x676f_02d9,
    0x8d2a_4c8a,
    0xfffa_3942,
    0x8771_f681,
    0x6d9d_6122,
    0xfde5_380c,
    0xa4be_ea44,
    0x4bde_cfa9,
    0xf6bb_4b60,
    0xbebf_bc70,
    0x289b_7ec6,
    0xeaa1_27fa,
    0xd4ef_3085,
    0x0488_1d05,
    0xd9d4_d039,
    0xe6db_99e5,
    0x1fa2_7cf8,
    0xc4ac_5665,
    0xf429_2244,
    0x432a_ff97,
    0xab94_23a7,
    0xfc93_a039,
    0x655b_59c3,
    0x8f0c_cc92,
    0xffef_f47d,
    0x8584_5dd1,
    0x6fa8_7e4f,
    0xfe2c_e6e0,
    0xa301_4314,
    0x4e08_11a1,
    0xf753_7e82,
    0xbd3a_f235,
    0x2ad7_d2bb,
    0xeb86_d391,
];
/// MD5's initial state.
const INITIAL: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
/// Where the hashed bytes start: after the magic and the digest itself.
const HASH_START: usize = 20;
const BLOCK: usize = 64;

/// The checksum of `container`.
///
/// # Panics
///
/// If `container` is shorter than 20 bytes, or 4 GiB or longer (the C++ asserts the first and
/// truncates the size for the second; a root signature's container is far from either).
#[must_use]
pub fn dxbc_hash(container: &[u8]) -> [u8; 16] {
    let bytes = &container[HASH_START..];
    let bit_count = u32::try_from(bytes.len())
        .expect("a root signature's container is below 4 GiB")
        .wrapping_mul(8);
    let magic = (bit_count >> 2) | 1;
    let (body, last) = bytes.split_at(bytes.len() - bytes.len() % BLOCK);
    let mut tail = Vec::with_capacity(2 * BLOCK);
    if last.len() >= 56 {
        // The last bytes and MD5's padding fill one block; a block of the counts follows.
        tail.extend_from_slice(last);
        tail.push(0x80);
        tail.resize(BLOCK, 0);
        tail.extend_from_slice(&bit_count.to_le_bytes());
        tail.resize(2 * BLOCK - 4, 0);
    } else {
        // The bit count first, then the last bytes and the padding, in one block.
        tail.extend_from_slice(&bit_count.to_le_bytes());
        tail.extend_from_slice(last);
        tail.push(0x80);
        tail.resize(BLOCK - 4, 0);
    }
    tail.extend_from_slice(&magic.to_le_bytes());

    let mut state = INITIAL;
    for block in body
        .as_chunks::<BLOCK>()
        .0
        .iter()
        .chain(tail.as_chunks::<BLOCK>().0)
    {
        process(&mut state, block);
    }
    let mut digest = [0; 16];
    for (out, word) in digest.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *out = word.to_le_bytes();
    }
    digest
}

/// MD5's rounds over one 64-byte block (`Hasher::processBlock`).
fn process(state: &mut [u32; 4], block: &[u8; BLOCK]) {
    let words = block.as_chunks::<4>().0;
    // `state[0..4]` are MD5's A, B, C and D.
    let mut s = *state;
    for i in 0..64 {
        let (mix, word) = match i / 16 {
            0 => ((s[1] & s[2]) | (!s[1] & s[3]), i),
            1 => ((s[3] & s[1]) | (!s[3] & s[2]), (5 * i + 1) % 16),
            2 => (s[1] ^ s[2] ^ s[3], (3 * i + 5) % 16),
            _ => (s[2] ^ (s[1] | !s[3]), (7 * i) % 16),
        };
        let mix = mix
            .wrapping_add(s[0])
            .wrapping_add(CONSTANTS[i])
            .wrapping_add(u32::from_le_bytes(words[word]));
        s = [
            s[3],
            s[1].wrapping_add(mix.rotate_left(SHIFTS[i])),
            s[1],
            s[2],
        ];
    }
    for (out, value) in state.iter_mut().zip(s) {
        *out = out.wrapping_add(value);
    }
}
