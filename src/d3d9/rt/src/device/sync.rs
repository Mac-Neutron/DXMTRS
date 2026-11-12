//! Waits on the core's queue (C4-12): for one chunk, and for everything recorded so far.

use super::CoreDevice;

impl CoreDevice {
    /// Waits until chunk `seq` is done, committing it first when it is the one being recorded.
    ///
    /// The open render pass ends with the commit; the queue never finishes a chunk it was not
    /// handed.
    pub fn sync_to(&mut self, seq: u64) {
        if seq >= self.device.queue().current_seq() {
            self.commit();
        }
        self.device.queue().wait_cpu(seq);
    }

    /// Waits until the GPU finished everything recorded.
    ///
    /// Commits the chunk if it holds work, then waits for the last chunk committed (a clean chunk
    /// is never committed, so it waits for nothing of its own).
    pub fn drain(&mut self) {
        if self.dirty || self.render_open || self.blit_open {
            self.commit();
        }
        let last = self.device.queue().current_seq() - 1;
        self.device.queue().wait_cpu(last);
    }

    /// Ends the open render pass and hands the chunk to the encode thread.
    pub fn commit(&mut self) {
        self.close_pass();
        self.device.queue_mut().commit();
        self.dirty = false;
        self.staged_bytes = 0;
    }
}

#[cfg(test)]
mod tests;
