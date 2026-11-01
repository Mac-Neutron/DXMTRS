//! CPU/GPU synchronization fences and the shared-event listener (`dxmt_fence.*`).
//!
//! A [`Fence`] pairs a `MTLSharedEvent` (CPU-visible and shareable across queues) with a
//! `MTLEvent` (GPU-only). Because `MTLSharedEvent` values only increase while a D3D12 fence can be
//! reset to a lower value, [`Fence::ask`] opens a new [`Generation`] whenever a signal requests a
//! value below the current shared event's signaled value. [`EventListener`] runs winemetal's
//! listener thread and registers Win32 events for single- and multi-fence completion.

use core::{
    ffi::c_void,
    sync::atomic::{Ordering, fence},
};
use std::{
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    thread::{self, JoinHandle},
    time::Duration,
};

#[cfg(not(test))]
use winemetal_sys::Pool;
use winemetal_sys::{
    MTLCommandBuffer_commit, MTLCommandBuffer_encodeSignalEvent, MTLCommandQueue_commandBuffer,
    MTLDevice_newEvent, MTLDevice_newSharedEvent, MTLSharedEvent_setWin32EventAtValue,
    MTLSharedEvent_setWin32EventAtValues, MTLSharedEvent_signalValue, MTLSharedEvent_signaledValue,
    MTLSharedEvent_waitUntilSignaledValue, Obj, SharedEventListener_create,
    SharedEventListener_destroy, SharedEventListener_start,
};

/// A Win32 `HANDLE` passed to winemetal's shared-event notifications.
pub type Handle = *mut c_void;

/// The listener thread's 15-character name.
const LISTENER_THREAD: &str = "dxmt-event-list";

/// Stack reservation of the listener thread, matching `dxmt::thread`.
const STACK: usize = 0x10_0000;

fn retain(object: &Obj) -> Obj {
    // SAFETY: `object` is a live Objective-C object.
    unsafe { Obj::retain(object.handle()) }.expect("a live object's handle is non-zero")
}

/// One generation of a [`Fence`]: its `MTLSharedEvent` and `MTLEvent` (`DXMTFence::Generation`).
pub struct Generation {
    pub shared: Obj,
    pub gpu: Obj,
}

impl Generation {
    fn new(device: &Obj) -> Self {
        // SAFETY: `device` is a live `MTLDevice`.
        let shared = unsafe { MTLDevice_newSharedEvent(device.handle()) };
        // SAFETY: `newSharedEvent` returns a +1 object handle.
        let shared = unsafe { Obj::adopt(shared) }.expect("MTLDevice_newSharedEvent returned nil");
        // SAFETY: `device` is a live `MTLDevice`.
        let gpu = unsafe { MTLDevice_newEvent(device.handle()) };
        // SAFETY: `newEvent` returns a +1 object handle.
        let gpu = unsafe { Obj::adopt(gpu) }.expect("MTLDevice_newEvent returned nil");
        Self { shared, gpu }
    }
}

impl Clone for Generation {
    fn clone(&self) -> Self {
        Self {
            shared: retain(&self.shared),
            gpu: retain(&self.gpu),
        }
    }
}

/// A CPU/GPU fence (`DXMTFence`).
pub struct Fence {
    device: Obj,
    helper_queue: Obj,
    generation: Mutex<Generation>,
}

impl Fence {
    /// Creates a fence on `device` whose CPU signals forward to GPU events via `helper_queue`.
    #[must_use]
    pub fn new(device: &Obj, helper_queue: &Obj) -> Arc<Self> {
        let generation = Generation::new(device);
        Arc::new(Self {
            device: retain(device),
            helper_queue: retain(helper_queue),
            generation: Mutex::new(generation),
        })
    }

    /// Returns the fence's current `(shared, gpu)` event generation.
    #[must_use]
    pub fn current(&self) -> Generation {
        self.lock().clone()
    }

    /// Returns a generation suitable for signaling `value`.
    ///
    /// Starts a new generation when `value` is strictly below the current shared event's signaled
    /// value, since `MTLSharedEvent` values are monotonic while D3D12 fences can be reset lower.
    #[must_use]
    pub fn ask(&self, value: u64) -> Generation {
        let mut generation = self.lock();
        // SAFETY: `generation.shared` is a live `MTLSharedEvent`.
        let signaled = unsafe { MTLSharedEvent_signaledValue(generation.shared.handle()) };
        if value < signaled {
            *generation = Generation::new(&self.device);
        }
        generation.clone()
    }

    /// Returns the current generation's `MTLSharedEvent`.
    #[must_use]
    pub fn shared_event(&self) -> Obj {
        retain(&self.lock().shared)
    }

    /// Returns the current generation's signaled value.
    #[must_use]
    pub fn completed_value(&self) -> u64 {
        let shared = self.shared_event();
        // SAFETY: `shared` is a live `MTLSharedEvent`.
        unsafe { MTLSharedEvent_signaledValue(shared.handle()) }
    }

    /// Signals `value` on the CPU and, when `value` is non-zero, forwards it to the GPU event.
    pub fn signal(&self, value: u64) {
        let generation = self.ask(value);
        // SAFETY: `generation.shared` is a live `MTLSharedEvent`.
        unsafe { MTLSharedEvent_signalValue(generation.shared.handle(), value) };
        if value != 0 {
            self.forward(&generation, value);
        }
    }

