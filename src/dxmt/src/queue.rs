//! The command queue: chunks committed in order, encoded and finished on threads of their own.
//!
//! The front end records commands into the current chunk and commits it; the encode thread runs
//! each committed chunk's commands into a Metal command buffer and commits that, and the finish
//! thread waits for the buffer, retires the chunk and signals the CPU fence
//! (`dxmt_command_queue.cpp`). A chunk belongs to one thread at a time, handed on by the
//! ring's counters: the recording thread owns chunk `seq` until it commits it, the encode
//! thread until it counts it encoded, the finish thread until it counts it finished. At most
//! `chunks - 1` are in flight, so the slot the recording thread moves to is always retired.

use core::{
    alloc::Layout,
    cell::UnsafeCell,
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
use std::{
    io,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    thread::{self, JoinHandle},
    time::Instant,
};

use dxmt_util::sync::{self, CpuFence};
use winemetal_sys::{
    MTLCommandBuffer_commit, MTLCommandBuffer_encodeWaitForEvent, MTLCommandBuffer_status,
    MTLCommandBuffer_waitUntilCompleted, MTLCommandQueue_commandBuffer, MTLDevice_newCommandQueue,
    MTLDevice_newSharedEvent, MTLSharedEvent_signaledValue, Obj, Pool, SharedEventListener_create,
    SharedEventListener_destroy, SharedEventListener_start,
    wire::WMT_COMMAND_BUFFER_STATUS_SCHEDULED,
};

use crate::{
    arena::{BlockPool, Commands},
    encoder::{Encoder, QueryBuffers},
    frame_stats::{Container, FrameStatistics},
    initializer::ResourceInitializer,
    resource::BufferAllocationFlags,
    ring::{BlockRef, RingBump, RingConfig},
};

/// The encode thread's name.
///
/// `dxmt-encode-thread` cut to the 15 characters the C++'s `setThreadName` keeps.
const ENCODE_THREAD: &str = "dxmt-encode-thr";

/// The finish thread's name: `dxmt-finish-thread` cut to 15 characters, as the C++'s.
const FINISH_THREAD: &str = "dxmt-finish-thr";

/// The queue's threads' stack reservation, the C++'s `dxmt::thread`'s.
const STACK: usize = 0x10_0000;

/// A chunk whose retirement signals no frame.
const NO_FRAME: u64 = u64::MAX;

/// The queue's sizes; `Default` is D3D11's, and D3D9's memory profile has its own.
pub struct QueueConfig {
    /// Chunks in the ring, and Metal command buffers in flight; at least 2.
    pub chunks: u32,
    /// Bytes in a block of the chunks' command memory (`kCommandChunkCPUHeapSize`).
    pub cpu_heap: usize,
    /// Bytes in a block of argument-buffer memory (`kCommandChunkGPUHeapSize`), the encoder's.
    pub gpu_heap: usize,
    /// Bytes in a block of the encoder's own memory (`kEncodingContextCPUHeapSize`).
    pub encoder_heap: usize,
    /// Frames the CPU may run ahead of the GPU until the front end sets another number.
    pub max_latency: u32,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            chunks: 32,
            cpu_heap: 0x40_0000,
            gpu_heap: 0x40_0000,
            encoder_heap: 0x10_0000,
            max_latency: 3,
        }
    }
}

/// A unit of recorded work.
///
/// Commands, the allocations they keep alive and, once encoded, the command buffer they went
/// into.
pub struct Chunk {
    commands: Commands,
    retained: Vec<Arc<dyn Send + Sync>>,
    cmdbuf: Option<Obj>,
    /// The resource initializer's event value the command buffer waits for, or 0.
    initializer_wait: u64,
    id: u64,
    event_id: u64,
    frame: u64,
    frame_latency: u64,
}

impl Chunk {
    fn new(pool: Arc<BlockPool>) -> Self {
        Self {
            commands: Commands::new(pool),
            retained: Vec::new(),
            cmdbuf: None,
            initializer_wait: 0,
            id: 0,
            event_id: 0,
            frame: 0,
            frame_latency: NO_FRAME,
        }
    }

