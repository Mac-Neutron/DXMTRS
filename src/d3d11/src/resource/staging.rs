//! Staging buffer and staging texture resources (`d3d11_resource_staging.cpp`).

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::staging::Staging;
use winemetal_sys::wire::WMTTextureInfo;

use super::{
    Common, Resource, ResourceDesc, ResourceKind,
    helper::{
        create_mtl_texture_descriptor_1d, create_mtl_texture_descriptor_2d,
        create_mtl_texture_descriptor_3d, get_linear_texture_layout_1d,
        get_linear_texture_layout_2d, get_linear_texture_layout_3d, get_mipmap_size_1d,
        get_mipmap_size_2d, get_mipmap_size_3d,
    },
};
use crate::{
    abi::records::{
        D3d11BufferDesc, D3d11SubresourceData, D3d11Texture1dDesc, D3d11Texture2dDesc1,
        D3d11Texture3dDesc1,
    },
    com::{E_FAIL, E_INVALIDARG, Hresult, S_OK},
    device::Device,
    resource::device_supports_family,
    subresource::{
        enumerate_subresources_1d, enumerate_subresources_2d, enumerate_subresources_3d,
    },
};

const fn zero_tex_info() -> WMTTextureInfo {
    WMTTextureInfo {
        pixel_format: 0,
        width: 0,
        height: 0,
        depth: 0,
        array_length: 0,
        bits: 0,
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    }
}

/// Staging resource wrapper (`StagingBuffer`, `StagingTexture`).
pub struct StagingResource {
    pub common: Common,
}

/// Copies initial data into a staging texture subresource (`d3d11_resource_staging.cpp`).
fn copy_initial_staging_subresource(
    staging: &Staging,
    init: *const D3d11SubresourceData,
    subresource_id: u32,
    depth: u32,
    bpr: u32,
    bpi: u32,
    dim: u8,
) {
    if init.is_null() || bpr == 0 {
        return;
    }
    let idx = usize::try_from(subresource_id).unwrap_or(0);
    // SAFETY: `init` points to an array of `D3d11SubresourceData` covering all subresources.
    let sub_ptr = unsafe { init.add(idx) };
    // SAFETY: `sub_ptr` is a valid element of the initial data array.
    let init_sub = unsafe { &*sub_ptr };
    let mapped = staging.mapped_immediate_memory();
    if mapped.is_null() || init_sub.p_sys_mem.is_null() {
        return;
    }
    let slice_pitch = if dim == 3 {
        init_sub.sys_mem_slice_pitch
    } else {
        0
    };
    let row_pitch = if dim == 1 { 0 } else { init_sub.sys_mem_pitch };
    let copy_len = usize::try_from(bpr.min(row_pitch)).unwrap_or(0);
    if copy_len == 0 {
        return;
    }
    let rows = bpi / bpr;
    let src_base = init_sub.p_sys_mem.cast::<u8>();
    for image in 0..depth {
        for row in 0..rows {
            let dst_off = usize::try_from(image * bpi + row * bpr).unwrap_or(0);
            let src_off = usize::try_from(image * slice_pitch + row * row_pitch).unwrap_or(0);
            // SAFETY: `dst_off + copy_len` lies within the staging buffer allocation.
            let dst_ptr = unsafe { mapped.add(dst_off) };
            // SAFETY: `src_off + copy_len` lies within the caller's initial data slice.
            let src_ptr = unsafe { src_base.add(src_off) };
            // SAFETY: `src_ptr` and `dst_ptr` are valid for `copy_len` bytes and do not overlap.
            unsafe { core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, copy_len) };
        }
    }
}

