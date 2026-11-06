//! View descriptor validation and default-view constructors (`d3d11_resource_view_helper.cpp`).

use dxmt_core::{format::query_dxgi_ref, resource::TextureViewDescriptor};
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_DEPTH16_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_TEXTURE_TYPE_CUBE,
    WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D,
    WMTTextureInfo,
};

use crate::{
    abi::records::{
        D3d11DepthStencilViewDesc, D3d11RenderTargetViewDesc, D3d11RenderTargetViewDesc1,
        D3d11RenderTargetViewDesc1Union, D3d11RenderTargetViewDescUnion,
        D3d11ShaderResourceViewDesc, D3d11ShaderResourceViewDesc1,
        D3d11ShaderResourceViewDesc1Union, D3d11ShaderResourceViewDescUnion, D3d11Tex2dArrayRtv,
        D3d11Tex2dArrayRtv1, D3d11Tex2dArraySrv, D3d11Tex2dArraySrv1, D3d11Tex2dArrayUav,
        D3d11Tex2dArrayUav1, D3d11Tex2dRtv, D3d11Tex2dRtv1, D3d11Tex2dSrv, D3d11Tex2dSrv1,
        D3d11Tex2dUav, D3d11Tex2dUav1, D3d11UnorderedAccessViewDesc, D3d11UnorderedAccessViewDesc1,
        D3d11UnorderedAccessViewDesc1Union, D3d11UnorderedAccessViewDescUnion,
    },
    com::{E_FAIL, Hresult},
};

/// Render-pass attachment parameters for an RTV or DSV (`MTL_RENDER_PASS_ATTACHMENT_DESC`).
#[derive(Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct RenderPassAttachmentDesc {
    pub render_target_array_length: u32,
    pub sample_count: u32,
    pub depth_plane: u32,
    pub width: u32,
    pub height: u32,
    pub slice: u16,
    pub level: u16,
}

/// Upgrades a [`D3d11ShaderResourceViewDesc`] to [`D3d11ShaderResourceViewDesc1`].
#[must_use]
pub const fn upgrade_srv_desc(src: &D3d11ShaderResourceViewDesc) -> D3d11ShaderResourceViewDesc1 {
    let u = match src.view_dimension {
        1 => D3d11ShaderResourceViewDesc1Union {
            // SAFETY: `view_dimension == 1` selects `buffer`.
            buffer: unsafe { src.u.buffer },
        },
        2 => D3d11ShaderResourceViewDesc1Union {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            texture1d: unsafe { src.u.texture1d },
        },
        3 => D3d11ShaderResourceViewDesc1Union {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            texture1d_array: unsafe { src.u.texture1d_array },
        },
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let t = unsafe { src.u.texture2d };
            D3d11ShaderResourceViewDesc1Union {
                texture2d: D3d11Tex2dSrv1 {
                    most_detailed_mip: t.most_detailed_mip,
                    mip_levels: t.mip_levels,
                    plane_slice: 0,
                },
            }
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let t = unsafe { src.u.texture2d_array };
            D3d11ShaderResourceViewDesc1Union {
                texture2d_array: D3d11Tex2dArraySrv1 {
                    most_detailed_mip: t.most_detailed_mip,
                    mip_levels: t.mip_levels,
                    first_array_slice: t.first_array_slice,
                    array_size: t.array_size,
                    plane_slice: 0,
                },
            }
        }
        6 => D3d11ShaderResourceViewDesc1Union {
            // SAFETY: `view_dimension == 6` selects `texture2dms`.
            texture2dms: unsafe { src.u.texture2dms },
        },
        7 => D3d11ShaderResourceViewDesc1Union {
            // SAFETY: `view_dimension == 7` selects `texture2dms_array`.
            texture2dms_array: unsafe { src.u.texture2dms_array },
        },
        8 => D3d11ShaderResourceViewDesc1Union {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            texture3d: unsafe { src.u.texture3d },
        },
        9 => D3d11ShaderResourceViewDesc1Union {
            // SAFETY: `view_dimension == 9` selects `texture_cube`.
            texture_cube: unsafe { src.u.texture_cube },
        },
        10 => D3d11ShaderResourceViewDesc1Union {
            // SAFETY: `view_dimension == 10` selects `texture_cube_array`.
            texture_cube_array: unsafe { src.u.texture_cube_array },
        },
        11 => D3d11ShaderResourceViewDesc1Union {
            // SAFETY: `view_dimension == 11` selects `buffer_ex`.
            buffer_ex: unsafe { src.u.buffer_ex },
        },
        _ => D3d11ShaderResourceViewDesc1Union {
            texture2d: D3d11Tex2dSrv1 {
                most_detailed_mip: 0,
                mip_levels: 0,
                plane_slice: 0,
            },
        },
    };
    D3d11ShaderResourceViewDesc1 {
        format: src.format,
        view_dimension: src.view_dimension,
        u,
    }
}

