//! The native build's Win32 calls: `winemetal.dylib`'s `wmt_native_*` exports.
//!
//! A second implementation of `win32_native_event.h`'s handle functions and of the
//! `win32_native.h` calls the Rust front ends make, on the same `struct mn_win32_handle`: what
//! one side makes, the other sets, waits on, duplicates and closes, so each call here does what
//! the header's inline function does. Objects come from `calloc` and go to `free`, as the
//! header's do. One process: a duplicate's source and target process are this one. These calls
//! set no last error: the header's is a cell of each translation unit, which no caller of the
//! exports reads.

use core::{
    cell::{Cell, UnsafeCell},
    ffi::{c_int, c_void},
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicI32, Ordering},
};

use crate::{ffi::AbortOnPanic, float_cast::usize_to_u32};

mod abi;

/// `MN_WIN32_EVENT`: an event.
pub const MN_WIN32_EVENT: u32 = 0x6e65_7665;

/// `MN_WIN32_SEMAPHORE`: a semaphore.
pub const MN_WIN32_SEMAPHORE: u32 = 0x6d65_7373;

/// `MN_WIN32_FILE`: a file descriptor.
pub const MN_WIN32_FILE: u32 = 0x656c_6966;

/// `MN_WIN32_THREAD`: a thread of this process, by its id.
pub const MN_WIN32_THREAD: u32 = 0x6472_6874;

/// `MN_WIN32_RELAY`: an event whose set calls a host's function instead of waking waiters.
pub const MN_WIN32_RELAY: u32 = 0x7961_6c72;

/// `INFINITE`: a wait with no deadline.
const INFINITE: u32 = 0xffff_ffff;

/// `WAIT_OBJECT_0`: the wait took the object.
const WAIT_OBJECT_0: u32 = 0;

/// `WAIT_TIMEOUT`: the time ran out first.
const WAIT_TIMEOUT: u32 = 0x102;

/// `WAIT_FAILED`: nothing to wait on.
const WAIT_FAILED: u32 = 0xffff_ffff;

/// `DUPLICATE_CLOSE_SOURCE`: the duplicate closes its source.
const DUPLICATE_CLOSE_SOURCE: u32 = 1;

/// `PROC_PIDTHREADID64INFO` (`<sys/proc_info.h>`): a thread's information by its 64-bit id.
///
/// The `libc` crate does not name it; its `PROC_PIDTHREADINFO` (5) is the query by the kernel's
/// thread handle, which a thread id does not answer.
const PROC_PIDTHREADID64INFO: c_int = 15;

/// `sizeof(struct proc_threadinfo)`: `proc_pidinfo`'s buffer and its whole answer.
const THREAD_INFO_SIZE: c_int = usize_to_u32(size_of::<libc::proc_threadinfo>()).cast_signed();

thread_local! {
    /// The calling thread's not-implemented flag, which `MTLD3DNotImplemented` reads and clears.
    ///
    /// Per-thread state with no object to hold it: a front end sets it on the thread of a call
    /// that reached something not implemented, and a C host reads it by `dlsym` on that thread
    /// after the call, with nothing but the thread in common. Const-initialised, with no
    /// destructor: every thread reads false until it sets it, at any point of its life.
    static NOT_IMPLEMENTED: Cell<bool> = const { Cell::new(false) };
}

/// `MTLD3DNotImplemented`, by its C name.
pub use self::mtld3d_not_implemented as MTLD3DNotImplemented;

/// `struct mn_win32_handle`: what a native `HANDLE` points to.
///
/// The header's layout, asserted in `abi.rs`. What another thread changes while a handle is
/// shared (the reference count, the mutex and condition, the count) is a cell, so a shared
/// reference to a live handle is sound; the other fields are set once, when it is made.
#[repr(C)]
pub struct MnWin32Handle {
    /// One of the five `MN_WIN32_*` kinds; 0 once freed.
    pub kind: u32,
    /// The references; the last one's drop frees the handle.
    pub refs: AtomicI32,
    /// The mutex of the count, initialised for every kind but a file and a relay.
    pub m: UnsafeCell<libc::pthread_mutex_t>,
    /// The condition waiters wait on, initialised with the mutex.
    pub c: UnsafeCell<libc::pthread_cond_t>,
    /// An event's state (1 when set) or a semaphore's count, under the mutex.
    pub count: UnsafeCell<i32>,
    /// A semaphore's maximum; an event's is 1.
    pub max: i32,
    /// Nonzero for an event that a wait does not reset.
    pub manual: c_int,
    /// A file's descriptor; -1 for every other kind.
    pub fd: c_int,
    /// A thread's id.
    pub tid: u64,
    /// What a relay's set calls, with `relay_ctx`.
    pub relay: Option<unsafe extern "C-unwind" fn(*mut c_void)>,
    /// What a relay's last reference's drop calls, with `relay_ctx`, when not null.
    pub relay_end: Option<unsafe extern "C-unwind" fn(*mut c_void)>,
    /// The argument of a relay's two functions.
    pub relay_ctx: *mut c_void,
}