fn create_staging_buffer_inner(
    device: *mut c_void,
    desc: &D3d11BufferDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let byte_width = desc.byte_width;
    let staging = Arc::new(Staging::new(
        mtl,
        u64::from(byte_width),
        byte_width,
        byte_width,
    ));
    if !init.is_null() {
        // SAFETY: `init` was checked non-null; the caller supplies a valid `D3d11SubresourceData`.
        let init_ref = unsafe { &*init };
        let dst = staging.mapped_immediate_memory();
        if !dst.is_null() && !init_ref.p_sys_mem.is_null() && byte_width > 0 {
            let len = usize::try_from(byte_width).unwrap_or(0);
            let src = init_ref.p_sys_mem.cast::<u8>();
            // SAFETY: `src` and `dst` are valid for `len` bytes and do not overlap.
            unsafe { core::ptr::copy_nonoverlapping(src, dst, len) };
        }
    }
    let common = Common {
        device,
        buffer: None,
        texture: None,
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: Some(staging),
        bind_flags: desc.bind_flags,
        usage: desc.usage,
        misc_flags: desc.misc_flags,
        keyed_mutex: None,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::StagingBuffer,
        ResourceDesc::Buffer(*desc),
    );
    // SAFETY: `out` was checked non-null by `create_buffer`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a staging `ID3D11Buffer` (`CreateStagingBuffer` in `d3d11_resource_staging.cpp`).
pub fn create_staging_buffer(
    device: *mut c_void,
    desc: &D3d11BufferDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_staging_buffer_inner(device, desc, init, out)
}

fn create_staging_texture_1d_inner(
    device: *mut c_void,
    desc: &D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let mut final_desc = *desc;
    let mut info = zero_tex_info();
    if create_mtl_texture_descriptor_1d(
        desc,
        &mut final_desc,
        &mut info,
        |f| device_supports_family(device, f),
        true,
    ) < 0
    {
        return E_INVALIDARG;
    }
    let subs = enumerate_subresources_1d(&final_desc);
    let mut subresources = Vec::with_capacity(subs.len());
    for sub in subs {
        let (_, _, d) = get_mipmap_size_1d(&final_desc, sub.mip_level);
        let Ok((bpr, bpi, buf_len)) =
            get_linear_texture_layout_1d(&final_desc, sub.mip_level, true)
        else {
            return E_FAIL;
        };
        let staging = Arc::new(Staging::new(mtl, u64::from(buf_len), bpr, bpi));
        copy_initial_staging_subresource(&staging, init, sub.subresource_id, d, bpr, bpi, 1);
        subresources.push(staging);
    }
    let common = Common {
        device,
        buffer: None,
        texture: None,
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: subresources.first().cloned(),
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex: None,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::StagingTexture,
        ResourceDesc::Texture1d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    unsafe { (*raw).staging_subresources = subresources };
    // SAFETY: `out` was checked non-null by `create_texture1d`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a 1D staging texture (`CreateStagingTexture1D` in `d3d11_resource_staging.cpp`).
pub fn create_staging_texture_1d(
    device: *mut c_void,
    desc: &D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_staging_texture_1d_inner(device, desc, init, out)
}

fn create_staging_texture_2d_inner(
    device: *mut c_void,
    desc: &D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let mut final_desc = *desc;
    let mut info = zero_tex_info();
    if create_mtl_texture_descriptor_2d(
        desc,
        &mut final_desc,
        &mut info,
        |f| device_supports_family(device, f),
        true,
    ) < 0
    {
        return E_INVALIDARG;
    }
    let subs = enumerate_subresources_2d(&final_desc);
    let mut subresources = Vec::with_capacity(subs.len());
    for sub in subs {
        let (_, _, d) = get_mipmap_size_2d(&final_desc, sub.mip_level);
        let Ok((bpr, bpi, buf_len)) =
            get_linear_texture_layout_2d(&final_desc, sub.mip_level, true)
        else {
            return E_FAIL;
        };
        let staging = Arc::new(Staging::new(mtl, u64::from(buf_len), bpr, bpi));
        copy_initial_staging_subresource(&staging, init, sub.subresource_id, d, bpr, bpi, 2);
        subresources.push(staging);
    }
    let common = Common {
        device,
        buffer: None,
        texture: None,
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: subresources.first().cloned(),
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex: None,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::StagingTexture,
        ResourceDesc::Texture2d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    unsafe { (*raw).staging_subresources = subresources };
    // SAFETY: `out` was checked non-null by `create_texture2d1`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a 2D staging texture (`CreateStagingTexture2D` in `d3d11_resource_staging.cpp`).
pub fn create_staging_texture_2d(
    device: *mut c_void,
    desc: &D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_staging_texture_2d_inner(device, desc, init, out)
}

fn create_staging_texture_3d_inner(
    device: *mut c_void,
    desc: &D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let mut final_desc = *desc;
    let mut info = zero_tex_info();
    if create_mtl_texture_descriptor_3d(
        desc,
        &mut final_desc,
        &mut info,
        |f| device_supports_family(device, f),
        true,
    ) < 0
    {
        return E_INVALIDARG;
    }
    let subs = enumerate_subresources_3d(&final_desc);
    let mut subresources = Vec::with_capacity(subs.len());
    for sub in subs {
        let (_, _, d) = get_mipmap_size_3d(&final_desc, sub.mip_level);
        let Ok((bpr, bpi, buf_len)) =
            get_linear_texture_layout_3d(&final_desc, sub.mip_level, true)
        else {
            return E_FAIL;
        };
        let staging = Arc::new(Staging::new(mtl, u64::from(buf_len), bpr, bpi));
        copy_initial_staging_subresource(&staging, init, sub.subresource_id, d, bpr, bpi, 3);
        subresources.push(staging);
    }
    let common = Common {
        device,
        buffer: None,
        texture: None,
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: subresources.first().cloned(),
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex: None,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::StagingTexture,
        ResourceDesc::Texture3d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    unsafe { (*raw).staging_subresources = subresources };
    // SAFETY: `out` was checked non-null by `create_texture3d1`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a 3D staging texture (`CreateStagingTexture3D` in `d3d11_resource_staging.cpp`).
pub fn create_staging_texture_3d(
    device: *mut c_void,
    desc: &D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_staging_texture_3d_inner(device, desc, init, out)
}
