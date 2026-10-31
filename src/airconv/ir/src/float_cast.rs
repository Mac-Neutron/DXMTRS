//! The float and integer conversions the C++ made by `static_cast` or assignment, each named once.
//!
//! allow: the bounded-cast exception, as macdrv's `bounded_cast` (CONVENTIONS, accepted per-site
//! exception 12). The hull estimators convert a tessellation factor between `float`, `double` and
//! `uint32_t` as `dxbc_converter_ts.cpp` does, on values a guest's shader declares, where
//! `try_from(..).expect(..)` would panic and a translation must not. In range each helper is the
//! C++'s conversion; out of range `as` saturates where the C++'s float-to-integer conversion is
//! undefined (`fcvtzu` on arm64 saturates too).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

/// A `float` assigned to a `uint32_t` (`get_integer_factor`, `dxbc_converter_ts.cpp:84-100`).
///
/// Truncated toward zero; NaN is 0 and an out-of-range value saturates, as `fcvtzu` answers.
#[must_use]
pub const fn f32_to_u32(x: f32) -> u32 {
    x as u32
}

/// A `double` returned as a `uint32_t` (`get_max_potential_workload_count`, `:116-118`).
///
/// As [`f32_to_u32`].
#[must_use]
pub const fn f64_to_u32(x: f64) -> u32 {
    x as u32
}

/// A `uint32_t` assigned to a `float` (`get_final_maxtessfactor`, `:164,172`).
///
/// Rounded to the nearest `float`.
#[must_use]
pub const fn u32_to_f32(x: u32) -> f32 {
    x as f32
}