/// Downgrades a [`D3d11ShaderResourceViewDesc1`] to [`D3d11ShaderResourceViewDesc`].
#[must_use]
pub const fn downgrade_srv_desc(src: &D3d11ShaderResourceViewDesc1) -> D3d11ShaderResourceViewDesc {
    let u = match src.view_dimension {
        1 => D3d11ShaderResourceViewDescUnion {
            // SAFETY: `view_dimension == 1` selects `buffer`.
            buffer: unsafe { src.u.buffer },
        },
        2 => D3d11ShaderResourceViewDescUnion {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            texture1d: unsafe { src.u.texture1d },
        },
        3 => D3d11ShaderResourceViewDescUnion {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            texture1d_array: unsafe { src.u.texture1d_array },
        },
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let t = unsafe { src.u.texture2d };
            D3d11ShaderResourceViewDescUnion {
                texture2d: D3d11Tex2dSrv {
                    most_detailed_mip: t.most_detailed_mip,
                    mip_levels: t.mip_levels,
                },
            }
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let t = unsafe { src.u.texture2d_array };
            D3d11ShaderResourceViewDescUnion {
                texture2d_array: D3d11Tex2dArraySrv {
                    most_detailed_mip: t.most_detailed_mip,
                    mip_levels: t.mip_levels,
                    first_array_slice: t.first_array_slice,
                    array_size: t.array_size,
                },
            }
        }
        6 => D3d11ShaderResourceViewDescUnion {
            // SAFETY: `view_dimension == 6` selects `texture2dms`.
            texture2dms: unsafe { src.u.texture2dms },
        },
        7 => D3d11ShaderResourceViewDescUnion {
            // SAFETY: `view_dimension == 7` selects `texture2dms_array`.
            texture2dms_array: unsafe { src.u.texture2dms_array },
        },
        8 => D3d11ShaderResourceViewDescUnion {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            texture3d: unsafe { src.u.texture3d },
        },
        9 => D3d11ShaderResourceViewDescUnion {
            // SAFETY: `view_dimension == 9` selects `texture_cube`.
            texture_cube: unsafe { src.u.texture_cube },
        },
        10 => D3d11ShaderResourceViewDescUnion {
            // SAFETY: `view_dimension == 10` selects `texture_cube_array`.
            texture_cube_array: unsafe { src.u.texture_cube_array },
        },
        11 => D3d11ShaderResourceViewDescUnion {
            // SAFETY: `view_dimension == 11` selects `buffer_ex`.
            buffer_ex: unsafe { src.u.buffer_ex },
        },
        _ => D3d11ShaderResourceViewDescUnion {
            texture2d: D3d11Tex2dSrv {
                most_detailed_mip: 0,
                mip_levels: 0,
            },
        },
    };
    D3d11ShaderResourceViewDesc {
        format: src.format,
        view_dimension: src.view_dimension,
        u,
    }
}

/// Upgrades a [`D3d11RenderTargetViewDesc`] to [`D3d11RenderTargetViewDesc1`].
#[must_use]
pub const fn upgrade_rtv_desc(src: &D3d11RenderTargetViewDesc) -> D3d11RenderTargetViewDesc1 {
    let u = match src.view_dimension {
        1 => D3d11RenderTargetViewDesc1Union {
            // SAFETY: `view_dimension == 1` selects `buffer`.
            buffer: unsafe { src.u.buffer },
        },
        2 => D3d11RenderTargetViewDesc1Union {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            texture1d: unsafe { src.u.texture1d },
        },
        3 => D3d11RenderTargetViewDesc1Union {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            texture1d_array: unsafe { src.u.texture1d_array },
        },
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let t = unsafe { src.u.texture2d };
            D3d11RenderTargetViewDesc1Union {
                texture2d: D3d11Tex2dRtv1 {
                    mip_slice: t.mip_slice,
                    plane_slice: 0,
                },
            }
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let t = unsafe { src.u.texture2d_array };
            D3d11RenderTargetViewDesc1Union {
                texture2d_array: D3d11Tex2dArrayRtv1 {
                    mip_slice: t.mip_slice,
                    first_array_slice: t.first_array_slice,
                    array_size: t.array_size,
                    plane_slice: 0,
                },
            }
        }
        6 => D3d11RenderTargetViewDesc1Union {
            // SAFETY: `view_dimension == 6` selects `texture2dms`.
            texture2dms: unsafe { src.u.texture2dms },
        },
        7 => D3d11RenderTargetViewDesc1Union {
            // SAFETY: `view_dimension == 7` selects `texture2dms_array`.
            texture2dms_array: unsafe { src.u.texture2dms_array },
        },
        8 => D3d11RenderTargetViewDesc1Union {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            texture3d: unsafe { src.u.texture3d },
        },
        _ => D3d11RenderTargetViewDesc1Union {
            texture2d: D3d11Tex2dRtv1 {
                mip_slice: 0,
                plane_slice: 0,
            },
        },
    };
    D3d11RenderTargetViewDesc1 {
        format: src.format,
        view_dimension: src.view_dimension,
        u,
    }
}

