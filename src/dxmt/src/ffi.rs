//! What the core's C exports share: the C-or-C-unwind definition and the pointers they take.
//!
//! On Windows an export is `extern "C"` (`production` aborts on a panic). Natively it is
//! `extern "C-unwind"` behind [`AbortOnPanic`]: the native static libraries build with
//! `production-unwind`, so an Objective-C exception winemetal raises still reaches the VM host's
//! `catch`, while a Rust panic aborts as winemetal's own handlers do.

use std::{
    borrow::Cow,
    ffi::{CStr, c_char, c_void},
};

/// Aborts the process when dropped during a Rust panic; a foreign unwind passes through.
pub struct AbortOnPanic;

impl Drop for AbortOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::process::abort();
        }
    }
}

/// The text of a C string argument, `None` for null.
///
/// # Safety
///
/// `text` is null or points to a NUL-terminated string that outlives `'a`.
#[must_use]
pub unsafe fn c_str<'a>(text: *const c_char) -> Option<Cow<'a, str>> {
    // SAFETY: the caller's contract: null or a live NUL-terminated string.
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_string_lossy())
}

/// The bytes of a pointer and length argument: empty for length 0, `None` for null with a length.
///
/// # Safety
///
/// Unless `len` is 0, `data` is null or points to `len` readable bytes that outlive `'a`.
#[must_use]
pub unsafe fn c_bytes<'a>(data: *const c_void, len: usize) -> Option<&'a [u8]> {
    if len == 0 {
        return Some(&[]);
    }
    // SAFETY: the caller's contract: `len` readable bytes at a non-null `data`.
    (!data.is_null()).then(|| unsafe { core::slice::from_raw_parts(data.cast::<u8>(), len) })
}

/// Defines one function as a C export under its own name, for the ABI and guard of the target.
///
/// `seam_export!(fn name(args) -> ret { body })`, `const fn` for one whose body can be (const on
/// Windows only, since natively it takes the guard), or `unsafe fn` for one whose caller upholds
/// a contract: `extern "C"` on Windows, natively `extern "C-unwind"` with an [`AbortOnPanic`]
/// taken first. Each seam's generated `abi.rs` pins the signature to its C header.
#[macro_export]
macro_rules! seam_export {
    ($(#[$attr:meta])* const fn $name:ident($($arg:ident: $ty:ty),*) $(-> $ret:ty)? $body:block) => {
        $(#[$attr])*
        #[cfg(windows)]
        #[unsafe(no_mangle)]
        const extern "C" fn $name($($arg: $ty),*) $(-> $ret)? $body

        $(#[$attr])*
        #[cfg(not(windows))]
        #[unsafe(no_mangle)]
        extern "C-unwind" fn $name($($arg: $ty),*) $(-> $ret)? {
            let _guard = $crate::ffi::AbortOnPanic;
            $body
        }
    };
    ($(#[$attr:meta])* fn $name:ident($($arg:ident: $ty:ty),*) $(-> $ret:ty)? $body:block) => {
        $(#[$attr])*
        #[cfg(windows)]
        #[unsafe(no_mangle)]
        extern "C" fn $name($($arg: $ty),*) $(-> $ret)? $body

        $(#[$attr])*
        #[cfg(not(windows))]
        #[unsafe(no_mangle)]
        extern "C-unwind" fn $name($($arg: $ty),*) $(-> $ret)? {
            let _guard = $crate::ffi::AbortOnPanic;
            $body
        }
    };
    ($(#[$attr:meta])* unsafe fn $name:ident($($arg:ident: $ty:ty),*) $(-> $ret:ty)? $body:block) => {
        $(#[$attr])*
        #[cfg(windows)]
        #[unsafe(no_mangle)]
        unsafe extern "C" fn $name($($arg: $ty),*) $(-> $ret)? $body

        $(#[$attr])*
        #[cfg(not(windows))]
        #[unsafe(no_mangle)]
        unsafe extern "C-unwind" fn $name($($arg: $ty),*) $(-> $ret)? {
            let _guard = $crate::ffi::AbortOnPanic;
            $body
        }
    };
}

// The test runs its own binary again and reads the signal: it runs on the host.
#[cfg(all(test, target_os = "macos"))]
mod tests;