/// `CreateEventW` with no attributes and no name; null when the allocation fails.
///
/// A manual-reset event for a nonzero `manual`, set for a nonzero `initial`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn wmt_native_create_event(manual: i32, initial: i32) -> *mut c_void {
    let _guard = AbortOnPanic;
    let Some(o) = new_handle(MN_WIN32_EVENT) else {
        return ptr::null_mut();
    };
    o.manual = manual;
    *o.count.get_mut() = i32::from(initial != 0);
    o.max = 1;
    ptr::from_mut(o).cast()
}

/// `CreateSemaphoreW` with no attributes and no name.
///
/// Null for a maximum below 1, an initial count below 0 or above the maximum, and when the
/// allocation fails.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn wmt_native_create_semaphore(initial: i32, max: i32) -> *mut c_void {
    let _guard = AbortOnPanic;
    if max <= 0 || initial < 0 || initial > max {
        return ptr::null_mut();
    }
    let Some(o) = new_handle(MN_WIN32_SEMAPHORE) else {
        return ptr::null_mut();
    };
    *o.count.get_mut() = initial;
    o.max = max;
    ptr::from_mut(o).cast()
}

/// `ReleaseSemaphore`: 1 when the count rose by `count`, else 0.
///
/// 0 for anything but a semaphore, a `count` below 1 and a rise past the maximum. On success a
/// non-null `previous` gets the count before the rise; otherwise it is not written.
///
/// # Safety
///
/// `handle` is null, -1, -2, or points to four readable bytes, which, when they hold a handle's
/// kind, begin a live handle. A non-null `previous` points to an `i32` to write.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn wmt_native_release_semaphore(
    handle: *mut c_void,
    count: i32,
    previous: *mut i32,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let Some(o) = (unsafe { handle_at(handle) }).filter(|o| o.kind == MN_WIN32_SEMAPHORE) else {
        return 0;
    };
    if count <= 0 {
        return 0;
    }
    // SAFETY: a semaphore, whose mutex and condition are initialised.
    let lock = unsafe { Locked::new(o) };
    let before = lock.count();
    let ok = before <= o.max.wrapping_sub(count);
    if ok {
        if !previous.is_null() {
            // SAFETY: the caller's contract: a non-null `previous` is an `i32` to write.
            unsafe { previous.write(before) };
        }
        lock.set_count(before.wrapping_add(count));
        lock.broadcast();
    }
    i32::from(ok)
}

/// `WaitForSingleObject` on an event or a semaphore.
///
/// `WAIT_OBJECT_0` when it took the object, `WAIT_TIMEOUT` when the time ran out first,
/// `WAIT_FAILED` for anything else (a thread, a file and a relay included). A wait of 0 polls,
/// `INFINITE` has no deadline, any other runs to a deadline on the uptime clock, so a change of
/// the wall clock does not stretch it. A manual-reset event stays set; an auto-reset event and a
/// semaphore are taken.
///
/// # Safety
///
/// `handle` is null, -1, -2, or points to four readable bytes, which, when they hold a handle's
/// kind, begin a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn wmt_native_wait_for_single_object(
    handle: *mut c_void,
    milliseconds: u32,
) -> u32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let Some(o) = (unsafe { handle_at(handle) })
        .filter(|o| matches!(o.kind, MN_WIN32_EVENT | MN_WIN32_SEMAPHORE))
    else {
        return WAIT_FAILED;
    };
    let end = uptime_ns().wrapping_add(u64::from(milliseconds) * 1_000_000);
    // SAFETY: an event or a semaphore, whose mutex and condition are initialised.
    let lock = unsafe { Locked::new(o) };
    let mut r = 0;
    while lock.count() == 0 && r != libc::ETIMEDOUT {
        let now = uptime_ns();
        if milliseconds == 0 || (milliseconds != INFINITE && now >= end) {
            r = libc::ETIMEDOUT;
        } else if milliseconds == INFINITE {
            lock.wait();
        } else {
            let left = end - now;
            let left = libc::timespec {
                tv_sec: (left / 1_000_000_000).cast_signed(),
                tv_nsec: (left % 1_000_000_000).cast_signed(),
            };
            r = lock.wait_for(&left);
        }
    }
    let got = lock.count() > 0;
    if got && (o.kind == MN_WIN32_SEMAPHORE || o.manual == 0) {
        lock.set_count(lock.count() - 1);
    }
    if got { WAIT_OBJECT_0 } else { WAIT_TIMEOUT }
}