/// Downgrades a [`D3d11RenderTargetViewDesc1`] to [`D3d11RenderTargetViewDesc`].
#[must_use]
pub const fn downgrade_rtv_desc(src: &D3d11RenderTargetViewDesc1) -> D3d11RenderTargetViewDesc {
    let u = match src.view_dimension {
        1 => D3d11RenderTargetViewDescUnion {
            // SAFETY: `view_dimension == 1` selects `buffer`.
            buffer: unsafe { src.u.buffer },
        },
        2 => D3d11RenderTargetViewDescUnion {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            texture1d: unsafe { src.u.texture1d },
        },
        3 => D3d11RenderTargetViewDescUnion {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            texture1d_array: unsafe { src.u.texture1d_array },
        },
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let t = unsafe { src.u.texture2d };
            D3d11RenderTargetViewDescUnion {
                texture2d: D3d11Tex2dRtv {
                    mip_slice: t.mip_slice,
                },
            }
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let t = unsafe { src.u.texture2d_array };
            D3d11RenderTargetViewDescUnion {
                texture2d_array: D3d11Tex2dArrayRtv {
                    mip_slice: t.mip_slice,
                    first_array_slice: t.first_array_slice,
                    array_size: t.array_size,
                },
            }
        }
        6 => D3d11RenderTargetViewDescUnion {
            // SAFETY: `view_dimension == 6` selects `texture2dms`.
            texture2dms: unsafe { src.u.texture2dms },
        },
        7 => D3d11RenderTargetViewDescUnion {
            // SAFETY: `view_dimension == 7` selects `texture2dms_array`.
            texture2dms_array: unsafe { src.u.texture2dms_array },
        },
        8 => D3d11RenderTargetViewDescUnion {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            texture3d: unsafe { src.u.texture3d },
        },
        _ => D3d11RenderTargetViewDescUnion {
            texture2d: D3d11Tex2dRtv { mip_slice: 0 },
        },
    };
    D3d11RenderTargetViewDesc {
        format: src.format,
        view_dimension: src.view_dimension,
        u,
    }
}

/// Upgrades a [`D3d11UnorderedAccessViewDesc`] to [`D3d11UnorderedAccessViewDesc1`].
#[must_use]
pub const fn upgrade_uav_desc(src: &D3d11UnorderedAccessViewDesc) -> D3d11UnorderedAccessViewDesc1 {
    let u = match src.view_dimension {
        1 => D3d11UnorderedAccessViewDesc1Union {
            // SAFETY: `view_dimension == 1` selects `buffer`.
            buffer: unsafe { src.u.buffer },
        },
        2 => D3d11UnorderedAccessViewDesc1Union {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            texture1d: unsafe { src.u.texture1d },
        },
        3 => D3d11UnorderedAccessViewDesc1Union {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            texture1d_array: unsafe { src.u.texture1d_array },
        },
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let t = unsafe { src.u.texture2d };
            D3d11UnorderedAccessViewDesc1Union {
                texture2d: D3d11Tex2dUav1 {
                    mip_slice: t.mip_slice,
                    plane_slice: 0,
                },
            }
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let t = unsafe { src.u.texture2d_array };
            D3d11UnorderedAccessViewDesc1Union {
                texture2d_array: D3d11Tex2dArrayUav1 {
                    mip_slice: t.mip_slice,
                    first_array_slice: t.first_array_slice,
                    array_size: t.array_size,
                    plane_slice: 0,
                },
            }
        }
        8 => D3d11UnorderedAccessViewDesc1Union {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            texture3d: unsafe { src.u.texture3d },
        },
        _ => D3d11UnorderedAccessViewDesc1Union {
            texture2d: D3d11Tex2dUav1 {
                mip_slice: 0,
                plane_slice: 0,
            },
        },
    };
    D3d11UnorderedAccessViewDesc1 {
        format: src.format,
        view_dimension: src.view_dimension,
        u,
    }
}

