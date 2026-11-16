//! D3D9 vertex and index buffers on the core (DXMTRS C4c-1): a core buffer per D3D9 buffer.
//!
//! The store keys each core `Buffer` by the D3D9 buffer's process-unique, never-reused
//! [`BufferId`]. Every buffer is staged: the front end's CPU copy is the truth, and the bytes a
//! draw reads are uploaded before it, copied into the queue's staging ring at record time and
//! blitted into the buffer in API order, as a texture's are ([`CoreDevice::texture_upload`]). The
//! buffer's allocation is private to the GPU and tracked, so a blit waits on the fences of the
//! draws that read the bytes it overwrites, and the draws after it wait on its own.

use std::sync::Arc;

use dxmt_core::{
    deptrack::Access,
    encoder::{NO_NEXT, stage},
    resource::{Buffer, BufferAllocationFlags},
};
use mtld3d_core::ids::BufferId;
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER, WmtcmdBlitCopyFromBufferToBuffer,
};

use super::{CoreDevice, LOG_TARGET};

impl CoreDevice {
    /// Makes `id`'s core buffer of `length` bytes, private to the GPU.
    ///
    /// Its allocation is made current now, on the API thread: a buffer with none binds nil. No
    /// byte is zeroed: the caller's first upload carries every byte of `length`. False, and no
    /// entry, when Metal makes no buffer. A second call for one id replaces the first.
    #[must_use]
    pub fn buffer_create(&mut self, id: BufferId, length: u64) -> bool {
        let buffer = Buffer::new(length, self.device.device());
        // Private and nothing else: read-only would turn off the tracking that orders an upload
        // against the draws around it, and the buffer is neither suballocated nor CPU-placed.
        let allocation = buffer.allocate(BufferAllocationFlags::GPU_PRIVATE);
        if allocation.buffer() == 0 {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: Metal makes no buffer of {length} bytes; its draws are dropped"
            );
            return false;
        }
        let replaced = buffer.rename(allocation);
        debug_assert!(replaced.is_none(), "a new buffer has no allocation");
        self.buffers.insert(id, Arc::new(buffer));
        true
    }

    /// Whether the store holds `id`'s buffer.
    #[must_use]
    pub fn has_buffer(&self, id: BufferId) -> bool {
        self.buffers.contains_key(&id)
    }

    /// Drops the store's buffer of `id`; the closures already recorded hold their own.
    pub fn forget_buffer(&mut self, id: BufferId) {
        self.buffers.remove(&id);
    }

    /// Uploads `bytes` into `id`'s buffer at byte `offset`.
    ///
    /// The bytes are copied into a staging-ring block now; a blit copies them at the chunk's
    /// encode, after every pass recorded before it, which ends an open render pass. False, and
    /// nothing recorded, when the store has no `id`, `bytes` is empty, the range ends past the
    /// buffer, or this process cannot reach the staging block.
    #[must_use]
    pub fn buffer_upload(&mut self, id: BufferId, offset: u64, bytes: &[u8]) -> bool {
        let Some(buffer) = self.buffers.get(&id) else {
            return false;
        };
        let Ok(length) = u64::try_from(bytes.len()) else {
            return false;
        };
        if length == 0
            || offset
                .checked_add(length)
                .is_none_or(|end| end > buffer.length())
        {
            return false;
        }
        let buffer = Arc::clone(buffer);
        let (block, staged) = self.device.queue_mut().allocate_staging_buffer(length, 16);
        let Ok(start) = usize::try_from(staged) else {
            return false;
        };
        if block.mapped.is_null() {
            log::error!(target: LOG_TARGET, "d3d9 core: a staging block this process cannot reach");
            return false;
        }
        // SAFETY: the block's mapping holds `length` bytes from `start`.
        let dst = unsafe { block.mapped.add(start) };
        // SAFETY: `dst` holds `length` writable bytes (above), which `bytes` (the caller's slice)
        // does not overlap.
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len()) };
        self.open_blit_pass();
        let source = block.buffer;
        self.emit(move |enc| {
            // A stored buffer has its allocation from its creation on.
            let Some((allocation, at)) =
                enc.access_buffer::<{ stage::COMPUTE }>(&buffer, Access::WRITE)
            else {
                return;
            };
            enc.encode_blit_command(WmtcmdBlitCopyFromBufferToBuffer {
                r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
                reserved: [0; 3],
                next: NO_NEXT,
                src: source,
                src_offset: staged,
                dst: allocation.buffer(),
                dst_offset: at + offset,
                copy_length: length,
            });
        });
        self.staged_bytes += length;
        if self.staged_bytes > self.staged_budget {
            self.commit();
        }
        true
    }
}

#[cfg(test)]
mod tests;
