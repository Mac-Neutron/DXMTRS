//! `IDXGIObject`'s private data: bytes or an interface per GUID, as `com_private_data.cpp` has it.
//!
//! An application calls these from any thread, so the entries sit behind a lock; an interface's
//! `AddRef` and `Release` run outside it (a `Release` may call back into this object), but for the
//! `AddRef` that hands an entry out, which must happen while the entry still holds its reference.

use core::ffi::c_void;
use std::sync::{Mutex, MutexGuard, PoisonError};

use dxmt_util::com::{IUnknownVtbl, release, vtbl};

use crate::abi::{
    DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_FOUND, E_INVALIDARG, Guid, Hresult, S_FALSE, S_OK,
};

/// The size of an interface entry: a pointer.
const POINTER_SIZE: u32 = if size_of::<*mut c_void>() == 4 { 4 } else { 8 };

/// One object's private data: its entries in the order they were first set.
#[derive(Default)]
pub struct PrivateData {
    entries: Mutex<Vec<Entry>>,
}

impl PrivateData {
    /// `SetPrivateData`: stores `size` bytes of `data`, or with a null `data` erases the entry.
    ///
    /// Erasing answers `S_OK`, or `S_FALSE` when there was none. An entry replaced or erased gives
    /// its interface back after the lock is dropped.
    ///
    /// # Safety
    ///
    /// `data` is null or addresses `size` readable bytes.
    pub unsafe fn set_data(&self, guid: Guid, size: u32, data: *const c_void) -> Hresult {
        if data.is_null() {
            let mut entries = self.lock();
            let at = entries.iter().position(|entry| entry.guid == guid);
            let erased = at.map(|at| entries.remove(at));
            drop(entries);
            return if erased.is_some() { S_OK } else { S_FALSE };
        }
        // SAFETY: the caller's: `data` addresses `size` readable bytes; bytes have no alignment.
        let bytes = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size as usize) };
        drop(self.insert(guid, Value::Data(bytes.into())));
        S_OK
    }

    /// `SetPrivateDataInterface`: stores `unknown`, taking a reference, or a null one.
    ///
    /// A replaced interface is given back after the lock is dropped.
    ///
    /// # Safety
    ///
    /// `unknown` is null or a live COM object.
    pub unsafe fn set_interface(&self, guid: Guid, unknown: *mut c_void) -> Hresult {
        // SAFETY: the caller's.
        let unknown = unsafe { Unknown::new(unknown) };
        drop(self.insert(guid, Value::Interface(unknown)));
        S_OK
    }

    /// `GetPrivateData`: copies the entry to `data`, or with a null `data` only answers its size.
    ///
    /// A null `size` is `E_INVALIDARG`; a missing entry `DXGI_ERROR_NOT_FOUND` with `*size` 0.
    /// With a non-null `data`, a `*size` below the entry's is `DXGI_ERROR_MORE_DATA` with nothing
    /// copied, and on both paths `*size` becomes the entry's size. An interface entry takes a
    /// reference for the caller before its pointer is written.
    ///
    /// # Safety
    ///
    /// `size` is null or addresses a readable and writable `u32`; `data` is null or addresses
    /// `*size` writable bytes. Neither needs to be aligned.
    pub unsafe fn get_data(&self, guid: Guid, size: *mut u32, data: *mut c_void) -> Hresult {
        if size.is_null() {
            return E_INVALIDARG;
        }
        let entries = self.lock();
        let Some(entry) = entries.iter().find(|entry| entry.guid == guid) else {
            drop(entries);
            // SAFETY: the caller's: `size` is non-null here and writable.
            unsafe { size.write_unaligned(0) };
            return DXGI_ERROR_NOT_FOUND;
        };
        let needed = entry.value.size();
        let hr = if data.is_null() {
            S_OK
        } else {
            // SAFETY: as above, and readable.
            let room = unsafe { size.read_unaligned() };
            if room < needed {
                DXGI_ERROR_MORE_DATA
            } else {
                // SAFETY: the caller's: `data` has room for `needed` bytes.
                unsafe { entry.value.copy_to(data) };
                S_OK
            }
        };
        drop(entries);
        // SAFETY: as above.
        unsafe { size.write_unaligned(needed) };
        hr
    }

    /// The entries, whatever a thread that panicked holding them left.
    fn lock(&self) -> MutexGuard<'_, Vec<Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sets `guid`'s entry to `value`: the value it replaces, for the caller to drop unlocked.
    fn insert(&self, guid: Guid, value: Value) -> Option<Value> {
        let mut entries = self.lock();
        if let Some(entry) = entries.iter_mut().find(|entry| entry.guid == guid) {
            return Some(core::mem::replace(&mut entry.value, value));
        }
        entries.push(Entry { guid, value });
        None
    }
}

/// An entry: its GUID and what it holds.
struct Entry {
    guid: Guid,
    value: Value,
}

/// What an entry holds.
enum Value {
    Data(Box<[u8]>),
    Interface(Unknown),
}

impl Value {
    /// The size `GetPrivateData` answers: the bytes', or a pointer's.
    fn size(&self) -> u32 {
        match self {
            Self::Data(bytes) => u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            Self::Interface(_) => POINTER_SIZE,
        }
    }

    /// Copies the value to `data`; an interface takes a reference for the caller first.
    ///
    /// # Safety
    ///
    /// `data` addresses `self.size()` writable bytes, with no alignment promised.
    unsafe fn copy_to(&self, data: *mut c_void) {
        match self {
            Self::Data(bytes) => {
                // SAFETY: the caller's: room for the bytes, and bytes have no alignment.
                unsafe {
                    core::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast::<u8>(), bytes.len());
                }
            }
            Self::Interface(unknown) => {
                unknown.add_ref();
                // SAFETY: the caller's: room for a pointer; the write assumes no alignment.
                unsafe { data.cast::<*mut c_void>().write_unaligned(unknown.0) };
            }
        }
    }
}

/// An interface the store holds a reference to, or null; dropping it gives the reference back.
struct Unknown(*mut c_void);

// SAFETY: an interface handed to DXGI's private data is a COM object any thread may AddRef and
// Release (the C++ store holds it the same way); `Unknown` does nothing else with it.
unsafe impl Send for Unknown {}

impl Unknown {
    /// Takes a reference to `unknown`.
    ///
    /// # Safety
    ///
    /// `unknown` is null or a live COM object.
    unsafe fn new(unknown: *mut c_void) -> Self {
        let unknown = Self(unknown);
        unknown.add_ref();
        unknown
    }

    /// Takes another reference to the interface, unless it is null.
    fn add_ref(&self) {
        if self.0.is_null() {
            return;
        }
        // SAFETY: a non-null `Unknown` holds a reference to a live COM object, which starts with
        // `IUnknown`'s slots.
        let slots = unsafe { vtbl::<IUnknownVtbl>(self.0) };
        // SAFETY: as above: the object is live while this reference is held.
        unsafe { (slots.add_ref)(self.0) };
    }
}

impl Drop for Unknown {
    fn drop(&mut self) {
        // SAFETY: `new` took the reference this gives back; a null one does nothing.
        unsafe { release(self.0) };
    }
}

const _: () = assert!(POINTER_SIZE as usize == size_of::<*mut c_void>());

#[cfg(test)]
mod tests;
