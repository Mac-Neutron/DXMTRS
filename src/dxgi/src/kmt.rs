//! D3DKMT's adapter handle for an adapter's LUID.
//!
//! gdi32's on PE; natively DXMT's stubs, which return 0 (`util_d3dkmt.h`).

#[cfg(windows)]
use crate::LOG;
use crate::abi::Luid;
#[cfg(windows)]
use crate::abi::{D3dkmtCloseAdapter, D3dkmtOpenAdapterFromLuid};

#[cfg(windows)]
unsafe extern "system" {
    fn D3DKMTOpenAdapterFromLuid(desc: *mut D3dkmtOpenAdapterFromLuid) -> i32;
    fn D3DKMTCloseAdapter(desc: *const D3dkmtCloseAdapter) -> i32;
}

/// Opens the D3DKMT adapter of `luid`: its handle, or 0 after a warning on Windows.
#[cfg(windows)]
#[must_use]
pub fn open(luid: Luid) -> u32 {
    let mut desc = D3dkmtOpenAdapterFromLuid {
        adapter_luid: luid,
        ..Default::default()
    };
    // SAFETY: `desc` is a local `D3DKMT_OPENADAPTERFROMLUID` the call fills.
    let status = unsafe { D3DKMTOpenAdapterFromLuid(&raw mut desc) };
    if status == 0 {
        desc.adapter
    } else {
        LOG.warn("Failed to open D3DKMT adapter");
        0
    }
}

/// Opens the D3DKMT adapter of `luid`: 0 natively (no D3DKMT on macOS/Linux).
#[cfg(not(windows))]
#[must_use]
pub const fn open(_luid: Luid) -> u32 {
    0
}

/// Closes a handle `open` gave, warning if that fails; 0, no handle, is left alone.
#[cfg(windows)]
pub fn close(handle: u32) {
    if handle != 0 {
        let desc = D3dkmtCloseAdapter { adapter: handle };
        // SAFETY: `desc` is a local `D3DKMT_CLOSEADAPTER`.
        if unsafe { D3DKMTCloseAdapter(&raw const desc) != 0 } {
            LOG.warn("Failed to close D3DKMT adapter");
        }
    }
}

/// Closes a handle `open` gave: no-op natively (no D3DKMT on macOS/Linux).
#[cfg(not(windows))]
pub const fn close(_handle: u32) {}
