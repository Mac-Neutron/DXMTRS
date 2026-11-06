//! Resource description validation and upgrade helpers (`d3d11_resource_helper.cpp`).

use dxmt_core::{
    format::{
        MTL_DXGI_FORMAT_DEPTH_PLANER, MTL_DXGI_FORMAT_STENCIL_PLANER, MTL_DXGI_FORMAT_TYPELESS,
        query_dxgi,
    },
    resource::{
        BufferAllocationFlags,
        dims::{linear_layout, linear_row_pitch, mip_count_wh, mip_extent},
        texture_info::{Dimension, atomic_ok, info_bits, lossless, texture_type},
        view::{ResourceShape, ViewDesc, ViewDimension, ViewKind, entire_view},
    },
    settings::settings,
};
use dxmt_util::dxgi_consts::{
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_DEPTH_STENCIL, D3D11_BIND_INDEX_BUFFER,
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_STREAM_OUTPUT, D3D11_BIND_UNORDERED_ACCESS,
    D3D11_BIND_VERTEX_BUFFER, D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC, D3D11_USAGE_IMMUTABLE,
    D3D11_USAGE_STAGING, DXGI_FORMAT_D16_UNORM, DXGI_FORMAT_D32_FLOAT, DXGI_FORMAT_R16_TYPELESS,
    DXGI_FORMAT_R32_TYPELESS,
};
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_DEPTH16_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
    WMT_RESOURCE_CPU_CACHE_MODE_WRITE_COMBINED, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
    WMT_RESOURCE_STORAGE_MODE_MANAGED, WMT_RESOURCE_STORAGE_MODE_SHARED, WMT_TEXTURE_FLAG_LOSSLESS,
    WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE2D_ARRAY, WMT_TEXTURE_TYPE2D_MULTISAMPLE,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW,
    WMT_TEXTURE_USAGE_RENDER_TARGET, WMT_TEXTURE_USAGE_SHADER_ATOMIC,
    WMT_TEXTURE_USAGE_SHADER_READ, WMT_TEXTURE_USAGE_SHADER_WRITE, WMTGPU_FAMILY_APPLE8,
    WMTTextureInfo, original_format,
};

use crate::{
    abi::records::{
        D3d11BufferDesc, D3d11BufferSrv, D3d11BufferUav, D3d11DepthStencilViewDesc,
        D3d11DepthStencilViewDescUnion, D3d11RenderTargetViewDesc1,
        D3d11RenderTargetViewDesc1Union, D3d11ShaderResourceViewDesc1,
        D3d11ShaderResourceViewDesc1Union, D3d11Tex1dArrayDsv, D3d11Tex1dArrayRtv,
        D3d11Tex1dArraySrv, D3d11Tex1dArrayUav, D3d11Tex1dDsv, D3d11Tex1dRtv, D3d11Tex1dSrv,
        D3d11Tex1dUav, D3d11Tex2dArrayDsv, D3d11Tex2dArrayRtv1, D3d11Tex2dArraySrv1,
        D3d11Tex2dArrayUav1, D3d11Tex2dDsv, D3d11Tex2dRtv1, D3d11Tex2dSrv1, D3d11Tex2dUav1,
        D3d11Tex2dmsArrayDsv, D3d11Tex2dmsArrayRtv, D3d11Tex2dmsArraySrv, D3d11Tex2dmsDsv,
        D3d11Tex2dmsRtv, D3d11Tex2dmsSrv, D3d11Tex3dRtv, D3d11Tex3dSrv, D3d11Tex3dUav,
        D3d11TexcubeArraySrv, D3d11TexcubeSrv, D3d11Texture1dDesc, D3d11Texture2dDesc,
        D3d11Texture2dDesc1, D3d11Texture3dDesc, D3d11Texture3dDesc1,
        D3d11UnorderedAccessViewDesc1, D3d11UnorderedAccessViewDesc1Union,
    },
    com::{E_FAIL, E_INVALIDARG, Hresult, S_OK},
};

const D3D11_RESOURCE_MISC_SHARED: u32 = 0x2;
const D3D11_RESOURCE_MISC_TEXTURECUBE: u32 = 0x4;
const D3D11_RESOURCE_MISC_BUFFER_STRUCTURED: u32 = 0x40;
const D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX: u32 = 0x100;
const D3D11_RESOURCE_MISC_SHARED_NTHANDLE: u32 = 0x800;
const D3D11_RESOURCE_MISC_TILE_POOL: u32 = 0x20000;

const fn validate_buffer_desc_inner(desc: *const D3d11BufferDesc) -> Hresult {
    // SAFETY: Caller passes either null or a valid pointer to `D3d11BufferDesc`.
    let Some(d) = (unsafe { desc.as_ref() }) else {
        return E_INVALIDARG;
    };
    if d.byte_width == 0 && (d.misc_flags & D3D11_RESOURCE_MISC_TILE_POOL) == 0 {
        return E_INVALIDARG;
    }
    if d.usage > D3D11_USAGE_STAGING {
        return E_INVALIDARG;
    }
    S_OK
}

/// Validates a [`D3d11BufferDesc`] (`ValidateBufferProperties`).
#[must_use]
pub const fn validate_buffer_desc(desc: *const D3d11BufferDesc) -> Hresult {
    validate_buffer_desc_inner(desc)
}

