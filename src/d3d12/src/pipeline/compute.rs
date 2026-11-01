//! `MTLD3D12ComputePipelineState` (`d3d12_pipeline_compute.cpp`, `d3d12_device.hpp:276-283`).

use core::ffi::c_void;

use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::Obj;
use winemetal_wire::WMTSize;

use crate::{
    abi::vtbl::ID3D12PipelineStateVtbl,
    com::{ComRefCounted, Hresult},
    device::Device,
};

/// `MTLD3D12ComputePipelineState` (`d3d12_device.hpp:276-283`).
#[repr(C)]
pub struct Compute {
    pub vtbl: &'static ID3D12PipelineStateVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub is_compute: bool,
    pub desc_hash: u64,
    pub pso: Option<Obj>,
    pub threadgroup_size: WMTSize,
}

impl ComRefCounted for Compute {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

/// Computes the build-local description hash for a `D3D12_COMPUTE_PIPELINE_STATE_DESC`
///
/// (`HashComputeDesc`, `d3d12_pipeline_library.cpp:78`).
///
/// # Safety
///
/// `desc` must point to a valid `D3D12_COMPUTE_PIPELINE_STATE_DESC`.
#[must_use]
pub unsafe fn hash_compute_desc(_desc: *const c_void) -> u64 {
    crate::not_implemented!("pipeline::compute::hash_compute_desc")
}

/// `ID3D12Device::CreateComputePipelineState` (`d3d12_device.cpp:508`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `pso` are caller-supplied COM pointers.
pub unsafe fn create_compute_pipeline_state(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _pso: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateComputePipelineState");
    crate::not_implemented!("ID3D12Device::CreateComputePipelineState")
}
