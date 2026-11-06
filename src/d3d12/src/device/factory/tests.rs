use core::{ffi::c_void, ptr};

use super::{ext_query_interface, ext_release, query_interface};
use crate::{
    abi::ext::{IID_D3D12_DEVICE_DOWNLEVEL, IID_D3D12_DEVICE_EXT},
    com::{
        E_NOINTERFACE, E_POINTER, Guid, IID_ID3D10_DEVICE, IID_ID3D10_DEVICE1, IID_ID3D11_DEVICE,
        IID_ID3D11_DEVICE1, IID_ID3D11_DEVICE2, IID_ID3D11_DEVICE3, IID_ID3D11_DEVICE4,
        IID_ID3D11_DEVICE5, IID_ID3D12_DEVICE, IID_ID3D12_DEVICE1, IID_ID3D12_DEVICE2,
        IID_ID3D12_DEVICE3, IID_ID3D12_DEVICE4, IID_ID3D12_DEVICE5, IID_ID3D12_DEVICE6,
        IID_ID3D12_DEVICE7, IID_ID3D12_DEVICE8, IID_ID3D12_DEVICE9, IID_ID3D12_DEVICE10,
        IID_ID3D12_OBJECT, IID_IDXGI_DEVICE, IID_IDXGI_DEVICE1, IID_IDXGI_DEVICE2,
        IID_IDXGI_DEVICE3, IID_IDXGI_DEVICE4, IID_IUNKNOWN, S_OK, release_com,
    },
    device::Device,
};

#[test]
fn qi_matrix_as_probe() {
    let raw = Box::into_raw(Device::new_dummy());
    let this = raw.cast::<c_void>();

    // Null output pointer returns `E_POINTER`.
    let iid_unk = IID_IUNKNOWN;
    // SAFETY: `this` is a live `Device` and `iid_unk` is valid.
    let hr = unsafe { query_interface(this, (&raw const iid_unk).cast(), ptr::null_mut()) };
    assert_eq!(hr, E_POINTER);

    let answered = [
        IID_IUNKNOWN,
        IID_ID3D12_OBJECT,
        IID_ID3D12_DEVICE,
        IID_ID3D12_DEVICE1,
        IID_ID3D12_DEVICE2,
        IID_ID3D12_DEVICE3,
        IID_ID3D12_DEVICE4,
        IID_ID3D12_DEVICE5,
        IID_ID3D12_DEVICE6,
        IID_ID3D12_DEVICE7,
        IID_ID3D12_DEVICE8,
        IID_ID3D12_DEVICE9,
        IID_ID3D12_DEVICE10,
    ];
    for iid in answered {
        let mut out: *mut c_void = ptr::null_mut();
        // SAFETY: `this`, `iid`, and `out` are valid pointers.
        let hr = unsafe { query_interface(this, (&raw const iid).cast(), &raw mut out) };
        assert_eq!(hr, S_OK);
        assert_eq!(out, this);
        // SAFETY: `out` holds 1 public reference from `query_interface`.
        assert_eq!(unsafe { release_com(raw) }, 1);
    }

    let refused = [
        IID_IDXGI_DEVICE,
        IID_IDXGI_DEVICE1,
        IID_IDXGI_DEVICE2,
        IID_IDXGI_DEVICE3,
        IID_IDXGI_DEVICE4,
        IID_ID3D10_DEVICE,
        IID_ID3D10_DEVICE1,
        IID_ID3D11_DEVICE,
        IID_ID3D11_DEVICE1,
        IID_ID3D11_DEVICE2,
        IID_ID3D11_DEVICE3,
        IID_ID3D11_DEVICE4,
        IID_ID3D11_DEVICE5,
        IID_D3D12_DEVICE_DOWNLEVEL,
        Guid::new(1, 2, 3, [4, 5, 6, 7, 8, 9, 10, 11]),
    ];
    for iid in refused {
        let mut out: *mut c_void = ptr::without_provenance_mut(0xdead);
        // SAFETY: `this`, `iid`, and `out` are valid pointers.
        let hr = unsafe { query_interface(this, (&raw const iid).cast(), &raw mut out) };
        assert_eq!(hr, E_NOINTERFACE);
        assert!(out.is_null());
    }

    // SAFETY: releases the initial public reference on `raw`.
    assert_eq!(unsafe { release_com(raw) }, 0);
}

#[test]
fn ext_only_with_temporal_scaler() {
    let raw = Box::into_raw(Device::new_dummy());
    let this = raw.cast::<c_void>();

    // Without temporal scaler support (`new_dummy` defaults to `false`), `IMTLD3D12DeviceExt` is
    // refused with `E_NOINTERFACE`.
    let iid_ext = IID_D3D12_DEVICE_EXT;
    let iid_unk = IID_IUNKNOWN;
    let mut out: *mut c_void = ptr::without_provenance_mut(0xdead);
    // SAFETY: `this`, `iid_ext`, and `out` are valid pointers.
    let hr = unsafe { query_interface(this, (&raw const iid_ext).cast(), &raw mut out) };
    assert_eq!(hr, E_NOINTERFACE);
    assert!(out.is_null());

    // Enable temporal scaler support and verify companion pointer + COM identity law.
    // SAFETY: `raw` is uniquely owned here.
    unsafe { (*raw).temporal_supported = true };
    // SAFETY: `this`, `iid_ext`, and `out` are valid pointers.
    let hr = unsafe { query_interface(this, (&raw const iid_ext).cast(), &raw mut out) };
    assert_eq!(hr, S_OK);
    assert_eq!(out.addr(), raw.addr() + Device::EXT_OFFSET);

    let mut unk: *mut c_void = ptr::null_mut();
    // SAFETY: `out` is the companion pointer at `Device::EXT_OFFSET`.
    let hr = unsafe { ext_query_interface(out, (&raw const iid_unk).cast(), &raw mut unk) };
    assert_eq!(hr, S_OK);
    assert_eq!(unk, this);

    // SAFETY: release `unk`, `out`, and the initial reference on `raw`.
    assert_eq!(unsafe { release_com(raw) }, 2);
    // SAFETY: `out` is at `Device::EXT_OFFSET` inside `raw`.
    assert_eq!(unsafe { ext_release(out) }, 1);
    // SAFETY: releases the final public reference on `raw`.
    assert_eq!(unsafe { release_com(raw) }, 0);
}