/// `DuplicateHandle` within this process: 1 when `target` got the duplicate, else 0.
///
/// An event's, a semaphore's or a thread's duplicate is the same handle with one more reference;
/// a file's is a new handle of a `dup` of its descriptor. 0 for a null target, a null source,
/// the pseudo-handles -1 and -2, a relay and anything that is no handle; `target` is then not
/// written. `DUPLICATE_CLOSE_SOURCE` in `options` then closes the source.
///
/// # Safety
///
/// `source` is null, -1, -2, or points to four readable bytes, which, when they hold a handle's
/// kind, begin a live handle. A non-null `target` points to a pointer to write.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn wmt_native_duplicate_handle(
    source: *mut c_void,
    target: *mut *mut c_void,
    options: u32,
) -> i32 {
    let _guard = AbortOnPanic;
    if target.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    let Some(o) = (unsafe { handle_at(source) }).filter(|o| o.kind != MN_WIN32_RELAY) else {
        return 0;
    };
    let duplicate = if o.kind == MN_WIN32_FILE {
        let Some(d) = new_handle(MN_WIN32_FILE) else {
            return 0;
        };
        // SAFETY: plain call; a descriptor that is not open answers -1.
        d.fd = unsafe { libc::dup(o.fd) };
        if d.fd < 0 {
            // SAFETY: the allocation `new_handle` made, which nothing else holds.
            unsafe { libc::free(ptr::from_mut(d).cast()) };
            return 0;
        }
        ptr::from_mut(d).cast()
    } else {
        o.refs.fetch_add(1, Ordering::Relaxed);
        source
    };
    // SAFETY: the caller's contract: a non-null `target` is a pointer to write.
    unsafe { target.write(duplicate) };
    if options & DUPLICATE_CLOSE_SOURCE != 0 {
        // SAFETY: the caller's source, whose reference the caller gives back.
        unsafe { wmt_native_close_handle(source) };
    }
    1
}

/// `OpenThread` by the id of a thread of this process (`GetCurrentThreadId`'s).
///
/// Null for id 0, an id no thread of this process has, and when the allocation fails.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn wmt_native_open_thread(thread_id: u32) -> *mut c_void {
    let _guard = AbortOnPanic;
    if thread_id == 0 {
        return ptr::null_mut();
    }
    let mut info = MaybeUninit::<libc::proc_threadinfo>::uninit();
    // SAFETY: a buffer of `THREAD_INFO_SIZE` bytes for the answer.
    let n = unsafe {
        libc::proc_pidinfo(
            std::process::id().cast_signed(),
            PROC_PIDTHREADID64INFO,
            u64::from(thread_id),
            info.as_mut_ptr().cast(),
            THREAD_INFO_SIZE,
        )
    };
    if n != THREAD_INFO_SIZE {
        return ptr::null_mut();
    }
    let Some(o) = new_handle(MN_WIN32_THREAD) else {
        return ptr::null_mut();
    };
    o.tid = u64::from(thread_id);
    ptr::from_mut(o).cast()
}

/// `CloseHandle`: drops a handle's reference.
///
/// 1 for a handle and for the pseudo-handles -1 and -2, 0 for anything else.
///
/// # Safety
///
/// `handle` is null, -1, -2, or points to four readable bytes, which, when they hold a handle's
/// kind, begin a live handle whose reference the caller gives back.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn wmt_native_close_handle(handle: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    if is_pseudo(handle) {
        return 1;
    }
    // SAFETY: the caller's contract.
    unsafe { wmt_native_unref(handle) }
}

