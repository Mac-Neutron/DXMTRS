//! `ID3D11Multithread` tear-off and `DeviceMutex` toggleable recursive lock.

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use std::sync::{Condvar, Mutex, PoisonError};

use dxmt_util::sync::lock;

use crate::{abi::vtbl::ID3D11MultithreadVtbl, com::TearOff};

#[cfg(test)]
mod tests;

std::thread_local! {
    static THREAD_TOKEN: u8 = const { 0 };
}

/// Returns a non-zero per-thread identifier stable for the life of the calling thread.
fn current_thread_id() -> usize {
    THREAD_TOKEN.with(|token| ptr::from_ref(token).addr())
}

/// Recursive device lock controlled by `ID3D11Multithread` (`d3d11_multithread.hpp`).
///
/// When `protected` is `false`, `lock` and `unlock` are no-ops. When `protected`
/// is `true`, the lock is recursive and owned by the calling thread; a waiter
/// parks on its condition variable instead of spinning, so a long Present wait
/// does not burn the CPU of every other immediate-context thread.
pub struct DeviceMutex {
    protected: AtomicBool,
    state: Mutex<LockState>,
    released: Condvar,
}

#[derive(Default)]
struct LockState {
    owner: usize,
    depth: u32,
}

impl Default for DeviceMutex {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceMutex {
    /// Creates an unprotected `DeviceMutex`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            protected: AtomicBool::new(false),
            state: Mutex::new(LockState::default()),
            released: Condvar::new(),
        }
    }

    /// Acquires the lock if multithread protection is enabled.
    pub fn lock(&self) {
        if !self.protected.load(Ordering::Acquire) {
            return;
        }
        let me = current_thread_id();
        let mut state = lock(&self.state);
        loop {
            if !self.protected.load(Ordering::Acquire) {
                return;
            }
            if state.owner == me {
                state.depth = state.depth.saturating_add(1);
                return;
            }
            if state.owner == 0 {
                state.owner = me;
                state.depth = 1;
                return;
            }
            state = self
                .released
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Releases one level of ownership if multithread protection is enabled.
    pub fn unlock(&self) {
        if !self.protected.load(Ordering::Acquire) {
            return;
        }
        let me = current_thread_id();
        let mut state = lock(&self.state);
        if state.owner != me {
            return;
        }
        if state.depth > 1 {
            state.depth -= 1;
            return;
        }
        state.owner = 0;
        state.depth = 0;
        drop(state);
        self.released.notify_one();
    }

    /// Acquires the lock and returns an RAII guard that unlocks on drop.
    #[must_use]
    pub fn lock_guard(&self) -> DeviceLockGuard<'_> {
        self.lock();
        DeviceLockGuard { mutex: self }
    }

    /// Enables or disables multithread protection, returning the previous state.
    pub fn set_protected(&self, protected: bool) -> bool {
        let prev = self.protected.swap(protected, Ordering::AcqRel);
        if prev && !protected {
            let mut state = lock(&self.state);
            state.owner = 0;
            state.depth = 0;
            drop(state);
            self.released.notify_all();
        }
        prev
    }

    /// Returns whether multithread protection is currently enabled.
    #[must_use]
    pub fn get_protected(&self) -> bool {
        self.protected.load(Ordering::Acquire)
    }
}

/// RAII guard for [`DeviceMutex`].
pub struct DeviceLockGuard<'a> {
    mutex: &'a DeviceMutex,
}

impl Drop for DeviceLockGuard<'_> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

/// `ID3D11Multithread` tear-off subobject embedded in the device.
///
/// The first two fields match [`TearOff<ID3D11MultithreadVtbl>`] so `IUnknown`
/// calls forward directly to the outer COM container while the four
/// `ID3D11Multithread` slots operate on `mutex`.
#[repr(C)]
pub struct Multithread {
    /// Tear-off header (`vtbl` + `outer` COM pointer).
    pub tear_off: TearOff<ID3D11MultithreadVtbl>,
    /// Pointer to the owning device's [`DeviceMutex`].
    pub mutex: *const DeviceMutex,
}

impl Multithread {
    /// Creates an unlinked `Multithread` tear-off.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            tear_off: TearOff::new(&VTBL, ptr::null_mut()),
            mutex: ptr::null(),
        }
    }

    /// Links the tear-off to its outer COM container and target [`DeviceMutex`].
    pub const fn init(&mut self, outer: *mut c_void, mutex: *const DeviceMutex) {
        self.tear_off.outer = outer;
        self.mutex = mutex;
    }

    /// Returns the `ID3D11Multithread*` interface pointer for this subobject.
    #[must_use]
    pub const fn as_iface(&mut self) -> *mut c_void {
        (&raw mut self.tear_off).cast()
    }
}

impl Default for Multithread {
    fn default() -> Self {
        Self::new()
    }
}

/// Vtable for `ID3D11Multithread` (`ID3D10Multithread`).
pub static VTBL: ID3D11MultithreadVtbl = ID3D11MultithreadVtbl {
    query_interface: TearOff::<()>::query_interface,
    add_ref: TearOff::<()>::add_ref,
    release: TearOff::<()>::release,
    enter,
    leave,
    set_multithread_protected,
    get_multithread_protected,
};

unsafe extern "system-unwind" fn enter(this: *mut c_void) {
    if this.is_null() {
        return;
    }
    // SAFETY: `this` points to a `Multithread` tear-off whose `mutex` was set at init.
    let mt = unsafe { &*this.cast::<Multithread>() };
    if !mt.mutex.is_null() {
        // SAFETY: `mt.mutex` points to the enclosing device's `DeviceMutex`.
        unsafe { (*mt.mutex).lock() };
    }
}

unsafe extern "system-unwind" fn leave(this: *mut c_void) {
    if this.is_null() {
        return;
    }
    // SAFETY: `this` points to a `Multithread` tear-off whose `mutex` was set at init.
    let mt = unsafe { &*this.cast::<Multithread>() };
    if !mt.mutex.is_null() {
        // SAFETY: `mt.mutex` points to the enclosing device's `DeviceMutex`.
        unsafe { (*mt.mutex).unlock() };
    }
}

unsafe extern "system-unwind" fn set_multithread_protected(
    this: *mut c_void,
    mt_protect: i32,
) -> i32 {
    if this.is_null() {
        return 0;
    }
    // SAFETY: `this` points to a `Multithread` tear-off whose `mutex` was set at init.
    let mt = unsafe { &*this.cast::<Multithread>() };
    if mt.mutex.is_null() {
        return 0;
    }
    // SAFETY: `mt.mutex` points to the enclosing device's `DeviceMutex`.
    let prev = unsafe { (*mt.mutex).set_protected(mt_protect != 0) };
    i32::from(prev)
}

unsafe extern "system-unwind" fn get_multithread_protected(this: *mut c_void) -> i32 {
    if this.is_null() {
        return 0;
    }
    // SAFETY: `this` points to a `Multithread` tear-off whose `mutex` was set at init.
    let mt = unsafe { &*this.cast::<Multithread>() };
    if mt.mutex.is_null() {
        return 0;
    }
    // SAFETY: `mt.mutex` points to the enclosing device's `DeviceMutex`.
    let cur = unsafe { (*mt.mutex).get_protected() };
    i32::from(cur)
}
