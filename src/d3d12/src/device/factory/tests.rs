use core::{ffi::c_void, ptr};

use super::{ext_query_interface, ext_release, query_interface};
use crate::{
    abi::ext::{IID_D3D12_DEVICE_DOWNLEVEL, IID_D3D12_DEVICE_EXT},
    com::{
        E_NOINTERFACE, E_POINTER, Guid, IID_ID3D12_DEVICE, IID_ID3D12_DEVICE1, IID_ID3D12_DEVICE2,
        IID_ID3D12_DEVICE3, IID_ID3D12_DEVICE4, IID_ID3D12_DEVICE5, IID_ID3D12_DEVICE6,
        IID_ID3D12_DEVICE7, IID_ID3D12_DEVICE8, IID_ID3D12_DEVICE9, IID_ID3D12_DEVICE10,
        IID_ID3D12_OBJECT, IID_IUNKNOWN, S_OK, release_com,
    },
    device::Device,
};

const IID_IDXGI_DEVICE: Guid = Guid::new(
    0x54ec_77fa,
    0x1377,
    0x44e6,
    [0x8c, 0x32, 0x88, 0xfd, 0x5f, 0x44, 0xc8, 0x4c],
);
const IID_IDXGI_DEVICE1: Guid = Guid::new(
    0x77db_970f,
    0x6276,
    0x48ba,
    [0xba, 0x28, 0x07, 0x01, 0x43, 0xb4, 0x39, 0x2c],
);
const IID_IDXGI_DEVICE2: Guid = Guid::new(
    0x0500_8617,
    0xfbfd,
    0x4051,
    [0xa7, 0x90, 0x14, 0x48, 0x84, 0xb4, 0xf6, 0xa9],
);
const IID_IDXGI_DEVICE3: Guid = Guid::new(
    0x6007_896c,
    0x3244,
    0x4afd,
    [0xbf, 0x18, 0xa6, 0xd3, 0xbe, 0xda, 0x50, 0x23],
);
const IID_IDXGI_DEVICE4: Guid = Guid::new(
    0x95b4_f95f,
    0xd8da,
    0x4ca4,
    [0x9e, 0xe6, 0x3b, 0x76, 0xd5, 0x96, 0x8a, 0x10],
);
const IID_ID3D10_DEVICE: Guid = Guid::new(
    0x9b7e_4c0f,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);
const IID_ID3D10_DEVICE1: Guid = Guid::new(
    0x9b7e_4c8f,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);
const IID_ID3D11_DEVICE: Guid = Guid::new(
    0xdb6f_6ddb,
    0xac77,
    0x4e88,
    [0x82, 0x53, 0x81, 0x9d, 0xf9, 0xbb, 0xf1, 0x40],
);
const IID_ID3D11_DEVICE1: Guid = Guid::new(
    0xa04b_fb29,
    0x08ef,
    0x43d6,
    [0xa4, 0x9c, 0xa9, 0xbd, 0xbd, 0xcb, 0xe6, 0x86],
);
const IID_ID3D11_DEVICE2: Guid = Guid::new(
    0x9d06_dffa,
    0xd1e5,
    0x4d07,
    [0x83, 0xa8, 0x1b, 0xb1, 0x23, 0xf2, 0xf8, 0x41],
);
const IID_ID3D11_DEVICE3: Guid = Guid::new(
    0xa05c_8c37,
    0xd2c6,
    0x4732,
    [0xb3, 0xa0, 0x9c, 0xe0, 0xb0, 0xdc, 0x9a, 0xe6],
);
const IID_ID3D11_DEVICE4: Guid = Guid::new(
    0x8992_ab71,
    0x02e6,
    0x4b8d,
    [0xba, 0x48, 0xb0, 0x56, 0xdc, 0xda, 0x42, 0xc4],
);
const IID_ID3D11_DEVICE5: Guid = Guid::new(
    0x8ffd_e202,
    0xa0e7,
    0x45df,
    [0x9e, 0x01, 0xe8, 0x37, 0x80, 0x1b, 0x5e, 0xa0],
);

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