/// Upgrades a [`D3d11Texture2dDesc`] to [`D3d11Texture2dDesc1`] (`UpgradeResourceDescription`).
#[must_use]
pub const fn upgrade_texture2d_desc(src: &D3d11Texture2dDesc) -> D3d11Texture2dDesc1 {
    D3d11Texture2dDesc1 {
        width: src.width,
        height: src.height,
        mip_levels: src.mip_levels,
        array_size: src.array_size,
        format: src.format,
        sample_desc: src.sample_desc,
        usage: src.usage,
        bind_flags: src.bind_flags,
        cpu_access_flags: src.cpu_access_flags,
        misc_flags: src.misc_flags,
        texture_layout: 0,
    }
}

/// Downgrades a [`D3d11Texture2dDesc1`] to [`D3d11Texture2dDesc`] (`DowngradeResourceDescription`).
#[must_use]
pub const fn downgrade_texture2d_desc(src: &D3d11Texture2dDesc1) -> D3d11Texture2dDesc {
    D3d11Texture2dDesc {
        width: src.width,
        height: src.height,
        mip_levels: src.mip_levels,
        array_size: src.array_size,
        format: src.format,
        sample_desc: src.sample_desc,
        usage: src.usage,
        bind_flags: src.bind_flags,
        cpu_access_flags: src.cpu_access_flags,
        misc_flags: src.misc_flags,
    }
}

/// Upgrades a [`D3d11Texture3dDesc`] to [`D3d11Texture3dDesc1`] (`UpgradeResourceDescription`).
#[must_use]
pub const fn upgrade_texture3d_desc(src: &D3d11Texture3dDesc) -> D3d11Texture3dDesc1 {
    D3d11Texture3dDesc1 {
        width: src.width,
        height: src.height,
        depth: src.depth,
        mip_levels: src.mip_levels,
        format: src.format,
        usage: src.usage,
        bind_flags: src.bind_flags,
        cpu_access_flags: src.cpu_access_flags,
        misc_flags: src.misc_flags,
        texture_layout: 0,
    }
}

/// Downgrades a [`D3d11Texture3dDesc1`] to [`D3d11Texture3dDesc`] (`DowngradeResourceDescription`).
#[must_use]
pub const fn downgrade_texture3d_desc(src: &D3d11Texture3dDesc1) -> D3d11Texture3dDesc {
    D3d11Texture3dDesc {
        width: src.width,
        height: src.height,
        depth: src.depth,
        mip_levels: src.mip_levels,
        format: src.format,
        usage: src.usage,
        bind_flags: src.bind_flags,
        cpu_access_flags: src.cpu_access_flags,
        misc_flags: src.misc_flags,
    }
}

struct MtlTexDescParams {
    dim: Dimension,
    width: u32,
    height: u32,
    depth: u32,
    array_size: u32,
    sample_count: u32,
    bind_flags: u32,
    misc_flags: u32,
    usage: u32,
    mip_levels: u32,
    format: u32,
}

