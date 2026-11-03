use core::{ffi::c_void, ptr};

use super::create_device_context_state;
use crate::{
    abi::{
        IID_ID3D_DEVICE_CONTEXT_STATE, IID_ID3D11_DEVICE_CHILD, IID_ID3D11_DEVICE_CONTEXT,
        IID_ID3D11_DEVICE_CONTEXT_1, IID_IUNKNOWN, vtbl::ID3DDeviceContextStateVtbl,
    },
    com::{E_NOINTERFACE, Guid, S_FALSE, S_OK, release},
};

#[test]
fn create_device_context_state_validation_and_com_interfaces() {
    let fl = 0xb000_u32;
    let mut chosen = 0_u32;
    let mut out: *mut c_void = ptr::null_mut();
    let iid_ctx = IID_ID3D11_DEVICE_CONTEXT;
    let iid_ctx1 = IID_ID3D11_DEVICE_CONTEXT_1;

    // Null output pointer -> S_FALSE.
    assert_eq!(
        create_device_context_state(
            ptr::null_mut(),
            0,
            (&raw const fl, 1),
            7,
            (&raw const iid_ctx1).cast(),
            &raw mut chosen,
            ptr::null_mut(),
        ),
        S_FALSE
    );
    assert_eq!(chosen, 0);

    // Valid creation -> S_OK.
    assert_eq!(
        create_device_context_state(
            ptr::null_mut(),
            0,
            (&raw const fl, 1),
            7,
            (&raw const iid_ctx).cast(),
            ptr::null_mut(),
            &raw mut out,
        ),
        S_OK
    );
    assert!(!out.is_null());

    // QueryInterface for IUnknown, ID3D11DeviceChild, and ID3DDeviceContextState.
    // SAFETY: `out` is a live `DeviceContextState` whose first word is `ID3DDeviceContextStateVtbl`.
    let vtbl = unsafe { *out.cast::<*const ID3DDeviceContextStateVtbl>() };
    // SAFETY: `vtbl` points to `DCS_VTBL`.
    let qi = unsafe { (*vtbl).query_interface };
    for iid in [
        IID_IUNKNOWN,
        IID_ID3D11_DEVICE_CHILD,
        IID_ID3D_DEVICE_CONTEXT_STATE,
    ] {
        let mut queried: *mut c_void = ptr::null_mut();
        // SAFETY: calls `QueryInterface` on a live `DeviceContextState`.
        let hr = unsafe { qi(out, (&raw const iid).cast(), &raw mut queried) };
        assert_eq!(hr, S_OK);
        assert_eq!(queried, out);
        // SAFETY: releases the reference acquired from `QueryInterface`.
        unsafe { release(queried) };
    }

    let bad_iid = Guid::new(0x1111_1111, 0x2222, 0x3333, [0x44; 8]);
    let mut queried: *mut c_void = ptr::null_mut();
    // SAFETY: calls `QueryInterface` with an unknown IID.
    let hr = unsafe { qi(out, (&raw const bad_iid).cast(), &raw mut queried) };
    assert_eq!(hr, E_NOINTERFACE);
    assert!(queried.is_null());

    // SAFETY: releases the initial reference from `create_device_context_state`.
    unsafe { release(out) };
}
