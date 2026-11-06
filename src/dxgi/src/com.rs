//! The COM plumbing of dxgi's objects: the reference count, `QueryInterface` and out pointers.
//!
//! An object is `#[repr(C)]` with a `Head` first, boxed, and its address is the `this` of every
//! interface it answers. Calls on objects dxgi did not make (an application's, d3d11's) go through
//! `dxmt_util::com::{query, release, vtbl}` with a partial vtable.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering, fence},
};

pub use dxmt_util::com::write_out;

/// An object's first field: its vtable, then its reference count, from 1.
#[repr(C)]
pub struct Head<V: 'static> {
    vtbl: &'static V,
    refs: AtomicU32,
}

impl<V> Head<V> {
    /// A head with one reference, the creator's.
    pub const fn new(vtbl: &'static V) -> Self {
        Self {
            vtbl,
            refs: AtomicU32::new(1),
        }
    }

    /// The object's vtable.
    pub const fn vtbl(&self) -> &'static V {
        self.vtbl
    }
}

/// `AddRef`: the count after it.
///
/// # Safety
///
/// `this` is a live `T`, which starts with a `Head`.
pub unsafe fn add_ref<T>(this: *mut c_void) -> u32 {
    // SAFETY: the caller's.
    let refs = unsafe { refs::<T>(this) };
    refs.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
}

/// `Release`: the count after it; the last frees the object.
///
/// A `Release` past zero answers 0 and frees nothing.
///
/// # Safety
///
/// `this` is a live `T`, which starts with a `Head`, from `Box::into_raw`.
pub unsafe fn release<T>(this: *mut c_void) -> u32 {
    let before = {
        // SAFETY: the caller's.
        let refs = unsafe { refs::<T>(this) };
        refs.try_update(Ordering::Release, Ordering::Relaxed, |count| {
            count.checked_sub(1)
        })
    };
    match before {
        Ok(1) => {
            fence(Ordering::Acquire);
            // SAFETY: the last reference is gone, so nothing else holds `this`, which the caller
            // says came from `Box::into_raw`.
            drop(unsafe { Box::from_raw(this.cast::<T>()) });
            0
        }
        Ok(count) => count - 1,
        Err(_) => 0,
    }
}

/// The reference count of `this`, a `T` that starts with a `Head`.
///
/// A `Head`'s layout is the same for every vtable type (a reference, then the count), so it is read
/// as a `Head<()>`.
///
/// # Safety
///
/// `this` is a live `T`, which starts with a `Head`, and stays live while the count is used.
unsafe fn refs<'a, T>(this: *mut c_void) -> &'a AtomicU32 {
    let head = this.cast::<T>().cast::<Head<()>>();
    // SAFETY: the caller's: `head` addresses a live `Head`, whose count is an atomic.
    unsafe { &(*head).refs }
}

const _: () = assert!(
    core::mem::offset_of!(Head<()>, refs) == core::mem::offset_of!(Head<[usize; 30]>, refs)
);

#[cfg(test)]
mod tests;
