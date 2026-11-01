//! `IDXGIKeyedMutex` tear-off and `KeyedMutex` cross-process sync (`dxmt_keyed_mutex.cpp`).

use std::sync::Arc;

use crate::com::{E_NOTIMPL, Hresult};

/// Reference-counted handle to a [`KeyedMutex`].
pub type KeyedMutexRef = Arc<KeyedMutex>;

/// Cross-process keyed mutex over D3DKMT and `WMT::SharedEvent` (`dxmt::KeyedMutex`).
#[derive(Default)]
pub struct KeyedMutex {
    global_kmt: u32,
    shared_event: u64,
}

impl KeyedMutex {
    /// Returns the global D3DKMT handle.
    #[must_use]
    pub const fn global_handle(&self) -> u32 {
        self.global_kmt
    }

    /// Returns the `WMT::SharedEvent` handle.
    #[must_use]
    pub const fn shared_event(&self) -> u64 {
        self.shared_event
    }

    /// Acquires the keyed mutex for `key` within `milliseconds`.
    pub fn acquire(&self, func_acquire: impl FnOnce(u64), key: u64, milliseconds: u32) -> Hresult {
        let _ = (func_acquire, key, milliseconds);
        E_NOTIMPL
    }

    /// Releases the keyed mutex for `key`.
    pub fn release(&self, func_release: impl FnOnce(u64), key: u64) -> Hresult {
        let _ = (func_release, key);
        E_NOTIMPL
    }
}
