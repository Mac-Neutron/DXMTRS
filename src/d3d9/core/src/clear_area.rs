//! Which part of an attachment a `Clear` reaches.
//!
//! D3D9's `Clear` takes rects, is bounded by the scissor rect while the scissor test is on, and
//! by the viewport always. [`clear_regions`] is the first half, which both runtimes call: the
//! rects and the scissor as one list. mtld3d's encoder then clips that list to the viewport
//! ([`clip_rect_to_viewport`]) and folds a region that covers the attachment into a whole clear;
//! the core runtime decides the same on the API thread, an attachment at a time
//! ([`clear_area`]), and cuts what is left to the attachment ([`clamp_rect`]).

use crate::passes::region_covers_extent;

/// Intersects two half-open D3D9 rects `(x1, y1, x2, y2)`.
///
/// `None` when they are disjoint or either is degenerate. Clips a `Clear` rect to the scissor
/// rect when `D3DRS_SCISSORTESTENABLE` is on.
#[must_use]
pub fn intersect_d3d_rects(
    a: (i32, i32, i32, i32),
    b: (i32, i32, i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    let x1 = a.0.max(b.0);
    let y1 = a.1.max(b.1);
    let x2 = a.2.min(b.2);
    let y2 = a.3.min(b.3);
    (x2 > x1 && y2 > y1).then_some((x1, y1, x2, y2))
}

/// The regions a `Clear` names: its rects, cut to the scissor rect while the test is on.
///
/// `rects` is `None` for a null `pRects` (the whole target, whatever the count) and the list
/// otherwise (empty for a count of 0, which clears nothing). `scissor` is the scissor rect
/// ([`crate::render_state::scissor_region`]) while `D3DRS_SCISSORTESTENABLE` is on: whole target
/// ∩ scissor is the scissor rect, and an empty one clears nothing. `None` is the whole target; a
/// list is explicit, already cut to the scissor and possibly empty.
#[must_use]
pub fn clear_regions(
    rects: Option<Vec<(i32, i32, i32, i32)>>,
    scissor: Option<(i32, i32, i32, i32)>,
) -> Option<Vec<(i32, i32, i32, i32)>> {
    let Some(mut list) = rects else {
        return scissor.map(|(x1, y1, x2, y2)| {
            if x2 > x1 && y2 > y1 {
                vec![(x1, y1, x2, y2)]
            } else {
                Vec::new()
            }
        });
    };
    if let Some(scissor) = scissor {
        list.retain_mut(|r| {
            intersect_d3d_rects(*r, scissor).is_some_and(|clipped| {
                *r = clipped;
                true
            })
        });
    }
    Some(list)
}

/// Intersects a D3D9 `RECT` `(x1, y1, x2, y2)` with the viewport `(x, y, w, h)`.
///
/// The rect is half-open, top-left origin. Returns the overlap as `(x, y, w, h)`, or `None` if
/// the rect is inverted or degenerate or the overlap is empty. Turns each `Clear` rect into a
/// region inside the viewport.
#[must_use]
pub fn clip_rect_to_viewport(
    rect: (i32, i32, i32, i32),
    vp: (u32, u32, u32, u32),
) -> Option<(u32, u32, u32, u32)> {
    let (rx1, ry1, rx2, ry2) = rect;
    if rx2 <= rx1 || ry2 <= ry1 {
        return None;
    }
    let (vx, vy, vw, vh) = vp;
    let vx2 = vx.saturating_add(vw);
    let vy2 = vy.saturating_add(vh);
    let x1 = rx1.max(0).cast_unsigned().max(vx);
    let y1 = ry1.max(0).cast_unsigned().max(vy);
    let x2 = rx2.max(0).cast_unsigned().min(vx2);
    let y2 = ry2.max(0).cast_unsigned().min(vy2);
    if x2 <= x1 || y2 <= y1 {
        return None;
    }
    Some((x1, y1, x2 - x1, y2 - y1))
}

/// `rect` (`x, y, width, height`) cut to an attachment of `extent`.
///
/// `None` when nothing of it is left: Metal takes no scissor past its pass's extent, nor an
/// empty one.
#[must_use]
pub const fn clamp_rect(rect: [u32; 4], extent: (u32, u32)) -> Option<[u32; 4]> {
    const fn least(a: u32, b: u32) -> u32 {
        if a < b { a } else { b }
    }
    let [x, y, width, height] = rect;
    let (x, y) = (least(x, extent.0), least(y, extent.1));
    let (width, height) = (least(width, extent.0 - x), least(height, extent.1 - y));
    if width == 0 || height == 0 {
        None
    } else {
        Some([x, y, width, height])
    }
}

/// What a `Clear` clears of one attachment.
#[derive(Debug, PartialEq, Eq)]
pub enum ClearArea {
    /// All of it.
    Whole,
    /// These regions (`x, y, width, height`), each inside it and none empty; nothing for none.
    Regions(Vec<[u32; 4]>),
}

/// What a `Clear` of `regions` through `viewport` clears of an attachment of `extent`.
///
/// `regions` is [`clear_regions`]' answer and `viewport` the drawn viewport (`x, y, w, h`).
/// Each region is clipped to the viewport (none: the viewport itself); one that covers the
/// attachment from its origin makes the clear whole, as mtld3d's encoder decides it per
/// attachment; the others are cut to the attachment. A depth-stencil is judged against its own
/// extent, which may not be the colour target's.
#[must_use]
pub fn clear_area(
    regions: Option<&[(i32, i32, i32, i32)]>,
    viewport: (u32, u32, u32, u32),
    extent: (u32, u32),
) -> ClearArea {
    let clipped = regions.map_or_else(
        || vec![viewport],
        |list| {
            list.iter()
                .filter_map(|&rect| clip_rect_to_viewport(rect, viewport))
                .collect()
        },
    );
    if clipped
        .iter()
        .any(|&region| region_covers_extent(region, extent))
    {
        return ClearArea::Whole;
    }
    ClearArea::Regions(
        clipped
            .into_iter()
            .filter_map(|region| clamp_rect(region.into(), extent))
            .collect(),
    )
}

#[cfg(test)]
mod tests;
