use std::sync::Arc;

use rustc_hash::FxHashMap;
use winemetal_sys::wire::{
    WMT_COMPARE_FUNCTION_ALWAYS, WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
    WMT_PIXEL_FORMAT_INVALID, WMT_PIXEL_FORMAT_R32_FLOAT, WMT_PIXEL_FORMAT_R32_UINT,
    WMT_PIXEL_FORMAT_RGBA8_SINT, WMT_PIXEL_FORMAT_RGBA8_UINT, WMT_PIXEL_FORMAT_RGBA8_UNORM,
    WMT_STENCIL_OPERATION_REPLACE, WMT_TEXTURE_TYPE2D_ARRAY, WMTScissorRect,
};

use super::{ClearRtv, Kind, depth_stencil_infos, kind, pipeline_info, pso_key};
use crate::{
    library::fake::{Rec, obj, texture, view},
    resource::{Texture, TextureViewKey},
};

fn rtv(clearing: Option<(Arc<Texture>, TextureViewKey)>) -> ClearRtv {
    ClearRtv {
        device: obj(1),
        vertex: None,
        fragment_depth: None,
        fragment_float: None,
        fragment_sint: None,
        fragment_uint: None,
        states: [None, None, None, None],
        pipelines: FxHashMap::default(),
        clearing,
    }
}

const RECT: WMTScissorRect = WMTScissorRect {
    x: 1,
    y: 2,
    width: 3,
    height: 4,
};

#[test]
fn clear_draw_sizes() {
    let mut ctx = Rec::new();
    let target = texture(
        WMT_TEXTURE_TYPE2D_ARRAY,
        WMT_PIXEL_FORMAT_RGBA8_UNORM,
        [4, 4, 1],
        6,
    );

    // One instance per slice, one if the front end says none.
    let mut clear = rtv(Some((Arc::clone(&target), target.full_view())));
    clear.clear_color(&mut ctx, RECT, 6, [1.0, 0.5, 0.25, 0.0]);
    clear.clear_color(&mut ctx, RECT, 0, [0.0; 4]);
    let calls = ctx.take();
    assert_eq!(calls[0], "set_scissor_rect 1 2 3 4");
    assert_eq!(
        calls[1],
        "set_fragment_bytes 30 [3f800000, 3f000000, 3e800000, 0]"
    );
    assert_eq!(calls[2], "draw 3 0 3 0 6");
    assert_eq!(calls[5], "draw 3 0 3 0 1");

    // Depth and stencil: the depth four times over, the reference, the view's slices.
    let three = view(
        WMT_TEXTURE_TYPE2D_ARRAY,
        WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        0,
        3,
    );
    clear.clearing = Some((Arc::clone(&target), three));
    clear.clear_depth_stencil(&mut ctx, RECT, 1.0, 0x80);
    assert_eq!(
        ctx.take(),
        [
            "set_scissor_rect 1 2 3 4".to_owned(),
            "set_fragment_bytes 30 [3f800000, 3f800000, 3f800000, 3f800000]".to_owned(),
            "set_stencil_reference 128".to_owned(),
            "draw 3 0 3 0 3".to_owned(),
        ]
    );

    // The pass ends once.
    clear.end(&mut ctx);
    clear.end(&mut ctx);
    assert_eq!(ctx.take(), ["end_pass"]);

    // Without a begun clear nothing is recorded.
    clear.clear_color(&mut ctx, RECT, 6, [0.0; 4]);
    clear.clear_depth_stencil(&mut ctx, RECT, 1.0, 0);
    ctx.assert_quiet();
}