/// Downgrades a [`D3d11UnorderedAccessViewDesc1`] to [`D3d11UnorderedAccessViewDesc`].
#[must_use]
pub const fn downgrade_uav_desc(
    src: &D3d11UnorderedAccessViewDesc1,
) -> D3d11UnorderedAccessViewDesc {
    let u = match src.view_dimension {
        1 => D3d11UnorderedAccessViewDescUnion {
            // SAFETY: `view_dimension == 1` selects `buffer`.
            buffer: unsafe { src.u.buffer },
        },
        2 => D3d11UnorderedAccessViewDescUnion {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            texture1d: unsafe { src.u.texture1d },
        },
        3 => D3d11UnorderedAccessViewDescUnion {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            texture1d_array: unsafe { src.u.texture1d_array },
        },
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let t = unsafe { src.u.texture2d };
            D3d11UnorderedAccessViewDescUnion {
                texture2d: D3d11Tex2dUav {
                    mip_slice: t.mip_slice,
                },
            }
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let t = unsafe { src.u.texture2d_array };
            D3d11UnorderedAccessViewDescUnion {
                texture2d_array: D3d11Tex2dArrayUav {
                    mip_slice: t.mip_slice,
                    first_array_slice: t.first_array_slice,
                    array_size: t.array_size,
                },
            }
        }
        8 => D3d11UnorderedAccessViewDescUnion {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            texture3d: unsafe { src.u.texture3d },
        },
        _ => D3d11UnorderedAccessViewDescUnion {
            texture2d: D3d11Tex2dUav { mip_slice: 0 },
        },
    };
    D3d11UnorderedAccessViewDesc {
        format: src.format,
        view_dimension: src.view_dimension,
        u,
    }
}

const fn fix_depth_stencil_format(tex_pixel_format: u32, pixel_format: &mut u32) {
    match tex_pixel_format {
        WMT_PIXEL_FORMAT_DEPTH16_UNORM => *pixel_format = WMT_PIXEL_FORMAT_DEPTH16_UNORM,
        WMT_PIXEL_FORMAT_DEPTH32_FLOAT => *pixel_format = WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        _ => {}
    }
}

const fn set_render_target_size(
    tex_info: &WMTTextureInfo,
    level: u32,
    attach: &mut RenderPassAttachmentDesc,
) {
    let w = match tex_info.width.checked_shr(level) {
        Some(v) if v > 1 => v,
        _ => 1,
    };
    let h = match tex_info.height.checked_shr(level) {
        Some(v) if v > 1 => v,
        _ => 1,
    };
    attach.width = w;
    attach.height = h;
}