fn create_mtl_texture_descriptor_internal(
    p: &MtlTexDescParams,
    mtl_desc: &mut WMTTextureInfo,
    supports_family: impl Fn(u32) -> bool,
) -> Hresult {
    let MtlTexDescParams {
        dim,
        width,
        height,
        depth,
        array_size,
        sample_count,
        bind_flags,
        misc_flags,
        usage,
        mip_levels,
        format,
    } = *p;
    if width == 0 || height == 0 || depth == 0 || array_size == 0 || sample_count == 0 {
        return E_INVALIDARG;
    }

    let (hr, metal_format) = query_dxgi(format);
    if hr < 0 {
        return E_FAIL;
    }
    let mut pixel_format = metal_format.pixel_format;

    let shared_flag = D3D11_RESOURCE_MISC_SHARED | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX;
    if (misc_flags & shared_flag) == shared_flag {
        return E_INVALIDARG;
    }

    if (bind_flags & D3D11_BIND_DEPTH_STENCIL) != 0 {
        match format {
            DXGI_FORMAT_R32_TYPELESS | DXGI_FORMAT_D32_FLOAT => {
                pixel_format = WMT_PIXEL_FORMAT_DEPTH32_FLOAT;
            }
            DXGI_FORMAT_R16_TYPELESS | DXGI_FORMAT_D16_UNORM => {
                pixel_format = WMT_PIXEL_FORMAT_DEPTH16_UNORM;
            }
            _ => {}
        }
    }
    mtl_desc.pixel_format = original_format(pixel_format);

    let mut metal_usage = WMT_TEXTURE_USAGE_SHADER_READ;
    if (bind_flags
        & (D3D11_BIND_CONSTANT_BUFFER
            | D3D11_BIND_VERTEX_BUFFER
            | D3D11_BIND_INDEX_BUFFER
            | D3D11_BIND_STREAM_OUTPUT))
        != 0
    {
        return E_FAIL;
    }
    if (bind_flags & (D3D11_BIND_RENDER_TARGET | D3D11_BIND_DEPTH_STENCIL)) != 0 {
        metal_usage |= WMT_TEXTURE_USAGE_RENDER_TARGET;
    }
    if (bind_flags & D3D11_BIND_UNORDERED_ACCESS) != 0 {
        metal_usage |= WMT_TEXTURE_USAGE_SHADER_READ | WMT_TEXTURE_USAGE_SHADER_WRITE;
        if atomic_ok(pixel_format, || supports_family(WMTGPU_FAMILY_APPLE8)) {
            metal_usage |= WMT_TEXTURE_USAGE_SHADER_ATOMIC;
        }
    }
    if (metal_format.flag & MTL_DXGI_FORMAT_TYPELESS) != 0 {
        metal_usage |= WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW;
    }
    if (metal_format.flag & (MTL_DXGI_FORMAT_DEPTH_PLANER | MTL_DXGI_FORMAT_STENCIL_PLANER)) != 0 {
        metal_usage |= WMT_TEXTURE_USAGE_RENDER_TARGET;
    }

    let options = match usage {
        D3D11_USAGE_DEFAULT => WMT_RESOURCE_STORAGE_MODE_MANAGED,
        D3D11_USAGE_IMMUTABLE => {
            WMT_RESOURCE_STORAGE_MODE_MANAGED | WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED
        }
        D3D11_USAGE_DYNAMIC => {
            WMT_RESOURCE_STORAGE_MODE_SHARED | WMT_RESOURCE_CPU_CACHE_MODE_WRITE_COMBINED
        }
        D3D11_USAGE_STAGING => WMT_RESOURCE_STORAGE_MODE_SHARED,
        _ => 0,
    };
    mtl_desc.options = options;

    let mipmap_level_count = if mip_levels == 0 {
        mip_count_wh(width, height)
    } else {
        mip_levels
    };

    let is_2d = matches!(dim, Dimension::Texture2D);
    let cube = is_2d && (misc_flags & D3D11_RESOURCE_MISC_TEXTURECUBE) != 0;
    if is_2d && !cube && sample_count > 1 && !matches!(sample_count, 1 | 2 | 4 | 8) {
        return E_INVALIDARG;
    }
    let tex_type = texture_type(dim, array_size, sample_count, cube);
    let (out_array_length, out_sample_count) = match tex_type {
        WMT_TEXTURE_TYPE2D_ARRAY => (array_size, 1),
        WMT_TEXTURE_TYPE_CUBE_ARRAY => (array_size / 6, 1),
        WMT_TEXTURE_TYPE2D_MULTISAMPLE => (1, sample_count),
        WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY => (array_size, sample_count),
        _ => (1, 1),
    };

    mtl_desc.width = width;
    mtl_desc.height = height;
    mtl_desc.depth = depth;
    mtl_desc.array_length = out_array_length;
    mtl_desc.bits = info_bits(tex_type, mipmap_level_count, out_sample_count, metal_usage);

    let any_shared = D3D11_RESOURCE_MISC_SHARED
        | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX
        | D3D11_RESOURCE_MISC_SHARED_NTHANDLE;
    let uav = (bind_flags & D3D11_BIND_UNORDERED_ACCESS) != 0;
    mtl_desc.flags = if matches!(usage, D3D11_USAGE_DEFAULT | D3D11_USAGE_IMMUTABLE)
        && (misc_flags & any_shared) == 0
        && lossless(settings().d3d11_compression, mtl_desc.pixel_format, uav)
    {
        WMT_TEXTURE_FLAG_LOSSLESS
    } else {
        0
    };
    S_OK
}

/// Validates and translates a 1D texture description (`CreateMTLTextureDescriptor`).
#[must_use]
pub fn create_mtl_texture_descriptor_1d(
    desc: &D3d11Texture1dDesc,
    out_desc: &mut D3d11Texture1dDesc,
    mtl_desc: &mut WMTTextureInfo,
    supports_family: impl Fn(u32) -> bool,
    supports_bc: bool,
) -> Hresult {
    let _ = supports_bc;
    let hr = create_mtl_texture_descriptor_internal(
        &MtlTexDescParams {
            dim: Dimension::Texture1D,
            width: desc.width,
            height: 1,
            depth: 1,
            array_size: desc.array_size,
            sample_count: 1,
            bind_flags: desc.bind_flags,
            misc_flags: desc.misc_flags,
            usage: desc.usage,
            mip_levels: desc.mip_levels,
            format: desc.format,
        },
        mtl_desc,
        supports_family,
    );
    if hr >= 0 {
        *out_desc = *desc;
        out_desc.mip_levels = mtl_desc.mipmap_level_count();
    }
    hr
}

/// Validates and translates a 2D texture description (`CreateMTLTextureDescriptor`).
#[must_use]
pub fn create_mtl_texture_descriptor_2d(
    desc: &D3d11Texture2dDesc1,
    out_desc: &mut D3d11Texture2dDesc1,
    mtl_desc: &mut WMTTextureInfo,
    supports_family: impl Fn(u32) -> bool,
    supports_bc: bool,
) -> Hresult {
    let _ = supports_bc;
    let hr = create_mtl_texture_descriptor_internal(
        &MtlTexDescParams {
            dim: Dimension::Texture2D,
            width: desc.width,
            height: desc.height,
            depth: 1,
            array_size: desc.array_size,
            sample_count: desc.sample_desc.count,
            bind_flags: desc.bind_flags,
            misc_flags: desc.misc_flags,
            usage: desc.usage,
            mip_levels: desc.mip_levels,
            format: desc.format,
        },
        mtl_desc,
        supports_family,
    );
    if hr >= 0 {
        *out_desc = *desc;
        out_desc.mip_levels = mtl_desc.mipmap_level_count();
    }
    hr
}

