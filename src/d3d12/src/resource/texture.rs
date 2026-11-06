//! `ID3D12Resource` texture implementation (`d3d12_texture.cpp`).

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::{
    format::MTL_DXGI_FORMAT_BC,
    resource::{Texture, TextureAllocationFlags, TextureViewDescriptor},
};
use dxmt_util::{
    com::ComObject,
    float_cast::{f32_to_i32, i32_to_f32},
    private_data::PrivateData,
};
use winemetal_sys::{
    MTLDevice_heapTextureSizeAndAlign, MTLTexture_getBytes, MTLTexture_replaceRegion,
    wire::{
        MemPtr, WMT_PIXEL_FORMAT_DEPTH16_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        WMT_PIXEL_FORMAT_R32_FLOAT, WMT_PIXEL_FORMAT_R32_UINT,
        WMT_RESOURCE_STORAGE_MODE_MEMORYLESS, WMT_TEXTURE_FLAG_LOSSLESS, WMT_TEXTURE_TYPE_CUBE,
        WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY,
        WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D,
        WMTOrigin, WMTSize, WMTTextureInfo, encode_format_swizzle, get_swizzle_alpha,
        get_swizzle_blue, get_swizzle_green, get_swizzle_red,
    },
};

use crate::{
    abi::{
        resource::{
            D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT,
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT, D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            D3D12_RESOURCE_DIMENSION_TEXTURE3D, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
            D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET, D3d12Box, HeapProperties, ResourceDesc,
        },
        views::{
            D3D12_DSV_DIMENSION_TEXTURE1D, D3D12_DSV_DIMENSION_TEXTURE1DARRAY,
            D3D12_DSV_DIMENSION_TEXTURE2D, D3D12_DSV_DIMENSION_TEXTURE2DARRAY,
            D3D12_DSV_DIMENSION_TEXTURE2DMS, D3D12_DSV_DIMENSION_TEXTURE2DMSARRAY,
            D3D12_HEAP_FLAG_DENY_NON_RT_DS_TEXTURES, D3D12_HEAP_FLAG_DENY_RT_DS_TEXTURES,
            D3D12_RTV_DIMENSION_TEXTURE1D, D3D12_RTV_DIMENSION_TEXTURE1DARRAY,
            D3D12_RTV_DIMENSION_TEXTURE2D, D3D12_RTV_DIMENSION_TEXTURE2DARRAY,
            D3D12_RTV_DIMENSION_TEXTURE2DMS, D3D12_RTV_DIMENSION_TEXTURE2DMSARRAY,
            D3D12_RTV_DIMENSION_TEXTURE3D, D3D12_SRV_DIMENSION_TEXTURE1D,
            D3D12_SRV_DIMENSION_TEXTURE1DARRAY, D3D12_SRV_DIMENSION_TEXTURE2D,
            D3D12_SRV_DIMENSION_TEXTURE2DARRAY, D3D12_SRV_DIMENSION_TEXTURE2DMS,
            D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY, D3D12_SRV_DIMENSION_TEXTURE3D,
            D3D12_SRV_DIMENSION_TEXTURECUBE, D3D12_SRV_DIMENSION_TEXTURECUBEARRAY,
            D3D12_UAV_DIMENSION_TEXTURE1D, D3D12_UAV_DIMENSION_TEXTURE1DARRAY,
            D3D12_UAV_DIMENSION_TEXTURE2D, D3D12_UAV_DIMENSION_TEXTURE2DARRAY,
            D3D12_UAV_DIMENSION_TEXTURE3D, DepthStencilViewDesc, RenderTargetViewDesc,
            ShaderResourceViewDesc, UnorderedAccessViewDesc,
        },
        vtbl::{D3d12CpuDescriptorHandle, ID3D12Resource2Vtbl},
    },
    com::{self, E_INVALIDARG, E_NOTIMPL, Hresult, S_FALSE, S_OK},
    descriptor_heap::RenderTargetDesc,
    device::Device,
    heap::HeapObject,
    resource::Resource,
    resource_math::{
        decompose_subresource, is_box_in_bounds, is_cpu_visible_heap, is_private_heap,
        resource_extent,
    },
    slot,
};