    /// Records `f`, which the encode thread runs once.
    ///
    /// It is dropped when the chunk is retired, after the GPU finished the chunk.
    pub fn emit<F: Fn(&mut Encoder) + Send + 'static>(&mut self, f: F) {
        self.commands.emit(f);
    }

    /// Keeps `value` alive until the chunk is retired.
    pub fn retain<T: Send + 'static>(&mut self, value: T) {
        self.commands.retain(value);
    }

    /// Uninitialized memory of `size` bytes aligned to `align`, valid until the chunk is retired.
    ///
    /// # Panics
    ///
    /// When `align` is not a power of two, or `size` rounded up to it overflows `isize`.
    pub fn allocate_cpu(&mut self, size: usize, align: usize) -> NonNull<u8> {
        let layout = Layout::from_size_align(size, align).expect("a valid size and alignment");
        self.commands.allocate(layout)
    }

    /// Signals the frame-latency fence with `frame` once the chunk is retired (the present's).
    pub const fn signal_frame_latency(&mut self, frame: u64) {
        self.frame_latency = frame;
    }

    /// The chunk's sequence number, set when it is committed.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// The shared event's value the chunk signals, set when it is committed.
    #[must_use]
    pub const fn event_id(&self) -> u64 {
        self.event_id
    }

    /// The frame the chunk belongs to, set when it is committed.
    #[must_use]
    pub const fn frame(&self) -> u64 {
        self.frame
    }

    fn reset(&mut self) {
        self.frame_latency = NO_FRAME;
        self.commands.reset();
        self.retained.clear();
        self.cmdbuf = None;
    }
}

/// The queue's counters, without Metal.
///
/// The chunk being recorded and those committed, encoded and finished.
struct Ring {
    chunks: u64,
    /// The chunk being recorded; every chunk below it is committed (`ready_for_encode`).
    recording: AtomicU64,
    /// The chunk the encode thread counts next (`ready_for_commit`).
    ///
    /// Every chunk below it is encoded.
    encoded: AtomicU64,
    /// Chunks committed and not finished (`chunk_ongoing`).
    in_flight: AtomicU64,
    /// The last chunk finished (`cpu_coherent`).
    finished: CpuFence,
    stopped: AtomicBool,
}

impl Ring {
    /// Starts at chunk 1, so sequence 0 is always finished.
    fn new(chunks: u32) -> Self {
        Self {
            chunks: u64::from(chunks),
            recording: AtomicU64::new(1),
            encoded: AtomicU64::new(1),
            in_flight: AtomicU64::new(0),
            finished: CpuFence::new(0),
            stopped: AtomicBool::new(false),
        }
    }

    fn current(&self) -> u64 {
        self.recording.load(Ordering::Relaxed)
    }

    fn coherent(&self) -> u64 {
        self.finished.signaled()
    }

    /// The recording thread: hands the current chunk to the encode thread.
    ///
    /// Then waits while `chunks - 1` chunks are in flight.
    fn commit(&self) {
        self.recording.fetch_add(1, Ordering::Release);
        sync::wake_all(&self.recording);
        loop {
            let in_flight = self.in_flight.load(Ordering::Acquire);
            if in_flight < self.chunks - 1 {
                break;
            }
            sync::wait(&self.in_flight, in_flight);
        }
        self.in_flight.fetch_add(1, Ordering::Relaxed);
    }

    /// The encode thread: waits until chunk `seq` is committed; false once the ring stopped.
    fn wait_committed(&self, seq: u64) -> bool {
        self.wait_past(&self.recording, seq)
    }

    /// The encode thread: the next chunk is encoded and its command buffer committed.
    fn encoded(&self) {
        self.encoded.fetch_add(1, Ordering::Release);
        sync::wake_all(&self.encoded);
    }

    /// The finish thread: waits until chunk `seq` is encoded; false once the ring stopped.
    fn wait_encoded(&self, seq: u64) -> bool {
        self.wait_past(&self.encoded, seq)
    }

    /// The finish thread: chunk `seq` is retired.
    fn finish(&self, seq: u64) {
        self.finished.signal(seq);
        self.in_flight.fetch_sub(1, Ordering::Release);
        sync::wake_all(&self.in_flight);
    }

    /// Ends both threads' waits.
    fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        for counter in [&self.recording, &self.encoded] {
            counter.fetch_add(1, Ordering::Release);
            sync::wake_all(counter);
        }
    }

    fn wait_past(&self, counter: &AtomicU64, seq: u64) -> bool {
        loop {
            let value = counter.load(Ordering::Acquire);
            // After the load: a stop's bump of the counter carries the flag with it.
            if self.stopped.load(Ordering::Relaxed) {
                return false;
            }
            if value > seq {
                return true;
            }
            sync::wait(counter, value);
        }
    }
}

/// What the queue's threads share.
struct Shared {
    ring: Ring,
    chunks: Box<[UnsafeCell<Chunk>]>,
    queue: Option<Obj>,
    frame_latency: CpuFence,
    initializer: ResourceInitializer,
    statistics: Mutex<Container>,
}

