//! Poison-ignoring locks, waiting on an address and thread priority, for both front ends.
//!
//! The queue's threads hand chunks to each other through counters they wait on, as the C++'s
//! `std::atomic::wait` and `CpuFence` (`util_cpu_fence.hpp`) do: `WaitOnAddress` on Windows,
//! `os_sync_wait_on_address` natively. Natively a thread's priority stays as it is, as
//! `win32_native.h`'s `SetThreadPriority` leaves it.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU64, Ordering, fence},
};
use std::{
    sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard},
    thread::{self, JoinHandle},
    time::Duration,
};

/// Locks `mutex`, ignoring poison: a panic elsewhere under the lock leaves the data as it was.
///
/// DXMT's DLLs build with `panic = "abort"`, so a mutex is poisoned only in a unit test; the
/// state it guards is still the right one to continue with.
pub fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Read-locks `rwlock`, ignoring poison like [`lock`].
pub fn read<T: ?Sized>(rwlock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    rwlock.read().unwrap_or_else(PoisonError::into_inner)
}

/// Write-locks `rwlock`, ignoring poison like [`lock`].
pub fn write<T: ?Sized>(rwlock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    rwlock.write().unwrap_or_else(PoisonError::into_inner)
}

/// `THREAD_PRIORITY_TIME_CRITICAL`.
#[cfg(windows)]
const THREAD_PRIORITY_TIME_CRITICAL: i32 = 15;

/// `INFINITE`: no timeout.
#[cfg(windows)]
const INFINITE: u32 = u32::MAX;

#[cfg(windows)]
unsafe extern "system" {
    fn WaitOnAddress(
        address: *const c_void,
        compare: *const c_void,
        size: usize,
        milliseconds: u32,
    ) -> i32;
    fn WakeByAddressAll(address: *const c_void);
    fn GetCurrentThread() -> *mut c_void;
    fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
}

#[cfg(not(windows))]
unsafe extern "C" {
    fn os_sync_wait_on_address(address: *mut c_void, value: u64, size: usize, flags: u32) -> i32;
    fn os_sync_wake_by_address_all(address: *mut c_void, size: usize, flags: u32) -> i32;
}

/// Blocks while `atomic` holds `current`.
///
/// A wait can end early, as `std::atomic::wait`'s can: callers load the value again and decide.
#[cfg(windows)]
pub fn wait(atomic: &AtomicU64, current: u64) {
    // SAFETY: both addresses hold eight readable bytes for the call's length.
    unsafe {
        WaitOnAddress(
            atomic.as_ptr().cast(),
            (&raw const current).cast(),
            size_of::<u64>(),
            INFINITE,
        )
    };
}

/// Blocks while `atomic` holds `current`.
///
/// A wait can end early, as `std::atomic::wait`'s can: callers load the value again and decide.
#[cfg(not(windows))]
pub fn wait(atomic: &AtomicU64, current: u64) {
    // SAFETY: the address holds eight bytes for the call's length; no flags: within this process.
    unsafe { os_sync_wait_on_address(atomic.as_ptr().cast(), current, size_of::<u64>(), 0) };
}

/// Wakes every thread waiting on `atomic`.
#[cfg(windows)]
pub fn wake_all(atomic: &AtomicU64) {
    // SAFETY: any address may be woken; no memory is accessed through it.
    unsafe { WakeByAddressAll(atomic.as_ptr().cast()) };
}

/// Wakes every thread waiting on `atomic`.
#[cfg(not(windows))]
pub fn wake_all(atomic: &AtomicU64) {
    // SAFETY: the address and size are a waiter's; no flags: within this process.
    unsafe { os_sync_wake_by_address_all(atomic.as_ptr().cast(), size_of::<u64>(), 0) };
}

/// Raises the calling thread to time-critical priority, as the queue's C++ threads run.
#[cfg(windows)]
pub fn set_time_critical() {
    // SAFETY: no precondition: the calling thread's pseudo-handle.
    let thread = unsafe { GetCurrentThread() };
    // SAFETY: the pseudo-handle names the calling thread.
    unsafe { SetThreadPriority(thread, THREAD_PRIORITY_TIME_CRITICAL) };
}

/// Raises the calling thread to time-critical priority: natively nothing changes.
#[cfg(not(windows))]
pub const fn set_time_critical() {}

/// Waits for `handle`'s thread to end, polling, then drops the handle.
///
/// Never `JoinHandle::join`: Wine can invalidate a thread handle held for a long session, and
/// `join` panics on the failed wait.
pub fn wait_until_finished<T>(handle: JoinHandle<T>) {
    while !handle.is_finished() {
        thread::sleep(Duration::from_millis(1));
    }
    // The thread's last writes come before it releases its result; this makes them visible here.
    fence(Ordering::Acquire);
    drop(handle);
}

/// A value that only rises, which threads wait to reach (`util_cpu_fence.hpp`).
pub struct CpuFence(AtomicU64);

impl CpuFence {
    /// A fence at `value`.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(AtomicU64::new(value))
    }

    /// Blocks until the fence reaches `value`.
    pub fn wait(&self, value: u64) {
        loop {
            let current = self.0.load(Ordering::Acquire);
            if current >= value {
                return;
            }
            wait(&self.0, current);
        }
    }

    /// Raises the fence to `value` and wakes its waiters; a lower value changes nothing.
    pub fn signal(&self, value: u64) {
        if self.0.fetch_max(value, Ordering::Release) < value {
            wake_all(&self.0);
        }
    }

    /// The value the fence has reached.
    #[must_use]
    pub fn signaled(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests;
