//! D3D9 vertex and index buffers on the core (DXMTRS C4c-1): a core buffer per D3D9 buffer.
//!
//! The store keys each core `Buffer` by the D3D9 buffer's process-unique, never-reused
//! [`BufferId`]. A buffer is staged: the front end's CPU copy is the truth, and the bytes a
//! draw reads are uploaded before it, copied into the queue's staging ring at record time and
//! blitted into the buffer in API order, as a texture's are ([`CoreDevice::texture_upload`]). The
//! buffer's allocation is private to the GPU and tracked, so a blit waits on the fences of the
//! draws that read the bytes it overwrites, and the draws after it wait on its own.
//!
//! A dynamic buffer is D3D11's instead (DXMTRS C4c-2b): the CPU writes the memory the GPU reads,
//! renamed by mtld3d's lock plan ([`plan_lock`]) to a name the GPU is done with when a lock would
//! overwrite bytes a recorded draw reads. Every CPU reader (the lock's pointer, a rename's copy of
//! the old bytes, [`CoreDevice::dynamic_copy`]) reads the name and suballocation the last lock
//! answered, the immediate one; the buffer's current allocation is the encode thread's, moved by
//! the closures a rename emits.

use std::sync::Arc;

use dxmt_core::{
    deptrack::Access,
    dynamic::DynamicBuffer,
    encoder::{NO_NEXT, stage},
    resource::{Buffer, BufferAllocation, BufferAllocationFlags},
};
use mtld3d_core::{
    buffer_rename::{LockPlan, PreserveKind, plan_lock},
    ids::BufferId,
};
use rustc_hash::FxHashMap;
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER, WmtcmdBlitCopyFromBufferToBuffer,
};

use super::{CoreDevice, DrawData, IndexData, LOG_TARGET, StreamData};

/// The page a dynamic buffer's names are cut from or placed in: dxmt-core's page.
const PAGE: u64 = 4096;

/// A dynamic buffer's allocations, D3D11's for one: cut from one page when a name fits in it.
///
/// Placed in pages of their own in a 32-bit process, as D3D11's are. Never write-combined: D3D9
/// reads these bytes (a whole lock's copy, a fan's indices, a `D3DLOCK_READONLY` lock).
const DYNAMIC_FLAGS: BufferAllocationFlags = if cfg!(target_arch = "x86") {
    BufferAllocationFlags::SUBALLOCATE_FROM_ONE_PAGE.union(BufferAllocationFlags::CPU_PLACED)
} else {
    BufferAllocationFlags::SUBALLOCATE_FROM_ONE_PAGE
};

/// The store's buffers by id, and whether a draw has read a dynamic one (DXMTRS C4c-2b).
#[derive(Default)]
pub struct BufferStore {
    map: FxHashMap<BufferId, Stored>,
    /// A draw has read a dynamic buffer in place, and the line saying so is logged.
    read_in_place: bool,
}

impl BufferStore {
    /// The core buffer of `id`, which a draw binds.
    #[must_use]
    pub fn buffer(&self, id: BufferId) -> Option<&Arc<Buffer>> {
        self.map.get(&id).map(|stored| &stored.buffer)
    }
}

/// One stored buffer: its core buffer and, for a dynamic one, its names.
struct Stored {
    buffer: Arc<Buffer>,
    dynamic: Option<Dynamic>,
}

