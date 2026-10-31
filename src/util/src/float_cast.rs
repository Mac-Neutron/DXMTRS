//! C++'s `static_cast` between floats and integers, for DXMT's Rust DLLs.
//!
//! allow: the bounded-cast exception, as macdrv's `bounded_cast` (CONVENTIONS, accepted
//! per-site exception 9). NGX's parameter conversions, DXMT's render-size math and NVAPI's
//! scaled values are C++ casts, a contract with the game's calls; the structural alternative
//! re-implements `as` bit by bit for the same results. In range each is C++'s cast; out of
//! range `as` saturates where C++ is undefined.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

/// `static_cast<int>(float)`: toward zero.
#[must_use]
pub const fn f32_to_i32(v: f32) -> i32 {
    v as i32
}

/// `static_cast<unsigned>(float)`: toward zero.
#[must_use]
pub const fn f32_to_u32(v: f32) -> u32 {
    v as u32
}

/// `static_cast<uint16_t>(float)`: toward zero; out of range saturates where C++ is undefined.
#[must_use]
pub const fn f32_to_u16(v: f32) -> u16 {
    v as u16
}

/// `(float)double`: to nearest.
#[must_use]
pub const fn f64_to_f32(v: f64) -> f32 {
    v as f32
}

/// `double(uint64_t)`: to nearest.
#[must_use]
pub const fn u64_to_f64(v: u64) -> f64 {
    v as f64
}

/// `static_cast<float>(unsigned)`: to nearest.
#[must_use]
pub const fn u32_to_f32(v: u32) -> f32 {
    v as f32
}

/// `static_cast<float>(int)`: to nearest.
#[must_use]
pub const fn i32_to_f32(v: i32) -> f32 {
    v as f32
}

#[cfg(test)]
mod tests;