/// Validates and translates a 3D texture description (`CreateMTLTextureDescriptor`).
#[must_use]
pub fn create_mtl_texture_descriptor_3d(
    desc: &D3d11Texture3dDesc1,
    out_desc: &mut D3d11Texture3dDesc1,
    mtl_desc: &mut WMTTextureInfo,
    supports_family: impl Fn(u32) -> bool,
    supports_bc: bool,
) -> Hresult {
    let _ = supports_bc;
    let hr = create_mtl_texture_descriptor_internal(
        &MtlTexDescParams {
            dim: Dimension::Texture3D,
            width: desc.width,
            height: desc.height,
            depth: desc.depth,
            array_size: 1,
            sample_count: 1,
            bind_flags: desc.bind_flags,
            misc_flags: desc.misc_flags,
            usage: desc.usage,
            mip_levels: desc.mip_levels,
            format: desc.format,
        },
        mtl_desc,
        supports_family,
    );
    if hr >= 0 {
        *out_desc = *desc;
        out_desc.mip_levels = mtl_desc.mipmap_level_count();
    }
    hr
}

/// Computes `(width, height, depth)` at `level` of a 1D texture (`GetMipmapSize`).
#[must_use]
pub const fn get_mipmap_size_1d(desc: &D3d11Texture1dDesc, level: u32) -> (u32, u32, u32) {
    (mip_extent(desc.width, level), 1, 1)
}

/// Computes `(width, height, depth)` at `level` of a 2D texture (`GetMipmapSize`).
#[must_use]
pub const fn get_mipmap_size_2d(desc: &D3d11Texture2dDesc1, level: u32) -> (u32, u32, u32) {
    (
        mip_extent(desc.width, level),
        mip_extent(desc.height, level),
        1,
    )
}

/// Computes `(width, height, depth)` at `level` of a 3D texture (`GetMipmapSize`).
#[must_use]
pub const fn get_mipmap_size_3d(desc: &D3d11Texture3dDesc1, level: u32) -> (u32, u32, u32) {
    (
        mip_extent(desc.width, level),
        mip_extent(desc.height, level),
        mip_extent(desc.depth, level),
    )
}

/// Computes `(bytes_per_row, bytes_per_image, bytes_per_slice)` for a 1D texture.
///
/// # Errors
///
/// Returns [`E_FAIL`] if `desc.format` has no linear byte layout.
pub fn get_linear_texture_layout_1d(
    desc: &D3d11Texture1dDesc,
    level: u32,
    aligned: bool,
) -> Result<(u32, u32, u32), Hresult> {
    let (hr, metal_format) = query_dxgi(desc.format);
    if hr < 0 {
        return Err(E_FAIL);
    }
    let (w, _, _) = get_mipmap_size_1d(desc, level);
    let row = linear_row_pitch(metal_format, w, aligned).ok_or(E_FAIL)?;
    Ok((row, row, row))
}

/// Computes `(bytes_per_row, bytes_per_image, bytes_per_slice)` for a 2D texture.
///
/// # Errors
///
/// Returns [`E_FAIL`] if `desc.format` has no linear byte layout.
pub fn get_linear_texture_layout_2d(
    desc: &D3d11Texture2dDesc1,
    level: u32,
    aligned: bool,
) -> Result<(u32, u32, u32), Hresult> {
    linear_texture_layout(desc.format, get_mipmap_size_2d(desc, level), aligned)
}

/// Computes `(bytes_per_row, bytes_per_image, bytes_per_slice)` for a 3D texture.
///
/// # Errors
///
/// Returns [`E_FAIL`] if `desc.format` has no linear byte layout.
pub fn get_linear_texture_layout_3d(
    desc: &D3d11Texture3dDesc1,
    level: u32,
    aligned: bool,
) -> Result<(u32, u32, u32), Hresult> {
    linear_texture_layout(desc.format, get_mipmap_size_3d(desc, level), aligned)
}

fn linear_texture_layout(
    format: u32,
    (w, h, d): (u32, u32, u32),
    aligned: bool,
) -> Result<(u32, u32, u32), Hresult> {
    let (hr, metal_format) = query_dxgi(format);
    if hr < 0 {
        return Err(E_FAIL);
    }
    let layout = linear_layout(metal_format, w, h, d, aligned).ok_or(E_FAIL)?;
    Ok((
        layout.bytes_per_row,
        layout.bytes_per_image,
        layout.bytes_per_slice,
    ))
}

