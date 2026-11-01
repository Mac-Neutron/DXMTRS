//! D3D11 resource views (`d3d11_view.hpp`: `Srv`, `Rtv`, `Dsv`, `Uav`).

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::{
    counter::Counter,
    encoder::BufferSlice,
    resource::{Buffer, BufferViewKey, Texture, TextureViewKey},
};

use crate::{
    abi::records::{
        D3d11DepthStencilViewDesc, D3d11RenderTargetViewDesc, D3d11RenderTargetViewDesc1,
        D3d11ShaderResourceViewDesc, D3d11ShaderResourceViewDesc1, D3d11UnorderedAccessViewDesc,
        D3d11UnorderedAccessViewDesc1,
    },
    com::{E_NOTIMPL, Hresult},
    subresource::ResourceSubset,
};

pub mod helper;

/// Shader resource view (`MTLD3D11ShaderResourceView`).
pub struct Srv {
    pub resource: *mut c_void,
    pub buffer: Option<Arc<Buffer>>,
    pub texture: Option<Arc<Texture>>,
    pub buffer_view: Option<BufferViewKey>,
    pub texture_view: Option<TextureViewKey>,
    pub slice: BufferSlice,
    pub subset: ResourceSubset,
    pub format: u32,
}

impl Srv {
    /// Recovers `&Srv` from an `ID3D11ShaderResourceView*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }
}

/// Render target view (`MTLD3D11RenderTargetView`).
pub struct Rtv {
    pub resource: *mut c_void,
    pub buffer: Option<Arc<Buffer>>,
    pub texture: Option<Arc<Texture>>,
    pub buffer_view: Option<BufferViewKey>,
    pub texture_view: Option<TextureViewKey>,
    pub slice: BufferSlice,
    pub subset: ResourceSubset,
    pub format: u32,
    pub pixel_format: u32,
    pub array_length: u32,
    pub width: u32,
    pub height: u32,
    pub sample_count: u32,
}

impl Rtv {
    /// Recovers `&Rtv` from an `ID3D11RenderTargetView*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }
}

/// Depth-stencil view (`MTLD3D11DepthStencilView`).
pub struct Dsv {
    pub resource: *mut c_void,
    pub texture: Option<Arc<Texture>>,
    pub texture_view: Option<TextureViewKey>,
    pub subset: ResourceSubset,
    pub format: u32,
    pub planar_flags: u8,
    pub readonly_flags: u8,
    pub array_length: u32,
    pub width: u32,
    pub height: u32,
    pub sample_count: u32,
}

impl Dsv {
    /// Recovers `&Dsv` from an `ID3D11DepthStencilView*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }
}

/// Unordered access view (`MTLD3D11UnorderedAccessView`).
pub struct Uav {
    pub resource: *mut c_void,
    pub buffer: Option<Arc<Buffer>>,
    pub texture: Option<Arc<Texture>>,
    pub buffer_view: Option<BufferViewKey>,
    pub texture_view: Option<TextureViewKey>,
    pub slice: BufferSlice,
    pub subset: ResourceSubset,
    pub format: u32,
    pub counter: Option<Arc<Counter>>,
}

impl Uav {
    /// Recovers `&Uav` from an `ID3D11UnorderedAccessView*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }
}

/// `ID3D11Device::CreateShaderResourceView`.
pub const fn create_srv(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, resource, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device3::CreateShaderResourceView1`.
pub const fn create_srv1(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, resource, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateRenderTargetView`.
pub const fn create_rtv(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, resource, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device3::CreateRenderTargetView1`.
pub const fn create_rtv1(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, resource, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateDepthStencilView`.
pub const fn create_dsv(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11DepthStencilViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, resource, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateUnorderedAccessView`.
pub const fn create_uav(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, resource, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device3::CreateUnorderedAccessView1`.
pub const fn create_uav1(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, resource, desc, out);
    E_NOTIMPL
}
