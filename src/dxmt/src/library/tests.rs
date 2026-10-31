use winemetal_sys::wire::WMT_PIXEL_FORMAT_INVALID;

use super::render_pipeline_info;

#[test]
fn render_pipeline_info_as_cpp() {
    let info = render_pipeline_info();
    for color in info.colors {
        assert_eq!(color.pixel_format, WMT_PIXEL_FORMAT_INVALID);
        assert_eq!(color.write_mask, 15);
        assert_eq!(color.blending_enabled, 0);
        assert_eq!(
            (color.src_rgb_blend_factor, color.dst_rgb_blend_factor),
            (1, 0)
        );
        assert_eq!(
            (color.src_alpha_blend_factor, color.dst_alpha_blend_factor),
            (1, 0)
        );
    }
    assert_eq!(
        (info.rasterization_enabled, info.raster_sample_count),
        (1, 1)
    );
    assert_eq!(info.max_tessellation_factor, 16);
    assert_eq!(info.input_primitive_topology, 0);
    assert_eq!((info.vertex_function, info.fragment_function), (0, 0));
}
