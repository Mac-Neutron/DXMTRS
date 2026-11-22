//! Process-wide `DXMT_STATS` lifecycle hooks for `d3d9-rt`, backed by `dxmt_core::stats`.
//!
//! Under `perf-build`, every entry point compiles to a `const` no-op with zero static state.

#[cfg(not(feature = "perf-build"))]
use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(feature = "perf-build"))]
use std::sync::Once;

#[cfg(not(feature = "perf-build"))]
static STATS_ONCE: Once = Once::new();
#[cfg(not(feature = "perf-build"))]
static STATS_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Latches `DXMT_STATS` once when the first `CoreDevice` starts and enables `dxmt_core::stats`.
#[cfg(not(feature = "perf-build"))]
pub fn init_once() {
    STATS_ONCE.call_once(|| {
        let active = dxmt_core::settings::settings().enabled(dxmt_core::settings::Flags::STATS);
        dxmt_core::stats::init(active);
        STATS_ACTIVE.store(active, Ordering::Release);
    });
}

/// No-op under `perf-build`.
#[cfg(feature = "perf-build")]
pub const fn init_once() {}

/// Records one presented frame for `DXMT_STATS` periodic reporting.
#[cfg(not(feature = "perf-build"))]
pub fn frame() {
    if STATS_ACTIVE.load(Ordering::Acquire) {
        dxmt_core::stats::frame();
    }
}

/// No-op under `perf-build`.
#[cfg(feature = "perf-build")]
pub const fn frame() {}

/// Flushes the final `stats.txt` report on `DLL_PROCESS_DETACH` if `DXMT_STATS` was initialized.
#[cfg(not(feature = "perf-build"))]
pub fn flush_stats_on_detach() {
    if STATS_ACTIVE.swap(false, Ordering::AcqRel) {
        dxmt_core::stats::exit();
    }
}

/// No-op under `perf-build`.
#[cfg(feature = "perf-build")]
pub const fn flush_stats_on_detach() {}

#[cfg(all(test, not(feature = "perf-build")))]
mod tests;
