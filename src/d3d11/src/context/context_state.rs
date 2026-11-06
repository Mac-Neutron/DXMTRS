//! `ID3DDeviceContextState` and `SwapDeviceContextState` (`d3d11_context_state.hpp`).

use core::{ffi::c_void, ptr};

use dxmt_util::{
    com::{Qi, QiMiss, qi_table},
    private_data::{
        HasPrivateData, PrivateData, get_private_data, set_private_data, set_private_data_interface,
    },
};

use super::{
    Context, Mode,
    state::{ComPtr, ContextState},
};
use crate::{
    abi::{
        IID_ID3D_DEVICE_CONTEXT_STATE, IID_ID3D11_DEVICE_CHILD, IID_IUNKNOWN,
        vtbl::ID3DDeviceContextStateVtbl,
    },
    com::{E_INVALIDARG, Hresult, Managed, S_FALSE, S_OK, add_ref, release},
};

/// `ID3DDeviceContextState` implementation (`MTLD3D11DeviceContextState`).
#[repr(C)]
pub struct DeviceContextState {
    pub vtbl: *const ID3DDeviceContextStateVtbl,
    pub refs: Managed,
    pub private_data: PrivateData,
    pub device: *mut c_void,
    pub state: ContextState,
}

unsafe extern "system-unwind" fn dcs_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let qi = Qi {
        null_riid: E_INVALIDARG,
        silent: &[],
        log: &crate::LOG,
        miss: QiMiss::Once {
            object: &IID_ID3D_DEVICE_CONTEXT_STATE,
            name: "D3DDeviceContextState",
        },
    };
    let add_ref = || {
        // SAFETY: `this` is a live `DeviceContextState`.
        let obj = unsafe { &*this.cast::<DeviceContextState>() };
        // SAFETY: `obj.device` is either null or the owning device.
        let _ = unsafe { obj.refs.add_ref(obj.device) };
    };
    // SAFETY: the application's `riid` and `out`, null or valid.
    unsafe {
        qi_table(
            riid,
            out,
            &[(
                &[
                    IID_IUNKNOWN,
                    IID_ID3D11_DEVICE_CHILD,
                    IID_ID3D_DEVICE_CONTEXT_STATE,
                ],
                this,
            )],
            add_ref,
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn dcs_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `DeviceContextState`.
    let obj = unsafe { &*this.cast::<DeviceContextState>() };
    // SAFETY: `obj.device` is either null or the owning device.
    unsafe { obj.refs.add_ref(obj.device) }
}

unsafe extern "system-unwind" fn dcs_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `DeviceContextState`.
    let obj = unsafe { &*this.cast::<DeviceContextState>() };
    // SAFETY: `obj.device` is either null or the owning device.
    let (remaining, destroy) = unsafe { obj.refs.release(obj.device) };
    if destroy {
        // SAFETY: `this` was allocated via `Box::into_raw` and its last reference was released.
        let boxed = unsafe { Box::from_raw(this.cast::<DeviceContextState>()) };
        drop(boxed);
    }
    remaining
}

unsafe extern "system-unwind" fn dcs_get_device(this: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `DeviceContextState`.
    let obj = unsafe { &*this.cast::<DeviceContextState>() };
    // SAFETY: `obj.device` is either null or the owning device.
    unsafe { add_ref(obj.device) };
    // SAFETY: `out` was checked non-null.
    unsafe { *out = obj.device };
}

