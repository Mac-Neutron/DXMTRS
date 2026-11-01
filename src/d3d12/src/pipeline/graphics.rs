//! `MTLD3D12GraphicsPipelineState` (`d3d12_pipeline_graphics.cpp`, `d3d12_device.hpp:251-274`).

use core::ffi::c_void;
use std::sync::Mutex;

use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::Obj;

use crate::{
    abi::vtbl::ID3D12PipelineStateVtbl,
    com::{ComRefCounted, Hresult},
    device::Device,
};

/// `MTLD3D12GraphicsPipelineState` (`d3d12_device.hpp:251-274`).
#[repr(C)]
pub struct Graphics {
    pub vtbl: &'static ID3D12PipelineStateVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub is_compute: bool,
    pub desc_hash: u64,
    pub pso: Option<Obj>,
    pub slot_mask: u32,
    pub fill_mode: u32,
    pub cull_mode: u32,
    pub depth_clip_mode: u32,
    pub winding: u32,
    pub depth_bias: f32,
    pub slope_scale: f32,
    pub depth_bias_clamp: f32,
    pub forced_sample_count: u32,
    pub geometry: bool,
    pub writes_uav: bool,
    pub depth_stencil_states: [Option<Obj>; 6],
    pub geometry_variants: Mutex<[Option<Obj>; 6]>,
}

impl ComRefCounted for Graphics {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Graphics {
    /// Returns (compiling on first use) the mesh-emulation PSO handle for `(strip, index_format)`
    ///
    /// (`GeometryPipeline`, `d3d12_pipeline_graphics.cpp:545`).
    #[must_use]
    pub fn geometry_pipeline(&self, _strip: bool, _index_format: u32) -> u64 {
        crate::not_implemented!("pipeline::graphics::Graphics::geometry_pipeline")
    }

    /// Returns the `MTLDepthStencilState` handle for `(dsv_planar, dsv_readonly_flags)`
    ///
    /// (`GetDepthStencilState`, `d3d12_pipeline_graphics.cpp:575`).
    #[must_use]
    pub fn depth_stencil_state(&self, _dsv_planar: u32, _dsv_readonly_flags: u32) -> u64 {
        crate::not_implemented!("pipeline::graphics::Graphics::depth_stencil_state")
    }
}

/// Computes the build-local description hash for a `D3D12_GRAPHICS_PIPELINE_STATE_DESC`
///
/// (`HashGraphicsDesc`, `d3d12_pipeline_library.cpp:36`).
///
/// # Safety
///
/// `desc` must point to a valid `D3D12_GRAPHICS_PIPELINE_STATE_DESC`.
#[must_use]
pub unsafe fn hash_graphics_desc(_desc: *const c_void) -> u64 {
    crate::not_implemented!("pipeline::graphics::hash_graphics_desc")
}

/// `ID3D12Device::CreateGraphicsPipelineState` (`d3d12_device.cpp:498`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `pso` are caller-supplied COM pointers.
pub unsafe fn create_graphics_pipeline_state(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _pso: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateGraphicsPipelineState");
    crate::not_implemented!("ID3D12Device::CreateGraphicsPipelineState")
}
