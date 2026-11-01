//! `ID3D10Blob` (`src/d3d10/d3d10_blob.cpp`): byte buffers returned by root-signature serialization
//!
//! and pipeline libraries.

use core::ffi::c_void;

use dxmt_util::com::ComObject;

use crate::{
    abi::vtbl::ID3D10BlobVtbl,
    com::{
        self, ComRefCounted, E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, Hresult,
        IID_ID3D10_BLOB, IID_IUNKNOWN, S_OK,
    },
    slot,
};

/// `ID3D10Blob` COM object (`src/d3d10/d3d10_blob.cpp`).
#[repr(C)]
pub struct Blob {
    pub vtbl: &'static ID3D10BlobVtbl,
    pub com: ComObject,
    pub bytes: Box<[u8]>,
}

impl ComRefCounted for Blob {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

static BLOB_VTBL: ID3D10BlobVtbl = ID3D10BlobVtbl {
    query_interface: slot!((this: *mut c_void, riid: *const c_void, ppv_object: *mut *mut c_void) -> Hresult => {
        if ppv_object.is_null() {
            return E_POINTER;
        }
        // SAFETY: `ppv_object` was checked non-null above.
        unsafe { com::init_out_ptr(ppv_object) };
        // SAFETY: `riid` is a caller-supplied GUID pointer.
        let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        if iid == IID_IUNKNOWN || iid == IID_ID3D10_BLOB {
            // SAFETY: `this` is a live `Blob`.
            unsafe { com::add_ref_com(this.cast::<Blob>()) };
            // SAFETY: `ppv_object` is non-null and writable.
            unsafe { ppv_object.write_unaligned(this) };
            return S_OK;
        }
        if com::qi_log(&IID_ID3D10_BLOB, &iid) {
            crate::warn!("ID3D10Blob::QueryInterface: Unknown interface query {iid}");
        }
        E_NOINTERFACE
    }),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `Blob`.
        unsafe { com::add_ref_com(this.cast::<Blob>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `Blob`.
        unsafe { com::release_com(this.cast::<Blob>()) }
    }),
    get_buffer_pointer: slot!((this: *mut c_void) -> *mut c_void => {
        // SAFETY: `this` is a live `Blob`.
        let blob = unsafe { &*this.cast::<Blob>() };
        blob.bytes.as_ptr().cast_mut().cast::<c_void>()
    }),
    get_buffer_size: slot!((this: *mut c_void) -> usize => {
        // SAFETY: `this` is a live `Blob`.
        let blob = unsafe { &*this.cast::<Blob>() };
        blob.bytes.len()
    }),
};

/// Allocates a new [`Blob`] copying `bytes` (`CreateBlobFromMalloc`).
///
/// # Safety
///
/// `out` must be a valid writable `*mut *mut c_void` (or null).
pub unsafe fn create_blob_from_slice(bytes: &[u8], out: *mut *mut c_void) -> Hresult {
    // SAFETY: forwarded from caller.
    unsafe { create_blob(bytes.to_vec(), out) }
}

/// Allocates a new [`Blob`] taking ownership of `bytes`.
///
/// # Safety
///
/// `out` must be a valid writable `*mut *mut c_void` (or null).
pub unsafe fn create_blob(bytes: Vec<u8>, out: *mut *mut c_void) -> Hresult {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let blob = Box::new(Blob {
        vtbl: &BLOB_VTBL,
        com: ComObject::with_initial_ref(),
        bytes: bytes.into_boxed_slice(),
    });
    let raw = Box::into_raw(blob).cast::<c_void>();
    // SAFETY: `out` was checked non-null above.
    unsafe { out.write_unaligned(raw) };
    S_OK
}

#[cfg(test)]
mod tests;
