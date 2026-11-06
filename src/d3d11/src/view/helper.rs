//! View descriptor validation and default-view constructors (`d3d11_resource_view_helper.cpp`).

use dxmt_core::{
    format::query_dxgi,
    resource::{
        TextureViewDescriptor,
        dims::mip_extent,
        view::{ALL, ViewDesc, ViewDimension, ViewKind, all_wrapping, texture_view},
    },
};
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_DEPTH16_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_TEXTURE_TYPE_CUBE,
    WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D,
    WMTTextureInfo,
};

use crate::{
    abi::records::{
        D3d11DepthStencilViewDesc, D3d11DepthStencilViewDescUnion, D3d11RenderTargetViewDesc,
        D3d11RenderTargetViewDesc1, D3d11RenderTargetViewDesc1Union,
        D3d11RenderTargetViewDescUnion, D3d11ShaderResourceViewDesc, D3d11ShaderResourceViewDesc1,
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

/// Whether a `kind` view may view a texture of `texture_type`, and the view it is then.
///
/// D3D11's `InitializeAndNormalizeViewDescriptor` rule. A multisampled view over a texture of one
/// sample is a 2D view of its one level (a shader-resource or render-target view over a 2D
/// texture, a depth-stencil view over a 2D, 2D-array or cube one); a multisampled array over a
/// multisampled texture is one slice of it, over a 2D array a 2D array.
const fn d3d11_view(kind: ViewKind, view: &mut ViewDesc, texture_type: u32) -> bool {
    let flat = matches!(texture_type, WMT_TEXTURE_TYPE2D | WMT_TEXTURE_TYPE2D_ARRAY);
    let planes = matches!(
        texture_type,
        WMT_TEXTURE_TYPE2D
            | WMT_TEXTURE_TYPE2D_ARRAY
            | WMT_TEXTURE_TYPE_CUBE
            | WMT_TEXTURE_TYPE_CUBE_ARRAY
    );
    match (kind, view.dimension) {
        (_, ViewDimension::Texture1D | ViewDimension::Texture1DArray) => flat,
        (ViewKind::UnorderedAccess, ViewDimension::Texture2D) => {
            flat || texture_type == WMT_TEXTURE_TYPE_CUBE
        }
        (_, ViewDimension::Texture2D | ViewDimension::Texture2DArray) => planes,
        (ViewKind::ShaderResource | ViewKind::RenderTarget, ViewDimension::Texture2DMS) => {
            match texture_type {
                WMT_TEXTURE_TYPE2D_MULTISAMPLE => true,
                WMT_TEXTURE_TYPE2D => {
                    view.dimension = ViewDimension::Texture2D;
                    true
                }
                _ => false,
            }
        }
        (ViewKind::DepthStencil, ViewDimension::Texture2DMS) => match texture_type {
            WMT_TEXTURE_TYPE2D_MULTISAMPLE | WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY => true,
            WMT_TEXTURE_TYPE2D | WMT_TEXTURE_TYPE2D_ARRAY | WMT_TEXTURE_TYPE_CUBE => {
                view.dimension = ViewDimension::Texture2D;
                true
            }
            _ => false,
        },
        (_, ViewDimension::Texture2DMSArray) => match texture_type {
            WMT_TEXTURE_TYPE2D_MULTISAMPLE => {
                view.first_slice = 0;
                view.array_size = 1;
                true
            }
            WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY => true,
            WMT_TEXTURE_TYPE2D if matches!(kind, ViewKind::ShaderResource) => {
                view.dimension = ViewDimension::Texture2DArray;
                view.first_slice = 0;
                view.array_size = 1;
                true
            }
            WMT_TEXTURE_TYPE2D_ARRAY => {
                view.dimension = ViewDimension::Texture2DArray;
                true
            }
            _ => false,
        },
        (ViewKind::ShaderResource, ViewDimension::Texture3D) => texture_type == WMT_TEXTURE_TYPE3D,
        (ViewKind::UnorderedAccess, ViewDimension::Texture3D) => {
            texture_type == WMT_TEXTURE_TYPE3D && view.first_slice == 0
        }
        (ViewKind::RenderTarget, ViewDimension::Texture3D) => {
            texture_type == WMT_TEXTURE_TYPE3D && (view.array_size == 1 || view.first_slice == 0)
        }
        (
            ViewKind::ShaderResource,
            ViewDimension::TextureCube | ViewDimension::TextureCubeArray,
        ) => {
            matches!(
                texture_type,
                WMT_TEXTURE_TYPE_CUBE | WMT_TEXTURE_TYPE_CUBE_ARRAY
            )
        }
        _ => false,
    }
}

/// The view a shader-resource `view_dimension` selects.
///
/// Its "all the rest" counts resolve against the texture's and are written back into `u`.
fn srv_view_desc(
    view_dimension: u32,
    u: &mut D3d11ShaderResourceViewDesc1Union,
    mip_levels: u32,
    array_size: u32,
) -> Option<ViewDesc> {
    let view = |dimension, first_mip, mip_count, first_slice, array_size| ViewDesc {
        dimension,
        first_mip,
        mip_count,
        first_slice,
        array_size,
    };
    Some(match view_dimension {
        2 => {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            let mut t = unsafe { u.texture1d };
            t.mip_levels = all_wrapping(t.mip_levels, mip_levels, t.most_detailed_mip);
            u.texture1d = t;
            view(
                ViewDimension::Texture1D,
                t.most_detailed_mip,
                t.mip_levels,
                0,
                1,
            )
        }
        3 => {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            let mut t = unsafe { u.texture1d_array };
            t.mip_levels = all_wrapping(t.mip_levels, mip_levels, t.most_detailed_mip);
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture1d_array = t;
            let d = ViewDimension::Texture1DArray;
            view(
                d,
                t.most_detailed_mip,
                t.mip_levels,
                t.first_array_slice,
                t.array_size,
            )
        }
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let mut t = unsafe { u.texture2d };
            t.mip_levels = all_wrapping(t.mip_levels, mip_levels, t.most_detailed_mip);
            u.texture2d = t;
            view(
                ViewDimension::Texture2D,
                t.most_detailed_mip,
                t.mip_levels,
                0,
                1,
            )
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let mut t = unsafe { u.texture2d_array };
            t.mip_levels = all_wrapping(t.mip_levels, mip_levels, t.most_detailed_mip);
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture2d_array = t;
            let d = ViewDimension::Texture2DArray;
            view(
                d,
                t.most_detailed_mip,
                t.mip_levels,
                t.first_array_slice,
                t.array_size,
            )
        }
        6 => view(ViewDimension::Texture2DMS, 0, 1, 0, 1),
        7 => {
            // SAFETY: `view_dimension == 7` selects `texture2dms_array`.
            let mut t = unsafe { u.texture2dms_array };
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture2dms_array = t;
            view(
                ViewDimension::Texture2DMSArray,
                0,
                1,
                t.first_array_slice,
                t.array_size,
            )
        }
        8 => {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            let mut t = unsafe { u.texture3d };
            t.mip_levels = all_wrapping(t.mip_levels, mip_levels, t.most_detailed_mip);
            u.texture3d = t;
            view(
                ViewDimension::Texture3D,
                t.most_detailed_mip,
                t.mip_levels,
                0,
                1,
            )
        }
        9 => {
            // SAFETY: `view_dimension == 9` selects `texture_cube`.
            let mut t = unsafe { u.texture_cube };
            t.mip_levels = all_wrapping(t.mip_levels, mip_levels, t.most_detailed_mip);
            u.texture_cube = t;
            view(
                ViewDimension::TextureCube,
                t.most_detailed_mip,
                t.mip_levels,
                0,
                6,
            )
        }
        10 => {
            // SAFETY: `view_dimension == 10` selects `texture_cube_array`.
            let mut t = unsafe { u.texture_cube_array };
            t.mip_levels = all_wrapping(t.mip_levels, mip_levels, t.most_detailed_mip);
            if t.num_cubes == ALL {
                t.num_cubes = array_size.wrapping_sub(t.first2d_array_face) / 6;
            }
            u.texture_cube_array = t;
            let faces = t.num_cubes.wrapping_mul(6);
            let d = ViewDimension::TextureCubeArray;
            view(
                d,
                t.most_detailed_mip,
                t.mip_levels,
                t.first2d_array_face,
                faces,
            )
        }
        _ => return None,
    })
}

/// A view of `dimension` at `mip_slice`, with slices `first` and `size`.
const fn level_view(dimension: ViewDimension, mip_slice: u32, first: u32, size: u32) -> ViewDesc {
    ViewDesc {
        dimension,
        first_mip: mip_slice,
        mip_count: 1,
        first_slice: first,
        array_size: size,
    }
}

/// The view an unordered-access `view_dimension` selects, as [`srv_view_desc`].
///
/// A 3D view's W slices resolve against its level's depth.
const fn uav_view_desc(
    view_dimension: u32,
    u: &mut D3d11UnorderedAccessViewDesc1Union,
    array_size: u32,
) -> Option<ViewDesc> {
    Some(match view_dimension {
        2 => {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            let t = unsafe { u.texture1d };
            level_view(ViewDimension::Texture1D, t.mip_slice, 0, 1)
        }
        3 => {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            let mut t = unsafe { u.texture1d_array };
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture1d_array = t;
            let d = ViewDimension::Texture1DArray;
            level_view(d, t.mip_slice, t.first_array_slice, t.array_size)
        }
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let t = unsafe { u.texture2d };
            level_view(ViewDimension::Texture2D, t.mip_slice, 0, 1)
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let mut t = unsafe { u.texture2d_array };
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture2d_array = t;
            let d = ViewDimension::Texture2DArray;
            level_view(d, t.mip_slice, t.first_array_slice, t.array_size)
        }
        8 => {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            let mut t = unsafe { u.texture3d };
            let depth = mip_extent(array_size, t.mip_slice);
            t.w_size = all_wrapping(t.w_size, depth, t.first_w_slice);
            u.texture3d = t;
            level_view(
                ViewDimension::Texture3D,
                t.mip_slice,
                t.first_w_slice,
                t.w_size,
            )
        }
        _ => return None,
    })
}