/// `mn_win32_set_event`: sets an event and wakes its waiters, or calls a relay's function.
///
/// 1 for an event and a relay, whose function runs on this thread; 0 for anything else.
///
/// # Safety
///
/// `handle` is null, -1, -2, or points to four readable bytes, which, when they hold a handle's
/// kind, begin a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn wmt_native_set_event(handle: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let Some(o) = (unsafe { handle_at(handle) }) else {
        return 0;
    };
    match o.kind {
        MN_WIN32_RELAY => {
            // A relay is made with its function: without one the process ends, as a call
            // through null would end it.
            let Some(relay) = o.relay else {
                std::process::abort();
            };
            // SAFETY: the relay's function and its argument, as its maker gave them.
            unsafe { relay(o.relay_ctx) };
            1
        }
        MN_WIN32_EVENT => {
            // SAFETY: an event, whose mutex and condition are initialised.
            let lock = unsafe { Locked::new(o) };
            lock.set_count(1);
            lock.broadcast();
            1
        }
        _ => 0,
    }
}

/// `mn_win32_ref`: one more reference to a handle; nothing for anything else.
///
/// # Safety
///
/// `handle` is null, -1, -2, or points to four readable bytes, which, when they hold a handle's
/// kind, begin a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn wmt_native_ref(handle: *mut c_void) {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    if let Some(o) = unsafe { handle_at(handle) } {
        o.refs.fetch_add(1, Ordering::Relaxed);
    }
}

/// `mn_win32_unref`: drops a reference to a handle, 1, else 0.
///
/// The last reference's drop closes a file's descriptor, calls a relay's end function if it has
/// one, destroys an event's or a semaphore's mutex and condition, and frees the handle.
///
/// # Safety
///
/// `handle` is null, -1, -2, or points to four readable bytes, which, when they hold a handle's
/// kind, begin a live handle whose reference the caller gives back.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn wmt_native_unref(handle: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let Some(o) = (unsafe { handle_at(handle) }) else {
        return 0;
    };
    if o.refs.fetch_sub(1, Ordering::AcqRel) != 1 {
        return 1;
    }
    match o.kind {
        MN_WIN32_FILE => {
            // SAFETY: the descriptor this handle owns.
            unsafe { libc::close(o.fd) };
        }
        MN_WIN32_RELAY => {
            if let Some(end) = o.relay_end {
                // SAFETY: the relay's end function and its argument, as its maker gave them.
                unsafe { end(o.relay_ctx) };
            }
        }
        MN_WIN32_THREAD => {}
        _ => {
            // SAFETY: an event's or a semaphore's mutex, which no one holds or waits on now.
            unsafe { libc::pthread_mutex_destroy(o.m.get()) };
            // SAFETY: its condition, which no one waits on now.
            unsafe { libc::pthread_cond_destroy(o.c.get()) };
        }
    }
    let p = handle.cast::<MnWin32Handle>();
    // SAFETY: the handle of the last reference, which nothing else reaches.
    unsafe { (*p).kind = 0 };
    // SAFETY: a `calloc` allocation (this module's or the header's), freed once, by its last
    // reference.
    unsafe { libc::free(handle) };
    1
}

/// Sets the calling thread's not-implemented flag, which `MTLD3DNotImplemented` reads.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn wmt_native_not_implemented() {
    let _guard = AbortOnPanic;
    NOT_IMPLEMENTED.set(true);
}

/// `MTLD3DNotImplemented`: reads and clears the calling thread's not-implemented flag.
#[must_use]
#[unsafe(export_name = "MTLD3DNotImplemented")]
pub extern "C-unwind" fn mtld3d_not_implemented() -> bool {
    let _guard = AbortOnPanic;
    NOT_IMPLEMENTED.replace(false)
}

/// The reference count of a handle (`refs`), 0 for anything else.
///
/// # Safety
///
/// `handle` is null, -1, -2, or points to four readable bytes, which, when they hold a handle's
/// kind, begin a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn wmt_native_handle_count(handle: *mut c_void) -> u32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    (unsafe { handle_at(handle) }).map_or(0, |o| o.refs.load(Ordering::Relaxed).cast_unsigned())
}

/// An event's or a semaphore's mutex, held until the drop; the count is read and written here.
struct Locked<'a>(&'a MnWin32Handle);

impl<'a> Locked<'a> {
    /// Locks `o`'s mutex.
    ///
    /// # Safety
    ///
    /// `o` is an event or a semaphore: its mutex and condition are initialised.
    unsafe fn new(o: &'a MnWin32Handle) -> Self {
        // SAFETY: the caller's contract: an initialised mutex.
        unsafe { libc::pthread_mutex_lock(o.m.get()) };
        Self(o)
    }