/// `ID3D12Resource2` vtable for texture resources (`d3d12_texture.cpp`).
pub static TEXTURE_VTBL: ID3D12Resource2Vtbl = ID3D12Resource2Vtbl {
    query_interface: slot!(
        "texture.QueryInterface",
        (this: *mut c_void, riid: *const c_void, out: *mut *mut c_void) -> Hresult => {
            // SAFETY: `this`, `riid`, and `out` are forwarded from COM slot.
            unsafe { super::query_interface(this, riid, out, "D3D12Texture") }
        }
    ),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a valid `Resource` COM pointer.
        unsafe { com::add_ref_com(this.cast::<Resource>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a valid `Resource` COM pointer.
        unsafe { com::release_com(this.cast::<Resource>()) }
    }),
    get_private_data: slot!(
        (this: *mut c_void, guid: *const c_void, data_size: *mut u32, data: *mut c_void) -> Hresult => {
            // SAFETY: forwarded from COM slot.
            unsafe { super::get_private_data(this, guid, data_size, data) }
        }
    ),
    set_private_data: slot!(
        (this: *mut c_void, guid: *const c_void, data_size: u32, data: *const c_void) -> Hresult => {
            // SAFETY: forwarded from COM slot.
            unsafe { super::set_private_data(this, guid, data_size, data) }
        }
    ),
    set_private_data_interface: slot!(
        (this: *mut c_void, guid: *const c_void, data: *const c_void) -> Hresult => {
            // SAFETY: forwarded from COM slot.
            unsafe { super::set_private_data_interface(this, guid, data) }
        }
    ),
    set_name: slot!((this: *mut c_void, name: *const c_void) -> Hresult => {
        // SAFETY: forwarded from COM slot.
        unsafe { super::set_name(this, name) }
    }),
    get_device: slot!(
        (this: *mut c_void, riid: *const c_void, out: *mut *mut c_void) -> Hresult => {
            // SAFETY: forwarded from COM slot.
            unsafe { super::get_device(this, riid, out) }
        }
    ),
    map: slot!(
        "texture.Map",
        (this: *mut c_void, subresource: u32, _read_range: *const c_void, data: *mut *mut c_void) -> Hresult => {
            // SAFETY: `this` is a valid `Resource` COM pointer.
            let res = unsafe { &*this.cast::<Resource>() };
            if !is_cpu_visible_heap(&res.heap_props) {
                return E_INVALIDARG;
            }
            let mut subresource_count = u32::from(res.desc.mip_levels);
            if res.desc.dimension != D3D12_RESOURCE_DIMENSION_TEXTURE3D {
                subresource_count *= u32::from(res.desc.depth_or_array_size);
            }
            if subresource >= subresource_count {
                return E_INVALIDARG;
            }
            if !data.is_null()
                || (res.desc.dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D
                    && res.desc.mip_levels > 1)
            {
                return E_INVALIDARG;
            }
            S_OK
        }
    ),
    unmap: slot!(
        "texture.Unmap",
        (_this: *mut c_void, _subresource: u32, _written_range: *const c_void) => {}
    ),
    get_desc: slot!(
        (this: *mut c_void, out: *mut c_void) -> *mut c_void => {
            // SAFETY: `this` is a valid `Resource` COM pointer.
            let res = unsafe { &*this.cast::<Resource>() };
            // SAFETY: `out` points to a caller-provided `D3D12_RESOURCE_DESC`.
            unsafe { *out.cast::<ResourceDesc>() = super::clone_desc(res.texture_desc()) };
            out
        }
    ),
    get_gpu_virtual_address: slot!(
        "texture.GetGPUVirtualAddress",
        (_this: *mut c_void) -> u64 => 0
    ),
    write_to_subresource: slot!(
        "texture.WriteToSubresource",
        (
            this: *mut c_void,
            dst_subresource: u32,
            dst_box: *const c_void,
            src_data: *const c_void,
            src_row_pitch: u32,
            src_depth_pitch: u32
        ) -> Hresult => {
            // SAFETY: `this` is a valid `Resource` COM pointer.
            let res = unsafe { &*this.cast::<Resource>() };
            // SAFETY: `dst_box` and `src_data` are caller-supplied pointers.
            unsafe {
                write_to_subresource(
                    res,
                    dst_subresource,
                    dst_box,
                    src_data,
                    src_row_pitch,
                    src_depth_pitch,
                )
            }
        }
    ),
    read_from_subresource: slot!(
        "texture.ReadFromSubresource",
        (
            this: *mut c_void,
            dst_data: *mut c_void,
            dst_row_pitch: u32,
            dst_depth_pitch: u32,
            src_subresource: u32,
            src_box: *const c_void
        ) -> Hresult => {
            // SAFETY: `this` is a valid `Resource` COM pointer.
            let res = unsafe { &*this.cast::<Resource>() };
            // SAFETY: `dst_data` and `src_box` are caller-supplied pointers.
            unsafe {
                read_from_subresource(
                    res,
                    dst_data,
                    dst_row_pitch,
                    dst_depth_pitch,
                    src_subresource,
                    src_box,
                )
            }
        }
    ),
    get_heap_properties: slot!(
        "texture.GetHeapProperties",
        (this: *mut c_void, heap_props: *mut c_void, heap_flags: *mut u32) -> Hresult => {
            // SAFETY: `this`, `heap_props`, and `heap_flags` are forwarded from COM slot.
            unsafe { super::get_heap_properties(this, heap_props, heap_flags) }
        }
    ),
    get_protected_resource_session: slot!(
        (_this: *mut c_void, _riid: *const c_void, session: *mut *mut c_void) -> Hresult => {
            if !session.is_null() {
                // SAFETY: `session` was checked non-null above.
                unsafe { com::init_out_ptr(session) };
            }
            E_NOTIMPL
        }
    ),
    get_desc1: slot!(
        (this: *mut c_void, out: *mut c_void) -> *mut c_void => {
            // SAFETY: `this` and `out` are forwarded from COM slot.
            unsafe { super::get_desc1(this, out) }
        }
    ),
};

unsafe fn validate_subresource_box(
    res: &Resource,
    subresource: u32,
    raw_box: *const c_void,
) -> Result<Option<(u32, u32, D3d12Box)>, Hresult> {
    if !is_cpu_visible_heap(&res.heap_props) {
        return Err(E_INVALIDARG);
    }
    if (res.desc.flags & D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL) != 0 {
        return Err(E_INVALIDARG);
    }
    let decomposed = decompose_subresource(&res.desc, subresource);
    if decomposed.plane_slice != 0 {
        return Err(E_INVALIDARG);
    }
    let level = decomposed.mip_slice;
    let slice = decomposed.array_slice;
    let full_box = resource_extent(&res.desc, level);
    let b = if raw_box.is_null() {
        super::clone_box(&full_box)
    } else {
        // SAFETY: `raw_box` is non-null and points to a caller-supplied `D3D12_BOX`.
        unsafe { ptr::read(raw_box.cast::<D3d12Box>()) }
    };
    if !is_box_in_bounds(&b, &full_box) {
        return Err(E_INVALIDARG);
    }
    if b.left == b.right || b.top == b.bottom || b.front == b.back {
        return Ok(None);
    }
    let (hr, fmt) = dxmt_core::format::query_dxgi(res.desc.format);
    if hr != S_OK {
        return Err(E_INVALIDARG);
    }
    if (fmt.flag & MTL_DXGI_FORMAT_BC) != 0
        && ((b.left | b.right | b.top | b.bottom) & (fmt.bytes_per_texel - 1)) != 0
    {
        return Err(E_INVALIDARG);
    }
    Ok(Some((level, slice, b)))
}

