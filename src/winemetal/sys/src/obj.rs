//! Owned Metal objects and autorelease pools over winemetal's handles.

use core::marker::PhantomData;

use crate::{NSAutoreleasePool_alloc_init, NSObject_release, NSObject_retain};

/// A Metal object this side holds one reference to: a non-null `obj_handle_t`.
///
/// Dropping it releases that reference. Metal's retain and release are thread-safe, so it is
/// `Send` and `Sync`.
pub struct Obj(u64);

impl Obj {
    /// Takes over the reference a +1 handle carries; `None` for the null handle.
    ///
    /// # Safety
    ///
    /// `handle` is null or a live object's handle carrying a reference the caller gives up here.
    #[must_use]
    pub const unsafe fn adopt(handle: u64) -> Option<Self> {
        if handle == 0 {
            None
        } else {
            Some(Self(handle))
        }
    }

    /// Takes a reference of its own to a borrowed handle; `None` for the null handle.
    ///
    /// # Safety
    ///
    /// `handle` is null or a live object's handle.
    #[must_use]
    pub unsafe fn retain(handle: u64) -> Option<Self> {
        if handle == 0 {
            return None;
        }
        // SAFETY: a live object (the caller's promise); the new reference is this `Obj`'s.
        unsafe { NSObject_retain(handle) };
        Some(Self(handle))
    }

    /// The handle, borrowed for as long as `self` lives.
    #[must_use]
    pub const fn handle(&self) -> u64 {
        self.0
    }
}

impl Drop for Obj {
    fn drop(&mut self) {
        // SAFETY: `self` holds one reference to a live object (`adopt`'s or `retain`'s), given up
        // here, once.
        unsafe { NSObject_release(self.0) };
    }
}

/// An autorelease pool on the thread that pushed it, drained when dropped.
///
/// A pool belongs to its thread: it is neither `Send` nor `Sync`.
pub struct Pool {
    handle: u64,
    thread: PhantomData<*const ()>,
}

impl Pool {
    /// Pushes a pool on the calling thread.
    #[must_use]
    pub fn new() -> Self {
        // SAFETY: no precondition; the pool is released by this guard's drop on the same thread
        // (the guard is not `Send`).
        let handle = unsafe { NSAutoreleasePool_alloc_init() };
        Self {
            handle,
            thread: PhantomData,
        }
    }
}

impl Default for Pool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        // SAFETY: the pool `new` pushed on this thread, released once, which drains it.
        unsafe { NSObject_release(self.handle) };
    }
}

#[cfg(test)]
mod tests;
