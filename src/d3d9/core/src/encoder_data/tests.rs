use super::StretchSurfaceFlags;

#[test]
fn stretch_surface_flags_are_distinct_bits() {
    let all = StretchSurfaceFlags::IS_RENDER_TARGET
        | StretchSurfaceFlags::IS_OFFSCREEN_PLAIN_DEFAULT
        | StretchSurfaceFlags::IS_DEPTH_STENCIL;
    assert_eq!(all.bits().count_ones(), 3);
    assert!(StretchSurfaceFlags::empty().is_empty());
}