    /// Encodes a signal of `value` on `generation.gpu` into a command buffer on the helper queue.
    pub fn forward(&self, generation: &Generation, value: u64) {
        // In unit tests `presenter::tests` owns the `NSAutoreleasePool_alloc_init` symbol.
        #[cfg(not(test))]
        let _pool = Pool::new();
        // SAFETY: `self.helper_queue` is a live `MTLCommandQueue`.
        let cmdbuf = unsafe { MTLCommandQueue_commandBuffer(self.helper_queue.handle()) };
        // SAFETY: `cmdbuf` is the command buffer just obtained and `generation.gpu` is a live
        // `MTLEvent`.
        unsafe { MTLCommandBuffer_encodeSignalEvent(cmdbuf, generation.gpu.handle(), value) };
        // SAFETY: `cmdbuf` is the command buffer just encoded into.
        unsafe { MTLCommandBuffer_commit(cmdbuf) };
    }

    /// Blocks until the current shared event reaches `value` or `timeout` milliseconds elapse.
    #[must_use]
    pub fn wait(&self, value: u64, timeout: u64) -> bool {
        let shared = self.shared_event();
        // SAFETY: `shared` is a live `MTLSharedEvent`.
        unsafe { MTLSharedEvent_waitUntilSignaledValue(shared.handle(), value, timeout) }
    }

    fn lock(&self) -> MutexGuard<'_, Generation> {
        self.generation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Winemetal's shared-event listener and its dispatch thread (`EventListener` in `dxmt_fence.hpp`).
pub struct EventListener {
    listener: u64,
    thread: Option<JoinHandle<()>>,
}

impl EventListener {
    /// Creates a winemetal shared-event listener and starts its dispatch thread.
    ///
    /// # Panics
    ///
    /// When the listener thread cannot be spawned.
    #[must_use]
    pub fn new() -> Self {
        // SAFETY: no precondition; `drop` destroys the listener.
        let listener = unsafe { SharedEventListener_create() };
        let thread = thread::Builder::new()
            .name(LISTENER_THREAD.to_owned())
            .stack_size(STACK)
            .spawn(move || {
                let _guard = crate::ffi::AbortOnPanic;
                // SAFETY: `listener` was created above and `SharedEventListener_start` returns once
                // `drop` calls `SharedEventListener_destroy(listener)`.
                unsafe { SharedEventListener_start(listener) };
            })
            .expect("failed to spawn dxmt-event-list thread");
        Self {
            listener,
            thread: Some(thread),
        }
    }

    /// The underlying winemetal `SharedEventListener` handle.
    #[must_use]
    pub const fn handle(&self) -> u64 {
        self.listener
    }

    /// Registers `event` to be signaled when `fence` reaches `value` (winemetal slot 104).
    pub fn set_event_on_value(&self, fence: &Fence, event: Handle, value: u64) {
        let shared = fence.shared_event();
        notify_value(shared.handle(), self.listener, event, value);
        drop(shared);
    }

    /// Registers `event` to be signaled when `fences` reach `values` (winemetal slot 146).
    ///
    /// When `all` is true every fence must reach its value (`needed = count`); when false any one
    /// fence suffices (`needed = 1`). Each fence's shared event is held alive until the winemetal
    /// call returns.
    ///
    /// # Panics
    ///
    /// When `fences.len() != values.len()` or `fences.len()` exceeds `u32::MAX`.
    pub fn set_event_on_values(&self, fences: &[&Fence], values: &[u64], all: bool, event: Handle) {
        assert_eq!(fences.len(), values.len(), "each fence has a target value");
        let count = u32::try_from(fences.len()).expect("fence count fits in u32");
        if count == 0 {
            return;
        }
        let shared: Vec<Obj> = fences.iter().map(|fence| fence.shared_event()).collect();
        let handles: Vec<u64> = shared.iter().map(Obj::handle).collect();
        let needed = if all { count } else { 1 };
        notify_values(self.listener, &handles, values, count, needed, event);
        drop(shared);
    }
}

fn notify_value(shared: u64, listener: u64, event: Handle, value: u64) {
    // SAFETY: `shared` is kept alive by the caller through the call and `listener` is live;
    // `event` is an opaque Win32 event handle forwarded by value across winemetal's bridge.
    unsafe {
        MTLSharedEvent_setWin32EventAtValue(shared, listener, event, value);
    }
}

fn notify_values(
    listener: u64,
    handles: &[u64],
    values: &[u64],
    count: u32,
    needed: u32,
    event: Handle,
) {
    // SAFETY: `listener` is live, `handles` and `values` hold `count` elements, and the caller
    // keeps every `MTLSharedEvent` alive across the call; `event` is an opaque Win32 handle.
    unsafe {
        MTLSharedEvent_setWin32EventAtValues(
            listener,
            handles.as_ptr(),
            values.as_ptr(),
            count,
            needed,
            event,
        );
    }
}

impl Default for EventListener {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for EventListener {
    fn drop(&mut self) {
        thread::sleep(Duration::from_millis(10));
        // SAFETY: `self.listener` was created in `new` and is destroyed once here, waking
        // `SharedEventListener_start`.
        unsafe { SharedEventListener_destroy(self.listener) };
        if let Some(thread) = self.thread.take() {
            wait_until_finished(thread);
        }
    }
}

/// Waits for `handle`'s thread to end by polling, then drops the handle.
///
/// Wine can invalidate a thread handle held across a long session, so PE crates poll
/// `is_finished` rather than blocking on the OS thread handle.
fn wait_until_finished(handle: JoinHandle<()>) {
    while !handle.is_finished() {
        thread::sleep(Duration::from_millis(1));
    }
    fence(Ordering::Acquire);
    drop(handle);
}

#[cfg(test)]
mod tests;
