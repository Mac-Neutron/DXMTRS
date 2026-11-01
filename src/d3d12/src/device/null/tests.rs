use winemetal_wire::{
    WMT_PIXEL_FORMAT_RGBA32_FLOAT, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
    WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_RESOURCE_STORAGE_MODE_SHARED, WMT_TEXTURE_TYPE_CUBE,
    WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE1D, WMT_TEXTURE_TYPE2D,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D,
    WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW, WMT_TEXTURE_USAGE_SHADER_READ,
};

use super::{
    ATOMIC64_LOCKS, atomic64_locks, null_texel_buffer, null_texel_buffer_view, null_texture,
    null_texture_info, null_vertex_buffer, reset_atomic64_locks,
};
use crate::device::Device;

#[test]
fn null_views_as_cpp() {
    assert_eq!(ATOMIC64_LOCKS, 65536);

    for ty in [
        WMT_TEXTURE_TYPE1D,
        WMT_TEXTURE_TYPE2D,
        WMT_TEXTURE_TYPE3D,
        WMT_TEXTURE_TYPE_CUBE,
        WMT_TEXTURE_TYPE_CUBE_ARRAY,
    ] {
        let info = null_texture_info(ty);
        assert_eq!(info.pixel_format, WMT_PIXEL_FORMAT_RGBA32_FLOAT);
        assert_eq!(info.r#type(), ty);
        assert_eq!((info.width, info.height, info.depth), (1, 1, 1));
        assert_eq!(info.array_length, 1);
        assert_eq!(info.mipmap_level_count(), 1);
        assert_eq!(info.sample_count(), 1);
        assert_eq!(
            info.usage(),
            WMT_TEXTURE_USAGE_SHADER_READ | WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW
        );
        assert_eq!(
            info.options,
            WMT_RESOURCE_STORAGE_MODE_SHARED | WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED
        );
    }

    for ty in [
        WMT_TEXTURE_TYPE2D_MULTISAMPLE,
        WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
    ] {
        let info = null_texture_info(ty);
        assert_eq!(info.pixel_format, WMT_PIXEL_FORMAT_RGBA32_FLOAT);
        assert_eq!(info.r#type(), ty);
        assert_eq!(info.sample_count(), 2);
        assert_eq!(
            info.options,
            WMT_RESOURCE_STORAGE_MODE_PRIVATE | WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED
        );
    }

    let buf_key = null_texel_buffer_view();
    assert_eq!(buf_key.format(), WMT_PIXEL_FORMAT_RGBA32_FLOAT);
    assert_eq!(buf_key.texel_size(), 16);

    let dev = Device::new_dummy();
    assert!(null_texture(&dev, WMT_TEXTURE_TYPE2D).is_none());
    assert!(null_texel_buffer(&dev).is_none());
    assert_eq!(null_vertex_buffer(&dev), 0);
    assert_eq!(atomic64_locks(&dev), 0);
    reset_atomic64_locks(&dev);
}
