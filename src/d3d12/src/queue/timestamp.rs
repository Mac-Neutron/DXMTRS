//! Timestamp counter sampling, folding, and shared sample buffer ring
//!
//! (`d3d12_command_queue.cpp:576-660, 1269-1355`).

use core::{
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use winemetal_sys::{Obj, wire::WMTSampleBufferAttachmentInfo};

use super::merge::AsSubmission;
use crate::{
    encoder::{EncoderData, EncoderSample, EncoderType},
    query_heap::TIMESTAMPS_PER_BUFFER,
};

/// Sentinel sample index (`MTLCounterDontSample = ~0ull`).
pub const DONT_SAMPLE: u64 = u64::MAX;

static RING_FULL_WARNED: AtomicBool = AtomicBool::new(false);

/// Reason a timestamp sample could not be folded into the next encoder (`LoneReason`,
///
/// `d3d12_command_queue.cpp:582`).
pub enum LoneReason {
    CounterBuffer = 0,
    Several = 1,
    NoStart = 2,
    End = 3,
}

impl Clone for LoneReason {
    fn clone(&self) -> Self {
        match self {
            Self::CounterBuffer => Self::CounterBuffer,
            Self::Several => Self::Several,
            Self::NoStart => Self::NoStart,
            Self::End => Self::End,
        }
    }
}

/// Borrowed in-flight ring sampling fields updated by [`TimestampState::point`],
///
/// [`TimestampState::take_start`], and [`TimestampState::attach`].
pub struct RingSampling<'a> {
    pub samples: u64,
    pub ring_done: u64,
    pub sample_first: &'a mut u64,
    pub sample_count: &'a mut u32,
    pub sample_values: &'a mut Vec<(u32, *mut u64)>,
}

/// Per-queue timestamp folding and ring-sampling state
///
/// (`d3d12_command_queue.cpp:578-617, 1276-1355`).
pub struct TimestampState {
    pub pending_samples: Vec<EncoderSample>,
    pub pending_join: bool,
    pub ring: Option<Obj>,
    pub ring_failed: bool,
    pub ring_next: u64,
    pub taken: bool,
    pub taken_stop: *mut EncoderData,
    pub ends: Vec<EncoderSample>,
}

