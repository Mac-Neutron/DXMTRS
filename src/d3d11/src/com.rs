//! COM reference-counting and interface helpers for the D3D11 front end (`crate::com`).
//!
//! Re-exports [`dxmt_util::com`]'s `Guid`, `Hresult`, `ComObject` and `IUnknown` helpers, and its
//! `TearOff`, `container_of!` and `iface_of`; provides [`Managed`] (`ManagedDeviceChild` of
//! `d3d11_device_child.hpp`) and [`SendPtr`].

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering},
};

pub use dxmt_util::{
    com::{
        ComObject, ComObjectWithInitialRef, DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_FOUND, E_FAIL,
        E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_OUTOFMEMORY, E_POINTER, Guid, Hresult,
        IID_IUNKNOWN, IUnknownVtbl, Object, S_FALSE, S_OK, add_ref,
        companion::{TearOff, iface_of},
        qi_log, query, release, vtbl, write_out,
    },
    container_of,
};

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

#[cfg(test)]
mod tests;