#[test]
fn clear_shaders_by_format() {
    assert_eq!(kind(WMT_PIXEL_FORMAT_RGBA8_UNORM, 0), Kind::Float);
    assert_eq!(kind(WMT_PIXEL_FORMAT_R32_FLOAT, 0), Kind::Float);
    assert_eq!(kind(WMT_PIXEL_FORMAT_RGBA8_UINT, 0), Kind::Uint);
    assert_eq!(kind(WMT_PIXEL_FORMAT_R32_UINT, 0), Kind::Uint);
    assert_eq!(kind(WMT_PIXEL_FORMAT_RGBA8_SINT, 0), Kind::Sint);
    // The planes win over the format.
    for flag in 1..=3 {
        assert_eq!(kind(WMT_PIXEL_FORMAT_DEPTH32_FLOAT, flag), Kind::Depth);
    }
}

#[test]
fn clear_pipeline_keys_and_descriptions() {
    let keys = [
        pso_key(0, 1, WMT_PIXEL_FORMAT_RGBA8_UNORM),
        pso_key(1, 1, WMT_PIXEL_FORMAT_RGBA8_UNORM),
        pso_key(2, 1, WMT_PIXEL_FORMAT_RGBA8_UNORM),
        pso_key(0, 4, WMT_PIXEL_FORMAT_RGBA8_UNORM),
        pso_key(0, 1, WMT_PIXEL_FORMAT_BGRA8_UNORM),
    ];
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            assert_ne!(a, b);
        }
    }
    assert_eq!(keys[0], pso_key(0, 1, WMT_PIXEL_FORMAT_RGBA8_UNORM));

    // A colour target: the format in the first slot, no depth, the samples it has.
    let color = pipeline_info(11, 22, WMT_PIXEL_FORMAT_RGBA8_UNORM, 4, 0);
    assert_eq!(color.colors[0].pixel_format, WMT_PIXEL_FORMAT_RGBA8_UNORM);
    assert_eq!(color.colors[1].pixel_format, WMT_PIXEL_FORMAT_INVALID);
    assert_eq!(color.depth_pixel_format, WMT_PIXEL_FORMAT_INVALID);
    assert_eq!(color.stencil_pixel_format, WMT_PIXEL_FORMAT_INVALID);
    assert_eq!(color.raster_sample_count, 4);
    assert_eq!((color.vertex_function, color.fragment_function), (11, 22));
    assert_eq!(color.rasterization_enabled, 1);
    assert_eq!(color.input_primitive_topology, 3);

    // A depth-stencil target: the planes the flag names.
    let planes = |flag| {
        let info = pipeline_info(11, 22, WMT_PIXEL_FORMAT_DEPTH32_FLOAT, 1, flag);
        assert_eq!(info.colors[0].pixel_format, WMT_PIXEL_FORMAT_INVALID);
        (info.depth_pixel_format != 0, info.stencil_pixel_format != 0)
    };
    assert_eq!(planes(1), (true, false));
    assert_eq!(planes(2), (false, true));
    assert_eq!(planes(3), (true, true));
}

#[test]
fn depth_stencil_states_as_cpp() {
    let [read_only, depth, both, stencil] = depth_stencil_infos();
    for (info, depth_write, stencil_on) in [
        (read_only, 0, 0),
        (depth, 1, 0),
        (both, 1, 1),
        (stencil, 0, 1),
    ] {
        assert_eq!(info.depth_compare_function, WMT_COMPARE_FUNCTION_ALWAYS);
        assert_eq!(info.depth_write_enabled, depth_write);
        assert_eq!(info.front_stencil.enabled, stencil_on);
        assert_eq!(info.back_stencil.enabled, 0);
        if stencil_on == 1 {
            let s = info.front_stencil;
            assert_eq!(s.depth_stencil_pass_op, WMT_STENCIL_OPERATION_REPLACE);
            assert_eq!(s.depth_fail_op, WMT_STENCIL_OPERATION_REPLACE);
            assert_eq!(s.stencil_fail_op, WMT_STENCIL_OPERATION_REPLACE);
            assert_eq!(s.stencil_compare_function, WMT_COMPARE_FUNCTION_ALWAYS);
            assert_eq!((s.read_mask, s.write_mask), (0xff, 0xff));
        }
    }
}