/// Validates and normalizes an SRV description (`InitializeAndNormalizeViewDescriptor`).
///
/// # Errors
///
/// Returns `E_FAIL` if the view format or dimension is incompatible with `tex_info`.
pub fn initialize_and_normalize_srv_descriptor(
    mip_levels: u32,
    array_size: u32,
    tex_info: &WMTTextureInfo,
    view_desc: &mut D3d11ShaderResourceViewDesc1,
) -> Result<TextureViewDescriptor, Hresult> {
    let (hr, metal_format) = query_dxgi_ref(view_desc.format);
    if hr < 0 {
        return Err(E_FAIL);
    }
    let mut pixel_format = metal_format.pixel_format;
    fix_depth_stencil_format(tex_info.pixel_format, &mut pixel_format);
    let texture_type = tex_info.r#type();
    match view_desc.view_dimension {
        2 => {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            let mut t = unsafe { view_desc.u.texture1d };
            if !t.mip_levels == 0 {
                t.mip_levels = mip_levels.wrapping_sub(t.most_detailed_mip);
                view_desc.u.texture1d = t;
            }
            if texture_type == WMT_TEXTURE_TYPE2D || texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D,
                    first_mip: t.most_detailed_mip,
                    mip_count: t.mip_levels,
                    first_slice: 0,
                    array_size: 1,
                });
            }
        }
        3 => {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            let mut t = unsafe { view_desc.u.texture1d_array };
            if !t.mip_levels == 0 {
                t.mip_levels = mip_levels.wrapping_sub(t.most_detailed_mip);
            }
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
            }
            view_desc.u.texture1d_array = t;
            if texture_type == WMT_TEXTURE_TYPE2D || texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                    first_mip: t.most_detailed_mip,
                    mip_count: t.mip_levels,
                    first_slice: t.first_array_slice,
                    array_size: t.array_size,
                });
            }
        }
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let mut t = unsafe { view_desc.u.texture2d };
            if !t.mip_levels == 0 {
                t.mip_levels = mip_levels.wrapping_sub(t.most_detailed_mip);
                view_desc.u.texture2d = t;
            }
            if matches!(
                texture_type,
                WMT_TEXTURE_TYPE2D
                    | WMT_TEXTURE_TYPE2D_ARRAY
                    | WMT_TEXTURE_TYPE_CUBE
                    | WMT_TEXTURE_TYPE_CUBE_ARRAY
            ) {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D,
                    first_mip: t.most_detailed_mip,
                    mip_count: t.mip_levels,
                    first_slice: 0,
                    array_size: 1,
                });
            }
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let mut t = unsafe { view_desc.u.texture2d_array };
            if !t.mip_levels == 0 {
                t.mip_levels = mip_levels.wrapping_sub(t.most_detailed_mip);
            }
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
            }
            view_desc.u.texture2d_array = t;
            if matches!(
                texture_type,
                WMT_TEXTURE_TYPE2D
                    | WMT_TEXTURE_TYPE2D_ARRAY
                    | WMT_TEXTURE_TYPE_CUBE
                    | WMT_TEXTURE_TYPE_CUBE_ARRAY
            ) {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                    first_mip: t.most_detailed_mip,
                    mip_count: t.mip_levels,
                    first_slice: t.first_array_slice,
                    array_size: t.array_size,
                });
            }
        }
        6 => {
            if texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D_MULTISAMPLE,
                    first_mip: 0,
                    mip_count: 1,
                    first_slice: 0,
                    array_size: 1,
                });
            }
            if texture_type == WMT_TEXTURE_TYPE2D {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D,
                    first_mip: 0,
                    mip_count: 1,
                    first_slice: 0,
                    array_size: 1,
                });
            }
        }
        7 => {
            // SAFETY: `view_dimension == 7` selects `texture2dms_array`.
            let mut t = unsafe { view_desc.u.texture2dms_array };
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
                view_desc.u.texture2dms_array = t;
            }
            if texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
                    first_mip: 0,
                    mip_count: 1,
                    first_slice: 0,
                    array_size: 1,
                });
            }
            if texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
                    first_mip: 0,
                    mip_count: 1,
                    first_slice: t.first_array_slice,
                    array_size: t.array_size,
                });
            }
            if texture_type == WMT_TEXTURE_TYPE2D {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                    first_mip: 0,
                    mip_count: 1,
                    first_slice: 0,
                    array_size: 1,
                });
            }
            if texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                    first_mip: 0,
                    mip_count: 1,
                    first_slice: t.first_array_slice,
                    array_size: t.array_size,
                });
            }
        }
        8 => {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            let mut t = unsafe { view_desc.u.texture3d };
            if !t.mip_levels == 0 {
                t.mip_levels = mip_levels.wrapping_sub(t.most_detailed_mip);
                view_desc.u.texture3d = t;
            }
            if texture_type == WMT_TEXTURE_TYPE3D {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE3D,
                    first_mip: t.most_detailed_mip,
                    mip_count: t.mip_levels,
                    first_slice: 0,
                    array_size: 1,
                });
            }
        }
        9 => {
            // SAFETY: `view_dimension == 9` selects `texture_cube`.
            let mut t = unsafe { view_desc.u.texture_cube };
            if !t.mip_levels == 0 {
                t.mip_levels = mip_levels.wrapping_sub(t.most_detailed_mip);
                view_desc.u.texture_cube = t;
            }
            if texture_type == WMT_TEXTURE_TYPE_CUBE || texture_type == WMT_TEXTURE_TYPE_CUBE_ARRAY
            {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE_CUBE,
                    first_mip: t.most_detailed_mip,
                    mip_count: t.mip_levels,
                    first_slice: 0,
                    array_size: 6,
                });
            }
        }
        10 => {
            // SAFETY: `view_dimension == 10` selects `texture_cube_array`.
            let mut t = unsafe { view_desc.u.texture_cube_array };
            if !t.mip_levels == 0 {
                t.mip_levels = mip_levels.wrapping_sub(t.most_detailed_mip);
            }
            if !t.num_cubes == 0 {
                t.num_cubes = array_size.wrapping_sub(t.first2d_array_face) / 6;
            }
            view_desc.u.texture_cube_array = t;
            if texture_type == WMT_TEXTURE_TYPE_CUBE || texture_type == WMT_TEXTURE_TYPE_CUBE_ARRAY
            {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE_CUBE_ARRAY,
                    first_mip: t.most_detailed_mip,
                    mip_count: t.mip_levels,
                    first_slice: t.first2d_array_face,
                    array_size: t.num_cubes * 6,
                });
            }
        }
        _ => {}
    }
    Err(E_FAIL)
}

