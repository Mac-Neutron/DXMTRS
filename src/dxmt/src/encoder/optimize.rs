//! The flush's first step: clear and render passes merged into a later render pass.
//!
//! From the second-last pass back, a clear or render pass is compared with each pass after it
//! until one depends on it. Passes in between only stay where they are when neither waits for
//! the other. A clear becomes the load action of a later render pass that draws to its view, a
//! render pass is prepended to a later one with the same targets, and a resolve after a render
//! pass becomes its store action. Merged passes become [`PassKind::Null`]; none moves.
//!
//! Behaviour is the C++ `flushCommands`' first loop and `checkEncoderRelation`
//! (`dxmt_context.cpp:892-905, 1285-1602`).

use std::sync::Arc;

use winemetal_sys::wire::{
    WMT_LOAD_ACTION_CLEAR, WMT_LOAD_ACTION_LOAD, WMT_STORE_ACTION_DONT_CARE,
    WMT_STORE_ACTION_STORE, WMT_STORE_ACTION_STORE_AND_MULTISAMPLE_RESOLVE,
};

use super::{
    Encoder, NO_NEXT,
    pass::{Clear, Pass, PassKind, RenderTargets, Resolve, argument_buffer, same},
};

impl Encoder {
    /// Merges the chunk's passes where the C++ does, before they are encoded.
    pub(super) fn optimize(&mut self) {
        let count = self.passes.len();
        for j in (0..count.saturating_sub(1)).rev() {
            if !matches!(
                self.passes[j].kind,
                PassKind::Clear(_) | PassKind::Render(_)
            ) {
                continue;
            }
            for i in j + 1..count {
                let (former, latter) = self.passes.split_at_mut(i);
                if matches!(latter[0].kind, PassKind::Null) {
                    continue;
                }
                if relation(&mut former[j], &mut latter[0], &mut self.frame_stats) {
                    break;
                }
            }
        }
    }
}

/// What a comparison merged.
enum Merged {
    Nothing,
    /// The former pass went into the latter.
    Former,
    /// The latter (a resolve) went into the former.
    Latter,
}

/// Compares `former` with a pass after it, merging what can be.
///
/// True when they keep their order (the C++'s `DXMT_ENCODER_LIST_OP_SYNCHRONIZE`), false when
/// the scan goes on past `latter`.
fn relation(
    former: &mut Pass,
    latter: &mut Pass,
    stats: &mut crate::frame_stats::FrameStatistics,
) -> bool {
    if matches!(former.kind, PassKind::Null) || matches!(latter.kind, PassKind::Null) {
        return false;
    }
    if keeps_order(&former.kind) || keeps_order(&latter.kind) {
        return true;
    }
    let vertex_waits_for_former = latter.fence_wait_vertex.intersects(&former.fence_update);
    let merged = match (&mut former.kind, &mut latter.kind) {
        (PassKind::Clear(clear), PassKind::Render(targets)) => {
            if clear_into_render(clear, targets) {
                Merged::Former
            } else {
                Merged::Nothing
            }
        }
        (PassKind::Render(targets), PassKind::Clear(clear)) => {
            drop_cleared_stores(targets, clear);
            Merged::Nothing
        }
        (PassKind::Render(targets), PassKind::Resolve(resolve)) => {
            if resolve_into_render(targets, resolve) {
                Merged::Latter
            } else {
                Merged::Nothing
            }
        }
        (PassKind::Render(r0), PassKind::Render(r1)) => {
            if !vertex_waits_for_former
                && same_targets(r0, r1)
                && argument_buffer(former.arguments.as_ref())
                    == argument_buffer(latter.arguments.as_ref())
            {
                take_targets(r0, r1);
                Merged::Former
            } else {
                Merged::Nothing
            }
        }
        _ => Merged::Nothing,
    };
    match merged {
        Merged::Former => {
            if matches!(former.kind, PassKind::Render(_)) {
                render_into_render(former, latter);
                stats.render_pass_optimized += 1;
            } else {
                absorb(latter, former);
                stats.clear_pass_optimized += 1;
            }
            former.kind = PassKind::Null;
            true
        }
        Merged::Latter => {
            absorb(former, latter);
            latter.kind = PassKind::Null;
            stats.resolve_pass_optimized += 1;
            false
        }
        Merged::Nothing => depends(former, latter),
    }
}

/// Events and timestamps order every pass around them.
const fn keeps_order(kind: &PassKind) -> bool {
    matches!(
        kind,
        PassKind::SignalEvent(..) | PassKind::WaitForEvent(..) | PassKind::SampleTimestamp(_)
    )
}

