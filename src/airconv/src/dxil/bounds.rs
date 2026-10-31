//! `DXMT_DXIL_BOUNDS`: how the DXIL lowering bounds-checks buffer accesses.
//!
//! Read once per process, as the C++'s function statics did (`A/dxil/dxil_lower.cpp:89-102`).
//! Unset or anything else: a raw or structured buffer load is checked once for all its
//! components (E5); `component` checks each component (triage); `off` (measurement only) checks
//! no typed, raw or structured access, so a read past a view reads whatever lies there and a
//! write past it lands there.

use std::{env, ffi::OsStr, sync::LazyLock};

/// `DxilBoundsPerComponent`: `DXMT_DXIL_BOUNDS=component`.
pub fn per_component() -> bool {
    mode() == Some(OsStr::new("component"))
}

/// `DxilBoundsOff`: `DXMT_DXIL_BOUNDS=off`.
pub fn off() -> bool {
    mode() == Some(OsStr::new("off"))
}

/// The variable's value, latched at the first read.
fn mode() -> Option<&'static OsStr> {
    /// The process's environment, latched once and immutable after.
    ///
    /// The C++'s function statics: every compile of the process checks alike.
    static MODE: LazyLock<Option<std::ffi::OsString>> =
        LazyLock::new(|| env::var_os("DXMT_DXIL_BOUNDS"));
    MODE.as_deref()
}
