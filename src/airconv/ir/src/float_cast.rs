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
