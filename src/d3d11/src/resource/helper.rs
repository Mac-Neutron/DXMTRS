//! Resource description validation and upgrade helpers (`d3d11_resource_helper.cpp`).

use dxmt_core::{
    format::{
        MTL_DXGI_FORMAT_BC, MTL_DXGI_FORMAT_DEPTH_PLANER, MTL_DXGI_FORMAT_STENCIL_PLANER,
        MTL_DXGI_FORMAT_TYPELESS, query_dxgi,
    },
    resource::BufferAllocationFlags,
};
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_DEPTH16_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_PIXEL_FORMAT_INVALID,
    WMT_PIXEL_FORMAT_R32_SINT, WMT_PIXEL_FORMAT_R32_UINT, WMT_PIXEL_FORMAT_RG32_UINT,
    WMT_RESOURCE_CPU_CACHE_MODE_WRITE_COMBINED, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
    WMT_RESOURCE_STORAGE_MODE_MANAGED, WMT_RESOURCE_STORAGE_MODE_SHARED, WMT_TEXTURE_TYPE_CUBE,
    WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D,
    WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW, WMT_TEXTURE_USAGE_RENDER_TARGET,
    WMT_TEXTURE_USAGE_SHADER_ATOMIC, WMT_TEXTURE_USAGE_SHADER_READ, WMT_TEXTURE_USAGE_SHADER_WRITE,
    WMTGPU_FAMILY_APPLE8, WMTTextureInfo, original_format,
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

const D3D11_USAGE_DEFAULT: u32 = 0;
const D3D11_USAGE_IMMUTABLE: u32 = 1;
const D3D11_USAGE_DYNAMIC: u32 = 2;
const D3D11_USAGE_STAGING: u32 = 3;

const D3D11_BIND_VERTEX_BUFFER: u32 = 0x1;
const D3D11_BIND_INDEX_BUFFER: u32 = 0x2;
const D3D11_BIND_CONSTANT_BUFFER: u32 = 0x4;
const D3D11_BIND_STREAM_OUTPUT: u32 = 0x10;
const D3D11_BIND_RENDER_TARGET: u32 = 0x20;
const D3D11_BIND_DEPTH_STENCIL: u32 = 0x40;
const D3D11_BIND_UNORDERED_ACCESS: u32 = 0x80;

const D3D11_RESOURCE_MISC_SHARED: u32 = 0x2;
const D3D11_RESOURCE_MISC_TEXTURECUBE: u32 = 0x4;
const D3D11_RESOURCE_MISC_BUFFER_STRUCTURED: u32 = 0x40;
const D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX: u32 = 0x100;
const D3D11_RESOURCE_MISC_TILE_POOL: u32 = 0x20000;

const DXGI_FORMAT_R32_TYPELESS: u32 = 39;
const DXGI_FORMAT_D32_FLOAT: u32 = 40;
const DXGI_FORMAT_R16_TYPELESS: u32 = 53;
const DXGI_FORMAT_D16_UNORM: u32 = 55;

