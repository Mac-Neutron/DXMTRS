//! D3D12 resource objects (`ID3D12Resource2`, `d3d12_buffer.cpp`, `d3d12_texture.cpp`).

pub mod buffer;
pub mod texture;

use core::{
    ffi::c_void,
    mem::ManuallyDrop,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use std::sync::Arc;

use dxmt_core::{
    format::{
        CAP_COLOR, MTL_DXGI_FORMAT_BC, MTL_DXGI_FORMAT_DEPTH_PLANER, MTL_DXGI_FORMAT_STENCIL_PLANER,
    },
    resource::{
        Buffer, Texture, TextureAllocationFlags, TextureViewDescriptor,
        dims::clamped_mip_count,
        texture_info::{Dimension, atomic_ok, info_bits, lossless, texture_type},
        view::{ResourceShape, ViewDesc, ViewDimension, ViewKind, entire_view},
    },
    settings::{Flags, settings},
};
use dxmt_util::{
    com::ComObject,
    private_data::{HasPrivateData, PrivateData},
};
use winemetal_sys::{
    MTLDevice_heapTextureSizeAndAlign, MTLDevice_supportsFamily,
    wire::{
        WMT_PIXEL_FORMAT_DEPTH16_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_PIXEL_FORMAT_R16_FLOAT,
        WMT_PIXEL_FORMAT_R16_SINT, WMT_PIXEL_FORMAT_R16_SNORM, WMT_PIXEL_FORMAT_R16_UINT,
        WMT_PIXEL_FORMAT_R16_UNORM, WMT_PIXEL_FORMAT_R32_FLOAT, WMT_PIXEL_FORMAT_R32_SINT,
        WMT_PIXEL_FORMAT_R32_UINT, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
        WMT_TEXTURE_FLAG_LOSSLESS, WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW,
        WMT_TEXTURE_USAGE_RENDER_TARGET, WMT_TEXTURE_USAGE_SHADER_ATOMIC,
        WMT_TEXTURE_USAGE_SHADER_READ, WMT_TEXTURE_USAGE_SHADER_WRITE, WMTGPU_FAMILY_APPLE8,
        WMTTextureInfo,
    },
};

use crate::{
    abi::{
        barrier::{D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_RENDER_TARGET},
        pipeline::SampleDesc,
        resource::{
            D3D12_CPU_PAGE_PROPERTY_UNKNOWN, D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT,
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT, D3D12_HEAP_FLAG_NONE,
            D3D12_MEMORY_POOL_UNKNOWN, D3D12_RESOURCE_DIMENSION_BUFFER,
            D3D12_RESOURCE_DIMENSION_TEXTURE1D, D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            D3D12_RESOURCE_DIMENSION_TEXTURE3D, D3D12_RESOURCE_FLAG_ALLOW_CROSS_ADAPTER,
            D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
            D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, D3D12_TEXTURE_LAYOUT_UNKNOWN, D3d12Box,
            HeapProperties, MipRegion, ResourceDesc, ResourceDesc1,
        },
        views::{
            ConstantBufferViewDesc, D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            D3D12_DSV_DIMENSION_TEXTURE1D, D3D12_DSV_DIMENSION_TEXTURE1DARRAY,
            D3D12_DSV_DIMENSION_TEXTURE2D, D3D12_DSV_DIMENSION_TEXTURE2DARRAY,
            D3D12_DSV_DIMENSION_TEXTURE2DMS, D3D12_DSV_DIMENSION_TEXTURE2DMSARRAY,
            D3D12_DSV_FLAG_NONE, D3D12_RTV_DIMENSION_TEXTURE1D, D3D12_RTV_DIMENSION_TEXTURE1DARRAY,
            D3D12_RTV_DIMENSION_TEXTURE2D, D3D12_RTV_DIMENSION_TEXTURE2DARRAY,
            D3D12_RTV_DIMENSION_TEXTURE2DMS, D3D12_RTV_DIMENSION_TEXTURE2DMSARRAY,
            D3D12_RTV_DIMENSION_TEXTURE3D, D3D12_SRV_DIMENSION_TEXTURE1D,
            D3D12_SRV_DIMENSION_TEXTURE1DARRAY, D3D12_SRV_DIMENSION_TEXTURE2D,
            D3D12_SRV_DIMENSION_TEXTURE2DARRAY, D3D12_SRV_DIMENSION_TEXTURE2DMS,
            D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY, D3D12_SRV_DIMENSION_TEXTURE3D,
            D3D12_SRV_DIMENSION_TEXTURECUBE, D3D12_SRV_DIMENSION_TEXTURECUBEARRAY,
            D3D12_UAV_DIMENSION_TEXTURE1D, D3D12_UAV_DIMENSION_TEXTURE1DARRAY,
            D3D12_UAV_DIMENSION_TEXTURE2D, D3D12_UAV_DIMENSION_TEXTURE2DARRAY,
            D3D12_UAV_DIMENSION_TEXTURE3D, D3d12HeapDesc, DepthStencilViewDesc,
            DepthStencilViewDescBody, RenderTargetViewDesc, RenderTargetViewDescBody,
            ShaderResourceViewDesc, ShaderResourceViewDescBody, Tex1dArrayDsv, Tex1dArrayRtv,
            Tex1dArraySrv, Tex1dArrayUav, Tex1dDsv, Tex1dRtv, Tex1dSrv, Tex1dUav, Tex2dArrayDsv,
            Tex2dArrayRtv, Tex2dArraySrv, Tex2dArrayUav, Tex2dDsv, Tex2dRtv, Tex2dSrv, Tex2dUav,
            Tex2dmsArrayDsv, Tex2dmsArrayRtv, Tex2dmsArraySrv, Tex2dmsDsv, Tex2dmsRtv, Tex2dmsSrv,
            Tex3dRtv, Tex3dSrv, Tex3dUav, TexCubeArraySrv, TexCubeSrv, UnorderedAccessViewDesc,
            UnorderedAccessViewDescBody,
        },
        vtbl::{D3d12CpuDescriptorHandle, ID3D12Resource2Vtbl},
    },
    com::{self, ComRefCounted, E_FAIL, E_INVALIDARG, E_NOTIMPL, Hresult, S_OK},
    descriptor_heap::RenderTargetDesc,
    device::Device,
    heap::HeapObject,
    resource_math::{
        is_private_heap, plain_desc, validate_heap_properties, validate_resource_descs,
        validate_resource_states,
    },
};

/// `D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE`.
const D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE: u32 = 0x8;
/// `D3D12_SMALL_RESOURCE_PLACEMENT_ALIGNMENT`.
const D3D12_SMALL_RESOURCE_PLACEMENT_ALIGNMENT: u64 = 4096;

/// Unified `ID3D12Resource2` COM object backing both `MTLD3D12Buffer` and `MTLD3D12Texture`.
#[repr(C)]
pub struct Resource {
    pub vtbl: &'static ID3D12Resource2Vtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub texture: Option<Arc<Texture>>,
    pub buffer: Option<Arc<Buffer>>,
    pub placed_heap: *mut HeapObject,
    pub desc: ResourceDesc,
    pub heap_props: HeapProperties,
}

impl Resource {
    /// Returns the underlying [`Buffer`] when this resource is a buffer.
    #[must_use]
    pub const fn buffer(&self) -> Option<&Arc<Buffer>> {
        self.buffer.as_ref()
    }

    /// Returns the underlying [`Texture`] when this resource is a texture.
    #[must_use]
    pub const fn texture(&self) -> Option<&Arc<Texture>> {
        self.texture.as_ref()
    }

    /// Returns the GPU virtual address of this resource (`0` for textures).
    #[must_use]
    pub fn gpu_va(&self) -> u64 {
        self.buffer
            .as_ref()
            .and_then(|b| b.current())
            .map_or(0, |a| a.gpu_address())
    }

    /// Returns the [`ResourceDesc`] of this resource.
    #[must_use]
    pub const fn desc(&self) -> &ResourceDesc {
        &self.desc
    }

    /// Returns the [`HeapProperties`] of this resource.
    #[must_use]
    pub const fn heap_props(&self) -> &HeapProperties {
        &self.heap_props
    }
}

impl ComRefCounted for Resource {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for Resource {
    fn drop(&mut self) {
        if !self.device.is_null() {
            // SAFETY: `self.device` was retained at construction and remains valid until released below.
            let dev = unsafe { &*self.device };
            if let Some(buf) = &self.buffer {
                let gpu_va = buf.current().map_or(0, |a| a.gpu_address());
                crate::device::residency::unregister(dev, buf, gpu_va);
            }
            if let Some(tex) = &self.texture
                && tex.current().is_some_and(|a| {
                    !a.flags()
                        .contains(TextureAllocationFlags::ALLOCATED_ON_HEAP)
                })
            {
                crate::device::residency::unregister_texture(dev, tex);
            }
        }
        if !self.placed_heap.is_null() {
            // SAFETY: `self.placed_heap` was retained with `add_ref_private_com` at construction.
            unsafe { com::release_private_com(self.placed_heap) };
        }
        if !self.device.is_null() {
            // SAFETY: `self.device` was retained with `add_ref_com` at construction.
            unsafe { com::release_com(self.device) };
        }
    }
}

impl Resource {
    /// Returns `&self.desc` while recording `buffer.GetDesc`.
    #[inline(never)]
    #[must_use]
    pub fn buffer_desc(&self) -> &ResourceDesc {
        crate::scope!("buffer.GetDesc");
        &self.desc
    }

    /// Returns `&self.desc` while recording `texture.GetDesc`.
    #[inline(never)]
    #[must_use]
    pub fn texture_desc(&self) -> &ResourceDesc {
        crate::scope!("texture.GetDesc");
        &self.desc
    }

    /// Returns `&self.desc` while recording `buffer.GetDesc` or `texture.GetDesc`.
    #[must_use]
    pub fn desc_scoped(&self) -> &ResourceDesc {
        if self.desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
            self.buffer_desc()
        } else {
            self.texture_desc()
        }
    }
}

/// Copies a [`ResourceDesc`] by value.
#[must_use]
pub const fn clone_desc(d: &ResourceDesc) -> ResourceDesc {
    ResourceDesc {
        dimension: d.dimension,
        alignment: d.alignment,
        width: d.width,
        height: d.height,
        depth_or_array_size: d.depth_or_array_size,
        mip_levels: d.mip_levels,
        format: d.format,
        sample_desc: SampleDesc {
            count: d.sample_desc.count,
            quality: d.sample_desc.quality,
        },
        layout: d.layout,
        flags: d.flags,
    }
}

/// Copies [`HeapProperties`] by value.
#[must_use]
pub const fn clone_heap_props(h: &HeapProperties) -> HeapProperties {
    HeapProperties {
        r#type: h.r#type,
        cpu_page_property: h.cpu_page_property,
        memory_pool_preference: h.memory_pool_preference,
        creation_node_mask: h.creation_node_mask,
        visible_node_mask: h.visible_node_mask,
    }
}

/// Copies a [`D3d12Box`] by value.
#[must_use]
pub const fn clone_box(b: &D3d12Box) -> D3d12Box {
    D3d12Box {
        left: b.left,
        top: b.top,
        front: b.front,
        right: b.right,
        bottom: b.bottom,
        back: b.back,
    }
}

/// Whether an opt-in experiment uses memoryless storage for transient depth targets.
#[must_use]
pub fn memoryless_enabled() -> bool {
    settings().enabled(Flags::MEMORYLESS)
}

/// Whether `DXMT_D3D12_PRIVATE` is enabled (defaults to `true` unless set to `"0"`).
#[must_use]
pub fn private_heap_enabled() -> bool {
    settings().enabled(Flags::PRIVATE_HEAP)
}

/// Shared `IUnknown::QueryInterface` for `ID3D12Resource2` (`d3d12_buffer.cpp:88-107`).
///
/// # Safety
///
/// `this` is a valid `Resource` pointer; `riid` and `out` are caller-supplied COM pointers.
pub unsafe fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
    class_name: &'static str,
) -> Hresult {
    let qi = com::Qi {
        null_riid: E_INVALIDARG,
        silent: &[],
        log: &crate::LOG,
        miss: com::QiMiss::Once {
            object: &com::IID_ID3D12_RESOURCE,
            name: class_name,
        },
    };
    let add_ref = || {
        // SAFETY: `this` is a valid `Resource` pointer.
        unsafe { com::add_ref_com(this.cast::<Resource>()) };
    };
    // SAFETY: the application's `riid` and `out`, null or valid; `this` is a live `Resource`.
    unsafe {
        com::qi_table(
            riid,
            out,
            &[(
                &[
                    com::IID_IUNKNOWN,
                    com::IID_ID3D12_OBJECT,
                    com::IID_ID3D12_DEVICE_CHILD,
                    com::IID_ID3D12_PAGEABLE,
                    com::IID_ID3D12_RESOURCE,
                    com::IID_ID3D12_RESOURCE1,
                    com::IID_ID3D12_RESOURCE2,
                ],
                this,
            )],
            add_ref,
            &qi,
        )
    }
}

