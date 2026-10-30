//! The Unix-call boundary: the handler type, the panic guard, params, PE memory and statuses.
//!
//! A handler gets the address of the params block its caller built (the PE thunk, or the VM
//! host's forwarder) and answers an `NTSTATUS`. The pointers inside a block are the caller's
//! `WMTMemoryPointer`s ([`MemPtr`]); the helpers here turn them into references and slices, `None`
//! for a null pointer, and leave what a null means to each handler, which does what the C did.

use core::ffi::{CStr, c_char, c_void};

use winemetal_wire::MemPtr;

use crate::float_cast::u64_to_usize;

/// A Unix-call handler: the params block in, an `NTSTATUS` out.
pub type Handler = unsafe extern "C-unwind" fn(*mut c_void) -> i32;

/// `STATUS_SUCCESS`.
pub const STATUS_SUCCESS: i32 = 0;

/// `STATUS_UNSUCCESSFUL`.
pub const STATUS_UNSUCCESSFUL: i32 = 0xC000_0001_u32.cast_signed();

/// `STATUS_NOT_IMPLEMENTED`.
pub const STATUS_NOT_IMPLEMENTED: i32 = 0xC000_0002_u32.cast_signed();

/// Aborts the process when a Rust panic unwinds through the frame that holds it.
///
/// Every handler and every export under a C name takes one first: `let _guard = AbortOnPanic;`.
/// A Rust panic then aborts, as `panic = "abort"` did, and never unwinds into the C or C++ caller
/// (the VM host's `catch (...)` around native calls). An Objective-C exception is a foreign
/// unwind, which does not set [`std::thread::panicking`]: it still leaves the handler for the
/// caller's `@catch`, as it left the C's.
pub struct AbortOnPanic;

impl Drop for AbortOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::process::abort();
        }
    }
}

/// The params block a handler was called with, as `T`.
///
/// # Safety
///
/// `args` points to a live, aligned `T` that nothing else touches during the call: the block the
/// caller built for this slot.
#[must_use]
pub unsafe fn params<'a, T>(args: *mut c_void) -> &'a mut T {
    // SAFETY: the caller's contract.
    unsafe { &mut *args.cast::<T>() }
}

/// The `T` a `WMTMemoryPointer` points to, `None` for null.
///
/// # Safety
///
/// A non-null `p` points to a live, aligned `T` that nothing writes during `'a`.
#[must_use]
pub const unsafe fn mem_ref<'a, T>(p: MemPtr) -> Option<&'a T> {
    // SAFETY: the caller's contract.
    unsafe { ptr::<T>(p).as_ref() }
}

/// The `T` a `WMTMemoryPointer` points to, writable, `None` for null.
///
/// # Safety
///
/// A non-null `p` points to a live, aligned `T` that nothing else reads or writes during `'a`.
#[must_use]
pub const unsafe fn mem_mut<'a, T>(p: MemPtr) -> Option<&'a mut T> {
    // SAFETY: the caller's contract.
    unsafe { ptr::<T>(p).as_mut() }
}

/// The `count` `T`s a `WMTMemoryPointer` points to.
///
/// Empty for a count of 0, whatever the pointer; `None` for a null pointer with a count above 0
/// (the caller does what the C did with that pointer).
///
/// # Safety
///
/// A non-null `p` with a count above 0 points to `count` live, aligned `T`s that nothing writes
/// during `'a`.
#[must_use]
pub const unsafe fn mem_slice<'a, T>(p: MemPtr, count: u64) -> Option<&'a [T]> {
    if count == 0 {
        return Some(&[]);
    }
    let p = ptr::<T>(p);
    if p.is_null() {
        return None;
    }
    // SAFETY: the caller's contract; `count` `T`s fit in the address space they live in.
    Some(unsafe { core::slice::from_raw_parts(p, u64_to_usize(count)) })
}

/// The NUL-terminated string a `WMTMemoryPointer` points to, `None` for null.
///
/// # Safety
///
/// A non-null `p` points to a NUL-terminated string that nothing writes during `'a`.
#[must_use]
pub const unsafe fn mem_cstr<'a>(p: MemPtr) -> Option<&'a CStr> {
    let p = ptr::<c_char>(p);
    if p.is_null() {
        return None;
    }
    // SAFETY: the caller's contract.
    Some(unsafe { CStr::from_ptr(p) })
}

/// The address a `WMTMemoryPointer` holds, as a pointer.
const fn ptr<T>(p: MemPtr) -> *mut T {
    core::ptr::with_exposed_provenance_mut(u64_to_usize(p.ptr))
}

#[cfg(test)]
mod tests;