#[derive(PartialEq, Eq)]
enum TexDim {
    Tex1d,
    Tex2d,
    Tex3d,
}

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
    dim: TexDim,
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
    p: MtlTexDescParams,
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
    } = p;
    if width == 0 || height == 0 || depth == 0 || array_size == 0 || sample_count == 0 {
        return E_INVALIDARG;
    }

    let (hr, mut metal_format) = query_dxgi(format);
    if hr < 0 {
        return E_FAIL;
    }

    let shared_flag = D3D11_RESOURCE_MISC_SHARED | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX;
    if (misc_flags & shared_flag) == shared_flag {
        return E_INVALIDARG;
    }

    if (bind_flags & D3D11_BIND_DEPTH_STENCIL) != 0 {
        match format {
            DXGI_FORMAT_R32_TYPELESS | DXGI_FORMAT_D32_FLOAT => {
                metal_format.pixel_format = WMT_PIXEL_FORMAT_DEPTH32_FLOAT;
            }
            DXGI_FORMAT_R16_TYPELESS | DXGI_FORMAT_D16_UNORM => {
                metal_format.pixel_format = WMT_PIXEL_FORMAT_DEPTH16_UNORM;
            }
            _ => {}
        }
    }
    mtl_desc.pixel_format = original_format(metal_format.pixel_format);

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
        if metal_format.pixel_format == WMT_PIXEL_FORMAT_R32_UINT
            || metal_format.pixel_format == WMT_PIXEL_FORMAT_R32_SINT
            || (metal_format.pixel_format == WMT_PIXEL_FORMAT_RG32_UINT
                && supports_family(WMTGPU_FAMILY_APPLE8))
        {
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
        32 - (width | height).leading_zeros()
    } else {
        mip_levels
    };

    let mut out_sample_count = 1u32;
    let mut out_array_length = 1u32;
    let tex_type = match dim {
        TexDim::Tex1d => {
            if array_size > 1 {
                out_array_length = array_size;
                WMT_TEXTURE_TYPE2D_ARRAY
            } else {
                WMT_TEXTURE_TYPE2D
            }
        }
        TexDim::Tex2d => {
            if (misc_flags & D3D11_RESOURCE_MISC_TEXTURECUBE) != 0 {
                if (array_size / 6) > 1 {
                    out_array_length = array_size / 6;
                    WMT_TEXTURE_TYPE_CUBE_ARRAY
                } else {
                    WMT_TEXTURE_TYPE_CUBE
                }
            } else if sample_count > 1 {
                if !matches!(sample_count, 1 | 2 | 4 | 8) {
                    return E_INVALIDARG;
                }
                out_sample_count = sample_count;
                if array_size > 1 {
                    out_array_length = array_size;
                    WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY
                } else {
                    WMT_TEXTURE_TYPE2D_MULTISAMPLE
                }
            } else if array_size > 1 {
                out_array_length = array_size;
                WMT_TEXTURE_TYPE2D_ARRAY
            } else {
                WMT_TEXTURE_TYPE2D
            }
        }
        TexDim::Tex3d => WMT_TEXTURE_TYPE3D,
    };

    mtl_desc.width = width;
    mtl_desc.height = height;
    mtl_desc.depth = depth;
    mtl_desc.array_length = out_array_length;
    mtl_desc.bits = (tex_type & 0xff)
        | ((mipmap_level_count & 0xff) << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
        | ((out_sample_count & 0xff) << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
        | ((metal_usage & 0xff) << WMTTextureInfo::USAGE_SHIFT);
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
        MtlTexDescParams {
            dim: TexDim::Tex1d,
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
        MtlTexDescParams {
            dim: TexDim::Tex2d,
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
        MtlTexDescParams {
            dim: TexDim::Tex3d,
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

const fn mip_dim(base: u32, level: u32) -> u32 {
    match base.checked_shr(level) {
        Some(shifted) if shifted > 1 => shifted,
        _ => 1,
    }
}

/// Computes `(width, height, depth)` at `level` of a 1D texture (`GetMipmapSize`).
#[must_use]
pub const fn get_mipmap_size_1d(desc: &D3d11Texture1dDesc, level: u32) -> (u32, u32, u32) {
    (mip_dim(desc.width, level), 1, 1)
}

/// Computes `(width, height, depth)` at `level` of a 2D texture (`GetMipmapSize`).
#[must_use]
pub const fn get_mipmap_size_2d(desc: &D3d11Texture2dDesc1, level: u32) -> (u32, u32, u32) {
    (mip_dim(desc.width, level), mip_dim(desc.height, level), 1)
}

/// Computes `(width, height, depth)` at `level` of a 3D texture (`GetMipmapSize`).
#[must_use]
pub const fn get_mipmap_size_3d(desc: &D3d11Texture3dDesc1, level: u32) -> (u32, u32, u32) {
    (
        mip_dim(desc.width, level),
        mip_dim(desc.height, level),
        mip_dim(desc.depth, level),
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
    if hr < 0
        || metal_format.pixel_format == WMT_PIXEL_FORMAT_INVALID
        || metal_format.bytes_per_texel == 0
    {
        return Err(E_FAIL);
    }
    let (w, _, _) = get_mipmap_size_1d(desc, level);
    let bytes_per_row_unaligned = metal_format.bytes_per_texel * w;
    let alignment = if aligned
        && (metal_format.flag & (MTL_DXGI_FORMAT_DEPTH_PLANER | MTL_DXGI_FORMAT_STENCIL_PLANER))
            == 0
    {
        16
    } else {
        metal_format.bytes_per_texel.max(1)
    };
    let aligned_bytes_per_row = bytes_per_row_unaligned.div_ceil(alignment) * alignment;
    Ok((
        aligned_bytes_per_row,
        aligned_bytes_per_row,
        aligned_bytes_per_row,
    ))
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
    let (hr, metal_format) = query_dxgi(desc.format);
    if hr < 0
        || metal_format.pixel_format == WMT_PIXEL_FORMAT_INVALID
        || metal_format.bytes_per_texel == 0
    {
        return Err(E_FAIL);
    }
    let (w, h, _) = get_mipmap_size_2d(desc, level);
    if (metal_format.flag & MTL_DXGI_FORMAT_BC) != 0 {
        let w_physical = w.div_ceil(4) * 4;
        let h_physical = h.div_ceil(4) * 4;
        let aligned_bytes_per_row = (metal_format.bytes_per_texel * w_physical) >> 2;
        let slice = (aligned_bytes_per_row * h_physical) >> 2;
        return Ok((aligned_bytes_per_row, slice, slice));
    }
    let bytes_per_row_unaligned = metal_format.bytes_per_texel * w;
    let alignment = if aligned
        && (metal_format.flag & (MTL_DXGI_FORMAT_DEPTH_PLANER | MTL_DXGI_FORMAT_STENCIL_PLANER))
            == 0
    {
        16
    } else {
        metal_format.bytes_per_texel.max(1)
    };
    let aligned_bytes_per_row = bytes_per_row_unaligned.div_ceil(alignment) * alignment;
    let slice = aligned_bytes_per_row * h;
    Ok((aligned_bytes_per_row, slice, slice))
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
    let (hr, metal_format) = query_dxgi(desc.format);
    if hr < 0
        || metal_format.pixel_format == WMT_PIXEL_FORMAT_INVALID
        || metal_format.bytes_per_texel == 0
    {
        return Err(E_FAIL);
    }
    let (w, h, d) = get_mipmap_size_3d(desc, level);
    if (metal_format.flag & MTL_DXGI_FORMAT_BC) != 0 {
        let w_physical = w.div_ceil(4) * 4;
        let h_physical = h.div_ceil(4) * 4;
        let aligned_bytes_per_row = (metal_format.bytes_per_texel * w_physical) >> 2;
        let image = (aligned_bytes_per_row * h_physical) >> 2;
        let slice = (aligned_bytes_per_row * h_physical * d) >> 2;
        return Ok((aligned_bytes_per_row, image, slice));
    }
    let bytes_per_row_unaligned = metal_format.bytes_per_texel * w;
    let alignment = if aligned
        && (metal_format.flag & (MTL_DXGI_FORMAT_DEPTH_PLANER | MTL_DXGI_FORMAT_STENCIL_PLANER))
            == 0
    {
        16
    } else {
        metal_format.bytes_per_texel.max(1)
    };
    let aligned_bytes_per_row = bytes_per_row_unaligned.div_ceil(alignment) * alignment;
    let image = aligned_bytes_per_row * h;
    let slice = image * d;
    Ok((aligned_bytes_per_row, image, slice))
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

/// Extracts or populates a 1D texture SRV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Infallible for 1D textures; returns [`Result`] for symmetry with the other view helpers.
pub const fn extract_entire_srv_1d(
    desc: &D3d11Texture1dDesc,
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
) -> Result<D3d11ShaderResourceViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    if desc.array_size == 1 {
        Ok(D3d11ShaderResourceViewDesc1 {
            format: desc.format,
            view_dimension: 2,
            u: D3d11ShaderResourceViewDesc1Union {
                texture1d: D3d11Tex1dSrv {
                    most_detailed_mip: 0,
                    mip_levels: desc.mip_levels,
                },
            },
        })
    } else {
        Ok(D3d11ShaderResourceViewDesc1 {
            format: desc.format,
            view_dimension: 3,
            u: D3d11ShaderResourceViewDesc1Union {
                texture1d_array: D3d11Tex1dArraySrv {
                    most_detailed_mip: 0,
                    mip_levels: desc.mip_levels,
                    first_array_slice: 0,
                    array_size: desc.array_size,
                },
            },
        })
    }
}

/// Extracts or populates a 1D texture UAV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Infallible for 1D textures; returns [`Result`] for symmetry with the other view helpers.
pub const fn extract_entire_uav_1d(
    desc: &D3d11Texture1dDesc,
    view_in: Option<&D3d11UnorderedAccessViewDesc1>,
) -> Result<D3d11UnorderedAccessViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    if desc.array_size == 1 {
        Ok(D3d11UnorderedAccessViewDesc1 {
            format: desc.format,
            view_dimension: 2,
            u: D3d11UnorderedAccessViewDesc1Union {
                texture1d: D3d11Tex1dUav { mip_slice: 0 },
            },
        })
    } else {
        Ok(D3d11UnorderedAccessViewDesc1 {
            format: desc.format,
            view_dimension: 3,
            u: D3d11UnorderedAccessViewDesc1Union {
                texture1d_array: D3d11Tex1dArrayUav {
                    mip_slice: 0,
                    first_array_slice: 0,
                    array_size: desc.array_size,
                },
            },
        })
    }
}

/// Extracts or populates a 1D texture RTV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Infallible for 1D textures; returns [`Result`] for symmetry with the other view helpers.
pub const fn extract_entire_rtv_1d(
    desc: &D3d11Texture1dDesc,
    view_in: Option<&D3d11RenderTargetViewDesc1>,
) -> Result<D3d11RenderTargetViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    if desc.array_size == 1 {
        Ok(D3d11RenderTargetViewDesc1 {
            format: desc.format,
            view_dimension: 2,
            u: D3d11RenderTargetViewDesc1Union {
                texture1d: D3d11Tex1dRtv { mip_slice: 0 },
            },
        })
    } else {
        Ok(D3d11RenderTargetViewDesc1 {
            format: desc.format,
            view_dimension: 3,
            u: D3d11RenderTargetViewDesc1Union {
                texture1d_array: D3d11Tex1dArrayRtv {
                    mip_slice: 0,
                    first_array_slice: 0,
                    array_size: desc.array_size,
                },
            },
        })
    }
}

/// Extracts or populates a 1D texture DSV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Infallible for 1D textures; returns [`Result`] for symmetry with the other view helpers.
pub const fn extract_entire_dsv_1d(
    desc: &D3d11Texture1dDesc,
    view_in: Option<&D3d11DepthStencilViewDesc>,
) -> Result<D3d11DepthStencilViewDesc, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    if desc.array_size == 1 {
        Ok(D3d11DepthStencilViewDesc {
            format: desc.format,
            view_dimension: 1,
            flags: 0,
            u: D3d11DepthStencilViewDescUnion {
                texture1d: D3d11Tex1dDsv { mip_slice: 0 },
            },
        })
    } else {
        Ok(D3d11DepthStencilViewDesc {
            format: desc.format,
            view_dimension: 2,
            flags: 0,
            u: D3d11DepthStencilViewDescUnion {
                texture1d_array: D3d11Tex1dArrayDsv {
                    mip_slice: 0,
                    first_array_slice: 0,
                    array_size: desc.array_size,
                },
            },
        })
    }
}

/// Extracts or populates a 2D texture SRV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Infallible for 2D textures; returns [`Result`] for symmetry with the other view helpers.
pub const fn extract_entire_srv_2d(
    desc: &D3d11Texture2dDesc1,
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
) -> Result<D3d11ShaderResourceViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    if desc.sample_desc.count == 1 {
        if desc.array_size == 1 {
            Ok(D3d11ShaderResourceViewDesc1 {
                format: desc.format,
                view_dimension: 4,
                u: D3d11ShaderResourceViewDesc1Union {
                    texture2d: D3d11Tex2dSrv1 {
                        most_detailed_mip: 0,
                        mip_levels: desc.mip_levels,
                        plane_slice: 0,
                    },
                },
            })
        } else if (desc.misc_flags & D3D11_RESOURCE_MISC_TEXTURECUBE) != 0 {
            let num_cubes = desc.array_size / 6;
            if num_cubes > 1 {
                Ok(D3d11ShaderResourceViewDesc1 {
                    format: desc.format,
                    view_dimension: 10,
                    u: D3d11ShaderResourceViewDesc1Union {
                        texture_cube_array: D3d11TexcubeArraySrv {
                            most_detailed_mip: 0,
                            mip_levels: desc.mip_levels,
                            first2d_array_face: 0,
                            num_cubes,
                        },
                    },
                })
            } else {
                Ok(D3d11ShaderResourceViewDesc1 {
                    format: desc.format,
                    view_dimension: 9,
                    u: D3d11ShaderResourceViewDesc1Union {
                        texture_cube: D3d11TexcubeSrv {
                            most_detailed_mip: 0,
                            mip_levels: desc.mip_levels,
                        },
                    },
                })
            }
        } else {
            Ok(D3d11ShaderResourceViewDesc1 {
                format: desc.format,
                view_dimension: 5,
                u: D3d11ShaderResourceViewDesc1Union {
                    texture2d_array: D3d11Tex2dArraySrv1 {
                        most_detailed_mip: 0,
                        mip_levels: desc.mip_levels,
                        first_array_slice: 0,
                        array_size: desc.array_size,
                        plane_slice: 0,
                    },
                },
            })
        }
    } else if desc.array_size == 1 {
        Ok(D3d11ShaderResourceViewDesc1 {
            format: desc.format,
            view_dimension: 6,
            u: D3d11ShaderResourceViewDesc1Union {
                texture2dms: D3d11Tex2dmsSrv {
                    unused_field_nothing_to_define: 0,
                },
            },
        })
    } else {
        Ok(D3d11ShaderResourceViewDesc1 {
            format: desc.format,
            view_dimension: 7,
            u: D3d11ShaderResourceViewDesc1Union {
                texture2dms_array: D3d11Tex2dmsArraySrv {
                    first_array_slice: 0,
                    array_size: desc.array_size,
                },
            },
        })
    }
}

/// Extracts or populates a 2D texture UAV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Returns [`E_FAIL`] if `view_in` is `None` and the texture is multisampled.
pub const fn extract_entire_uav_2d(
    desc: &D3d11Texture2dDesc1,
    view_in: Option<&D3d11UnorderedAccessViewDesc1>,
) -> Result<D3d11UnorderedAccessViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    if desc.sample_desc.count == 1 {
        if desc.array_size == 1 {
            Ok(D3d11UnorderedAccessViewDesc1 {
                format: desc.format,
                view_dimension: 4,
                u: D3d11UnorderedAccessViewDesc1Union {
                    texture2d: D3d11Tex2dUav1 {
                        mip_slice: 0,
                        plane_slice: 0,
                    },
                },
            })
        } else {
            Ok(D3d11UnorderedAccessViewDesc1 {
                format: desc.format,
                view_dimension: 5,
                u: D3d11UnorderedAccessViewDesc1Union {
                    texture2d_array: D3d11Tex2dArrayUav1 {
                        mip_slice: 0,
                        first_array_slice: 0,
                        array_size: desc.array_size,
                        plane_slice: 0,
                    },
                },
            })
        }
    } else {
        Err(E_FAIL)
    }
}

/// Extracts or populates a 2D texture RTV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Infallible for 2D textures; returns [`Result`] for symmetry with the other view helpers.
pub const fn extract_entire_rtv_2d(
    desc: &D3d11Texture2dDesc1,
    view_in: Option<&D3d11RenderTargetViewDesc1>,
) -> Result<D3d11RenderTargetViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    if desc.sample_desc.count == 1 {
        if desc.array_size == 1 {
            Ok(D3d11RenderTargetViewDesc1 {
                format: desc.format,
                view_dimension: 4,
                u: D3d11RenderTargetViewDesc1Union {
                    texture2d: D3d11Tex2dRtv1 {
                        mip_slice: 0,
                        plane_slice: 0,
                    },
                },
            })
        } else {
            Ok(D3d11RenderTargetViewDesc1 {
                format: desc.format,
                view_dimension: 5,
                u: D3d11RenderTargetViewDesc1Union {
                    texture2d_array: D3d11Tex2dArrayRtv1 {
                        mip_slice: 0,
                        first_array_slice: 0,
                        array_size: desc.array_size,
                        plane_slice: 0,
                    },
                },
            })
        }
    } else if desc.array_size == 1 {
        Ok(D3d11RenderTargetViewDesc1 {
            format: desc.format,
            view_dimension: 6,
            u: D3d11RenderTargetViewDesc1Union {
                texture2dms: D3d11Tex2dmsRtv {
                    unused_field_nothing_to_define: 0,
                },
            },
        })
    } else {
        Ok(D3d11RenderTargetViewDesc1 {
            format: desc.format,
            view_dimension: 7,
            u: D3d11RenderTargetViewDesc1Union {
                texture2dms_array: D3d11Tex2dmsArrayRtv {
                    first_array_slice: 0,
                    array_size: desc.array_size,
                },
            },
        })
    }
}

