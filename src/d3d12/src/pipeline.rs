//! `ID3D12PipelineState` and `ID3D12PipelineLibrary1` (`d3d12_pipeline_*.cpp`).

pub mod compute;
pub mod graphics;
pub mod library;
pub mod record;

use core::ffi::c_void;

pub use self::{compute::Compute, graphics::Graphics};
use crate::com::Hresult;

/// Resolved `ID3D12PipelineState` kind (`IsComputePipelineState`, `d3d12_device.hpp:244`).
#[derive(Clone, Copy)]
pub enum PipelineKind<'a> {
    Graphics(&'a Graphics),
    Compute(&'a Compute),
}

/// Casts a non-null `ID3D12PipelineState` pointer into [`PipelineKind`] if its vtable matches
///
/// [`Graphics`] or [`Compute`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_pipeline<'a>(raw: *mut c_void) -> Option<PipelineKind<'a>> {
    let _ = raw;
    crate::not_implemented!("pipeline::cast_pipeline")
}

/// `ID3D12Device2::CreatePipelineState` (`d3d12_device.cpp:1503`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `pso` are caller-supplied COM pointers.
pub unsafe fn create_pipeline_state(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _pso: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreatePipelineState");
    crate::not_implemented!("ID3D12Device::CreatePipelineState")
}
