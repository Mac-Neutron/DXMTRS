//! `ID3D12CommandQueue` and `IMTLSwapChainFactory` (`d3d12_command_queue.cpp`): encoder ordering,
//!
//! merging, clear/timestamp folding, in-flight command-buffer pool, completion thread, and present.

pub mod dump;
pub mod merge;
pub mod order;
pub mod timestamp;

use core::{
    cell::Cell,
    ffi::{CStr, c_char, c_void},
    fmt::Write as _,
    mem::offset_of,
    ptr,
    sync::atomic::{AtomicU32, AtomicU64, Ordering, fence},
};
use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread::{self, JoinHandle},
    time::Duration,
};

use dxmt_core::{
    fence::{Fence, Generation},
    presenter::{
        Presenter,
        framegen::{self, FrameGen},
    },
    settings::{Flags, settings},
};
use dxmt_util::{
    com::ComObject,
    private_data::{self, HasPrivateData, PrivateData},
    qpc,
    sync::lock,
};
use winemetal_sys::{
    MTLBlitCommandEncoder_encodeCommands, MTLCommandBuffer_blitCommandEncoder,
    MTLCommandBuffer_blitCommandEncoderWithSampleBuffers, MTLCommandBuffer_commit,
    MTLCommandBuffer_computeCommandEncoder,
    MTLCommandBuffer_computeCommandEncoderWithSampleBuffers, MTLCommandBuffer_encodeSignalEvent,
    MTLCommandBuffer_encodeTemporalScale, MTLCommandBuffer_encodeWaitForEvent,
    MTLCommandBuffer_error, MTLCommandBuffer_presentDrawable,
    MTLCommandBuffer_presentDrawableAfterMinimumDuration, MTLCommandBuffer_renderCommandEncoder,
    MTLCommandBuffer_status, MTLCommandBuffer_waitUntilCompleted, MTLCommandEncoder_endEncoding,
    MTLCommandEncoder_setLabel, MTLCommandQueue_addResidencySet, MTLCommandQueue_commandBuffer,
    MTLCounterSampleBuffer_newTimestampBuffer, MTLCounterSampleBuffer_resolveCounterRange,
    MTLDevice_newCommandQueue, MTLDevice_newFence, MTLDevice_sampleTimestamps,
    MTLRenderCommandEncoder_encodeCommands, MTLSharedEvent_signaledValue,
    MTLSharedEvent_waitUntilSignaledValue, MTLTexture_height, MTLTexture_width,
    NSObject_description, NSString_getCString, NSString_lengthOfBytesUsingEncoding,
    NSString_string, Obj, Pool,
    wire::{
        MemPtr, WMT_BLIT_COMMAND_UPDATE_FENCE, WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
        WMT_COMMAND_BUFFER_STATUS_ERROR, WMT_COMMAND_BUFFER_STATUS_SCHEDULED,
        WMT_LOAD_ACTION_CLEAR, WMT_LOAD_ACTION_LOAD, WMT_RENDER_COMMAND_UPDATE_FENCE,
        WMT_RENDER_STAGE_FRAGMENT, WMT_RENDER_STAGE_PRE_RASTER, WMT_RENDER_STAGE_VERTEX,
        WMT_STORE_ACTION_STORE, WMT_STORE_ACTION_STORE_AND_MULTISAMPLE_RESOLVE, WMTRenderPassInfo,
        WMTRenderPassSampleBufferInfo, WMTSampleBufferAttachmentInfo, WMTUTF8_STRING_ENCODING,
        WmtcmdBase, WmtcmdBlitFenceOp, WmtcmdRenderFenceOp,
    },
};

use self::{
    merge::{AsSubmission, ClearFold, MergePlan, apply_clear, clear_target},
    order::{FENCE_COUNT, FrameGenOrder, NO_FENCE, OrderState, serial_from_env},
    timestamp::{DONT_SAMPLE, LoneReason, RingSampling, TimestampState},
};
use crate::{
    abi::{
        ext::{
            Hwnd, IID_D3D12_COMMAND_QUEUE_DOWNLEVEL, IID_IMTL_SWAP_CHAIN_FACTORY,
            IMTLSwapChainFactoryVtbl, SwapChainDesc1, SwapChainFullscreenDesc,
        },
        queue::CommandQueueDesc,
        vtbl::{ID3D12CommandQueueVtbl, IUnknownVtbl},
    },
    com::{
        self, ComRefCounted, E_FAIL, E_INVALIDARG, Hresult, IID_ID3D10_DEVICE, IID_ID3D10_DEVICE1,
        IID_ID3D11_DEVICE, IID_ID3D11_DEVICE1, IID_ID3D11_DEVICE2, IID_ID3D11_DEVICE3,
        IID_ID3D11_DEVICE4, IID_ID3D11_DEVICE5, IID_ID3D12_COMMAND_QUEUE, IID_ID3D12_DEVICE_CHILD,
        IID_ID3D12_OBJECT, IID_ID3D12_PAGEABLE, IID_IUNKNOWN, S_OK,
    },
    device::Device,
    encoder::{
        BlitEncoderData, ClearEncoderData, ComputeEncoderData, EncoderData, EncoderType,
        RenderEncoderData, ResolveEncoderData, TemporalUpscaleEncoderData, TextureViewRef,
        TimestampResolve,
    },
    fence::{self, FenceObject},
    list::{self, List},
    query_heap::TIMESTAMPS_PER_BUFFER,
    resource::Resource,
    slot, swapchain,
};

#[cfg(windows)]
unsafe extern "system" {
    fn ReleaseSemaphore(semaphore: *mut c_void, count: i32, previous: *mut i32) -> i32;
    fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
}

/// Default and maximum number of in-flight command buffer slots (`kCommandQueueSlots = 64`).
pub const COMMAND_QUEUE_SLOTS: u32 = 64;

/// How many `ExecuteCommandLists` calls may share one Metal command buffer before it is committed.
///
/// Queue operations that a CPU thread can observe (`Signal`, `Wait`, `Present`) commit on their
/// own; this only joins back-to-back executes, where Metal has no ordering boundary to preserve.
const EXECUTE_BATCH_LIMIT: u32 = 8;

/// Clamps `DXMT_D3D12_CMDBUF_SLOTS` to `1..=64`, defaulting to `64` (`SlotsFromEnv`,
///
/// `d3d12_command_queue.cpp:1494-1504`).
#[must_use]
pub fn slots_from_env(raw: Option<&str>) -> u32 {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return COMMAND_QUEUE_SLOTS;
    };
    let Ok(parsed) = s.parse::<u64>() else {
        return COMMAND_QUEUE_SLOTS;
    };
    u32::try_from(parsed)
        .unwrap_or(COMMAND_QUEUE_SLOTS)
        .clamp(1, COMMAND_QUEUE_SLOTS)
}

/// Fence signal deferred behind pending CPU work (`DeferredSignal`,
///
/// `d3d12_command_queue.cpp:1227`).
struct DeferredSignal {
    fence: Arc<Fence>,
    generation: Generation,
    value: u64,
    forward: bool,
}

/// In-flight CPU timestamp resolve (`TimestampResolve`, `d3d12_device.hpp:56-66`).
struct InflightResolve {
    samples: u64,
    start: u32,
    count: u32,
    dst: *mut c_void,
    aliases: Vec<(u32, u32)>,
    values: *const u64,
}

// SAFETY: `dst` points to a GPU-visible destination buffer and `values` points to query-heap
// storage, both kept alive until deferred fence signals complete on the waiting thread.
unsafe impl Send for InflightResolve {}

/// One slot in the queue's in-flight command-buffer ring (`InflightCommandBuffer`,
///
/// `d3d12_command_queue.cpp:1233`).
struct InflightCommandBuffer {
    cmdbuf: Option<Obj>,
    semaphore: *mut c_void,
    owes_cpu: bool,
    waits: Vec<(Obj, u64)>,
    resolves: Vec<InflightResolve>,
    signals: Vec<DeferredSignal>,
    samples: u64,
    sample_first: u64,
    sample_count: u32,
    sample_values: Vec<(u32, *mut u64)>,
}

// SAFETY: `semaphore` is a thread-safe OS semaphore handle (`HANDLE`), and `sample_values` raw
// pointers point into query heaps kept alive until deferred fence signals complete.
unsafe impl Send for InflightCommandBuffer {}

impl InflightCommandBuffer {
    const fn new() -> Self {
        Self {
            cmdbuf: None,
            semaphore: ptr::null_mut(),
            owes_cpu: false,
            waits: Vec::new(),
            resolves: Vec::new(),
            signals: Vec::new(),
            samples: 0,
            sample_first: 0,
            sample_count: 0,
            sample_values: Vec::new(),
        }
    }

    fn clear_for_reuse(&mut self) {
        self.cmdbuf = None;
        self.semaphore = ptr::null_mut();
        self.owes_cpu = false;
        self.waits.clear();
        self.resolves.clear();
        self.signals.clear();
        self.samples = 0;
        self.sample_first = 0;
        self.sample_count = 0;
        self.sample_values.clear();
    }

    const fn ring_sampling(&mut self, samples: u64, ring_done: u64) -> RingSampling<'_> {
        RingSampling {
            samples,
            ring_done,
            sample_first: &mut self.sample_first,
            sample_count: &mut self.sample_count,
            sample_values: &mut self.sample_values,
        }
    }

    const fn ring_sampling_opt(
        &mut self,
        samples: u64,
        ring_done: u64,
    ) -> Option<RingSampling<'_>> {
        if samples != 0 {
            Some(self.ring_sampling(samples, ring_done))
        } else {
            None
        }
    }
}

/// Sequence numbers and active slot count coordinating the submission thread and waiting thread
///
/// (`d3d12_command_queue.cpp:1221-1223`).
struct InflightSync {
    seq: u64,
    count: u32,
    stop: u64,
}

/// Shared pool of in-flight command buffers between [`Queue`] and its completion thread.
pub struct InflightPool {
    slots: [Mutex<InflightCommandBuffer>; COMMAND_QUEUE_SLOTS as usize],
    sync: Mutex<InflightSync>,
    seq_cv: Condvar,
    count_cv: Condvar,
    cpu_work: AtomicU32,
    ring_done: AtomicU64,
    cmdbuf_slots: u32,
    device: *mut Device,
}

// SAFETY: `device` is a thread-safe COM `Device` pointer kept alive for the lifetime of `Queue`.
unsafe impl Send for InflightPool {}
// SAFETY: all mutable state on `InflightPool` is synchronized through `Mutex` or atomics.
unsafe impl Sync for InflightPool {}

impl InflightPool {
    fn new(device: *mut Device, cmdbuf_slots: u32) -> Arc<Self> {
        Arc::new(Self {
            slots: core::array::from_fn(|_| Mutex::new(InflightCommandBuffer::new())),
            sync: Mutex::new(InflightSync {
                seq: 1,
                count: 0,
                stop: 0,
            }),
            seq_cv: Condvar::new(),
            count_cv: Condvar::new(),
            cpu_work: AtomicU32::new(0),
            ring_done: AtomicU64::new(0),
            cmdbuf_slots: cmdbuf_slots.clamp(1, COMMAND_QUEUE_SLOTS),
            device,
        })
    }

    fn owe(&self, inflight: &mut InflightCommandBuffer) {
        if inflight.owes_cpu {
            return;
        }
        inflight.owes_cpu = true;
        self.cpu_work.fetch_add(1, Ordering::Relaxed);
    }

