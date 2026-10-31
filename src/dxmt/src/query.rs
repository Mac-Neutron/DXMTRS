//! Queries: occlusion (visibility results) and timestamps.
//!
//! The encoder keeps a [`Queries`]: a render pass's draws count samples into the chunk's
//! visibility-result buffer at an offset that moves whenever a query begins or ends after a
//! draw, so each query sums a range of results per chunk ([`VisibilityQuery`]: slots by offset,
//! polled by sequence, which D3D9's queries use as D3D11's). At the chunk's flush
//! [`Queries::readbacks`] hands over the chunk's [`Readbacks`], dropped on the finish thread once
//! the GPU finished the chunk, which issues the results to their queries. A timestamp query
//! reads the command buffer's GPU end time, never moving back.
//!
//! Behaviour is the C++'s (`dxmt_occlusion_query.hpp`, and the encoder's query calls in
//! `dxmt_context.cpp`). Written ahead for the switches of `d3d11.dll` and `d3d12.dll`, which are
//! its first callers.

use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use winemetal_sys::{
    MTLCommandBuffer_property, Obj,
    wire::{
        WMT_COMMAND_BUFFER_PROPERTY_GPU_END_TIME, WMT_RENDER_COMMAND_SET_VISIBILITY_MODE,
        WMT_VISIBILITY_RESULT_MODE_COUNTING, WMT_VISIBILITY_RESULT_MODE_DISABLED,
        WmtcmdRenderSetvisibilitymode,
    },
};

use crate::{
    encoder::NO_NEXT,
    resource::{Buffer, BufferAllocation, BufferAllocationFlags},
};

/// The write offset that turns counting off.
pub const NO_OFFSET: u64 = u64::MAX;
/// A value no query has: not begun, not ended, or (timestamps) not issued.
const NONE: u64 = u64::MAX;

/// Where a chunk's draws write their visibility results, one `u64` each.
///
/// The C++'s `VisibilityResultOffsetBumpState`.
pub struct VisibilityOffsets {
    flags: OffsetFlags,
    /// The offset the last write record set, or [`NO_OFFSET`].
    previous: u64,
    next: u64,
}

impl Default for VisibilityOffsets {
    fn default() -> Self {
        Self::new()
    }
}

impl VisibilityOffsets {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            flags: OffsetFlags::empty(),
            previous: NO_OFFSET,
            next: 0,
        }
    }

    /// A render pass starts.
    pub fn begin_pass(&mut self) {
        debug_assert!(self.flags.is_empty(), "a pass at a time");
        debug_assert!(self.previous == NO_OFFSET, "a pass at a time");
        self.flags.insert(OffsetFlags::IN_PASS);
    }

    /// The offset the next draw writes at, when it differs from the last one said.
    ///
    /// With no query active it is [`NO_OFFSET`]: counting off.
    pub fn next_write(&mut self, active: bool) -> Option<u64> {
        debug_assert!(
            self.flags.contains(OffsetFlags::IN_PASS),
            "a draw in a pass"
        );
        let offset = if active {
            self.flags.insert(OffsetFlags::DIRTY);
            self.next
        } else {
            NO_OFFSET
        };
        if offset == self.previous {
            return None;
        }
        self.previous = offset;
        Some(offset)
    }

    /// The offset a query that begins or ends now starts or stops at.
    ///
    /// After a draw wrote the current result, it moves to the next one.
    pub fn next_read(&mut self) -> u64 {
        if self
            .flags
            .contains(OffsetFlags::IN_PASS | OffsetFlags::DIRTY)
        {
            self.flags.remove(OffsetFlags::DIRTY);
            self.next += 1;
            return self.next;
        }
        debug_assert!(
            !self.flags.contains(OffsetFlags::DIRTY),
            "dirty only in a pass"
        );
        self.next
    }

    /// The render pass ends.
    pub fn end_pass(&mut self) {
        debug_assert!(self.flags.contains(OffsetFlags::IN_PASS), "a pass to end");
        self.previous = NO_OFFSET;
        if self.flags.contains(OffsetFlags::DIRTY) {
            self.next += 1;
        }
        self.flags = OffsetFlags::empty();
    }

    /// The chunk's result count; the next chunk starts at 0.
    pub fn reset(&mut self) -> u64 {
        debug_assert!(self.flags.is_empty(), "the chunk ends between passes");
        core::mem::take(&mut self.next)
    }
}