/// Extracts or populates a buffer SRV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Returns [`E_FAIL`] if `view_in` is `None` and the buffer is not structured.
pub const fn extract_entire_srv_buffer(
    desc: &D3d11BufferDesc,
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
) -> Result<D3d11ShaderResourceViewDesc1, Hresult> {
    if let Some(v) = view_in {
        return Ok(*v);
    }
    if (desc.misc_flags & D3D11_RESOURCE_MISC_BUFFER_STRUCTURED) != 0
        && desc.structure_byte_stride != 0
    {
        Ok(D3d11ShaderResourceViewDesc1 {
            format: 0,
            view_dimension: 1,
            u: D3d11ShaderResourceViewDesc1Union {
                buffer: D3d11BufferSrv {
                    first_element: 0,
                    num_elements: desc.byte_width / desc.structure_byte_stride,
                },
            },
        })
    } else {
        Err(E_FAIL)
    }
}

/// Extracts or populates a buffer UAV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Returns [`E_FAIL`] if `view_in` is `None` and the buffer is not structured.
pub const fn extract_entire_uav_buffer(
    desc: &D3d11BufferDesc,
    view_in: Option<&D3d11UnorderedAccessViewDesc1>,
) -> Result<D3d11UnorderedAccessViewDesc1, Hresult> {
    if let Some(v) = view_in {
        return Ok(*v);
    }
    if (desc.misc_flags & D3D11_RESOURCE_MISC_BUFFER_STRUCTURED) != 0
        && desc.structure_byte_stride != 0
    {
        Ok(D3d11UnorderedAccessViewDesc1 {
            format: 0,
            view_dimension: 1,
            u: D3d11UnorderedAccessViewDesc1Union {
                buffer: D3d11BufferUav {
                    first_element: 0,
                    num_elements: desc.byte_width / desc.structure_byte_stride,
                    flags: 0,
                },
            },
        })
    } else {
        Err(E_FAIL)
    }
}

/// Extracts or populates a buffer RTV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Returns [`E_FAIL`] if `view_in` is `None`.
pub const fn extract_entire_rtv_buffer(
    desc: &D3d11BufferDesc,
    view_in: Option<&D3d11RenderTargetViewDesc1>,
) -> Result<D3d11RenderTargetViewDesc1, Hresult> {
    let _ = desc;
    if let Some(v) = view_in {
        Ok(*v)
    } else {
        Err(E_FAIL)
    }
}

/// Extracts or populates a buffer DSV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Returns [`E_FAIL`] if `view_in` is `None`.
pub const fn extract_entire_dsv_buffer(
    desc: &D3d11BufferDesc,
    view_in: Option<&D3d11DepthStencilViewDesc>,
) -> Result<D3d11DepthStencilViewDesc, Hresult> {
    let _ = desc;
    if let Some(v) = view_in {
        Ok(*v)
    } else {
        Err(E_FAIL)
    }
}

/// D3D11's shape of a texture for its whole-resource views.
///
/// An array unless it has exactly one slice, multisampled unless it has exactly one sample.
const fn shape(
    dimension: Dimension,
    mip_levels: u32,
    array_size: u32,
    sample_count: u32,
    misc_flags: u32,
) -> ResourceShape {
    ResourceShape {
        dimension,
        mip_levels,
        array_size,
        is_array: array_size != 1,
        multisampled: sample_count != 1,
        cube: misc_flags & D3D11_RESOURCE_MISC_TEXTURECUBE != 0,
    }
}

const fn shape_1d(desc: &D3d11Texture1dDesc) -> ResourceShape {
    shape(
        Dimension::Texture1D,
        desc.mip_levels,
        desc.array_size,
        1,
        desc.misc_flags,
    )
}

const fn shape_2d(desc: &D3d11Texture2dDesc1) -> ResourceShape {
    let samples = desc.sample_desc.count;
    shape(
        Dimension::Texture2D,
        desc.mip_levels,
        desc.array_size,
        samples,
        desc.misc_flags,
    )
}

const fn shape_3d(desc: &D3d11Texture3dDesc1) -> ResourceShape {
    shape(
        Dimension::Texture3D,
        desc.mip_levels,
        desc.depth,
        1,
        desc.misc_flags,
    )
}

/// `view` as a `D3D11_SHADER_RESOURCE_VIEW_DESC1` in `format`.
const fn srv_desc(format: u32, view: &ViewDesc) -> D3d11ShaderResourceViewDesc1 {
    let (mip, mips) = (view.first_mip, view.mip_count);
    let (first, size) = (view.first_slice, view.array_size);
    let (view_dimension, u) = match view.dimension {
        ViewDimension::Texture1D => (
            2,
            D3d11ShaderResourceViewDesc1Union {
                texture1d: D3d11Tex1dSrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                },
            },
        ),
        ViewDimension::Texture1DArray => (
            3,
            D3d11ShaderResourceViewDesc1Union {
                texture1d_array: D3d11Tex1dArraySrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    first_array_slice: first,
                    array_size: size,
                },
            },
        ),
        ViewDimension::Texture2D => (
            4,
            D3d11ShaderResourceViewDesc1Union {
                texture2d: D3d11Tex2dSrv1 {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    plane_slice: 0,
                },
            },
        ),
        ViewDimension::Texture2DArray => (
            5,
            D3d11ShaderResourceViewDesc1Union {
                texture2d_array: D3d11Tex2dArraySrv1 {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    first_array_slice: first,
                    array_size: size,
                    plane_slice: 0,
                },
            },
        ),
        ViewDimension::Texture2DMS => (
            6,
            D3d11ShaderResourceViewDesc1Union {
                texture2dms: D3d11Tex2dmsSrv {
                    unused_field_nothing_to_define: 0,
                },
            },
        ),
        ViewDimension::Texture2DMSArray => (
            7,
            D3d11ShaderResourceViewDesc1Union {
                texture2dms_array: D3d11Tex2dmsArraySrv {
                    first_array_slice: first,
                    array_size: size,
                },
            },
        ),
        ViewDimension::Texture3D => (
            8,
            D3d11ShaderResourceViewDesc1Union {
                texture3d: D3d11Tex3dSrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                },
            },
        ),
        ViewDimension::TextureCube => (
            9,
            D3d11ShaderResourceViewDesc1Union {
                texture_cube: D3d11TexcubeSrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                },
            },
        ),
        ViewDimension::TextureCubeArray => (
            10,
            D3d11ShaderResourceViewDesc1Union {
                texture_cube_array: D3d11TexcubeArraySrv {
                    most_detailed_mip: mip,
                    mip_levels: mips,
                    first2d_array_face: first,
                    num_cubes: size / 6,
                },
            },
        ),
    };
    D3d11ShaderResourceViewDesc1 {
        format,
        view_dimension,
        u,
    }
}

