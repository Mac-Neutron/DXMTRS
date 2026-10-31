//! COM's `IUnknown`, and the calls on it every DXMT DLL makes.

use core::{ffi::c_void, fmt};

/// A COM interface ID.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

impl Guid {
    /// A GUID from its four fields.
    #[must_use]
    pub const fn new(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> Self {
        Self {
            data1,
            data2,
            data3,
            data4,
        }
    }

    /// Reads the GUID at `p`, which promises no alignment: none for a null pointer.
    ///
    /// # Safety
    ///
    /// `p` is null or addresses 16 readable bytes.
    #[must_use]
    pub unsafe fn read(p: *const Self) -> Option<Self> {
        // SAFETY: the caller's: `p` is non-null here and addresses a GUID; the read assumes no alignment.
        (!p.is_null()).then(|| unsafe { p.read_unaligned() })
    }
}

/// `com_guid.cpp`'s form: lowercase hex, `00000000-0000-0000-0000-000000000000`.
impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-",
            self.data1, self.data2, self.data3
        )?;
        for (i, byte) in self.data4.iter().enumerate() {
            if i == 2 {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

pub type Hresult = i32;

/// `S_OK`.
pub const S_OK: Hresult = 0;
/// `S_FALSE`.
pub const S_FALSE: Hresult = 1;
/// `E_NOTIMPL`.
pub const E_NOTIMPL: Hresult = 0x8000_4001_u32.cast_signed();
/// `E_NOINTERFACE`.
pub const E_NOINTERFACE: Hresult = 0x8000_4002_u32.cast_signed();
/// `E_POINTER`.
pub const E_POINTER: Hresult = 0x8000_4003_u32.cast_signed();
/// `E_FAIL`.
pub const E_FAIL: Hresult = 0x8000_4005_u32.cast_signed();
/// `E_OUTOFMEMORY`.
pub const E_OUTOFMEMORY: Hresult = 0x8007_000e_u32.cast_signed();
/// `E_INVALIDARG`.
pub const E_INVALIDARG: Hresult = 0x8007_0057_u32.cast_signed();

/// `IUnknown`'s slots, which every interface starts with.
#[repr(C)]
pub struct IUnknownVtbl {
    pub query_interface:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult,
    pub add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system" fn(*mut c_void) -> u32,
}

/// A COM object: its vtable pointer first.
#[repr(C)]
pub struct Object<V> {
    pub vtbl: *const V,
}

/// Asks `object` for the interface `iid`: the new reference, or none.
///
/// # Safety
///
/// `object` is a live COM object or null.
pub unsafe fn query(object: *mut c_void, iid: &Guid) -> Option<*mut c_void> {
    if object.is_null() {
        return None;
    }
    let mut out = core::ptr::null_mut();
    // SAFETY: every COM interface starts with `IUnknown`'s slots.
    let unknown = unsafe { vtbl::<IUnknownVtbl>(object) };
    // SAFETY: `object` is live; `out` is a local the call writes.
    let hr = unsafe { (unknown.query_interface)(object, iid, &raw mut out) };
    (hr >= 0 && !out.is_null()).then_some(out)
}

/// Releases a reference to `object`; null does nothing.
///
/// # Safety
///
/// `object` is a live COM object the caller holds a reference to, or null.
pub unsafe fn release(object: *mut c_void) {
    if !object.is_null() {
        // SAFETY: every COM interface starts with `IUnknown`'s slots.
        let unknown = unsafe { vtbl::<IUnknownVtbl>(object) };
        // SAFETY: the caller holds the reference this gives back.
        unsafe { (unknown.release)(object) };
    }
}

/// The vtable of `object`, seen as `V`.
///
/// # Safety
///
/// `object` is a live COM object whose interface starts with `V`'s slots.
pub unsafe fn vtbl<'a, V>(object: *mut c_void) -> &'a V {
    // SAFETY: the caller's: a live COM object, whose first field is its vtable pointer.
    let head = unsafe { &*object.cast::<Object<V>>() };
    // SAFETY: a live object's vtable lives as long as its module, past any use here.
    unsafe { &*head.vtbl }
}

#[cfg(test)]
mod tests;
