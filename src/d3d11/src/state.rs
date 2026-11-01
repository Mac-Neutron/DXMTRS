//! D3D11 blend, rasterizer, depth-stencil and sampler state objects (`d3d11_state_object.cpp`).

use core::ffi::c_void;

use winemetal_wire::{WMTRenderPipelineBlendInfo, WmtcmdRenderSetrasterizerstate};

use crate::{
    abi::records::{
        D3d11BlendDesc, D3d11BlendDesc1, D3d11DepthStencilDesc, D3d11RasterizerDesc,
        D3d11RasterizerDesc1, D3d11RasterizerDesc2, D3d11SamplerDesc,
    },
    com::{E_NOTIMPL, Hresult},
};

pub mod cache;

/// Blend state object (`MTLD3D11BlendState`).
pub struct BlendState {
    blend_info: WMTRenderPipelineBlendInfo,
    dual_source: bool,
}

impl BlendState {
    /// Recovers `&BlendState` from an `ID3D11BlendState*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }

    /// Returns the precomputed Metal blend attachment info.
    #[must_use]
    pub const fn info(&self) -> &WMTRenderPipelineBlendInfo {
        &self.blend_info
    }

    /// Returns `true` when dual-source blending is enabled (`IsDualSourceBlending`).
    #[must_use]
    pub const fn is_dual_source_blending(&self) -> bool {
        self.dual_source
    }

    /// Configures Metal render pipeline color attachments (`SetupMetalPipelineDescriptor`).
    pub const fn setup_metal_pipeline_descriptor(
        &self,
        blend_info: &mut WMTRenderPipelineBlendInfo,
        num_rt: u32,
        valid_rt: u32,
    ) {
        let _ = (blend_info, num_rt, valid_rt);
    }
}

/// Rasterizer state object (`MTLD3D11RasterizerState`).
pub struct RasterizerState {
    scissor_enabled: bool,
    uav_only_sample_count: u32,
}

impl RasterizerState {
    /// Recovers `&RasterizerState` from an `ID3D11RasterizerState*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }

    /// Writes this rasterizer state's fields into `cmd` (`SetupRasterizerState`).
    pub const fn setup(&self, cmd: &mut WmtcmdRenderSetrasterizerstate) {
        let _ = cmd;
    }

    /// Returns `true` when scissor testing is enabled (`IsScissorEnabled`).
    #[must_use]
    pub const fn is_scissor_enabled(&self) -> bool {
        self.scissor_enabled
    }

    /// Returns the forced sample count for UAV-only rendering (`UAVOnlySampleCount`).
    #[must_use]
    pub const fn uav_only_sample_count(&self) -> u32 {
        self.uav_only_sample_count
    }
}

/// Depth-stencil state object (`MTLD3D11DepthStencilState`).
pub struct DepthStencilState {
    handles: [u64; 4],
    enabled: bool,
}

impl DepthStencilState {
    /// Recovers `&DepthStencilState` from an `ID3D11DepthStencilState*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }

    /// Returns the `WMT::DepthStencilState` handle for `planar` mask (`GetDepthStencilState`).
    #[must_use]
    pub const fn state(&self, planar: u32) -> u64 {
        self.handles[(planar & 3) as usize]
    }

    /// Returns `true` when depth or stencil testing is enabled (`IsEnabled`).
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Sampler state object (`MTLD3D11SamplerState`).
pub struct SamplerState {
    pub handle: u64,
    pub argument: u64,
    pub lod_bias: f32,
}

impl SamplerState {
    /// Recovers `&SamplerState` from an `ID3D11SamplerState*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }
}

/// Default depth-stencil state (`kDefaultDepthStencilDesc`).
pub struct Defaults {
    pub blend: *mut c_void,
    pub rasterizer: *mut c_void,
    pub depth_stencil: *mut c_void,
    pub sampler: *mut c_void,
}

impl Default for Defaults {
    fn default() -> Self {
        Self::new(core::ptr::null_mut())
    }
}

impl Defaults {
    /// Creates default state objects for `device`.
    #[must_use]
    pub const fn new(device: *mut c_void) -> Self {
        let _ = device;
        Self {
            blend: core::ptr::null_mut(),
            rasterizer: core::ptr::null_mut(),
            depth_stencil: core::ptr::null_mut(),
            sampler: core::ptr::null_mut(),
        }
    }
}

/// `ID3D11Device::CreateBlendState`.
pub const fn create_blend_state(
    device: *mut c_void,
    desc: *const D3d11BlendDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device1::CreateBlendState1`.
pub const fn create_blend_state1(
    device: *mut c_void,
    desc: *const D3d11BlendDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateRasterizerState`.
pub const fn create_rasterizer_state(
    device: *mut c_void,
    desc: *const D3d11RasterizerDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device1::CreateRasterizerState1`.
pub const fn create_rasterizer_state1(
    device: *mut c_void,
    desc: *const D3d11RasterizerDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device3::CreateRasterizerState2`.
pub const fn create_rasterizer_state2(
    device: *mut c_void,
    desc: *const D3d11RasterizerDesc2,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateDepthStencilState`.
pub const fn create_depth_stencil_state(
    device: *mut c_void,
    desc: *const D3d11DepthStencilDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateSamplerState`.
pub const fn create_sampler_state(
    device: *mut c_void,
    desc: *const D3d11SamplerDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, out);
    E_NOTIMPL
}