unsafe fn write_to_subresource(
    res: &Resource,
    dst_subresource: u32,
    dst_box: *const c_void,
    src_data: *const c_void,
    src_row_pitch: u32,
    src_depth_pitch: u32,
) -> Hresult {
    // SAFETY: `dst_box` is forwarded from the COM caller.
    let Some((level, slice, b)) =
        (match unsafe { validate_subresource_box(res, dst_subresource, dst_box) } {
            Ok(v) => v,
            Err(hr) => return hr,
        })
    else {
        return S_OK;
    };
    let slice_pitch = if res.desc.dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D {
        src_depth_pitch
    } else {
        0
    };
    if let Some(tex) = &res.texture
        && let Some(alloc) = tex.current()
    {
        let handle = alloc.texture();
        if handle != 0 {
            // SAFETY: `handle` is a live `MTLTexture` and `src_data` is valid for the box.
            unsafe {
                MTLTexture_replaceRegion(
                    handle,
                    WMTOrigin {
                        x: u64::from(b.left),
                        y: u64::from(b.top),
                        z: u64::from(b.front),
                    },
                    WMTSize {
                        width: u64::from(b.right - b.left),
                        height: u64::from(b.bottom - b.top),
                        depth: u64::from(b.back - b.front),
                    },
                    u64::from(level),
                    u64::from(slice),
                    MemPtr::from_ptr(src_data),
                    u64::from(src_row_pitch),
                    u64::from(slice_pitch),
                );
            }
        }
    }
    S_OK
}

unsafe fn read_from_subresource(
    res: &Resource,
    dst_data: *mut c_void,
    dst_row_pitch: u32,
    dst_depth_pitch: u32,
    src_subresource: u32,
    src_box: *const c_void,
) -> Hresult {
    // SAFETY: `src_box` is forwarded from the COM caller.
    let Some((level, slice, b)) =
        (match unsafe { validate_subresource_box(res, src_subresource, src_box) } {
            Ok(v) => v,
            Err(hr) => return hr,
        })
    else {
        return S_OK;
    };
    let slice_pitch = if res.desc.dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D {
        dst_depth_pitch
    } else {
        0
    };
    if let Some(tex) = &res.texture
        && let Some(alloc) = tex.current()
    {
        let handle = alloc.texture();
        if handle != 0 {
            // SAFETY: `handle` is a live `MTLTexture` and `dst_data` is writable for the box.
            unsafe {
                MTLTexture_getBytes(
                    handle,
                    WMTOrigin {
                        x: u64::from(b.left),
                        y: u64::from(b.top),
                        z: u64::from(b.front),
                    },
                    WMTSize {
                        width: u64::from(b.right - b.left),
                        height: u64::from(b.bottom - b.top),
                        depth: u64::from(b.back - b.front),
                    },
                    u64::from(level),
                    u64::from(slice),
                    MemPtr::from_ptr(dst_data),
                    u64::from(dst_row_pitch),
                    u64::from(slice_pitch),
                );
            }
        }
    }
    S_OK
}

/// Encodes a D3D12 `Shader4ComponentMapping` into a `WMTPixelFormat`.
///
/// Mirrors `EncodeComponentMapping` (`d3d12_texture.cpp:184-198`).
#[must_use]
pub const fn encode_component_mapping(mapping: u32, format: u32) -> u32 {
    let map = [
        0u8,
        1u8,
        get_swizzle_red(format).to_le_bytes()[0],
        get_swizzle_green(format).to_le_bytes()[0],
        get_swizzle_blue(format).to_le_bytes()[0],
        get_swizzle_alpha(format).to_le_bytes()[0],
    ];
    let r = (((mapping & 0x7) + 2) % 6) as usize;
    let g = ((((mapping >> 3) & 0x7) + 2) % 6) as usize;
    let b = ((((mapping >> 6) & 0x7) + 2) % 6) as usize;
    let a = ((((mapping >> 9) & 0x7) + 2) % 6) as usize;
    encode_format_swizzle(format, map[r], map[g], map[b], map[a])
}