// SAFETY: a chunk is reached by the one thread the ring hands it to (the module's rule); the
// rest is atomic, locked or never written after `Queue::new`.
unsafe impl Sync for Shared {}

impl Shared {
    fn slot(&self, seq: u64) -> *mut Chunk {
        // Below `chunks`, a `u32`: the conversion cannot fail.
        let index = usize::try_from(seq % self.ring.chunks).unwrap_or_default();
        self.chunks[index].get()
    }

    fn statistics(&self) -> MutexGuard<'_, Container> {
        self.statistics
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// D3D11's (and D3D9's) command queue.
///
/// The ring of chunks, its Metal queue and shared event, the encode and finish threads and the
/// shared-event listener's.
pub struct Queue {
    shared: Arc<Shared>,
    config: QueueConfig,
    event: Option<Obj>,
    listener: u64,
    /// Frames presented (`frame_count`).
    frame: AtomicU64,
    max_latency: AtomicU32,
    event_seq: u64,
    staging: RingBump,
    threads: Vec<JoinHandle<()>>,
}

impl Queue {
    /// A queue on `device` with `config`'s sizes, its threads running.
    ///
    /// # Errors
    ///
    /// When a thread cannot be started (the C++ answers `E_FAIL`); what started is stopped.
    ///
    /// # Panics
    ///
    /// When `config.chunks` is below 2.
    pub fn new(device: &Obj, config: QueueConfig) -> io::Result<Self> {
        assert!(config.chunks >= 2, "a ring of at least two chunks");
        let pool = Arc::new(BlockPool::new(config.cpu_heap));
        let chunks = (0..config.chunks)
            .map(|_| UnsafeCell::new(Chunk::new(Arc::clone(&pool))))
            .collect();
        // SAFETY: `device` is a live device.
        let queue = unsafe { MTLDevice_newCommandQueue(device.handle(), u64::from(config.chunks)) };
        // SAFETY: a new object's handle carries the reference the `Obj` takes over.
        let queue = unsafe { Obj::adopt(queue) };
        // SAFETY: `device` is a live device.
        let event = unsafe { MTLDevice_newSharedEvent(device.handle()) };
        // SAFETY: a new object's handle carries the reference the `Obj` takes over.
        let event = unsafe { Obj::adopt(event) };
        // SAFETY: no precondition; `drop` destroys it.
        let listener = unsafe { SharedEventListener_create() };
        let mut staging_flags = BufferAllocationFlags::CPU_WRITE_COMBINED;
        if cfg!(not(target_os = "macos")) {
            staging_flags |= BufferAllocationFlags::CPU_PLACED;
        }
        let staging = RingBump::new(
            device,
            staging_flags,
            RingConfig {
                block: 0x40_0000,
                lifetime: 300,
            },
        );
        let mut this = Self {
            shared: Arc::new(Shared {
                ring: Ring::new(config.chunks),
                chunks,
                queue,
                frame_latency: CpuFence::new(0),
                initializer: ResourceInitializer::new(device),
                statistics: Mutex::new(Container::default()),
            }),
            max_latency: AtomicU32::new(config.max_latency),
            config,
            event,
            listener,
            frame: AtomicU64::new(0),
            event_seq: 0,
            staging,
            threads: Vec::with_capacity(3),
        };
        // A failed start drops `this`, whose `drop` stops and waits for what started.
        let shared = Arc::clone(&this.shared);
        let mut encoder = Encoder::new(device, this.config.encoder_heap);
        // Borrowed: `drop` waits for the encode thread before the event is released.
        let event = this.shared_event();
        this.threads.push(spawn(Some(ENCODE_THREAD), move || {
            let _guard = crate::ffi::AbortOnPanic;
            encode(&shared, &mut encoder, event);
        })?);
        let shared = Arc::clone(&this.shared);
        this.threads.push(spawn(Some(FINISH_THREAD), move || {
            let _guard = crate::ffi::AbortOnPanic;
            finish(&shared);
        })?);
        this.threads.push(spawn(None, move || {
            let _guard = crate::ffi::AbortOnPanic;
            // SAFETY: the listener `new` created; it returns when `drop` destroys it.
            unsafe { SharedEventListener_start(listener) };
        })?);
        Ok(this)
    }

    /// The sizes the queue was made with.
    #[must_use]
    pub const fn config(&self) -> &QueueConfig {
        &self.config
    }

    /// The initializer of new resources, whose work the next committed chunk waits for.
    #[must_use]
    pub fn initializer(&self) -> &ResourceInitializer {
        &self.shared.initializer
    }

