//! `IMTLD3D12CommandListExt::TemporalUpscale` (`d3d12_command_list.cpp:2489`).

use core::ffi::c_void;

use crate::{abi::ext::D3D12UpscaleDesc, com::Hresult};

/// `IMTLD3D12CommandListExt::TemporalUpscale` (`d3d12_command_list.cpp:2489`).
///
/// # Safety
///
/// `this`, `scaler`, and `desc` are caller-supplied pointers.
pub unsafe fn temporal_upscale(
    _this: *mut c_void,
    _scaler: *mut c_void,
    _desc: *const D3D12UpscaleDesc,
) -> Hresult {
    crate::not_implemented!("IMTLD3D12CommandListExt::TemporalUpscale")
}