/// Validates and normalizes a UAV description (`InitializeAndNormalizeViewDescriptor`).
///
/// # Errors
///
/// Returns `E_FAIL` if the view format or dimension is incompatible with `tex_info`.
pub fn initialize_and_normalize_uav_descriptor(
    mip_levels: u32,
    array_size: u32,
    tex_info: &WMTTextureInfo,
    view_desc: &mut D3d11UnorderedAccessViewDesc1,
) -> Result<TextureViewDescriptor, Hresult> {
    let _ = mip_levels;
    let (hr, metal_format) = query_dxgi_ref(view_desc.format);
    if hr < 0 {
        return Err(E_FAIL);
    }
    let mut pixel_format = metal_format.pixel_format;
    fix_depth_stencil_format(tex_info.pixel_format, &mut pixel_format);
    let texture_type = tex_info.r#type();
    match view_desc.view_dimension {
        2 => {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            let t = unsafe { view_desc.u.texture1d };
            if texture_type == WMT_TEXTURE_TYPE2D || texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D,
                    first_mip: t.mip_slice,
                    mip_count: 1,
                    first_slice: 0,
                    array_size: 1,
                });
            }
        }
        3 => {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            let mut t = unsafe { view_desc.u.texture1d_array };
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
                view_desc.u.texture1d_array = t;
            }
            if texture_type == WMT_TEXTURE_TYPE2D || texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                    first_mip: t.mip_slice,
                    mip_count: 1,
                    first_slice: t.first_array_slice,
                    array_size: t.array_size,
                });
            }
        }
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let t = unsafe { view_desc.u.texture2d };
            if matches!(
                texture_type,
                WMT_TEXTURE_TYPE2D | WMT_TEXTURE_TYPE2D_ARRAY | WMT_TEXTURE_TYPE_CUBE
            ) {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D,
                    first_mip: t.mip_slice,
                    mip_count: 1,
                    first_slice: 0,
                    array_size: 1,
                });
            }
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let mut t = unsafe { view_desc.u.texture2d_array };
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
                view_desc.u.texture2d_array = t;
            }
            if matches!(
                texture_type,
                WMT_TEXTURE_TYPE2D
                    | WMT_TEXTURE_TYPE2D_ARRAY
                    | WMT_TEXTURE_TYPE_CUBE
                    | WMT_TEXTURE_TYPE_CUBE_ARRAY
            ) {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                    first_mip: t.mip_slice,
                    mip_count: 1,
                    first_slice: t.first_array_slice,
                    array_size: t.array_size,
                });
            }
        }
        8 => {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            let mut t = unsafe { view_desc.u.texture3d };
            let mip_depth = array_size.checked_shr(t.mip_slice).unwrap_or(0).max(1);
            if !t.w_size == 0 {
                t.w_size = mip_depth.wrapping_sub(t.first_w_slice);
                view_desc.u.texture3d = t;
            }
            if texture_type == WMT_TEXTURE_TYPE3D && t.first_w_slice == 0 {
                return Ok(TextureViewDescriptor {
                    format: pixel_format,
                    texture_type: WMT_TEXTURE_TYPE3D,
                    first_mip: t.mip_slice,
                    mip_count: 1,
                    first_slice: 0,
                    array_size: 1,
                });
            }
        }
        _ => {}
    }
    Err(E_FAIL)
}