    /// Rolling 16-frame statistics container (`CommandQueue::statistics`).
    pub fn statistics(&self) -> MutexGuard<'_, Container> {
        self.shared.statistics()
    }

    /// Mutable access to the current frame's statistics (`currentFrameStatistics`).
    pub fn with_current_frame_statistics<R>(&self, f: impl FnOnce(&mut FrameStatistics) -> R) -> R {
        let frame = self.frame.load(Ordering::Relaxed);
        let mut stats = self.shared.statistics();
        f(stats.at_mut(frame))
    }

    /// Allocates `(block, offset)` from the queue's CPU-visible staging ring.
    pub fn allocate_staging_buffer(&mut self, size: u64, align: u64) -> (BlockRef, u64) {
        let (seq, coherent) = (self.current_seq(), self.coherent_seq());
        self.staging.allocate(seq, coherent, size, align)
    }

    /// The chunk being recorded: the recording thread's alone, which `&mut self` makes so.
    pub fn chunk(&mut self) -> &mut Chunk {
        let slot = self.shared.slot(self.shared.ring.current());
        // SAFETY: the ring gives the chunk being recorded to the recording thread, and `&mut
        // self` keeps it to this one caller.
        unsafe { &mut *slot }
    }

    /// Hands the current chunk to the encode thread, waiting while `chunks - 1` are in flight.
    pub fn commit(&mut self) {
        self.event_seq += 1;
        let (id, event_id) = (self.current_seq(), self.event_seq);
        let frame = self.frame.load(Ordering::Relaxed);
        let initializer_wait = self.shared.initializer.flush_to_wait();
        let chunk = self.chunk();
        chunk.id = id;
        chunk.event_id = event_id;
        chunk.frame = frame;
        chunk.initializer_wait = initializer_wait;
        let t0 = Instant::now();
        self.shared.ring.commit();
        if !cfg!(feature = "perf-build") {
            let elapsed = t0.elapsed();
            let mut stats = self.shared.statistics();
            let fstats = stats.at_mut(frame);
            fstats.command_buffer_count += 1;
            fstats.commit_interval += elapsed;
            drop(stats);
        }
    }

    /// The chunk being recorded's sequence number.
    #[must_use]
    pub fn current_seq(&self) -> u64 {
        self.shared.ring.current()
    }

    /// The last chunk the GPU finished and the queue retired.
    #[must_use]
    pub fn coherent_seq(&self) -> u64 {
        self.shared.ring.coherent()
    }

    /// Blocks until chunk `seq` is retired.
    pub fn wait_cpu(&self, seq: u64) {
        let t0 = Instant::now();
        self.shared.ring.finished.wait(seq);
        if !cfg!(feature = "perf-build") {
            let frame = self.frame.load(Ordering::Relaxed);
            self.shared
                .statistics()
                .at_mut(frame)
                .add_sync_interval(t0.elapsed());
        }
    }

    /// The next shared-event value, taken for a chunk or an event query.
    pub const fn next_event_seq(&mut self) -> u64 {
        self.event_seq += 1;
        self.event_seq
    }

    /// The last shared-event value taken.
    #[must_use]
    pub const fn current_event_seq(&self) -> u64 {
        self.event_seq
    }

    /// The shared event's signaled value.
    #[must_use]
    pub fn signaled_event_seq(&self) -> u64 {
        // SAFETY: a live shared event, or nil, which answers 0.
        unsafe { MTLSharedEvent_signaledValue(self.shared_event()) }
    }

    /// The shared event the chunks signal (borrowed).
    #[must_use]
    pub fn shared_event(&self) -> u64 {
        self.event.as_ref().map_or(0, Obj::handle)
    }

    /// The listener that sets Win32 events at shared-event values (borrowed).
    #[must_use]
    pub const fn shared_event_listener(&self) -> u64 {
        self.listener
    }

    /// The frame being recorded: frames presented, plus one.
    #[must_use]
    pub fn current_frame_seq(&self) -> u64 {
        self.frame.load(Ordering::Relaxed) + 1
    }

    /// Counts a present.
    ///
    /// With `wait`, blocks until the frame `max_latency` presents back is retired. A VM host
    /// passes false: its guest paces the frames.
    pub fn present_boundary(&self, wait: bool) {
        let prev = self.frame.fetch_add(1, Ordering::Relaxed);
        let frame = prev + 1;
        {
            let mut stats = self.shared.statistics();
            stats.compute(prev);
            stats.at_mut(frame).reset();
        }
        let latency = u64::from(self.max_latency.load(Ordering::Relaxed));
        if wait && frame > latency {
            let t0 = Instant::now();
            self.shared.frame_latency.wait(frame - latency);
            if !cfg!(feature = "perf-build") {
                let mut stats = self.shared.statistics();
                stats.at_mut(prev).present_latency_interval += t0.elapsed();
            }
        }
        let completed = self.shared.frame_latency.signaled();
        if !cfg!(feature = "perf-build") {
            let mut stats = self.shared.statistics();
            stats.at_mut(prev).latency = u32::try_from(prev.saturating_sub(completed)).unwrap_or(0);
        }
    }

