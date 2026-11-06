use winemetal_wire::{
    WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE_CUBE_ARRAY,
    WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D,
};

use super::{
    ALL, ResourceShape, ViewDesc, ViewDimension, ViewKind, all_saturating, all_wrapping,
    entire_view, texture_view,
};
use crate::resource::texture_info::Dimension;

fn numbers(v: &ViewDesc) -> [u32; 4] {
    [v.first_mip, v.mip_count, v.first_slice, v.array_size]
}

#[test]
fn all_the_rest_per_api() {
    assert_eq!(all_wrapping(3, 8, 5), 3);
    assert_eq!(all_wrapping(ALL, 8, 5), 3);
    assert_eq!(all_wrapping(ALL, 4, 5), u32::MAX);
    assert_eq!(all_saturating(ALL, 4, 5), 0);
    assert_eq!(all_saturating(7, 4, 5), 7);
}

#[test]
fn views_keep_only_what_their_dimension_has() {
    let view = |dimension| ViewDesc {
        dimension,
        first_mip: 2,
        mip_count: 3,
        first_slice: 4,
        array_size: 5,
    };
    let rgba = WMT_PIXEL_FORMAT_RGBA8_UNORM;
    let d = texture_view(rgba, &view(ViewDimension::Texture1D));
    assert_eq!(
        [
            d.texture_type,
            d.first_mip,
            d.mip_count,
            d.first_slice,
            d.array_size
        ],
        [WMT_TEXTURE_TYPE2D, 2, 3, 0, 1]
    );
    let d = texture_view(rgba, &view(ViewDimension::Texture2DMSArray));
    assert_eq!(
        [
            d.texture_type,
            d.first_mip,
            d.mip_count,
            d.first_slice,
            d.array_size
        ],
        [WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, 0, 1, 4, 5]
    );
    let d = texture_view(rgba, &view(ViewDimension::TextureCube));
    assert_eq!(
        [d.texture_type, d.first_slice, d.array_size],
        [WMT_TEXTURE_TYPE_CUBE, 0, 6]
    );
    let d = texture_view(rgba, &view(ViewDimension::TextureCubeArray));
    assert_eq!(
        [d.texture_type, d.first_slice, d.array_size],
        [WMT_TEXTURE_TYPE_CUBE_ARRAY, 4, 5]
    );
    let d = texture_view(rgba, &view(ViewDimension::Texture3D));
    assert_eq!(
        [d.format, d.texture_type, d.first_slice, d.array_size],
        [rgba, WMT_TEXTURE_TYPE3D, 0, 1]
    );
}

#[test]
fn whole_resource_views() {
    let shape = |dimension, array_size, is_array, multisampled, cube| ResourceShape {
        dimension,
        mip_levels: 7,
        array_size,
        is_array,
        multisampled,
        cube,
    };
    let srv = ViewKind::ShaderResource;
    let cubes = shape(Dimension::Texture2D, 18, true, false, true);
    let v = entire_view(srv, &cubes).expect("cube array");
    assert!(matches!(v.dimension, ViewDimension::TextureCubeArray));
    assert_eq!(numbers(&v), [0, 7, 0, 18]);
    let v = entire_view(ViewKind::RenderTarget, &cubes).expect("2D array");
    assert!(matches!(v.dimension, ViewDimension::Texture2DArray));
    assert_eq!(numbers(&v), [0, 1, 0, 18]);
    let one_cube = shape(Dimension::Texture2D, 11, true, false, true);
    let v = entire_view(srv, &one_cube).expect("cube");
    assert!(matches!(v.dimension, ViewDimension::TextureCube));
    let ms = shape(Dimension::Texture2D, 1, false, true, false);
    assert!(entire_view(ViewKind::UnorderedAccess, &ms).is_none());
    let v = entire_view(ViewKind::DepthStencil, &ms).expect("MS");
    assert!(matches!(v.dimension, ViewDimension::Texture2DMS));
    let volume = shape(Dimension::Texture3D, 8, false, false, false);
    assert!(entire_view(ViewKind::DepthStencil, &volume).is_none());
    let v = entire_view(ViewKind::UnorderedAccess, &volume).expect("3D");
    assert_eq!(numbers(&v), [0, 1, 0, 8]);
    let line = shape(Dimension::Texture1D, 4, true, false, true);
    let v = entire_view(srv, &line).expect("1D array");
    assert!(matches!(v.dimension, ViewDimension::Texture1DArray));
    assert_eq!(numbers(&v), [0, 7, 0, 4]);
}
