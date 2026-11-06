//! `IDXGIObject`'s private data: bytes or an interface per GUID, as `com_private_data.cpp` has it.
//!
//! An application calls these from any thread, so the entries sit behind a lock; an interface's
//! `AddRef` and `Release` run outside it (a `Release` may call back into this object), but for the
//! `AddRef` that hands an entry out, which must happen while the entry still holds its reference.

use core::{ffi::c_void, ptr};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::com::{
    DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_FOUND, E_INVALIDARG, Guid, Hresult, IUnknownVtbl, S_FALSE,
    S_OK, release, vtbl,
};

/// The size of an interface entry: a pointer.
const POINTER_SIZE: u32 = if size_of::<*mut c_void>() == 4 { 4 } else { 8 };

/// `WKPDID_D3DDebugObjectNameW`: the name `SetName` stores (`d3d12_device_child.hpp:26`).
pub const WKPDID_D3D_DEBUG_OBJECT_NAME_W: Guid = Guid::new(
    0x4cca_5fd8,
    0x921f,
    0x42c8,
    [0x85, 0x66, 0x70, 0xca, 0xf2, 0xa9, 0xb7, 0x41],
);

/// An object whose interfaces answer the private-data slots from one [`PrivateData`].
///
/// The slots are `GetPrivateData`, `SetPrivateData`, `SetPrivateDataInterface` and `SetName`.
///
/// The four slot functions below ([`get_private_data`] and its siblings) are generic over it, so
/// a vtable names `get_private_data::<Resource>` where it had a thunk of its own.
pub trait HasPrivateData {
    /// The private data of the object `this` is an interface pointer of.
    ///
    /// # Safety
    ///
    /// `this` is a live interface pointer of a `Self`, which outlives `'a`.
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData;
}

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

    /// `GetPrivateData` from its slot: the application's GUID, then [`Self::get_data`].
    ///
    /// A null `guid` is `E_INVALIDARG`. The GUID is read unaligned.
    ///
    /// # Safety
    ///
    /// `guid` is null or addresses 16 readable bytes; `size` and `data` as [`Self::get_data`].
    pub unsafe fn get_ffi(
        &self,
        guid: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> Hresult {
        // SAFETY: the caller's: null or 16 readable bytes; the read assumes no alignment.
        let Some(guid) = (unsafe { Guid::read(guid.cast()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: the caller's.
        unsafe { self.get_data(guid, size, data) }
    }

    /// `SetPrivateData` from its slot: the application's GUID, then [`Self::set_data`].
    ///
    /// A null `guid` is `E_INVALIDARG`. The GUID is read unaligned.
    ///
    /// # Safety
    ///
    /// `guid` is null or addresses 16 readable bytes; `data` as [`Self::set_data`].
    pub unsafe fn set_ffi(&self, guid: *const c_void, size: u32, data: *const c_void) -> Hresult {
        // SAFETY: the caller's: null or 16 readable bytes; the read assumes no alignment.
        let Some(guid) = (unsafe { Guid::read(guid.cast()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: the caller's.
        unsafe { self.set_data(guid, size, data) }
    }

    /// `SetPrivateDataInterface` from its slot: the GUID, then [`Self::set_interface`].
    ///
    /// A null `guid` is `E_INVALIDARG`. The GUID is read unaligned.
    ///
    /// # Safety
    ///
    /// `guid` is null or addresses 16 readable bytes; `unknown` is null or a live COM object.
    pub unsafe fn set_interface_ffi(&self, guid: *const c_void, unknown: *const c_void) -> Hresult {
        // SAFETY: the caller's: null or 16 readable bytes; the read assumes no alignment.
        let Some(guid) = (unsafe { Guid::read(guid.cast()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: the caller's.
        unsafe { self.set_interface(guid, unknown.cast_mut()) }
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

/// `GetPrivateData`'s slot for a `T`.
///
/// # Safety
///
/// `this` is a live interface pointer of a `T`; the rest as [`PrivateData::get_ffi`].
pub unsafe extern "system-unwind" fn get_private_data<T: HasPrivateData>(
    this: *mut c_void,
    guid: *const c_void,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: the caller's: `this` is a live `T`'s interface.
    let private_data = unsafe { T::private_data(this) };
    // SAFETY: the caller's.
    unsafe { private_data.get_ffi(guid, size, data) }
}

/// `SetPrivateData`'s slot for a `T`.
///
/// # Safety
///
/// `this` is a live interface pointer of a `T`; the rest as [`PrivateData::set_ffi`].
pub unsafe extern "system-unwind" fn set_private_data<T: HasPrivateData>(
    this: *mut c_void,
    guid: *const c_void,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: the caller's: `this` is a live `T`'s interface.
    let private_data = unsafe { T::private_data(this) };
    // SAFETY: the caller's.
    unsafe { private_data.set_ffi(guid, size, data) }
}

/// `SetPrivateDataInterface`'s slot for a `T`.
///
/// # Safety
///
/// `this` is a live interface pointer of a `T`; the rest as [`PrivateData::set_interface_ffi`].
pub unsafe extern "system-unwind" fn set_private_data_interface<T: HasPrivateData>(
    this: *mut c_void,
    guid: *const c_void,
    unknown: *const c_void,
) -> Hresult {
    // SAFETY: the caller's: `this` is a live `T`'s interface.
    let private_data = unsafe { T::private_data(this) };
    // SAFETY: the caller's.
    unsafe { private_data.set_interface_ffi(guid, unknown) }
}

/// `ID3D12Object::SetName`'s slot for a `T`: [`set_debug_name`].
///
/// # Safety
///
/// `this` is a live interface pointer of a `T`; `name` is null or a NUL-terminated `WCHAR` string.
pub unsafe extern "system-unwind" fn set_name<T: HasPrivateData>(
    this: *mut c_void,
    name: *const c_void,
) -> Hresult {
    // SAFETY: the caller's: `this` is a live `T`'s interface.
    let private_data = unsafe { T::private_data(this) };
    // SAFETY: the caller's.
    unsafe { set_debug_name(private_data, name.cast()) }
}

/// Stores `name` under [`WKPDID_D3D_DEBUG_OBJECT_NAME_W`], its terminating NUL included.
///
/// A null `name` erases the entry (`d3d12_device_child.hpp:50-56`).
///
/// `WCHAR` is two bytes on Windows and four natively, where `wchar_t` is.
///
/// # Safety
///
/// `name` is null or a NUL-terminated `WCHAR` string, with no alignment promised.
pub unsafe fn set_debug_name(private_data: &PrivateData, name: *const u16) -> Hresult {
    #[cfg(windows)]
    type Wchar = u16;
    #[cfg(not(windows))]
    type Wchar = u32;

    if name.is_null() {
        // SAFETY: a null `data` erases the entry.
        return unsafe { private_data.set_data(WKPDID_D3D_DEBUG_OBJECT_NAME_W, 0, ptr::null()) };
    }
    let wide = name.cast::<Wchar>();
    let mut len = 0usize;
    loop {
        // SAFETY: `wide` is non-null and `len` has not yet reached the terminating NUL.
        let ch_ptr = unsafe { wide.add(len) };
        // SAFETY: `ch_ptr` points within the caller-supplied NUL-terminated wide string.
        if unsafe { ch_ptr.read_unaligned() } == 0 {
            break;
        }
        len += 1;
    }
    let byte_len = u32::try_from((len + 1) * size_of::<Wchar>()).unwrap_or(u32::MAX);
    // SAFETY: `name` points to `byte_len` readable bytes.
    unsafe { private_data.set_data(WKPDID_D3D_DEBUG_OBJECT_NAME_W, byte_len, name.cast()) }
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