/// Extracts or populates a 2D texture DSV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Infallible for 2D textures; returns [`Result`] for symmetry with the other view helpers.
pub const fn extract_entire_dsv_2d(
    desc: &D3d11Texture2dDesc1,
    view_in: Option<&D3d11DepthStencilViewDesc>,
) -> Result<D3d11DepthStencilViewDesc, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    if desc.sample_desc.count == 1 {
        if desc.array_size == 1 {
            Ok(D3d11DepthStencilViewDesc {
                format: desc.format,
                view_dimension: 3,
                flags: 0,
                u: D3d11DepthStencilViewDescUnion {
                    texture2d: D3d11Tex2dDsv { mip_slice: 0 },
                },
            })
        } else {
            Ok(D3d11DepthStencilViewDesc {
                format: desc.format,
                view_dimension: 4,
                flags: 0,
                u: D3d11DepthStencilViewDescUnion {
                    texture2d_array: D3d11Tex2dArrayDsv {
                        mip_slice: 0,
                        first_array_slice: 0,
                        array_size: desc.array_size,
                    },
                },
            })
        }
    } else if desc.array_size == 1 {
        Ok(D3d11DepthStencilViewDesc {
            format: desc.format,
            view_dimension: 5,
            flags: 0,
            u: D3d11DepthStencilViewDescUnion {
                texture2dms: D3d11Tex2dmsDsv {
                    unused_field_nothing_to_define: 0,
                },
            },
        })
    } else {
        Ok(D3d11DepthStencilViewDesc {
            format: desc.format,
            view_dimension: 6,
            flags: 0,
            u: D3d11DepthStencilViewDescUnion {
                texture2dms_array: D3d11Tex2dmsArrayDsv {
                    first_array_slice: 0,
                    array_size: desc.array_size,
                },
            },
        })
    }
}

