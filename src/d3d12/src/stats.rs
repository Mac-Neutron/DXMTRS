//! Per-call timing and counter instrumentation, backed by `dxmt_core::stats`.
//!
//! Under `perf-build` (`DXMT_PERF_BUILD`), every macro and function compiles away to nothing so
//! `d3d12.dll` and `d3d12.dylib` carry no stat names or `dxmt_core::stats` symbols.

#[cfg(not(feature = "perf-build"))]
pub use dxmt_core::stats::{
    Scope, StatId, count as count_id, exit, frame, id, init, next_pass, on,
};

/// No-op under `perf-build`.
#[cfg(feature = "perf-build")]
pub const fn init(_on: bool) {}

/// No-op under `perf-build`.
#[cfg(feature = "perf-build")]
pub const fn exit() {}

/// Always `false` under `perf-build`.
#[cfg(feature = "perf-build")]
#[must_use]
pub const fn on() -> bool {
    false
}

/// No-op under `perf-build`.
#[cfg(feature = "perf-build")]
pub const fn frame() {}

/// Always `0` under `perf-build`.
#[cfg(feature = "perf-build")]
#[must_use]
pub const fn next_pass() -> u32 {
    0
}

/// `DXMT_STAT_SCOPE($name)`: times the enclosing scope when `DXMT_STATS=1` is active.
///
/// The id is registered on the call site's first run while `DXMT_STATS=1` is active in a per-site
/// `LazyLock<StatId>` (matching the C++ function-local static).
#[cfg(not(feature = "perf-build"))]
#[macro_export]
macro_rules! scope {
    ($name:literal) => {
        let _dxmt_stat_scope = if $crate::stats::on() {
            static ID: ::std::sync::LazyLock<$crate::stats::StatId> =
                ::std::sync::LazyLock::new(|| $crate::stats::id($name));
            Some($crate::stats::Scope::new(*ID))
        } else {
            None
        };
    };
}

/// `DXMT_STAT_SCOPE($name)`: compiles to nothing under `perf-build`.
#[cfg(feature = "perf-build")]
#[macro_export]
macro_rules! scope {
    ($name:literal) => {
        let _ = ::core::option::Option::from(());
    };
}

/// `DXMT_STAT_COUNT($name, $delta)`: increments counter `$name` by `$delta` when `DXMT_STATS=1`.
#[cfg(not(feature = "perf-build"))]
#[macro_export]
macro_rules! count {
    ($name:literal, $delta:expr) => {{
        let delta: u64 = $delta;
        if $crate::stats::on() {
            static ID: ::std::sync::LazyLock<$crate::stats::StatId> =
                ::std::sync::LazyLock::new(|| $crate::stats::id($name));
            $crate::stats::count_id(*ID, delta);
        }
    }};
}

/// `DXMT_STAT_COUNT($name, $delta)`: compiles to nothing under `perf-build`.
#[cfg(feature = "perf-build")]
#[macro_export]
macro_rules! count {
    ($name:literal, $delta:expr) => {{
        let _: u64 = $delta;
    }};
}

/// `StatsFrame()`: records the end of a presented frame when `DXMT_STATS=1`.
#[cfg(not(feature = "perf-build"))]
#[macro_export]
macro_rules! stat_frame {
    () => {
        $crate::stats::frame();
    };
}

/// `StatsFrame()`: compiles to nothing under `perf-build`.
#[cfg(feature = "perf-build")]
#[macro_export]
macro_rules! stat_frame {
    () => {};
}

pub use crate::{count as stat_count, scope as stat_scope};