/// Validates and normalizes an RTV description (`InitializeAndNormalizeViewDescriptor`).
///
/// # Errors
///
/// Returns `E_FAIL` if the view format or dimension is incompatible with `tex_info`.
pub fn initialize_and_normalize_rtv_descriptor(
    mip_levels: u32,
    array_size: u32,
    tex_info: &WMTTextureInfo,
    view_desc: &mut D3d11RenderTargetViewDesc1,
) -> Result<(RenderPassAttachmentDesc, TextureViewDescriptor), Hresult> {
    let _ = mip_levels;
    let (hr, metal_format) = query_dxgi_ref(view_desc.format);
    if hr < 0 {
        return Err(E_FAIL);
    }
    let mut attach = RenderPassAttachmentDesc {
        render_target_array_length: 0,
        sample_count: tex_info.sample_count(),
        depth_plane: 0,
        width: 0,
        height: 0,
        slice: 0,
        level: 0,
    };
    let texture_type = tex_info.r#type();
    match view_desc.view_dimension {
        2 => {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            let t = unsafe { view_desc.u.texture1d };
            if texture_type == WMT_TEXTURE_TYPE2D || texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                set_render_target_size(tex_info, t.mip_slice, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D,
                        first_mip: t.mip_slice,
                        mip_count: 1,
                        first_slice: 0,
                        array_size: 1,
                    },
                ));
            }
        }
        3 => {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            let mut t = unsafe { view_desc.u.texture1d_array };
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
                view_desc.u.texture1d_array = t;
            }
            if texture_type == WMT_TEXTURE_TYPE2D || texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                attach.render_target_array_length = t.array_size;
                set_render_target_size(tex_info, t.mip_slice, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                        first_mip: t.mip_slice,
                        mip_count: 1,
                        first_slice: t.first_array_slice,
                        array_size: t.array_size,
                    },
                ));
            }
        }
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let t = unsafe { view_desc.u.texture2d };
            if matches!(
                texture_type,
                WMT_TEXTURE_TYPE2D
                    | WMT_TEXTURE_TYPE2D_ARRAY
                    | WMT_TEXTURE_TYPE_CUBE
                    | WMT_TEXTURE_TYPE_CUBE_ARRAY
            ) {
                set_render_target_size(tex_info, t.mip_slice, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D,
                        first_mip: t.mip_slice,
                        mip_count: 1,
                        first_slice: 0,
                        array_size: 1,
                    },
                ));
            }
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let mut t = unsafe { view_desc.u.texture2d_array };
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
                view_desc.u.texture2d_array = t;
            }
            if matches!(
                texture_type,
                WMT_TEXTURE_TYPE2D
                    | WMT_TEXTURE_TYPE2D_ARRAY
                    | WMT_TEXTURE_TYPE_CUBE
                    | WMT_TEXTURE_TYPE_CUBE_ARRAY
            ) {
                attach.render_target_array_length = t.array_size;
                set_render_target_size(tex_info, t.mip_slice, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                        first_mip: t.mip_slice,
                        mip_count: 1,
                        first_slice: t.first_array_slice,
                        array_size: t.array_size,
                    },
                ));
            }
        }
        6 => {
            if texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE {
                set_render_target_size(tex_info, 0, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_MULTISAMPLE,
                        first_mip: 0,
                        mip_count: 1,
                        first_slice: 0,
                        array_size: 1,
                    },
                ));
            }
            if texture_type == WMT_TEXTURE_TYPE2D {
                set_render_target_size(tex_info, 0, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D,
                        first_mip: 0,
                        mip_count: 1,
                        first_slice: 0,
                        array_size: 1,
                    },
                ));
            }
        }
        7 => {
            // SAFETY: `view_dimension == 7` selects `texture2dms_array`.
            let mut t = unsafe { view_desc.u.texture2dms_array };
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
                view_desc.u.texture2dms_array = t;
            }
            if texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE {
                attach.render_target_array_length = 1;
                set_render_target_size(tex_info, 0, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
                        first_mip: 0,
                        mip_count: 1,
                        first_slice: 0,
                        array_size: 1,
                    },
                ));
            }
            if texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY {
                attach.render_target_array_length = t.array_size;
                set_render_target_size(tex_info, 0, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
                        first_mip: 0,
                        mip_count: 1,
                        first_slice: t.first_array_slice,
                        array_size: t.array_size,
                    },
                ));
            }
            if texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                attach.render_target_array_length = t.array_size;
                set_render_target_size(tex_info, 0, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                        first_mip: 0,
                        mip_count: 1,
                        first_slice: t.first_array_slice,
                        array_size: t.array_size,
                    },
                ));
            }
        }
        8 => {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            let mut t = unsafe { view_desc.u.texture3d };
            let mip_depth = array_size.checked_shr(t.mip_slice).unwrap_or(0).max(1);
            if !t.w_size == 0 {
                t.w_size = mip_depth.wrapping_sub(t.first_w_slice);
                view_desc.u.texture3d = t;
            }
            if texture_type == WMT_TEXTURE_TYPE3D {
                if t.w_size == 1 {
                    attach.depth_plane = t.first_w_slice;
                    set_render_target_size(tex_info, t.mip_slice, &mut attach);
                    return Ok((
                        attach,
                        TextureViewDescriptor {
                            format: metal_format.pixel_format,
                            texture_type: WMT_TEXTURE_TYPE3D,
                            first_mip: t.mip_slice,
                            mip_count: 1,
                            first_slice: 0,
                            array_size: 1,
                        },
                    ));
                }
                if t.first_w_slice == 0 {
                    attach.render_target_array_length = mip_depth;
                    set_render_target_size(tex_info, t.mip_slice, &mut attach);
                    return Ok((
                        attach,
                        TextureViewDescriptor {
                            format: metal_format.pixel_format,
                            texture_type: WMT_TEXTURE_TYPE3D,
                            first_mip: t.mip_slice,
                            mip_count: 1,
                            first_slice: 0,
                            array_size: 1,
                        },
                    ));
                }
            }
        }
        _ => {}
    }
    Err(E_FAIL)
}