impl HasPrivateData for Resource {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `Resource`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

/// Shared `ID3D12DeviceChild::GetDevice` for `Resource`.
///
/// # Safety
///
/// `this` is a valid `Resource` pointer; `riid` and `out` are caller-supplied COM pointers.
pub unsafe fn get_device(this: *mut c_void, riid: *const c_void, out: *mut *mut c_void) -> Hresult {
    // SAFETY: `this` is a valid `Resource` pointer.
    let res = unsafe { &*this.cast::<Resource>() };
    if res.device.is_null() {
        if !out.is_null() {
            // SAFETY: `out` was checked non-null above.
            unsafe { com::init_out_ptr(out) };
        }
        return E_FAIL;
    }
    // SAFETY: `res.device` is a valid `Device` COM pointer.
    let vtbl = unsafe { (*res.device).vtbl };
    // SAFETY: `query_interface` is the device's COM `QueryInterface` slot.
    unsafe { (vtbl.query_interface)(res.device.cast(), riid, out) }
}

/// Completes a resource factory call via `QueryInterface(riid, out)`.
///
/// # Safety
///
/// `riid` and `out` are caller-supplied COM pointers (`out` must be non-null).
pub unsafe fn return_resource(
    resource: Box<Resource>,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let raw = Box::into_raw(resource);
    // SAFETY: `raw` is a freshly boxed `Resource`.
    let qi = unsafe { (*raw).vtbl.query_interface };
    // SAFETY: `raw` has 0 refs; `qi` adds 1 ref on `S_OK`.
    let hr = unsafe { qi(raw.cast(), riid, out) };
    if hr != S_OK {
        // SAFETY: `query_interface` did not retain `raw`, so `raw` is uniquely owned here.
        unsafe { drop(Box::from_raw(raw)) };
    }
    hr
}

/// Queries the `D3d12HeapDesc` of a live `HeapObject` via its vtable (`GetDesc`).
///
/// # Safety
///
/// `heap` must be a non-null, live `HeapObject` pointer.
#[must_use]
pub unsafe fn heap_desc(heap: *mut HeapObject) -> D3d12HeapDesc {
    let mut desc = D3d12HeapDesc {
        size_in_bytes: 0,
        properties: HeapProperties {
            r#type: 0,
            cpu_page_property: 0,
            memory_pool_preference: 0,
            creation_node_mask: 0,
            visible_node_mask: 0,
        },
        alignment: 0,
        flags: 0,
    };
    // SAFETY: `heap` is a valid `HeapObject` pointer.
    let vtbl = unsafe { (*heap).vtbl };
    // SAFETY: `get_desc` writes a `D3D12_HEAP_DESC` into `desc`.
    let _ = unsafe { (vtbl.get_desc)(heap.cast(), (&raw mut desc).cast()) };
    desc
}

/// Shared `GetHeapProperties` implementation (`d3d12_buffer.cpp:156-163`).
///
/// # Safety
///
/// `this` is a valid `Resource` pointer; `heap_props` and `heap_flags` are null or writable.
pub unsafe fn get_heap_properties(
    this: *mut c_void,
    heap_props: *mut c_void,
    heap_flags: *mut u32,
) -> Hresult {
    // SAFETY: `this` is a valid `Resource` COM pointer.
    let res = unsafe { &*this.cast::<Resource>() };
    if !heap_props.is_null() {
        // SAFETY: `heap_props` was checked non-null above.
        unsafe { *heap_props.cast::<HeapProperties>() = clone_heap_props(&res.heap_props) };
    }
    if !heap_flags.is_null() {
        let flags = if res.placed_heap.is_null() {
            D3D12_HEAP_FLAG_NONE
        } else {
            // SAFETY: `res.placed_heap` is a live `HeapObject` pointer.
            unsafe { heap_desc(res.placed_heap).flags }
        };
        // SAFETY: `heap_flags` was checked non-null above.
        unsafe { *heap_flags = flags };
    }
    S_OK
}

/// Shared `ID3D12Resource1::GetDesc1` implementation (`d3d12_device.hpp:125-131`).
///
/// # Safety
///
/// `this` is a valid `Resource` pointer and `out` is non-null.
pub unsafe fn get_desc1(this: *mut c_void, out: *mut c_void) -> *mut c_void {
    // SAFETY: `this` is a valid `Resource` COM pointer.
    let res = unsafe { &*this.cast::<Resource>() };
    let mut d = clone_desc(&res.desc);
    // SAFETY: `get_desc` is the resource's vtable slot.
    let _ = unsafe { (res.vtbl.get_desc)(this, (&raw mut d).cast()) };
    if !out.is_null() {
        let d1 = ResourceDesc1 {
            dimension: d.dimension,
            alignment: d.alignment,
            width: d.width,
            height: d.height,
            depth_or_array_size: d.depth_or_array_size,
            mip_levels: d.mip_levels,
            format: d.format,
            sample_desc: SampleDesc {
                count: d.sample_desc.count,
                quality: d.sample_desc.quality,
            },
            layout: d.layout,
            flags: d.flags,
            sampler_feedback_mip_region: MipRegion {
                width: 0,
                height: 0,
                depth: 0,
            },
        };
        // SAFETY: `out` was checked non-null above.
        unsafe { *out.cast::<ResourceDesc1>() = d1 };
    }
    out
}

/// Resolves the `WMTPixelFormat` a texture resource uses (`d3d12_texture.cpp:35-54`).
#[must_use]
pub fn resource_pixel_format(desc: &ResourceDesc) -> u32 {
    let (hr, fmt) = dxmt_core::format::query_dxgi(desc.format);
    if hr != S_OK {
        return 0;
    }
    let mut pf = fmt.pixel_format;
    if (desc.flags & D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL) != 0 {
        match pf {
            WMT_PIXEL_FORMAT_R32_UINT | WMT_PIXEL_FORMAT_R32_SINT | WMT_PIXEL_FORMAT_R32_FLOAT => {
                pf = WMT_PIXEL_FORMAT_DEPTH32_FLOAT;
            }
            WMT_PIXEL_FORMAT_R16_UINT
            | WMT_PIXEL_FORMAT_R16_SINT
            | WMT_PIXEL_FORMAT_R16_FLOAT
            | WMT_PIXEL_FORMAT_R16_UNORM
            | WMT_PIXEL_FORMAT_R16_SNORM => {
                pf = WMT_PIXEL_FORMAT_DEPTH16_UNORM;
            }
            _ => {}
        }
    }
    pf
}

/// Casts a non-null `ID3D12Resource` pointer to [`Resource`] if its vtable matches.
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_resource<'a>(raw: *mut c_void) -> Option<&'a Resource> {
    if raw.is_null() {
        return None;
    }
    // SAFETY: `raw` is non-null and points to a COM object header by contract.
    let is_buf = unsafe { com::has_vtbl(raw, &buffer::BUFFER_VTBL) };
    // SAFETY: as above.
    let is_tex = unsafe { com::has_vtbl(raw, &texture::TEXTURE_VTBL) };
    if is_buf || is_tex {
        // SAFETY: vtable matched `BUFFER_VTBL` or `TEXTURE_VTBL`, both backed by `Resource`.
        Some(unsafe { &*raw.cast::<Resource>() })
    } else {
        None
    }
}

/// D3D12's shape of a texture for its whole-resource views.
///
/// An array past one slice, multisampled past one sample, never a cube; none for a buffer or an
/// unknown dimension.
fn shape(desc: &ResourceDesc) -> Option<ResourceShape> {
    let dimension = match desc.dimension {
        D3D12_RESOURCE_DIMENSION_TEXTURE1D => Dimension::Texture1D,
        D3D12_RESOURCE_DIMENSION_TEXTURE2D => Dimension::Texture2D,
        D3D12_RESOURCE_DIMENSION_TEXTURE3D => Dimension::Texture3D,
        _ => return None,
    };
    Some(ResourceShape {
        dimension,
        mip_levels: u32::from(desc.mip_levels),
        array_size: u32::from(desc.depth_or_array_size),
        is_array: desc.depth_or_array_size > 1,
        multisampled: desc.sample_desc.count > 1,
        cube: false,
    })
}

/// `view` as a `D3D12_SHADER_RESOURCE_VIEW_DESC` in `format`, default mapping and no LOD clamp.
const fn srv_desc(format: u32, view: &ViewDesc) -> ShaderResourceViewDesc {
    let (mip, mips) = (view.first_mip, view.mip_count);
    let (first, size) = (view.first_slice, view.array_size);
    let (view_dimension, body) = match view.dimension {
        ViewDimension::Texture1D => (
            D3D12_SRV_DIMENSION_TEXTURE1D,
            ShaderResourceViewDescBody {
                texture_1d: ManuallyDrop::new(Tex1dSrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    resource_min_lod_clamp: 0.0,
                }),
            },
        ),
        ViewDimension::Texture1DArray => (
            D3D12_SRV_DIMENSION_TEXTURE1DARRAY,
            ShaderResourceViewDescBody {
                texture_1d_array: ManuallyDrop::new(Tex1dArraySrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    first_array_slice: first,
                    array_size: size,
                    resource_min_lod_clamp: 0.0,
                }),
            },
        ),
        ViewDimension::Texture2D => (
            D3D12_SRV_DIMENSION_TEXTURE2D,
            ShaderResourceViewDescBody {
                texture_2d: ManuallyDrop::new(Tex2dSrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    plane_slice: 0,
                    resource_min_lod_clamp: 0.0,
                }),
            },
        ),
        ViewDimension::Texture2DArray => (
            D3D12_SRV_DIMENSION_TEXTURE2DARRAY,
            ShaderResourceViewDescBody {
                texture_2d_array: ManuallyDrop::new(Tex2dArraySrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    first_array_slice: first,
                    array_size: size,
                    plane_slice: 0,
                    resource_min_lod_clamp: 0.0,
                }),
            },
        ),
        ViewDimension::Texture2DMS => (
            D3D12_SRV_DIMENSION_TEXTURE2DMS,
            ShaderResourceViewDescBody {
                texture_2dms: ManuallyDrop::new(Tex2dmsSrv {
                    unused_field_nothing_to_define: 0,
                }),
            },
        ),
        ViewDimension::Texture2DMSArray => (
            D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY,
            ShaderResourceViewDescBody {
                texture_2dms_array: ManuallyDrop::new(Tex2dmsArraySrv {
                    first_array_slice: first,
                    array_size: size,
                }),
            },
        ),
        ViewDimension::Texture3D => (
            D3D12_SRV_DIMENSION_TEXTURE3D,
            ShaderResourceViewDescBody {
                texture_3d: ManuallyDrop::new(Tex3dSrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    resource_min_lod_clamp: 0.0,
                }),
            },
        ),
        ViewDimension::TextureCube => (
            D3D12_SRV_DIMENSION_TEXTURECUBE,
            ShaderResourceViewDescBody {
                texture_cube: ManuallyDrop::new(TexCubeSrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    resource_min_lod_clamp: 0.0,
                }),
            },
        ),
        ViewDimension::TextureCubeArray => (
            D3D12_SRV_DIMENSION_TEXTURECUBEARRAY,
            ShaderResourceViewDescBody {
                texture_cube_array: ManuallyDrop::new(TexCubeArraySrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    first_2d_array_face: first,
                    num_cubes: size / 6,
                    resource_min_lod_clamp: 0.0,
                }),
            },
        ),
    };
    ShaderResourceViewDesc {
        format,
        view_dimension,
        shader_4_component_mapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
        view: body,
    }
}

