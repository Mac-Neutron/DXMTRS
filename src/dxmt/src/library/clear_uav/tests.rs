use std::sync::Arc;

use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_A8_UNORM, WMT_PIXEL_FORMAT_BGR10A2_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM,
    WMT_PIXEL_FORMAT_INVALID, WMT_PIXEL_FORMAT_R32_FLOAT, WMT_PIXEL_FORMAT_R32_UINT,
    WMT_PIXEL_FORMAT_RG11B10_FLOAT, WMT_PIXEL_FORMAT_RGB9E5_FLOAT, WMT_PIXEL_FORMAT_RGB10A2_UNORM,
    WMT_PIXEL_FORMAT_RGBA8_UINT, WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_PIXEL_FORMAT_RGBA16_FLOAT,
    WMT_PIXEL_FORMAT_RGBA32_UINT, WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE_CUBE_ARRAY,
    WMT_TEXTURE_TYPE_TEXTURE_BUFFER, WMT_TEXTURE_TYPE1D, WMT_TEXTURE_TYPE1D_ARRAY,
    WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY, WMT_TEXTURE_TYPE2D_MULTISAMPLE,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D,
};

use super::{ClearUav, Shape, set_uint_color, shape};
use crate::{
    library::fake::{Rec, obj, texture, view},
    resource::{Buffer, BufferViewDescriptor},
};

#[test]
fn clear_dispatch_sizes() {
    let mut ctx = Rec::new();
    let mut clear = ClearUav::new(&mut ctx);
    assert_eq!(ctx.names.len(), 10);
    ctx.take();
    let color = [1.0, 0.5, 0.25, 0.0];
    let f32_words = "[3f800000, 3f000000, 3e800000, 0";

    // A 2D texture: one plane of the rectangle's size.
    let plane = texture(
        WMT_TEXTURE_TYPE2D,
        WMT_PIXEL_FORMAT_RGBA8_UNORM,
        [64, 32, 1],
        1,
    );
    clear.begin_texture_f32(&mut ctx, color, &plane, plane.full_view());
    clear.clear(&mut ctx, 3, 4, 10, 20);
    clear.end(&mut ctx);
    assert_eq!(
        ctx.take(),
        [
            "start_compute_pass".to_owned(),
            "set_compute_pso cs_clear_texture2d_float 32x1x1".to_owned(),
            "set_compute_texture 0 format=70 type=2 access=2".to_owned(),
            format!("set_compute_bytes 1 {f32_words}, 3, 4, a, 14]"),
            "dispatch 10x20x1".to_owned(),
            "end_pass".to_owned(),
        ]
    );

    // An array: the dispatch's depth is its slices.
    let array = texture(
        WMT_TEXTURE_TYPE2D_ARRAY,
        WMT_PIXEL_FORMAT_RGBA16_FLOAT,
        [8, 8, 1],
        5,
    );
    clear.begin_texture_f32(&mut ctx, color, &array, array.full_view());
    clear.clear(&mut ctx, 0, 0, 8, 8);
    clear.end(&mut ctx);
    let calls = ctx.take();
    assert_eq!(
        calls[1],
        "set_compute_pso cs_clear_texture2d_array_float 32x1x1"
    );
    assert_eq!(calls[4], "dispatch 8x8x5");

    // A volume: the depth of the first level the view covers.
    let volume = texture(
        WMT_TEXTURE_TYPE3D,
        WMT_PIXEL_FORMAT_R32_FLOAT,
        [16, 16, 8],
        1,
    );
    let level_1 = view(WMT_TEXTURE_TYPE3D, WMT_PIXEL_FORMAT_R32_FLOAT, 1, 1);
    clear.begin_texture_f32(&mut ctx, color, &volume, level_1);
    clear.clear(&mut ctx, 0, 0, 8, 8);
    clear.end(&mut ctx);
    let calls = ctx.take();
    assert_eq!(calls[1], "set_compute_pso cs_clear_texture3d_float 32x1x1");
    assert_eq!(calls[4], "dispatch 8x8x4");

    // The depth goes back to 1 with the end of the clear.
    clear.begin_texture_f32(&mut ctx, color, &plane, plane.full_view());
    clear.clear(&mut ctx, 0, 0, 2, 2);
    assert_eq!(ctx.take()[4], "dispatch 2x2x1");
}

#[test]
fn a_cube_is_no_clear_target() {
    let mut ctx = Rec::new();
    let mut clear = ClearUav::new(&mut ctx);
    ctx.take();
    let cube = texture(
        WMT_TEXTURE_TYPE_CUBE,
        WMT_PIXEL_FORMAT_RGBA8_UNORM,
        [4, 4, 1],
        1,
    );
    clear.begin_texture_f32(&mut ctx, [0.0; 4], &cube, cube.full_view());
    // The pass starts and no pipeline is set; ending it still ends the pass.
    assert_eq!(ctx.take(), ["start_compute_pass"]);
    clear.end(&mut ctx);
    assert_eq!(ctx.take(), ["end_pass"]);
    clear.end(&mut ctx);
    ctx.assert_quiet();
}