/// The view a render-target `view_dimension` selects, as [`uav_view_desc`].
const fn rtv_view_desc(
    view_dimension: u32,
    u: &mut D3d11RenderTargetViewDesc1Union,
    array_size: u32,
) -> Option<ViewDesc> {
    Some(match view_dimension {
        2 => {
            // SAFETY: `view_dimension == 2` selects `texture1d`.
            let t = unsafe { u.texture1d };
            level_view(ViewDimension::Texture1D, t.mip_slice, 0, 1)
        }
        3 => {
            // SAFETY: `view_dimension == 3` selects `texture1d_array`.
            let mut t = unsafe { u.texture1d_array };
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture1d_array = t;
            let d = ViewDimension::Texture1DArray;
            level_view(d, t.mip_slice, t.first_array_slice, t.array_size)
        }
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d`.
            let t = unsafe { u.texture2d };
            level_view(ViewDimension::Texture2D, t.mip_slice, 0, 1)
        }
        5 => {
            // SAFETY: `view_dimension == 5` selects `texture2d_array`.
            let mut t = unsafe { u.texture2d_array };
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture2d_array = t;
            let d = ViewDimension::Texture2DArray;
            level_view(d, t.mip_slice, t.first_array_slice, t.array_size)
        }
        6 => level_view(ViewDimension::Texture2DMS, 0, 0, 1),
        7 => {
            // SAFETY: `view_dimension == 7` selects `texture2dms_array`.
            let mut t = unsafe { u.texture2dms_array };
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture2dms_array = t;
            let d = ViewDimension::Texture2DMSArray;
            level_view(d, 0, t.first_array_slice, t.array_size)
        }
        8 => {
            // SAFETY: `view_dimension == 8` selects `texture3d`.
            let mut t = unsafe { u.texture3d };
            let depth = mip_extent(array_size, t.mip_slice);
            t.w_size = all_wrapping(t.w_size, depth, t.first_w_slice);
            u.texture3d = t;
            level_view(
                ViewDimension::Texture3D,
                t.mip_slice,
                t.first_w_slice,
                t.w_size,
            )
        }
        _ => return None,
    })
}

/// The view a depth-stencil `view_dimension` selects, as [`uav_view_desc`].
const fn dsv_view_desc(
    view_dimension: u32,
    u: &mut D3d11DepthStencilViewDescUnion,
    array_size: u32,
) -> Option<ViewDesc> {
    Some(match view_dimension {
        1 => {
            // SAFETY: `view_dimension == 1` selects `texture1d`.
            let t = unsafe { u.texture1d };
            level_view(ViewDimension::Texture1D, t.mip_slice, 0, 1)
        }
        2 => {
            // SAFETY: `view_dimension == 2` selects `texture1d_array`.
            let mut t = unsafe { u.texture1d_array };
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture1d_array = t;
            let d = ViewDimension::Texture1DArray;
            level_view(d, t.mip_slice, t.first_array_slice, t.array_size)
        }
        3 => {
            // SAFETY: `view_dimension == 3` selects `texture2d`.
            let t = unsafe { u.texture2d };
            level_view(ViewDimension::Texture2D, t.mip_slice, 0, 1)
        }
        4 => {
            // SAFETY: `view_dimension == 4` selects `texture2d_array`.
            let mut t = unsafe { u.texture2d_array };
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture2d_array = t;
            let d = ViewDimension::Texture2DArray;
            level_view(d, t.mip_slice, t.first_array_slice, t.array_size)
        }
        5 => level_view(ViewDimension::Texture2DMS, 0, 0, 1),
        6 => {
            // SAFETY: `view_dimension == 6` selects `texture2dms_array`.
            let mut t = unsafe { u.texture2dms_array };
            t.array_size = all_wrapping(t.array_size, array_size, t.first_array_slice);
            u.texture2dms_array = t;
            let d = ViewDimension::Texture2DMSArray;
            level_view(d, 0, t.first_array_slice, t.array_size)
        }
        _ => return None,
    })
}

/// The render-pass attachment of `view` over `tex_info`.
///
/// An array view renders its slices as layers; a 3D view one W slice as its depth plane, or the
/// level's whole depth as layers from W slice 0.
const fn attachment(
    view: &ViewDesc,
    tex_info: &WMTTextureInfo,
    depth: u32,
) -> RenderPassAttachmentDesc {
    let mut attach = RenderPassAttachmentDesc {
        render_target_array_length: 0,
        sample_count: tex_info.sample_count(),
        depth_plane: 0,
        width: mip_extent(tex_info.width, view.first_mip),
        height: mip_extent(tex_info.height, view.first_mip),
        slice: 0,
        level: 0,
    };
    if view.dimension.is_array() {
        attach.render_target_array_length = view.array_size;
    } else if matches!(view.dimension, ViewDimension::Texture3D) {
        if view.array_size == 1 {
            attach.depth_plane = view.first_slice;
        } else {
            attach.render_target_array_length = mip_extent(depth, view.first_mip);
        }
    }
    attach
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
    let (hr, metal_format) = query_dxgi(view_desc.format);
    if hr < 0 {
        return Err(E_FAIL);
    }
    let mut pixel_format = metal_format.pixel_format;
    fix_depth_stencil_format(tex_info.pixel_format, &mut pixel_format);
    let mut view = srv_view_desc(
        view_desc.view_dimension,
        &mut view_desc.u,
        mip_levels,
        array_size,
    )
    .ok_or(E_FAIL)?;
    if !d3d11_view(ViewKind::ShaderResource, &mut view, tex_info.r#type()) {
        return Err(E_FAIL);
    }
    Ok(texture_view(pixel_format, &view))
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
    let (hr, metal_format) = query_dxgi(view_desc.format);
    if hr < 0 {
        return Err(E_FAIL);
    }
    let mut pixel_format = metal_format.pixel_format;
    fix_depth_stencil_format(tex_info.pixel_format, &mut pixel_format);
    let mut view =
        uav_view_desc(view_desc.view_dimension, &mut view_desc.u, array_size).ok_or(E_FAIL)?;
    if !d3d11_view(ViewKind::UnorderedAccess, &mut view, tex_info.r#type()) {
        return Err(E_FAIL);
    }
    Ok(texture_view(pixel_format, &view))
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
    let (hr, metal_format) = query_dxgi(view_desc.format);
    if hr < 0 {
        return Err(E_FAIL);
    }
    let mut view =
        rtv_view_desc(view_desc.view_dimension, &mut view_desc.u, array_size).ok_or(E_FAIL)?;
    if !d3d11_view(ViewKind::RenderTarget, &mut view, tex_info.r#type()) {
        return Err(E_FAIL);
    }
    Ok((
        attachment(&view, tex_info, array_size),
        texture_view(metal_format.pixel_format, &view),
    ))
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
    let (hr, metal_format) = query_dxgi(view_desc.format);
    if hr < 0 {
        return Err(E_FAIL);
    }
    view_desc.flags &= 3;
    let mut view =
        dsv_view_desc(view_desc.view_dimension, &mut view_desc.u, array_size).ok_or(E_FAIL)?;
    if !d3d11_view(ViewKind::DepthStencil, &mut view, tex_info.r#type()) {
        return Err(E_FAIL);
    }
    Ok((
        attachment(&view, tex_info, array_size),
        texture_view(metal_format.pixel_format, &view),
    ))
}

#[cfg(test)]
mod tests;