    fn waiting_thread_loop(&self) {
        boost_thread_priority();
        let mut internal_seq = 1u64;
        let mut sample_scratch_values = Vec::<u64>::new();
        loop {
            {
                let mut sync = lock(&self.sync);
                while sync.seq == internal_seq {
                    sync = self
                        .seq_cv
                        .wait(sync)
                        .unwrap_or_else(PoisonError::into_inner);
                }
                if sync.stop == internal_seq {
                    break;
                }
            }

            let slot_idx =
                usize::try_from(internal_seq % u64::from(self.cmdbuf_slots)).unwrap_or(0);
            {
                let _pool = Pool::new();
                let mut inflight = lock(&self.slots[slot_idx]);
                let cmdbuf_handle = inflight.cmdbuf.as_ref().map_or(0, Obj::handle);
                if cmdbuf_handle != 0 {
                    // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer`.
                    let status = unsafe { MTLCommandBuffer_status(cmdbuf_handle) };
                    if status <= WMT_COMMAND_BUFFER_STATUS_SCHEDULED {
                        // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer`.
                        unsafe { MTLCommandBuffer_waitUntilCompleted(cmdbuf_handle) };
                    }
                }
                crate::scope!("queue.(retire a completed command buffer)");
                let status = if cmdbuf_handle != 0 {
                    // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer`.
                    unsafe { MTLCommandBuffer_status(cmdbuf_handle) }
                } else {
                    0
                };
                let failed = status == WMT_COMMAND_BUFFER_STATUS_ERROR;
                if failed {
                    let desc = cmdbuf_error_description(cmdbuf_handle);
                    crate::err!("Device error: {desc}");
                    if !self.device.is_null() {
                        // SAFETY: `self.device` is kept alive by the owning `Queue`.
                        let dev = unsafe { &*self.device };
                        crate::device::null::reset_atomic64_locks(dev);
                    }
                }

                release_semaphore(inflight.semaphore);

                let n = inflight.sample_count;
                if n > 0 {
                    sample_scratch_values.resize(n as usize, 0);
                    let first =
                        u32::try_from(inflight.sample_first % u64::from(TIMESTAMPS_PER_BUFFER))
                            .unwrap_or(0);
                    let head = n.min(TIMESTAMPS_PER_BUFFER - first);
                    // SAFETY: `inflight.samples` is a live `MTLCounterSampleBuffer` and
                    // `sample_scratch_values` has at least `head` `u64` elements.
                    unsafe {
                        MTLCounterSampleBuffer_resolveCounterRange(
                            inflight.samples,
                            first,
                            head,
                            sample_scratch_values.as_mut_ptr().cast::<c_void>(),
                            u64::from(head) * 8,
                        );
                    }
                    if head < n {
                        let tail = n - head;
                        // SAFETY: `head < n` and `sample_scratch_values.len() == n`.
                        let tail_ptr =
                            unsafe { sample_scratch_values.as_mut_ptr().add(head as usize) };
                        // SAFETY: `inflight.samples` is live and `tail_ptr` has `tail` elements.
                        unsafe {
                            MTLCounterSampleBuffer_resolveCounterRange(
                                inflight.samples,
                                0,
                                tail,
                                tail_ptr.cast::<c_void>(),
                                u64::from(tail) * 8,
                            );
                        }
                    }
                    for &(index, value_ptr) in &inflight.sample_values {
                        if !value_ptr.is_null()
                            && let Some(&val) = sample_scratch_values.get(index as usize)
                        {
                            // SAFETY: `value_ptr` points into the query heap's `values` array.
                            unsafe { *value_ptr = val };
                        }
                    }
                    self.ring_done
                        .store(inflight.sample_first + u64::from(n), Ordering::Release);
                }

                if inflight.owes_cpu {
                    if !failed {
                        for (shared, value) in &inflight.waits {
                            // SAFETY: `shared` is a live `MTLSharedEvent`.
                            unsafe {
                                MTLSharedEvent_waitUntilSignaledValue(
                                    shared.handle(),
                                    *value,
                                    u64::MAX,
                                );
                            }
                        }
                    }
                    for r in &inflight.resolves {
                        if !r.values.is_null() {
                            let count = r.count as usize;
                            let dst = r.dst.cast::<u64>();
                            if count > 0 && !dst.is_null() {
                                // SAFETY: `r.values` has at least `r.start + r.count` elements.
                                let src = unsafe { r.values.add(r.start as usize) };
                                // SAFETY: `src` and `dst` are non-overlapping `u64` slices.
                                unsafe { ptr::copy_nonoverlapping(src, dst, count) };
                            }
                            if !dst.is_null() {
                                for &(slot, sample) in &r.aliases {
                                    // SAFETY: `r.values` is valid at `sample`.
                                    let val_ptr = unsafe { r.values.add(sample as usize) };
                                    // SAFETY: `val_ptr` points to a valid `u64` timestamp.
                                    let val = unsafe { *val_ptr };
                                    // SAFETY: `dst` is valid at `slot`.
                                    let dst_slot = unsafe { dst.add(slot as usize) };
                                    // SAFETY: `dst_slot` is writable.
                                    unsafe { *dst_slot = val };
                                }
                            }
                            continue;
                        }
                        let byte_len = u64::from(r.count) * 8;
                        // SAFETY: `r.samples` is a live `MTLCounterSampleBuffer` and `r.dst`
                        // points to `byte_len` writable bytes.
                        unsafe {
                            MTLCounterSampleBuffer_resolveCounterRange(
                                r.samples, r.start, r.count, r.dst, byte_len,
                            );
                        }
                        let dst = r.dst.cast::<u64>();
                        if !dst.is_null() {
                            for &(slot, sample) in &r.aliases {
                                // SAFETY: `dst.add(slot)` points to an 8-byte slot in `r.dst`.
                                let slot_ptr = unsafe { dst.add(slot as usize) }.cast::<c_void>();
                                // SAFETY: `r.samples` is live and `slot_ptr` is 8 writable bytes.
                                unsafe {
                                    MTLCounterSampleBuffer_resolveCounterRange(
                                        r.samples, sample, 1, slot_ptr, 8,
                                    );
                                }
                            }
                        }
                    }
                    for d in &inflight.signals {
                        // SAFETY: `d.generation.shared` is a live `MTLSharedEvent`.
                        unsafe {
                            winemetal_sys::MTLSharedEvent_signalValue(
                                d.generation.shared.handle(),
                                d.value,
                            );
                        }
                        if d.forward || failed {
                            d.fence.forward(&d.generation, d.value);
                        }
                    }
                    self.cpu_work.fetch_sub(1, Ordering::Release);
                }

                inflight.clear_for_reuse();
            }

            {
                let mut sync = lock(&self.sync);
                sync.count -= 1;
            }
            self.count_cv.notify_one();
            internal_seq += 1;
        }
    }
}

/// Submission-side state protected by `mutex_commit_` (`d3d12_command_queue.cpp:1357`).
pub struct QueueCommitState {
    pub order: OrderState,
    pub merge: MergePlan,
    pub timestamps: TimestampState,
    pub open: bool,
    pub open_seq: u64,
    pub execute_calls: u64,
    pub custom_resolves: bool,
    pending_executes: u32,
}

impl QueueCommitState {
    fn new(serial: bool) -> Self {
        let merge = settings().enabled(Flags::MERGE);
        let deferred_resolves = settings().enabled(Flags::DEFER_RESOLVES);
        Self {
            order: OrderState::new(serial),
            merge: MergePlan::new(merge, deferred_resolves),
            timestamps: TimestampState::new(),
            open: false,
            open_seq: 1,
            execute_calls: 0,
            custom_resolves: false,
            pending_executes: 0,
        }
    }
}

/// `MTLD3D12CommandQueueImpl` (`d3d12_command_queue.cpp:1227`).
///
/// Primary vtable pointer (`ID3D12CommandQueue`) sits at offset 0; the companion
/// `IMTLSwapChainFactory` vtable pointer sits at [`Queue::FACTORY_OFFSET`].
#[repr(C)]
pub struct Queue {
    pub vtbl: &'static ID3D12CommandQueueVtbl,
    pub swapchain_factory_vtbl: &'static IMTLSwapChainFactoryVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub mtl_queue: Option<Obj>,
    pub frame_gen: Mutex<FrameGen>,
    pub desc: CommandQueueDesc,
    pub holds_device_ref: bool,
    pub fences: [Option<Obj>; FENCE_COUNT],
    pub pool: Arc<InflightPool>,
    pub commit: Mutex<QueueCommitState>,
    pub wait_thread: Option<JoinHandle<()>>,
    pub queue_index: u32,
}

impl ComRefCounted for Queue {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        if let Ok(fg) = self.frame_gen.lock()
            && fg.running()
        {
            fg.drain();
        }
        {
            let commit = lock(&self.commit);
            if commit.open {
                let slot_idx = usize::try_from(commit.open_seq % u64::from(self.pool.cmdbuf_slots))
                    .unwrap_or(0);
                let inflight = lock(&self.pool.slots[slot_idx]);
                self.commit_open_cmdbuf(commit, inflight);
            } else {
                drop(commit);
            }
            let mut sync = lock(&self.pool.sync);
            let stop_seq = sync.seq;
            sync.stop = stop_seq;
            sync.seq = stop_seq + 1;
        }
        self.pool.seq_cv.notify_one();
        if let Some(handle) = self.wait_thread.take() {
            wait_until_finished(handle);
            dump::save_dumps();
        }
        if self.holds_device_ref && !self.device.is_null() {
            // SAFETY: `self.holds_device_ref` guarantees `self` owns one public reference to
            // `self.device`.
            unsafe { com::release_com(self.device) };
        }
    }
}

impl Queue {
    /// Byte offset of `swapchain_factory_vtbl` inside `Queue` (`IMTLSwapChainFactory` companion
    ///
    /// pointer).
    pub const FACTORY_OFFSET: usize = offset_of!(Self, swapchain_factory_vtbl);

    /// Constructs an in-process test [`Queue`] bound to `device` without spawning a background
    ///
    /// waiting thread.
    #[must_use]
    pub fn new_for_test(device: *mut Device) -> Self {
        Self {
            vtbl: &QUEUE_VTBL,
            swapchain_factory_vtbl: &SWAPCHAIN_FACTORY_VTBL,
            com: ComObject::new(),
            private_data: PrivateData::default(),
            device,
            mtl_queue: None,
            frame_gen: Mutex::new(FrameGen::new()),
            desc: CommandQueueDesc {
                r#type: 0,
                priority: 0,
                flags: 0,
                node_mask: 1,
            },
            holds_device_ref: false,
            fences: core::array::from_fn(|_| None),
            pool: InflightPool::new(device, COMMAND_QUEUE_SLOTS),
            commit: Mutex::new(QueueCommitState::new(serial_from_env(None, None))),
            wait_thread: None,
            queue_index: 0,
        }
    }

    fn mtl_device_obj(&self) -> Option<&Obj> {
        if self.device.is_null() {
            return None;
        }
        // SAFETY: `self.device` is valid for the lifetime of `self`.
        unsafe { (*self.device).mtl.as_ref() }
    }

    fn mtl_device_handle(&self) -> u64 {
        self.mtl_device_obj().map_or(0, Obj::handle)
    }