/// `view` as a `D3D12_UNORDERED_ACCESS_VIEW_DESC` in `format`; none for a dimension it lacks.
const fn uav_desc(format: u32, view: &ViewDesc) -> Option<UnorderedAccessViewDesc> {
    let (mip, first, size) = (view.first_mip, view.first_slice, view.array_size);
    let (view_dimension, body) = match view.dimension {
        ViewDimension::Texture1D => (
            D3D12_UAV_DIMENSION_TEXTURE1D,
            UnorderedAccessViewDescBody {
                texture_1d: ManuallyDrop::new(Tex1dUav { mip_slice: mip }),
            },
        ),
        ViewDimension::Texture1DArray => (
            D3D12_UAV_DIMENSION_TEXTURE1DARRAY,
            UnorderedAccessViewDescBody {
                texture_1d_array: ManuallyDrop::new(Tex1dArrayUav {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                }),
            },
        ),
        ViewDimension::Texture2D => (
            D3D12_UAV_DIMENSION_TEXTURE2D,
            UnorderedAccessViewDescBody {
                texture_2d: ManuallyDrop::new(Tex2dUav {
                    mip_slice: mip,
                    plane_slice: 0,
                }),
            },
        ),
        ViewDimension::Texture2DArray => (
            D3D12_UAV_DIMENSION_TEXTURE2DARRAY,
            UnorderedAccessViewDescBody {
                texture_2d_array: ManuallyDrop::new(Tex2dArrayUav {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                    plane_slice: 0,
                }),
            },
        ),
        ViewDimension::Texture3D => (
            D3D12_UAV_DIMENSION_TEXTURE3D,
            UnorderedAccessViewDescBody {
                texture_3d: ManuallyDrop::new(Tex3dUav {
                    mip_slice: mip,
                    first_w_slice: first,
                    w_size: size,
                }),
            },
        ),
        _ => return None,
    };
    Some(UnorderedAccessViewDesc {
        format,
        view_dimension,
        view: body,
    })
}

