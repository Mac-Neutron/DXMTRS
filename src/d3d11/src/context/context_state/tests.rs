use core::{ffi::c_void, ptr};

use super::create_device_context_state;
use crate::{
    abi::{
        IID_ID3D_DEVICE_CONTEXT_STATE, IID_ID3D11_DEVICE_CHILD, IID_ID3D11_DEVICE_CONTEXT,
        IID_ID3D11_DEVICE_CONTEXT_1, IID_IUNKNOWN, vtbl::ID3DDeviceContextStateVtbl,
    },
    com::{E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, S_FALSE, S_OK, release},
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

#[test]
fn query_interface_counts_as_add_ref() {
    let fl = 0xb000_u32;
    let iid_ctx = IID_ID3D11_DEVICE_CONTEXT;
    let mut this: *mut c_void = ptr::null_mut();
    assert_eq!(
        create_device_context_state(
            ptr::null_mut(),
            0,
            (&raw const fl, 1),
            7,
            (&raw const iid_ctx).cast(),
            ptr::null_mut(),
            &raw mut this,
        ),
        S_OK
    );
    // SAFETY: `this` is a live `DeviceContextState` whose first word is its vtable.
    let vtbl_ptr = unsafe { *this.cast::<*const ID3DDeviceContextStateVtbl>() };
    // SAFETY: `vtbl_ptr` is `DCS_VTBL`, a static.
    let vtbl = unsafe { &*vtbl_ptr };
    let mut out: *mut c_void = ptr::null_mut();
    // SAFETY: `this` is live; `out` is null or writable.
    let qi = |riid: *const Guid, out: *mut *mut c_void| unsafe {
        (vtbl.query_interface)(this, riid.cast(), out)
    };
    // SAFETY: `this` is live and holds a reference.
    let add_ref = || unsafe { (vtbl.add_ref)(this) };
    // SAFETY: as above.
    let release = || unsafe { (vtbl.release)(this) };
    let base = add_ref() - 1;
    assert_eq!(release(), base);

    for iid in [
        IID_IUNKNOWN,
        IID_ID3D11_DEVICE_CHILD,
        IID_ID3D_DEVICE_CONTEXT_STATE,
    ] {
        assert_eq!(qi(&raw const iid, &raw mut out), S_OK);
        assert_eq!(out, this);
        assert_eq!(add_ref(), base + 2);
        assert_eq!(release(), base + 1);
        assert_eq!(release(), base);
    }
    out = this;
    assert_eq!(qi(ptr::null(), &raw mut out), E_INVALIDARG);
    assert!(out.is_null());
    let iunknown = IID_IUNKNOWN;
    assert_eq!(qi(&raw const iunknown, ptr::null_mut()), E_POINTER);
    assert_eq!(add_ref(), base + 1);
    assert_eq!(release(), base);

    // SAFETY: releases the initial reference from `create_device_context_state`.
    unsafe { crate::com::release(this) };
}
