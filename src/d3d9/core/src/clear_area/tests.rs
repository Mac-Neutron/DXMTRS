use super::{
    ClearArea, clamp_rect, clear_area, clear_regions, clip_rect_to_viewport, intersect_d3d_rects,
};

const FULL: (u32, u32, u32, u32) = (0, 0, 256, 256);

#[test]
fn clear_regions_follow_device_clear() {
    let scissor = (64, 64, 192, 192);
    // No rects: the whole target, or the scissor rect, or nothing for an empty one.
    assert_eq!(clear_regions(None, None), None);
    assert_eq!(clear_regions(None, Some(scissor)), Some(vec![scissor]));
    assert_eq!(clear_regions(None, Some((10, 10, 10, 50))), Some(vec![]));
    // Rects: themselves; cut to the scissor, and dropped outside it.
    let rects = vec![(0, 0, 100, 100), (200, 200, 250, 250)];
    assert_eq!(
        clear_regions(Some(rects.clone()), None),
        Some(rects.clone())
    );
    assert_eq!(
        clear_regions(Some(rects), Some(scissor)),
        Some(vec![(64, 64, 100, 100)])
    );
    // A count of 0 clears nothing, with the scissor or without.
    assert_eq!(clear_regions(Some(vec![]), None), Some(vec![]));
    assert_eq!(clear_regions(Some(vec![]), Some(scissor)), Some(vec![]));
}

#[test]
fn intersect_d3d_rects_cases() {
    assert_eq!(
        intersect_d3d_rects((0, 0, 10, 10), (5, 5, 20, 20)),
        Some((5, 5, 10, 10))
    );
    assert_eq!(intersect_d3d_rects((0, 0, 10, 10), (10, 0, 20, 10)), None);
    assert_eq!(intersect_d3d_rects((5, 5, 5, 9), (0, 0, 10, 10)), None);
}

#[test]
fn clip_rect_to_viewport_cases() {
    let vp = (64, 64, 128, 128);
    assert_eq!(
        clip_rect_to_viewport((32, 100, 100, 150), vp),
        Some((64, 100, 36, 50))
    );
    assert_eq!(clip_rect_to_viewport((0, 0, 64, 64), vp), None);
    assert_eq!(clip_rect_to_viewport((100, 100, 90, 120), vp), None);
    assert_eq!(clip_rect_to_viewport((100, 100, 100, 120), vp), None);
    assert_eq!(
        clip_rect_to_viewport((-50, -50, 10, 10), FULL),
        Some((0, 0, 10, 10))
    );
    assert_eq!(clip_rect_to_viewport((-50, -50, -10, -10), FULL), None);
    // A viewport that ends past u32 does not wrap.
    assert_eq!(
        clip_rect_to_viewport((0, 0, 8, 8), (4, 4, u32::MAX, u32::MAX)),
        Some((4, 4, 4, 4))
    );
}

#[test]
fn clamp_rect_cases() {
    let extent = (64, 32);
    assert_eq!(clamp_rect([8, 8, 16, 16], extent), Some([8, 8, 16, 16]));
    assert_eq!(clamp_rect([0, 0, 300, 300], extent), Some([0, 0, 64, 32]));
    assert_eq!(clamp_rect([60, 30, 16, 16], extent), Some([60, 30, 4, 2]));
    assert_eq!(clamp_rect([64, 0, 16, 16], extent), None);
    assert_eq!(clamp_rect([300, 300, 16, 16], extent), None);
    assert_eq!(clamp_rect([8, 8, 0, 16], extent), None);
    assert_eq!(clamp_rect([8, 8, 16, 0], extent), None);
    assert_eq!(
        clamp_rect([0, 0, u32::MAX, u32::MAX], extent),
        Some([0, 0, 64, 32])
    );
}

#[test]
fn clear_area_none_with_a_covering_viewport_is_whole() {
    assert_eq!(clear_area(None, FULL, (256, 256)), ClearArea::Whole);
}

#[test]
fn clear_area_none_with_a_small_viewport_is_that_region() {
    assert_eq!(
        clear_area(None, (64, 64, 128, 128), (256, 256)),
        ClearArea::Regions(vec![[64, 64, 128, 128]])
    );
}

#[test]
fn clear_area_an_oversized_viewport_covers() {
    assert_eq!(
        clear_area(None, (0, 0, 512, 512), (256, 256)),
        ClearArea::Whole
    );
    // But not one that starts past the origin.
    assert_eq!(
        clear_area(None, (1, 0, 512, 512), (256, 256)),
        ClearArea::Regions(vec![[1, 0, 255, 256]])
    );
}

#[test]
fn clear_area_no_rects_clear_nothing() {
    assert_eq!(
        clear_area(Some(&[]), FULL, (256, 256)),
        ClearArea::Regions(vec![])
    );
}

#[test]
fn clear_area_rects_are_clipped_to_the_viewport() {
    assert_eq!(
        clear_area(
            Some(&[(32, 100, 100, 150), (0, 0, 10, 10)]),
            (64, 64, 128, 128),
            (256, 256)
        ),
        ClearArea::Regions(vec![[64, 100, 36, 50]])
    );
}

#[test]
fn clear_area_a_rect_covering_the_extent_is_whole() {
    assert_eq!(
        clear_area(
            Some(&[(8, 8, 16, 16), (-4, -4, 300, 300)]),
            FULL,
            (256, 256)
        ),
        ClearArea::Whole
    );
}

#[test]
fn clear_area_a_rect_past_the_extent_is_clamped() {
    // A 128x128 depth-stencil under a 256x256 viewport: the rect is cut to it.
    assert_eq!(
        clear_area(Some(&[(64, 64, 200, 200)]), FULL, (128, 128)),
        ClearArea::Regions(vec![[64, 64, 64, 64]])
    );
    // And one wholly past it clears nothing.
    assert_eq!(
        clear_area(Some(&[(130, 130, 200, 200)]), FULL, (128, 128)),
        ClearArea::Regions(vec![])
    );
    // The viewport of the larger colour target covers the smaller depth-stencil.
    assert_eq!(clear_area(None, FULL, (128, 128)), ClearArea::Whole);
}

#[test]
fn clear_area_a_larger_depth_is_not_covered_by_the_colour_viewport() {
    assert_eq!(
        clear_area(None, FULL, (512, 512)),
        ClearArea::Regions(vec![[0, 0, 256, 256]])
    );
}

#[test]
fn clear_area_inverted_and_negative_rects() {
    assert_eq!(
        clear_area(
            Some(&[(100, 100, 50, 150), (-20, -20, 30, 30), (40, 40, 40, 80)]),
            FULL,
            (256, 256)
        ),
        ClearArea::Regions(vec![[0, 0, 30, 30]])
    );
}
