//! SHA-1 (`sha1.c` and `sha1_util.cpp`), as the front ends hash bytecode and compile arguments.

use core::fmt;

/// The SHA-1 block size in bytes (`SHA1_BLOCK_LENGTH` in `sha1.h`).
pub const BLOCK_LENGTH: usize = 64;

/// The SHA-1 digest size in bytes (`SHA1_DIGEST_LENGTH` in `sha1.h`).
pub const DIGEST_LENGTH: usize = 20;

const INITIAL_STATE: [u32; 5] = [
    0x6745_2301,
    0xefcd_ab89,
    0x98ba_dcfe,
    0x1032_5476,
    0xc3d2_e1f0,
];

const HEX_NIBBLES: [u8; 16] = *b"0123456789abcdef";

/// A 20-byte SHA-1 digest (`Sha1Digest` in `sha1_util.hpp`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Sha1Digest(pub [u8; DIGEST_LENGTH]);

impl Sha1Digest {
    /// The digest's 20 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIGEST_LENGTH] {
        &self.0
    }

    /// The 40-character lowercase hex string (`Sha1Digest::string()` in `sha1_util.cpp`).
    #[must_use]
    pub fn hex(&self) -> String {
        let mut out = String::with_capacity(2 * DIGEST_LENGTH);
        for &byte in &self.0 {
            out.push(char::from(HEX_NIBBLES[usize::from(byte >> 4)]));
            out.push(char::from(HEX_NIBBLES[usize::from(byte & 0x0f)]));
        }
        out
    }
}

impl fmt::Display for Sha1Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Streaming SHA-1 state (`SHA1_CTX` in `sha1.h`, `Sha1HashState` in `sha1_util.hpp`).
pub struct Sha1 {
    state: [u32; 5],
    count: u64,
    buffer: [u8; BLOCK_LENGTH],
}

impl Default for Sha1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha1 {
    /// A fresh SHA-1 state (`SHA1Init`).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: INITIAL_STATE,
            count: 0,
            buffer: [0; BLOCK_LENGTH],
        }
    }

    /// Hashes `data` in one step (`Sha1HashState::compute`).
    #[must_use]
    pub fn compute(data: &[u8]) -> Sha1Digest {
        let mut h = Self::new();
        h.update(data);
        h.finalize()
    }

    /// Feeds `data` into the running hash (`SHA1Update`).
    pub fn update(&mut self, data: &[u8]) {
        let used = usize::try_from((self.count >> 3) & 0x3f).unwrap_or(0);
        let bits = u64::try_from(data.len()).unwrap_or(0).wrapping_shl(3);
        self.count = self.count.wrapping_add(bits);
        if used + data.len() >= BLOCK_LENGTH {
            let head = BLOCK_LENGTH - used;
            self.buffer[used..BLOCK_LENGTH].copy_from_slice(&data[..head]);
            transform(&mut self.state, &self.buffer);
            let (blocks, tail) = data[head..].as_chunks::<BLOCK_LENGTH>();
            for block in blocks {
                transform(&mut self.state, block);
            }
            self.buffer[..tail.len()].copy_from_slice(tail);
        } else {
            self.buffer[used..used + data.len()].copy_from_slice(data);
        }
    }

    /// Pads the message and produces its 20-byte digest (`SHA1Final`).
    #[must_use]
    pub fn finalize(mut self) -> Sha1Digest {
        let final_count = self.count.to_be_bytes();
        self.update(&[0x80]);
        while (self.count & 0x01f8) != 0x01c0 {
            self.update(&[0]);
        }
        self.update(&final_count);

        let mut digest = [0_u8; DIGEST_LENGTH];
        for (chunk, &word) in digest.as_chunks_mut::<4>().0.iter_mut().zip(&self.state) {
            *chunk = word.to_be_bytes();
        }
        Sha1Digest(digest)
    }
}

/// Hashes one 512-bit block into `state` (`SHA1Transform` in `sha1.c`).
const fn transform(state: &mut [u32; 5], buffer: &[u8; BLOCK_LENGTH]) {
    let mut words = [0_u32; 16];
    let mut idx = 0;
    while idx < 16 {
        let base = idx * 4;
        words[idx] = u32::from_be_bytes([
            buffer[base],
            buffer[base + 1],
            buffer[base + 2],
            buffer[base + 3],
        ]);
        idx += 1;
    }

    let [mut s0, mut s1, mut s2, mut s3, mut s4] = *state;

    let mut round = 0;
    while round < 80 {
        let word = if round < 16 {
            words[round]
        } else {
            let expanded = (words[(round + 13) & 0x0f]
                ^ words[(round + 8) & 0x0f]
                ^ words[(round + 2) & 0x0f]
                ^ words[round & 0x0f])
                .rotate_left(1);
            words[round & 0x0f] = expanded;
            expanded
        };

        let (mix, constant) = if round < 20 {
            ((s1 & (s2 ^ s3)) ^ s3, 0x5a82_7999_u32)
        } else if round < 40 {
            (s1 ^ s2 ^ s3, 0x6ed9_eba1_u32)
        } else if round < 60 {
            (((s1 | s2) & s3) | (s1 & s2), 0x8f1b_bcdc_u32)
        } else {
            (s1 ^ s2 ^ s3, 0xca62_c1d6_u32)
        };

        let next_s0 = s4
            .wrapping_add(mix)
            .wrapping_add(word)
            .wrapping_add(constant)
            .wrapping_add(s0.rotate_left(5));
        s4 = s3;
        s3 = s2;
        s2 = s1.rotate_left(30);
        s1 = s0;
        s0 = next_s0;

        round += 1;
    }

    state[0] = state[0].wrapping_add(s0);
    state[1] = state[1].wrapping_add(s1);
    state[2] = state[2].wrapping_add(s2);
    state[3] = state[3].wrapping_add(s3);
    state[4] = state[4].wrapping_add(s4);
}

const _: () = assert!(size_of::<Sha1Digest>() == DIGEST_LENGTH && align_of::<Sha1Digest>() == 1);

#[cfg(test)]
mod tests;
