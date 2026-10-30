//! The shared-event listener: the run loop on which a fence's Win32 events are set.
//!
//! Metal calls a notification block on its own queue; the block never sets the event there. It
//! queues the set on the listener's run loop, which slot 109 runs on its caller's thread, so
//! `NtSetEvent` runs on the thread Wine (and the VM host's `events.m`) expects.
//!
//! The listener's state, not its first word, decides what a notification does, so neither a
//! destroy before the start nor a notification after the destroy touches freed memory:
//! - unstarted: the notification waits for the start, as the C's did;
//! - running: it queues its set on the loop and wakes it;
//! - destroyed: it drops its hold on the event and never sets it.
//!
//! A destroy of a running listener queues the loop's stop behind every set queued before it, so
//! none is lost, and a stop queued before the loop runs still stops it. A count of owners frees
//! the listener after the last: its handle, its one start (counted from the create) and each
//! notification Metal holds.

use core::hint::spin_loop;
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_core_foundation::{
    CFRetained, CFRunLoop, CFRunLoopMode, CFRunLoopSource, CFRunLoopSourceContext, CFType,
    kCFRunLoopCommonModes,
};
use objc2_metal::MTLSharedEventListener;

use crate::ffi::AbortOnPanic;
use crate::float_cast::u64_to_usize;

/// A listener; its handle is its address.
#[repr(C)]
pub struct Listener {
    /// Null, or the run loop the winning start runs: the C's first word.
    ///
    /// The VM host's `events.m` polls it for non-null before it destroys a listener it started.
    /// The loop it names is the one `state` keeps a reference to until the listener is freed.
    first_word: AtomicPtr<CFRunLoop>,
    state: Mutex<State>,
    events: Retained<MTLSharedEventListener>,
}

impl Listener {
    /// Slot 108: a new listener, unstarted, owned by its handle and by its start.
    #[must_use]
    pub fn create() -> u64 {
        let listener = Box::new(Self {
            first_word: AtomicPtr::new(ptr::null_mut()),
            state: Mutex::new(State { phase: Phase::Unstarted, owners: 2, run_loop: None }),
            events: MTLSharedEventListener::init(MTLSharedEventListener::alloc()),
        });
        Box::into_raw(listener).expose_provenance() as u64
    }

    /// The listener a handle names, `None` for the null handle.
    #[must_use]
    pub const fn from_handle(h: u64) -> Option<NonNull<Self>> {
        NonNull::new(core::ptr::with_exposed_provenance_mut(u64_to_usize(h)))
    }

    /// The `MTLSharedEventListener` its notifications are registered with.
    #[must_use]
    pub fn events(&self) -> &MTLSharedEventListener {
        &self.events
    }

    /// Slot 109: runs the listener's loop on this thread until its destroy stops it.
    ///
    /// The first start publishes the loop and runs it, then gives back its reference. A start
    /// after the destroy gives it back and returns at once; any other start returns at once.
    ///
    /// # Safety
    ///
    /// `this` is a listener [`Self::create`] made, still owned by its handle or its start.
    pub unsafe fn start(this: NonNull<Self>) {
        // SAFETY: the caller's contract.
        let listener = unsafe { this.as_ref() };
        let mut state = listener.lock();
        match state.phase {
            Phase::Unstarted => {}
            Phase::DestroyedUnstarted => {
                state.phase = Phase::Destroyed;
                let last = state.give_back();
                drop(state);
                // SAFETY: the start's reference, given back once.
                unsafe { Self::free_if(this, last) };
                return;
            }
            Phase::Running | Phase::Destroyed => return,
        }
        let Some(run_loop) = CFRunLoop::current() else {
            return;
        };
        listener.first_word.store(CFRetained::as_ptr(&run_loop).as_ptr(), Ordering::Release);
        state.run_loop = Some(run_loop.clone());
        state.phase = Phase::Running;
        drop(state);
        // A source that never fires keeps the loop running.
        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: ptr::null_mut(),
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: None,
        };
        // SAFETY: a context with no callbacks, which CFRunLoopSourceCreate copies.
        let source = unsafe { CFRunLoopSource::new(None, 0, &raw mut context) };
        run_loop.add_source(source.as_deref(), common_modes());
        CFRunLoop::run();
        // SAFETY: the start's reference, given back once, after the loop it ran stopped.
        unsafe { Self::release(this) };
    }

    /// Slot 110: destroys the listener for its handle.
    ///
    /// A running listener's loop stops after every set queued before this.
    ///
    /// # Safety
    ///
    /// `this` is a listener [`Self::create`] made, still owned by its handle.
    pub unsafe fn destroy(this: NonNull<Self>) {
        // SAFETY: the caller's contract.
        let listener = unsafe { this.as_ref() };
        let mut state = listener.lock();
        match state.phase {
            Phase::Unstarted => state.phase = Phase::DestroyedUnstarted,
            Phase::Running => {
                state.phase = Phase::Destroyed;
                if let Some(run_loop) = &state.run_loop {
                    let stop = RcBlock::new(|| {
                        let _guard = AbortOnPanic;
                        if let Some(run_loop) = CFRunLoop::current() {
                            run_loop.stop();
                        }
                    });
                    perform(run_loop, &stop);
                }
            }
            Phase::DestroyedUnstarted | Phase::Destroyed => return,
        }
        let last = state.give_back();
        drop(state);
        // SAFETY: the handle's reference, given back once.
        unsafe { Self::free_if(this, last) };
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Gives back one reference to `this`; the last frees it.
    ///
    /// # Safety
    ///
    /// The caller owns one reference to `this` and gives it up.
    unsafe fn release(this: NonNull<Self>) {
        // SAFETY: the caller's reference keeps it alive until here.
        let last = unsafe { this.as_ref() }.lock().give_back();
        // SAFETY: as above.
        unsafe { Self::free_if(this, last) };
    }

    /// Frees `this` when `last`: the reference just given back was the last one.
    ///
    /// # Safety
    ///
    /// `last` was read under the lock, after the caller gave back its reference.
    unsafe fn free_if(this: NonNull<Self>, last: bool) {
        if last {
            // SAFETY: no owner is left: nothing else reaches the listener.
            drop(unsafe { Box::from_raw(this.as_ptr()) });
        }
    }
}