/// `view` as a `D3D12_RENDER_TARGET_VIEW_DESC` in `format`; none for a dimension it lacks.
const fn rtv_desc(format: u32, view: &ViewDesc) -> Option<RenderTargetViewDesc> {
    let (mip, first, size) = (view.first_mip, view.first_slice, view.array_size);
    let (view_dimension, body) = match view.dimension {
        ViewDimension::Texture1D => (
            D3D12_RTV_DIMENSION_TEXTURE1D,
            RenderTargetViewDescBody {
                texture_1d: ManuallyDrop::new(Tex1dRtv { mip_slice: mip }),
            },
        ),
        ViewDimension::Texture1DArray => (
            D3D12_RTV_DIMENSION_TEXTURE1DARRAY,
            RenderTargetViewDescBody {
                texture_1d_array: ManuallyDrop::new(Tex1dArrayRtv {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                }),
            },
        ),
        ViewDimension::Texture2D => (
            D3D12_RTV_DIMENSION_TEXTURE2D,
            RenderTargetViewDescBody {
                texture_2d: ManuallyDrop::new(Tex2dRtv {
                    mip_slice: mip,
                    plane_slice: 0,
                }),
            },
        ),
        ViewDimension::Texture2DArray => (
            D3D12_RTV_DIMENSION_TEXTURE2DARRAY,
            RenderTargetViewDescBody {
                texture_2d_array: ManuallyDrop::new(Tex2dArrayRtv {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                    plane_slice: 0,
                }),
            },
        ),
        ViewDimension::Texture2DMS => (
            D3D12_RTV_DIMENSION_TEXTURE2DMS,
            RenderTargetViewDescBody {
                texture_2dms: ManuallyDrop::new(Tex2dmsRtv {
                    unused_field_nothing_to_define: 0,
                }),
            },
        ),
        ViewDimension::Texture2DMSArray => (
            D3D12_RTV_DIMENSION_TEXTURE2DMSARRAY,
            RenderTargetViewDescBody {
                texture_2dms_array: ManuallyDrop::new(Tex2dmsArrayRtv {
                    first_array_slice: first,
                    array_size: size,
                }),
            },
        ),
        ViewDimension::Texture3D => (
            D3D12_RTV_DIMENSION_TEXTURE3D,
            RenderTargetViewDescBody {
                texture_3d: ManuallyDrop::new(Tex3dRtv {
                    mip_slice: mip,
                    first_w_slice: first,
                    w_size: size,
                }),
            },
        ),
        _ => return None,
    };
    Some(RenderTargetViewDesc {
        format,
        view_dimension,
        view: body,
    })
}

/// `view` as a `D3D12_DEPTH_STENCIL_VIEW_DESC` in `format`; none for a dimension it lacks.
const fn dsv_desc(format: u32, view: &ViewDesc) -> Option<DepthStencilViewDesc> {
    let (mip, first, size) = (view.first_mip, view.first_slice, view.array_size);
    let (view_dimension, body) = match view.dimension {
        ViewDimension::Texture1D => (
            D3D12_DSV_DIMENSION_TEXTURE1D,
            DepthStencilViewDescBody {
                texture_1d: ManuallyDrop::new(Tex1dDsv { mip_slice: mip }),
            },
        ),
        ViewDimension::Texture1DArray => (
            D3D12_DSV_DIMENSION_TEXTURE1DARRAY,
            DepthStencilViewDescBody {
                texture_1d_array: ManuallyDrop::new(Tex1dArrayDsv {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                }),
            },
        ),
        ViewDimension::Texture2D => (
            D3D12_DSV_DIMENSION_TEXTURE2D,
            DepthStencilViewDescBody {
                texture_2d: ManuallyDrop::new(Tex2dDsv { mip_slice: mip }),
            },
        ),
        ViewDimension::Texture2DArray => (
            D3D12_DSV_DIMENSION_TEXTURE2DARRAY,
            DepthStencilViewDescBody {
                texture_2d_array: ManuallyDrop::new(Tex2dArrayDsv {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                }),
            },
        ),
        ViewDimension::Texture2DMS => (
            D3D12_DSV_DIMENSION_TEXTURE2DMS,
            DepthStencilViewDescBody {
                texture_2dms: ManuallyDrop::new(Tex2dmsDsv {
                    unused_field_nothing_to_define: 0,
                }),
            },
        ),
        ViewDimension::Texture2DMSArray => (
            D3D12_DSV_DIMENSION_TEXTURE2DMSARRAY,
            DepthStencilViewDescBody {
                texture_2dms_array: ManuallyDrop::new(Tex2dmsArrayDsv {
                    first_array_slice: first,
                    array_size: size,
                }),
            },
        ),
        _ => return None,
    };
    Some(DepthStencilViewDesc {
        format,
        view_dimension,
        flags: D3D12_DSV_FLAG_NONE,
        view: body,
    })
}

