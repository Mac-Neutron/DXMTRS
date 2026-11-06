//! COM objects freed through their two-count [`ComObject`], and the holds that keep them alive.
//!
//! An object is a `Box<T>` whose `T` answers its counter; `AddRef` and `Release` go through
//! [`add_ref_com`] and [`release_com`], and the last private reference frees the box. [`ComHold`]
//! keeps a public reference, as `Com<T>` does, and [`PrivateHold`] a private one (`Com<T, false>`).

use core::{ops::Deref, ptr::NonNull};

use super::ComObject;

/// A heap-allocated COM implementation struct backed by [`ComObject`].
pub trait ComRefCounted: Sized {
    /// Returns the object's two-count reference counter.
    fn com(&self) -> &ComObject;
}

/// Increments the public reference count of `this` (`AddRef`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>`.
pub unsafe fn add_ref_com<T: ComRefCounted>(this: *mut T) -> u32 {
    // SAFETY: `this` is a live `T`.
    let obj = unsafe { &*this };
    obj.com().add_ref()
}

/// Decrements the public reference count of `this` (`Release`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>` whose caller owns one public reference.
pub unsafe fn release_com<T: ComRefCounted>(this: *mut T) -> u32 {
    // SAFETY: `this` is a live `T`.
    let (remaining, free) = unsafe { (*this).com().release_and_free() };
    if free {
        // SAFETY: the last private reference was just released and `DESTROYING_BIT` is set.
        unsafe { drop(Box::from_raw(this)) };
    }
    remaining
}

/// Increments the private reference count of `this` (`AddRefPrivate`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>`.
pub unsafe fn add_ref_private_com<T: ComRefCounted>(this: *mut T) {
    // SAFETY: `this` is a live `T`.
    let obj = unsafe { &*this };
    obj.com().add_ref_private();
}

/// Decrements the private reference count of `this` (`ReleasePrivate`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>` whose caller owns one private reference.
pub unsafe fn release_private_com<T: ComRefCounted>(this: *mut T) {
    // SAFETY: `this` is a live `T`.
    let free = unsafe { (*this).com().release_private() };
    if free {
        // SAFETY: the last private reference was just released and `DESTROYING_BIT` is set.
        unsafe { drop(Box::from_raw(this)) };
    }
}

/// Smart pointer holding a public reference to a [`ComRefCounted`] COM object (`Com<T>`).
pub struct ComHold<T: ComRefCounted> {
    ptr: NonNull<T>,
}

// SAFETY: `T` is a COM object with atomic reference counts; sharing or sending `ComHold<T>`
// requires `T: Send + Sync`.
unsafe impl<T: ComRefCounted + Send + Sync> Send for ComHold<T> {}
// SAFETY: see `Send` above.
unsafe impl<T: ComRefCounted + Send + Sync> Sync for ComHold<T> {}

impl<T: ComRefCounted> ComHold<T> {
    /// Acquires a new public reference to `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a live `Box<T>`.
    #[must_use]
    pub unsafe fn new(ptr: NonNull<T>) -> Self {
        // SAFETY: `ptr` points to a live `Box<T>`.
        unsafe { add_ref_com(ptr.as_ptr()) };
        Self { ptr }
    }

    /// Raw pointer to the held `T`.
    #[must_use]
    pub const fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T: ComRefCounted> Clone for ComHold<T> {
    fn clone(&self) -> Self {
        // SAFETY: `self.ptr` is kept alive by `self`.
        unsafe { Self::new(self.ptr) }
    }
}

impl<T: ComRefCounted> Deref for ComHold<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `self` holds a live public reference to `self.ptr`.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: ComRefCounted> Drop for ComHold<T> {
    fn drop(&mut self) {
        // SAFETY: `self` owns one public reference acquired in `new`.
        unsafe { release_com(self.ptr.as_ptr()) };
    }
}

/// Smart pointer holding a private reference to a [`ComRefCounted`] COM object (`Com<T, false>`).
///
/// Used by placed resources (`heap_`) and command lists (`allocator_`, `pso_*`, `rootsig_*`) so an
/// internal hold keeps the target allocation alive after its last public `Release`.
pub struct PrivateHold<T: ComRefCounted> {
    ptr: NonNull<T>,
}

// SAFETY: `T` is a COM object with atomic reference counts; sharing or sending `PrivateHold<T>`
// requires `T: Send + Sync`.
unsafe impl<T: ComRefCounted + Send + Sync> Send for PrivateHold<T> {}
// SAFETY: see `Send` above.
unsafe impl<T: ComRefCounted + Send + Sync> Sync for PrivateHold<T> {}

impl<T: ComRefCounted> PrivateHold<T> {
    /// Acquires a new private reference to `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a live `Box<T>`.
    #[must_use]
    pub unsafe fn new(ptr: NonNull<T>) -> Self {
        // SAFETY: `ptr` points to a live `Box<T>`.
        unsafe { add_ref_private_com(ptr.as_ptr()) };
        Self { ptr }
    }

    /// Raw pointer to the held `T`.
    #[must_use]
    pub const fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T: ComRefCounted> Clone for PrivateHold<T> {
    fn clone(&self) -> Self {
        // SAFETY: `self.ptr` is kept alive by `self`.
        unsafe { Self::new(self.ptr) }
    }
}

impl<T: ComRefCounted> Deref for PrivateHold<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `self` holds a live private reference to `self.ptr`.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: ComRefCounted> Drop for PrivateHold<T> {
    fn drop(&mut self) {
        // SAFETY: `self` owns one private reference acquired in `new`.
        unsafe { release_private_com(self.ptr.as_ptr()) };
    }
}

#[cfg(test)]
mod tests;
