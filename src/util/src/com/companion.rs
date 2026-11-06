//! Interfaces a COM object answers from inside itself: companions and tear-offs.
//!
//! A companion is a second vtable field of the object, at a fixed offset, whose `this` is moved
//! back to the object ([`from_companion`], [`container_of!`](crate::container_of)) and which counts
//! on the object's own counter. A [`TearOff`] is a small embedded sub-object that remembers its
//! outer object and forwards `IUnknown` to it, so its references are the outer object's.

use core::ffi::c_void;

use super::{
    Guid, Hresult, IUnknownVtbl,
    hold::{ComRefCounted, add_ref_com, release_com},
    vtbl,
};

/// A tear-off COM sub-object embedded by value inside `outer` (`P6`).
///
/// Its first field is `vtbl`, so a pointer to `TearOff<V>` is a valid COM interface pointer; its
/// [`Self::query_interface`], [`Self::add_ref`] and [`Self::release`] thunks forward `IUnknown`
/// calls directly to `outer`.
#[repr(C)]
pub struct TearOff<V> {
    pub vtbl: *const V,
    pub outer: *mut c_void,
}

impl<V> TearOff<V> {
    /// Creates a tear-off with `vtbl` forwarding `IUnknown` to `outer`.
    #[must_use]
    pub const fn new(vtbl: &'static V, outer: *mut c_void) -> Self {
        Self { vtbl, outer }
    }

    /// Raw COM interface pointer for this tear-off.
    #[must_use]
    pub const fn as_iface(&self) -> *mut c_void {
        core::ptr::from_ref(self).cast_mut().cast()
    }
}

impl TearOff<()> {
    /// Forwards `IUnknown::QueryInterface` to the tear-off's `outer` container.
    ///
    /// # Safety
    ///
    /// `this` is a valid pointer to a [`TearOff<V>`] whose `outer` is a live COM object.
    pub unsafe extern "system-unwind" fn query_interface(
        this: *mut c_void,
        riid: *const c_void,
        out: *mut *mut c_void,
    ) -> Hresult {
        // SAFETY: `this` points to a live `TearOff<V>`.
        let outer = unsafe { (*this.cast::<Self>()).outer };
        // SAFETY: `outer` is a live COM object whose vtable starts with `IUnknownVtbl`.
        let unk = unsafe { vtbl::<IUnknownVtbl>(outer) };
        // SAFETY: forwards the caller's `riid` and `out` to `outer`.
        unsafe { (unk.query_interface)(outer, riid.cast::<Guid>(), out) }
    }

    /// Forwards `IUnknown::AddRef` to the tear-off's `outer` container.
    ///
    /// # Safety
    ///
    /// `this` is a valid pointer to a [`TearOff<V>`] whose `outer` is a live COM object.
    pub unsafe extern "system-unwind" fn add_ref(this: *mut c_void) -> u32 {
        // SAFETY: `this` points to a live `TearOff<V>`.
        let outer = unsafe { (*this.cast::<Self>()).outer };
        // SAFETY: `outer` is a live COM object whose vtable starts with `IUnknownVtbl`.
        let unk = unsafe { vtbl::<IUnknownVtbl>(outer) };
        // SAFETY: `outer` is live.
        unsafe { (unk.add_ref)(outer) }
    }

    /// Forwards `IUnknown::Release` to the tear-off's `outer` container.
    ///
    /// # Safety
    ///
    /// `this` is a valid pointer to a [`TearOff<V>`] whose `outer` is a live COM object.
    pub unsafe extern "system-unwind" fn release(this: *mut c_void) -> u32 {
        // SAFETY: `this` points to a live `TearOff<V>`.
        let outer = unsafe { (*this.cast::<Self>()).outer };
        // SAFETY: `outer` is a live COM object whose vtable starts with `IUnknownVtbl`.
        let unk = unsafe { vtbl::<IUnknownVtbl>(outer) };
        // SAFETY: `outer` is live.
        unsafe { (unk.release)(outer) }
    }
}

/// Recovers a pointer to outer `#[repr(C)]` `$Container` from a pointer `$ptr` to `$field`.
#[macro_export]
macro_rules! container_of {
    ($ptr:expr, $Container:ty, $field:ident) => {
        $ptr.wrapping_byte_sub(::core::mem::offset_of!($Container, $field))
            .cast::<$Container>()
    };
}

/// Returns a raw COM interface pointer to a vtable field inside `&self`.
#[must_use]
pub const fn iface_of<V>(vtbl_field: &*const V) -> *mut c_void {
    core::ptr::from_ref(vtbl_field).cast_mut().cast()
}

/// Adjusts a primary COM object pointer `this` forward by `ext_offset` bytes.
///
/// # Safety
///
/// `this` must be a non-null pointer to `T`, and `ext_offset` must be `offset_of!(T, <ext_vtbl>)`.
#[must_use]
pub const unsafe fn to_companion<T>(this: *mut T, ext_offset: usize) -> *mut c_void {
    // SAFETY: `ext_offset` is within the same allocation `T`.
    unsafe { this.cast::<c_void>().byte_add(ext_offset) }
}

/// Adjusts a companion interface pointer `ext_this` backward by `ext_offset` bytes.
///
/// # Safety
///
/// `ext_this` must point to the companion vtable field at byte offset `ext_offset` inside a live
/// `T`.
#[must_use]
pub const unsafe fn from_companion<T>(ext_this: *mut c_void, ext_offset: usize) -> *mut T {
    // SAFETY: `ext_this` was produced by adding `ext_offset` to a `*mut T` within the same
    // allocation.
    unsafe { ext_this.byte_sub(ext_offset).cast::<T>() }
}

/// Increments the public reference count of `this` through a companion pointer at `OFFSET`.
///
/// # Safety
///
/// `this` must point to the companion vtable field at byte offset `OFFSET` inside a live `Box<T>`.
pub unsafe fn companion_add_ref<T: ComRefCounted, const OFFSET: usize>(this: *mut c_void) -> u32 {
    // SAFETY: `this` is at byte offset `OFFSET` inside a live `T`.
    let base = unsafe { from_companion::<T>(this, OFFSET) };
    // SAFETY: `base` points to a live `Box<T>`.
    unsafe { add_ref_com(base) }
}

/// Decrements the public reference count of `this` through a companion pointer at `OFFSET`.
///
/// # Safety
///
/// `this` must point to the companion vtable field at byte offset `OFFSET` inside a live `Box<T>`
/// whose caller owns one public reference.
pub unsafe fn companion_release<T: ComRefCounted, const OFFSET: usize>(this: *mut c_void) -> u32 {
    // SAFETY: `this` is at byte offset `OFFSET` inside a live `T`.
    let base = unsafe { from_companion::<T>(this, OFFSET) };
    // SAFETY: `base` points to a live `Box<T>`.
    unsafe { release_com(base) }
}

#[cfg(test)]
mod tests;
