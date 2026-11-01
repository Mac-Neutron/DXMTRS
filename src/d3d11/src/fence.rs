//! `ID3D11Fence` over `WMT::SharedEvent` and D3DKMT (`d3d11_fence.cpp`).

use core::ffi::c_void;

use crate::com::{E_NOTIMPL, Guid, Hresult};

/// D3D11 fence object (`MTLD3D11Fence`).
#[derive(Default)]
pub struct Fence {
    event: u64,
}

impl Fence {
    /// Recovers `&Fence` from an `ID3D11Fence*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }

    /// Returns the `WMT::SharedEvent` handle (`MTLD3D11Fence::event`).
    #[must_use]
    pub const fn event(&self) -> u64 {
        self.event
    }
}

/// `ID3D11Device5::CreateFence`.
pub const fn create_fence(
    device: *mut c_void,
    initial_value: u64,
    flags: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, initial_value, flags, riid, out);
    E_NOTIMPL
}

/// `ID3D11Device5::OpenSharedFence`.
pub const fn open_shared_fence(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, handle, riid, out);
    E_NOTIMPL
}