/// A notification's reference to its listener, given back when the notification's block goes.
pub struct Owner(NonNull<Listener>);

impl Owner {
    /// One more reference to `listener`, for a notification Metal will hold.
    ///
    /// # Safety
    ///
    /// `listener` is a listener [`Listener::create`] made, still owned by another reference.
    #[must_use]
    pub unsafe fn new(listener: NonNull<Listener>) -> Self {
        // SAFETY: the caller's contract.
        unsafe { listener.as_ref() }.lock().owners += 1;
        Self(listener)
    }

    /// What a notification does, on the thread Metal calls it on.
    ///
    /// Waits while the listener is unstarted (the C's wait). Running, it queues `set` on the
    /// listener's loop and wakes it; destroyed, it calls `destroyed` instead.
    pub fn notify(&self, set: impl Fn() + 'static, destroyed: impl FnOnce()) {
        // SAFETY: the owner's reference keeps the listener alive.
        let listener = unsafe { self.0.as_ref() };
        loop {
            let state = listener.lock();
            match state.phase {
                Phase::Unstarted => {
                    drop(state);
                    spin_loop();
                }
                Phase::Running => {
                    if let Some(run_loop) = &state.run_loop {
                        perform(run_loop, &RcBlock::new(set));
                    }
                    return;
                }
                Phase::DestroyedUnstarted | Phase::Destroyed => {
                    drop(state);
                    destroyed();
                    return;
                }
            }
        }
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: this owner's reference, given back once.
        unsafe { Listener::release(self.0) };
    }
}

/// The listener's state, behind its lock.
struct State {
    phase: Phase,
    /// The handle, the start, and each notification Metal holds.
    owners: u32,
    /// The winning start's loop, kept until the listener is freed.
    run_loop: Option<CFRetained<CFRunLoop>>,
}

impl State {
    /// Gives back one reference: whether it was the last.
    const fn give_back(&mut self) -> bool {
        self.owners -= 1;
        self.owners == 0
    }
}

enum Phase {
    /// No start has claimed the listener, nor has its destroy come.
    Unstarted,
    /// The start that claimed it runs its loop.
    Running,
    /// Destroyed before any start: the start's reference is still out.
    DestroyedUnstarted,
    /// Destroyed after its start claimed it.
    Destroyed,
}

/// Queues `block` on `run_loop` in the common modes and wakes the loop.
fn perform(run_loop: &CFRunLoop, block: &RcBlock<dyn Fn()>) {
    // SAFETY: the common modes and a live block, which the loop copies.
    unsafe { run_loop.perform_block(common_modes().map(|m| -> &CFType { m }), Some(&**block)) };
    run_loop.wake_up();
}

/// `kCFRunLoopCommonModes`.
fn common_modes() -> Option<&'static CFRunLoopMode> {
    // SAFETY: a constant CoreFoundation defines.
    unsafe { kCFRunLoopCommonModes }
}
