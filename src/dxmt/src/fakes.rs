//! The winemetal functions more than one module's tests reach, faked once for the test binary.
//!
//! `NSObject_retain` and `NSObject_release` come from every `Obj`, whichever module's test made
//! it. Each call goes to the hook its thread set with [`log_object_calls`], and nowhere when the
//! thread set none, so a test sees the calls of its own threads only.

use core::cell::Cell;

/// What a hook is handed: the call's name and the object.
pub type Hook = fn(&'static str, u64);

thread_local! {
    /// The calling thread's hook.
    ///
    /// A thread-local, because winemetal's C functions reach no object of the test's.
    static HOOK: Cell<Option<Hook>> = const { Cell::new(None) };
}

/// Sends this thread's `NSObject_retain` and `NSObject_release` calls to `hook`.
pub fn log_object_calls(hook: Hook) {
    HOOK.set(Some(hook));
}

fn call(name: &'static str, object: u64) {
    if let Some(hook) = HOOK.get() {
        hook(name, object);
    }
}

#[unsafe(export_name = "NSObject_retain")]
extern "C-unwind" fn fake_retain(object: u64) {
    call("NSObject_retain", object);
}

#[unsafe(export_name = "NSObject_release")]
extern "C-unwind" fn fake_release(object: u64) {
    call("NSObject_release", object);
}