    fn open_cmdbuf(&self, commit: &mut QueueCommitState) -> (usize, u64) {
        if commit.open {
            let slot_idx =
                usize::try_from(commit.open_seq % u64::from(self.pool.cmdbuf_slots)).unwrap_or(0);
            let handle = lock(&self.pool.slots[slot_idx])
                .cmdbuf
                .as_ref()
                .map_or(0, Obj::handle);
            return (slot_idx, handle);
        }
        let seq = {
            crate::scope!("queue.(wait for a command buffer slot)");
            let mut sync = lock(&self.pool.sync);
            while sync.count >= self.pool.cmdbuf_slots {
                sync = self
                    .pool
                    .count_cv
                    .wait(sync)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            sync.seq
        };
        crate::scope!("queue.(new command buffer)");
        let slot_idx = usize::try_from(seq % u64::from(self.pool.cmdbuf_slots)).unwrap_or(0);
        let mtl_queue = self.mtl_queue.as_ref().map_or(0, Obj::handle);
        // SAFETY: `mtl_queue` is 0 (in fake tests) or a live `MTLCommandQueue` handle.
        let raw_cmdbuf = unsafe { MTLCommandQueue_commandBuffer(mtl_queue) };
        // SAFETY: `commandBuffer` returns an autoreleased `MTLCommandBuffer` handle or 0.
        let cmdbuf_obj = unsafe { Obj::retain(raw_cmdbuf) };
        let handle = cmdbuf_obj.as_ref().map_or(0, Obj::handle);
        {
            let mut inflight = lock(&self.pool.slots[slot_idx]);
            inflight.cmdbuf = cmdbuf_obj;
        }
        commit.open = true;
        commit.open_seq = seq;
        (slot_idx, handle)
    }

    fn commit_open_cmdbuf(
        &self,
        mut commit: MutexGuard<'_, QueueCommitState>,
        mut inflight: MutexGuard<'_, InflightCommandBuffer>,
    ) {
        if !commit.open {
            drop(inflight);
            drop(commit);
            return;
        }
        crate::scope!("queue.(commit)");
        crate::count!("#command buffers committed", 1);
        commit.pending_executes = 0;
        if !self.device.is_null() {
            // SAFETY: `self.device` is a live `Device` pointer for the lifetime of `Queue`.
            crate::device::residency::flush(unsafe { &*self.device });
        }
        let cmdbuf_handle = inflight.cmdbuf.as_ref().map_or(0, Obj::handle);
        // SAFETY: `cmdbuf_handle` is 0 or a live `MTLCommandBuffer` handle.
        unsafe { MTLCommandBuffer_commit(cmdbuf_handle) };
        commit.open = false;
        if self.wait_thread.is_none() {
            inflight.clear_for_reuse();
            drop(inflight);
            drop(commit);
            return;
        }
        drop(inflight);
        {
            let mut sync = lock(&self.pool.sync);
            sync.seq += 1;
            sync.count += 1;
        }
        drop(commit);
        self.pool.seq_cv.notify_one();
    }

    /// Executes `lists` in a single Metal command buffer (`ExecuteCommandLists`,
    ///
    /// `d3d12_command_queue.cpp:1604`).
    pub fn execute(&self, lists: &[&List]) {
        crate::scope!("queue.ExecuteCommandLists");
        if lists.is_empty() {
            return;
        }
        let _pool = Pool::new();
        let mtl_device = self.mtl_device_handle();
        let mut commit_guard = lock(&self.commit);
        let commit = &mut *commit_guard;
        commit.execute_calls += 1;
        let (slot_idx, cmdbuf_handle) = self.open_cmdbuf(commit);
        let mut inflight = lock(&self.pool.slots[slot_idx]);
        let dumping = dump::is_dumping();
        let serial = commit.order.serial || dumping;
        let fold_timestamps = !dumping && commit.merge.merge;
        if commit.timestamps.ring.is_none()
            && !commit.timestamps.ring_failed
            && lists.iter().any(|l| l.as_submission().timestamps)
            && !crate::query_heap::timestamp_blits()
        {
            let raw_ring = if mtl_device != 0 {
                // SAFETY: `mtl_device` is a live `MTLDevice` handle.
                unsafe {
                    MTLCounterSampleBuffer_newTimestampBuffer(
                        mtl_device,
                        TIMESTAMPS_PER_BUFFER,
                        true,
                    )
                }
            } else {
                0
            };
            // SAFETY: `newTimestampBuffer` returns a +1 `MTLCounterSampleBuffer` handle or 0.
            commit.timestamps.ring = unsafe { Obj::adopt(raw_ring) };
            commit.timestamps.ring_failed = commit.timestamps.ring.is_none();
            if commit.timestamps.ring_failed {
                crate::warn!(
                    "D3D12CommandQueue: no counter buffer left for timestamps; they go to their query heaps' own"
                );
            }
        }
        let ring_handle = commit.timestamps.ring.as_ref().map_or(0, Obj::handle);
        if ring_handle != 0 {
            inflight.samples = ring_handle;
        }
        let ring_done = self.pool.ring_done.load(Ordering::Acquire);
        commit.timestamps.taken = false;
        commit.timestamps.taken_stop = ptr::null_mut();
        commit.merge.skipped.clear();
        commit.merge.skip = 0;
        commit.merge.clear_folds.clear();

        for (i, list_item) in lists.iter().enumerate() {
            let sub = list_item.as_submission();
            commit.custom_resolves |= sub.custom_resolves;
            if !sub.timestamp_resolves.is_empty() {
                self.pool.owe(&mut inflight);
                let ring_failed = commit.timestamps.ring_failed;
                for r in &sub.timestamp_resolves {
                    inflight.resolves.push(clone_resolve(r, ring_failed));
                }
            }
            let mut current = sub.entry;
            let mut previous: *mut EncoderData = ptr::null_mut();
            commit.order.list_join = 0;
            while !current.is_null() {
                if commit.timestamps.taken && ptr::eq(current, commit.timestamps.taken_stop) {
                    commit.timestamps.taken = false;
                }
                if commit.merge.skip < commit.merge.skipped.len()
                    && ptr::eq(commit.merge.skipped[commit.merge.skip].0, current)
                {
                    let fence = commit.merge.skipped[commit.merge.skip].1;
                    commit.merge.skip += 1;
                    // SAFETY: `current` is a valid encoder header in `sub`.
                    let cur_ref = unsafe { &*current };
                    let pos = cur_ref.position as usize;
                    if pos >= commit.order.list_fences.len() {
                        commit.order.list_fences.resize(pos + 1, NO_FENCE);
                    }
                    commit.order.list_fences[pos] = fence;
                    if cur_ref.kind == EncoderType::Render {
                        crate::count!("#render passes merged", 1);
                    } else if cur_ref.query_resolve_only {
                        crate::count!("#timestamp resolves moved after a merged pass", 1);
                    } else {
                        crate::count!("#timestamp blits folded", 1);
                    }
                    current = cur_ref.next;
                    continue;
                }
                // SAFETY: `current` is a valid encoder header in `sub`.
                let cur_ref = unsafe { &*current };
                if fold_timestamps && cur_ref.kind == EncoderType::Clear {
                    let clear_ptr = current.cast::<ClearEncoderData>();
                    // SAFETY: `cur_ref.kind == EncoderType::Clear` guarantees `ClearEncoderData`,
                    // and all encoders in `lists` are valid for the duration of `execute`.
                    if let Some((at, target)) = unsafe { clear_target(clear_ptr, i, lists) } {
                        crate::count!("#clears folded at execute", 1);
                        commit.merge.clear_folds.push(ClearFold {
                            list: at,
                            target: target.cast::<EncoderData>(),
                            clear: clear_ptr,
                        });
                        commit.timestamps.pending_join |= cur_ref.join;
                        current = cur_ref.next;
                        continue;
                    }
                }
                if fold_timestamps && cur_ref.kind == EncoderType::Blit && cur_ref.timestamp_only {
                    if !commit.timestamps.taken {
                        let n = usize::from(cur_ref.num_samples);
                        commit
                            .timestamps
                            .pending_samples
                            .extend_from_slice(&cur_ref.samples[..n]);
                    }
                    commit.timestamps.pending_join |= cur_ref.join;
                    current = cur_ref.next;
                    continue;
                }
                let mut strict =
                    serial || (cur_ref.kind != EncoderType::Null && commit.timestamps.pending_join);
                if cur_ref.kind != EncoderType::Null {
                    commit.timestamps.pending_join = false;
                }
                let mut pass = 0u32;
                if crate::stats::on() {
                    #[cfg(not(feature = "perf-build"))]
                    static IDS: std::sync::LazyLock<[crate::stats::StatId; 9]> =
                        std::sync::LazyLock::new(|| {
                            [
                                crate::stats::id("#null encoders"),
                                crate::stats::id("#clear passes"),
                                crate::stats::id("#render passes"),
                                crate::stats::id("#blit passes"),
                                crate::stats::id("#compute passes"),
                                crate::stats::id("#resolve passes"),
                                crate::stats::id("#temporal upscale passes"),
                                crate::stats::id("#encoder boundaries"),
                                crate::stats::id("#encoder boundaries with no barrier"),
                            ]
                        });
                    pass = crate::stats::next_pass();
                    #[cfg(not(feature = "perf-build"))]
                    {
                        let kind_idx = match cur_ref.kind {
                            EncoderType::Null => 0,
                            EncoderType::Clear => 1,
                            EncoderType::Render => 2,
                            EncoderType::Blit => 3,
                            EncoderType::Compute => 4,
                            EncoderType::Resolve => 5,
                            EncoderType::TemporalUpscale => 6,
                        };
                        crate::stats::count_id(IDS[kind_idx], 1);
                    }
                    if cur_ref.kind != EncoderType::Null {
                        if !previous.is_null() {
                            #[cfg(not(feature = "perf-build"))]
                            crate::stats::count_id(IDS[7], 1);
                            // SAFETY: `previous` points to an earlier valid encoder in `sub`.
                            let prev_barriers = unsafe { (*previous).barriers };
                            if cur_ref.barriers == prev_barriers {
                                #[cfg(not(feature = "perf-build"))]
                                crate::stats::count_id(IDS[8], 1);
                            }
                        }
                        previous = current;
                    }
                }
                match cur_ref.kind {
                    EncoderType::Null => {}
                    EncoderType::Clear => {
                        crate::scope!("queue.(encode clear)");
                        let data = current.cast::<ClearEncoderData>();
                        // SAFETY: `cur_ref.kind == EncoderType::Clear` guarantees
                        // `ClearEncoderData`.
                        let data = unsafe { &*data };
                        let mut info = init_render_pass_info();
                        let tex = data.attachment.as_ref().map_or(0, TextureViewRef::texture);
                        if data.clear_dsv != 0 {
                            if (data.clear_dsv & 1) != 0 {
                                info.depth.clear_depth = data.depth_stencil.0;
                                info.depth.texture = tex;
                                info.depth.load_action = WMT_LOAD_ACTION_CLEAR;
                                info.depth.store_action = WMT_STORE_ACTION_STORE;
                                info.depth.depth_plane = data.depth_plane;
                            }
                            if (data.clear_dsv & 2) != 0 {
                                info.stencil.clear_stencil = data.depth_stencil.1;
                                info.stencil.texture = tex;
                                info.stencil.load_action = WMT_LOAD_ACTION_CLEAR;
                                info.stencil.store_action = WMT_STORE_ACTION_STORE;
                                info.stencil.depth_plane = data.depth_plane;
                            }
                            info.render_target_width = data.width;
                            info.render_target_height = data.height;
                        } else {
                            info.colors[0].clear_color = data.color;
                            info.colors[0].texture = tex;
                            info.colors[0].load_action = WMT_LOAD_ACTION_CLEAR;
                            info.colors[0].store_action = WMT_STORE_ACTION_STORE;
                            info.colors[0].depth_plane = data.depth_plane;
                        }
                        info.render_target_array_length =
                            u8::try_from(data.array_length).unwrap_or(0);
                        let ring_opt = inflight.ring_sampling_opt(ring_handle, ring_done);
                        if flush_pending(
                            cmdbuf_handle,
                            mtl_device,
                            &mut commit.timestamps,
                            &mut commit.order,
                            ring_opt,
                            &LoneReason::NoStart,
                        ) {
                            self.pool.owe(&mut inflight);
                        }
                        // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer` and `info` is valid.
                        let encoder = unsafe {
                            MTLCommandBuffer_renderCommandEncoder(cmdbuf_handle, &raw mut info)
                        };
                        label_pass(encoder, pass, "clear", None);
                        let fence = commit.order.order(Some(cur_ref), strict);
                        // SAFETY: `encoder` is a valid `MTLRenderCommandEncoder`.
                        unsafe {
                            commit.order.encode_render(
                                encoder,
                                fence,
                                WMT_RENDER_STAGE_FRAGMENT,
                                ptr::null_mut(),
                                ptr::null_mut(),
                                NO_FENCE,
                            );
                        }
                        // SAFETY: `encoder` is ended once.
                        unsafe { MTLCommandEncoder_endEncoding(encoder) };
                    }
                    EncoderType::Render => {
                        crate::scope!("queue.(encode render)");
                        let data = current.cast::<RenderEncoderData>();
                        // SAFETY: `cur_ref.kind == EncoderType::Render` guarantees
                        // `RenderEncoderData`.
                        let data = unsafe { &*data };
                        let mut render_pass_info = build_render_pass_info(data);
                        let mut folded = false;
                        for f in &commit.merge.clear_folds {
                            if f.list == i && ptr::eq(f.target, current) {
                                // SAFETY: `f.clear` is a valid `ClearEncoderData` from `lists`.
                                apply_clear(unsafe { &*f.clear }, data, &mut render_pass_info);
                                folded = true;
                            }
                        }
                        if folded {
                            strict = true;
                        }
                        if !dumping && commit.merge.merge {
                            // SAFETY: `current` and all encoders in `lists` are valid for the call.
                            unsafe {
                                commit.merge.plan_merge(
                                    current.cast::<RenderEncoderData>(),
                                    i,
                                    lists,
                                    &mut render_pass_info,
                                    serial,
                                    ring_handle != 0,
                                );
                            }
                        } else {
                            commit.merge.merged.clear();
                            commit.merge.deferred.clear();
                        }
                        if dumping {
                            if !data.pre_tail.is_null() {
                                let pre_fence = commit.order.order(None, true);
                                encode_indirect_resolve(
                                    &IndirectResolvePass {
                                        cmdbuf: cmdbuf_handle,
                                        pass,
                                        fence: pre_fence,
                                        ring_handle: 0,
                                        start: DONT_SAMPLE,
                                        current: current.cast::<RenderEncoderData>(),
                                        pre_tail: data.pre_tail,
                                    },
                                    &mut commit.order,
                                );
                            }
                            if let (Some(cmdbuf_ref), Some(mtl_dev_ref)) =
                                (inflight.cmdbuf.as_ref(), self.mtl_device_obj())
                            {
                                let data_mut = current.cast::<RenderEncoderData>();
                                let order = &mut commit.order;
                                dump::with_pass_dumps(|d| {
                                    // SAFETY: `data_mut` points to the live `RenderEncoderData` for
                                    // `current`.
                                    dump::pixel_history(
                                        d,
                                        cmdbuf_ref,
                                        mtl_dev_ref,
                                        unsafe { &mut *data_mut },
                                        &render_pass_info,
                                        order,
                                    );
                                });
                            }
                        }
                        let mut resolver_start = DONT_SAMPLE;
                        if ring_handle != 0 {
                            let mut ring_sampling = inflight.ring_sampling(ring_handle, ring_done);
                            let (mut start, owed_start) =
                                commit.timestamps.take_start(&mut ring_sampling);
                            let own_count = usize::from(data.base.num_samples);
                            commit.timestamps.ends.clear();
                            commit
                                .timestamps
                                .ends
                                .extend_from_slice(&data.base.samples[..own_count]);
                            for &m in &commit.merge.merged {
                                // SAFETY: `m` is a valid encoder header in `lists`.
                                let m_ref = unsafe { &*m };
                                let m_count = usize::from(m_ref.num_samples);
                                commit
                                    .timestamps
                                    .ends
                                    .extend_from_slice(&m_ref.samples[..m_count]);
                            }
                            let (trail_enc, trail_list) = match commit.merge.merged.last().copied()
                            {
                                Some(last) => (last, commit.merge.merged_list),
                                None => (current, i),
                            };
                            // SAFETY: `trail_enc` and `lists` are valid for the call.
                            unsafe {
                                commit.timestamps.trailing(
                                    trail_enc,
                                    trail_list,
                                    lists,
                                    fold_timestamps,
                                );
                            }
                            let (end, owed_end) = commit.timestamps.point_ends(&mut ring_sampling);
                            if owed_start || owed_end {
                                self.pool.owe(&mut inflight);
                            }
                            if !data.pre_tail.is_null() && !dumping {
                                core::mem::swap(&mut start, &mut resolver_start);
                            }
                            render_pass_info
                                .sample_buffers
                                .fill(WMTRenderPassSampleBufferInfo {
                                    sample_buffer: 0,
                                    end_of_fragment_sample_index: 0,
                                    start_of_vertex_sample_index: 0,
                                });
                            if start != DONT_SAMPLE || end != DONT_SAMPLE {
                                render_pass_info.sample_buffers[0] =
                                    WMTRenderPassSampleBufferInfo {
                                        sample_buffer: ring_handle,
                                        end_of_fragment_sample_index: end,
                                        start_of_vertex_sample_index: start,
                                    };
                            }
                        } else {
                            if !data.pre_tail.is_null() && !dumping {
                                flush_pending(
                                    cmdbuf_handle,
                                    mtl_device,
                                    &mut commit.timestamps,
                                    &mut commit.order,
                                    None,
                                    &LoneReason::NoStart,
                                );
                            }
                            let own_buf = render_pass_info.sample_buffers[0].sample_buffer;
                            let ts = &mut commit.timestamps;
                            let ord = &mut commit.order;
                            let (buffer, index) = ts.take_pending(own_buf, |buf, idx, why| {
                                sample_after(cmdbuf_handle, mtl_device, ord, buf, idx, &why);
                            });
                            if buffer != 0 {
                                if render_pass_info.sample_buffers[0].sample_buffer != 0 {
                                    render_pass_info.sample_buffers[0]
                                        .start_of_vertex_sample_index = index;
                                } else {
                                    render_pass_info.sample_buffers[0] =
                                        WMTRenderPassSampleBufferInfo {
                                            sample_buffer: buffer,
                                            end_of_fragment_sample_index: DONT_SAMPLE,
                                            start_of_vertex_sample_index: index,
                                        };
                                }
                            }
                        }
                        let fence = commit.order.order(Some(cur_ref), strict);
                        let early = commit.order.early;
                        if !data.pre_tail.is_null() && !dumping {
                            encode_indirect_resolve(
                                &IndirectResolvePass {
                                    cmdbuf: cmdbuf_handle,
                                    pass,
                                    fence,
                                    ring_handle,
                                    start: resolver_start,
                                    current: current.cast::<RenderEncoderData>(),
                                    pre_tail: data.pre_tail,
                                },
                                &mut commit.order,
                            );
                            commit.order.waits.clear();
                            commit.order.waits.push(fence);
                        }
                        let mut trailing = [WMTRenderPassSampleBufferInfo {
                            sample_buffer: 0,
                            end_of_fragment_sample_index: 0,
                            start_of_vertex_sample_index: 0,
                        }; 4];
                        for (slot_idx, item) in trailing.iter_mut().enumerate().skip(1) {
                            core::mem::swap(item, &mut render_pass_info.sample_buffers[slot_idx]);
                        }
                        // SAFETY: `cmdbuf_handle` is live and `render_pass_info` is valid.
                        let encoder = unsafe {
                            MTLCommandBuffer_renderCommandEncoder(
                                cmdbuf_handle,
                                &raw mut render_pass_info,
                            )
                        };
                        label_pass(encoder, pass, "render", Some(data));
                        let mut tail = data.cmd_tail;
                        for &m in &commit.merge.merged {
                            // SAFETY: `m` is a valid encoder header in `lists`.
                            if unsafe { (*m).kind } == EncoderType::Render {
                                let m_render = m.cast::<RenderEncoderData>();
                                // SAFETY: `m_render` is a valid `RenderEncoderData`.
                                let next_head =
                                    unsafe { &raw mut (*m_render).cmd_head }.cast::<u8>();
                                // SAFETY: `tail` is the tail node of the preceding render pass.
                                unsafe { (*tail).next = MemPtr::from_ptr(next_head) };
                                // SAFETY: `m_render` is a valid `RenderEncoderData`.
                                tail = unsafe { (*m_render).cmd_tail };
                            }
                        }
                        let stage = if data.use_geometry {
                            WMT_RENDER_STAGE_PRE_RASTER
                        } else {
                            WMT_RENDER_STAGE_VERTEX
                        };
                        // SAFETY: `current` heads a valid `RenderEncoderData`.
                        let head =
                            unsafe { &raw mut (*current.cast::<RenderEncoderData>()).cmd_head }
                                .cast::<WmtcmdBase>();
                        // SAFETY: `encoder` is a valid `MTLRenderCommandEncoder` and `head..tail`
                        // is a valid command chain.
                        unsafe {
                            commit
                                .order
                                .encode_render(encoder, fence, stage, head, tail, early);
                        }
                        tail = data.cmd_tail;
                        let first_skipped = commit.merge.skipped.len();
                        for &m in &commit.merge.merged {
                            // SAFETY: `m` is a valid encoder header in `lists`.
                            if unsafe { (*m).kind } == EncoderType::Render {
                                // SAFETY: `tail` was linked above; restore its `next` to null.
                                unsafe { (*tail).next = MemPtr::NULL };
                                let m_render = m.cast::<RenderEncoderData>();
                                // SAFETY: `m_render` is a valid `RenderEncoderData`.
                                tail = unsafe { (*m_render).cmd_tail };
                            }
                            commit.merge.skipped.push((m, fence));
                        }
                        // SAFETY: `encoder` is ended once.
                        unsafe { MTLCommandEncoder_endEncoding(encoder) };
                        for t in &trailing {
                            if t.sample_buffer != 0 {
                                sample_after(
                                    cmdbuf_handle,
                                    mtl_device,
                                    &mut commit.order,
                                    t.sample_buffer,
                                    t.end_of_fragment_sample_index,
                                    &LoneReason::CounterBuffer,
                                );
                            }
                        }
                        for &d in &commit.merge.deferred {
                            let blit = d.cast::<BlitEncoderData>();
                            // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer`.
                            let deferred_enc =
                                unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf_handle) };
                            label_pass(
                                deferred_enc,
                                pass,
                                "blit (resolves after the merged pass)",
                                None,
                            );
                            let after = commit.order.order(None, true);
                            // SAFETY: `blit` is a valid `BlitEncoderData`.
                            let d_head = unsafe { &raw mut (*blit).cmd_head }.cast::<WmtcmdBase>();
                            // SAFETY: `blit` is a valid `BlitEncoderData`.
                            let d_tail = unsafe { (*blit).cmd_tail };
                            // SAFETY: `deferred_enc` and `d_head..d_tail` are valid.
                            unsafe {
                                commit
                                    .order
                                    .encode_blit(deferred_enc, after, d_head, d_tail);
                            }
                            // SAFETY: `deferred_enc` is ended once.
                            unsafe { MTLCommandEncoder_endEncoding(deferred_enc) };
                            for skipped_entry in &mut commit.merge.skipped[first_skipped..] {
                                if ptr::eq(skipped_entry.0, d) {
                                    skipped_entry.1 = after;
                                }
                            }
                        }
                    }
                    EncoderType::Blit => {
                        crate::scope!("queue.(encode blit)");
                        let data = current.cast::<BlitEncoderData>();
                        // SAFETY: `cur_ref.kind == EncoderType::Blit` guarantees `BlitEncoderData`.
                        let data = unsafe { &*data };
                        let mut samples = [WMTSampleBufferAttachmentInfo {
                            sample_buffer: 0,
                            start_of_encoder_sample_index: DONT_SAMPLE,
                            end_of_encoder_sample_index: DONT_SAMPLE,
                        }; 4];
                        let mut num_samples = u32::from(data.base.num_samples);
                        for (dst, src) in samples
                            .iter_mut()
                            .zip(&data.base.samples[..usize::from(data.base.num_samples)])
                        {
                            *dst = WMTSampleBufferAttachmentInfo {
                                sample_buffer: src.buffer,
                                start_of_encoder_sample_index: DONT_SAMPLE,
                                end_of_encoder_sample_index: u64::from(src.index),
                            };
                        }
                        if ring_handle != 0 {
                            let mut ring_sampling = inflight.ring_sampling(ring_handle, ring_done);
                            // SAFETY: `current` and `lists` are valid for the call.
                            let (att, count, owed) = unsafe {
                                commit.timestamps.attach(
                                    current,
                                    i,
                                    lists,
                                    fold_timestamps,
                                    &mut ring_sampling,
                                )
                            };
                            samples[0] = att;
                            num_samples = count;
                            if owed {
                                self.pool.owe(&mut inflight);
                            }
                        }
                        // SAFETY: `current` heads a valid `BlitEncoderData`.
                        let head =
                            unsafe { &raw mut (*current.cast::<BlitEncoderData>()).cmd_head }
                                .cast::<WmtcmdBase>();
                        if data.cmd_tail.is_null() || ptr::eq(data.cmd_tail, head) {
                            let ring_opt = inflight.ring_sampling_opt(ring_handle, ring_done);
                            let empty_ctx = EmptyPass {
                                cmdbuf: cmdbuf_handle,
                                mtl_device,
                                current: cur_ref,
                                pass,
                                strict,
                            };
                            if encode_empty(&empty_ctx, commit, ring_opt, &mut samples, num_samples)
                            {
                                self.pool.owe(&mut inflight);
                            }
                        } else {
                            let own_buf = if num_samples > 0 {
                                samples[0].sample_buffer
                            } else {
                                0
                            };
                            let ts = &mut commit.timestamps;
                            let ord = &mut commit.order;
                            let (buffer, index) = ts.take_pending(own_buf, |buf, idx, why| {
                                sample_after(cmdbuf_handle, mtl_device, ord, buf, idx, &why);
                            });
                            if buffer != 0 {
                                if num_samples == 0 {
                                    samples[0] = WMTSampleBufferAttachmentInfo {
                                        sample_buffer: buffer,
                                        start_of_encoder_sample_index: index,
                                        end_of_encoder_sample_index: DONT_SAMPLE,
                                    };
                                    num_samples = 1;
                                } else {
                                    samples[0].start_of_encoder_sample_index = index;
                                }
                            }
                            let encoder = if num_samples > 0 {
                                // SAFETY: `cmdbuf_handle` is live and `samples` has `num_samples`
                                // valid entries.
                                unsafe {
                                    MTLCommandBuffer_blitCommandEncoderWithSampleBuffers(
                                        cmdbuf_handle,
                                        samples.as_mut_ptr(),
                                        u64::from(num_samples),
                                    )
                                }
                            } else {
                                // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer`.
                                unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf_handle) }
                            };
                            label_pass(encoder, pass, "blit", None);
                            let fence = commit.order.order(Some(cur_ref), strict);
                            // SAFETY: `encoder` and `head..data.cmd_tail` are valid.
                            unsafe {
                                commit
                                    .order
                                    .encode_blit(encoder, fence, head, data.cmd_tail);
                            }
                            // SAFETY: `encoder` is ended once.
                            unsafe { MTLCommandEncoder_endEncoding(encoder) };
                        }
                    }
                    EncoderType::Compute => {
                        crate::scope!("queue.(encode compute)");
                        let data = current.cast::<ComputeEncoderData>();
                        // SAFETY: `cur_ref.kind == EncoderType::Compute` guarantees
                        // `ComputeEncoderData`.
                        let data = unsafe { &*data };
                        let mut samples = [WMTSampleBufferAttachmentInfo {
                            sample_buffer: 0,
                            start_of_encoder_sample_index: DONT_SAMPLE,
                            end_of_encoder_sample_index: DONT_SAMPLE,
                        }; 4];
                        let mut num_samples = u32::from(data.base.num_samples);
                        for (dst, src) in samples
                            .iter_mut()
                            .zip(&data.base.samples[..usize::from(data.base.num_samples)])
                        {
                            *dst = WMTSampleBufferAttachmentInfo {
                                sample_buffer: src.buffer,
                                start_of_encoder_sample_index: DONT_SAMPLE,
                                end_of_encoder_sample_index: u64::from(src.index),
                            };
                        }
                        if ring_handle != 0 {
                            let mut ring_sampling = inflight.ring_sampling(ring_handle, ring_done);
                            // SAFETY: `current` and `lists` are valid for the call.
                            let (att, count, owed) = unsafe {
                                commit.timestamps.attach(
                                    current,
                                    i,
                                    lists,
                                    fold_timestamps,
                                    &mut ring_sampling,
                                )
                            };
                            samples[0] = att;
                            num_samples = count;
                            if owed {
                                self.pool.owe(&mut inflight);
                            }
                        }
                        // SAFETY: `current` heads a valid `ComputeEncoderData`.
                        let head =
                            unsafe { &raw mut (*current.cast::<ComputeEncoderData>()).cmd_head }
                                .cast::<WmtcmdBase>();
                        if data.cmd_tail.is_null() || ptr::eq(data.cmd_tail, head) {
                            let ring_opt = inflight.ring_sampling_opt(ring_handle, ring_done);
                            let empty_ctx = EmptyPass {
                                cmdbuf: cmdbuf_handle,
                                mtl_device,
                                current: cur_ref,
                                pass,
                                strict,
                            };
                            if encode_empty(&empty_ctx, commit, ring_opt, &mut samples, num_samples)
                            {
                                self.pool.owe(&mut inflight);
                            }
                        } else {
                            let own_buf = if num_samples > 0 {
                                samples[0].sample_buffer
                            } else {
                                0
                            };
                            let ts = &mut commit.timestamps;
                            let ord = &mut commit.order;
                            let (buffer, index) = ts.take_pending(own_buf, |buf, idx, why| {
                                sample_after(cmdbuf_handle, mtl_device, ord, buf, idx, &why);
                            });
                            if buffer != 0 {
                                if num_samples == 0 {
                                    samples[0] = WMTSampleBufferAttachmentInfo {
                                        sample_buffer: buffer,
                                        start_of_encoder_sample_index: index,
                                        end_of_encoder_sample_index: DONT_SAMPLE,
                                    };
                                    num_samples = 1;
                                } else {
                                    samples[0].start_of_encoder_sample_index = index;
                                }
                            }
                            let encoder = if num_samples > 0 {
                                // SAFETY: `cmdbuf_handle` is live and `samples` has `num_samples`
                                // valid entries.
                                unsafe {
                                    MTLCommandBuffer_computeCommandEncoderWithSampleBuffers(
                                        cmdbuf_handle,
                                        false,
                                        samples.as_mut_ptr(),
                                        u64::from(num_samples),
                                    )
                                }
                            } else {
                                // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer`.
                                unsafe {
                                    MTLCommandBuffer_computeCommandEncoder(cmdbuf_handle, false)
                                }
                            };
                            label_pass(encoder, pass, "compute", None);
                            let fence = commit.order.order(Some(cur_ref), strict);
                            // SAFETY: `encoder` and `head..data.cmd_tail` are valid.
                            unsafe {
                                commit
                                    .order
                                    .encode_compute(encoder, fence, head, data.cmd_tail);
                            }
                            // SAFETY: `encoder` is ended once.
                            unsafe { MTLCommandEncoder_endEncoding(encoder) };
                        }
                    }
                    EncoderType::Resolve => {
                        crate::scope!("queue.(encode resolve)");
                        let data = current.cast::<ResolveEncoderData>();
                        // SAFETY: `cur_ref.kind == EncoderType::Resolve` guarantees
                        // `ResolveEncoderData`.
                        let data = unsafe { &*data };
                        let mut info = init_render_pass_info();
                        info.colors[0].texture =
                            data.src.as_ref().map_or(0, TextureViewRef::texture);
                        info.colors[0].load_action = WMT_LOAD_ACTION_LOAD;
                        info.colors[0].store_action =
                            WMT_STORE_ACTION_STORE_AND_MULTISAMPLE_RESOLVE;
                        info.colors[0].resolve_texture =
                            data.dst.as_ref().map_or(0, TextureViewRef::texture);
                        let ring_opt = inflight.ring_sampling_opt(ring_handle, ring_done);
                        if flush_pending(
                            cmdbuf_handle,
                            mtl_device,
                            &mut commit.timestamps,
                            &mut commit.order,
                            ring_opt,
                            &LoneReason::NoStart,
                        ) {
                            self.pool.owe(&mut inflight);
                        }
                        // SAFETY: `cmdbuf_handle` is live and `info` is valid.
                        let encoder = unsafe {
                            MTLCommandBuffer_renderCommandEncoder(cmdbuf_handle, &raw mut info)
                        };
                        set_cstr_label(encoder, c"ResolvePass");
                        let fence = commit.order.order(Some(cur_ref), strict);
                        // SAFETY: `encoder` is a valid `MTLRenderCommandEncoder`.
                        unsafe {
                            commit.order.encode_render(
                                encoder,
                                fence,
                                WMT_RENDER_STAGE_FRAGMENT,
                                ptr::null_mut(),
                                ptr::null_mut(),
                                NO_FENCE,
                            );
                        }
                        // SAFETY: `encoder` is ended once.
                        unsafe { MTLCommandEncoder_endEncoding(encoder) };
                    }
                    EncoderType::TemporalUpscale => {
                        crate::scope!("queue.(encode temporal upscale)");
                        let data = current.cast::<TemporalUpscaleEncoderData>();
                        // SAFETY: `cur_ref.kind == EncoderType::TemporalUpscale` guarantees
                        // `TemporalUpscaleEncoderData`.
                        let data = unsafe { &*data };
                        let scaler_fence = data
                            .scaler
                            .as_deref()
                            .and_then(|s| s.fence())
                            .map_or(0, Obj::handle);
                        let scaler_handle = data
                            .scaler
                            .as_deref()
                            .and_then(|s| s.scaler())
                            .map_or(0, Obj::handle);
                        let mut first = WMTSampleBufferAttachmentInfo {
                            sample_buffer: 0,
                            start_of_encoder_sample_index: DONT_SAMPLE,
                            end_of_encoder_sample_index: DONT_SAMPLE,
                        };
                        let mut last = first;
                        if ring_handle != 0 {
                            let mut ring_sampling = inflight.ring_sampling(ring_handle, ring_done);
                            let (start, owed_start) =
                                commit.timestamps.take_start(&mut ring_sampling);
                            first = WMTSampleBufferAttachmentInfo {
                                sample_buffer: ring_handle,
                                start_of_encoder_sample_index: start,
                                end_of_encoder_sample_index: DONT_SAMPLE,
                            };
                            commit.timestamps.ends.clear();
                            // SAFETY: `current` and `lists` are valid for the call.
                            unsafe {
                                commit
                                    .timestamps
                                    .trailing(current, i, lists, fold_timestamps);
                            }
                            let (end, owed_end) = commit.timestamps.point_ends(&mut ring_sampling);
                            last = WMTSampleBufferAttachmentInfo {
                                sample_buffer: ring_handle,
                                start_of_encoder_sample_index: DONT_SAMPLE,
                                end_of_encoder_sample_index: end,
                            };
                            if owed_start || owed_end {
                                self.pool.owe(&mut inflight);
                            }
                        } else {
                            flush_pending(
                                cmdbuf_handle,
                                mtl_device,
                                &mut commit.timestamps,
                                &mut commit.order,
                                None,
                                &LoneReason::NoStart,
                            );
                        }
                        let fence = commit.order.order(Some(cur_ref), strict);
                        let fill = commit.order.scratch_fill(mtl_device);
                        let begin = if first.start_of_encoder_sample_index == DONT_SAMPLE {
                            // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer`.
                            unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf_handle) }
                        } else {
                            // SAFETY: `cmdbuf_handle` is live and `first` is valid.
                            unsafe {
                                MTLCommandBuffer_blitCommandEncoderWithSampleBuffers(
                                    cmdbuf_handle,
                                    &raw mut first,
                                    1,
                                )
                            }
                        };
                        label_pass(begin, pass, "upscale", None);
                        for idx in 0..commit.order.waits.len() {
                            let wait = commit.order.waits[idx];
                            let op = WmtcmdBlitFenceOp {
                                r#type: WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
                                reserved: [0; 3],
                                next: MemPtr::NULL,
                                fence: commit.order.fence_handles[usize::from(wait)],
                            };
                            // SAFETY: `begin` is valid and `op` is live on the stack.
                            unsafe {
                                MTLBlitCommandEncoder_encodeCommands(
                                    begin,
                                    ptr::from_ref(&op).cast::<WmtcmdBase>(),
                                );
                            }
                        }
                        // SAFETY: `begin` is valid and `fill` is live on the stack.
                        unsafe {
                            MTLBlitCommandEncoder_encodeCommands(
                                begin,
                                ptr::from_ref(&fill).cast::<WmtcmdBase>(),
                            );
                        }
                        let update_scaler = WmtcmdBlitFenceOp {
                            r#type: WMT_BLIT_COMMAND_UPDATE_FENCE,
                            reserved: [0; 3],
                            next: MemPtr::NULL,
                            fence: scaler_fence,
                        };
                        // SAFETY: `begin` is valid and `update_scaler` is live on the stack.
                        unsafe {
                            MTLBlitCommandEncoder_encodeCommands(
                                begin,
                                ptr::from_ref(&update_scaler).cast::<WmtcmdBase>(),
                            );
                        }
                        // SAFETY: `begin` is ended once.
                        unsafe { MTLCommandEncoder_endEncoding(begin) };
                        let color_tex = data.color.as_ref().map_or(0, TextureViewRef::texture);
                        let output_tex = data.output.as_ref().map_or(0, TextureViewRef::texture);
                        let depth_tex = data.depth.as_ref().map_or(0, TextureViewRef::texture);
                        let motion_tex = data.motion.as_ref().map_or(0, TextureViewRef::texture);
                        let exposure_tex =
                            data.exposure.as_ref().map_or(0, TextureViewRef::texture);
                        let reactive_tex =
                            data.reactive.as_ref().map_or(0, TextureViewRef::texture);
                        // SAFETY: `cmdbuf_handle` is live and `data.props` is valid.
                        unsafe {
                            MTLCommandBuffer_encodeTemporalScale(
                                cmdbuf_handle,
                                scaler_handle,
                                color_tex,
                                output_tex,
                                depth_tex,
                                motion_tex,
                                exposure_tex,
                                scaler_fence,
                                &raw const data.props,
                                reactive_tex,
                            );
                        }
                        if framegen::mode() != framegen::Mode::Off {
                            // SAFETY: `depth_tex` is 0 or a live `MTLTexture` handle.
                            let depth_obj = unsafe { Obj::retain(depth_tex) };
                            // SAFETY: `motion_tex` is 0 or a live `MTLTexture` handle.
                            let motion_obj = unsafe { Obj::retain(motion_tex) };
                            if let (Some(scaler), Some(depth_ref), Some(motion_ref)) = (
                                data.scaler.as_deref(),
                                depth_obj.as_ref(),
                                motion_obj.as_ref(),
                            ) {
                                framegen::record_upscale(
                                    scaler,
                                    depth_ref,
                                    motion_ref,
                                    &data.props,
                                );
                            }
                        }
                        let end_enc = if last.end_of_encoder_sample_index == DONT_SAMPLE {
                            // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer`.
                            unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf_handle) }
                        } else {
                            // SAFETY: `cmdbuf_handle` is live and `last` is valid.
                            unsafe {
                                MTLCommandBuffer_blitCommandEncoderWithSampleBuffers(
                                    cmdbuf_handle,
                                    &raw mut last,
                                    1,
                                )
                            }
                        };
                        let wait_scaler = WmtcmdBlitFenceOp {
                            r#type: WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
                            reserved: [0; 3],
                            next: MemPtr::NULL,
                            fence: scaler_fence,
                        };
                        // SAFETY: `end_enc` is valid and `wait_scaler` is live on the stack.
                        unsafe {
                            MTLBlitCommandEncoder_encodeCommands(
                                end_enc,
                                ptr::from_ref(&wait_scaler).cast::<WmtcmdBase>(),
                            );
                        }
                        // SAFETY: `end_enc` is valid and `fill` is live on the stack.
                        unsafe {
                            MTLBlitCommandEncoder_encodeCommands(
                                end_enc,
                                ptr::from_ref(&fill).cast::<WmtcmdBase>(),
                            );
                        }
                        let update_done = WmtcmdBlitFenceOp {
                            r#type: WMT_BLIT_COMMAND_UPDATE_FENCE,
                            reserved: [0; 3],
                            next: MemPtr::NULL,
                            fence: commit.order.fence_handles[usize::from(fence)],
                        };
                        // SAFETY: `end_enc` is valid and `update_done` is live on the stack.
                        unsafe {
                            MTLBlitCommandEncoder_encodeCommands(
                                end_enc,
                                ptr::from_ref(&update_done).cast::<WmtcmdBase>(),
                            );
                        }
                        // SAFETY: `end_enc` is ended once.
                        unsafe { MTLCommandEncoder_endEncoding(end_enc) };
                    }
                }
                if dumping {
                    let cmdbuf_opt = inflight.cmdbuf.as_ref();
                    let mtl_dev_opt = self.mtl_device_obj();
                    let queue_index = self.queue_index;
                    let execute_calls = commit.execute_calls;
                    let order = &mut commit.order;
                    dump::with_pass_dumps(|d| {
                        dump::dump_pass_fenced(
                            d,
                            cur_ref,
                            queue_index,
                            execute_calls,
                            cmdbuf_opt,
                            mtl_dev_opt,
                            Some(order),
                        );
                    });
                }
                current = cur_ref.next;
            }
        }

        let ring_opt = inflight.ring_sampling_opt(ring_handle, ring_done);
        if flush_pending(
            cmdbuf_handle,
            mtl_device,
            &mut commit.timestamps,
            &mut commit.order,
            ring_opt,
            &LoneReason::End,
        ) {
            self.pool.owe(&mut inflight);
        }
        commit.timestamps.pending_join = false;
        commit.timestamps.taken = false;
        commit.pending_executes = commit.pending_executes.saturating_add(1);
        if commit.pending_executes >= EXECUTE_BATCH_LIMIT {
            self.commit_open_cmdbuf(commit_guard, inflight);
        }
    }

    /// Encodes a GPU signal on `fence` to `value` (`Signal`, `d3d12_command_queue.cpp:2039`).
    pub fn signal(&self, fence: &FenceObject, value: u64) -> Hresult {
        crate::scope!("queue.Signal");
        let _pool = Pool::new();
        let mut commit = lock(&self.commit);
        let (slot_idx, cmdbuf_handle) = self.open_cmdbuf(&mut commit);
        let mut inflight = lock(&self.pool.slots[slot_idx]);
        let generation = fence.fence.ask(value);
        let owes = self.pool.cpu_work.load(Ordering::Acquire) != 0;
        let hold = owes && commit.custom_resolves;
        if !hold {
            // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer` and `generation.gpu` is a live
            // `MTLEvent`.
            unsafe {
                MTLCommandBuffer_encodeSignalEvent(cmdbuf_handle, generation.gpu.handle(), value);
            }
        }
        if owes {
            crate::count!("#fence signals deferred to the CPU", 1);
            if hold {
                crate::count!("#fence signals forwarded to the GPU event", 1);
            }
            self.pool.owe(&mut inflight);
            inflight.signals.push(DeferredSignal {
                fence: Arc::clone(&fence.fence),
                generation,
                value,
                forward: hold,
            });
            self.commit_open_cmdbuf(commit, inflight);
            return S_OK;
        }
        // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer` and `generation.shared` is a live
        // `MTLSharedEvent`.
        unsafe {
            MTLCommandBuffer_encodeSignalEvent(cmdbuf_handle, generation.shared.handle(), value);
        }
        self.commit_open_cmdbuf(commit, inflight);
        S_OK
    }

    /// Encodes a GPU wait on `fence` for `value` (`Wait`, `d3d12_command_queue.cpp:2063`).
    pub fn wait(&self, fence: &FenceObject, value: u64) -> Hresult {
        crate::scope!("queue.Wait");
        let generation = fence.fence.current();
        // SAFETY: `generation.shared` is a live `MTLSharedEvent`.
        let signaled = unsafe { MTLSharedEvent_signaledValue(generation.shared.handle()) };
        if signaled >= value {
            crate::count!("#queue waits already met", 1);
            return S_OK;
        }
        crate::count!("#queue waits on the GPU event", 1);
        let _pool = Pool::new();
        let mut commit = lock(&self.commit);
        let (slot_idx, cmdbuf_handle) = self.open_cmdbuf(&mut commit);
        let mut inflight = lock(&self.pool.slots[slot_idx]);
        // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer` and `generation.gpu` is a live
        // `MTLEvent`.
        unsafe {
            MTLCommandBuffer_encodeWaitForEvent(cmdbuf_handle, generation.gpu.handle(), value);
        }
        self.pool.owe(&mut inflight);
        inflight.waits.push((generation.shared, value));
        // A CPU thread can wait on the fence this precedes; make the wait visible now rather
        // than leaving it in a command buffer that only a later Execute would commit.
        self.commit_open_cmdbuf(commit, inflight);
        S_OK
    }

    /// Presents `backbuffer` through `presenter` (`Present`, `d3d12_command_queue.cpp:2122`).
    pub fn present(
        &self,
        presenter: &Arc<Presenter>,
        backbuffer: &Resource,
        latency_waitable: *mut c_void,
        after: f64,
        refresh: f64,
    ) -> Hresult {
        let hr = self.present_frame(presenter, backbuffer, latency_waitable, after, refresh);
        if let Some(mtl_dev) = self.mtl_device_obj() {
            dump::on_present(mtl_dev);
        } else {
            crate::stat_frame!();
        }
        hr
    }

    fn present_frame(
        &self,
        presenter: &Arc<Presenter>,
        backbuffer: &Resource,
        latency_waitable: *mut c_void,
        after: f64,
        refresh: f64,
    ) -> Hresult {
        let mut handed = false;
        let fg_on = framegen::mode() != framegen::Mode::Off;
        // These can wait for the display or rebuild presenter pipelines. Do them before the
        // queue lock so a drawable wait does not block every Execute/Signal on this queue.
        let (frame_id, metadata) = presenter.synchronize();
        let mut pre_acquired = if fg_on && after <= 0.0 {
            None
        } else {
            presenter.acquire_drawable()
        };
        {
            let _pool = Pool::new();
            let mtl_device = self.mtl_device_handle();
            let mut commit = lock(&self.commit);
            let (slot_idx, cmdbuf_handle) = self.open_cmdbuf(&mut commit);
            let mut inflight = lock(&self.pool.slots[slot_idx]);
            let view_handle = backbuffer
                .texture()
                .and_then(|t| t.current().map(|a| a.view(t.full_view()).texture))
                .unwrap_or(0);
            if fg_on && after <= 0.0 {
                // SAFETY: `view_handle` is 0 or a live `MTLTexture` handle owned by `backbuffer`.
                let frame_obj = unsafe { Obj::retain(view_handle) };
                if let (Some(cmdbuf_ref), Some(frame_ref)) =
                    (inflight.cmdbuf.as_ref(), frame_obj.as_ref())
                {
                    let mut fg = lock(&self.frame_gen);
                    let mut fg_order = FrameGenOrder::new(&mut commit.order, mtl_device);
                    handed = fg.present(
                        cmdbuf_ref,
                        presenter,
                        frame_ref,
                        &metadata,
                        refresh,
                        &mut fg_order,
                    );
                }
            }
            if !handed {
                if fg_on {
                    let fg = lock(&self.frame_gen);
                    if fg.running() {
                        fg.drain();
                    }
                }
                let fence_handle = Cell::new(0u64);
                let order = &mut commit.order;
                let wait_fn = |enc| {
                    // SAFETY: `enc` is the present pass's `MTLRenderCommandEncoder`.
                    let fence = unsafe { order.join_render(enc, WMT_RENDER_STAGE_FRAGMENT) };
                    fence_handle.set(order.fence_handles[usize::from(fence)]);
                };
                let update_fn = |enc| {
                    let op = WmtcmdRenderFenceOp {
                        r#type: WMT_RENDER_COMMAND_UPDATE_FENCE,
                        reserved: [0; 3],
                        next: MemPtr::NULL,
                        fence: fence_handle.get(),
                        stages: WMT_RENDER_STAGE_FRAGMENT,
                    };
                    // SAFETY: `enc` is valid and `op` is live on the stack.
                    unsafe {
                        MTLRenderCommandEncoder_encodeCommands(
                            enc,
                            ptr::from_ref(&op).cast::<WmtcmdBase>(),
                        );
                    }
                };
                let drawable = pre_acquired.take().or_else(|| presenter.acquire_drawable());
                // SAFETY: `cmdbuf_handle` and `view_handle` are live Metal handles.
                let drawable = unsafe {
                    presenter.encode_with_drawable(
                        drawable,
                        cmdbuf_handle,
                        view_handle,
                        &metadata,
                        wait_fn,
                        update_fn,
                    )
                };
                let drawable_handle = drawable.as_ref().map_or(0, Obj::handle);
                if after > 0.0 {
                    // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer`.
                    unsafe {
                        MTLCommandBuffer_presentDrawableAfterMinimumDuration(
                            cmdbuf_handle,
                            drawable_handle,
                            after,
                        );
                    }
                } else {
                    // SAFETY: `cmdbuf_handle` is a live `MTLCommandBuffer`.
                    unsafe { MTLCommandBuffer_presentDrawable(cmdbuf_handle, drawable_handle) };
                }
            }
            inflight.semaphore = latency_waitable;
            presenter.frame_done(frame_id);
            self.commit_open_cmdbuf(commit, inflight);
        }
        if handed {
            let fg = lock(&self.frame_gen);
            fg.notify();
        }
        S_OK
    }
}

