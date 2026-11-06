//! Staging resources: the memory CPU reads and writes go through.
//!
//! A [`Staging`] is a buffer the CPU maps. A map waits for the GPU's copies into it (reads and
//! writes) and out of it (writes); a write that would only wait for the GPU to read renames the
//! buffer instead, to an allocation whose last use is finished. The C++'s `StagingResource`.
//!
//! Written ahead for the switch of `d3d11.dll`, its first caller.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use dxmt_util::sync::lock;
use winemetal_sys::Obj;

use crate::resource::{Buffer, BufferAllocation, BufferAllocationFlags};

/// What a map may do now.
#[derive(PartialEq, Eq)]
pub enum MapResult {
    /// Map the current allocation.
    Mappable,
    /// A write would wait for the GPU only reading the buffer: rename it instead.
    ///
    /// [`Staging::allocate`] and [`Staging::update_immediate_name`] make a new allocation the
    /// one maps return.
    Renamable,
    /// Wait until the GPU finished this command sequence.
    WaitFor(u64),
}

/// A staging buffer, its allocations and the sequences its maps wait for.
///
/// Its methods take `&self` and lock: the resource, its views and the command lists share it.
pub struct Staging {
    buffer: Arc<Buffer>,
    bytes_per_row: u32,
    bytes_per_image: u32,
    state: Mutex<State>,
}

impl Staging {
    /// A staging buffer of `length` bytes on `device`, with its first allocation current.
    #[must_use]
    pub fn new(device: &Obj, length: u64, bytes_per_row: u32, bytes_per_image: u32) -> Self {
        let buffer = Arc::new(Buffer::new(length, device));
        let first = buffer.allocate(flags());
        buffer.rename(Arc::clone(&first));
        Self {
            buffer,
            bytes_per_row,
            bytes_per_image,
            state: Mutex::new(State {
                immediate: 0,
                pool: vec![first],
                fifo: VecDeque::new(),
                readable_after: 0,
                writable_after: 0,
            }),
        }
    }

    #[must_use]
    pub const fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    #[must_use]
    pub fn length(&self) -> u64 {
        self.buffer.length()
    }

    #[must_use]
    pub const fn bytes_per_row(&self) -> u32 {
        self.bytes_per_row
    }

    #[must_use]
    pub const fn bytes_per_image(&self) -> u32 {
        self.bytes_per_image
    }

    /// Records a GPU copy into the buffer in command sequence `seq`.
    pub fn use_copy_destination(&self, seq: u64) {
        self.lock().use_copy_destination(seq);
    }

    /// Records a GPU copy out of the buffer in command sequence `seq`.
    pub fn use_copy_source(&self, seq: u64) {
        self.lock().use_copy_source(seq);
    }

    /// Whether the CPU may `read` and `write` the buffer now that the GPU finished `coherent`.
    #[must_use]
    pub fn try_map(&self, coherent: u64, read: bool, write: bool) -> MapResult {
        self.lock().try_map(coherent, read, write)
    }

    /// The id of an allocation the GPU is done with as of `coherent`, made if there is none.
    #[must_use]
    pub fn allocate(&self, coherent: u64) -> usize {
        let mut state = self.lock();
        if let Some(id) = state.free_id(coherent) {
            return id;
        }
        state.pool.push(self.buffer.allocate(flags()));
        state.pool.len() - 1
    }

    /// Makes allocation `id` the one maps return.
    ///
    /// The current one is free once the GPU finished `current_seq`. The buffer itself is renamed
    /// by the command the caller records.
    pub fn update_immediate_name(&self, current_seq: u64, id: usize) {
        self.lock().update_immediate_name(current_seq, id);
    }

    /// Allocation `id`, if the buffer has one by that id.
    #[must_use]
    pub fn allocation(&self, id: usize) -> Option<Arc<BufferAllocation>> {
        self.lock().pool.get(id).cloned()
    }

    /// Allocation `id`'s CPU memory, or null.
    #[must_use]
    pub fn mapped_memory(&self, id: usize) -> *mut u8 {
        self.allocation(id)
            .map_or(core::ptr::null_mut(), |a| a.mapped_memory(0))
    }

    /// The CPU memory maps return.
    #[must_use]
    pub fn mapped_immediate_memory(&self) -> *mut u8 {
        let state = self.lock();
        state
            .pool
            .get(state.immediate)
            .map_or(core::ptr::null_mut(), |a| a.mapped_memory(0))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        lock(&self.state)
    }
}

/// A new allocation's flags.
///
/// A 32-bit process places it in its own memory, as the C++'s `__i386__` build.
const fn flags() -> BufferAllocationFlags {
    if cfg!(target_arch = "x86") {
        BufferAllocationFlags::CPU_PLACED
    } else {
        BufferAllocationFlags::empty()
    }
}

struct State {
    /// The id of the allocation maps return.
    immediate: usize,
    /// Every allocation, by id.
    pool: Vec<Arc<BufferAllocation>>,
    /// Allocations renamed away, with the sequence after which the GPU is done with each.
    fifo: VecDeque<(usize, u64)>,
    /// The CPU reads only after the GPU finished this sequence (its last copy into the buffer).
    readable_after: u64,
    /// The CPU writes only after the GPU finished this sequence (its last copy in or out).
    writable_after: u64,
}

impl State {
    fn use_copy_destination(&mut self, seq: u64) {
        self.readable_after = self.readable_after.max(seq);
        self.writable_after = self.writable_after.max(seq);
    }

    fn use_copy_source(&mut self, seq: u64) {
        self.writable_after = self.writable_after.max(seq);
    }

    const fn try_map(&self, coherent: u64, read: bool, write: bool) -> MapResult {
        if read && coherent < self.readable_after {
            return MapResult::WaitFor(self.readable_after);
        }
        if write && coherent < self.writable_after {
            if coherent >= self.readable_after {
                return MapResult::Renamable;
            }
            return MapResult::WaitFor(self.writable_after);
        }
        MapResult::Mappable
    }

    fn free_id(&mut self, coherent: u64) -> Option<usize> {
        self.fifo
            .pop_front_if(|&mut (_, at)| at <= coherent)
            .map(|(id, _)| id)
    }

    fn update_immediate_name(&mut self, current_seq: u64, id: usize) {
        self.fifo.push_back((self.immediate, current_seq));
        self.immediate = id;
        self.readable_after = 0;
        self.writable_after = 0;
    }
}

#[cfg(test)]
mod tests;