/// An occlusion query: the samples its draws passed, summed over the chunks it spans.
///
/// The encode thread begins and ends it, the finish thread issues each chunk's results, the
/// front end polls [`Self::value`]; one lock keeps each of those whole.
pub struct VisibilityQuery {
    state: Mutex<Visibility>,
}

impl Default for VisibilityQuery {
    fn default() -> Self {
        Self::new()
    }
}

impl VisibilityQuery {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(Visibility::IDLE),
        }
    }

    /// Begins at result `offset` of chunk `seq`.
    pub fn begin(&self, seq: u64, offset: u64) {
        let mut s = self.lock();
        *s = Visibility {
            seq_begin: seq,
            offset_begin: offset,
            seq_issued: s.seq_issued,
            ..Visibility::IDLE
        };
    }

    /// Ends at result `offset` of chunk `seq`; ready at once when nothing was drawn.
    pub fn end(&self, seq: u64, offset: u64) {
        let mut s = self.lock();
        s.seq_end = seq;
        s.offset_end = offset;
        if s.seq_begin == seq && s.offset_begin == offset {
            s.seq_issued = seq;
        }
    }

    /// The chunk it ended in, or `u64::MAX` while it runs.
    #[must_use]
    pub fn end_at(&self) -> u64 {
        self.lock().seq_end
    }

    /// Adds chunk `seq`'s results it covers.
    pub fn issue(&self, seq: u64, results: &[u64]) {
        let mut s = self.lock();
        debug_assert!(
            seq >= s.seq_begin && seq <= s.seq_end,
            "a chunk the query spans"
        );
        let start = if seq == s.seq_begin {
            s.offset_begin
        } else {
            0
        };
        let end = if seq == s.seq_end {
            s.offset_end
        } else {
            results.len() as u64
        };
        debug_assert!(start <= end, "a range of results");
        let range = usize::try_from(start).ok().zip(usize::try_from(end).ok());
        let covered = range
            .and_then(|(a, b)| results.get(a..b))
            .unwrap_or_default();
        s.accumulated = covered
            .iter()
            .fold(s.accumulated, |sum, r| sum.wrapping_add(*r));
        s.seq_issued = seq;
    }

    /// The samples passed, once every chunk it spans is issued.
    #[must_use]
    pub fn value(&self) -> Option<u64> {
        let s = self.lock();
        (s.seq_end <= s.seq_issued).then_some(s.accumulated)
    }

    /// Forgets the last use.
    pub fn reset(&self) {
        *self.lock() = Visibility::IDLE;
    }

    fn lock(&self) -> MutexGuard<'_, Visibility> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A chunk's visibility-result buffer, read back into its queries when it drops.
pub struct VisibilityReadback {
    seq: u64,
    results: usize,
    queries: Vec<Arc<VisibilityQuery>>,
    allocation: Arc<BufferAllocation>,
}

impl VisibilityReadback {
    /// The buffer of `results` results chunk `seq`'s passes write, for `queries`.
    ///
    /// An i686 process places it in its own memory, which its 32-bit pointers reach.
    #[must_use]
    pub fn new(device: &Obj, seq: u64, results: u64, queries: Vec<Arc<VisibilityQuery>>) -> Self {
        let flags = if cfg!(target_arch = "x86") {
            BufferAllocationFlags::CPU_PLACED
        } else {
            BufferAllocationFlags::empty()
        };
        let allocation = Buffer::new(results.saturating_mul(8), device).allocate(flags);
        Self {
            seq,
            results: usize::try_from(results).unwrap_or(0),
            queries,
            allocation,
        }
    }
}

impl Drop for VisibilityReadback {
    /// Issues the results: the GPU finished the chunk before its readbacks drop.
    fn drop(&mut self) {
        let mapped = self.allocation.mapped_memory(0);
        let bytes = if mapped.is_null() {
            &[][..]
        } else {
            // SAFETY: the buffer's `results` results of 8 bytes, written by the GPU before the
            // chunk was retired and by nothing since.
            unsafe { core::slice::from_raw_parts(mapped, self.results * 8) }
        };
        let results: Vec<u64> = bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|r| u64::from_ne_bytes(*r))
            .collect();
        for query in &self.queries {
            query.issue(self.seq, &results);
        }
    }
}