fn clone_resolve(r: &TimestampResolve, ring_failed: bool) -> InflightResolve {
    InflightResolve {
        samples: r.samples,
        start: r.start,
        count: r.count,
        dst: r.dst,
        aliases: r.aliases.clone(),
        values: if ring_failed { ptr::null() } else { r.values },
    }
}

fn init_render_pass_info() -> WMTRenderPassInfo {
    // SAFETY: `WMTRenderPassInfo` is a `#[repr(C)]` plain-old-data wire struct valid when zeroed.
    let mut info: WMTRenderPassInfo = unsafe { core::mem::zeroed() };
    for color in &mut info.colors {
        color.load_action = WMT_LOAD_ACTION_CLEAR;
        color.store_action = WMT_STORE_ACTION_STORE;
        color.clear_color.a = 1.0;
    }
    info.stencil.load_action = WMT_LOAD_ACTION_CLEAR;
    info.depth.load_action = WMT_LOAD_ACTION_CLEAR;
    info.depth.clear_depth = 1.0;
    info
}

fn build_render_pass_info(data: &RenderEncoderData) -> WMTRenderPassInfo {
    let mut info = init_render_pass_info();
    for (color_info, color_data) in info.colors.iter_mut().zip(&data.colors) {
        let Some(att) = color_data.attachment.as_ref() else {
            continue;
        };
        color_info.texture = att.texture();
        color_info.load_action = color_data.load_action;
        color_info.store_action = color_data.store_action;
        color_info.level = color_data.level;
        color_info.slice = color_data.slice;
        color_info.depth_plane = color_data.depth_plane;
        color_info.clear_color = color_data.clear_color;
        color_info.resolve_texture = color_data
            .resolve_attachment
            .as_ref()
            .map_or(0, TextureViewRef::texture);
        color_info.resolve_level = color_data.resolve_level;
        color_info.resolve_slice = color_data.resolve_slice;
        color_info.resolve_depth_plane = color_data.resolve_depth_plane;
    }
    if let Some(att) = data.depth.attachment.as_ref() {
        info.depth.texture = att.texture();
        info.depth.load_action = data.depth.load_action;
        info.depth.store_action = data.depth.store_action;
        info.depth.level = data.depth.level;
        info.depth.slice = data.depth.slice;
        info.depth.depth_plane = data.depth.depth_plane;
        info.depth.clear_depth = data.depth.clear_depth;
    }
    if let Some(att) = data.stencil.attachment.as_ref() {
        info.stencil.texture = att.texture();
        info.stencil.load_action = data.stencil.load_action;
        info.stencil.store_action = data.stencil.store_action;
        info.stencil.level = data.stencil.level;
        info.stencil.slice = data.stencil.slice;
        info.stencil.depth_plane = data.stencil.depth_plane;
        info.stencil.clear_stencil = data.stencil.clear_stencil;
    }
    info.default_raster_sample_count = data.default_raster_sample_count;
    info.render_target_array_length = u8::try_from(data.render_target_array_length).unwrap_or(0);
    info.render_target_width = data.render_target_width;
    info.render_target_height = data.render_target_height;
    info.visibility_buffer = data.visibility_buffer;
    info.visibility_accumulate = u8::from(data.visibility_buffer != 0);
    for idx in 0..usize::from(data.base.num_samples) {
        info.sample_buffers[idx] = WMTRenderPassSampleBufferInfo {
            sample_buffer: data.base.samples[idx].buffer,
            end_of_fragment_sample_index: u64::from(data.base.samples[idx].index),
            start_of_vertex_sample_index: DONT_SAMPLE,
        };
    }
    info
}

