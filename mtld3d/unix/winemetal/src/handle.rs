//! Objective-C objects as winemetal's 64-bit handles: inputs borrowed, outputs +1 or +0.
//!
//! A handle is an object's address. An input is borrowed for the call ([`obj`]): never released,
//! never wrapped in a `Retained`. An output keeps the C's ownership per field:
//! - +1, an object the handler made or retained: [`ret_owned`];
//! - +0, an object the C wrote straight from a send, autoreleased or owned elsewhere:
//!   [`ret_passthrough`] of the raw pointer a typed re-declaration returns (`-> *mut T`, on which
//!   objc2 does no memory management), so the reference counts and the autorelease pool see what
//!   they saw with the C;
//! - +0, a reference the Rust already holds (an input, an object it keeps): [`ret_borrowed`].

use core::ffi::CStr;

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};

use crate::float_cast::u64_to_usize;

/// The object a handle names, `None` for the null handle.
///
/// A `None` receiver answers what the C's message to nil answered (zeros), and a `None` argument
/// is sent as nil.
///
/// # Safety
///
/// A non-zero `h` is the address of a live `T` for `'a`.
#[must_use]
pub const unsafe fn obj<'a, T: Message>(h: u64) -> Option<&'a T> {
    // SAFETY: the caller's contract.
    unsafe { core::ptr::with_exposed_provenance::<T>(u64_to_usize(h)).as_ref() }
}

/// A +1 output: the reference `o` holds goes to the caller, 0 for `None`.
#[must_use]
pub fn ret_owned<T: Message>(o: Option<Retained<T>>) -> u64 {
    o.map_or(0, |o| addr(Retained::into_raw(o)))
}

/// A +0 output the C wrote straight from a send: the pointer as is.
///
/// No retain, release or autorelease. `p` comes from a typed local re-declaration whose method
/// returns `*mut T`, so an autoreleased result stays in the pool the C's stayed in and an object
/// owned elsewhere keeps its count.
#[must_use]
pub fn ret_passthrough(p: *mut AnyObject) -> u64 {
    addr(p)
}

/// A +0 output of a reference the Rust already holds (an input, or an object it keeps).
///
/// Never a send's result: a `Retained` got from a send and dropped after this frees an object
/// the caller still holds (a fresh autoreleased command buffer); such results go through
/// [`ret_passthrough`].
#[must_use]
pub fn ret_borrowed<T: Message>(o: Option<&T>) -> u64 {
    o.map_or(0, |o| addr(core::ptr::from_ref(o)))
}

/// The class `name` names, `None` when no such class is loaded: the C's `objc_lookUpClass`.
///
/// For a class the C looks up by name, where objc2's cached lookup of a declared class would
/// panic on a missing one.
#[must_use]
pub fn lookup_class(name: &CStr) -> Option<&'static AnyClass> {
    AnyClass::get(name)
}

/// An object's address as a handle.
fn addr<T>(p: *const T) -> u64 {
    p.expose_provenance() as u64
}

#[cfg(test)]
mod tests;