/// A timestamp query: the GPU time the command buffer that sampled it ended at.
pub struct TimestampQuery {
    value: AtomicU64,
}

impl Default for TimestampQuery {
    fn default() -> Self {
        Self::new()
    }
}

impl TimestampQuery {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            value: AtomicU64::new(NONE),
        }
    }

    /// The time, once the command buffer finished.
    #[must_use]
    pub fn value(&self) -> Option<u64> {
        let value = self.value.load(Ordering::Acquire);
        (value != NONE).then_some(value)
    }

    fn issue(&self, time: u64) {
        self.value.store(time, Ordering::Release);
    }
}

/// A chunk's timestamp queries, issued when it drops.
///
/// The C++'s `TimestampReadbackCBuf`, the form it compiles: every query of the chunk reads the
/// command buffer's GPU end time.
pub struct TimestampReadback {
    queries: Vec<Arc<TimestampQuery>>,
    cmdbuf: Option<Obj>,
    latest: Arc<AtomicU64>,
}

impl Drop for TimestampReadback {
    fn drop(&mut self) {
        let end = self.cmdbuf.as_ref().map_or(0, |cmdbuf| {
            // SAFETY: a live command buffer the readback holds, finished.
            unsafe {
                MTLCommandBuffer_property(cmdbuf.handle(), WMT_COMMAND_BUFFER_PROPERTY_GPU_END_TIME)
            }
        });
        issue_timestamps(&self.queries, &self.latest, end);
    }
}

/// What a chunk reads back once the GPU finished it: dropping it issues its queries.
#[derive(Default)]
pub struct Readbacks {
    visibility: Option<VisibilityReadback>,
    /// Held for its drop, which issues the timestamps.
    _timestamp: Option<TimestampReadback>,
}

impl Readbacks {
    /// The buffer the chunk's render passes write visibility results to, or the null handle.
    #[must_use]
    pub fn visibility_buffer(&self) -> u64 {
        self.visibility
            .as_ref()
            .map_or(0, |readback| readback.allocation.buffer())
    }
}

/// The encoder's query state: offsets, active and pending queries, timestamps to sample.
pub struct Queries {
    offsets: VisibilityOffsets,
    /// Begun and not yet read back in the chunk they ended in.
    pending: Vec<Arc<VisibilityQuery>>,
    active: u32,
    timestamps: Vec<Arc<TimestampQuery>>,
    /// The latest GPU end time issued, so a later query never reads an earlier time.
    latest: Arc<AtomicU64>,
    /// The queries of the command lists being replayed, innermost last.
    deferred: Vec<Arc<[Arc<VisibilityQuery>]>>,
}

impl Default for Queries {
    fn default() -> Self {
        Self::new()
    }
}

impl Queries {
    #[must_use]
    pub fn new() -> Self {
        Self {
            offsets: VisibilityOffsets::new(),
            pending: Vec::new(),
            active: 0,
            timestamps: Vec::new(),
            latest: Arc::new(AtomicU64::new(0)),
            deferred: Vec::new(),
        }
    }

    /// A render pass starts.
    pub fn begin_pass(&mut self) {
        self.offsets.begin_pass();
    }

    /// The render pass ends.
    pub fn end_pass(&mut self) {
        self.offsets.end_pass();
    }

    /// Begins `query` in chunk `seq`.
    pub fn begin_visibility(&mut self, seq: u64, query: Arc<VisibilityQuery>) {
        query.begin(seq, self.offsets.next_read());
        self.active += 1;
        self.pending.push(query);
    }

    /// Ends `query` in chunk `seq`.
    pub fn end_visibility(&mut self, seq: u64, query: &VisibilityQuery) {
        query.end(seq, self.offsets.next_read());
        debug_assert!(self.active > 0, "an active query to end");
        self.active = self.active.saturating_sub(1);
    }

