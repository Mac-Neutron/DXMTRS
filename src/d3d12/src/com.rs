//! COM base types, `DeviceChild` ownership, companion vtables, `slot!`, and D3D12 IIDs.

use core::{ffi::c_void, ptr, ptr::NonNull};

pub use dxmt_util::{
    com::{
        ComObject, ComObjectWithInitialRef, Guid, Hresult, Object, Qi, QiMiss, add_ref,
        companion::{companion_add_ref, companion_release, from_companion, to_companion},
        hold::{
            ComHold, ComRefCounted, PrivateHold, add_ref_com, add_ref_private_com, release_com,
            release_private_com,
        },
        hresult::*,
        iid::*,
        qi_log, qi_refuse, qi_table, query, release, vtbl, write_out,
    },
    private_data::{PrivateData, WKPDID_D3D_DEBUG_OBJECT_NAME_W, set_debug_name},
};

use crate::abi::vtbl::IUnknownVtbl;

/// Zeroes `*out` when `out` is non-null (`InitReturnPtr` in `com_pointer.hpp`).
///
/// # Safety
///
/// `out` must be null or point to a writable `*mut c_void`.
pub const unsafe fn init_out_ptr(out: *mut *mut c_void) {
    if !out.is_null() {
        // SAFETY: `out` is non-null and the caller guarantees it points to a writable slot.
        unsafe { out.write_unaligned(ptr::null_mut()) };
    }
}

/// Returns `true` when `ptr` is non-null and its first pointer-sized field equals `expected_vtbl`.
///
/// # Safety
///
/// `ptr` must be null or point to a readable vtable pointer.
#[must_use]
pub unsafe fn has_vtbl<V>(ptr: *const c_void, expected_vtbl: &'static V) -> bool {
    if ptr.is_null() {
        return false;
    }
    // SAFETY: `ptr` is non-null and the caller guarantees its vtable slot is readable.
    let vtbl_ptr = unsafe { ptr.cast::<*const V>().read_unaligned() };
    ptr::eq(vtbl_ptr, expected_vtbl)
}

/// Casts a non-null COM object pointer `ptr` to `&'a T` when its vtable matches `expected_vtbl`.
///
/// # Safety
///
/// `ptr` must be null or a valid COM object pointer, and any live object whose vtable pointer is
/// `expected_vtbl` must be a valid `T` outliving `'a`.
#[must_use]
pub unsafe fn cast_com<'a, T, V>(ptr: *const c_void, expected_vtbl: &'static V) -> Option<&'a T> {
    // SAFETY: forwarded from caller.
    if unsafe { has_vtbl(ptr, expected_vtbl) } {
        // SAFETY: only `T` instances carry `expected_vtbl` at offset 0.
        Some(unsafe { &*ptr.cast::<T>() })
    } else {
        None
    }
}

/// Common header for `MTLD3D12DeviceChild` classes (`d3d12_device_child.hpp`).
#[repr(C)]
pub struct DeviceChild<D: ComRefCounted> {
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: ComHold<D>,
}

impl<D: ComRefCounted> DeviceChild<D> {
    /// Creates a `DeviceChild` header holding a reference to `device` (`device_->AddRef()`).
    ///
    /// # Safety
    ///
    /// `device` must point to a live `Box<D>`.
    #[must_use]
    pub unsafe fn new(device: NonNull<D>) -> Self {
        // SAFETY: `device` is a live `Box<D>`.
        let device = unsafe { ComHold::new(device) };
        Self {
            com: ComObject::new(),
            private_data: PrivateData::default(),
            device,
        }
    }

