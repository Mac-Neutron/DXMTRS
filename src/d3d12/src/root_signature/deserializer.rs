//! `ID3D12RootSignatureDeserializer` and `ID3D12VersionedRootSignatureDeserializer`
//!
//! (`d3d12_root_signature.cpp:229-345`).

use core::ffi::c_void;

use crate::com::{Guid, Hresult};

/// `D3D12CreateRootSignatureDeserializer` (`d3d12_root_signature.cpp:331-345`).
///
/// # Safety
///
/// `src_data`, `deserializer_interface`, and `deserializer` are caller-supplied pointers.
pub unsafe fn create_deserializer(
    _src_data: *const c_void,
    _src_data_size_in_bytes: usize,
    _deserializer_interface: *const Guid,
    _deserializer: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("D3D12CreateRootSignatureDeserializer")
}

/// `D3D12CreateVersionedRootSignatureDeserializer` (`d3d12_root_signature.cpp:312-328`).
///
/// # Safety
///
/// `src_data`, `deserializer_interface`, and `deserializer` are caller-supplied pointers.
pub unsafe fn create_versioned_deserializer(
    _src_data: *const c_void,
    _src_data_size_in_bytes: usize,
    _deserializer_interface: *const Guid,
    _deserializer: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("D3D12CreateVersionedRootSignatureDeserializer")
}

/// `D3D12SerializeRootSignature` (`d3d12_root_signature.cpp:229-243`).
///
/// # Safety
///
/// `root_signature`, `blob`, and `error_blob` are caller-supplied pointers.
pub unsafe fn serialize_root_signature(
    _root_signature: *const c_void,
    _version: u32,
    _blob: *mut *mut c_void,
    _error_blob: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("D3D12SerializeRootSignature")
}

/// `D3D12SerializeVersionedRootSignature` (`d3d12_root_signature.cpp:178-226`).
///
/// # Safety
///
/// `root_signature`, `blob`, and `error_blob` are caller-supplied pointers.
pub unsafe fn serialize_versioned_root_signature(
    _root_signature: *const c_void,
    _blob: *mut *mut c_void,
    _error_blob: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("D3D12SerializeVersionedRootSignature")
}
