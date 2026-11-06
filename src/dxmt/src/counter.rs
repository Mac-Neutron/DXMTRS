//! UAV counters and the pool they are allocated from.
//!
//! A counter (a structured buffer's append/consume count) lives in a 128-byte slot, one Apple GPU
//! cache line, of a set of 64 slots in one buffer allocation. Setting a counter's value takes a
//! new slot holding it, so the commands still using the old value keep it; a slot goes back to
//! its set when the last user of its [`CounterAllocation`] drops it. The pool's methods take
//! `&mut self`: its owner serialises them, as the C++'s lock does.
//!
//! Behaviour is the C++'s (`dxmt_counter.*`). Written ahead for the switches of `d3d11.dll` and
//! `d3d12.dll`, which are its first callers.

use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use dxmt_util::sync::lock;
use winemetal_sys::Obj;

use crate::resource::{Buffer, BufferAllocation, BufferAllocationFlags};

/// Slots in a set: the C++'s `kCounterSetSize`.
pub const COUNTER_SET_SIZE: u32 = 64;
/// Bytes between slots, the Apple GPU's cache line: the C++'s `kCounterAlignAs`.
pub const COUNTER_ALIGN: u32 = 128;

/// A UAV's counter: the slot holding its current value, once it has one.
#[derive(Default)]
pub struct Counter {
    current: Mutex<Option<Arc<CounterAllocation>>>,
}

impl Counter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a value was ever set or used.
    #[must_use]
    pub fn is_allocated(&self) -> bool {
        self.lock().is_some()
    }

    fn lock(&self) -> MutexGuard<'_, Option<Arc<CounterAllocation>>> {
        lock(&self.current)
    }
}

/// One counter's slot, back in its set when the last holder drops it.
pub struct CounterAllocation {
    set: Arc<CounterSet>,
    index: u32,
    retained: AtomicU64,
}

impl CounterAllocation {
    /// The allocation of the slot's set.
    #[must_use]
    pub fn buffer(&self) -> &Arc<BufferAllocation> {
        &self.set.allocation
    }

    /// The slot's byte offset in [`Self::buffer`].
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.index * COUNTER_ALIGN
    }

    /// Records that command sequence `seq` retains it: true when `seq` had already.
    pub fn check_retained(&self, seq: u64) -> bool {
        self.retained.swap(seq, Ordering::Relaxed) == seq
    }
}

impl Drop for CounterAllocation {
    fn drop(&mut self) {
        self.set.recycle(self.index);
    }
}

/// The sets of counter slots, a buffer allocation each.
pub struct CounterPool {
    sets: Vec<Arc<CounterSet>>,
    /// The set tried first, round-robin.
    next: usize,
    buffer: Buffer,
}

impl CounterPool {
    /// A pool on `device`, with one set.
    #[must_use]
    pub fn new(device: &Obj) -> Self {
        let buffer = Buffer::new(u64::from(COUNTER_ALIGN * COUNTER_SET_SIZE), device);
        Self {
            sets: vec![CounterSet::new(&buffer)],
            next: 0,
            buffer,
        }
    }

    /// `counter`'s current slot, allocated with the value 0 the first time.
    pub fn current(&mut self, counter: &Counter) -> Arc<CounterAllocation> {
        let mut current = counter.lock();
        if let Some(allocation) = current.as_ref() {
            return Arc::clone(allocation);
        }
        let allocation = self.allocate(0);
        *current = Some(Arc::clone(&allocation));
        allocation
    }

    /// Gives `counter` a new slot holding `value`.
    pub fn update(&mut self, counter: &Counter, value: u32) {
        let allocation = self.allocate(value);
        let old = counter.lock().replace(allocation);
        // Dropped after the lock: the slot may go back to its set here.
        drop(old);
    }

    /// A free slot holding `initial`: from the sets in turn, or a new set when all are full.
    fn allocate(&mut self, initial: u32) -> Arc<CounterAllocation> {
        loop {
            let count = self.sets.len();
            for _ in 0..count {
                if let Some(allocation) = self.sets[self.next % count].try_allocate(initial) {
                    return allocation;
                }
                self.next += 1;
            }
            // The next pass starts at the new set, which has every slot free.
            self.next = count;
            self.sets.push(CounterSet::new(&self.buffer));
        }
    }
}

/// [`COUNTER_SET_SIZE`] slots in one allocation, a bit each in `used`.
struct CounterSet {
    used: AtomicU64,
    allocation: Arc<BufferAllocation>,
}

impl CounterSet {
    fn new(buffer: &Buffer) -> Arc<Self> {
        let flags = BufferAllocationFlags::GPU_MANAGED | BufferAllocationFlags::NO_TRACKING;
        Arc::new(Self {
            used: AtomicU64::new(0),
            allocation: buffer.allocate(flags),
        })
    }

    /// The lowest free slot, holding `initial`, or `None` when all are used.
    fn try_allocate(self: &Arc<Self>, initial: u32) -> Option<Arc<CounterAllocation>> {
        let mut used = self.used.load(Ordering::Relaxed);
        loop {
            let free = !used;
            if free == 0 {
                return None;
            }
            let index = free.trailing_zeros();
            match self.used.compare_exchange_weak(
                used,
                used | (1 << index),
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => break Some(self.take(index, initial)),
                Err(now) => used = now,
            }
        }
    }

    fn take(self: &Arc<Self>, index: u32, initial: u32) -> Arc<CounterAllocation> {
        let allocation = Arc::new(CounterAllocation {
            set: Arc::clone(self),
            index,
            retained: AtomicU64::new(0),
        });
        self.allocation
            .update_contents(u64::from(allocation.offset()), &initial.to_ne_bytes(), 0);
        allocation
    }

    fn recycle(&self, index: u32) {
        let previous = self.used.fetch_and(!(1 << index), Ordering::Release);
        debug_assert!(previous & (1 << index) != 0, "a used slot comes back");
    }
}

#[cfg(test)]
mod tests;
