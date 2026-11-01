//! `ID3D10Blob` (`src/d3d10/d3d10_blob.cpp`): byte buffers returned by root-signature serialization
//!
//! and pipeline libraries.

use core::ffi::c_void;

use dxmt_util::com::ComObject;

use crate::{
    abi::vtbl::ID3D10BlobVtbl,
    com::{ComRefCounted, Hresult},
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

/// Allocates a new [`Blob`] copying `bytes` (`CreateBlobFromMalloc`).
///
/// # Safety
///
/// `out` must be a valid writable `*mut *mut c_void` (or null).
pub unsafe fn create_blob_from_slice(_bytes: &[u8], _out: *mut *mut c_void) -> Hresult {
    crate::not_implemented!("blob::create_blob_from_slice")
}

/// Allocates a new [`Blob`] taking ownership of `bytes`.
///
/// # Safety
///
/// `out` must be a valid writable `*mut *mut c_void` (or null).
pub unsafe fn create_blob(_bytes: Vec<u8>, _out: *mut *mut c_void) -> Hresult {
    crate::not_implemented!("blob::create_blob")
}