/// `view` as a `D3D11_UNORDERED_ACCESS_VIEW_DESC1` in `format`; none for a dimension it lacks.
const fn uav_desc(format: u32, view: &ViewDesc) -> Option<D3d11UnorderedAccessViewDesc1> {
    let (mip, first, size) = (view.first_mip, view.first_slice, view.array_size);
    let (view_dimension, u) = match view.dimension {
        ViewDimension::Texture1D => (
            2,
            D3d11UnorderedAccessViewDesc1Union {
                texture1d: D3d11Tex1dUav { mip_slice: mip },
            },
        ),
        ViewDimension::Texture1DArray => (
            3,
            D3d11UnorderedAccessViewDesc1Union {
                texture1d_array: D3d11Tex1dArrayUav {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                },
            },
        ),
        ViewDimension::Texture2D => (
            4,
            D3d11UnorderedAccessViewDesc1Union {
                texture2d: D3d11Tex2dUav1 {
                    mip_slice: mip,
                    plane_slice: 0,
                },
            },
        ),
        ViewDimension::Texture2DArray => (
            5,
            D3d11UnorderedAccessViewDesc1Union {
                texture2d_array: D3d11Tex2dArrayUav1 {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                    plane_slice: 0,
                },
            },
        ),
        ViewDimension::Texture3D => (
            8,
            D3d11UnorderedAccessViewDesc1Union {
                texture3d: D3d11Tex3dUav {
                    mip_slice: mip,
                    first_w_slice: first,
                    w_size: size,
                },
            },
        ),
        _ => return None,
    };
    Some(D3d11UnorderedAccessViewDesc1 {
        format,
        view_dimension,
        u,
    })
}

/// `view` as a `D3D11_RENDER_TARGET_VIEW_DESC1` in `format`; none for a dimension it lacks.
const fn rtv_desc(format: u32, view: &ViewDesc) -> Option<D3d11RenderTargetViewDesc1> {
    let (mip, first, size) = (view.first_mip, view.first_slice, view.array_size);
    let (view_dimension, u) = match view.dimension {
        ViewDimension::Texture1D => (
            2,
            D3d11RenderTargetViewDesc1Union {
                texture1d: D3d11Tex1dRtv { mip_slice: mip },
            },
        ),
        ViewDimension::Texture1DArray => (
            3,
            D3d11RenderTargetViewDesc1Union {
                texture1d_array: D3d11Tex1dArrayRtv {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                },
            },
        ),
        ViewDimension::Texture2D => (
            4,
            D3d11RenderTargetViewDesc1Union {
                texture2d: D3d11Tex2dRtv1 {
                    mip_slice: mip,
                    plane_slice: 0,
                },
            },
        ),
        ViewDimension::Texture2DArray => (
            5,
            D3d11RenderTargetViewDesc1Union {
                texture2d_array: D3d11Tex2dArrayRtv1 {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                    plane_slice: 0,
                },
            },
        ),
        ViewDimension::Texture2DMS => (
            6,
            D3d11RenderTargetViewDesc1Union {
                texture2dms: D3d11Tex2dmsRtv {
                    unused_field_nothing_to_define: 0,
                },
            },
        ),
        ViewDimension::Texture2DMSArray => (
            7,
            D3d11RenderTargetViewDesc1Union {
                texture2dms_array: D3d11Tex2dmsArrayRtv {
                    first_array_slice: first,
                    array_size: size,
                },
            },
        ),
        ViewDimension::Texture3D => (
            8,
            D3d11RenderTargetViewDesc1Union {
                texture3d: D3d11Tex3dRtv {
                    mip_slice: mip,
                    first_w_slice: first,
                    w_size: size,
                },
            },
        ),
        _ => return None,
    };
    Some(D3d11RenderTargetViewDesc1 {
        format,
        view_dimension,
        u,
    })
}

