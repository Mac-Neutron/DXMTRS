//! The vertex and index bytes the GPU side still holds, and their cap (DXMTRS C4c-2b).
//!
//! A buffer upload's staged copy stays in the staging ring until the GPU retires the chunk that
//! recorded it, and so does a dynamic buffer's renamed-away name before it can be named again.
//! The ledger counts those bytes by chunk. At `memory.vbibRetentionCapMB` the API thread waits
//! for the oldest chunk instead of growing, as mtld3d's cap on its retained backings has it.
//! Texture uploads are not counted: the key bounds vertex and index memory.

use std::collections::VecDeque;

use super::CoreDevice;

/// Bytes the GPU side still holds, by the chunk that recorded them, oldest first.
#[derive(Default)]
pub struct Retained {
    /// Each chunk's sequence and the bytes added at it, in increasing sequence.
    entries: VecDeque<(u64, u64)>,
    /// The entries' bytes added up.
    bytes: u64,
}

impl Retained {
    /// Adds `bytes` at chunk `seq`, once the chunks up to `coherent` are settled.
    ///
    /// `seq` is never below an earlier add's: the chunk being recorded only moves on.
    pub fn add(&mut self, seq: u64, bytes: u64, coherent: u64) {
        self.settle(coherent);
        if bytes == 0 {
            return;
        }
        match self.entries.back_mut() {
            Some((last, held)) if *last == seq => *held += bytes,
            _ => self.entries.push_back((seq, bytes)),
        }
        self.bytes += bytes;
    }

    /// Drops what the chunks up to `coherent`, which the GPU has retired, held.
    pub fn settle(&mut self, coherent: u64) {
        while let Some(&(seq, held)) = self.entries.front()
            && seq <= coherent
        {
            self.entries.pop_front();
            self.bytes -= held;
        }
    }

    /// The bytes still held.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The oldest chunk that still holds bytes.
    #[must_use]
    pub fn oldest(&self) -> Option<u64> {
        self.entries.front().map(|&(seq, _)| seq)
    }
}

impl CoreDevice {
    /// Sets the cap on the vertex and index bytes the GPU side still holds; 0 for none.
    ///
    /// `memory.vbibRetentionCapMB` in bytes. A buffer upload and a dynamic buffer's new name
    /// wait for the oldest chunks while the bytes are at the cap.
    pub const fn set_retention_cap(&mut self, bytes: u64) {
        self.retention_cap = bytes;
    }

    /// Waits for the oldest chunks while the bytes still held are at the cap or over it.
    ///
    /// The oldest can be the chunk being recorded, which [`CoreDevice::sync_to`] commits first,
    /// ending the open pass. Nothing under a cap of 0.
    pub(super) fn hold_under_cap(&mut self) {
        self.retained.settle(self.device.queue().coherent_seq());
        while self.retention_cap != 0
            && self.retained.bytes() >= self.retention_cap
            && let Some(oldest) = self.retained.oldest()
        {
            self.sync_to(oldest);
            self.retained.settle(self.device.queue().coherent_seq());
        }
    }
}

#[cfg(test)]
mod tests;