/// Whether either pass updates a fence the other waits for (`hasDataDependency`).
///
/// A pass other than a render pass has no vertex half: its vertex sets are empty.
fn depends(former: &Pass, latter: &Pass) -> bool {
    let wait_former = former.fence_wait.union(&former.fence_wait_vertex);
    let update_former = former.fence_update_vertex.union(&former.fence_update);
    let wait_latter = latter.fence_wait.union(&latter.fence_wait_vertex);
    let update_latter = latter.fence_update_vertex.union(&latter.fence_update);
    update_former.intersects(&wait_latter) || update_latter.intersects(&wait_former)
}

/// `from`'s updates and waits taken by `into`, which no longer waits for `from`.
fn absorb(into: &mut Pass, from: &Pass) {
    into.fence_update.merge(&from.fence_update);
    into.fence_wait.merge(&from.fence_wait);
    into.fence_wait.subtract(&from.fence_update);
}

/// The render pass `r0` prepended to `r1`, after [`take_targets`] took its load actions.
///
/// `r1` takes its records, its emulated stream output and both halves' fences.
fn render_into_render(r0: &mut Pass, r1: &mut Pass) {
    if r0.records.tail != r0.records.head {
        // SAFETY: both lists are records of the encoder's memory, which lives until the chunk's
        // next reset (after the flush); each `next` written is a record of one of them.
        let r1_first = unsafe { (*r1.records.head.as_ptr()).next };
        // SAFETY: as above.
        let r0_first = unsafe { (*r0.records.head.as_ptr()).next };
        // SAFETY: as above.
        unsafe { (*r0.records.tail.as_ptr()).next = r1_first };
        // SAFETY: as above.
        unsafe { (*r1.records.head.as_ptr()).next = r0_first };
        // The C++ leaves `r1`'s tail at its head here; it is kept right so the list stays whole.
        if r1.records.tail == r1.records.head {
            r1.records.tail = r0.records.tail;
        }
        // SAFETY: as above.
        unsafe { (*r0.records.head.as_ptr()).next = NO_NEXT };
        r0.records.tail = r0.records.head;
    }
    r1.emulated_stream_output |= r0.emulated_stream_output;
    absorb(r1, r0);
    r1.fence_update_vertex.merge(&r0.fence_update_vertex);
    r1.fence_wait_vertex.merge(&r0.fence_wait_vertex);
    r1.fence_wait_vertex.subtract(&r0.fence_update_vertex);
    // As the C++'s, "just in case".
    r1.fence_wait.subtract(&r0.fence_update_vertex);
}

/// Whether `r1` draws to `r0`'s targets as `r0` left them (`isEncoderSignatureMatched`).
fn same_targets(r0: &RenderTargets, r1: &RenderTargets) -> bool {
    if r0.render_target_count != r1.render_target_count
        || r0.dsv_planar_flags != r1.dsv_planar_flags
        || r0.dsv_readonly_flags != r1.dsv_readonly_flags
        || r0.render_target_array_length != r1.render_target_array_length
    {
        return false;
    }
    let planes = [
        (&r0.depth.target, &r1.depth.target, 1),
        (&r0.stencil.target, &r1.stencil.target, 2),
    ];
    for (t0, t1, plane) in planes {
        if r0.dsv_planar_flags & plane == 0 {
            continue;
        }
        if !same(t0.attachment.as_ref(), t1.attachment.as_ref()) {
            return false;
        }
        let kept = if r0.dsv_readonly_flags & plane == 0 {
            t0.store_action == WMT_STORE_ACTION_STORE && t1.load_action == WMT_LOAD_ACTION_LOAD
        } else {
            t1.load_action != WMT_LOAD_ACTION_CLEAR
        };
        if !kept {
            return false;
        }
    }
    let count = usize::from(r0.render_target_count);
    r0.colors
        .iter()
        .zip(&r1.colors)
        .take(count)
        .all(|(a0, a1)| {
            let (t0, t1) = (&a0.target, &a1.target);
            same(t0.attachment.as_ref(), t1.attachment.as_ref())
                && t0.depth_plane == t1.depth_plane
                && (t0.attachment.is_none()
                    || (t0.store_action == WMT_STORE_ACTION_STORE
                        && t1.load_action == WMT_LOAD_ACTION_LOAD))
        })
}