/// `view` as a `D3D11_DEPTH_STENCIL_VIEW_DESC` in `format`; none for a dimension it lacks.
const fn dsv_desc(format: u32, view: &ViewDesc) -> Option<D3d11DepthStencilViewDesc> {
    let (mip, first, size) = (view.first_mip, view.first_slice, view.array_size);
    let (view_dimension, u) = match view.dimension {
        ViewDimension::Texture1D => (
            1,
            D3d11DepthStencilViewDescUnion {
                texture1d: D3d11Tex1dDsv { mip_slice: mip },
            },
        ),
        ViewDimension::Texture1DArray => (
            2,
            D3d11DepthStencilViewDescUnion {
                texture1d_array: D3d11Tex1dArrayDsv {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                },
            },
        ),
        ViewDimension::Texture2D => (
            3,
            D3d11DepthStencilViewDescUnion {
                texture2d: D3d11Tex2dDsv { mip_slice: mip },
            },
        ),
        ViewDimension::Texture2DArray => (
            4,
            D3d11DepthStencilViewDescUnion {
                texture2d_array: D3d11Tex2dArrayDsv {
                    mip_slice: mip,
                    first_array_slice: first,
                    array_size: size,
                },
            },
        ),
        ViewDimension::Texture2DMS => (
            5,
            D3d11DepthStencilViewDescUnion {
                texture2dms: D3d11Tex2dmsDsv {
                    unused_field_nothing_to_define: 0,
                },
            },
        ),
        ViewDimension::Texture2DMSArray => (
            6,
            D3d11DepthStencilViewDescUnion {
                texture2dms_array: D3d11Tex2dmsArrayDsv {
                    first_array_slice: first,
                    array_size: size,
                },
            },
        ),
        _ => return None,
    };
    Some(D3d11DepthStencilViewDesc {
        format,
        view_dimension,
        flags: 0,
        u,
    })
}

/// The application's SRV description with its format filled in, or the whole resource's.
///
/// `ExtractEntireResourceViewDescription`, as are the other `extract_entire_*`.
fn entire_srv(
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
    format: u32,
    resource: &ResourceShape,
) -> Result<D3d11ShaderResourceViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = format;
        }
        return Ok(out);
    }
    entire_view(ViewKind::ShaderResource, resource)
        .map(|v| srv_desc(format, &v))
        .ok_or(E_FAIL)
}

/// [`entire_srv`] for a UAV: none for a multisampled texture.
fn entire_uav(
    view_in: Option<&D3d11UnorderedAccessViewDesc1>,
    format: u32,
    resource: &ResourceShape,
) -> Result<D3d11UnorderedAccessViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = format;
        }
        return Ok(out);
    }
    entire_view(ViewKind::UnorderedAccess, resource)
        .and_then(|v| uav_desc(format, &v))
        .ok_or(E_FAIL)
}

/// [`entire_srv`] for an RTV.
fn entire_rtv(
    view_in: Option<&D3d11RenderTargetViewDesc1>,
    format: u32,
    resource: &ResourceShape,
) -> Result<D3d11RenderTargetViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = format;
        }
        return Ok(out);
    }
    entire_view(ViewKind::RenderTarget, resource)
        .and_then(|v| rtv_desc(format, &v))
        .ok_or(E_FAIL)
}

/// [`entire_srv`] for a DSV: none for a 3D texture.
fn entire_dsv(
    view_in: Option<&D3d11DepthStencilViewDesc>,
    format: u32,
    resource: &ResourceShape,
) -> Result<D3d11DepthStencilViewDesc, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = format;
        }
        return Ok(out);
    }
    entire_view(ViewKind::DepthStencil, resource)
        .and_then(|v| dsv_desc(format, &v))
        .ok_or(E_FAIL)
}

/// The 1D texture SRV `view_in` describes, or the whole texture's.
///
/// # Errors
///
/// Infallible for 1D textures; returns [`Result`] for symmetry with the other view helpers.
pub fn extract_entire_srv_1d(
    desc: &D3d11Texture1dDesc,
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
) -> Result<D3d11ShaderResourceViewDesc1, Hresult> {
    entire_srv(view_in, desc.format, &shape_1d(desc))
}

/// The 1D texture UAV `view_in` describes, or the whole texture's.
///
/// # Errors
///
/// Infallible for 1D textures; returns [`Result`] for symmetry with the other view helpers.
pub fn extract_entire_uav_1d(
    desc: &D3d11Texture1dDesc,
    view_in: Option<&D3d11UnorderedAccessViewDesc1>,
) -> Result<D3d11UnorderedAccessViewDesc1, Hresult> {
    entire_uav(view_in, desc.format, &shape_1d(desc))
}

/// The 1D texture RTV `view_in` describes, or the whole texture's.
///
/// # Errors
///
/// Infallible for 1D textures; returns [`Result`] for symmetry with the other view helpers.
pub fn extract_entire_rtv_1d(
    desc: &D3d11Texture1dDesc,
    view_in: Option<&D3d11RenderTargetViewDesc1>,
) -> Result<D3d11RenderTargetViewDesc1, Hresult> {
    entire_rtv(view_in, desc.format, &shape_1d(desc))
}

