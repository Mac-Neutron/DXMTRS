//! EVENT and OCCLUSION queries on the core (DXMTRS C4c-5).
//!
//! An EVENT's END is a chunk's sequence number, done once the queue retired that chunk. An
//! OCCLUSION bracket is a `VisibilityQuery` of its own a BEGIN, begun and ended by closures in the
//! chunk; the core's encoder sums, chunk by chunk, the samples the draws between them passed, and
//! a poll reads the sum once the END's chunk is issued. Only an application's draws count: the
//! draw closure bumps the result offset and no pass of the device's own does. From the device's
//! first BEGIN on, each draw pass and each triangle pass of the device's own starts with counting
//! off, since the core's optimizer may put a counting pass's records in front of it.

use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dxmt_core::{
    encoder::{Encoder, NO_NEXT},
    query::VisibilityQuery,
};
use mtld3d_core::{
    query_fence::{event_completed, event_needs_submit},
    visibility::logical_samples,
};
use winemetal_sys::wire::{
    WMT_RENDER_COMMAND_SET_VISIBILITY_MODE, WMT_VISIBILITY_RESULT_MODE_DISABLED,
    WmtcmdRenderSetvisibilitymode,
};

use super::CoreDevice;

/// Render vs logical area ratio captured for an open occlusion bracket on its counted draws.
#[derive(Default)]
pub(super) struct QueryScale {
    render_area: AtomicU64,
    logical_area: AtomicU64,
}

impl QueryScale {
    fn record(&self, render_area: u64, logical_area: u64) {
        self.render_area.store(render_area, Ordering::Relaxed);
        self.logical_area.store(logical_area, Ordering::Relaxed);
    }

    fn areas(&self) -> (u64, u64) {
        (
            self.render_area.load(Ordering::Relaxed),
            self.logical_area.load(Ordering::Relaxed),
        )
    }
}

/// An OCCLUSION query's core state, which the front end's query owns.
///
/// The bracket it last began: a `VisibilityQuery` of its own a BEGIN, since a chunk still to be
/// issued may hold the one before (a read-back of the results it covers), which a BEGIN on it
/// would reset.
#[derive(Default)]
pub struct Occlusion {
    span: Span,
}

/// What [`CoreDevice::occlusion_poll`] answers.
#[derive(Debug, PartialEq, Eq)]
pub enum OcclusionAnswer {
    /// Never issued.
    Never,
    /// Open, or ended and its samples not yet read back.
    Pending,
    /// The samples the draws between its BEGIN and its END passed.
    Count(u64),
}

impl CoreDevice {
    /// The stamp of an EVENT query's END: the sequence number of the chunk being recorded.
    ///
    /// Never 0 (the queue's first chunk is 1), which the front end keeps for "never issued".
    #[must_use]
    pub fn event_stamp(&self) -> u64 {
        self.device.queue().current_seq()
    }

    /// Whether the work recorded before the EVENT END stamped `seq` is done.
    ///
    /// Done once the queue retired chunk `seq`: the GPU finished it and its read-backs ran; `seq`
    /// 0, never issued, is done. While `seq` is the chunk being recorded, the call commits it
    /// first, with `D3DGETDATA_FLUSH` or without, as mtld3d does, so a title that polls without
    /// the flag and without `Present` finishes. The open render pass ends with that commit; a
    /// chunk with nothing in it is committed like any other.
    #[must_use]
    pub fn event_done(&mut self, seq: u64) -> bool {
        if event_completed(seq, self.device.queue().coherent_seq()) {
            return true;
        }
        if !event_needs_submit(seq, self.device.queue().current_seq()) {
            return false;
        }
        self.commit();
        event_completed(seq, self.device.queue().coherent_seq())
    }

    /// Begins an OCCLUSION bracket: `D3DISSUE_BEGIN`.
    ///
    /// An open bracket is ended first, its END recorded (the core's encoder counts its active
    /// queries, and a BEGIN without its END would count on every later pass). The new bracket is
    /// a fresh `VisibilityQuery`, begun by a closure in the chunk being recorded. From the
    /// device's first BEGIN on, its draw passes start with counting off ([`CoreDevice::draw`]).
    pub fn occlusion_begin(&mut self, occlusion: &mut Occlusion) {
        self.occlusion_release(occlusion);
        let query = Arc::new(VisibilityQuery::new());
        let scale = Arc::new(QueryScale::default());
        let (render_area, logical_area) = self.target_areas();
        scale.record(render_area, logical_area);
        self.open_query_scales
            .retain(|entry| entry.strong_count() > 0);
        self.open_query_scales.push(Arc::downgrade(&scale));
        let begun = Arc::clone(&query);
        self.emit(move |enc| enc.begin_visibility_query(Arc::clone(&begun)));
        self.open_brackets = Some(self.open_brackets.map_or(1, |open| open + 1));
        occlusion.span = Span::Open { query, scale };
    }

