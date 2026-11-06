//! Process-wide [`Store`] instances, one per Metal version (`ShaderCache::getInstance`).

use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

#[cfg(not(windows))]
use dxmt_util::sync::lock;
use rustc_hash::FxHashMap;

use super::Store;

/// The open [`Store`] for each `WMTMetalVersion` (`ShaderCache::getInstance`'s map).
///
/// A static: the process-wide resource is the on-disk SQLite cache per Metal version, shared
/// across every device in the process so each table is opened and written through one handle.
static CACHES: LazyLock<Mutex<FxHashMap<u32, Arc<Store>>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

/// Whether the native `atexit` hook that runs [`close_all`] has been registered.
///
/// A static: `atexit` registration is once per process on the first [`instance`] open.
#[cfg(not(windows))]
static ATEXIT: AtomicBool = AtomicBool::new(false);

#[cfg(not(windows))]
unsafe extern "C" {
    fn atexit(cb: extern "C" fn()) -> core::ffi::c_int;
}

#[cfg(not(windows))]
extern "C" fn close_at_exit() {
    let _guard = crate::ffi::AbortOnPanic;
    close_all();
}

#[cfg(not(windows))]
fn register_atexit_once() {
    if !ATEXIT.swap(true, Ordering::Relaxed) {
        // SAFETY: `close_at_exit` is a valid C function pointer with static lifetime.
        unsafe { atexit(close_at_exit) };
    }
}

/// Returns the process's [`Store`] for `metal_version`, opening it with `version` on first use.
#[must_use]
pub fn instance(metal_version: u32, version: u64) -> Arc<Store> {
    #[cfg(not(windows))]
    register_atexit_once();

    let mut caches = lock(&CACHES);
    Arc::clone(
        caches
            .entry(metal_version)
            .or_insert_with(|| Arc::new(Store::new(metal_version, version))),
    )
}

/// Closes and drops every cached [`Store`] in the process.
///
/// Even when a leaked device, shader or pipeline still holds an [`Arc<Store>`], [`Store::close`]
/// releases the underlying reader and writer in the C++ destruction order so later reads and
/// writes see the cache as off.
pub fn close_all() {
    let stores: Vec<Arc<Store>> = {
        let mut caches = lock(&CACHES);
        caches.drain().map(|(_, store)| store).collect()
    };
    for store in stores {
        store.close();
    }
}
