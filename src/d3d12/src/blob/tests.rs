use core::{ffi::c_void, ptr};

use crate::{
    abi::vtbl::ID3D10BlobVtbl,
    blob,
    com::{
        E_INVALIDARG, E_NOINTERFACE, E_POINTER, IID_ID3D10_BLOB, IID_ID3D12_OBJECT, IID_IUNKNOWN,
        S_OK,
    },
};

#[repr(C)]
struct RawBlob {
    vtbl: *const ID3D10BlobVtbl,
}

#[test]
fn qi_answers_id3d10blob() {
    // Null output pointer returns E_INVALIDARG.
    // SAFETY: Testing null out-pointer validation.
    let hr_null_out = unsafe { blob::create_blob_from_slice(b"test", ptr::null_mut()) };
    assert_eq!(hr_null_out, E_INVALIDARG);

    let payload = b"dxmt-d3d12-blob-payload";
    let mut raw: *mut c_void = ptr::null_mut();
    // SAFETY: `raw` is a valid writable out-pointer.
    let hr = unsafe { blob::create_blob_from_slice(payload, &raw mut raw) };
    assert_eq!(hr, S_OK);
    assert!(!raw.is_null());

    // SAFETY: `raw` is a newly created `ID3D10Blob` COM pointer.
    let raw_blob = unsafe { &*raw.cast::<RawBlob>() };
    // SAFETY: `raw_blob.vtbl` points to the static `BLOB_VTBL`.
    let vtbl = unsafe { &*raw_blob.vtbl };

    // SAFETY: `raw` is valid for `GetBufferSize`.
    let size = unsafe { (vtbl.get_buffer_size)(raw) };
    assert_eq!(size, payload.len());

    // SAFETY: `raw` is valid for `GetBufferPointer`.
    let data_ptr = unsafe { (vtbl.get_buffer_pointer)(raw) };
    assert!(!data_ptr.is_null());
    // SAFETY: `data_ptr` points to `size` initialized bytes owned by `raw`.
    let slice = unsafe { core::slice::from_raw_parts(data_ptr.cast::<u8>(), size) };
    assert_eq!(slice, payload);

    // Null output pointer to QueryInterface returns E_POINTER.
    // SAFETY: Testing `QueryInterface` null-check with valid `raw` and `IID_ID3D10_BLOB`.
    let hr_null = unsafe {
        (vtbl.query_interface)(
            raw,
            ptr::from_ref(&IID_ID3D10_BLOB).cast::<c_void>(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_null, E_POINTER);

    // QueryInterface for IID_ID3D10_BLOB succeeds and increments refcount.
    let mut out_blob: *mut c_void = ptr::null_mut();
    // SAFETY: `raw` and `out_blob` are valid for `QueryInterface`.
    let hr_blob = unsafe {
        (vtbl.query_interface)(
            raw,
            ptr::from_ref(&IID_ID3D10_BLOB).cast::<c_void>(),
            &raw mut out_blob,
        )
    };
    assert_eq!(hr_blob, S_OK);
    assert_eq!(out_blob, raw);

    // QueryInterface for IID_IUNKNOWN succeeds and increments refcount.
    let mut out_unk: *mut c_void = ptr::null_mut();
    // SAFETY: `raw` and `out_unk` are valid for `QueryInterface`.
    let hr_unk = unsafe {
        (vtbl.query_interface)(
            raw,
            ptr::from_ref(&IID_IUNKNOWN).cast::<c_void>(),
            &raw mut out_unk,
        )
    };
    assert_eq!(hr_unk, S_OK);
    assert_eq!(out_unk, raw);

    // QueryInterface for an unrelated IID returns E_NOINTERFACE and zeroes out-pointer.
    let mut out_bad: *mut c_void = raw;
    // SAFETY: `raw` and `out_bad` are valid for `QueryInterface`.
    let hr_bad = unsafe {
        (vtbl.query_interface)(
            raw,
            ptr::from_ref(&IID_ID3D12_OBJECT).cast::<c_void>(),
            &raw mut out_bad,
        )
    };
    assert_eq!(hr_bad, E_NOINTERFACE);
    assert!(out_bad.is_null());

    // Release the two queried references and the initial reference.
    // SAFETY: `out_unk` holds a strong reference from `QueryInterface`.
    assert_eq!(unsafe { (vtbl.release)(out_unk) }, 2);
    // SAFETY: `out_blob` holds a strong reference from `QueryInterface`.
    assert_eq!(unsafe { (vtbl.release)(out_blob) }, 1);
    // SAFETY: `raw` holds the final strong reference.
    assert_eq!(unsafe { (vtbl.release)(raw) }, 0);
}
