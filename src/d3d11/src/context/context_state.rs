//! `ID3DDeviceContextState` and `SwapDeviceContextState` (`d3d11_context_state.hpp`).

use core::ffi::c_void;

use super::{Context, Mode};
use crate::com::{E_NOTIMPL, Hresult};

/// Creates a new `ID3DDeviceContextState` (`ID3D11Device1::CreateDeviceContextState`).
#[must_use]
pub const fn create_device_context_state(
    device: *mut c_void,
    flags: u32,
    feature_levels: (*const u32, u32),
    sdk_version: u32,
    emulated_interface: *const c_void,
    chosen_feature_level: *mut u32,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (
        device,
        flags,
        feature_levels,
        sdk_version,
        emulated_interface,
        chosen_feature_level,
        out,
    );
    E_NOTIMPL
}

impl<M: Mode> Context<M> {
    /// `ID3D11DeviceContext1::SwapDeviceContextState`.
    pub const fn swap_device_context_state(
        &mut self,
        state: *mut c_void,
        previous_state: *mut *mut c_void,
    ) {
        let _ = (state, previous_state);
    }
}