/// Creates a committed or placed texture `Resource`.
///
/// Mirrors `CreateCommittedTexture` / `CreatePlacedTexture` (`d3d12_texture.cpp:910-947`).
///
/// # Safety
///
/// `device`, `optimized_clear`, and `out_com` are caller-supplied COM pointers. If `placed` is
/// `Some`, its `HeapObject` pointer must be valid.
pub unsafe fn create(
    device: *mut Device,
    heap: (&HeapProperties, u32),
    desc: &ResourceDesc,
    optimized_clear: *const c_void,
    placed: Option<(*mut HeapObject, u64)>,
    castable_formats: &[u32],
    out_com: (*const c_void, *mut *mut c_void),
) -> Hresult {
    let (heap_props, heap_flags) = heap;
    let (riid, out) = out_com;
    let _ = (optimized_clear, castable_formats);

    if placed.is_some() {
        let is_rt_ds = (desc.flags
            & (D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET | D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL))
            != 0;
        if is_rt_ds {
            if (heap_flags & D3D12_HEAP_FLAG_DENY_RT_DS_TEXTURES) != 0 {
                return E_INVALIDARG;
            }
        } else if (heap_flags & D3D12_HEAP_FLAG_DENY_NON_RT_DS_TEXTURES) != 0 {
            return E_INVALIDARG;
        }
    }

    let mut texture_info = WMTTextureInfo {
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
    };
    // SAFETY: `device` is null or a valid `Device` pointer.
    let hr = unsafe { super::populate_texture_info_opt(device, desc, &mut texture_info) };
    if hr != S_OK {
        return hr;
    }

    // Opt-in: a depth-only, single-sample target whose contents are not shader-visible and
    // cannot be copied can live entirely in tile memory. The fallback below keeps a rejected
    // allocation working on a device without memoryless depth.
    let memoryless = placed.is_none()
        && super::memoryless_enabled()
        && desc.dimension == D3D12_RESOURCE_DIMENSION_TEXTURE2D
        && (desc.flags
            & (D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL
                | super::D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE))
            == (D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL
                | super::D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE)
        && desc.sample_desc.count <= 1;
    if memoryless {
        texture_info.options = WMT_RESOURCE_STORAGE_MODE_MEMORYLESS;
        texture_info.flags &= !WMT_TEXTURE_FLAG_LOSSLESS;
    }

    let mut norm_desc = super::clone_desc(desc);
    if norm_desc.mip_levels == 0 {
        norm_desc.mip_levels = u16::try_from(texture_info.mipmap_level_count()).unwrap_or(1);
    }
    if norm_desc.alignment == 0 {
        norm_desc.alignment = if norm_desc.sample_desc.count > 1 {
            D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT
        } else {
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT
        };
    }

    // SAFETY: if non-null, `device` is a valid `Device` pointer.
    let mtl = (!device.is_null())
        .then(|| unsafe { (*device).mtl.as_ref() })
        .flatten();

    if let Some((heap_ptr, offset)) = placed
        && !heap_ptr.is_null()
    {
        // SAFETY: `heap_ptr` is a valid `HeapObject` pointer.
        let hdesc = unsafe { super::heap_desc(heap_ptr) };
        if let Some(mtl_dev) = mtl {
            let mut ti = texture_info;
            // SAFETY: `mtl_dev` holds a live `MTLDevice`.
            let size_and_align =
                unsafe { MTLDevice_heapTextureSizeAndAlign(mtl_dev.handle(), &raw mut ti) };
            if u64::from(size_and_align.size).saturating_add(offset) > hdesc.size_in_bytes {
                return E_INVALIDARG;
            }
        }
        // SAFETY: `heap_ptr` is a valid `HeapObject` pointer.
        let cpu_mem = unsafe { (*heap_ptr).cpu_memory };
        if !cpu_mem.is_null() {
            crate::err!(
                "CreatePlacedResource: a texture in a CPU-visible heap is not supported with memory hooks"
            );
            return E_INVALIDARG;
        }
    }

    if (texture_info.flags & WMT_TEXTURE_FLAG_LOSSLESS) != 0 {
        crate::count!("#textures created compressed", 1);
    }

    if out.is_null() {
        return S_FALSE;
    }

    let mut placed_heap = ptr::null_mut();
    let texture = if let Some(mtl_dev) = mtl {
        let tex = Arc::new(Texture::new(texture_info, mtl_dev));
        let mut flags = TextureAllocationFlags::empty();
        if memoryless {
            flags |= TextureAllocationFlags::MEMORYLESS;
        } else if is_private_heap(heap_props, super::private_heap_enabled()) {
            flags |= TextureAllocationFlags::CPU_INVISIBLE;
        }
        if let Some((heap_ptr, offset)) = placed
            && !heap_ptr.is_null()
        {
            // SAFETY: `heap_ptr` is a valid `HeapObject` pointer.
            let mtl_heap = unsafe { (*heap_ptr).heap.as_ref() };
            if let Some(h) = mtl_heap {
                let _ = tex.rename(tex.allocate_on_heap(flags, h, offset));
            } else {
                let _ = tex.rename(tex.allocate(flags));
            }
            // SAFETY: `heap_ptr` is a valid `ComRefCounted` `HeapObject`.
            unsafe { com::add_ref_private_com(heap_ptr) };
            placed_heap = heap_ptr;
        } else {
            let mut allocation = tex.allocate(flags);
            if memoryless && allocation.texture() == 0 {
                // Metal on this GPU declined the storage mode; use the ordinary private
                // allocation rather than losing the target.
                flags.remove(TextureAllocationFlags::MEMORYLESS);
                allocation = tex.allocate(flags);
            }
            let _ = tex.rename(allocation);
            // SAFETY: `device` is non-null when `mtl` is `Some`.
            let dev_ref = unsafe { &*device };
            crate::device::residency::register_texture(dev_ref, &tex);
        }
        Some(tex)
    } else {
        if let Some((heap_ptr, _)) = placed
            && !heap_ptr.is_null()
        {
            // SAFETY: `heap_ptr` is a valid `ComRefCounted` `HeapObject`.
            unsafe { com::add_ref_private_com(heap_ptr) };
            placed_heap = heap_ptr;
        }
        None
    };

    if !device.is_null() {
        // SAFETY: `device` is a valid `ComRefCounted` `Device` pointer.
        unsafe { com::add_ref_com(device) };
    }

    let resource = Box::new(Resource {
        vtbl: &TEXTURE_VTBL,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device,
        texture,
        buffer: None,
        placed_heap,
        desc: norm_desc,
        heap_props: super::clone_heap_props(heap_props),
    });
    // SAFETY: `riid` and `out` are caller-supplied COM pointers.
    unsafe { super::return_resource(resource, riid, out) }
}