    /// Whether a query is active: a draw now makes its pass use the visibility buffer.
    #[must_use]
    pub const fn visibility_active(&self) -> bool {
        self.active > 0
    }

    /// The record a draw is preceded by when its result offset changed (offsets in bytes).
    pub fn visibility_mode(&mut self) -> Option<WmtcmdRenderSetvisibilitymode> {
        let offset = self.offsets.next_write(self.visibility_active())?;
        let (mode, offset) = if offset == NO_OFFSET {
            (WMT_VISIBILITY_RESULT_MODE_DISABLED, 0)
        } else {
            (WMT_VISIBILITY_RESULT_MODE_COUNTING, offset << 3)
        };
        Some(WmtcmdRenderSetvisibilitymode {
            r#type: WMT_RENDER_COMMAND_SET_VISIBILITY_MODE,
            reserved: [0; 3],
            next: NO_NEXT,
            offset,
            mode,
        })
    }

    /// Samples the GPU time into `query` at this point of the chunk.
    pub fn sample_timestamp(&mut self, query: Arc<TimestampQuery>) {
        self.timestamps.push(query);
    }

    /// Makes `queries` the replayed command list's, until [`Self::pop_deferred`].
    pub fn push_deferred(&mut self, queries: Arc<[Arc<VisibilityQuery>]>) {
        self.deferred.push(queries);
    }

    pub fn pop_deferred(&mut self) {
        debug_assert!(!self.deferred.is_empty(), "a command list to leave");
        self.deferred.pop();
    }

    /// Query `id` of the command list being replayed.
    #[must_use]
    pub fn deferred(&self, id: usize) -> Option<Arc<VisibilityQuery>> {
        self.deferred.last()?.get(id).cloned()
    }

    /// Chunk `seq`'s readbacks; its command buffer is `cmdbuf` (borrowed).
    pub fn readbacks(&mut self, device: &Obj, seq: u64, cmdbuf: u64) -> Readbacks {
        let visibility = self
            .take_visibility(seq)
            .map(|(results, queries)| VisibilityReadback::new(device, seq, results, queries));
        let timestamps = self.take_timestamps();
        let timestamp = (!timestamps.is_empty()).then(|| TimestampReadback {
            queries: timestamps,
            // SAFETY: the chunk's command buffer, live for the call, or the null handle.
            cmdbuf: unsafe { Obj::retain(cmdbuf) },
            latest: Arc::clone(&self.latest),
        });
        Readbacks {
            visibility,
            _timestamp: timestamp,
        }
    }

    /// Chunk `seq`'s result count and the queries its results go to, if it wrote any.
    ///
    /// The queries that ended in it are no longer pending.
    fn take_visibility(&mut self, seq: u64) -> Option<(u64, Vec<Arc<VisibilityQuery>>)> {
        let results = self.offsets.reset();
        let taken = (results > 0).then(|| (results, self.pending.clone()));
        self.pending.retain(|query| query.end_at() != seq);
        taken
    }

    fn take_timestamps(&mut self) -> Vec<Arc<TimestampQuery>> {
        core::mem::take(&mut self.timestamps)
    }
}

bitflags::bitflags! {
    /// Where [`VisibilityOffsets`] is: in a pass, and whether a draw wrote the current result.
    struct OffsetFlags: u8 {
        const IN_PASS = 1;
        const DIRTY = 1 << 1;
    }
}

/// A [`VisibilityQuery`]'s chunks, result offsets and sum.
struct Visibility {
    accumulated: u64,
    seq_begin: u64,
    offset_begin: u64,
    seq_end: u64,
    offset_end: u64,
    seq_issued: u64,
}

impl Visibility {
    const IDLE: Self = Self {
        accumulated: 0,
        seq_begin: NONE,
        offset_begin: NONE,
        seq_end: NONE,
        offset_end: NONE,
        seq_issued: 0,
    };
}

/// Issues `end`, or the latest time issued before if that is later, to `queries`.
fn issue_timestamps(queries: &[Arc<TimestampQuery>], latest: &AtomicU64, end: u64) {
    let time = latest.fetch_max(end, Ordering::Relaxed).max(end);
    for query in queries {
        query.issue(time);
    }
}

#[cfg(test)]
mod tests;
