//! `ID3D12RootSignature` (`d3d12_root_signature.cpp:347-527`, `d3d12_device.hpp:184-205`).

use core::ffi::c_void;

use dxmt_util::{com::ComObject, private_data::PrivateData};

use crate::{
    abi::vtbl::ID3D12RootSignatureVtbl,
    com::{ComRefCounted, Hresult},
    device::Device,
    root_signature::QwordOffsets,
    shader_cache::Sha1Digest,
};

/// `MTLD3D12RootSignatureImpl` (`d3d12_device.hpp:184-205`, `d3d12_root_signature.cpp:347`).
#[repr(C)]
pub struct RootSignature {
    pub vtbl: &'static ID3D12RootSignatureVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub blob: Box<[u8]>,
    pub qword_offsets: QwordOffsets,
    pub static_samplers: Box<[u64]>,
    pub flags: u32,
    pub sha1: Sha1Digest,
}

impl ComRefCounted for RootSignature {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl RootSignature {
    /// Total qwords uploaded to the root-argument table per draw/dispatch (`UploadQwords`).
    #[must_use]
    pub const fn upload_qwords(&self) -> u32 {
        self.qword_offsets.total
    }

    /// Number of root parameter slots (`ParameterSlots`).
    #[must_use]
    pub fn parameter_slots(&self) -> u32 {
        u32::try_from(self.qword_offsets.parameters.len()).unwrap_or(0)
    }

    /// Qword offset of each root parameter slot (`SlotQwordOffsets`).
    #[must_use]
    pub fn slot_qword_offsets(&self) -> &[u32] {
        &self.qword_offsets.parameters
    }

    /// Qword offset of the directly-indexed heap pair (`~0u` when absent; `HeapQwordOffset`).
    #[must_use]
    pub const fn heap_qword_offset(&self) -> u32 {
        self.qword_offsets.heaps
    }

    /// Qword offset of the 64-bit atomic lock table (`LockQwordOffset`).
    #[must_use]
    pub const fn lock_qword_offset(&self) -> u32 {
        self.qword_offsets.lock
    }

    /// Encoded static sampler descriptors (4 qwords per static sampler).
    #[must_use]
    pub fn encoded_static_samplers(&self) -> &[u64] {
        &self.static_samplers
    }
}

/// Casts a non-null `ID3D12RootSignature` pointer if its vtable matches [`RootSignature`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_root_signature<'a>(raw: *mut c_void) -> Option<&'a RootSignature> {
    let _ = raw;
    crate::not_implemented!("root_signature::object::cast_root_signature")
}

/// `ID3D12Device::CreateRootSignature` (`d3d12_device.cpp:829`).
///
/// # Safety
///
/// `this`, `blob`, `riid`, and `root_sig` are caller-supplied pointers.
pub unsafe fn create_root_signature(
    _this: *mut c_void,
    _node_mask: u32,
    _blob: *const c_void,
    _length: usize,
    _riid: *const c_void,
    _root_sig: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateRootSignature");
    crate::not_implemented!("ID3D12Device::CreateRootSignature")
}