/// Validates and normalizes a DSV description (`InitializeAndNormalizeViewDescriptor`).
///
/// # Errors
///
/// Returns `E_FAIL` if the view format or dimension is incompatible with `tex_info`.
pub fn initialize_and_normalize_dsv_descriptor(
    mip_levels: u32,
    array_size: u32,
    tex_info: &WMTTextureInfo,
    view_desc: &mut D3d11DepthStencilViewDesc,
) -> Result<(RenderPassAttachmentDesc, TextureViewDescriptor), Hresult> {
    let _ = mip_levels;
    let (hr, metal_format) = query_dxgi_ref(view_desc.format);
    if hr < 0 {
        return Err(E_FAIL);
    }
    view_desc.flags &= 3;
    let mut attach = RenderPassAttachmentDesc {
        render_target_array_length: 0,
        sample_count: tex_info.sample_count(),
        depth_plane: 0,
        width: 0,
        height: 0,
        slice: 0,
        level: 0,
    };
    let texture_type = tex_info.r#type();
    match view_desc.view_dimension {
        1 => {
            // SAFETY: `view_dimension == 1` selects `texture1d`.
            let t = unsafe { view_desc.u.texture1d };
            if texture_type == WMT_TEXTURE_TYPE2D || texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                set_render_target_size(tex_info, t.mip_slice, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D,
                        first_mip: t.mip_slice,
                        mip_count: 1,
                        first_slice: 0,
                        array_size: 1,
                    },
                ));
            }
        }
        2 => {
            // SAFETY: `view_dimension == 2` selects `texture1d_array`.
            let mut t = unsafe { view_desc.u.texture1d_array };
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
                view_desc.u.texture1d_array = t;
            }
            if texture_type == WMT_TEXTURE_TYPE2D || texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                attach.render_target_array_length = t.array_size;
                set_render_target_size(tex_info, t.mip_slice, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                        first_mip: t.mip_slice,
                        mip_count: 1,
                        first_slice: t.first_array_slice,
                        array_size: t.array_size,
                    },
                ));
            }
        }
        3 => {
            // SAFETY: `view_dimension == 3` selects `texture2d`.
            let t = unsafe { view_desc.u.texture2d };
            if matches!(
                texture_type,
                WMT_TEXTURE_TYPE2D
                    | WMT_TEXTURE_TYPE2D_ARRAY
                    | WMT_TEXTURE_TYPE_CUBE
                    | WMT_TEXTURE_TYPE_CUBE_ARRAY
            ) {
                set_render_target_size(tex_info, t.mip_slice, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D,
                        first_mip: t.mip_slice,
                        mip_count: 1,
                        first_slice: 0,
                        array_size: 1,
                    },
                ));
            }
        }
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d_array`.
            let mut t = unsafe { view_desc.u.texture2d_array };
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
                view_desc.u.texture2d_array = t;
            }
            if matches!(
                texture_type,
                WMT_TEXTURE_TYPE2D
                    | WMT_TEXTURE_TYPE2D_ARRAY
                    | WMT_TEXTURE_TYPE_CUBE
                    | WMT_TEXTURE_TYPE_CUBE_ARRAY
            ) {
                attach.render_target_array_length = t.array_size;
                set_render_target_size(tex_info, t.mip_slice, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                        first_mip: t.mip_slice,
                        mip_count: 1,
                        first_slice: t.first_array_slice,
                        array_size: t.array_size,
                    },
                ));
            }
        }
        5 => {
            if texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE
                || texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY
            {
                set_render_target_size(tex_info, 0, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_MULTISAMPLE,
                        first_mip: 0,
                        mip_count: 1,
                        first_slice: 0,
                        array_size: 1,
                    },
                ));
            }
            if matches!(
                texture_type,
                WMT_TEXTURE_TYPE2D | WMT_TEXTURE_TYPE2D_ARRAY | WMT_TEXTURE_TYPE_CUBE
            ) {
                set_render_target_size(tex_info, 0, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D,
                        first_mip: 0,
                        mip_count: 1,
                        first_slice: 0,
                        array_size: 1,
                    },
                ));
            }
        }
        6 => {
            // SAFETY: `view_dimension == 6` selects `texture2dms_array`.
            let mut t = unsafe { view_desc.u.texture2dms_array };
            if !t.array_size == 0 {
                t.array_size = array_size.wrapping_sub(t.first_array_slice);
                view_desc.u.texture2dms_array = t;
            }
            if texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE {
                attach.render_target_array_length = 1;
                set_render_target_size(tex_info, 0, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
                        first_mip: 0,
                        mip_count: 1,
                        first_slice: 0,
                        array_size: 1,
                    },
                ));
            }
            if texture_type == WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY {
                attach.render_target_array_length = t.array_size;
                set_render_target_size(tex_info, 0, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
                        first_mip: 0,
                        mip_count: 1,
                        first_slice: t.first_array_slice,
                        array_size: t.array_size,
                    },
                ));
            }
            if texture_type == WMT_TEXTURE_TYPE2D_ARRAY {
                attach.render_target_array_length = t.array_size;
                set_render_target_size(tex_info, 0, &mut attach);
                return Ok((
                    attach,
                    TextureViewDescriptor {
                        format: metal_format.pixel_format,
                        texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
                        first_mip: 0,
                        mip_count: 1,
                        first_slice: t.first_array_slice,
                        array_size: t.array_size,
                    },
                ));
            }
        }
        _ => {}
    }
    Err(E_FAIL)
}