/// `MTLD3D12Texture::CreateShaderResourceView` (`d3d12_texture.cpp:419-578`).
///
/// # Safety
///
/// `desc` is null or points to a readable `D3D12_SHADER_RESOURCE_VIEW_DESC`.
pub unsafe fn create_srv(
    resource: &Resource,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::scope!("texture.CreateShaderResourceView");
    let view_desc = if desc.is_null() {
        match super::extract_entire_srv(&resource.desc) {
            Ok(v) => v,
            Err(hr) => return hr,
        }
    } else {
        // SAFETY: `desc` is non-null and points to a `ShaderResourceViewDesc`.
        let mut v = unsafe { ptr::read(desc.cast::<ShaderResourceViewDesc>()) };
        if v.format == 0 {
            v.format = resource.desc.format;
        }
        v
    };

    let (hr, metal_format) = dxmt_core::format::query_dxgi(view_desc.format);
    if hr != S_OK {
        return hr;
    }

    let mut format = encode_component_mapping(
        view_desc.shader_4_component_mapping,
        metal_format.pixel_format,
    );
    let tex_pf = resource.texture.as_ref().map_or_else(
        || super::resource_pixel_format(&resource.desc),
        |t| t.info().pixel_format,
    );
    if tex_pf == WMT_PIXEL_FORMAT_DEPTH32_FLOAT || tex_pf == WMT_PIXEL_FORMAT_DEPTH16_UNORM {
        format = encode_component_mapping(view_desc.shader_4_component_mapping, tex_pf);
    }

    let mip_levels = u32::from(resource.desc.mip_levels);
    let depth_or_array = u32::from(resource.desc.depth_or_array_size);
    let mut min_lod = 0.0f32;
    let (texture_type, first_mip, mip_count, first_slice, array_size) =
        match view_desc.view_dimension {
            D3D12_SRV_DIMENSION_TEXTURE1D => {
                // SAFETY: `view_dimension == D3D12_SRV_DIMENSION_TEXTURE1D` selects `texture_1d`.
                let v = unsafe { &*view_desc.view.texture_1d };
                min_lod = v.resource_min_lod_clamp;
                let count = if v.mip_levels == u32::MAX {
                    mip_levels.saturating_sub(v.most_detailed_mip)
                } else {
                    v.mip_levels
                };
                (WMT_TEXTURE_TYPE2D, v.most_detailed_mip, count, 0, 1)
            }
            D3D12_SRV_DIMENSION_TEXTURE1DARRAY => {
                // SAFETY: `view_dimension == D3D12_SRV_DIMENSION_TEXTURE1DARRAY` selects `texture_1d_array`.
                let v = unsafe { &*view_desc.view.texture_1d_array };
                min_lod = v.resource_min_lod_clamp;
                let count = if v.mip_levels == u32::MAX {
                    mip_levels.saturating_sub(v.most_detailed_mip)
                } else {
                    v.mip_levels
                };
                let slices = if v.array_size == u32::MAX {
                    depth_or_array.saturating_sub(v.first_array_slice)
                } else {
                    v.array_size
                };
                (
                    WMT_TEXTURE_TYPE2D_ARRAY,
                    v.most_detailed_mip,
                    count,
                    v.first_array_slice,
                    slices,
                )
            }
            D3D12_SRV_DIMENSION_TEXTURE2D => {
                // SAFETY: `view_dimension == D3D12_SRV_DIMENSION_TEXTURE2D` selects `texture_2d`.
                let v = unsafe { &*view_desc.view.texture_2d };
                min_lod = v.resource_min_lod_clamp;
                let count = if v.mip_levels == u32::MAX {
                    mip_levels.saturating_sub(v.most_detailed_mip)
                } else {
                    v.mip_levels
                };
                (WMT_TEXTURE_TYPE2D, v.most_detailed_mip, count, 0, 1)
            }
            D3D12_SRV_DIMENSION_TEXTURE2DARRAY => {
                // SAFETY: `view_dimension == D3D12_SRV_DIMENSION_TEXTURE2DARRAY` selects `texture_2d_array`.
                let v = unsafe { &*view_desc.view.texture_2d_array };
                min_lod = v.resource_min_lod_clamp;
                let count = if v.mip_levels == u32::MAX {
                    mip_levels.saturating_sub(v.most_detailed_mip)
                } else {
                    v.mip_levels
                };
                let slices = if v.array_size == u32::MAX {
                    depth_or_array.saturating_sub(v.first_array_slice)
                } else {
                    v.array_size
                };
                (
                    WMT_TEXTURE_TYPE2D_ARRAY,
                    v.most_detailed_mip,
                    count,
                    v.first_array_slice,
                    slices,
                )
            }
            D3D12_SRV_DIMENSION_TEXTURECUBE => {
                // SAFETY: `view_dimension == D3D12_SRV_DIMENSION_TEXTURECUBE` selects `texture_cube`.
                let v = unsafe { &*view_desc.view.texture_cube };
                min_lod = v.resource_min_lod_clamp;
                let count = if v.mip_levels == u32::MAX {
                    mip_levels.saturating_sub(v.most_detailed_mip)
                } else {
                    v.mip_levels
                };
                (WMT_TEXTURE_TYPE_CUBE, v.most_detailed_mip, count, 0, 6)
            }
            D3D12_SRV_DIMENSION_TEXTURECUBEARRAY => {
                // SAFETY: `view_dimension == D3D12_SRV_DIMENSION_TEXTURECUBEARRAY` selects `texture_cube_array`.
                let v = unsafe { &*view_desc.view.texture_cube_array };
                min_lod = v.resource_min_lod_clamp;
                let count = if v.mip_levels == u32::MAX {
                    mip_levels.saturating_sub(v.most_detailed_mip)
                } else {
                    v.mip_levels
                };
                let slices = if v.num_cubes == u32::MAX {
                    depth_or_array.saturating_sub(v.first_2d_array_face)
                } else {
                    v.num_cubes * 6
                };
                (
                    WMT_TEXTURE_TYPE_CUBE_ARRAY,
                    v.most_detailed_mip,
                    count,
                    0,
                    slices,
                )
            }
            D3D12_SRV_DIMENSION_TEXTURE2DMS => (WMT_TEXTURE_TYPE2D_MULTISAMPLE, 0, 1, 0, 1),
            D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY => {
                // SAFETY: `view_dimension == D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY` selects `texture_2dms_array`.
                let v = unsafe { &*view_desc.view.texture_2dms_array };
                let slices = if v.array_size == u32::MAX {
                    depth_or_array.saturating_sub(v.first_array_slice)
                } else {
                    v.array_size
                };
                (
                    WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
                    0,
                    1,
                    v.first_array_slice,
                    slices,
                )
            }
            D3D12_SRV_DIMENSION_TEXTURE3D => {
                // SAFETY: `view_dimension == D3D12_SRV_DIMENSION_TEXTURE3D` selects `texture_3d`.
                let v = unsafe { &*view_desc.view.texture_3d };
                min_lod = v.resource_min_lod_clamp;
                let count = if v.mip_levels == u32::MAX {
                    mip_levels.saturating_sub(v.most_detailed_mip)
                } else {
                    v.mip_levels
                };
                (WMT_TEXTURE_TYPE3D, v.most_detailed_mip, count, 0, 1)
            }
            _ => return E_INVALIDARG,
        };

    let view = TextureViewDescriptor {
        format,
        texture_type,
        first_mip,
        mip_count,
        first_slice,
        array_size,
    }
    .key();
    let clamp = i32_to_f32(f32_to_i32(min_lod).wrapping_sub((first_mip & 0xf).cast_signed()));
    // SAFETY: `descriptor` is forwarded to `descriptor_heap`.
    unsafe {
        crate::descriptor_heap::write_srv_texture(
            descriptor,
            resource.texture.as_deref(),
            view,
            clamp,
        )
    }
}