/// `r1` loads, clears and stores as `r0` did, and uses what `r0` used.
fn take_targets(r0: &mut RenderTargets, r1: &mut RenderTargets) {
    let count = usize::from(r0.render_target_count);
    for (a0, a1) in r0.colors.iter().zip(&mut r1.colors).take(count) {
        a1.target.load_action = a0.target.load_action;
        a1.clear_color = a0.clear_color;
    }
    r1.depth.target.load_action = r0.depth.target.load_action;
    r1.depth.clear_depth = r0.depth.clear_depth;
    r1.depth.target.store_action = r0.depth.target.store_action;
    r1.stencil.target.load_action = r0.stencil.target.load_action;
    r1.stencil.clear_stencil = r0.stencil.clear_stencil;
    r1.stencil.target.store_action = r0.stencil.target.store_action;
    r1.uses |= r0.uses;
    if !r0.gs_marshal_tasks.is_empty() {
        r0.gs_marshal_tasks.append(&mut r1.gs_marshal_tasks);
        core::mem::swap(&mut r0.gs_marshal_tasks, &mut r1.gs_marshal_tasks);
    }
    if !r0.ts_marshal_tasks.is_empty() {
        r0.ts_marshal_tasks.append(&mut r1.ts_marshal_tasks);
        core::mem::swap(&mut r0.ts_marshal_tasks, &mut r1.ts_marshal_tasks);
    }
}

/// `clear` as the load action of `render`'s target that is its view.
///
/// True when nothing of the clear is left: a depth-stencil clear can keep the plane `render` does
/// not draw to.
fn clear_into_render(clear: &mut Clear, render: &mut RenderTargets) -> bool {
    if u32::from(render.render_target_array_length) != clear.array_length {
        return false;
    }
    let Some(view) = clear.attachment.as_ref() else {
        return false;
    };
    let is_view = |a: Option<&_>| same(a, Some(view));
    if clear.planes == 0 {
        let count = usize::from(render.render_target_count);
        let Some(color) = render
            .colors
            .iter_mut()
            .take(count)
            .find(|c| is_view(c.target.attachment.as_ref()))
        else {
            return false;
        };
        if color.target.load_action == WMT_LOAD_ACTION_LOAD {
            color.target.load_action = WMT_LOAD_ACTION_CLEAR;
            color.clear_color = clear.color;
        }
        return true;
    }
    if clear.planes & 1 != 0 && is_view(render.depth.target.attachment.as_ref()) {
        if render.depth.target.load_action == WMT_LOAD_ACTION_LOAD {
            render.depth.clear_depth = clear.depth;
            render.depth.target.load_action = WMT_LOAD_ACTION_CLEAR;
        }
        clear.planes &= !1;
    }
    if clear.planes & 2 != 0 && is_view(render.stencil.target.attachment.as_ref()) {
        if render.stencil.target.load_action == WMT_LOAD_ACTION_LOAD {
            render.stencil.clear_stencil = clear.stencil;
            render.stencil.target.load_action = WMT_LOAD_ACTION_CLEAR;
        }
        clear.planes &= !2;
    }
    clear.planes == 0
}

/// A depth or stencil target a later clear clears need not be stored.
///
/// `DontCare`, not `DontStore`: it is cleared anyway.
fn drop_cleared_stores(render: &mut RenderTargets, clear: &Clear) {
    let Some(view) = clear.attachment.as_ref() else {
        return;
    };
    for (target, plane) in [
        (&mut render.depth.target, 1),
        (&mut render.stencil.target, 2),
    ] {
        if clear.planes & plane != 0 && same(target.attachment.as_ref(), Some(view)) {
            target.store_action = WMT_STORE_ACTION_DONT_CARE;
        }
    }
}

/// `resolve` as the store action of `render`'s colour target that draws to its source.
///
/// The resolved view is taken in that target's format (`isResolveSignatureMatched`). True when
/// merged.
fn resolve_into_render(render: &mut RenderTargets, resolve: &mut Resolve) -> bool {
    let (Some(src), Some(_)) = (&resolve.src, &resolve.dst) else {
        return false;
    };
    let count = usize::from(render.render_target_count);
    for color in render.colors.iter_mut().take(count) {
        let target = &mut color.target;
        let Some(a) = &target.attachment else {
            continue;
        };
        if target.store_action != WMT_STORE_ACTION_STORE
            || color.resolve.is_some()
            || !Arc::ptr_eq(&a.allocation, &src.allocation)
        {
            continue;
        }
        let format = a.key.format();
        let in_color_format = if a.key == src.key {
            false
        } else if a.key == src.key.with_format(format) {
            true
        } else {
            continue;
        };
        let Some(mut dst) = resolve.dst.take() else {
            return false;
        };
        if in_color_format {
            dst.key = dst.key.with_format(format);
            dst.view = dst.allocation.view(dst.key).texture;
        }
        target.store_action = WMT_STORE_ACTION_STORE_AND_MULTISAMPLE_RESOLVE;
        color.resolve = Some(dst);
        return true;
    }
    false
}

#[cfg(test)]
mod tests;
