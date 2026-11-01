//! Dynamic linear texture (`d3d11_texture_linear.cpp`).

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::{
    dynamic::DynamicLinearTexture,
    encoder::BufferSlice,
    format::{MTL_DXGI_FORMAT_BC, MTL_DXGI_FORMAT_EMULATED_LINEAR_DEPTH_STENCIL, query_dxgi},
    resource::{Texture, TextureAllocationFlags, TextureViewDescriptor},
};
use winemetal_sys::wire::{WMT_PIXEL_FORMAT_INVALID, WMT_TEXTURE_TYPE2D, WMTTextureInfo};

use super::{
    Common, Resource, ResourceDesc, ResourceKind, device_supports_family,
    helper::{
        create_mtl_texture_descriptor_1d, create_mtl_texture_descriptor_2d, extract_entire_srv_1d,
        extract_entire_srv_2d, get_linear_texture_layout_1d, get_linear_texture_layout_2d,
    },
};
use crate::{
    abi::records::{
        D3d11ShaderResourceViewDesc1, D3d11SubresourceData, D3d11Texture1dDesc, D3d11Texture2dDesc1,
    },
    com::{E_FAIL, E_INVALIDARG, Hresult, S_FALSE, S_OK},
    device::Device,
    subresource::ResourceSubset,
    view::{Srv, SrvObject},
};

const D3D11_USAGE_IMMUTABLE: u32 = 1;
const D3D11_USAGE_DYNAMIC: u32 = 2;
const D3D11_RESOURCE_MISC_TEXTURECUBE: u32 = 0x4;

const D3D11_SRV_DIMENSION_TEXTURE1D: u32 = 2;
const D3D11_SRV_DIMENSION_TEXTURE2D: u32 = 4;

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

/// Linear texture resource wrapper (`MTLD3D11DynamicLinearTexture`).
pub struct LinearTextureResource {
    pub common: Common,
}

/// Copies initial data into a newly allocated linear texture (`d3d11_texture_linear.cpp`).
fn copy_initial_linear_data(
    mapped: *mut u8,
    height: u32,
    bytes_per_row: u32,
    bytes_per_image: u32,
    init: *const D3d11SubresourceData,
) {
    if init.is_null() || mapped.is_null() {
        return;
    }
    // SAFETY: `init` was checked non-null; the caller provides a valid `D3d11SubresourceData`.
    let init_ref = unsafe { &*init };
    if init_ref.p_sys_mem.is_null() {
        return;
    }
    let src_base = init_ref.p_sys_mem.cast::<u8>();
    if init_ref.sys_mem_pitch == bytes_per_row {
        let len = usize::try_from(bytes_per_image).unwrap_or(0);
        if len > 0 {
            // SAFETY: `src_base` and `mapped` are valid for `len` bytes and do not overlap.
            unsafe { core::ptr::copy_nonoverlapping(src_base, mapped, len) };
        }
    } else {
        let copy_len = usize::try_from(init_ref.sys_mem_pitch.min(bytes_per_row)).unwrap_or(0);
        if copy_len == 0 {
            return;
        }
        for row in 0..height {
            let dst_off = usize::try_from(row * bytes_per_row).unwrap_or(0);
            let src_off = usize::try_from(row * init_ref.sys_mem_pitch).unwrap_or(0);
            // SAFETY: `dst_off + copy_len <= bytes_per_image` within the linear texture buffer.
            let dst_ptr = unsafe { mapped.add(dst_off) };
            // SAFETY: `src_off + copy_len` lies within the caller's initial data slice.
            let src_ptr = unsafe { src_base.add(src_off) };
            // SAFETY: `src_ptr` and `dst_ptr` are valid for `copy_len` bytes and do not overlap.
            unsafe { core::ptr::copy_nonoverlapping(src_ptr, dst_ptr, copy_len) };
        }
    }
}

