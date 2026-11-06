//! The neutral view description and the Metal texture view built from it, for D3D11 and D3D12.
//!
//! Each API reads its own view description into a [`ViewDesc`], resolving "all the rest" counts
//! with its own rule ([`all_wrapping`] for D3D11, [`all_saturating`] for D3D12) and checking what
//! it checks; [`texture_view`] turns the result into the Metal view. [`entire_view`] is the view
//! of a whole resource an API describes when the application passes none, which each API writes
//! back into its own description.

use winemetal_wire::{
    WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE2D,
    WMT_TEXTURE_TYPE2D_ARRAY, WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
    WMT_TEXTURE_TYPE3D,
};

use super::{TextureViewDescriptor, texture_info::Dimension};

/// The count both APIs write for "every level (or slice) from the first on".
pub const ALL: u32 = u32::MAX;

/// A texture view's dimension, as both APIs name it.
#[derive(Clone, Copy)]
pub enum ViewDimension {
    Texture1D,
    Texture1DArray,
    Texture2D,
    Texture2DArray,
    Texture2DMS,
    Texture2DMSArray,
    Texture3D,
    TextureCube,
    TextureCubeArray,
}

impl ViewDimension {
    /// The Metal texture type a view of this dimension is: a 1D one is 2D.
    #[must_use]
    pub const fn texture_type(self) -> u32 {
        match self {
            Self::Texture1D | Self::Texture2D => WMT_TEXTURE_TYPE2D,
            Self::Texture1DArray | Self::Texture2DArray => WMT_TEXTURE_TYPE2D_ARRAY,
            Self::Texture2DMS => WMT_TEXTURE_TYPE2D_MULTISAMPLE,
            Self::Texture2DMSArray => WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
            Self::Texture3D => WMT_TEXTURE_TYPE3D,
            Self::TextureCube => WMT_TEXTURE_TYPE_CUBE,
            Self::TextureCubeArray => WMT_TEXTURE_TYPE_CUBE_ARRAY,
        }
    }

    /// Whether a view of this dimension has array slices (a render target's layers).
    #[must_use]
    pub const fn is_array(self) -> bool {
        matches!(
            self,
            Self::Texture1DArray | Self::Texture2DArray | Self::Texture2DMSArray
        )
    }
}

/// A view of a texture, whatever API described it.
///
/// A cube's slices count faces; a 3D view's slices are its W slices, which the Metal view does
/// not take. A shader-resource view has `mip_count` levels; the others have one.
pub struct ViewDesc {
    pub dimension: ViewDimension,
    pub first_mip: u32,
    pub mip_count: u32,
    pub first_slice: u32,
    pub array_size: u32,
}

/// `count`, or what is left of `total` from `first` on when it is [`ALL`], wrapping: D3D11's rule.
#[must_use]
pub const fn all_wrapping(count: u32, total: u32, first: u32) -> u32 {
    if count == ALL {
        total.wrapping_sub(first)
    } else {
        count
    }
}

/// `count`, or what is left of `total` from `first` on when it is [`ALL`], never below 0.
///
/// D3D12's rule.
#[must_use]
pub const fn all_saturating(count: u32, total: u32, first: u32) -> u32 {
    if count == ALL {
        total.saturating_sub(first)
    } else {
        count
    }
}

/// The Metal view of `view` in `format` (a `WMTPixelFormat`).
///
/// A multisampled view has the one level; a view that is not an array has the one slice, a cube
/// its six faces; a 3D view's W slices stay with the render pass.
#[must_use]
pub const fn texture_view(format: u32, view: &ViewDesc) -> TextureViewDescriptor {
    let (first_mip, mip_count) = match view.dimension {
        ViewDimension::Texture2DMS | ViewDimension::Texture2DMSArray => (0, 1),
        _ => (view.first_mip, view.mip_count),
    };
    let (first_slice, array_size) = match view.dimension {
        ViewDimension::Texture1D
        | ViewDimension::Texture2D
        | ViewDimension::Texture2DMS
        | ViewDimension::Texture3D => (0, 1),
        ViewDimension::TextureCube => (0, 6),
        _ => (view.first_slice, view.array_size),
    };
    TextureViewDescriptor {
        format,
        texture_type: view.dimension.texture_type(),
        first_mip,
        mip_count,
        first_slice,
        array_size,
    }
}

/// What a view is for.
#[derive(Clone, Copy)]
pub enum ViewKind {
    ShaderResource,
    UnorderedAccess,
    RenderTarget,
    DepthStencil,
}

/// The shape of a texture resource, as its API decided it.
///
/// `array_size` is a 3D texture's depth. Each API decides `is_array` (D3D11: a size other than 1;
/// D3D12: more than 1); `cube` is D3D11's misc flag, which only a shader-resource view follows.
pub struct ResourceShape {
    pub dimension: Dimension,
    pub mip_levels: u32,
    pub array_size: u32,
    pub is_array: bool,
    pub multisampled: bool,
    pub cube: bool,
}

/// The view of the whole of `resource` a `kind` view takes when the application describes none.
///
/// None for a multisampled unordered-access view and a 3D depth-stencil view, which neither API
/// has. A cube of more than one cube (six faces each) is a cube array.
#[must_use]
pub fn entire_view(kind: ViewKind, resource: &ResourceShape) -> Option<ViewDesc> {
    let srv = matches!(kind, ViewKind::ShaderResource);
    let mip_count = if srv { resource.mip_levels } else { 1 };
    let whole = |dimension, first_slice, array_size| ViewDesc {
        dimension,
        first_mip: 0,
        mip_count,
        first_slice,
        array_size,
    };
    let array_size = resource.array_size;
    Some(match resource.dimension {
        Dimension::Texture1D if resource.is_array => {
            whole(ViewDimension::Texture1DArray, 0, array_size)
        }
        Dimension::Texture1D => whole(ViewDimension::Texture1D, 0, 1),
        Dimension::Texture2D if resource.multisampled => {
            if matches!(kind, ViewKind::UnorderedAccess) {
                return None;
            }
            if resource.is_array {
                whole(ViewDimension::Texture2DMSArray, 0, array_size)
            } else {
                whole(ViewDimension::Texture2DMS, 0, 1)
            }
        }
        Dimension::Texture2D if !resource.is_array => whole(ViewDimension::Texture2D, 0, 1),
        Dimension::Texture2D if srv && resource.cube => {
            let num_cubes = array_size / 6;
            if num_cubes > 1 {
                whole(ViewDimension::TextureCubeArray, 0, num_cubes * 6)
            } else {
                whole(ViewDimension::TextureCube, 0, 6)
            }
        }
        Dimension::Texture2D => whole(ViewDimension::Texture2DArray, 0, array_size),
        Dimension::Texture3D => {
            if matches!(kind, ViewKind::DepthStencil) {
                return None;
            }
            whole(ViewDimension::Texture3D, 0, array_size)
        }
    })
}

#[cfg(test)]
mod tests;