/// `MTLD3D12Texture::CreateUnorderedAccessView` (`d3d12_texture.cpp:581-676`).
///
/// # Safety
///
/// `counter` is unused; `desc` is null or points to a readable `D3D12_UNORDERED_ACCESS_VIEW_DESC`.
pub unsafe fn create_uav(
    resource: &Resource,
    _counter: *mut c_void,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::scope!("texture.CreateUnorderedAccessView");
    let view_desc = if desc.is_null() {
        match super::extract_entire_uav(&resource.desc) {
            Ok(v) => v,
            Err(hr) => return hr,
        }
    } else {
        // SAFETY: `desc` is non-null and points to an `UnorderedAccessViewDesc`.
        let mut v = unsafe { ptr::read(desc.cast::<UnorderedAccessViewDesc>()) };
        if v.format == 0 {
            v.format = resource.desc.format;
        }
        v
    };

    let (hr, metal_format) = dxmt_core::format::query_dxgi(view_desc.format);
    if hr != S_OK {
        return hr;
    }

    let depth_or_array = u32::from(resource.desc.depth_or_array_size);
    let (texture_type, first_mip, first_slice, array_size) = match view_desc.view_dimension {
        D3D12_UAV_DIMENSION_TEXTURE1D => {
            // SAFETY: `view_dimension == D3D12_UAV_DIMENSION_TEXTURE1D` selects `texture_1d`.
            let v = unsafe { &*view_desc.view.texture_1d };
            (WMT_TEXTURE_TYPE2D, v.mip_slice, 0, 1)
        }
        D3D12_UAV_DIMENSION_TEXTURE2D => {
            // SAFETY: `view_dimension == D3D12_UAV_DIMENSION_TEXTURE2D` selects `texture_2d`.
            let v = unsafe { &*view_desc.view.texture_2d };
            (WMT_TEXTURE_TYPE2D, v.mip_slice, 0, 1)
        }
        D3D12_UAV_DIMENSION_TEXTURE1DARRAY => {
            // SAFETY: `view_dimension == D3D12_UAV_DIMENSION_TEXTURE1DARRAY` selects `texture_1d_array`.
            let v = unsafe { &*view_desc.view.texture_1d_array };
            let slices = if v.array_size == u32::MAX {
                depth_or_array.saturating_sub(v.first_array_slice)
            } else {
                v.array_size
            };
            (
                WMT_TEXTURE_TYPE2D_ARRAY,
                v.mip_slice,
                v.first_array_slice,
                slices,
            )
        }
        D3D12_UAV_DIMENSION_TEXTURE2DARRAY => {
            // SAFETY: `view_dimension == D3D12_UAV_DIMENSION_TEXTURE2DARRAY` selects `texture_2d_array`.
            let v = unsafe { &*view_desc.view.texture_2d_array };
            let slices = if v.array_size == u32::MAX {
                depth_or_array.saturating_sub(v.first_array_slice)
            } else {
                v.array_size
            };
            (
                WMT_TEXTURE_TYPE2D_ARRAY,
                v.mip_slice,
                v.first_array_slice,
                slices,
            )
        }
        D3D12_UAV_DIMENSION_TEXTURE3D => {
            // SAFETY: `view_dimension == D3D12_UAV_DIMENSION_TEXTURE3D` selects `texture_3d`.
            let v = unsafe { &*view_desc.view.texture_3d };
            if v.first_w_slice > 0 {
                crate::err!("3D UAV with WSlice unsupported");
            }
            (WMT_TEXTURE_TYPE3D, v.mip_slice, 0, 1)
        }
        _ => return E_INVALIDARG,
    };

    let view = TextureViewDescriptor {
        format: metal_format.pixel_format,
        texture_type,
        first_mip,
        mip_count: 1,
        first_slice,
        array_size,
    }
    .key();
    let atomic_view = (metal_format.pixel_format == WMT_PIXEL_FORMAT_R32_FLOAT).then(|| {
        TextureViewDescriptor {
            format: WMT_PIXEL_FORMAT_R32_UINT,
            texture_type,
            first_mip,
            mip_count: 1,
            first_slice,
            array_size,
        }
        .key()
    });
    // SAFETY: `descriptor` is forwarded to `descriptor_heap`.
    unsafe {
        crate::descriptor_heap::write_uav_texture(
            descriptor,
            resource.texture.clone(),
            view,
            atomic_view,
        )
    }
}

