//! EVENT and OCCLUSION queries on the core (DXMTRS C4c-5).
//!
//! An EVENT's END is a chunk's sequence number, done once the queue retired that chunk. An
//! OCCLUSION bracket is a `VisibilityQuery` of its own a BEGIN, begun and ended by closures in the
//! chunk; the core's encoder sums, chunk by chunk, the samples the draws between them passed, and
//! a poll reads the sum once the END's chunk is issued. Only an application's draws count: the
//! draw closure bumps the result offset and no pass of the device's own does. From the device's
//! first BEGIN on, each draw pass and each triangle pass of the device's own starts with counting
//! off, since the core's optimizer may put a counting pass's records in front of it.

use std::sync::Arc;

use dxmt_core::{
    encoder::{Encoder, NO_NEXT},
    query::VisibilityQuery,
};
use mtld3d_core::query_fence::{event_completed, event_needs_submit};
use winemetal_sys::wire::{
    WMT_RENDER_COMMAND_SET_VISIBILITY_MODE, WMT_VISIBILITY_RESULT_MODE_DISABLED,
    WmtcmdRenderSetvisibilitymode,
};

use super::CoreDevice;

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
        let begun = Arc::clone(&query);
        self.emit(move |enc| enc.begin_visibility_query(Arc::clone(&begun)));
        self.open_brackets = Some(self.open_brackets.map_or(1, |open| open + 1));
        occlusion.span = Span::Open(query);
    }

    /// Ends an OCCLUSION bracket: `D3DISSUE_END`.
    ///
    /// An open bracket's END is recorded in the chunk being recorded, whose sequence number a poll
    /// waits for. A bracket never begun becomes an empty span that answers 0, with nothing
    /// recorded; one already ended keeps its result. Both as mtld3d's.
    pub fn occlusion_end(&mut self, occlusion: &mut Occlusion) {
        occlusion.span = match core::mem::take(&mut occlusion.span) {
            Span::Open(query) => {
                let ended = Arc::clone(&query);
                self.emit(move |enc| enc.end_visibility_query(Arc::clone(&ended)));
                self.open_brackets = self.open_brackets.map(|open| open.saturating_sub(1));
                Span::Closed {
                    query,
                    end_seq: self.device.queue().current_seq(),
                }
            }
            Span::Never => Span::Empty,
            ended @ (Span::Closed { .. } | Span::Empty) => ended,
        };
    }

    /// The samples an OCCLUSION bracket counted, once they are read back.
    ///
    /// Pending while it is open, `flush` or not. An ended bracket's samples are read back once
    /// the queue retired its END's chunk. With `flush` (`D3DGETDATA_FLUSH`) the call waits for
    /// that, committing the chunk when it is the one being recorded; without it nothing is
    /// committed, as mtld3d's poll without the flag submits nothing. dxmt-core reads a chunk back
    /// only when its passes wrote results, so an END alone in its chunk is marked issued here once
    /// that chunk retired: every earlier chunk's read-back ran before the queue retired it.
    #[must_use]
    pub fn occlusion_poll(&mut self, occlusion: &Occlusion, flush: bool) -> OcclusionAnswer {
        let (query, end_seq) = match &occlusion.span {
            Span::Never => return OcclusionAnswer::Never,
            Span::Open(_) => return OcclusionAnswer::Pending,
            Span::Empty => return OcclusionAnswer::Count(0),
            Span::Closed { query, end_seq } => (query, *end_seq),
        };
        if flush && query.value().is_none() {
            self.sync_to(end_seq);
        }
        // The END's chunk retired and read nothing back for this bracket: an empty range of it,
        // which adds nothing and marks the chunk issued (a read-back that ran issued it already).
        if query.value().is_none() && self.device.queue().coherent_seq() >= end_seq {
            query.issue(end_seq, &[]);
        }
        query
            .value()
            .map_or(OcclusionAnswer::Pending, OcclusionAnswer::Count)
    }

    /// Ends an open OCCLUSION bracket: the query's final release.
    ///
    /// Its END is recorded as [`CoreDevice::occlusion_end`] records it: the core's encoder would
    /// otherwise count on every later pass for the device's life. Nothing for a bracket that is
    /// not open.
    pub fn occlusion_release(&mut self, occlusion: &mut Occlusion) {
        if matches!(occlusion.span, Span::Open(_)) {
            self.occlusion_end(occlusion);
        }
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
    Open(Arc<VisibilityQuery>),
    /// Ended in chunk `end_seq`.
    Closed {
        query: Arc<VisibilityQuery>,
        end_seq: u64,
    },
    /// Ended without a BEGIN: an empty span, nothing recorded, which answers 0.
    Empty,
}

#[cfg(test)]
mod tests;