/// Extracts a default `D3D12_SHADER_RESOURCE_VIEW_DESC` (`d3d12_resource_helper.cpp:145-217`).
///
/// # Errors
///
/// Returns `E_FAIL` for buffer resources or `E_INVALIDARG` for unknown dimensions.
pub fn extract_entire_srv(desc: &ResourceDesc) -> Result<ShaderResourceViewDesc, Hresult> {
    if desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
        crate::err!("Unsupported buffer SRV");
        return Err(E_FAIL);
    }
    let resource = shape(desc).ok_or(E_INVALIDARG)?;
    entire_view(ViewKind::ShaderResource, &resource)
        .map(|v| srv_desc(desc.format, &v))
        .ok_or(E_FAIL)
}

/// Extracts a default `D3D12_UNORDERED_ACCESS_VIEW_DESC` (`d3d12_resource_helper.cpp:221-270`).
///
/// # Errors
///
/// Returns `E_FAIL` for buffer or MSAA resources, or `E_INVALIDARG` for unknown dimensions.
pub fn extract_entire_uav(desc: &ResourceDesc) -> Result<UnorderedAccessViewDesc, Hresult> {
    if desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
        crate::err!("Unsupported buffer UAV");
        return Err(E_FAIL);
    }
    let resource = shape(desc).ok_or(E_INVALIDARG)?;
    entire_view(ViewKind::UnorderedAccess, &resource)
        .and_then(|v| uav_desc(desc.format, &v))
        .ok_or(E_FAIL)
}

/// Extracts a default `D3D12_RENDER_TARGET_VIEW_DESC` (`d3d12_resource_helper.cpp:84-141`).
///
/// # Errors
///
/// Returns `E_FAIL` for buffer resources or `E_INVALIDARG` for unknown dimensions.
pub fn extract_entire_rtv(desc: &ResourceDesc) -> Result<RenderTargetViewDesc, Hresult> {
    if desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
        crate::err!("Unsupported buffer RTV");
        return Err(E_FAIL);
    }
    let resource = shape(desc).ok_or(E_INVALIDARG)?;
    entire_view(ViewKind::RenderTarget, &resource)
        .and_then(|v| rtv_desc(desc.format, &v))
        .ok_or(E_FAIL)
}

/// Extracts a default `D3D12_DEPTH_STENCIL_VIEW_DESC` (`d3d12_resource_helper.cpp:27-80`).
///
/// # Errors
///
/// Returns `E_FAIL` for buffer or 3D texture resources, or `E_INVALIDARG` for unknown dimensions.
pub fn extract_entire_dsv(desc: &ResourceDesc) -> Result<DepthStencilViewDesc, Hresult> {
    if desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
        crate::err!("Unsupported buffer DSV");
        return Err(E_FAIL);
    }
    if desc.dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D {
        crate::err!("Unsupported 3d DSV");
        return Err(E_FAIL);
    }
    let resource = shape(desc).ok_or(E_INVALIDARG)?;
    entire_view(ViewKind::DepthStencil, &resource)
        .and_then(|v| dsv_desc(desc.format, &v))
        .ok_or(E_FAIL)
}

/// Populates a `WMTTextureInfo` from a `D3D12_RESOURCE_DESC` (`d3d12_texture.cpp:28-181`).
///
/// # Safety
///
/// If non-null, `device` must point to a valid `Device`.
pub unsafe fn populate_texture_info_opt(
    device: *const Device,
    desc: &ResourceDesc,
    info: &mut WMTTextureInfo,
) -> Hresult {
    let (hr, fmt) = dxmt_core::format::query_dxgi(desc.format);
    if hr != S_OK {
        return hr;
    }

    info.pixel_format = resource_pixel_format(desc);

    let (tex_type, sample_count) = match desc.dimension {
        D3D12_RESOURCE_DIMENSION_TEXTURE1D => {
            if (fmt.flag
                & (MTL_DXGI_FORMAT_BC
                    | MTL_DXGI_FORMAT_DEPTH_PLANER
                    | MTL_DXGI_FORMAT_STENCIL_PLANER))
                != 0
            {
                return E_INVALIDARG;
            }
            info.width = (desc.width & 0xffff_ffff) as u32;
            info.height = 1;
            info.depth = 1;
            info.array_length = u32::from(desc.depth_or_array_size);
            (
                texture_type(Dimension::Texture1D, info.array_length, 1, false),
                1u32,
            )
        }
        D3D12_RESOURCE_DIMENSION_TEXTURE2D => {
            info.width = (desc.width & 0xffff_ffff) as u32;
            info.height = desc.height;
            info.depth = 1;
            info.array_length = u32::from(desc.depth_or_array_size);
            if desc.sample_desc.count == 0 {
                return E_INVALIDARG;
            }
            let samples = desc.sample_desc.count;
            let t = texture_type(Dimension::Texture2D, info.array_length, samples, false);
            (t, if samples > 1 { samples } else { 1 })
        }
        D3D12_RESOURCE_DIMENSION_TEXTURE3D => {
            info.width = (desc.width & 0xffff_ffff) as u32;
            info.height = desc.height;
            info.depth = u32::from(desc.depth_or_array_size);
            info.array_length = 1;
            (texture_type(Dimension::Texture3D, 1, 1, false), 1u32)
        }
        _ => return E_INVALIDARG,
    };

    let mip_count = clamped_mip_count(
        info.width,
        info.height,
        info.depth,
        u32::from(desc.mip_levels),
    );

    // SAFETY: if non-null, `device` is a valid `Device` pointer.
    let dev_opt = unsafe { device.as_ref() };
    let mtl_opt = dev_opt.and_then(|d| d.mtl.as_ref());

    let mut usage = WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW;
    if (desc.flags & D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET) != 0 {
        usage |= WMT_TEXTURE_USAGE_RENDER_TARGET;
    }
    if (desc.flags & D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL) != 0 {
        usage |= WMT_TEXTURE_USAGE_RENDER_TARGET;
    }
    if (desc.flags & D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS) != 0 {
        usage |= WMT_TEXTURE_USAGE_SHADER_READ | WMT_TEXTURE_USAGE_SHADER_WRITE;
        let supports_apple8 = || {
            mtl_opt.is_some_and(|m| {
                // SAFETY: `m` holds a live `MTLDevice`.
                unsafe { MTLDevice_supportsFamily(m.handle(), WMTGPU_FAMILY_APPLE8) }
            })
        };
        if atomic_ok(fmt.pixel_format, supports_apple8) {
            usage |= WMT_TEXTURE_USAGE_SHADER_ATOMIC;
        }
        let gpu_only = HeapProperties {
            r#type: D3D12_HEAP_TYPE_DEFAULT,
            cpu_page_property: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            memory_pool_preference: D3D12_MEMORY_POOL_UNKNOWN,
            creation_node_mask: 0,
            visible_node_mask: 0,
        };
        let color_cap =
            dev_opt.is_some_and(|d| (d.format_capability(info.pixel_format) & CAP_COLOR) != 0);
        if desc.dimension == D3D12_RESOURCE_DIMENSION_TEXTURE2D
            && is_private_heap(&gpu_only, private_heap_enabled())
            && color_cap
        {
            usage |= WMT_TEXTURE_USAGE_RENDER_TARGET;
        }
    }
    if (desc.flags & D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE) == 0 {
        usage |= WMT_TEXTURE_USAGE_SHADER_READ;
    }

    info.bits = info_bits(tex_type, mip_count, sample_count, usage);
    info.options = WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED;

    let uav = (desc.flags & D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS) != 0;
    info.flags = 0;
    if lossless(settings().d3d12_compression, info.pixel_format, uav)
        && (desc.flags & D3D12_RESOURCE_FLAG_ALLOW_CROSS_ADAPTER) == 0
        && desc.layout == D3D12_TEXTURE_LAYOUT_UNKNOWN
        && desc.alignment != D3D12_SMALL_RESOURCE_PLACEMENT_ALIGNMENT
    {
        info.flags = WMT_TEXTURE_FLAG_LOSSLESS;
    }

    if desc.alignment != 0 {
        if desc.alignment != D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT
            && desc.alignment != D3D12_SMALL_RESOURCE_PLACEMENT_ALIGNMENT
            && (desc.sample_desc.count == 1
                || desc.alignment != D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT)
        {
            return E_INVALIDARG;
        }
        if desc.alignment == D3D12_SMALL_RESOURCE_PLACEMENT_ALIGNMENT {
            if desc.layout != D3D12_TEXTURE_LAYOUT_UNKNOWN {
                return E_INVALIDARG;
            }
            if (desc.flags
                & (D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL
                    | D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET))
                != 0
            {
                return E_INVALIDARG;
            }
            if desc.sample_desc.count > 1 {
                return E_INVALIDARG;
            }
            if let Some(mtl_dev) = mtl_opt {
                let mut info_one_slice = *info;
                info_one_slice.bits = info_bits(tex_type, 1, sample_count, usage);
                info_one_slice.array_length = 1;
                // SAFETY: `mtl_dev` holds a live `MTLDevice`.
                let size_and_align = unsafe {
                    MTLDevice_heapTextureSizeAndAlign(mtl_dev.handle(), &raw mut info_one_slice)
                };
                if u64::from(size_and_align.size) > D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT {
                    return E_INVALIDARG;
                }
            }
        }
    }

    S_OK
}