/// `MTLD3D12Texture::CreateRenderTargetView` (`d3d12_texture.cpp:679-796`).
///
/// # Safety
///
/// `desc` is null or points to a readable `D3D12_RENDER_TARGET_VIEW_DESC`.
pub unsafe fn create_rtv(
    resource: &Resource,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::scope!("texture.CreateRenderTargetView");
    let view_desc = if desc.is_null() {
        match super::extract_entire_rtv(&resource.desc) {
            Ok(v) => v,
            Err(hr) => return hr,
        }
    } else {
        // SAFETY: `desc` is non-null and points to a `RenderTargetViewDesc`.
        let mut v = unsafe { ptr::read(desc.cast::<RenderTargetViewDesc>()) };
        if v.format == 0 {
            v.format = resource.desc.format;
        }
        v
    };

    let (hr, metal_format) = dxmt_core::format::query_dxgi(view_desc.format);
    if hr != S_OK {
        return hr;
    }

    let tex_array_len = resource.texture.as_deref().map_or_else(
        || u32::from(resource.desc.depth_or_array_size),
        Texture::array_length,
    );
    let mut depth_plane = 0u32;
    let mut rt_array_len = 0u32;

    let (texture_type, first_mip, first_slice, array_size) = match view_desc.view_dimension {
        D3D12_RTV_DIMENSION_TEXTURE1D => {
            // SAFETY: `view_dimension == D3D12_RTV_DIMENSION_TEXTURE1D` selects `texture_1d`.
            let v = unsafe { &*view_desc.view.texture_1d };
            (WMT_TEXTURE_TYPE2D, v.mip_slice, 0, 1)
        }
        D3D12_RTV_DIMENSION_TEXTURE1DARRAY => {
            // SAFETY: `view_dimension == D3D12_RTV_DIMENSION_TEXTURE1DARRAY` selects `texture_1d_array`.
            let v = unsafe { &*view_desc.view.texture_1d_array };
            let slices = if v.array_size == u32::MAX {
                tex_array_len
            } else {
                v.array_size
            };
            rt_array_len = slices;
            (
                WMT_TEXTURE_TYPE2D_ARRAY,
                v.mip_slice,
                v.first_array_slice,
                slices,
            )
        }
        D3D12_RTV_DIMENSION_TEXTURE2D => {
            // SAFETY: `view_dimension == D3D12_RTV_DIMENSION_TEXTURE2D` selects `texture_2d`.
            let v = unsafe { &*view_desc.view.texture_2d };
            if v.plane_slice != 0 {
                let (): () = crate::not_implemented!("Texture::CreateRenderTargetView");
            }
            (WMT_TEXTURE_TYPE2D, v.mip_slice, 0, 1)
        }
        D3D12_RTV_DIMENSION_TEXTURE2DARRAY => {
            // SAFETY: `view_dimension == D3D12_RTV_DIMENSION_TEXTURE2DARRAY` selects `texture_2d_array`.
            let v = unsafe { &*view_desc.view.texture_2d_array };
            if v.plane_slice != 0 {
                let (): () = crate::not_implemented!("Texture::CreateRenderTargetView");
            }
            let slices = if v.array_size == u32::MAX {
                tex_array_len
            } else {
                v.array_size
            };
            rt_array_len = slices;
            (
                WMT_TEXTURE_TYPE2D_ARRAY,
                v.mip_slice,
                v.first_array_slice,
                slices,
            )
        }
        D3D12_RTV_DIMENSION_TEXTURE2DMS => (WMT_TEXTURE_TYPE2D_MULTISAMPLE, 0, 0, 1),
        D3D12_RTV_DIMENSION_TEXTURE2DMSARRAY => {
            // SAFETY: `view_dimension == D3D12_RTV_DIMENSION_TEXTURE2DMSARRAY` selects `texture_2dms_array`.
            let v = unsafe { &*view_desc.view.texture_2dms_array };
            let slices = if v.array_size == u32::MAX {
                tex_array_len
            } else {
                v.array_size
            };
            rt_array_len = slices;
            (
                WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
                0,
                v.first_array_slice,
                slices,
            )
        }
        D3D12_RTV_DIMENSION_TEXTURE3D => {
            // SAFETY: `view_dimension == D3D12_RTV_DIMENSION_TEXTURE3D` selects `texture_3d`.
            let v = unsafe { &*view_desc.view.texture_3d };
            let mip_depth = (u32::from(resource.desc.depth_or_array_size) >> v.mip_slice).max(1);
            depth_plane = v.first_w_slice;
            rt_array_len = if v.w_size == u32::MAX {
                mip_depth
            } else {
                v.w_size
            };
            (WMT_TEXTURE_TYPE3D, v.mip_slice, 0, 1)
        }
        _ => return E_INVALIDARG,
    };

    let view = TextureViewDescriptor {
        format: metal_format.pixel_format,
        texture_type,
        first_mip,
        mip_count: 1,
        first_slice,
        array_size,
    }
    .key();
    let mip_shift = first_mip & 0xf;
    let base_w = resource.texture.as_ref().map_or_else(
        || u32::try_from(resource.desc.width).unwrap_or(1),
        |t| t.info().width,
    );
    let base_h = resource
        .texture
        .as_ref()
        .map_or(resource.desc.height, |t| t.info().height);
    let rt_desc = RenderTargetDesc {
        texture: resource.texture.clone(),
        view,
        width: (base_w >> mip_shift).max(1),
        height: (base_h >> mip_shift).max(1),
        depth_plane,
        render_target_array_length: rt_array_len,
        flags: 0,
    };
    // SAFETY: `descriptor` is forwarded to `descriptor_heap`.
    unsafe { crate::descriptor_heap::write_rtv(descriptor, rt_desc) }
}

