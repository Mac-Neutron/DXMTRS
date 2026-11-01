//! Texture resources (`d3d12_texture.cpp`): committed, placed, and swap-chain textures and
//!
//! SRV/UAV/RTV/DSV view creation.

use core::ffi::c_void;

use crate::{
    abi::{
        resource::{HeapProperties, ResourceDesc},
        vtbl::D3d12CpuDescriptorHandle,
    },
    com::Hresult,
    device::Device,
    heap::HeapObject,
    resource::Resource,
};

/// Creates a texture [`Resource`] (`CreateTextureResource`, `d3d12_texture.cpp`).
///
/// # Safety
///
/// `device`, `optimized_clear`, `placed`, and `out_com` are caller-supplied pointers.
pub unsafe fn create(
    _device: *mut Device,
    _heap: (&HeapProperties, u32),
    _desc: &ResourceDesc,
    _optimized_clear: *const c_void,
    _placed: Option<(*mut HeapObject, u64)>,
    _castable_formats: &[u32],
    _out_com: (*const c_void, *mut *mut c_void),
) -> Hresult {
    crate::not_implemented!("resource::texture::create")
}

/// Creates a texture SRV in `descriptor` (`d3d12_texture.cpp:365`).
///
/// # Safety
///
/// `desc` is null or points to a valid `D3D12_SHADER_RESOURCE_VIEW_DESC`.
pub unsafe fn create_srv(
    _resource: &Resource,
    _desc: *const c_void,
    _descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::not_implemented!("resource::texture::create_srv")
}

/// Creates a texture UAV in `descriptor` (`d3d12_texture.cpp:419`).
///
/// # Safety
///
/// `counter` and `desc` are caller-supplied pointers.
pub unsafe fn create_uav(
    _resource: &Resource,
    _counter: *mut c_void,
    _desc: *const c_void,
    _descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::not_implemented!("resource::texture::create_uav")
}

/// Creates an RTV in `descriptor` (`d3d12_texture.cpp:486`).
///
/// # Safety
///
/// `desc` is null or points to a valid `D3D12_RENDER_TARGET_VIEW_DESC`.
pub unsafe fn create_rtv(
    _resource: &Resource,
    _desc: *const c_void,
    _descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::not_implemented!("resource::texture::create_rtv")
}

/// Creates a DSV in `descriptor` (`d3d12_texture.cpp:545`).
///
/// # Safety
///
/// `desc` is null or points to a valid `D3D12_DEPTH_STENCIL_VIEW_DESC`.
pub unsafe fn create_dsv(
    _resource: &Resource,
    _desc: *const c_void,
    _descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::not_implemented!("resource::texture::create_dsv")
}
