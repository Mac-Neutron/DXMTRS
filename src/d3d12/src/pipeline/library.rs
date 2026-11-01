//! `ID3D12PipelineLibrary1` (`d3d12_pipeline_library.cpp`): `DXMTPLB1` serialization and loading.

use core::ffi::c_void;
use std::sync::Mutex;

use dxmt_util::{com::ComObject, private_data::PrivateData};
use rustc_hash::FxHashMap;

use crate::{
    abi::vtbl::ID3D12PipelineLibrary1Vtbl,
    com::{ComRefCounted, Hresult},
    device::Device,
};

/// `MTLD3D12PipelineLibrary` (`d3d12_pipeline_library.cpp:87`).
#[repr(C)]
pub struct PipelineLibrary {
    pub vtbl: &'static ID3D12PipelineLibrary1Vtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub entries: Mutex<FxHashMap<Vec<u16>, (u8, u64)>>,
}

impl ComRefCounted for PipelineLibrary {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

/// `ID3D12Device1::CreatePipelineLibrary` (`d3d12_device.cpp:1482`,
///
/// `d3d12_pipeline_library.cpp:259`).
///
/// # Safety
///
/// `this`, `blob`, `riid`, and `library` are caller-supplied pointers.
pub unsafe fn create_pipeline_library(
    _this: *mut c_void,
    _blob: *const c_void,
    _blob_len: usize,
    _riid: *const c_void,
    _library: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreatePipelineLibrary");
    crate::not_implemented!("ID3D12Device::CreatePipelineLibrary")
}