fn sample_after(
    cmdbuf: u64,
    mtl_device: u64,
    order: &mut OrderState,
    sample_buffer: u64,
    index: u64,
    why: &LoneReason,
) {
    crate::count!("#timestamps given their own encoder", 1);
    #[cfg(not(feature = "perf-build"))]
    if crate::stats::on() {
        static IDS: std::sync::LazyLock<[crate::stats::StatId; 4]> =
            std::sync::LazyLock::new(|| {
                [
                    crate::stats::id("#lone timestamps (another counter buffer)"),
                    crate::stats::id("#lone timestamps (several waiting)"),
                    crate::stats::id("#lone timestamps (next encoder takes none)"),
                    crate::stats::id("#lone timestamps (end of call)"),
                ]
            });
        let idx = match why {
            LoneReason::CounterBuffer => 0,
            LoneReason::Several => 1,
            LoneReason::NoStart => 2,
            LoneReason::End => 3,
        };
        crate::stats::count_id(IDS[idx], 1);
    }
    #[cfg(feature = "perf-build")]
    let _ = why;
    let fill = order.scratch_fill(mtl_device);
    let mut attachment = WMTSampleBufferAttachmentInfo {
        sample_buffer,
        start_of_encoder_sample_index: DONT_SAMPLE,
        end_of_encoder_sample_index: index,
    };
    // SAFETY: `cmdbuf` is a live `MTLCommandBuffer` and `attachment` is valid on the stack.
    let blit = unsafe {
        MTLCommandBuffer_blitCommandEncoderWithSampleBuffers(cmdbuf, &raw mut attachment, 1)
    };
    // SAFETY: `blit` is a valid `MTLBlitCommandEncoder`.
    let fence = unsafe { order.join_blit(blit) };
    // SAFETY: `blit` is valid and `fill` is live on the stack.
    unsafe {
        MTLBlitCommandEncoder_encodeCommands(blit, ptr::from_ref(&fill).cast::<WmtcmdBase>());
    }
    let update = WmtcmdBlitFenceOp {
        r#type: WMT_BLIT_COMMAND_UPDATE_FENCE,
        reserved: [0; 3],
        next: MemPtr::NULL,
        fence: order.fence_handles[usize::from(fence)],
    };
    // SAFETY: `blit` is valid and `update` is live on the stack.
    unsafe {
        MTLBlitCommandEncoder_encodeCommands(blit, ptr::from_ref(&update).cast::<WmtcmdBase>());
    }
    // SAFETY: `blit` is ended once.
    unsafe { MTLCommandEncoder_endEncoding(blit) };
}

