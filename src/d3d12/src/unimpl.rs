//! Unimplemented-method reporting (`IMPLEMENT_ME` in `d3d12_device.hpp:37-50`, P7).
//!
//! `not_implemented!("<Interface>::<Method>")` logs `<file>:<method>(<line>) is not implemented.`
//! at error level (`Logger::err` writes immediately to the OS file descriptor); on PE it then calls
//! the C runtime's `abort()` (matching `IMPLEMENT_ME_END`), and natively it sets the thread's
//! `wmt_native_not_implemented` flag and returns a fallback value (`E_NOTIMPL` for `Hresult`, `()`
//! for `void`, or a supplied fallback).

#[cfg(test)]
use core::cell::RefCell;

use crate::com::{E_NOTIMPL, Hresult};

#[cfg(test)]
std::thread_local! {
    static LAST_REPORTED: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Returns and clears the most recent `method` passed to [`report_with`] on the calling thread.
#[cfg(test)]
#[must_use]
pub fn take_last_reported() -> Option<String> {
    LAST_REPORTED.with(|slot| slot.borrow_mut().take())
}

#[cfg(windows)]
unsafe extern "C" {
    /// C runtime `abort()`, matching `IMPLEMENT_ME_END` (`d3d12_device.hpp:38`).
    fn abort() -> !;
}

/// Fallback return value for `not_implemented!` on the native build (P7).
pub trait NotImplementedReturn {
    /// Fallback value returned after setting `wmt_native_not_implemented()`.
    #[must_use]
    fn not_implemented_return() -> Self;
}

impl NotImplementedReturn for () {
    fn not_implemented_return() -> Self {}
}

impl NotImplementedReturn for Hresult {
    fn not_implemented_return() -> Self {
        E_NOTIMPL
    }
}

impl NotImplementedReturn for u32 {
    fn not_implemented_return() -> Self {
        0
    }
}

impl NotImplementedReturn for u64 {
    fn not_implemented_return() -> Self {
        0
    }
}

impl NotImplementedReturn for usize {
    fn not_implemented_return() -> Self {
        0
    }
}

impl NotImplementedReturn for bool {
    fn not_implemented_return() -> Self {
        false
    }
}

impl<T> NotImplementedReturn for *mut T {
    fn not_implemented_return() -> Self {
        core::ptr::null_mut()
    }
}

impl<T> NotImplementedReturn for *const T {
    fn not_implemented_return() -> Self {
        core::ptr::null()
    }
}

impl<T> NotImplementedReturn for Option<T> {
    fn not_implemented_return() -> Self {
        None
    }
}

impl<T> NotImplementedReturn for Result<T, Hresult> {
    fn not_implemented_return() -> Self {
        Err(E_NOTIMPL)
    }
}

/// Logs `<file>:<method>(<line>) is not implemented.` and aborts via the CRT (`windows`).
#[cfg(windows)]
pub fn report_with<R>(file: &str, method: &str, line: u32, _fallback: R) -> R {
    crate::err!("{file}:{method}({line}) is not implemented.");
    // SAFETY: CRT `abort()` terminates the process as C++ `IMPLEMENT_ME` does.
    unsafe { abort() }
}

/// Logs `<file>:<method>(<line>) is not implemented.` and sets `wmt_native_not_implemented()`.
#[cfg(not(windows))]
pub fn report_with<R>(file: &str, method: &str, line: u32, fallback: R) -> R {
    #[cfg(test)]
    LAST_REPORTED.with(|slot| *slot.borrow_mut() = Some(method.to_owned()));
    crate::err!("{file}:{method}({line}) is not implemented.");
    // SAFETY: sets the calling thread's thread-local flag in `winemetal.dylib`.
    unsafe { winemetal_sys::wmt_native_not_implemented() };
    fallback
}

/// Logs `<file>:<method>(<line>) is not implemented.` and returns `<R as NotImplementedReturn>`.
#[must_use]
pub fn report<R: NotImplementedReturn>(file: &str, method: &str, line: u32) -> R {
    report_with(file, method, line, R::not_implemented_return())
}

/// Reports an unimplemented method (`IMPLEMENT_ME`, P7).
#[macro_export]
macro_rules! not_implemented {
    ($method:literal) => {
        $crate::unimpl::report(file!(), $method, line!())
    };
    ($method:literal, $fallback:expr) => {
        $crate::unimpl::report_with(file!(), $method, line!(), $fallback)
    };
}

pub use crate::not_implemented as implement_me;