    /// Frames the CPU may run ahead of the GPU.
    #[must_use]
    pub fn max_latency(&self) -> u32 {
        self.max_latency.load(Ordering::Relaxed)
    }

    /// Sets the frames the CPU may run ahead of the GPU.
    pub fn set_max_latency(&self, frames: u32) {
        self.max_latency.store(frames, Ordering::Relaxed);
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        self.shared.ring.stop();
        // SAFETY: the listener `new` created, destroyed this once; its thread's start returns.
        unsafe { SharedEventListener_destroy(self.listener) };
        for thread in self.threads.drain(..) {
            sync::wait_until_finished(thread);
        }
    }
}

fn spawn(name: Option<&str>, f: impl FnOnce() + Send + 'static) -> io::Result<JoinHandle<()>> {
    let builder = thread::Builder::new().stack_size(STACK);
    match name {
        Some(name) => builder.name(name.to_owned()),
        None => builder,
    }
    .spawn(f)
}

/// `dxmt-encode-thread`: each committed chunk's commands into a command buffer, in order.
///
/// The encoder's flush encodes the chunk's passes and signals `event` at the chunk's value.
// ponytail: the capture joins this loop.
fn encode(shared: &Shared, encoder: &mut Encoder, event: u64) {
    sync::set_time_critical();
    let queue = shared.queue.as_ref().map_or(0, Obj::handle);
    let mut seq = 1;
    while shared.ring.wait_committed(seq) {
        let slot = shared.slot(seq);
        // SAFETY: chunk `seq` is committed and not counted encoded: the encode thread's alone.
        let chunk = unsafe { &mut *slot };
        let _pool = Pool::new();
        // SAFETY: a live command queue, or nil, which answers nil.
        let cmdbuf = unsafe { MTLCommandQueue_commandBuffer(queue) };
        // SAFETY: an autoreleased command buffer the pool keeps alive until here, or nil.
        chunk.cmdbuf = unsafe { Obj::retain(cmdbuf) };
        if chunk.initializer_wait != 0 {
            let event = shared.initializer.event();
            // SAFETY: the command buffer just made, or nil; the initializer's live event.
            unsafe { MTLCommandBuffer_encodeWaitForEvent(cmdbuf, event, chunk.initializer_wait) };
        }
        encoder.begin_chunk(seq);
        chunk.commands.run(encoder);
        // SAFETY: the command buffer just made, or nil; the queue's shared event, or nil, which
        // `drop` releases only after this thread ends; the allocations the passes name are
        // retained by the encoder until `end_chunk` hands them to the chunk.
        unsafe { encoder.flush(cmdbuf, event, chunk.event_id, &QueryBuffers::NONE) };
        encoder.end_chunk(&mut chunk.retained);
        // SAFETY: the command buffer just made, or nil, which ignores it.
        unsafe { MTLCommandBuffer_commit(cmdbuf) };
        shared.ring.encoded();
        seq += 1;
    }
}

/// `dxmt-finish-thread`: waits for each chunk's command buffer, then retires the chunk.
// ponytail: no device-error report yet: the core has no log to write it to.
fn finish(shared: &Shared) {
    sync::set_time_critical();
    let mut seq = 1;
    while shared.ring.wait_encoded(seq) {
        let slot = shared.slot(seq);
        // SAFETY: chunk `seq` is encoded and not finished: the finish thread's alone.
        let chunk = unsafe { &mut *slot };
        if let Some(cmdbuf) = &chunk.cmdbuf {
            // SAFETY: a live command buffer the chunk holds.
            let status = unsafe { MTLCommandBuffer_status(cmdbuf.handle()) };
            if status <= WMT_COMMAND_BUFFER_STATUS_SCHEDULED {
                // SAFETY: a live command buffer the chunk holds.
                unsafe { MTLCommandBuffer_waitUntilCompleted(cmdbuf.handle()) };
            }
        }
        if chunk.frame_latency != NO_FRAME {
            shared.frame_latency.signal(chunk.frame_latency);
        }
        chunk.reset();
        shared.ring.finish(seq);
        seq += 1;
    }
}

#[cfg(test)]
mod tests;
