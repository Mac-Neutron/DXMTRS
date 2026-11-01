//! Buffer resource (`d3d11_buffer.cpp`).

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::{
    counter::Counter,
    dynamic::DynamicBuffer,
    encoder::BufferSlice,
    format::query_dxgi,
    resource::{Buffer, BufferAllocationFlags, BufferViewDescriptor},
};
use winemetal_sys::wire::WMT_PIXEL_FORMAT_R32_UINT;

use super::{
    Common, Resource, ResourceDesc, ResourceKind,
    helper::{
        calculate_buffer_view_offset_and_size, extract_entire_srv_buffer, extract_entire_uav_buffer,
    },
};
use crate::{
    abi::records::{
        D3d11BufferDesc, D3d11ShaderResourceViewDesc1, D3d11SubresourceData,
        D3d11UnorderedAccessViewDesc1,
    },
    com::{E_FAIL, E_INVALIDARG, Hresult, S_FALSE, S_OK},
    device::Device,
    subresource::ResourceSubset,
    view::{Srv, SrvObject, Uav, UavObject},
};

const D3D11_USAGE_IMMUTABLE: u32 = 1;
const D3D11_USAGE_DYNAMIC: u32 = 2;

const D3D11_BIND_STREAM_OUTPUT: u32 = 0x10;
const D3D11_BIND_RENDER_TARGET: u32 = 0x20;
const D3D11_BIND_DEPTH_STENCIL: u32 = 0x40;
const D3D11_BIND_UNORDERED_ACCESS: u32 = 0x80;

const D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS: u32 = 0x20;
const D3D11_RESOURCE_MISC_BUFFER_STRUCTURED: u32 = 0x40;

const D3D11_SRV_DIMENSION_BUFFER: u32 = 1;
const D3D11_SRV_DIMENSION_BUFFEREX: u32 = 11;
const D3D11_UAV_DIMENSION_BUFFER: u32 = 1;

const D3D11_BUFFEREX_SRV_FLAG_RAW: u32 = 0x1;
const D3D11_BUFFER_UAV_FLAG_RAW: u32 = 0x1;
const D3D11_BUFFER_UAV_FLAG_APPEND: u32 = 0x2;
const D3D11_BUFFER_UAV_FLAG_COUNTER: u32 = 0x4;

const DXGI_FORMAT_UNKNOWN: u32 = 0;
const DXGI_FORMAT_R32_TYPELESS: u32 = 39;

const OUTPUT_BIND_FLAGS: u32 = D3D11_BIND_STREAM_OUTPUT
    | D3D11_BIND_UNORDERED_ACCESS
    | D3D11_BIND_RENDER_TARGET
    | D3D11_BIND_DEPTH_STENCIL;

/// Buffer resource wrapper (`MTLD3D11Buffer`).
pub struct BufferResource {
    pub common: Common,
}