/// A dynamic buffer's names, its D3D9 length and the chunk that last read it.
struct Dynamic {
    names: Arc<DynamicBuffer>,
    /// The D3D9 length: what a lock plans over and a copy reads up to.
    length: u32,
    /// The chunk of the last draw that read the immediate name; 0 when none has.
    ///
    /// Set after the draw is recorded; back to 0 when a lock takes a new name.
    last_use: u64,
}

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
        let stored = Stored {
            buffer: Arc::new(buffer),
            dynamic: None,
        };
        self.buffers.map.insert(id, stored);
        true
    }

    /// Makes `id`'s dynamic buffer of `length` bytes, which the CPU writes in place.
    ///
    /// D3D11's dynamic buffer, renamed by [`Self::dynamic_lock`] (DXMTRS C4c-2b). Its core
    /// length is `length` up to one page and whole pages above, so a write past the end stays in
    /// memory the name owns. The first name's bytes are zeroed. False, no entry and an error
    /// logged when that name has no Metal buffer or no memory this process reaches: the front
    /// end then stages the buffer. A second call for one id replaces the first.
    ///
    /// # Panics
    ///
    /// When the buffer just given its first name has none, which [`Buffer::rename`] rules out.
    #[must_use]
    pub fn dynamic_create(&mut self, id: BufferId, length: u32) -> bool {
        let logical = u64::from(length);
        let size = if logical <= PAGE {
            logical
        } else {
            logical.next_multiple_of(PAGE)
        };
        let buffer = Arc::new(Buffer::new(size, self.device.device()));
        let name = buffer.allocate(DYNAMIC_FLAGS);
        let Some((memory, bytes)) = mapped(&name) else {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: a dynamic buffer of {length} bytes has no mapped memory (Metal buffer {:#x}); it is staged",
                name.buffer()
            );
            return false;
        };
        // SAFETY: `memory` is the name's, `bytes` long (every suballocation), and nothing else
        // has it yet.
        unsafe { memory.write_bytes(0, bytes) };
        let replaced = buffer.rename(name);
        debug_assert!(replaced.is_none(), "a new buffer has no allocation");
        let names = DynamicBuffer::new(Arc::clone(&buffer), DYNAMIC_FLAGS)
            .expect("a buffer just named has a name");
        let dynamic = Dynamic {
            names: Arc::new(names),
            length,
            last_use: 0,
        };
        let stored = Stored {
            buffer,
            dynamic: Some(dynamic),
        };
        self.buffers.map.insert(id, stored);
        true
    }

    /// Whether the store holds `id`'s buffer.
    #[must_use]
    pub fn has_buffer(&self, id: BufferId) -> bool {
        self.buffers.map.contains_key(&id)
    }

    /// Drops the store's buffer of `id`; the closures already recorded hold their own.
    ///
    /// A dynamic buffer's names go with it, but for those a chunk encoded, which it retains, and
    /// those an emitted rename installs, which its closure holds.
    pub fn forget_buffer(&mut self, id: BufferId) {
        self.buffers.map.remove(&id);
    }

    /// Locks `id`'s dynamic buffer: a pointer at byte `offset` of the memory the CPU writes.
    ///
    /// mtld3d's lock plan ([`plan_lock`]) over the D3D9 `flags`, `usage` and window (`offset`,
    /// `size`), against the chunk of the last draw that read the buffer: in place, or a new name
    /// when a recorded draw may still read the bytes. A new name is the next suballocation of
    /// the immediate one when it has one, else a name the GPU is done with or a new allocation,
    /// taken once the bytes the GPU side holds are under the cap; the old name's bytes are then
    /// counted until its chunk retires. A whole lock of a buffer that keeps its contents copies
    /// them to the new name before the pointer is answered. A new name with no memory this
    /// process reaches is not taken: the lock writes in place, and an error is logged. Null when
    /// the store holds no dynamic buffer of `id`, or `offset` is past its length.
    #[must_use]
    pub fn dynamic_lock(
        &mut self,
        id: BufferId,
        flags: u32,
        usage: u32,
        offset: u32,
        size: u32,
    ) -> *mut u8 {
        // The entry is not held across the rename: the hold, the emits and the ledger are the
        // device's.
        let Some((names, length, last_use)) = self.dynamic(id) else {
            return core::ptr::null_mut();
        };
        if offset > length {
            return core::ptr::null_mut();
        }
        let coherent = self.device.queue().coherent_seq();
        if let LockPlan::Rename { preserve } =
            plan_lock(flags, usage, length, offset, size, last_use, coherent)
            && self.rename(&names, length, preserve)
            && let Some(dynamic) = self.dynamic_mut(id)
        {
            dynamic.last_use = 0;
        }
        let base = names.immediate_mapped_memory();
        if base.is_null() {
            // Every name taken has memory (checked as it is taken).
            return base;
        }
        // SAFETY: `offset` is at most the buffer's length, which a suballocation of every name
        // holds, so the pointer is inside the name's memory or one past its end.
        unsafe { base.add(offset as usize) }
    }

    /// The bytes `[first, first + len)` of `id`'s dynamic buffer as the CPU last wrote them.
    ///
    /// Read from the immediate name, the one the last lock answered, not from what a draw
    /// recorded before binds (DXMTRS C4c-2b). None when the store holds no dynamic buffer of `id`
    /// or the window ends past its length.
    #[must_use]
    pub fn dynamic_copy(&self, id: BufferId, first: usize, len: usize) -> Option<Vec<u8>> {
        let dynamic = self.buffers.map.get(&id)?.dynamic.as_ref()?;
        if first.checked_add(len)? > dynamic.length as usize {
            return None;
        }
        let base = dynamic.names.immediate_mapped_memory();
        if base.is_null() {
            return None;
        }
        // SAFETY: `first` is at most the buffer's length, which the immediate suballocation
        // holds.
        let from = unsafe { base.add(first) };
        let mut bytes = vec![0; len];
        // SAFETY: `from` is followed by `len` bytes of the suballocation (the window ends inside
        // the length), which `bytes`, a new vector of `len`, does not overlap.
        unsafe { core::ptr::copy_nonoverlapping(from, bytes.as_mut_ptr(), len) };
        Some(bytes)
    }

    /// Uploads `bytes` into `id`'s buffer at byte `offset`.
    ///
    /// The bytes are copied into a staging-ring block now; a blit copies them at the chunk's
    /// encode, after every pass recorded before it, which ends an open render pass. They count
    /// against the retention cap until the chunk retires (DXMTRS C4c-2b), and the upload waits
    /// first while the bytes the GPU side holds are at the cap. False, and nothing recorded,
    /// when the store has no `id`, `bytes` is empty, the range ends past the buffer, or this
    /// process cannot reach the staging block.
    #[must_use]
    pub fn buffer_upload(&mut self, id: BufferId, offset: u64, bytes: &[u8]) -> bool {
        let Some(buffer) = self.buffers.buffer(id) else {
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
        self.hold_under_cap();
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
        // At the chunk being recorded, before a commit for the staged budget moves it on.
        let queue = self.device.queue();
        let (seq, coherent) = (queue.current_seq(), queue.coherent_seq());
        self.retained.add(seq, length, coherent);
        self.staged_bytes += length;
        if self.staged_bytes > self.staged_budget {
            self.commit();
        }
        true
    }

    /// Stamps the chunk being recorded on each dynamic buffer `draw` reads, its index buffer too.
    ///
    /// Called once the draw is recorded, with nothing committed since, so a lock before that
    /// chunk retires plans against the draw. The first stamp logs that dynamic buffers are read
    /// in place, once a device, at the level the gates read.
    pub(super) fn stamp_dynamic(&mut self, draw: &DrawData<'_>) {
        let seq = self.device.queue().current_seq();
        let streams = draw.streams.iter().filter_map(|data| match *data {
            StreamData::Buffer { id, .. } => Some(id),
            StreamData::None | StreamData::Bytes(_) | StreamData::Zeros => None,
        });
        let index = draw
            .indices
            .as_ref()
            .and_then(|indices| match indices.data {
                IndexData::Buffer { id, .. } => Some(id),
                IndexData::Bytes(_) => None,
            });
        let mut stamped = false;
        for id in streams.chain(index) {
            if let Some(dynamic) = self.dynamic_mut(id) {
                dynamic.last_use = seq;
                stamped = true;
            }
        }
        if stamped && !self.buffers.read_in_place {
            self.buffers.read_in_place = true;
            log::log!(
                target: LOG_TARGET,
                super::gate_level(),
                "d3d9 core: dynamic buffers are read in place"
            );
        }
    }

    /// `id`'s dynamic buffer: its names, its length and the chunk that last read it.
    fn dynamic(&self, id: BufferId) -> Option<(Arc<DynamicBuffer>, u32, u64)> {
        let dynamic = self.buffers.map.get(&id)?.dynamic.as_ref()?;
        Some((Arc::clone(&dynamic.names), dynamic.length, dynamic.last_use))
    }

    fn dynamic_mut(&mut self, id: BufferId) -> Option<&mut Dynamic> {
        self.buffers.map.get_mut(&id)?.dynamic.as_mut()
    }

    /// Gives `names` a new name for a lock, D3D11's discarding `Map`; false when none is taken.
    ///
    /// The next suballocation of the immediate name, else a name the GPU is done with as of the
    /// coherent chunk read after the hold, or a new allocation. The step and the new name each
    /// reach the encode thread as a closure, after the draws recorded before; a new name puts
    /// the old one's bytes in the ledger. `PreserveKind::Cpu` copies the first `length` bytes of
    /// the immediate suballocation as it was before the rename into the new one.
    fn rename(&mut self, names: &DynamicBuffer, length: u32, preserve: PreserveKind) -> bool {
        // Before the step, which moves the immediate suballocation.
        let source = names.immediate_mapped_memory();
        if let Some((name, next, _)) = names.next_suballocation() {
            self.emit(move |_| name.use_suballocation(next));
        } else {
            self.hold_under_cap();
            let queue = self.device.queue();
            let (seq, coherent) = (queue.current_seq(), queue.coherent_seq());
            let name = names.allocate(coherent);
            if mapped(&name).is_none() {
                log::error!(
                    target: LOG_TARGET,
                    "d3d9 core: a dynamic buffer's new name has no mapped memory (Metal buffer {:#x}); the lock writes in place",
                    name.buffer()
                );
                return false;
            }
            let old = names.immediate_name();
            names.update_immediate_name(seq, Arc::clone(&name), 0, false);
            let buffer = Arc::clone(names.buffer());
            self.emit(move |_| {
                name.use_suballocation(0);
                let _ = buffer.rename(Arc::clone(&name));
            });
            let held = old.suballocation_size() * u64::from(old.suballocation_count());
            self.retained.add(seq, held, coherent);
        }
        if preserve == PreserveKind::Cpu {
            let to = names.immediate_mapped_memory();
            // SAFETY: `source` and `to` are suballocations of names taken with memory, each
            // holding the buffer's `length` bytes, and distinct (a step or another name).
            unsafe { core::ptr::copy_nonoverlapping(source, to, length as usize) };
        }
        true
    }
}

/// A name's memory and its bytes (every suballocation), when it is usable.
///
/// Usable: Metal made it a buffer, and it has memory this process reaches. A placed name Metal
/// refused keeps its memory and has no buffer.
fn mapped(name: &BufferAllocation) -> Option<(*mut u8, usize)> {
    let memory = name.mapped_memory(0);
    if name.buffer() == 0 || memory.is_null() {
        return None;
    }
    let bytes = name.suballocation_size() * u64::from(name.suballocation_count());
    Some((memory, usize::try_from(bytes).ok()?))
}

#[cfg(test)]
mod tests;