impl HasPrivateData for DeviceContextState {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `DeviceContextState`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

static DCS_VTBL: ID3DDeviceContextStateVtbl = ID3DDeviceContextStateVtbl {
    query_interface: dcs_query_interface,
    add_ref: dcs_add_ref,
    release: dcs_release,
    get_device: dcs_get_device,
    get_private_data: get_private_data::<DeviceContextState>,
    set_private_data: set_private_data::<DeviceContextState>,
    set_private_data_interface: set_private_data_interface::<DeviceContextState>,
};

const fn write_opt<T: Copy>(out: *mut T, value: T) {
    if !out.is_null() {
        // SAFETY: `out` was checked non-null and points to caller-supplied output storage.
        unsafe { *out = value };
    }
}

fn add_initial_device_ref(refs: &Managed, device: *mut c_void) {
    // SAFETY: `device` is either null or a live `ID3D11Device` whose public refcount we bump
    // on the 0 -> 1 transition (`ref(new MTLD3D11DeviceContextState(this))`).
    let _ = unsafe { refs.add_ref(device) };
}

impl DeviceContextState {
    /// Allocates a new [`DeviceContextState`] with reference count 1.
    #[must_use]
    pub fn new_raw(device: *mut c_void) -> *mut Self {
        let refs = Managed::new();
        add_initial_device_ref(&refs, device);
        Box::into_raw(Box::new(Self {
            vtbl: &raw const DCS_VTBL,
            refs,
            private_data: PrivateData::default(),
            device,
            state: ContextState::default(),
        }))
    }
}

/// Creates a new `ID3DDeviceContextState` (`ID3D11Device1::CreateDeviceContextState`).
#[must_use]
pub fn create_device_context_state(
    device: *mut c_void,
    flags: u32,
    feature_levels: (*const u32, u32),
    sdk_version: u32,
    emulated_interface: *const c_void,
    chosen_feature_level: *mut u32,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (
        flags,
        feature_levels,
        sdk_version,
        emulated_interface,
        chosen_feature_level,
    );
    write_opt(out, ptr::null_mut());
    if out.is_null() {
        return S_FALSE;
    }
    let dcs = DeviceContextState::new_raw(device);
    write_opt(out, dcs.cast::<c_void>());
    S_OK
}

impl<M: Mode> Context<M> {
    fn swap_device_context_state_inner(
        &mut self,
        state: *mut c_void,
        previous_state: *mut *mut c_void,
    ) {
        write_opt(previous_state, ptr::null_mut());
        if state.is_null() {
            return;
        }

        self.reset_encoding_context_state();

        let prev_raw = self.state.active_state_object.take_raw();
        let prev_dcs = if prev_raw.is_null() {
            DeviceContextState::new_raw(self.device)
        } else {
            prev_raw.cast::<DeviceContextState>()
        };

        let next_dcs = state.cast::<DeviceContextState>();
        if prev_dcs == next_dcs {
            if !previous_state.is_null() {
                // SAFETY: `prev_dcs` is a live `DeviceContextState`.
                unsafe { add_ref(prev_dcs.cast()) };
                write_opt(previous_state, prev_dcs.cast());
            }
            self.state.active_state_object = ComPtr::from_owned_raw(prev_dcs.cast());
            self.restore_encoding_context_state();
            return;
        }

        // SAFETY: `prev_dcs` is a live `DeviceContextState` distinct from `next_dcs`.
        let prev_ref = unsafe { &mut *prev_dcs };
        prev_ref.state = core::mem::take(&mut self.state);

        if !previous_state.is_null() {
            // SAFETY: `prev_dcs` is a live `DeviceContextState`.
            unsafe { add_ref(prev_dcs.cast()) };
            write_opt(previous_state, prev_dcs.cast());
        }
        // SAFETY: releases the context's active reference to `prev_dcs`.
        unsafe { release(prev_dcs.cast()) };

        // SAFETY: `next_dcs` is a live `DeviceContextState` provided by the caller.
        let next_ref = unsafe { &mut *next_dcs };
        self.state = core::mem::take(&mut next_ref.state);
        let _ = self.state.active_state_object.set(state);
        self.reset_encoding_context_state();
        self.restore_encoding_context_state();
    }

    /// `ID3D11DeviceContext1::SwapDeviceContextState`.
    pub fn swap_device_context_state(
        &mut self,
        state: *mut c_void,
        previous_state: *mut *mut c_void,
    ) {
        self.swap_device_context_state_inner(state, previous_state);
    }
}

#[cfg(test)]
mod tests;