fn flush_pending(
    cmdbuf: u64,
    mtl_device: u64,
    timestamps: &mut TimestampState,
    order: &mut OrderState,
    ring: Option<RingSampling<'_>>,
    why: &LoneReason,
) -> bool {
    let mut owed = false;
    match ring {
        None => {
            for idx in 0..timestamps.pending_samples.len() {
                let s = &timestamps.pending_samples[idx];
                sample_after(cmdbuf, mtl_device, order, s.buffer, u64::from(s.index), why);
            }
        }
        Some(mut r) => {
            let ring_buf = r.samples;
            let (slot, newly_owed) = timestamps.point_pending(&mut r);
            owed = newly_owed;
            if slot != DONT_SAMPLE {
                sample_after(cmdbuf, mtl_device, order, ring_buf, slot, why);
            }
        }
    }
    timestamps.pending_samples.clear();
    owed
}

struct IndirectResolvePass {
    cmdbuf: u64,
    pass: u32,
    fence: u16,
    ring_handle: u64,
    start: u64,
    current: *mut RenderEncoderData,
    pre_tail: *mut WmtcmdBase,
}

fn encode_indirect_resolve(ctx: &IndirectResolvePass, order: &mut OrderState) {
    crate::count!("#indirect resolve passes", 1);
    let mut at = WMTSampleBufferAttachmentInfo {
        sample_buffer: if ctx.start == DONT_SAMPLE {
            0
        } else {
            ctx.ring_handle
        },
        start_of_encoder_sample_index: ctx.start,
        end_of_encoder_sample_index: DONT_SAMPLE,
    };
    let pre = if at.sample_buffer != 0 {
        // SAFETY: `ctx.cmdbuf` is live and `at` is valid.
        unsafe {
            MTLCommandBuffer_computeCommandEncoderWithSampleBuffers(
                ctx.cmdbuf,
                true,
                &raw mut at,
                1,
            )
        }
    } else {
        // SAFETY: `ctx.cmdbuf` is a live `MTLCommandBuffer`.
        unsafe { MTLCommandBuffer_computeCommandEncoder(ctx.cmdbuf, true) }
    };
    label_pass(pre, ctx.pass, "indirect resolve", None);
    // SAFETY: `ctx.current` heads a valid `RenderEncoderData`.
    let pre_head = unsafe { &raw mut (*ctx.current).pre_head }.cast::<WmtcmdBase>();
    // SAFETY: `pre` is a valid `MTLComputeCommandEncoder` and
    // `pre_head..ctx.pre_tail` is a valid command chain.
    unsafe {
        order.encode_compute(pre, ctx.fence, pre_head, ctx.pre_tail);
    }
    // SAFETY: `pre` is ended once.
    unsafe { MTLCommandEncoder_endEncoding(pre) };
}

