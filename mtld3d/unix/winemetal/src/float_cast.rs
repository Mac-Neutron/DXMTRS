//! The float and narrowing conversions winemetal's C made implicitly, each named once.
//!
//! allow: the bounded-cast exception, as macdrv's `bounded_cast` (CONVENTIONS, accepted per-site
//! exception 10). Each helper is a conversion the C made by assignment or argument passing
//! (`clang -Wconversion` over `winemetal_unix.c` and `cache.c` lists them), on values a guest or
//! Metal hands over, where `try_from(..).expect(..)` would panic and the handlers must not. In
//! range each is the C's conversion; out of range `as` truncates as the C does for integers and
//! saturates where the C's float conversion is undefined (`fcvtzu` on arm64 saturates too).
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

/// `NSUInteger` to `uint32_t`, keeping the low 32 bits.
///
/// A descriptor's size or count, a size or alignment, a Metal enum written to a `WMT` one.
#[must_use]
pub const fn usize_to_u32(v: usize) -> u32 {
    v as u32
}

/// `uint64_t` to `uint32_t`, keeping the low 32 bits.
///
/// A handle or a `uint64_t` field taken as a `CGDirectDisplayID`.
#[must_use]
pub const fn u64_to_u32(v: u64) -> u32 {
    v as u32
}

/// `uint64_t` to `NSUInteger`, and an address to a pointer.
///
/// The same 64 bits on the hosts winemetal builds for.
#[must_use]
pub const fn u64_to_usize(v: u64) -> usize {
    v as usize
}

/// `int` to `size_t` (a SQLite byte count handed to libdispatch): sign-extended, as the C.
#[must_use]
pub const fn i32_to_usize(v: i32) -> usize {
    v as usize
}

/// `double` (`CGFloat`) to `float`: to nearest.
#[must_use]
pub const fn f64_to_f32(v: f64) -> f32 {
    v as f32
}

/// `(uint64_t)double`: toward zero (a command buffer's time in nanoseconds).
#[must_use]
pub const fn f64_to_u64(v: f64) -> u64 {
    v as u64
}
