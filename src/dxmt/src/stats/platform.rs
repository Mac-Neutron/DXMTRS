//! The system calls the statistics make: the CPU's counter, thread ids and names, local time.
//!
//! On Windows kernel32's, as `d3d12_stats.cpp` makes them; natively libc's, the calls
//! `win32_native.h` makes for the same functions (`pthread_threadid_np`, `proc_pidinfo` with
//! `PROC_PIDTHREADID64INFO`, `localtime_r`).

#[cfg(windows)]
use core::ffi::c_void;

/// `PROC_PIDTHREADID64INFO` of `<sys/proc_info.h>`: a thread's info by its 64-bit id.
#[cfg(not(windows))]
const PROC_PIDTHREADID64INFO: libc::c_int = 15;

/// `THREAD_QUERY_LIMITED_INFORMATION`: what `GetThreadDescription` needs.
#[cfg(windows)]
const THREAD_QUERY_LIMITED_INFORMATION: u32 = 0x0800;

#[cfg(windows)]
unsafe extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn OpenThread(access: u32, inherit: i32, tid: u32) -> *mut c_void;
    fn GetThreadDescription(thread: *mut c_void, description: *mut *mut u16) -> i32;
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
    fn CloseHandle(handle: *mut c_void) -> i32;
    /// `SYSTEMTIME`'s eight `WORD`s: year, month, day of the week, day, hour, minute, second, ms.
    fn GetLocalTime(time: *mut [u16; 8]);
}

/// The CPU's counter: `rdtsc` (the report calibrates it against the clock, so its rate is free).
#[cfg(target_arch = "x86")]
#[must_use]
pub fn counter() -> u64 {
    // SAFETY: `rdtsc` reads the time-stamp counter; every x86 CPU DXMT runs on has it.
    unsafe { core::arch::x86::_rdtsc() }
}

/// The CPU's counter: `rdtsc` (the report calibrates it against the clock, so its rate is free).
#[cfg(target_arch = "x86_64")]
#[must_use]
pub fn counter() -> u64 {
    // SAFETY: `rdtsc` reads the time-stamp counter; every x86 CPU DXMT runs on has it.
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// The CPU's counter: `cntvct_el0` (the report calibrates it against the clock).
#[cfg(any(target_arch = "aarch64", target_arch = "arm64ec"))]
#[must_use]
pub fn counter() -> u64 {
    let ticks: u64;
    // SAFETY: EL0 may read the virtual counter on every system DXMT runs on; the read touches no
    // memory, stack or flags.
    unsafe {
        core::arch::asm!("mrs {}, cntvct_el0", out(reg) ticks, options(nomem, nostack, preserves_flags));
    }
    ticks
}

/// The calling thread's id (`GetCurrentThreadId`).
#[cfg(windows)]
#[must_use]
pub fn thread_id() -> u32 {
    // SAFETY: takes nothing, cannot fail.
    unsafe { GetCurrentThreadId() }
}

/// The calling thread's id, cut to a `DWORD` as `win32_native.h`'s `GetCurrentThreadId`.
#[cfg(not(windows))]
#[must_use]
pub fn thread_id() -> u32 {
    let mut id = 0u64;
    // SAFETY: a null thread is the calling one; `id` is a live u64.
    unsafe { libc::pthread_threadid_np(0, &raw mut id) };
    let [b0, b1, b2, b3, ..] = id.to_le_bytes();
    u32::from_le_bytes([b0, b1, b2, b3])
}

/// The name of the thread `tid` (`GetThreadDescription`), `thread` when it has none or is gone.
#[cfg(windows)]
#[must_use]
pub fn thread_name(tid: u32) -> String {
    let mut name = String::new();
    // SAFETY: plain values; a thread that is gone gives null.
    let thread = unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, tid) };
    if !thread.is_null() {
        let mut description = core::ptr::null_mut();
        // SAFETY: `thread` is open; `description` is a live out-pointer.
        let hr = unsafe { GetThreadDescription(thread, &raw mut description) };
        if hr >= 0 && !description.is_null() {
            // SAFETY: GetThreadDescription returned a NUL-terminated string.
            name = unsafe { wide_text(description) };
            // SAFETY: the string GetThreadDescription allocated, freed once.
            unsafe { LocalFree(description.cast()) };
        }
        // SAFETY: the handle OpenThread gave, closed once.
        unsafe { CloseHandle(thread) };
    }
    if name.is_empty() {
        "thread".to_owned()
    } else {
        name
    }
}

/// The name of the thread `tid` (`proc_pidinfo`), `thread` when it has none or is gone.
///
/// As `win32_native.h`'s `GetThreadDescription`: each byte of the name is a character (a UTF-8
/// name's bytes come out one by one), at most 63 of them.
#[cfg(not(windows))]
#[must_use]
pub fn thread_name(tid: u32) -> String {
    if tid == 0 {
        return "thread".to_owned();
    }
    let mut info = libc::proc_threadinfo::default();
    let size = libc::c_int::try_from(size_of::<libc::proc_threadinfo>())
        .expect("proc_threadinfo is a few hundred bytes");
    // SAFETY: `info` is a live proc_threadinfo of `size` bytes; plain values otherwise.
    let got = unsafe {
        libc::proc_pidinfo(
            std::process::id().cast_signed(),
            PROC_PIDTHREADID64INFO,
            u64::from(tid),
            (&raw mut info).cast(),
            size,
        )
    };
    let name: String = if got == size {
        info.pth_name
            .iter()
            .take(libc::MAXTHREADNAMESIZE - 1)
            .take_while(|&&c| c != 0)
            .map(|&c| char::from(c.cast_unsigned()))
            .collect()
    } else {
        String::new()
    };
    if name.is_empty() {
        "thread".to_owned()
    } else {
        name
    }
}

/// The local time's hours, minutes and seconds (`GetLocalTime`).
#[cfg(windows)]
#[must_use]
pub fn local_time() -> [u16; 3] {
    let mut now = [0u16; 8];
    // SAFETY: `now` is a live SYSTEMTIME's worth of WORDs.
    unsafe { GetLocalTime(&raw mut now) };
    let [_, _, _, _, hour, minute, second, _] = now;
    [hour, minute, second]
}

/// The local time's hours, minutes and seconds (`localtime_r` of the wall clock's second).
#[cfg(not(windows))]
#[must_use]
pub fn local_time() -> [u16; 3] {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs().cast_signed());
    let mut tm = libc::tm::default();
    // SAFETY: both point to live values of their types.
    let filled = unsafe { libc::localtime_r(&raw const seconds, &raw mut tm) };
    if filled.is_null() {
        return [0; 3];
    }
    [tm.tm_hour, tm.tm_min, tm.tm_sec].map(|field| u16::try_from(field).unwrap_or(0))
}

/// The UTF-16 string at `text` up to its NUL, as UTF-8 (`str::fromws`).
///
/// # Safety
///
/// `text` points to a NUL-terminated UTF-16 string.
#[cfg(windows)]
unsafe fn wide_text(text: *const u16) -> String {
    let mut len = 0;
    // SAFETY: the string is NUL-terminated, so each unit up to the NUL is readable.
    while unsafe { text.wrapping_add(len).read() } != 0 {
        len += 1;
    }
    // SAFETY: the `len` units before the NUL are readable.
    String::from_utf16_lossy(unsafe { core::slice::from_raw_parts(text, len) })
}
