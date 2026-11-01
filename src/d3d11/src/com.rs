//! COM reference-counting and interface helpers for the D3D11 front end (`crate::com`).
//!
//! Re-exports [`dxmt_util::com`]'s `Guid`, `Hresult`, `ComObject` and `IUnknown` helpers, and
//! provides [`Managed`] (`ManagedDeviceChild` of `d3d11_device_child.hpp`), [`TearOff`],
//! [`SendPtr`], [`container_of!`] and [`iface_of`].

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering},
};

pub use dxmt_util::com::{
    ComObject, ComObjectWithInitialRef, DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_FOUND, E_FAIL,
    E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_OUTOFMEMORY, E_POINTER, Guid, Hresult, IID_IUNKNOWN,
    IUnknownVtbl, Object, S_FALSE, S_OK, add_ref, qi_log, query, release, vtbl, write_out,
};

/// Recovers a pointer to outer `#[repr(C)]` `$Container` from a pointer `$ptr` to `$field`.
#[macro_export]
macro_rules! container_of {
    ($ptr:expr, $Container:ty, $field:ident) => {
        $ptr.wrapping_byte_sub(core::mem::offset_of!($Container, $field))
            .cast::<$Container>()
    };
}
pub use container_of;

/// Returns a raw COM interface pointer to a vtable field inside `&self`.
#[must_use]
pub const fn iface_of<V>(vtbl_field: &*const V) -> *mut c_void {
    core::ptr::from_ref(vtbl_field).cast_mut().cast()
}

/// [`Send`] + [`Sync`] raw pointer wrapper for COM objects or mapped GPU memory in closures.
#[repr(transparent)]
pub struct SendPtr<T>(pub *mut T);

impl<T> Clone for SendPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SendPtr<T> {}

// SAFETY: `SendPtr` wraps pointers to thread-safe COM objects or GPU-mapped slices whose
// lifetime is guaranteed by the enclosing command chunk or device.
unsafe impl<T> Send for SendPtr<T> {}
// SAFETY: concurrent access is synchronized by the device lock or chunk ownership rules.
unsafe impl<T> Sync for SendPtr<T> {}

/// Reference counter for `ManagedDeviceChild` (`d3d11_device_child.hpp:93-136`).
///
/// Both public and private counts start at 0, and 0 is a valid state (cached state objects and
/// pooled command lists live at 0 references). The 0 to 1 public transition takes one public
/// reference on the parent device and one private reference on `self`; the 1 to 0 public
/// transition releases both. Reaching 0 private references signals the caller to `Recycle` the
/// object without freeing its memory.
#[repr(C)]
pub struct Managed {
    pub pub_count: AtomicU32,
    pub priv_count: AtomicU32,
}

impl Default for Managed {
    fn default() -> Self {
        Self::new()
    }
}

impl Managed {
    /// A counter starting at 0 public and 0 private references.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pub_count: AtomicU32::new(0),
            priv_count: AtomicU32::new(0),
        }
    }

    /// `AddRef`: increments the public count, taking a device and private ref on 0 to 1.
    ///
    /// # Safety
    ///
    /// `device` is a live COM object or null.
    pub unsafe fn add_ref(&self, device: *mut c_void) -> u32 {
        let prev = self.pub_count.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            // SAFETY: the caller guarantees `device` is a live COM object or null.
            unsafe { add_ref(device) };
            self.add_ref_private();
        }
        prev + 1
    }

    /// `Release`: decrements the public count, releasing the device and private ref on 1 to 0.
    ///
    /// # Safety
    ///
    /// `device` is a live COM object or null.
    pub unsafe fn release(&self, device: *mut c_void) -> (u32, bool) {
        let remaining = self.pub_count.fetch_sub(1, Ordering::SeqCst) - 1;
        let recycle = if remaining == 0 {
            let rec = self.release_private();
            // SAFETY: the caller guarantees `device` is a live COM object or null.
            unsafe { release(device) };
            rec
        } else {
            false
        };
        (remaining, recycle)
    }

    /// `AddRefPrivate`: increments the private count.
    pub fn add_ref_private(&self) {
        self.priv_count.fetch_add(1, Ordering::SeqCst);
    }

    /// `ReleasePrivate`: decrements the private count, returning `true` at 1 to 0 (`Recycle`).
    #[must_use]
    pub fn release_private(&self) -> bool {
        self.priv_count.fetch_sub(1, Ordering::SeqCst) == 1
    }

    /// The live public reference count.
    #[must_use]
    pub fn public_count(&self) -> u32 {
        self.pub_count.load(Ordering::SeqCst)
    }

    /// The live private reference count.
    #[must_use]
    pub fn private_count(&self) -> u32 {
        self.priv_count.load(Ordering::SeqCst)
    }
}

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

#[cfg(test)]
mod tests;
