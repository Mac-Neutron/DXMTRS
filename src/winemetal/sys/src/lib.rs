//! winemetal's C API for Rust callers.
//!
//! Its functions are declared from `winemetal.h` by `tools/winemetal-layouts.sh --decls`: the
//! ones the PE thunks export, `extern "C"` on Windows and `extern "C-unwind"` natively (a Metal
//! exception passes to the caller), the guest's behind the `guest-vm` feature. The native
//! build's own functions are declared here, each checked against its C prototype by the
//! generated file. A Metal object is a `u64` handle; [`Obj`] owns one and [`Pool`] is an
//! autorelease pool. meson links winemetal.

#[cfg(not(windows))]
use core::ffi::c_void;

mod decls;
mod obj;

#[cfg(not(windows))]
unsafe extern "C-unwind" {
    /// Whether a VM host set the memory hooks (`WMTSetMemoryHooks`): nonzero when it did.
    pub fn WMTMemoryHooked() -> i32;
    /// The window-system hooks a VM host set (`struct WMTWsiHooks`), or null.
    pub fn WMTGetWsiHooks() -> *const c_void;
    /// `CreateEventW` with no attributes and no name: a manual- or auto-reset event.
    pub fn wmt_native_create_event(manual: i32, initial: i32) -> *mut c_void;
    /// `CreateSemaphoreW` with no attributes and no name; null on bad counts.
    pub fn wmt_native_create_semaphore(initial: i32, max: i32) -> *mut c_void;
    /// `ReleaseSemaphore`: nonzero when the count rose by `count`.
    pub fn wmt_native_release_semaphore(handle: *mut c_void, count: i32, previous: *mut i32)
    -> i32;
    /// `WaitForSingleObject` on an event or a semaphore: 0, `WAIT_TIMEOUT` or `WAIT_FAILED`.
    pub fn wmt_native_wait_for_single_object(handle: *mut c_void, milliseconds: u32) -> u32;
    /// `DuplicateHandle` within this process: the same object again, a file's descriptor dup'd.
    pub fn wmt_native_duplicate_handle(
        source: *mut c_void,
        target: *mut *mut c_void,
        options: u32,
    ) -> i32;
    /// `OpenThread` by a thread id of this process (`GetCurrentThreadId`'s); null if none.
    pub fn wmt_native_open_thread(thread_id: u32) -> *mut c_void;
    /// `CloseHandle`: drops a reference; the last frees the object.
    pub fn wmt_native_close_handle(handle: *mut c_void) -> i32;
}

pub use decls::*;
pub use obj::{Obj, Pool};
pub use winemetal_wire as wire;
