//! C++'s `static_cast` between floats and integers, for NGX's parameter map and render sizes.
//!
//! allow: the bounded-cast exception, as macdrv's `bounded_cast` (CONVENTIONS, accepted per-site
//! exception 9). NGX's parameter conversions and DXMT's render-size math are C++ casts, a contract
//! with the game's calls; the structural alternative re-implements `as` bit by bit for the same
//! results. In range each is C++'s cast; out of range `as` saturates where C++ is undefined.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]

/// `static_cast<int>(float)`: toward zero.
pub const fn f32_to_i32(v: f32) -> i32 {
    v as i32
}

/// `static_cast<unsigned>(float)`: toward zero.
pub const fn f32_to_u32(v: f32) -> u32 {
    v as u32
}

/// `(float)double`: to nearest.
pub const fn f64_to_f32(v: f64) -> f32 {
    v as f32
}

/// `static_cast<float>(unsigned)`: to nearest.
pub const fn u32_to_f32(v: u32) -> f32 {
    v as f32
}

/// `static_cast<float>(int)`: to nearest.
pub const fn i32_to_f32(v: i32) -> f32 {
    v as f32
}