struct EmptyPass<'a> {
    cmdbuf: u64,
    mtl_device: u64,
    current: &'a EncoderData,
    pass: u32,
    strict: bool,
}

fn encode_empty(
    ctx: &EmptyPass<'_>,
    commit: &mut QueueCommitState,
    ring: Option<RingSampling<'_>>,
    samples: &mut [WMTSampleBufferAttachmentInfo; 4],
    num_samples: u32,
) -> bool {
    let owed = flush_pending(
        ctx.cmdbuf,
        ctx.mtl_device,
        &mut commit.timestamps,
        &mut commit.order,
        ring,
        &LoneReason::NoStart,
    );
    let mut fill = commit.order.scratch_fill(ctx.mtl_device);
    let encoder = if num_samples > 0 {
        // SAFETY: `ctx.cmdbuf` is live and `samples` has `num_samples` valid entries.
        unsafe {
            MTLCommandBuffer_blitCommandEncoderWithSampleBuffers(
                ctx.cmdbuf,
                samples.as_mut_ptr(),
                u64::from(num_samples),
            )
        }
    } else {
        // SAFETY: `ctx.cmdbuf` is a live `MTLCommandBuffer`.
        unsafe { MTLCommandBuffer_blitCommandEncoder(ctx.cmdbuf) }
    };
    label_pass(encoder, ctx.pass, "empty", None);
    let fence = commit.order.order(Some(ctx.current), ctx.strict);
    let fill_ptr = ptr::from_mut(&mut fill).cast::<WmtcmdBase>();
    // SAFETY: `encoder` is valid and `fill` is a single-element command chain on the stack.
    unsafe { commit.order.encode_blit(encoder, fence, fill_ptr, fill_ptr) };
    // SAFETY: `encoder` is ended once.
    unsafe { MTLCommandEncoder_endEncoding(encoder) };
    owed
}

struct StackLabel {
    buf: [u8; 64],
    len: usize,
}

impl StackLabel {
    const fn new() -> Self {
        Self {
            buf: [0; 64],
            len: 0,
        }
    }

    fn as_c_ptr(&mut self) -> *const c_char {
        let end = self.len.min(63);
        self.buf[end] = 0;
        self.buf.as_ptr().cast::<c_char>()
    }
}

impl core::fmt::Write for StackLabel {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let avail = 63usize.saturating_sub(self.len);
        let take = s.len().min(avail);
        self.buf[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

fn set_cstr_label(encoder: u64, label: &CStr) {
    // SAFETY: `label` is a valid NUL-terminated string.
    let ns_str = unsafe { NSString_string(label.as_ptr(), WMTUTF8_STRING_ENCODING) };
    // SAFETY: `encoder` is a live `MTLCommandEncoder` and `ns_str` is an autoreleased `NSString`.
    unsafe { MTLCommandEncoder_setLabel(encoder, ns_str) };
}

fn label_pass(encoder: u64, pass: u32, kind: &str, render: Option<&RenderEncoderData>) {
    if !crate::stats::on() {
        return;
    }
    let mut buf = StackLabel::new();
    let _ = write!(buf, "pass-{pass} {kind}");
    if let Some(r) = render {
        let mut colors = 0u32;
        let mut first = 0u64;
        for c in &r.colors {
            if let Some(att) = c.attachment.as_ref() {
                if colors == 0 {
                    first = att.texture();
                }
                colors += 1;
            }
        }
        if first == 0
            && let Some(att) = r.depth.attachment.as_ref()
        {
            first = att.texture();
        }
        let d_suffix = if r.depth.attachment.is_some() {
            "+d"
        } else {
            ""
        };
        let _ = write!(buf, " {colors}c{d_suffix}");
        if first != 0 {
            // SAFETY: `first` is a non-zero `MTLTexture` handle.
            let w = unsafe { MTLTexture_width(first) };
            // SAFETY: `first` is a non-zero `MTLTexture` handle.
            let h = unsafe { MTLTexture_height(first) };
            let _ = write!(buf, " {w}x{h}");
        }
    }
    // SAFETY: `buf.as_c_ptr()` returns a NUL-terminated buffer on the stack.
    let ns_str = unsafe { NSString_string(buf.as_c_ptr(), WMTUTF8_STRING_ENCODING) };
    // SAFETY: `encoder` is a live `MTLCommandEncoder`.
    unsafe { MTLCommandEncoder_setLabel(encoder, ns_str) };
}

fn cmdbuf_error_description(cmdbuf: u64) -> String {
    // SAFETY: `cmdbuf` is a live `MTLCommandBuffer`.
    let err = unsafe { MTLCommandBuffer_error(cmdbuf) };
    if err == 0 {
        return String::new();
    }
    // SAFETY: `err` is an autoreleased `NSError`.
    let desc = unsafe { NSObject_description(err) };
    if desc == 0 {
        return String::new();
    }
    // SAFETY: `desc` is an autoreleased `NSString`.
    let len = unsafe { NSString_lengthOfBytesUsingEncoding(desc, WMTUTF8_STRING_ENCODING) };
    let cap = usize::try_from(len).unwrap_or(0).saturating_add(1);
    let mut buf = vec![0u8; cap];
    // SAFETY: `buf` has capacity `cap` and `desc` is a live `NSString`.
    let ok = unsafe {
        NSString_getCString(
            desc,
            buf.as_mut_ptr().cast::<c_char>(),
            cap as u64,
            WMTUTF8_STRING_ENCODING,
        )
    };
    if ok == 0 {
        return String::new();
    }
    if let Some(nul) = buf.iter().position(|&b| b == 0) {
        buf.truncate(nul);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn release_semaphore(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    #[cfg(windows)]
    {
        // SAFETY: `handle` is a Win32 semaphore handle supplied by the swap chain.
        unsafe { ReleaseSemaphore(handle, 1, ptr::null_mut()) };
    }
    #[cfg(not(windows))]
    {
        // SAFETY: `handle` is a native semaphore handle supplied by the swap chain.
        unsafe { winemetal_sys::wmt_native_release_semaphore(handle, 1, ptr::null_mut()) };
    }
}

#[cfg(windows)]
fn boost_thread_priority() {
    const CURRENT_THREAD: *mut c_void = (-2_isize) as *mut c_void;
    const THREAD_PRIORITY_TIME_CRITICAL: i32 = 15;
    // SAFETY: `CURRENT_THREAD` is the Win32 pseudo-handle for the calling thread.
    unsafe { SetThreadPriority(CURRENT_THREAD, THREAD_PRIORITY_TIME_CRITICAL) };
}

#[cfg(not(windows))]
const fn boost_thread_priority() {}

fn wait_until_finished(handle: JoinHandle<()>) {
    while !handle.is_finished() {
        thread::sleep(Duration::from_millis(1));
    }
    fence(Ordering::Acquire);
    drop(handle);
}

fn timestamp_frequency(device: &Device) -> u64 {
    *device.timestamp_frequency.get_or_init(|| {
        let Some(mtl) = device.mtl.as_ref() else {
            return 1;
        };
        let mut cpu0 = 0u64;
        let mut gpu0 = 0u64;
        let mut cpu1 = 0u64;
        let mut gpu1 = 0u64;
        let (q0, qf) = qpc::now();
        // SAFETY: `mtl` is a live `MTLDevice` and `cpu0`/`gpu0` are valid stack pointers.
        unsafe { MTLDevice_sampleTimestamps(mtl.handle(), &raw mut cpu0, &raw mut gpu0) };
        thread::sleep(Duration::from_millis(10));
        let (q1, _) = qpc::now();
        // SAFETY: `mtl` is a live `MTLDevice` and `cpu1`/`gpu1` are valid stack pointers.
        unsafe { MTLDevice_sampleTimestamps(mtl.handle(), &raw mut cpu1, &raw mut gpu1) };
        if gpu1 <= gpu0 || q1 <= q0 || qf <= 0 {
            return 1;
        }
        let ticks = u128::from(gpu1 - gpu0);
        let freq = u128::try_from(qf).unwrap_or(1);
        let elapsed = u128::try_from(q1 - q0).unwrap_or(1);
        let est = (ticks * freq + elapsed / 2) / elapsed;
        u64::try_from(est).unwrap_or(1).max(1)
    })
}

/// Casts a non-null `ID3D12CommandQueue` pointer if its vtable matches [`Queue`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_queue<'a>(raw: *mut c_void) -> Option<&'a Queue> {
    // SAFETY: forwarded from caller.
    unsafe { com::cast_com::<Queue, ID3D12CommandQueueVtbl>(raw, &QUEUE_VTBL) }
}

/// `ID3D12CommandQueue::QueryInterface` (`d3d12_command_queue.cpp:1549`).
///
/// # Safety
///
/// `this`, `riid`, and `ppv_object` are caller-supplied COM pointers.
unsafe fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> Hresult {
    crate::scope!("queue.QueryInterface");
    let queue_ptr = this.cast::<Queue>();
    // SAFETY: `Queue::FACTORY_OFFSET` is `offset_of!(Queue, swapchain_factory_vtbl)`.
    let factory = unsafe { com::to_companion(queue_ptr, Queue::FACTORY_OFFSET) };
    let qi = com::Qi {
        null_riid: E_INVALIDARG,
        silent: &[
            IID_ID3D10_DEVICE,
            IID_ID3D10_DEVICE1,
            IID_ID3D11_DEVICE,
            IID_ID3D11_DEVICE1,
            IID_ID3D11_DEVICE2,
            IID_ID3D11_DEVICE3,
            IID_ID3D11_DEVICE4,
            IID_ID3D11_DEVICE5,
            IID_D3D12_COMMAND_QUEUE_DOWNLEVEL,
        ],
        log: &crate::LOG,
        miss: com::QiMiss::Once {
            object: &IID_ID3D12_COMMAND_QUEUE,
            name: "D3D12CommandQueue",
        },
    };
    let add_ref = || {
        // SAFETY: `queue_ptr` is a live `Queue`.
        unsafe { com::add_ref_com(queue_ptr) };
    };
    // SAFETY: the application's `riid` and `ppv_object`, null or valid; `this` is a live `Queue`.
    unsafe {
        com::qi_table(
            riid,
            ppv_object,
            &[
                (
                    &[
                        IID_IUNKNOWN,
                        IID_ID3D12_OBJECT,
                        IID_ID3D12_DEVICE_CHILD,
                        IID_ID3D12_PAGEABLE,
                        IID_ID3D12_COMMAND_QUEUE,
                    ],
                    this,
                ),
                (&[IID_IMTL_SWAP_CHAIN_FACTORY], factory),
            ],
            add_ref,
            &qi,
        )
    }
}

/// Primary `ID3D12CommandQueue` vtable for [`Queue`].
impl HasPrivateData for Queue {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `Queue`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

pub static QUEUE_VTBL: ID3D12CommandQueueVtbl = ID3D12CommandQueueVtbl {
    query_interface: slot!(
        (this: *mut c_void, riid: *const c_void, ppv_object: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { query_interface(this, riid, ppv_object) }
        }
    ),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `Queue` COM pointer.
        unsafe { com::add_ref_com(this.cast::<Queue>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `Queue` COM pointer.
        unsafe { com::release_com(this.cast::<Queue>()) }
    }),
    get_private_data: private_data::get_private_data::<Queue>,
    set_private_data: private_data::set_private_data::<Queue>,
    set_private_data_interface: private_data::set_private_data_interface::<Queue>,
    set_name: private_data::set_name::<Queue>,
    get_device: slot!(
        (this: *mut c_void, riid: *const c_void, out_device: *mut *mut c_void) -> Hresult => {
            // SAFETY: `this` is a live `Queue` COM pointer.
            let queue = unsafe { &*this.cast::<Queue>() };
            let dev_ptr = queue.device.cast::<c_void>();
            if dev_ptr.is_null() {
                return E_INVALIDARG;
            }
            // SAFETY: `dev_ptr` is a live `Device` whose vtable starts with `IUnknownVtbl`.
            let unk = unsafe { com::vtbl::<IUnknownVtbl>(dev_ptr) };
            // SAFETY: `dev_ptr` is live and `riid`/`out_device` are forwarded from caller.
            unsafe { (unk.query_interface)(dev_ptr, riid, out_device) }
        }
    ),
    update_tile_mappings: slot!(
        "queue.UpdateTileMappings",
        (
            _this: *mut c_void,
            _resource: *mut c_void,
            _region_count: u32,
            _region_start_coordinates: *const c_void,
            _region_sizes: *const c_void,
            _heap: *mut c_void,
            _range_count: u32,
            _range_flags: *const u32,
            _heap_range_offsets: *const u32,
            _range_tile_counts: *const u32,
            _flags: u32
        ) => {
            let (): () = crate::not_implemented!("ID3D12CommandQueue::UpdateTileMappings");
        }
    ),
    copy_tile_mappings: slot!(
        "queue.CopyTileMappings",
        (
            _this: *mut c_void,
            _dst_resource: *mut c_void,
            _dst_region_start_coordinate: *const c_void,
            _src_resource: *mut c_void,
            _src_region_start_coordinate: *const c_void,
            _region_size: *const c_void,
            _flags: u32
        ) => {
            let (): () = crate::not_implemented!("ID3D12CommandQueue::CopyTileMappings");
        }
    ),
    execute_command_lists: slot!(
        (this: *mut c_void, count: u32, command_lists: *const *mut c_void) => {
            if count == 0 || command_lists.is_null() {
                return;
            }
            // SAFETY: `this` is a live `Queue` COM pointer.
            let queue = unsafe { &*this.cast::<Queue>() };
            let n = count as usize;
            if n <= 64 {
                // SAFETY: `command_lists` points to at least 1 pointer (`count > 0`).
                let first_raw = unsafe { *command_lists };
                // SAFETY: `first_raw` is a caller-supplied command-list pointer.
                let first_opt = unsafe { list::cast_list(first_raw) };
                if let Some(first) = first_opt {
                    let mut stack_lists = [first; 64];
                    let mut valid_len = 0usize;
                    for idx in 0..n {
                        // SAFETY: `idx < n` is within `command_lists`.
                        let elem_ptr = unsafe { command_lists.add(idx) };
                        // SAFETY: `elem_ptr` is readable.
                        let raw = unsafe { *elem_ptr };
                        // SAFETY: `raw` is a caller-supplied command-list pointer.
                        if let Some(l) = unsafe { list::cast_list(raw) } {
                            stack_lists[valid_len] = l;
                            valid_len += 1;
                        }
                    }
                    queue.execute(&stack_lists[..valid_len]);
                    return;
                }
            }
            let mut heap_lists = Vec::with_capacity(n);
            for idx in 0..n {
                // SAFETY: `idx < n` is within `command_lists`.
                let elem_ptr = unsafe { command_lists.add(idx) };
                // SAFETY: `elem_ptr` is readable.
                let raw = unsafe { *elem_ptr };
                // SAFETY: `raw` is a caller-supplied command-list pointer.
                if let Some(l) = unsafe { list::cast_list(raw) } {
                    heap_lists.push(l);
                }
            }
            if !heap_lists.is_empty() {
                queue.execute(&heap_lists);
            }
        }
    ),
    set_marker: slot!(
        "queue.SetMarker",
        (_this: *mut c_void, _metadata: u32, _data: *const c_void, _size: u32) => {}
    ),
    begin_event: slot!(
        "queue.BeginEvent",
        (_this: *mut c_void, _metadata: u32, _data: *const c_void, _size: u32) => {}
    ),
    end_event: slot!("queue.EndEvent", (_this: *mut c_void) => {}),
    signal: slot!(
        (this: *mut c_void, fence_ptr: *mut c_void, value: u64) -> Hresult => {
            // SAFETY: `this` is a live `Queue` COM pointer.
            let queue = unsafe { &*this.cast::<Queue>() };
            // SAFETY: `fence_ptr` is a caller-supplied `ID3D12Fence` pointer.
            let Some(fence_obj) = (unsafe { fence::cast_fence(fence_ptr) }) else {
                return E_INVALIDARG;
            };
            queue.signal(fence_obj, value)
        }
    ),
    wait: slot!(
        (this: *mut c_void, fence_ptr: *mut c_void, value: u64) -> Hresult => {
            // SAFETY: `this` is a live `Queue` COM pointer.
            let queue = unsafe { &*this.cast::<Queue>() };
            // SAFETY: `fence_ptr` is a caller-supplied `ID3D12Fence` pointer.
            let Some(fence_obj) = (unsafe { fence::cast_fence(fence_ptr) }) else {
                return E_INVALIDARG;
            };
            queue.wait(fence_obj, value)
        }
    ),
    get_timestamp_frequency: slot!(
        "queue.GetTimestampFrequency",
        (this: *mut c_void, frequency: *mut u64) -> Hresult => {
            if frequency.is_null() {
                return E_INVALIDARG;
            }
            // SAFETY: `this` is a live `Queue` COM pointer.
            let queue = unsafe { &*this.cast::<Queue>() };
            if queue.device.is_null() {
                return E_INVALIDARG;
            }
            // SAFETY: `queue.device` is live for the lifetime of `queue`.
            let freq = timestamp_frequency(unsafe { &*queue.device });
            // SAFETY: `frequency` is non-null and writable.
            unsafe { frequency.write_unaligned(freq) };
            S_OK
        }
    ),
    get_clock_calibration: slot!(
        "queue.GetClockCalibration",
        (this: *mut c_void, gpu_timestamp: *mut u64, cpu_timestamp: *mut u64) -> Hresult => {
            // SAFETY: `this` is a live `Queue` COM pointer.
            let queue = unsafe { &*this.cast::<Queue>() };
            let mtl_dev = queue.mtl_device_handle();
            let mut cpu = 0u64;
            let mut gpu = 0u64;
            let (before, _) = qpc::now();
            if mtl_dev != 0 {
                // SAFETY: `mtl_dev` is a live `MTLDevice` and `cpu`/`gpu` are stack locals.
                unsafe { MTLDevice_sampleTimestamps(mtl_dev, &raw mut cpu, &raw mut gpu) };
            }
            let (after, _) = qpc::now();
            if !gpu_timestamp.is_null() {
                // SAFETY: `gpu_timestamp` is non-null and writable.
                unsafe { gpu_timestamp.write_unaligned(gpu) };
            }
            if !cpu_timestamp.is_null() {
                let mid = (before + (after - before) / 2).cast_unsigned();
                // SAFETY: `cpu_timestamp` is non-null and writable.
                unsafe { cpu_timestamp.write_unaligned(mid) };
            }
            S_OK
        }
    ),
    get_desc: slot!(
        "queue.GetDesc",
        (this: *mut c_void, ret: *mut c_void) -> *mut c_void => {
            if !ret.is_null() {
                // SAFETY: `this` is a live `Queue` COM pointer.
                let queue = unsafe { &*this.cast::<Queue>() };
                // SAFETY: `ret` points to a caller-supplied `CommandQueueDesc` return slot.
                unsafe { ret.cast::<CommandQueueDesc>().write_unaligned(queue.desc.clone()) };
            }
            ret
        }
    ),
};

/// Companion `IMTLSwapChainFactory` vtable for [`Queue`], seated at [`Queue::FACTORY_OFFSET`].
pub static SWAPCHAIN_FACTORY_VTBL: IMTLSwapChainFactoryVtbl = IMTLSwapChainFactoryVtbl {
    unknown: IUnknownVtbl {
        query_interface: slot!(
            (this: *mut c_void, riid: *const c_void, object: *mut *mut c_void) -> Hresult => {
                // SAFETY: `this` is at `Queue::FACTORY_OFFSET` inside a live `Queue`.
                let base = unsafe { com::from_companion::<Queue>(this, Queue::FACTORY_OFFSET) };
                // SAFETY: `base` is a live `Queue`.
                unsafe { query_interface(base.cast::<c_void>(), riid, object) }
            }
        ),
        add_ref: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is at `Queue::FACTORY_OFFSET` inside a live `Queue`.
            unsafe { com::companion_add_ref::<Queue, { Queue::FACTORY_OFFSET }>(this) }
        }),
        release: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is at `Queue::FACTORY_OFFSET` inside a live `Queue`.
            unsafe { com::companion_release::<Queue, { Queue::FACTORY_OFFSET }>(this) }
        }),
    },
    create_swap_chain: slot!(
        "queue.CreateSwapChain",
        (
            this: *mut c_void,
            factory: *mut c_void,
            hwnd: Hwnd,
            desc: *const SwapChainDesc1,
            fullscreen_desc: *const SwapChainFullscreenDesc,
            out: *mut *mut c_void
        ) -> Hresult => {
            // SAFETY: `this` is at `Queue::FACTORY_OFFSET` inside a live `Queue`.
            let queue_ptr = unsafe { com::from_companion::<Queue>(this, Queue::FACTORY_OFFSET) };
            // SAFETY: `queue_ptr` is a live `Queue`.
            let queue = unsafe { &*queue_ptr };
            if queue.device.is_null() {
                return E_INVALIDARG;
            }
            // SAFETY: `queue.device` is live for the lifetime of `queue`.
            let device = unsafe { &*queue.device };
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                swapchain::create(
                    factory,
                    device,
                    queue,
                    hwnd,
                    desc.cast::<c_void>(),
                    fullscreen_desc.cast::<c_void>(),
                    out,
                )
            }
        }
    ),
};