/// Populates a `WMTTextureInfo` from a `D3D12_RESOURCE_DESC` (`d3d12_texture.cpp:28-181`).
pub fn populate_wmt_texture_info(
    device: &Device,
    desc: &ResourceDesc,
    info: &mut WMTTextureInfo,
) -> Hresult {
    // SAFETY: `device` is a valid reference.
    unsafe { populate_texture_info_opt(ptr::from_ref(device), desc, info) }
}

fn ignore_castable_formats(num_castable_formats: u32) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if num_castable_formats != 0 && !WARNED.swap(true, Ordering::Relaxed) {
        crate::warn!("CreateCommittedResource3/CreatePlacedResource2: castable formats ignored");
    }
}

/// `ID3D12Device::CreateConstantBufferView` (`d3d12_device.cpp:1023`).
///
/// # Safety
///
/// `this` and `desc` are caller-supplied pointers.
pub unsafe fn create_constant_buffer_view(
    _this: *mut c_void,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateConstantBufferView");
    if desc.is_null() {
        // SAFETY: `descriptor` is a caller-supplied CPU descriptor handle.
        let _ = unsafe { crate::descriptor_heap::write_cbv(descriptor, 0, 0) };
    } else {
        // SAFETY: `desc` is non-null and points to a `ConstantBufferViewDesc`.
        let d = unsafe { &*desc.cast::<ConstantBufferViewDesc>() };
        // SAFETY: `descriptor` is a caller-supplied CPU descriptor handle.
        let _ = unsafe {
            crate::descriptor_heap::write_cbv(descriptor, d.buffer_location, d.size_in_bytes)
        };
    }
}

/// `ID3D12Device::CreateShaderResourceView` (`d3d12_device.cpp:1033`).
///
/// # Safety
///
/// `this`, `resource`, and `desc` are caller-supplied pointers.
pub unsafe fn create_shader_resource_view(
    _this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateShaderResourceView");
    if resource.is_null() {
        // SAFETY: `descriptor` and `desc` are caller-supplied.
        let _ = unsafe { crate::descriptor_heap::write_srv_null(descriptor, desc) };
        return;
    }
    // SAFETY: `resource` is non-null.
    if let Some(res) = unsafe { cast_resource(resource) } {
        if res.desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
            // SAFETY: `desc` and `descriptor` are caller-supplied.
            let _ = unsafe { buffer::create_srv(res, desc, descriptor) };
        } else {
            // SAFETY: `desc` and `descriptor` are caller-supplied.
            let _ = unsafe { texture::create_srv(res, desc, descriptor) };
        }
    }
}

/// `ID3D12Device::CreateUnorderedAccessView` (`d3d12_device.cpp:1047`).
///
/// # Safety
///
/// `this`, `resource`, `counter`, and `desc` are caller-supplied pointers.
pub unsafe fn create_unordered_access_view(
    _this: *mut c_void,
    resource: *mut c_void,
    counter: *mut c_void,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateUnorderedAccessView");
    if resource.is_null() {
        // SAFETY: `descriptor` and `desc` are caller-supplied.
        let _ = unsafe { crate::descriptor_heap::write_uav_null(descriptor, desc) };
        return;
    }
    // SAFETY: `resource` is non-null.
    if let Some(res) = unsafe { cast_resource(resource) } {
        if res.desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
            // SAFETY: `counter`, `desc`, and `descriptor` are caller-supplied.
            let _ = unsafe { buffer::create_uav(res, counter, desc, descriptor) };
        } else {
            // SAFETY: `counter`, `desc`, and `descriptor` are caller-supplied.
            let _ = unsafe { texture::create_uav(res, counter, desc, descriptor) };
        }
    }
}

/// `ID3D12Device::CreateRenderTargetView` (`d3d12_device.cpp:1062`).
///
/// # Safety
///
/// `this`, `resource`, and `desc` are caller-supplied pointers.
pub unsafe fn create_render_target_view(
    _this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateRenderTargetView");
    if resource.is_null() {
        let null_rt = RenderTargetDesc {
            texture: None,
            view: TextureViewDescriptor {
                format: 0,
                texture_type: 0,
                first_mip: 0,
                mip_count: 0,
                first_slice: 0,
                array_size: 0,
            }
            .key(),
            width: 0,
            height: 0,
            depth_plane: 0,
            render_target_array_length: 0,
            flags: 0,
        };
        // SAFETY: `descriptor` is caller-supplied.
        let _ = unsafe { crate::descriptor_heap::write_rtv(descriptor, null_rt) };
        return;
    }
    // SAFETY: `resource` is non-null.
    if let Some(res) = unsafe { cast_resource(resource) } {
        if res.desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
            let _ = buffer::create_rtv(res, desc, descriptor);
        } else {
            // SAFETY: `desc` and `descriptor` are caller-supplied.
            let _ = unsafe { texture::create_rtv(res, desc, descriptor) };
        }
    }
}

