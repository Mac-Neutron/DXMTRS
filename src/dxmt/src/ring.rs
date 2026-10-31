//! The ring allocator of short-lived upload memory.
//!
//! A [`RingBump`] bump-allocates from Metal buffers made a block at a time and kept in a FIFO
//! with the last command sequence that used each. Once the GPU finished that sequence a block is
//! reused from its start, and one left unused for [`RingConfig::lifetime`] frees is released. A
//! request larger than a block gets a block of its own size, released as soon as its sequence
//! finished. The C++'s `RingBumpState`; its host-memory blocks are the chunk arena's in Rust.
//!
//! Written ahead for the switches of `d3d11.dll` and `d3d12.dll`, which are its first callers.

use std::{collections::VecDeque, sync::Arc};

use winemetal_sys::Obj;

use crate::resource::{Buffer, BufferAllocation, BufferAllocationFlags};

/// The block of the queue's rings: the C++'s `kStagingBlockSize`.
pub const STAGING_BLOCK: u64 = 0x200_0000;
/// The block of a deferred context's staging ring: the C++'s `kStagingBlockSizeForDeferredContext`.
pub const DEFERRED_BLOCK: u64 = 0x20_0000;
/// Every C++ ring's lifetime: the C++'s `kStagingBlockLifetime`.
pub const LIFETIME: u64 = 300;

/// A ring's sizes, values of the process's memory profile.
pub struct RingConfig {
    /// The bytes a block holds; a larger request gets a block of its own size.
    pub block: u64,
    /// Frees a block stays unused for, or sequences it stays unused for, before it is released.
    pub lifetime: u64,
}

/// Where an allocation lives: its block's Metal buffer, GPU address and CPU address.
///
/// Borrowed handles, valid until the ring releases the block, which it does only after the GPU
/// finished the sequence that allocated.
pub struct BlockRef {
    /// The block's `MTLBuffer` handle, or the null handle if Metal made none.
    pub buffer: u64,
    pub gpu_address: u64,
    /// The block's start in CPU memory, or null if this process cannot reach it.
    pub mapped: *mut u8,
}

/// Blocks of Metal buffers that allocations bump through.
///
/// Its methods take `&mut self`: a ring shared between threads is behind its owner's `Mutex`, as
/// the C++'s are behind their own (and a ring one thread uses is behind none, as the C++'s
/// `null_mutex` ones).
pub struct RingBump {
    device: Obj,
    flags: BufferAllocationFlags,
    blocks: Blocks<Arc<BufferAllocation>>,
}

impl RingBump {
    /// A ring of buffers `flags` describe on `device`, in blocks of `config.block` bytes.
    ///
    /// The C++'s block kinds are flags: GPU-private memory is
    /// [`BufferAllocationFlags::GPU_PRIVATE`]; staging memory is Metal's (no storage flag, or
    /// [`BufferAllocationFlags::GPU_MANAGED`]), or placed with
    /// [`BufferAllocationFlags::CPU_PLACED`] (the guest's shared memory in a VM guest build).
    #[must_use]
    pub fn new(device: &Obj, flags: BufferAllocationFlags, config: RingConfig) -> Self {
        // SAFETY: `device` holds a reference, so its handle is a live object's.
        let device = unsafe { Obj::retain(device.handle()) };
        Self {
            device: device.unwrap_or_else(|| unreachable!("an Obj's handle is never null")),
            flags,
            blocks: Blocks::new(config),
        }
    }

    /// `size` bytes aligned to `align` for command sequence `seq`, and their offset in the block.
    ///
    /// `align` is a power of two. A block whose last sequence is below `coherent`, the last one
    /// the GPU finished, is reused.
    pub fn allocate(&mut self, seq: u64, coherent: u64, size: u64, align: u64) -> (BlockRef, u64) {
        let (block, offset) = self.blocks.allocate(seq, coherent, size, align, |length| {
            Buffer::new(length, &self.device).allocate(self.flags)
        });
        let block = BlockRef {
            buffer: block.buffer(),
            gpu_address: block.gpu_address(),
            mapped: block.mapped_memory(0),
        };
        (block, offset)
    }

    /// Releases the blocks that outlived their lifetime, oldest first.
    ///
    /// It stops at the first one still in use or still young. `coherent` is the last sequence the
    /// GPU finished; `u64::MAX` releases every block.
    pub fn free_blocks(&mut self, coherent: u64) {
        self.blocks.free(coherent);
    }
}

/// One block in the FIFO.
struct Block<B> {
    /// Bytes allocated from its start.
    used: u64,
    size: u64,
    /// The last command sequence that allocated from it.
    last_seq: u64,
    /// Frees it survived unused at the FIFO's front.
    idle_frees: u64,
    buffer: B,
}

/// The FIFO of blocks, oldest first, the latest at the back.
struct Blocks<B> {
    config: RingConfig,
    fifo: VecDeque<Block<B>>,
}

impl<B> Blocks<B> {
    const fn new(config: RingConfig) -> Self {
        Self {
            config,
            fifo: VecDeque::new(),
        }
    }

    /// The block and offset of `size` bytes; `make` makes a block of the bytes it is given.
    fn allocate(
        &mut self,
        seq: u64,
        coherent: u64,
        size: u64,
        align: u64,
        make: impl FnOnce(u64) -> B,
    ) -> (&B, u64) {
        debug_assert!(align.is_power_of_two(), "an allocation's alignment");
        let fits = self.fifo.back().is_some_and(|b| {
            b.used
                .next_multiple_of(align)
                .checked_add(size)
                .is_some_and(|end| end <= b.size)
        });
        if !fits {
            self.reuse_or_push(seq, coherent, size.max(self.config.block), make);
        }
        let Some(latest) = self.fifo.back_mut() else {
            unreachable!("a block was just pushed");
        };
        let offset = latest.used.next_multiple_of(align);
        latest.used = offset + size;
        latest.last_seq = seq;
        (&latest.buffer, offset)
    }

    /// Makes the oldest block the latest, or a new block of `size` bytes.
    ///
    /// The oldest is reused when the GPU is done with it and it holds `size` bytes; blocks of
    /// their own size are dropped on the way.
    fn reuse_or_push(&mut self, seq: u64, coherent: u64, size: u64, make: impl FnOnce(u64) -> B) {
        while let Some(front) = self.fifo.front_mut() {
            if front.last_seq >= coherent {
                break;
            }
            if front.size != self.config.block {
                self.fifo.pop_front();
                continue;
            }
            if front.size >= size {
                front.used = 0;
                front.last_seq = seq;
                front.idle_frees = 0;
                self.fifo.rotate_left(1);
                return;
            }
            // The C++ warns "forced to allocate new block of size <size>" here; the core's log
            // (`crate::log`) has no functions yet.
            break;
        }
        self.fifo.push_back(Block {
            used: 0,
            size,
            last_seq: seq,
            idle_frees: 0,
            buffer: make(size),
        });
    }

    fn free(&mut self, coherent: u64) {
        while let Some(front) = self.fifo.front_mut() {
            if front.last_seq > coherent {
                break;
            }
            let expired = coherent - front.last_seq > self.config.lifetime
                || front.idle_frees > self.config.lifetime
                || coherent == u64::MAX;
            if expired || front.size != self.config.block {
                self.fifo.pop_front();
                continue;
            }
            front.idle_frees += 1;
            break;
        }
    }
}

#[cfg(test)]
mod tests;