fn create_default_buffer_inner(
    device: *mut c_void,
    desc: &D3d11BufferDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let buffer = Arc::new(Buffer::new(u64::from(desc.byte_width), mtl));
    let mut flags = BufferAllocationFlags::empty();
    if desc.usage == D3D11_USAGE_DYNAMIC {
        flags.insert(BufferAllocationFlags::CPU_WRITE_COMBINED);
    }
    if desc.usage == D3D11_USAGE_IMMUTABLE {
        flags.insert(BufferAllocationFlags::GPU_READONLY);
    }
    if desc.usage == D3D11_USAGE_DYNAMIC {
        flags.insert(BufferAllocationFlags::SUBALLOCATE_FROM_ONE_PAGE);
        if cfg!(target_arch = "x86") {
            flags.insert(BufferAllocationFlags::CPU_PLACED);
        }
    } else if !init.is_null() || (desc.bind_flags & OUTPUT_BIND_FLAGS) == 0 {
        flags.insert(BufferAllocationFlags::GPU_MANAGED);
    } else {
        flags.insert(BufferAllocationFlags::GPU_PRIVATE);
    }
    let allocation = buffer.allocate(flags);
    if !init.is_null() {
        // SAFETY: `init` was checked non-null; the caller supplies a valid `D3d11SubresourceData`.
        let init_ref = unsafe { &*init };
        if !init_ref.p_sys_mem.is_null() && desc.byte_width > 0 {
            let len = usize::try_from(desc.byte_width).unwrap_or(0);
            let ptr = init_ref.p_sys_mem.cast::<u8>();
            // SAFETY: `p_sys_mem` points to at least `byte_width` bytes of initial buffer contents.
            let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
            allocation.update_contents(0, bytes, 0);
        }
    } else if flags.contains(BufferAllocationFlags::GPU_PRIVATE) {
        let _ = dev
            .queue()
            .initializer()
            .init_buffer_zero(&allocation, 0, buffer.length());
    }
    let prev = buffer.rename(allocation);
    debug_assert!(prev.is_none());
    let dynamic_buffer = if (desc.bind_flags & OUTPUT_BIND_FLAGS) == 0 {
        DynamicBuffer::new(Arc::clone(&buffer), flags).map(Arc::new)
    } else {
        None
    };
    let common = Common {
        device,
        buffer: Some(buffer),
        texture: None,
        dynamic_buffer,
        dynamic_texture: None,
        staging: None,
        bind_flags: desc.bind_flags,
        usage: desc.usage,
        misc_flags: desc.misc_flags,
        keyed_mutex: None,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::DefaultBuffer,
        ResourceDesc::Buffer(*desc),
    );
    // SAFETY: `out` was checked non-null by `create_buffer`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a default, immutable or dynamic `D3D11Buffer` (`CreateBuffer` in `d3d11_buffer.cpp`).
pub fn create_default_buffer(
    device: *mut c_void,
    desc: &D3d11BufferDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_default_buffer_inner(device, desc, init, out)
}

fn create_buffer_srv_inner(
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
    let ResourceDesc::Buffer(buf_desc) = res.desc else {
        return E_INVALIDARG;
    };
    if res.kind != ResourceKind::DefaultBuffer {
        return E_FAIL;
    }
    let structured = (buf_desc.misc_flags & D3D11_RESOURCE_MISC_BUFFER_STRUCTURED) != 0;
    let allow_raw_view = (buf_desc.misc_flags & D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS) != 0;
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_srv_buffer(&buf_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    if final_desc.view_dimension != D3D11_SRV_DIMENSION_BUFFER
        && final_desc.view_dimension != D3D11_SRV_DIMENSION_BUFFEREX
    {
        return E_FAIL;
    }
    let (first_element, num_elements, flags) =
        if final_desc.view_dimension == D3D11_SRV_DIMENSION_BUFFEREX {
            // SAFETY: `view_dimension == D3D11_SRV_DIMENSION_BUFFEREX` selects `u.buffer_ex`.
            let b = unsafe { final_desc.u.buffer_ex };
            (b.first_element, b.num_elements, b.flags)
        } else {
            // SAFETY: `view_dimension == D3D11_SRV_DIMENSION_BUFFER` selects `u.buffer`.
            let b = unsafe { final_desc.u.buffer };
            (b.first_element, b.num_elements, 0)
        };
    let (view_format, texel_size, offset, size, view_element_offset, view_element_width) =
        if structured {
            if final_desc.format != DXGI_FORMAT_UNKNOWN {
                return E_INVALIDARG;
            }
            let (offset, size) = calculate_buffer_view_offset_and_size(
                &buf_desc,
                buf_desc.structure_byte_stride,
                first_element,
                num_elements,
            );
            let words = buf_desc.structure_byte_stride >> 2;
            (
                WMT_PIXEL_FORMAT_R32_UINT,
                4,
                offset,
                size,
                first_element.wrapping_mul(words),
                num_elements.wrapping_mul(words),
            )
        } else if final_desc.view_dimension == D3D11_SRV_DIMENSION_BUFFEREX
            && (flags & D3D11_BUFFEREX_SRV_FLAG_RAW) != 0
        {
            if !allow_raw_view || final_desc.format != DXGI_FORMAT_R32_TYPELESS {
                return E_INVALIDARG;
            }
            let (offset, size) =
                calculate_buffer_view_offset_and_size(&buf_desc, 4, first_element, num_elements);
            (
                WMT_PIXEL_FORMAT_R32_UINT,
                4,
                offset,
                size,
                first_element,
                num_elements,
            )
        } else {
            let (hr, fmt_desc) = query_dxgi(final_desc.format);
            if hr < 0 || fmt_desc.bytes_per_texel == 0 {
                return E_FAIL;
            }
            let bpt = fmt_desc.bytes_per_texel;
            (
                fmt_desc.pixel_format,
                bpt,
                first_element.wrapping_mul(bpt),
                num_elements.wrapping_mul(bpt),
                first_element,
                num_elements,
            )
        };
    if out.is_null() {
        return S_FALSE;
    }
    let Some(buffer) = res.common.buffer.as_ref() else {
        return E_FAIL;
    };
    let view_key = BufferViewDescriptor {
        format: view_format,
        texel_size,
    }
    .key();
    let srv = Srv {
        resource,
        buffer: Some(Arc::clone(buffer)),
        texture: None,
        buffer_view: Some(view_key),
        texture_view: None,
        slice: BufferSlice {
            byte_offset: offset,
            byte_length: size,
            first_element: view_element_offset,
            element_count: view_element_width,
        },
        subset: ResourceSubset::buffer(offset, size),
        format: view_format,
    };
    let raw = SrvObject::into_raw(srv, device, res.common.bind_flags, final_desc);
    // SAFETY: `out` was checked non-null above.
    unsafe { *out = raw };
    S_OK
}

/// Creates a shader resource view on a `D3D11Buffer` (`d3d11_buffer.cpp`).
pub fn create_buffer_srv(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_buffer_srv_inner(device, resource, desc, out)
}

fn create_buffer_uav_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    if resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `resource` is a non-null primary `Resource` pointer.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Buffer(buf_desc) = res.desc else {
        return E_INVALIDARG;
    };
    if res.kind != ResourceKind::DefaultBuffer {
        return E_FAIL;
    }
    let structured = (buf_desc.misc_flags & D3D11_RESOURCE_MISC_BUFFER_STRUCTURED) != 0;
    let allow_raw_view = (buf_desc.misc_flags & D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS) != 0;
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_uav_buffer(&buf_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    if final_desc.view_dimension != D3D11_UAV_DIMENSION_BUFFER {
        return E_FAIL;
    }
    // SAFETY: `view_dimension == D3D11_UAV_DIMENSION_BUFFER` selects `u.buffer`.
    let b = unsafe { final_desc.u.buffer };
    let first_element = b.first_element;
    let num_elements = b.num_elements;
    let flags = b.flags;
    let mut counter = None;
    let (view_format, texel_size, offset, size, view_element_offset, view_element_width) =
        if structured {
            if final_desc.format != DXGI_FORMAT_UNKNOWN {
                return E_INVALIDARG;
            }
            let (offset, size) = calculate_buffer_view_offset_and_size(
                &buf_desc,
                buf_desc.structure_byte_stride,
                first_element,
                num_elements,
            );
            if (flags & (D3D11_BUFFER_UAV_FLAG_APPEND | D3D11_BUFFER_UAV_FLAG_COUNTER)) != 0 {
                counter = Some(Arc::new(Counter::new()));
            }
            let words = buf_desc.structure_byte_stride >> 2;
            (
                WMT_PIXEL_FORMAT_R32_UINT,
                4,
                offset,
                size,
                first_element.wrapping_mul(words),
                num_elements.wrapping_mul(words),
            )
        } else if (flags & D3D11_BUFFER_UAV_FLAG_RAW) != 0 {
            if !allow_raw_view || final_desc.format != DXGI_FORMAT_R32_TYPELESS {
                return E_INVALIDARG;
            }
            let (offset, size) =
                calculate_buffer_view_offset_and_size(&buf_desc, 4, first_element, num_elements);
            (
                WMT_PIXEL_FORMAT_R32_UINT,
                4,
                offset,
                size,
                first_element,
                num_elements,
            )
        } else {
            let (hr, fmt_desc) = query_dxgi(final_desc.format);
            if hr < 0 || fmt_desc.bytes_per_texel == 0 {
                return E_FAIL;
            }
            let bpt = fmt_desc.bytes_per_texel;
            (
                fmt_desc.pixel_format,
                bpt,
                first_element.wrapping_mul(bpt),
                num_elements.wrapping_mul(bpt),
                first_element,
                num_elements,
            )
        };
    if out.is_null() {
        return S_FALSE;
    }
    let Some(buffer) = res.common.buffer.as_ref() else {
        return E_FAIL;
    };
    let view_key = BufferViewDescriptor {
        format: view_format,
        texel_size,
    }
    .key();
    let uav = Uav {
        resource,
        buffer: Some(Arc::clone(buffer)),
        texture: None,
        buffer_view: Some(view_key),
        texture_view: None,
        slice: BufferSlice {
            byte_offset: offset,
            byte_length: size,
            first_element: view_element_offset,
            element_count: view_element_width,
        },
        subset: ResourceSubset::buffer(offset, size),
        format: view_format,
        counter,
    };
    let raw = UavObject::into_raw(uav, device, res.common.bind_flags, final_desc);
    // SAFETY: `out` was checked non-null above.
    unsafe { *out = raw };
    S_OK
}

/// Creates an unordered access view on a `D3D11Buffer` (`d3d11_buffer.cpp`).
pub fn create_buffer_uav(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_buffer_uav_inner(device, resource, desc, out)
}
