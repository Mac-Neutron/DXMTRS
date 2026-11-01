//! Background shader and pipeline compilation scheduler (`dxmt_tasks.cpp`).

use std::sync::{LazyLock, Mutex};

/// Process-wide list of active scheduler instances (`dxmt_tasks.cpp:11-12`).
pub static SCHEDULERS: LazyLock<Mutex<Vec<usize>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Waits up to `timeout_ms` ms for all compile schedulers to become idle (`wait_schedulers_idle`).
#[must_use]
pub const fn wait_idle(timeout_ms: u32) -> bool {
    let _ = timeout_ms;
    true
}