impl TimestampState {
    /// Creates a [`TimestampState`] with pre-allocated scratch vectors.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending_samples: Vec::with_capacity(64),
            pending_join: false,
            ring: None,
            ring_failed: false,
            ring_next: 0,
            taken: false,
            taken_stop: ptr::null_mut(),
            ends: Vec::with_capacity(64),
        }
    }

    /// Consumes pending samples for the next encoder whose own counter sample buffer is `own`
    ///
    /// (`TakePending`, `d3d12_command_queue.cpp:588-602`).
    ///
    /// Calls `flush_one(buffer, index, reason)` for each sample that must be emitted in its own
    /// lone blit encoder, and returns `(buffer, start_index)` for the sample folded into the next
    /// encoder's start (or `(0, DONT_SAMPLE)` when none fits).
    pub fn take_pending(
        &mut self,
        own: u64,
        mut flush_one: impl FnMut(u64, u64, LoneReason),
    ) -> (u64, u64) {
        let Some(last) = self.pending_samples.last().cloned() else {
            return (0, DONT_SAMPLE);
        };
        let fits = own == 0 || own == last.buffer;
        let len = self.pending_samples.len();
        let flush_count = if fits { len - 1 } else { len };
        for k in 0..flush_count {
            let sample = &self.pending_samples[k];
            let reason = if k + 1 < len {
                LoneReason::Several
            } else {
                LoneReason::CounterBuffer
            };
            flush_one(sample.buffer, u64::from(sample.index), reason);
        }
        self.pending_samples.clear();
        if !fits {
            return (0, DONT_SAMPLE);
        }
        crate::count!("#timestamps at the next encoder's start", 1);
        (last.buffer, u64::from(last.index))
    }

    /// Allocates one slot in the shared counter sample buffer ring for `samples` (`Point`,
    ///
    /// `d3d12_command_queue.cpp:1287-1305`).
    ///
    /// Returns `(slot, newly_owed)`.
    pub fn point(&mut self, samples: &[EncoderSample], ring: &mut RingSampling<'_>) -> (u64, bool) {
        allocate_ring_slot(&mut self.ring_next, samples, ring)
    }

    /// Allocates one slot in the shared counter sample buffer ring for `self.ends`.
    pub fn point_ends(&mut self, ring: &mut RingSampling<'_>) -> (u64, bool) {
        allocate_ring_slot(&mut self.ring_next, &self.ends, ring)
    }

    /// Allocates one slot in the shared counter sample buffer ring for `self.pending_samples`.
    pub fn point_pending(&mut self, ring: &mut RingSampling<'_>) -> (u64, bool) {
        allocate_ring_slot(&mut self.ring_next, &self.pending_samples, ring)
    }

    /// Takes the waiting timestamps' slot in the shared ring for the start of the current encoder
    ///
    /// (`TakeStart`, `d3d12_command_queue.cpp:1309-1315`).
    pub fn take_start(&mut self, ring: &mut RingSampling<'_>) -> (u64, bool) {
        let (slot, owed) = allocate_ring_slot(&mut self.ring_next, &self.pending_samples, ring);
        if slot != DONT_SAMPLE {
            crate::count!("#timestamps at the next encoder's start", 1);
        }
        self.pending_samples.clear();
        (slot, owed)
    }

    /// Appends to `self.ends` the timestamp-only blits right after `e` when the encoder after them
    ///
    /// takes no start sample (`Trailing`, `d3d12_command_queue.cpp:1321-1342`).
    ///
    /// # Safety
    ///
    /// All encoder pointers reachable from `e` and `lists` must be valid for the call.
    pub unsafe fn trailing<S: AsSubmission>(
        &mut self,
        e: *mut EncoderData,
        mut l: usize,
        lists: &[S],
        folding: bool,
    ) {
        let mark = self.ends.len();
        // SAFETY: `e` is a valid encoder pointer in `lists[l]`.
        let mut cursor = unsafe { (*e).next };
        loop {
            if cursor.is_null() {
                l += 1;
                if l == lists.len() {
                    break;
                }
                cursor = lists[l].as_submission().entry;
            } else {
                // SAFETY: `cursor` is non-null and valid in `lists[l]`.
                let cur_ref = unsafe { &*cursor };
                if cur_ref.kind == EncoderType::Null {
                    cursor = cur_ref.next;
                } else if folding && cur_ref.kind == EncoderType::Blit && cur_ref.timestamp_only {
                    let count = usize::from(cur_ref.num_samples);
                    self.ends.extend_from_slice(&cur_ref.samples[..count]);
                    cursor = cur_ref.next;
                } else {
                    break;
                }
            }
        }
        if !cursor.is_null() {
            // SAFETY: `cursor` is non-null and valid in `lists[l]`.
            let cur_kind = unsafe { (*cursor).kind };
            if cur_kind != EncoderType::Clear && cur_kind != EncoderType::Resolve {
                self.ends.truncate(mark);
                return;
            }
        }
        if self.ends.len() > mark {
            self.taken = true;
            self.taken_stop = cursor;
        }
    }

    /// Builds a blit or compute encoder's single ring sample buffer attachment (`Attach`,
    ///
    /// `d3d12_command_queue.cpp:1346-1355`).
    ///
    /// # Safety
    ///
    /// All encoder pointers reachable from `e` and `lists` must be valid for the call.
    pub unsafe fn attach<S: AsSubmission>(
        &mut self,
        e: *mut EncoderData,
        l: usize,
        lists: &[S],
        folding: bool,
        ring: &mut RingSampling<'_>,
    ) -> (WMTSampleBufferAttachmentInfo, u32, bool) {
        let (start, owed_start) = self.take_start(ring);
        // SAFETY: `e` is a valid encoder pointer in `lists[l]`.
        let e_ref = unsafe { &*e };
        let own_count = usize::from(e_ref.num_samples);
        self.ends.clear();
        self.ends.extend_from_slice(&e_ref.samples[..own_count]);
        // SAFETY: forwarded caller's precondition.
        unsafe { self.trailing(e, l, lists, folding) };
        let (end, owed_end) = self.point_ends(ring);
        let att = WMTSampleBufferAttachmentInfo {
            sample_buffer: ring.samples,
            start_of_encoder_sample_index: start,
            end_of_encoder_sample_index: end,
        };
        let count = u32::from(start != DONT_SAMPLE || end != DONT_SAMPLE);
        (att, count, owed_start || owed_end)
    }
}

fn allocate_ring_slot(
    ring_next: &mut u64,
    samples: &[EncoderSample],
    ring: &mut RingSampling<'_>,
) -> (u64, bool) {
    if samples.is_empty() {
        return (DONT_SAMPLE, false);
    }
    let cap = u64::from(TIMESTAMPS_PER_BUFFER);
    if ring_next.wrapping_sub(ring.ring_done) == cap {
        if !RING_FULL_WARNED.swap(true, Ordering::Relaxed) {
            crate::warn!(
                "D3D12CommandQueue: over {cap} timestamps in flight; later ones keep old values"
            );
        }
        return (DONT_SAMPLE, false);
    }
    let newly_owed = if *ring.sample_count == 0 {
        *ring.sample_first = *ring_next;
        true
    } else {
        false
    };
    for s in samples {
        ring.sample_values.push((*ring.sample_count, s.value));
    }
    *ring.sample_count += 1;
    let slot = *ring_next % cap;
    *ring_next += 1;
    (slot, newly_owed)
}

impl Default for TimestampState {
    fn default() -> Self {
        Self::new()
    }
}