#[test]
fn integer_clears_go_through_the_unsigned_format() {
    let mut ctx = Rec::new();
    let mut clear = ClearUav::new(&mut ctx);
    ctx.take();
    let rgba = texture(
        WMT_TEXTURE_TYPE2D,
        WMT_PIXEL_FORMAT_RGBA8_UNORM,
        [4, 4, 1],
        1,
    );
    clear.begin_texture_u32(&mut ctx, [1, 2, 3, 300], &rgba, rgba.full_view());
    clear.clear(&mut ctx, 0, 0, 4, 4);
    let calls = ctx.take();
    assert_eq!(calls[1], "set_compute_pso cs_clear_texture2d_uint 32x1x1");
    // The view is the unsigned one; the colour is cut to eight bits a channel.
    assert_eq!(
        calls[2],
        format!("set_compute_texture 0 format={WMT_PIXEL_FORMAT_RGBA8_UINT} type=2 access=2")
    );
    assert_eq!(calls[3], "set_compute_bytes 1 [1, 2, 3, 2c, 0, 0, 4, 4]");
    clear.end(&mut ctx);
    ctx.take();

    // RG11B10 and RGB9E5 have no unsigned form of their own: they are read as R32Uint.
    for format in [
        WMT_PIXEL_FORMAT_RG11B10_FLOAT,
        WMT_PIXEL_FORMAT_RGB9E5_FLOAT,
    ] {
        let packed = texture(WMT_TEXTURE_TYPE2D, format, [4, 4, 1], 1);
        clear.begin_texture_u32(&mut ctx, [0, 0, 0, 0], &packed, packed.full_view());
        clear.clear(&mut ctx, 0, 0, 1, 1);
        assert_eq!(
            ctx.take()[2],
            format!("set_compute_texture 0 format={WMT_PIXEL_FORMAT_R32_UINT} type=2 access=2")
        );
        clear.end(&mut ctx);
        ctx.take();
    }
}

#[test]
fn buffers_pick_their_pipelines() {
    let mut ctx = Rec::new();
    let mut clear = ClearUav::new(&mut ctx);
    ctx.take();
    let buffer = Arc::new(Buffer::new(256, &obj(1)));
    let texel = BufferViewDescriptor {
        format: WMT_PIXEL_FORMAT_RGBA8_UNORM,
        texel_size: 4,
    }
    .key();
    let mut pipelines = Vec::new();
    clear.begin_buffer_f32(&mut ctx, [0.0; 4], &buffer);
    clear.begin_buffer_u32(&mut ctx, [0; 4], &buffer);
    clear.begin_texel_buffer_f32(&mut ctx, [0.0; 4], &buffer, texel);
    clear.begin_texel_buffer_u32(&mut ctx, [0; 4], &buffer, texel);
    for call in ctx.take() {
        if let Some(pso) = call.strip_prefix("set_compute_pso ") {
            pipelines.push(pso.to_owned());
        }
    }
    assert_eq!(
        pipelines,
        [
            "cs_clear_buffer_float 32x1x1",
            "cs_clear_buffer_uint 32x1x1",
            "cs_clear_tbuffer_float 32x1x1",
            "cs_clear_tbuffer_uint 32x1x1",
        ]
    );
    // A buffer with no allocation has nothing to dispatch over.
    clear.clear(&mut ctx, 0, 0, 4, 1);
    ctx.assert_quiet();
}

#[test]
fn uav_shapes_as_cpp() {
    assert_eq!(shape(WMT_TEXTURE_TYPE2D_ARRAY, 7, 9), Shape::Array(7));
    assert_eq!(shape(WMT_TEXTURE_TYPE1D_ARRAY, 3, 9), Shape::Array(3));
    assert_eq!(shape(WMT_TEXTURE_TYPE3D, 7, 9), Shape::Volume(9));
    for invalid in [
        WMT_TEXTURE_TYPE_CUBE,
        WMT_TEXTURE_TYPE_CUBE_ARRAY,
        WMT_TEXTURE_TYPE2D_MULTISAMPLE,
        WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
    ] {
        assert_eq!(shape(invalid, 7, 9), Shape::Invalid);
    }
    for plane in [
        WMT_TEXTURE_TYPE1D,
        WMT_TEXTURE_TYPE2D,
        WMT_TEXTURE_TYPE_TEXTURE_BUFFER,
    ] {
        assert_eq!(shape(plane, 7, 9), Shape::Plane);
    }
}

#[test]
fn uint_clear_color_as_cpp() {
    let color = [0x1_0001, 0x2_0002, 0x3_0003, 0xffff_ffff];
    let set = |format| {
        let mut words = [7; 4];
        set_uint_color(&mut words, color, format);
        words
    };
    // Alpha alone in the first word; the others stay.
    assert_eq!(set(WMT_PIXEL_FORMAT_A8_UNORM), [0xff, 7, 7, 7]);
    assert_eq!(set(WMT_PIXEL_FORMAT_RGBA8_UINT), [1, 2, 3, 0xff]);
    assert_eq!(set(WMT_PIXEL_FORMAT_BGRA8_UNORM), [3, 2, 1, 0xff]);
    assert_eq!(set(WMT_PIXEL_FORMAT_RGBA16_FLOAT), [1, 2, 3, 0xffff]);
    assert_eq!(set(WMT_PIXEL_FORMAT_RGB10A2_UNORM), [1, 2, 3, 3]);
    assert_eq!(set(WMT_PIXEL_FORMAT_BGR10A2_UNORM), [3, 2, 1, 3]);
    assert_eq!(
        set(WMT_PIXEL_FORMAT_RG11B10_FLOAT),
        [1 | (2 << 11) | (3 << 22), 0, 0, 0]
    );
    assert_eq!(
        set(WMT_PIXEL_FORMAT_RGB9E5_FLOAT),
        [1 | (2 << 9) | (3 << 18) | (0x1f << 27), 0, 0, 0]
    );
    // Any other format, and a raw buffer (the invalid format), take the words whole.
    assert_eq!(set(WMT_PIXEL_FORMAT_RGBA32_UINT), color);
    assert_eq!(set(WMT_PIXEL_FORMAT_INVALID), color);
}