/// Extracts or populates a 3D texture SRV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Infallible for 3D textures; returns [`Result`] for symmetry with the other view helpers.
pub const fn extract_entire_srv_3d(
    desc: &D3d11Texture3dDesc1,
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
) -> Result<D3d11ShaderResourceViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    Ok(D3d11ShaderResourceViewDesc1 {
        format: desc.format,
        view_dimension: 8,
        u: D3d11ShaderResourceViewDesc1Union {
            texture3d: D3d11Tex3dSrv {
                most_detailed_mip: 0,
                mip_levels: desc.mip_levels,
            },
        },
    })
}

/// Extracts or populates a 3D texture UAV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Infallible for 3D textures; returns [`Result`] for symmetry with the other view helpers.
pub const fn extract_entire_uav_3d(
    desc: &D3d11Texture3dDesc1,
    view_in: Option<&D3d11UnorderedAccessViewDesc1>,
) -> Result<D3d11UnorderedAccessViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    Ok(D3d11UnorderedAccessViewDesc1 {
        format: desc.format,
        view_dimension: 8,
        u: D3d11UnorderedAccessViewDesc1Union {
            texture3d: D3d11Tex3dUav {
                mip_slice: 0,
                first_w_slice: 0,
                w_size: desc.depth,
            },
        },
    })
}

