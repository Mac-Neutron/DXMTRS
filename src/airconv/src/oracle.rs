//! The type oracle: every value's type checked as it is made.
//!
//! Built only with the feature `oracle` (the tests and the lane tools, never meson):
//! [`check()`], at each value the builder makes, checks that the `Ty` attached to it lowers to the
//! value's own LLVM type, or records `oracle: <op> made <llvm type>, Ty says <ty>` as the
//! context's first error ([`Context::take_error`] answers `LLVM error: oracle: …`).

use core::ffi::{CStr, c_char};

use crate::llvm::{Context, Val, sys};

/// The failure text of a value whose `Ty` does not lower to its LLVM type; `None` when it does.
///
/// `v` is what the builder of `cx` answered for `op`, with the type it attached.
#[must_use]
pub fn check(cx: &Context, op: &str, v: Val<'_>) -> Option<Vec<u8>> {
    let lowered = cx.tcx().lower(v.ty());
    let made = type_of(v.raw());
    (made != lowered).then(|| {
        [
            &b"oracle: "[..],
            op.as_bytes(),
            b" made ",
            &type_text(made),
            b", Ty says ",
            &type_text(lowered),
        ]
        .concat()
    })
}

fn type_of(v: sys::ValueRef) -> sys::TypeRef {
    // SAFETY: a live value (each caller's, of the module being read).
    unsafe { sys::type_of(v) }
}

/// The type's text, `?` for none.
fn type_text(ty: sys::TypeRef) -> Vec<u8> {
    if ty.is_null() {
        return b"?".to_vec();
    }
    // SAFETY: a live type.
    let text = unsafe { sys::print_type_to_string(ty) };
    // SAFETY: the message just made.
    unsafe { message(text) }
}

/// The bytes of a message the C API made, disposed of here.
///
/// # Safety
///
/// `text` is a NUL-terminated message the C API allocated, used by nothing else.
unsafe fn message(text: *mut c_char) -> Vec<u8> {
    // SAFETY: a NUL-terminated message (the caller's contract).
    let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
    // SAFETY: the C API's message, freed once.
    unsafe { sys::dispose_message(text) };
    bytes
}

#[cfg(test)]
mod tests;