    /// `ID3D12Object::GetPrivateData`.
    ///
    /// # Safety
    ///
    /// `guid` and `data_size` must be valid pointers or null; `data` must point to `*data_size`
    /// writable bytes when non-null.
    pub unsafe fn get_private_data(
        &self,
        guid: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> Hresult {
        // SAFETY: `guid` is null or 16 readable bytes; the rest is forwarded from the COM slot.
        unsafe { self.private_data.get_ffi(guid, data_size, data) }
    }

    /// `ID3D12Object::SetPrivateData`.
    ///
    /// # Safety
    ///
    /// `guid` must be valid or null; `data` must point to `data_size` readable bytes when
    /// non-null.
    pub unsafe fn set_private_data(
        &self,
        guid: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> Hresult {
        // SAFETY: `guid` is null or 16 readable bytes; the rest is forwarded from the COM slot.
        unsafe { self.private_data.set_ffi(guid, data_size, data) }
    }

    /// `ID3D12Object::SetPrivateDataInterface`.
    ///
    /// # Safety
    ///
    /// `guid` must be valid or null; `unknown` must be null or a valid `IUnknown` pointer.
    pub unsafe fn set_private_data_interface(
        &self,
        guid: *const c_void,
        unknown: *const c_void,
    ) -> Hresult {
        // SAFETY: `guid` is null or 16 readable bytes; the rest is forwarded from the COM slot.
        unsafe { self.private_data.set_interface_ffi(guid, unknown) }
    }

    /// `ID3D12Object::SetName` (`d3d12_device_child.hpp:50-56`).
    ///
    /// # Safety
    ///
    /// `name` must be null or point to a NUL-terminated UTF-16 string.
    pub unsafe fn set_name(&self, name: *const u16) -> Hresult {
        // SAFETY: forwarded from COM slot with caller's pointer.
        unsafe { set_debug_name(&self.private_data, name) }
    }

    /// `ID3D12DeviceChild::GetDevice`: forwards to the parent device's `QueryInterface`.
    ///
    /// # Safety
    ///
    /// `D`'s first field must be a vtable pointer starting with [`IUnknownVtbl`], and `riid` /
    /// `out` must be valid `QueryInterface` arguments.
    pub unsafe fn get_device(&self, riid: *const c_void, out: *mut *mut c_void) -> Hresult {
        let dev_ptr = self.device.as_ptr().cast::<c_void>();
        // SAFETY: `D` is a COM object whose first field is a vtable starting with `IUnknownVtbl`.
        let unk = unsafe { vtbl::<IUnknownVtbl>(dev_ptr) };
        // SAFETY: `dev_ptr` is live (`self.device` holds a reference) and `riid`/`out` are
        // forwarded from the caller.
        unsafe { (unk.query_interface)(dev_ptr, riid, out) }
    }
}

/// Stack reservation for D3D12 worker threads (`dxmt::thread` in `util/thread.hpp:55`, P16).
pub const THREAD_STACK_SIZE: usize = 0x10_0000;

/// Spawns a worker thread named `name` with [`THREAD_STACK_SIZE`] and `AbortOnPanic` (P16, P20).
///
/// # Panics
///
/// Panics if the operating system fails to spawn the thread.
pub fn spawn_thread<F>(name: &str, f: F) -> std::thread::JoinHandle<()>
where
    F: FnOnce() + Send + 'static,
{
    let truncated = &name[..name.floor_char_boundary(15)];
    std::thread::Builder::new()
        .name(truncated.to_owned())
        .stack_size(THREAD_STACK_SIZE)
        .spawn(move || {
            let _guard = dxmt_core::ffi::AbortOnPanic;
            f();
        })
        .expect("failed to spawn D3D12 worker thread")
}

/// Defines an `unsafe extern "system-unwind" fn` COM vtable slot thunk (P9, P20).
#[macro_export]
macro_rules! slot {
    ($scope:literal, ($($arg:ident : $arg_ty:ty),* $(,)?) -> $ret:ty => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) -> $ret {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $crate::scope!($scope);
            $body
        }
        thunk
    }};
    ($scope:literal, ($($arg:ident : $arg_ty:ty),* $(,)?) => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $crate::scope!($scope);
            $body;
        }
        thunk
    }};
    (($($arg:ident : $arg_ty:ty),* $(,)?) -> $ret:ty => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) -> $ret {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $body
        }
        thunk
    }};
    (($($arg:ident : $arg_ty:ty),* $(,)?) => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $body;
        }
        thunk
    }};
}

#[cfg(test)]
mod tests;
