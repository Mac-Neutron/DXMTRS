//! `ID3D12CommandSignature` (`d3d12_command_signature.cpp`): indirect-argument layouts and runtime
//!
//! MSL resolver kernels.

use core::ffi::c_void;

use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::Obj;

use crate::{
    abi::vtbl::ID3D12CommandSignatureVtbl,
    com::{ComRefCounted, Hresult},
    device::Device,
};

/// `MTLD3D12CommandSignatureImpl` (`d3d12_command_signature.cpp:94`, `d3d12_device.hpp:207-220`).
#[repr(C)]
pub struct CommandSignature {
    pub vtbl: &'static ID3D12CommandSignatureVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub command_type: u32,
    pub update_root_arguments: bool,
    pub update_vertex_buffers: bool,
    pub update_index_buffer: bool,
    pub byte_stride: u32,
    pub render_resolver: Option<Obj>,
    pub compute_resolver: Option<Obj>,
}

impl ComRefCounted for CommandSignature {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

/// Casts a non-null `ID3D12CommandSignature` pointer if its vtable matches [`CommandSignature`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_command_signature<'a>(raw: *mut c_void) -> Option<&'a CommandSignature> {
    let _ = raw;
    crate::not_implemented!("command_signature::cast_command_signature")
}

/// `ID3D12Device::CreateCommandSignature` (`d3d12_device.cpp:1458`).
///
/// # Safety
///
/// `this`, `desc`, `root_sig`, `riid`, and `cmd_sig` are caller-supplied COM pointers.
pub unsafe fn create_command_signature(
    _this: *mut c_void,
    _desc: *const c_void,
    _root_sig: *mut c_void,
    _riid: *const c_void,
    _cmd_sig: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandSignature");
    crate::not_implemented!("ID3D12Device::CreateCommandSignature")
}