    /// Ends an OCCLUSION bracket: `D3DISSUE_END`.
    ///
    /// An open bracket's END is recorded in the chunk being recorded, whose sequence number a poll
    /// waits for. A bracket never begun becomes an empty span that answers 0, with nothing
    /// recorded; one already ended keeps its result. Both as mtld3d's.
    pub fn occlusion_end(&mut self, occlusion: &mut Occlusion) {
        occlusion.span = match core::mem::take(&mut occlusion.span) {
            Span::Open { query, scale } => {
                self.open_query_scales
                    .retain(|entry| entry.upgrade().is_some_and(|s| !Arc::ptr_eq(&s, &scale)));
                let ended = Arc::clone(&query);
                self.emit(move |enc| enc.end_visibility_query(Arc::clone(&ended)));
                self.open_brackets = self.open_brackets.map(|open| open.saturating_sub(1));
                Span::Closed {
                    query,
                    scale,
                    end_seq: self.device.queue().current_seq(),
                }
            }
            Span::Never => Span::Empty,
            ended @ (Span::Closed { .. } | Span::Empty) => ended,
        };
    }

    /// The samples an OCCLUSION bracket counted, once they are read back.
    ///
    /// Rescaled by the bracket's recorded `logical_area / render_area` when targets are scaled.
    #[must_use]
    pub fn occlusion_poll(&mut self, occlusion: &Occlusion, flush: bool) -> OcclusionAnswer {
        let (query, scale, end_seq) = match &occlusion.span {
            Span::Never => return OcclusionAnswer::Never,
            Span::Open { .. } => return OcclusionAnswer::Pending,
            Span::Empty => return OcclusionAnswer::Count(0),
            Span::Closed {
                query,
                scale,
                end_seq,
            } => (query, scale, *end_seq),
        };
        if flush && query.value().is_none() {
            self.sync_to(end_seq);
        }
        // The END's chunk retired and read nothing back for this bracket: an empty range of it,
        // which adds nothing and marks the chunk issued (a read-back that ran issued it already).
        if query.value().is_none() && self.device.queue().coherent_seq() >= end_seq {
            query.issue(end_seq, &[]);
        }
        query.value().map_or(OcclusionAnswer::Pending, |count| {
            let (render_area, logical_area) = scale.areas();
            OcclusionAnswer::Count(logical_samples(count, render_area, logical_area))
        })
    }

    /// Ends an open OCCLUSION bracket: the query's final release.
    ///
    /// Its END is recorded as [`CoreDevice::occlusion_end`] records it: the core's encoder would
    /// otherwise count on every later pass for the device's life. Nothing for a bracket that is
    /// not open.
    pub fn occlusion_release(&mut self, occlusion: &mut Occlusion) {
        if matches!(occlusion.span, Span::Open { .. }) {
            self.occlusion_end(occlusion);
        }
    }

    /// Updates open occlusion query brackets with the active target's render/logical pixel areas.
    pub(super) fn record_open_query_areas(&mut self) {
        if self.open_query_scales.is_empty() {
            return;
        }
        let (render_area, logical_area) = self.target_areas();
        self.open_query_scales.retain(|entry| {
            entry.upgrade().is_some_and(|scale| {
                scale.record(render_area, logical_area);
                true
            })
        });
    }
}

/// Records counting off, at the head of a pass a counting pass may be merged in front of.
///
/// The core's optimizer puts an earlier render pass's records in front of a later pass's on the
/// same targets, and its encoder records a visibility mode only when a draw's result offset
/// changes, starting each recorded pass at "off": a pass with no query active records none, and
/// behind a counting pass its draws would count. Before any draw of the pass, whose bump records
/// what the encoder then believes. Metal takes the mode without a visibility buffer, as mtld3d
/// records it in such passes.
pub(super) fn visibility_off(enc: &mut Encoder) {
    enc.encode_render_command(WmtcmdRenderSetvisibilitymode {
        r#type: WMT_RENDER_COMMAND_SET_VISIBILITY_MODE,
        reserved: [0; 3],
        next: NO_NEXT,
        offset: 0,
        mode: WMT_VISIBILITY_RESULT_MODE_DISABLED,
    });
}

/// Where an OCCLUSION bracket stands.
#[derive(Default)]
enum Span {
    /// Never issued.
    #[default]
    Never,
    /// Begun, its END not yet recorded.
    Open {
        query: Arc<VisibilityQuery>,
        scale: Arc<QueryScale>,
    },
    /// Ended in chunk `end_seq`.
    Closed {
        query: Arc<VisibilityQuery>,
        scale: Arc<QueryScale>,
        end_seq: u64,
    },
    /// Ended without a BEGIN: an empty span, nothing recorded, which answers 0.
    Empty,
}

#[cfg(test)]
mod tests;