    /// The count.
    fn count(&self) -> i32 {
        // SAFETY: the count is read and written under the mutex, which this thread holds.
        unsafe { *self.0.count.get() }
    }

    /// Sets the count.
    fn set_count(&self, count: i32) {
        // SAFETY: as in `count`.
        unsafe { *self.0.count.get() = count };
    }

    /// Wakes every waiter.
    fn broadcast(&self) {
        // SAFETY: an initialised condition (`new`'s contract).
        unsafe { libc::pthread_cond_broadcast(self.0.c.get()) };
    }

    /// Waits for a wake, the mutex released meanwhile.
    fn wait(&self) {
        // SAFETY: an initialised condition and the mutex this thread holds.
        unsafe { libc::pthread_cond_wait(self.0.c.get(), self.0.m.get()) };
    }

    /// Waits for a wake at most `left`: `ETIMEDOUT` when it ran out.
    fn wait_for(&self, left: &libc::timespec) -> c_int {
        // SAFETY: as in `wait`.
        unsafe { libc::pthread_cond_timedwait_relative_np(self.0.c.get(), self.0.m.get(), left) }
    }
}

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        // SAFETY: the mutex this thread locked in `new`.
        unsafe { libc::pthread_mutex_unlock(self.0.m.get()) };
    }
}

/// `mn_win32_is_handle`: the handle `h` points to.
///
/// `None` for null, the pseudo-handles -1 and -2, and memory whose first four bytes are no
/// handle's kind.
///
/// # Safety
///
/// `h` is null, -1, -2, or points to four readable bytes, which, when they hold a handle's kind,
/// begin a handle that stays live during `'a`.
unsafe fn handle_at<'a>(h: *mut c_void) -> Option<&'a MnWin32Handle> {
    if h.is_null() || is_pseudo(h) {
        return None;
    }
    // SAFETY: the caller's contract: four readable bytes, where a handle has its kind.
    let kind = unsafe { h.cast::<u32>().read_unaligned() };
    if !matches!(
        kind,
        MN_WIN32_EVENT | MN_WIN32_SEMAPHORE | MN_WIN32_FILE | MN_WIN32_THREAD | MN_WIN32_RELAY
    ) {
        return None;
    }
    // SAFETY: the caller's contract: a handle's kind begins a live handle.
    Some(unsafe { &*h.cast::<MnWin32Handle>() })
}

/// Whether `h` is `GetCurrentProcess()` or `GetCurrentThread()`: -1 or -2.
fn is_pseudo(h: *mut c_void) -> bool {
    matches!(h.addr().cast_signed(), -1 | -2)
}

/// `mn_win32_new`: a handle of `kind` with one reference and no descriptor.
///
/// Its mutex and condition are initialised unless it is a file. `None` when the allocation
/// fails.
fn new_handle(kind: u32) -> Option<&'static mut MnWin32Handle> {
    // SAFETY: plain call: null, or zeroed memory aligned for any type.
    let p = unsafe { libc::calloc(1, size_of::<MnWin32Handle>()) }.cast::<MnWin32Handle>();
    // SAFETY: null, or a new allocation of a handle's size, zeroed (a valid handle: no field
    // forbids 0) and held by nothing else.
    let o = unsafe { p.as_mut() }?;
    o.kind = kind;
    *o.refs.get_mut() = 1;
    o.fd = -1;
    if kind != MN_WIN32_FILE {
        // SAFETY: the handle's own mutex, which nothing uses yet.
        unsafe { libc::pthread_mutex_init(o.m.get_mut(), ptr::null()) };
        // SAFETY: the handle's own condition, which nothing uses yet.
        unsafe { libc::pthread_cond_init(o.c.get_mut(), ptr::null()) };
    }
    Some(o)
}

/// The time on the uptime clock (`CLOCK_UPTIME_RAW`) in nanoseconds.
fn uptime_ns() -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: a clock the system has and a `timespec` to write.
    unsafe { libc::clock_gettime(libc::CLOCK_UPTIME_RAW, &raw mut now) };
    now.tv_sec
        .cast_unsigned()
        .wrapping_mul(1_000_000_000)
        .wrapping_add(now.tv_nsec.cast_unsigned())
}

#[cfg(test)]
mod tests;