/// `ID3D12Device::CreateCommandQueue` (`d3d12_device.cpp:477`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `queue` are caller-supplied COM pointers.
pub unsafe fn create_command_queue(
    this: *mut c_void,
    desc: *const c_void,
    riid: *const c_void,
    queue: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandQueue");
    // SAFETY: `queue` is null or a writable out-pointer.
    unsafe { com::init_out_ptr(queue) };
    if this.is_null() || desc.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    let dev_ptr = this.cast::<Device>();
    // SAFETY: `dev_ptr` is a live `Device` pointer supplied by the COM slot.
    let dev = unsafe { &*dev_ptr };
    // SAFETY: `desc` is non-null and points to a `CommandQueueDesc`.
    let raw_desc = unsafe { desc.cast::<CommandQueueDesc>().read_unaligned() };
    let normalized_desc = CommandQueueDesc {
        node_mask: 1,
        ..raw_desc
    };

    let env_slots = std::env::var("DXMT_D3D12_CMDBUF_SLOTS").ok();
    let cmdbuf_slots = slots_from_env(env_slots.as_deref());

    let Some(mtl_dev) = dev.mtl.as_ref() else {
        return E_FAIL;
    };
    // SAFETY: `mtl_dev` is a live `MTLDevice`.
    let raw_mtl_queue =
        unsafe { MTLDevice_newCommandQueue(mtl_dev.handle(), u64::from(cmdbuf_slots)) };
    // SAFETY: `newCommandQueue` returns a +1 `MTLCommandQueue` handle or 0.
    let Some(mtl_queue) = (unsafe { Obj::adopt(raw_mtl_queue) }) else {
        return E_FAIL;
    };
    if let Some(res_set) = dev.residency.set.as_ref() {
        // SAFETY: `mtl_queue` and `res_set` are live Metal objects.
        unsafe { MTLCommandQueue_addResidencySet(mtl_queue.handle(), res_set.handle()) };
    }

    let mut fences: [Option<Obj>; FENCE_COUNT] = core::array::from_fn(|_| None);
    let mut fence_handles = [0u64; FENCE_COUNT];
    for i in 0..FENCE_COUNT {
        // SAFETY: `mtl_dev` is a live `MTLDevice`.
        let raw_fence = unsafe { MTLDevice_newFence(mtl_dev.handle()) };
        // SAFETY: `newFence` returns a +1 `MTLFence` handle or 0.
        let fence_obj = unsafe { Obj::adopt(raw_fence) };
        fence_handles[i] = fence_obj.as_ref().map_or(0, Obj::handle);
        fences[i] = fence_obj;
    }

    let overlap_env = std::env::var("DXMT_D3D12_OVERLAP").ok();
    let serial_env = std::env::var("DXMT_D3D12_SERIAL").ok();
    let serial = serial_from_env(overlap_env.as_deref(), serial_env.as_deref());
    let mut commit_state = QueueCommitState::new(serial);
    commit_state.order.fence_handles = fence_handles;

    let pool = InflightPool::new(dev_ptr, cmdbuf_slots);
    let pool_for_thread = Arc::clone(&pool);
    let wait_thread = com::spawn_thread("dxmt-cmdbuf-waiting-thread", move || {
        pool_for_thread.waiting_thread_loop();
    });

    // SAFETY: `dev_ptr` is a live `Device`; `Queue` holds one public reference while alive.
    unsafe { com::add_ref_com(dev_ptr) };

    let boxed = Box::new(Queue {
        vtbl: &QUEUE_VTBL,
        swapchain_factory_vtbl: &SWAPCHAIN_FACTORY_VTBL,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device: dev_ptr,
        mtl_queue: Some(mtl_queue),
        frame_gen: Mutex::new(FrameGen::new()),
        desc: normalized_desc,
        holds_device_ref: true,
        fences,
        pool,
        commit: Mutex::new(commit_state),
        wait_thread: Some(wait_thread),
        queue_index: dump::next_queue_index(),
    });
    let raw_queue = Box::into_raw(boxed);
    // SAFETY: `raw_queue` is a freshly allocated `Queue` and `riid`/`queue` are caller pointers.
    let hr = unsafe { query_interface(raw_queue.cast::<c_void>(), riid, queue) };
    if hr < 0 {
        // SAFETY: `query_interface` did not hand out a reference, so we reclaim `raw_queue`.
        unsafe { drop(Box::from_raw(raw_queue)) };
    }
    hr
}

/// `ID3D12Device9::CreateCommandQueue1` (`d3d12_device.cpp:1833`).
///
/// # Safety
///
/// `this`, `desc`, `creator_id`, `riid`, and `queue` are caller-supplied COM pointers.
pub unsafe fn create_command_queue1(
    this: *mut c_void,
    desc: *const c_void,
    _creator_id: *const c_void,
    riid: *const c_void,
    queue: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandQueue1");
    // SAFETY: forwarded from caller.
    unsafe { create_command_queue(this, desc, riid, queue) }
}

#[cfg(test)]
mod tests;
