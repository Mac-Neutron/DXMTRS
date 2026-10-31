//! D3DKMT's adapter handle for an adapter's LUID.
//!
//! gdi32's on PE; natively DXMT's stubs, which warn and fail (`util_d3dkmt.h`).

use crate::{LOG, abi::Luid};

/// Opens the D3DKMT adapter of `luid`: its handle, or 0 after a warning.
#[must_use]
pub fn open(luid: Luid) -> u32 {
    platform::open(luid).unwrap_or_else(|| {
        LOG.warn("Failed to open D3DKMT adapter");
        0
    })
}

/// Closes a handle `open` gave, warning if that fails; 0, no handle, is left alone.
pub fn close(handle: u32) {
    if handle != 0 && !platform::close(handle) {
        LOG.warn("Failed to close D3DKMT adapter");
    }
}

#[cfg(windows)]
mod platform {
    use crate::abi::{D3dkmtCloseAdapter, D3dkmtOpenAdapterFromLuid, Luid};

    unsafe extern "system" {
        fn D3DKMTOpenAdapterFromLuid(desc: *mut D3dkmtOpenAdapterFromLuid) -> i32;
        fn D3DKMTCloseAdapter(desc: *const D3dkmtCloseAdapter) -> i32;
    }

    /// gdi32's handle for `luid`, or none when its status is not 0.
    pub fn open(luid: Luid) -> Option<u32> {
        let mut desc = D3dkmtOpenAdapterFromLuid {
            adapter_luid: luid,
            ..Default::default()
        };
        // SAFETY: `desc` is a local `D3DKMT_OPENADAPTERFROMLUID` the call fills.
        let status = unsafe { D3DKMTOpenAdapterFromLuid(&raw mut desc) };
        (status == 0).then_some(desc.adapter)
    }

    /// Whether gdi32 closed `handle` (status 0).
    pub fn close(handle: u32) -> bool {
        let desc = D3dkmtCloseAdapter { adapter: handle };
        // SAFETY: `desc` is a local `D3DKMT_CLOSEADAPTER`.
        unsafe { D3DKMTCloseAdapter(&raw const desc) == 0 }
    }
}

#[cfg(not(windows))]
mod platform {
    use crate::{LOG, abi::Luid};

    /// None: no D3DKMT natively.
    pub fn open(_luid: Luid) -> Option<u32> {
        LOG.warn("D3DKMTOpenAdapterFromLuid not implemented.");
        None
    }

    /// False: no D3DKMT natively (and no handle to close: `open` gives none).
    pub fn close(_handle: u32) -> bool {
        LOG.warn("D3DKMTCloseAdapter not implemented.");
        false
    }
}
