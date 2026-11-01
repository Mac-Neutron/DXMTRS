//! The CPU performance counter and its frequency, for `GetFrameStatistics`.
//!
//! On PE this calls `QueryPerformanceCounter` and `QueryPerformanceFrequency`; natively on macOS
//! it reads `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)` with a 1 GHz frequency, matching DXMT's
//! `win32_native.h`.

#[cfg(not(windows))]
const CLOCK_UPTIME_RAW: u32 = 8;

#[cfg(not(windows))]
const NSEC_PER_SEC: i64 = 1_000_000_000;

#[cfg(not(windows))]
unsafe extern "C" {
    fn clock_gettime_nsec_np(clock_id: u32) -> u64;
}

#[cfg(windows)]
#[link(name = "kernel32", kind = "raw-dylib")]
unsafe extern "system" {
    fn QueryPerformanceCounter(count: *mut i64) -> i32;
    fn QueryPerformanceFrequency(frequency: *mut i64) -> i32;
}

/// Returns `(counter, frequency)` for the current CPU performance counter.
///
/// Both values are non-negative ticks and ticks per second.
#[must_use]
pub fn now() -> (i64, i64) {
    #[cfg(windows)]
    {
        let mut count = 0_i64;
        let mut freq = 0_i64;
        // SAFETY: `count` is a live `LARGE_INTEGER` on this stack frame.
        unsafe { QueryPerformanceCounter(&raw mut count) };
        // SAFETY: `freq` is a live `LARGE_INTEGER` on this stack frame.
        unsafe { QueryPerformanceFrequency(&raw mut freq) };
        (count, freq)
    }
    #[cfg(not(windows))]
    {
        // SAFETY: `CLOCK_UPTIME_RAW` is a valid macOS clock id and reads no user memory.
        let ticks = unsafe { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) };
        (ticks.cast_signed(), NSEC_PER_SEC)
    }
}

/// Returns `(counter, frequency)` as unsigned 64-bit values for the CPU performance counter.
#[must_use]
pub fn qpc() -> (u64, u64) {
    let (count, freq) = now();
    (count.cast_unsigned(), freq.cast_unsigned())
}

#[cfg(test)]
mod tests;
