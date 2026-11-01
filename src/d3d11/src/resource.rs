//! D3D11 resources (`d3d11_resource.hpp`, `d3d11_buffer.cpp`, `d3d11_texture*.cpp`).

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::{
    dynamic::{DynamicBuffer, DynamicLinearTexture},
    resource::{Buffer, Texture},
    staging::Staging,
};

use crate::{
    abi::records::{
        D3d11BufferDesc, D3d11SubresourceData, D3d11Texture1dDesc, D3d11Texture2dDesc,
        D3d11Texture2dDesc1, D3d11Texture3dDesc, D3d11Texture3dDesc1,
    },
    com::{E_NOTIMPL, Guid, Hresult},
    keyed_mutex::KeyedMutexRef,
};

pub mod buffer;
pub mod dynamic;
pub mod helper;
pub mod linear;
pub mod staging;
pub mod texture;

/// Shared resource fields accessed across lanes (`MTLResourceCommon`).
#[derive(Default)]
pub struct Common {
    pub buffer: Option<Arc<Buffer>>,
    pub texture: Option<Arc<Texture>>,
    pub dynamic_buffer: Option<Arc<DynamicBuffer>>,
    pub dynamic_texture: Option<Arc<DynamicLinearTexture>>,
    pub staging: Option<Arc<Staging>>,
    pub bind_flags: u32,
    pub usage: u32,
    pub misc_flags: u32,
    pub keyed_mutex: Option<KeyedMutexRef>,
    pub min_lod: f32,
}

/// Recovers `&ResourceCommon` from an `ID3D11Resource*` pointer (`GetCommonResource`).
#[must_use]
pub const fn common(resource: *mut c_void) -> Option<&'static Common> {
    let _ = resource;
    None
}

/// Returns the [`DynamicBuffer`] for `resource` (`GetDynamicBuffer`).
#[must_use]
pub fn dynamic_buffer(resource: *mut c_void) -> Option<&'static Arc<DynamicBuffer>> {
    common(resource)?.dynamic_buffer.as_ref()
}

/// Returns the [`DynamicLinearTexture`] for `resource` (`GetDynamicLinearTexture`).
#[must_use]
pub fn dynamic_texture(resource: *mut c_void) -> Option<&'static Arc<DynamicLinearTexture>> {
    common(resource)?.dynamic_texture.as_ref()
}

/// Returns the [`Staging`] buffer for `subresource` of `resource` (`GetStagingResource`).
#[must_use]
pub fn staging(resource: *mut c_void, subresource: u32) -> Option<&'static Arc<Staging>> {
    let _ = subresource;
    common(resource)?.staging.as_ref()
}

/// `ID3D11Device::CreateBuffer`.
pub const fn create_buffer(
    device: *mut c_void,
    desc: *const D3d11BufferDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, init, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateTexture1D`.
pub const fn create_texture1d(
    device: *mut c_void,
    desc: *const D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, init, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateTexture2D`.
pub const fn create_texture2d(
    device: *mut c_void,
    desc: *const D3d11Texture2dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, init, out);
    E_NOTIMPL
}

/// `ID3D11Device3::CreateTexture2D1`.
pub const fn create_texture2d1(
    device: *mut c_void,
    desc: *const D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, init, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateTexture3D`.
pub const fn create_texture3d(
    device: *mut c_void,
    desc: *const D3d11Texture3dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, init, out);
    E_NOTIMPL
}

/// `ID3D11Device3::CreateTexture3D1`.
pub const fn create_texture3d1(
    device: *mut c_void,
    desc: *const D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, init, out);
    E_NOTIMPL
}

/// `ID3D11Device::OpenSharedResource` / `OpenSharedResource1` / `OpenSharedResourceByName`.
pub const fn open_shared(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, handle, riid, out);
    E_NOTIMPL
}