/// `ID3D12Device::CreateDepthStencilView` (`d3d12_device.cpp:1076`).
///
/// # Safety
///
/// `this`, `resource`, and `desc` are caller-supplied pointers.
pub unsafe fn create_depth_stencil_view(
    _this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateDepthStencilView");
    if resource.is_null() {
        let null_ds = RenderTargetDesc {
            texture: None,
            view: TextureViewDescriptor {
                format: 0,
                texture_type: 0,
                first_mip: 0,
                mip_count: 0,
                first_slice: 0,
                array_size: 0,
            }
            .key(),
            width: 0,
            height: 0,
            depth_plane: 0,
            render_target_array_length: 0,
            flags: 0,
        };
        // SAFETY: `descriptor` is caller-supplied.
        let _ = unsafe { crate::descriptor_heap::write_dsv(descriptor, null_ds) };
        return;
    }
    // SAFETY: `resource` is non-null.
    if let Some(res) = unsafe { cast_resource(resource) } {
        if res.desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
            let _ = buffer::create_dsv(res, desc, descriptor);
        } else {
            // SAFETY: `desc` and `descriptor` are caller-supplied.
            let _ = unsafe { texture::create_dsv(res, desc, descriptor) };
        }
    }
}

/// `ID3D12Device8::CreateSamplerFeedbackUnorderedAccessView` (`d3d12_device.cpp:1714`).
///
/// # Safety
///
/// `this`, `targeted`, and `feedback` are caller-supplied pointers.
pub unsafe fn create_sampler_feedback_unordered_access_view(
    _this: *mut c_void,
    _targeted: *mut c_void,
    _feedback: *mut c_void,
    _dest: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateSamplerFeedbackUnorderedAccessView");
    crate::warn!("CreateSamplerFeedbackUnorderedAccessView: sampler feedback not supported");
}

/// `ID3D12Device::CreateCommittedResource` (`d3d12_device.cpp:1203`).
///
/// # Safety
///
/// `this`, `heap_props`, `desc`, `optimized_clear`, `riid`, and `resource` are caller-supplied
/// COM pointers.
pub unsafe extern "system-unwind" fn create_committed_resource(
    this: *mut c_void,
    heap_props: *const c_void,
    heap_flags: u32,
    desc: *const c_void,
    initial_state: u32,
    optimized_clear: *const c_void,
    riid: *const c_void,
    resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommittedResource");
    if !resource.is_null() {
        // SAFETY: `resource` was checked non-null above.
        unsafe { com::init_out_ptr(resource) };
    }
    if heap_props.is_null() || desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `heap_props` and `desc` were checked non-null above.
    let hp = unsafe { &*heap_props.cast::<HeapProperties>() };
    // SAFETY: `desc` was checked non-null above.
    let rd = unsafe { &*desc.cast::<ResourceDesc>() };
    let dev_ptr = this.cast::<Device>();
    // SAFETY: if non-null, `dev_ptr` is a valid `Device` pointer.
    let advertise_numa = (!dev_ptr.is_null()) && unsafe { (*dev_ptr).advertise_numa };

    let hr = validate_heap_properties(hp, heap_flags, advertise_numa);
    if hr != S_OK {
        return hr;
    }
    let hr = validate_resource_descs(rd, hp);
    if hr != S_OK {
        return hr;
    }
    let hr = validate_resource_states(initial_state, hp);
    if hr != S_OK {
        return hr;
    }

    match rd.dimension {
        D3D12_RESOURCE_DIMENSION_TEXTURE1D
        | D3D12_RESOURCE_DIMENSION_TEXTURE2D
        | D3D12_RESOURCE_DIMENSION_TEXTURE3D => {
            if initial_state == D3D12_RESOURCE_STATE_RENDER_TARGET
                && (rd.flags & D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET) == 0
            {
                return E_INVALIDARG;
            }
            // SAFETY: pointers are validated caller inputs.
            unsafe {
                texture::create(
                    dev_ptr,
                    (hp, heap_flags),
                    rd,
                    optimized_clear,
                    None,
                    &[],
                    (riid, resource),
                )
            }
        }
        D3D12_RESOURCE_DIMENSION_BUFFER => {
            if !optimized_clear.is_null() {
                return E_INVALIDARG;
            }
            // SAFETY: pointers are validated caller inputs.
            unsafe { buffer::create(dev_ptr, hp, heap_flags, rd, None, riid, resource) }
        }
        _ => E_INVALIDARG,
    }
}

/// `ID3D12Device::CreatePlacedResource` (`d3d12_device.cpp:1247`).
///
/// # Safety
///
/// `this`, `heap`, `desc`, `optimized_clear`, `riid`, and `resource` are caller-supplied COM
/// pointers.
pub unsafe extern "system-unwind" fn create_placed_resource(
    this: *mut c_void,
    heap: *mut c_void,
    heap_offset: u64,
    desc: *const c_void,
    initial_state: u32,
    optimized_clear: *const c_void,
    riid: *const c_void,
    resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreatePlacedResource");
    if !resource.is_null() {
        // SAFETY: `resource` was checked non-null above.
        unsafe { com::init_out_ptr(resource) };
    }
    if heap.is_null() || desc.is_null() {
        return E_INVALIDARG;
    }
    let heap_obj = heap.cast::<HeapObject>();
    // SAFETY: `heap_obj` is a non-null `ID3D12Heap` pointer.
    let hdesc = unsafe { heap_desc(heap_obj) };
    // SAFETY: `desc` was checked non-null above.
    let rd = unsafe { &*desc.cast::<ResourceDesc>() };
    let dev_ptr = this.cast::<Device>();
    // SAFETY: if non-null, `dev_ptr` is a valid `Device` pointer.
    let advertise_numa = (!dev_ptr.is_null()) && unsafe { (*dev_ptr).advertise_numa };

    let hr = validate_heap_properties(&hdesc.properties, hdesc.flags, advertise_numa);
    if hr != S_OK {
        return hr;
    }
    let hr = validate_resource_descs(rd, &hdesc.properties);
    if hr != S_OK {
        return hr;
    }
    let hr = validate_resource_states(initial_state, &hdesc.properties);
    if hr != S_OK {
        return hr;
    }

    match rd.dimension {
        D3D12_RESOURCE_DIMENSION_TEXTURE1D
        | D3D12_RESOURCE_DIMENSION_TEXTURE2D
        | D3D12_RESOURCE_DIMENSION_TEXTURE3D => {
            if initial_state == D3D12_RESOURCE_STATE_RENDER_TARGET
                && (rd.flags & D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET) == 0
            {
                return E_INVALIDARG;
            }
            // SAFETY: pointers are validated caller inputs.
            unsafe {
                texture::create(
                    dev_ptr,
                    (&hdesc.properties, hdesc.flags),
                    rd,
                    optimized_clear,
                    Some((heap_obj, heap_offset)),
                    &[],
                    (riid, resource),
                )
            }
        }
        D3D12_RESOURCE_DIMENSION_BUFFER => {
            if !optimized_clear.is_null() {
                return E_INVALIDARG;
            }
            // SAFETY: pointers are validated caller inputs.
            unsafe {
                buffer::create(
                    dev_ptr,
                    &hdesc.properties,
                    hdesc.flags,
                    rd,
                    Some((heap_obj, heap_offset)),
                    riid,
                    resource,
                )
            }
        }
        _ => E_INVALIDARG,
    }
}