fn create_dynamic_linear_texture_1d_inner(
    device: *mut c_void,
    desc: &D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.array_size > 1 {
        return E_FAIL;
    }
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let (hr, fmt_desc) = query_dxgi(desc.format);
    if hr < 0 {
        return E_FAIL;
    }
    if (fmt_desc.flag & (MTL_DXGI_FORMAT_BC | MTL_DXGI_FORMAT_EMULATED_LINEAR_DEPTH_STENCIL)) != 0
        || fmt_desc.pixel_format == WMT_PIXEL_FORMAT_INVALID
    {
        return E_FAIL;
    }
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
    if info.mipmap_level_count() > 1 {
        return E_FAIL;
    }
    let Ok((bytes_per_row, _, bytes_per_image)) =
        get_linear_texture_layout_1d(&final_desc, 0, true)
    else {
        return E_FAIL;
    };
    let texture = Arc::new(Texture::new_linear(
        bytes_per_image,
        bytes_per_row,
        info,
        mtl,
    ));
    let mut flags = TextureAllocationFlags::SHADER_READONLY;
    if final_desc.usage == D3D11_USAGE_DYNAMIC {
        flags.insert(TextureAllocationFlags::CPU_WRITE_COMBINED);
    }
    if final_desc.usage == D3D11_USAGE_IMMUTABLE {
        flags.insert(TextureAllocationFlags::GPU_READONLY);
    }
    if final_desc.usage != D3D11_USAGE_DYNAMIC {
        flags.insert(TextureAllocationFlags::GPU_MANAGED);
    }
    let allocation = texture.allocate(flags);
    let prev = texture.rename(Arc::clone(&allocation));
    debug_assert!(prev.is_none());
    copy_initial_linear_data(
        allocation.mapped_memory(),
        texture.info().height,
        bytes_per_row,
        bytes_per_image,
        init,
    );
    let dynamic_texture = DynamicLinearTexture::new(Arc::clone(&texture), flags).map(Arc::new);
    let common = Common {
        device,
        buffer: None,
        texture: Some(texture),
        dynamic_buffer: None,
        dynamic_texture,
        staging: None,
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex: None,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::DynamicLinearTexture,
        ResourceDesc::Texture1d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    let res = unsafe { &mut *raw };
    res.linear_bytes_per_row = bytes_per_row;
    res.linear_bytes_per_image = bytes_per_image;
    // SAFETY: `out` was checked non-null by `create_texture1d`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a 1D dynamic linear texture (`d3d11_texture_linear.cpp`).
pub fn create_dynamic_linear_texture_1d(
    device: *mut c_void,
    desc: &D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_dynamic_linear_texture_1d_inner(device, desc, init, out)
}

fn create_dynamic_linear_texture_2d_inner(
    device: *mut c_void,
    desc: &D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.sample_desc.count > 1
        || (desc.misc_flags & D3D11_RESOURCE_MISC_TEXTURECUBE) != 0
        || desc.array_size > 1
    {
        return E_FAIL;
    }
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let (hr, fmt_desc) = query_dxgi(desc.format);
    if hr < 0 {
        return E_FAIL;
    }
    if (fmt_desc.flag & (MTL_DXGI_FORMAT_BC | MTL_DXGI_FORMAT_EMULATED_LINEAR_DEPTH_STENCIL)) != 0
        || fmt_desc.pixel_format == WMT_PIXEL_FORMAT_INVALID
    {
        return E_FAIL;
    }
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
    if info.mipmap_level_count() > 1 {
        return E_FAIL;
    }
    let Ok((bytes_per_row, _, bytes_per_image)) =
        get_linear_texture_layout_2d(&final_desc, 0, true)
    else {
        return E_FAIL;
    };
    let texture = Arc::new(Texture::new_linear(
        bytes_per_image,
        bytes_per_row,
        info,
        mtl,
    ));
    let mut flags = TextureAllocationFlags::SHADER_READONLY;
    if final_desc.usage == D3D11_USAGE_DYNAMIC {
        flags.insert(TextureAllocationFlags::CPU_WRITE_COMBINED);
    }
    if final_desc.usage == D3D11_USAGE_IMMUTABLE {
        flags.insert(TextureAllocationFlags::GPU_READONLY);
    }
    if final_desc.usage != D3D11_USAGE_DYNAMIC {
        flags.insert(TextureAllocationFlags::GPU_MANAGED);
    }
    let allocation = texture.allocate(flags);
    let prev = texture.rename(Arc::clone(&allocation));
    debug_assert!(prev.is_none());
    copy_initial_linear_data(
        allocation.mapped_memory(),
        texture.info().height,
        bytes_per_row,
        bytes_per_image,
        init,
    );
    let dynamic_texture = DynamicLinearTexture::new(Arc::clone(&texture), flags).map(Arc::new);
    let common = Common {
        device,
        buffer: None,
        texture: Some(texture),
        dynamic_buffer: None,
        dynamic_texture,
        staging: None,
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex: None,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::DynamicLinearTexture,
        ResourceDesc::Texture2d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    let res = unsafe { &mut *raw };
    res.linear_bytes_per_row = bytes_per_row;
    res.linear_bytes_per_image = bytes_per_image;
    // SAFETY: `out` was checked non-null by `create_texture2d1`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a 2D dynamic linear texture (`d3d11_texture_linear.cpp`).
pub fn create_dynamic_linear_texture_2d(
    device: *mut c_void,
    desc: &D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_dynamic_linear_texture_2d_inner(device, desc, init, out)
}

fn create_linear_texture_srv_1d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    if resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `resource` is a non-null primary `Resource` pointer.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture1d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(mut final_desc) = extract_entire_srv_1d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    if final_desc.view_dimension != D3D11_SRV_DIMENSION_TEXTURE1D {
        return E_FAIL;
    }
    // SAFETY: `view_dimension == D3D11_SRV_DIMENSION_TEXTURE1D` selects `u.texture1d`.
    let mut t1d = unsafe { final_desc.u.texture1d };
    if t1d.mip_levels == u32::MAX {
        t1d.mip_levels = tex_desc.mip_levels.wrapping_sub(t1d.most_detailed_mip);
        final_desc.u.texture1d = t1d;
    }
    if t1d.most_detailed_mip != 0 || t1d.mip_levels != 1 {
        return E_FAIL;
    }
    if out.is_null() {
        return S_FALSE;
    }
    let (hr, fmt_desc) = query_dxgi(final_desc.format);
    if hr < 0 {
        return E_FAIL;
    }
    let Some(texture) = res.common.texture.as_ref() else {
        return E_FAIL;
    };
    let descriptor = TextureViewDescriptor {
        format: fmt_desc.pixel_format,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    };
    let view_key = descriptor.key();
    let subset = ResourceSubset::from_texture_view(
        &descriptor,
        texture.info().mipmap_level_count(),
        texture.array_length(),
        0,
    );
    let srv = Srv {
        resource,
        buffer: None,
        texture: Some(Arc::clone(texture)),
        buffer_view: None,
        texture_view: Some(view_key),
        slice: BufferSlice {
            byte_offset: 0,
            byte_length: 0,
            first_element: 0,
            element_count: 0,
        },
        subset,
        format: fmt_desc.pixel_format,
    };
    let raw = SrvObject::into_raw(srv, device, res.common.bind_flags, final_desc);
    // SAFETY: `out` was checked non-null above.
    unsafe { *out = raw };
    S_OK
}

/// Creates a 1D SRV on a dynamic linear texture (`d3d11_texture_linear.cpp`).
pub fn create_linear_texture_srv_1d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_linear_texture_srv_1d_inner(device, resource, desc, out)
}

fn create_linear_texture_srv_2d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    if resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `resource` is a non-null primary `Resource` pointer.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture2d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(mut final_desc) = extract_entire_srv_2d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    if final_desc.view_dimension != D3D11_SRV_DIMENSION_TEXTURE2D {
        return E_FAIL;
    }
    // SAFETY: `view_dimension == D3D11_SRV_DIMENSION_TEXTURE2D` selects `u.texture2d`.
    let mut t2d = unsafe { final_desc.u.texture2d };
    if t2d.mip_levels == u32::MAX {
        t2d.mip_levels = tex_desc.mip_levels.wrapping_sub(t2d.most_detailed_mip);
        final_desc.u.texture2d = t2d;
    }
    if t2d.most_detailed_mip != 0 || t2d.mip_levels != 1 {
        return E_FAIL;
    }
    if out.is_null() {
        return S_FALSE;
    }
    let (hr, fmt_desc) = query_dxgi(final_desc.format);
    if hr < 0 {
        return E_FAIL;
    }
    let Some(texture) = res.common.texture.as_ref() else {
        return E_FAIL;
    };
    let descriptor = TextureViewDescriptor {
        format: fmt_desc.pixel_format,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: 0,
        mip_count: 1,
        first_slice: 0,
        array_size: 1,
    };
    let view_key = descriptor.key();
    let subset = ResourceSubset::from_texture_view(
        &descriptor,
        texture.info().mipmap_level_count(),
        texture.array_length(),
        0,
    );
    let srv = Srv {
        resource,
        buffer: None,
        texture: Some(Arc::clone(texture)),
        buffer_view: None,
        texture_view: Some(view_key),
        slice: BufferSlice {
            byte_offset: 0,
            byte_length: 0,
            first_element: 0,
            element_count: 0,
        },
        subset,
        format: fmt_desc.pixel_format,
    };
    let raw = SrvObject::into_raw(srv, device, res.common.bind_flags, final_desc);
    // SAFETY: `out` was checked non-null above.
    unsafe { *out = raw };
    S_OK
}

/// Creates a 2D SRV on a dynamic linear texture (`d3d11_texture_linear.cpp`).
pub fn create_linear_texture_srv_2d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_linear_texture_srv_2d_inner(device, resource, desc, out)
}