/// Extracts or populates a 3D texture RTV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Infallible for 3D textures; returns [`Result`] for symmetry with the other view helpers.
pub const fn extract_entire_rtv_3d(
    desc: &D3d11Texture3dDesc1,
    view_in: Option<&D3d11RenderTargetViewDesc1>,
) -> Result<D3d11RenderTargetViewDesc1, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    Ok(D3d11RenderTargetViewDesc1 {
        format: desc.format,
        view_dimension: 8,
        u: D3d11RenderTargetViewDesc1Union {
            texture3d: D3d11Tex3dRtv {
                mip_slice: 0,
                first_w_slice: 0,
                w_size: desc.depth,
            },
        },
    })
}

/// Extracts or populates a 3D texture DSV description (`ExtractEntireResourceViewDescription`).
///
/// # Errors
///
/// Returns [`E_FAIL`] if `view_in` is `None`.
pub const fn extract_entire_dsv_3d(
    desc: &D3d11Texture3dDesc1,
    view_in: Option<&D3d11DepthStencilViewDesc>,
) -> Result<D3d11DepthStencilViewDesc, Hresult> {
    if let Some(v) = view_in {
        let mut out = *v;
        if out.format == 0 {
            out.format = desc.format;
        }
        return Ok(out);
    }
    Err(E_FAIL)
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