/// `ID3D12Device::CreateReservedResource` (`d3d12_device.cpp:1281`).
///
/// # Safety
///
/// `this`, `desc`, `optimized_clear`, `riid`, and `resource` are caller-supplied COM pointers.
pub unsafe fn create_reserved_resource(
    _this: *mut c_void,
    _desc: *const c_void,
    _initial_state: u32,
    _optimized_clear: *const c_void,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateReservedResource");
    E_NOTIMPL
}

/// `ID3D12Device4::CreateCommittedResource1` (`d3d12_device.cpp:1574`).
///
/// # Safety
///
/// `this`, `heap_props`, `desc`, `optimized_clear`, `protected_session`, `riid`, and `resource`
/// are caller-supplied COM pointers.
pub unsafe extern "system-unwind" fn create_committed_resource1(
    this: *mut c_void,
    heap_props: *const c_void,
    heap_flags: u32,
    desc: *const c_void,
    initial_state: u32,
    optimized_clear: *const c_void,
    protected_session: *mut c_void,
    riid: *const c_void,
    resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommittedResource1");
    if !protected_session.is_null() {
        return E_NOTIMPL;
    }
    // SAFETY: arguments are forwarded to `create_committed_resource`.
    unsafe {
        create_committed_resource(
            this,
            heap_props,
            heap_flags,
            desc,
            initial_state,
            optimized_clear,
            riid,
            resource,
        )
    }
}

/// `ID3D12Device4::CreateReservedResource1` (`d3d12_device.cpp:1738`).
///
/// # Safety
///
/// `this`, `desc`, `optimized_clear`, `protected_session`, `riid`, and `resource` are
/// caller-supplied COM pointers.
pub unsafe fn create_reserved_resource1(
    _this: *mut c_void,
    _desc: *const c_void,
    _initial_state: u32,
    _optimized_clear: *const c_void,
    _protected_session: *mut c_void,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateReservedResource1");
    E_NOTIMPL
}

/// `ID3D12Device8::CreateCommittedResource2` (`d3d12_device.cpp:1689`).
///
/// # Safety
///
/// `this`, `heap_props`, `desc`, `optimized_clear`, `protected_session`, `riid`, and `resource`
/// are caller-supplied COM pointers.
pub unsafe extern "system-unwind" fn create_committed_resource2(
    this: *mut c_void,
    heap_props: *const c_void,
    heap_flags: u32,
    desc: *const c_void,
    initial_state: u32,
    optimized_clear: *const c_void,
    protected_session: *mut c_void,
    riid: *const c_void,
    resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommittedResource2");
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null (`D3D12_RESOURCE_DESC1`).
    let plain = plain_desc(unsafe { &*desc.cast::<ResourceDesc1>() });
    // SAFETY: arguments are forwarded to `create_committed_resource1`.
    unsafe {
        create_committed_resource1(
            this,
            heap_props,
            heap_flags,
            (&raw const plain).cast(),
            initial_state,
            optimized_clear,
            protected_session,
            riid,
            resource,
        )
    }
}

/// `ID3D12Device8::CreatePlacedResource1` (`d3d12_device.cpp:1702`).
///
/// # Safety
///
/// `this`, `heap`, `desc`, `optimized_clear`, `riid`, and `resource` are caller-supplied COM
/// pointers.
pub unsafe extern "system-unwind" fn create_placed_resource1(
    this: *mut c_void,
    heap: *mut c_void,
    heap_offset: u64,
    desc: *const c_void,
    initial_state: u32,
    optimized_clear: *const c_void,
    riid: *const c_void,
    resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreatePlacedResource1");
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null (`D3D12_RESOURCE_DESC1`).
    let plain = plain_desc(unsafe { &*desc.cast::<ResourceDesc1>() });
    // SAFETY: arguments are forwarded to `create_placed_resource`.
    unsafe {
        create_placed_resource(
            this,
            heap,
            heap_offset,
            (&raw const plain).cast(),
            initial_state,
            optimized_clear,
            riid,
            resource,
        )
    }
}

/// `ID3D12Device10::CreateCommittedResource3` (`d3d12_device.cpp:1777`).
///
/// # Safety
///
/// `this`, `heap_props`, `desc`, `optimized_clear`, `protected_session`, `castable_formats`,
/// `riid`, and `resource` are caller-supplied COM pointers.
pub unsafe extern "system-unwind" fn create_committed_resource3(
    this: *mut c_void,
    heap_props: *const c_void,
    heap_flags: u32,
    desc: *const c_void,
    initial_layout: u32,
    optimized_clear: *const c_void,
    protected_session: *mut c_void,
    num_castable_formats: u32,
    _castable_formats: *mut u32,
    riid: *const c_void,
    resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommittedResource3");
    if !resource.is_null() {
        // SAFETY: `resource` was checked non-null above.
        unsafe { com::init_out_ptr(resource) };
    }
    if heap_props.is_null() {
        return E_INVALIDARG;
    }
    ignore_castable_formats(num_castable_formats);
    // SAFETY: `heap_props` was checked non-null above.
    let heap_type = unsafe { (*heap_props.cast::<HeapProperties>()).r#type };
    let state = crate::barrier::initial_state(initial_layout, heap_type);
    // SAFETY: arguments are forwarded to `create_committed_resource2`.
    unsafe {
        create_committed_resource2(
            this,
            heap_props,
            heap_flags,
            desc,
            state,
            optimized_clear,
            protected_session,
            riid,
            resource,
        )
    }
}

/// `ID3D12Device10::CreatePlacedResource2` (`d3d12_device.cpp:1795`).
///
/// # Safety
///
/// `this`, `heap`, `desc`, `optimized_clear`, `castable_formats`, `riid`, and `resource` are
/// caller-supplied COM pointers.
pub unsafe extern "system-unwind" fn create_placed_resource2(
    this: *mut c_void,
    heap: *mut c_void,
    heap_offset: u64,
    desc: *const c_void,
    initial_layout: u32,
    optimized_clear: *const c_void,
    num_castable_formats: u32,
    _castable_formats: *mut u32,
    riid: *const c_void,
    resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreatePlacedResource2");
    if !resource.is_null() {
        // SAFETY: `resource` was checked non-null above.
        unsafe { com::init_out_ptr(resource) };
    }
    if heap.is_null() {
        return E_INVALIDARG;
    }
    ignore_castable_formats(num_castable_formats);
    // SAFETY: `heap` is a non-null `ID3D12Heap` pointer.
    let heap_type = unsafe { heap_desc(heap.cast()).properties.r#type };
    let state = crate::barrier::initial_state(initial_layout, heap_type);
    // SAFETY: arguments are forwarded to `create_placed_resource1`.
    unsafe {
        create_placed_resource1(
            this,
            heap,
            heap_offset,
            desc,
            state,
            optimized_clear,
            riid,
            resource,
        )
    }
}

/// `ID3D12Device10::CreateReservedResource2` (`d3d12_device.cpp:1812`).
///
/// # Safety
///
/// `this`, `desc`, `optimized_clear`, `protected_session`, `castable_formats`, `riid`, and
/// `resource` are caller-supplied COM pointers.
pub unsafe extern "system-unwind" fn create_reserved_resource2(
    _this: *mut c_void,
    _desc: *const c_void,
    _initial_layout: u32,
    _optimized_clear: *const c_void,
    _protected_session: *mut c_void,
    _num_castable_formats: u32,
    _castable_formats: *mut u32,
    _riid: *const c_void,
    resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateReservedResource2");
    if !resource.is_null() {
        // SAFETY: `resource` was checked non-null above.
        unsafe { com::init_out_ptr(resource) };
    }
    E_NOTIMPL
}

#[cfg(test)]
mod tests;