/// The 1D texture DSV `view_in` describes, or the whole texture's.
///
/// # Errors
///
/// Infallible for 1D textures; returns [`Result`] for symmetry with the other view helpers.
pub fn extract_entire_dsv_1d(
    desc: &D3d11Texture1dDesc,
    view_in: Option<&D3d11DepthStencilViewDesc>,
) -> Result<D3d11DepthStencilViewDesc, Hresult> {
    entire_dsv(view_in, desc.format, &shape_1d(desc))
}

/// The 2D texture SRV `view_in` describes, or the whole texture's (a cube's as a cube).
///
/// # Errors
///
/// Infallible for 2D textures; returns [`Result`] for symmetry with the other view helpers.
pub fn extract_entire_srv_2d(
    desc: &D3d11Texture2dDesc1,
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
) -> Result<D3d11ShaderResourceViewDesc1, Hresult> {
    entire_srv(view_in, desc.format, &shape_2d(desc))
}

/// The 2D texture UAV `view_in` describes, or the whole texture's.
///
/// # Errors
///
/// Returns [`E_FAIL`] if `view_in` is `None` and the texture is multisampled.
pub fn extract_entire_uav_2d(
    desc: &D3d11Texture2dDesc1,
    view_in: Option<&D3d11UnorderedAccessViewDesc1>,
) -> Result<D3d11UnorderedAccessViewDesc1, Hresult> {
    entire_uav(view_in, desc.format, &shape_2d(desc))
}

/// The 2D texture RTV `view_in` describes, or the whole texture's.
///
/// # Errors
///
/// Infallible for 2D textures; returns [`Result`] for symmetry with the other view helpers.
pub fn extract_entire_rtv_2d(
    desc: &D3d11Texture2dDesc1,
    view_in: Option<&D3d11RenderTargetViewDesc1>,
) -> Result<D3d11RenderTargetViewDesc1, Hresult> {
    entire_rtv(view_in, desc.format, &shape_2d(desc))
}

/// The 2D texture DSV `view_in` describes, or the whole texture's.
///
/// # Errors
///
/// Infallible for 2D textures; returns [`Result`] for symmetry with the other view helpers.
pub fn extract_entire_dsv_2d(
    desc: &D3d11Texture2dDesc1,
    view_in: Option<&D3d11DepthStencilViewDesc>,
) -> Result<D3d11DepthStencilViewDesc, Hresult> {
    entire_dsv(view_in, desc.format, &shape_2d(desc))
}

/// The 3D texture SRV `view_in` describes, or the whole texture's.
///
/// # Errors
///
/// Infallible for 3D textures; returns [`Result`] for symmetry with the other view helpers.
pub fn extract_entire_srv_3d(
    desc: &D3d11Texture3dDesc1,
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
) -> Result<D3d11ShaderResourceViewDesc1, Hresult> {
    entire_srv(view_in, desc.format, &shape_3d(desc))
}

/// The 3D texture UAV `view_in` describes, or the whole texture's (every W slice).
///
/// # Errors
///
/// Infallible for 3D textures; returns [`Result`] for symmetry with the other view helpers.
pub fn extract_entire_uav_3d(
    desc: &D3d11Texture3dDesc1,
    view_in: Option<&D3d11UnorderedAccessViewDesc1>,
) -> Result<D3d11UnorderedAccessViewDesc1, Hresult> {
    entire_uav(view_in, desc.format, &shape_3d(desc))
}

/// The 3D texture RTV `view_in` describes, or the whole texture's (every W slice).
///
/// # Errors
///
/// Infallible for 3D textures; returns [`Result`] for symmetry with the other view helpers.
pub fn extract_entire_rtv_3d(
    desc: &D3d11Texture3dDesc1,
    view_in: Option<&D3d11RenderTargetViewDesc1>,
) -> Result<D3d11RenderTargetViewDesc1, Hresult> {
    entire_rtv(view_in, desc.format, &shape_3d(desc))
}

/// The 3D texture DSV `view_in` describes.
///
/// # Errors
///
/// Returns [`E_FAIL`] if `view_in` is `None`.
pub fn extract_entire_dsv_3d(
    desc: &D3d11Texture3dDesc1,
    view_in: Option<&D3d11DepthStencilViewDesc>,
) -> Result<D3d11DepthStencilViewDesc, Hresult> {
    entire_dsv(view_in, desc.format, &shape_3d(desc))
}

/// Computes `(offset, size)` in bytes for a buffer view (`CalculateBufferViewOffsetAndSize`).
#[must_use]
pub const fn calculate_buffer_view_offset_and_size(
    buffer_desc: &D3d11BufferDesc,
    element_stride: u32,
    first_element: u32,
    num_elements: u32,
) -> (u32, u32) {
    let offset = first_element.wrapping_mul(element_stride);
    let rem = buffer_desc.byte_width.saturating_sub(offset);
    let req = element_stride.wrapping_mul(num_elements);
    let size = if rem < req { rem } else { req };
    (offset, size)
}

/// Returns the CPU-placement flag for 32-bit vs 64-bit targets (`__i386__`).
#[must_use]
pub const fn cpu_placement_flag(is_32bit: bool) -> BufferAllocationFlags {
    if is_32bit {
        BufferAllocationFlags::CPU_PLACED
    } else {
        BufferAllocationFlags::empty()
    }
}

#[cfg(test)]
mod tests;