/// `MTLD3D12Texture::CreateDepthStencilView` (`d3d12_texture.cpp:799-899`).
///
/// # Safety
///
/// `desc` is null or points to a readable `D3D12_DEPTH_STENCIL_VIEW_DESC`.
pub unsafe fn create_dsv(
    resource: &Resource,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::scope!("texture.CreateDepthStencilView");
    let view_desc = if desc.is_null() {
        match super::extract_entire_dsv(&resource.desc) {
            Ok(v) => v,
            Err(hr) => return hr,
        }
    } else {
        // SAFETY: `desc` is non-null and points to a `DepthStencilViewDesc`.
        let mut v = unsafe { ptr::read(desc.cast::<DepthStencilViewDesc>()) };
        if v.format == 0 {
            v.format = resource.desc.format;
        }
        v
    };

    let (hr, metal_format) = dxmt_core::format::query_dxgi(view_desc.format);
    if hr != S_OK {
        return hr;
    }

    let tex_array_len = resource.texture.as_deref().map_or_else(
        || u32::from(resource.desc.depth_or_array_size),
        Texture::array_length,
    );
    let mut rt_array_len = 0u32;

    let (texture_type, first_mip, first_slice, array_size) = match view_desc.view_dimension {
        D3D12_DSV_DIMENSION_TEXTURE1D => {
            // SAFETY: `view_dimension == D3D12_DSV_DIMENSION_TEXTURE1D` selects `texture_1d`.
            let v = unsafe { &*view_desc.view.texture_1d };
            (WMT_TEXTURE_TYPE2D, v.mip_slice, 0, 1)
        }
        D3D12_DSV_DIMENSION_TEXTURE1DARRAY => {
            // SAFETY: `view_dimension == D3D12_DSV_DIMENSION_TEXTURE1DARRAY` selects `texture_1d_array`.
            let v = unsafe { &*view_desc.view.texture_1d_array };
            let slices = if v.array_size == u32::MAX {
                tex_array_len
            } else {
                v.array_size
            };
            rt_array_len = slices;
            (
                WMT_TEXTURE_TYPE2D_ARRAY,
                v.mip_slice,
                v.first_array_slice,
                slices,
            )
        }
        D3D12_DSV_DIMENSION_TEXTURE2D => {
            // SAFETY: `view_dimension == D3D12_DSV_DIMENSION_TEXTURE2D` selects `texture_2d`.
            let v = unsafe { &*view_desc.view.texture_2d };
            (WMT_TEXTURE_TYPE2D, v.mip_slice, 0, 1)
        }
        D3D12_DSV_DIMENSION_TEXTURE2DARRAY => {
            // SAFETY: `view_dimension == D3D12_DSV_DIMENSION_TEXTURE2DARRAY` selects `texture_2d_array`.
            let v = unsafe { &*view_desc.view.texture_2d_array };
            let slices = if v.array_size == u32::MAX {
                tex_array_len
            } else {
                v.array_size
            };
            rt_array_len = slices;
            (
                WMT_TEXTURE_TYPE2D_ARRAY,
                v.mip_slice,
                v.first_array_slice,
                slices,
            )
        }
        D3D12_DSV_DIMENSION_TEXTURE2DMS => (WMT_TEXTURE_TYPE2D_MULTISAMPLE, 0, 0, 1),
        D3D12_DSV_DIMENSION_TEXTURE2DMSARRAY => {
            // SAFETY: `view_dimension == D3D12_DSV_DIMENSION_TEXTURE2DMSARRAY` selects `texture_2dms_array`.
            let v = unsafe { &*view_desc.view.texture_2dms_array };
            let slices = if v.array_size == u32::MAX {
                tex_array_len
            } else {
                v.array_size
            };
            rt_array_len = slices;
            (
                WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
                0,
                v.first_array_slice,
                slices,
            )
        }
        _ => return E_INVALIDARG,
    };

    let view = TextureViewDescriptor {
        format: metal_format.pixel_format,
        texture_type,
        first_mip,
        mip_count: 1,
        first_slice,
        array_size,
    }
    .key();
    let mip_shift = first_mip & 0xf;
    let base_w = resource.texture.as_ref().map_or_else(
        || u32::try_from(resource.desc.width).unwrap_or(1),
        |t| t.info().width,
    );
    let base_h = resource
        .texture
        .as_ref()
        .map_or(resource.desc.height, |t| t.info().height);
    let rt_desc = RenderTargetDesc {
        texture: resource.texture.clone(),
        view,
        width: (base_w >> mip_shift).max(1),
        height: (base_h >> mip_shift).max(1),
        depth_plane: 0,
        render_target_array_length: rt_array_len,
        flags: view_desc.flags,
    };
    // SAFETY: `descriptor` is forwarded to `descriptor_heap`.
    unsafe { crate::descriptor_heap::write_dsv(descriptor, rt_desc) }
}

/// `MTLD3D12Texture::GetResourceTiling` (`d3d12_texture.cpp:901-907`).
pub fn get_resource_tiling(_resource: &Resource) {
    crate::scope!("texture.GetResourceTiling");
    let (): () = crate::not_implemented!("Texture::GetResourceTiling");
}
