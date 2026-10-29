//! COM's `IUnknown`, and the calls on it every DXMT DLL makes.

use core::ffi::c_void;

/// A COM interface ID.
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
        Self { data1, data2, data3, data4 }
    }
}

pub type Hresult = i32;

/// `IUnknown`'s slots, which every interface starts with.
#[repr(C)]
pub struct IUnknownVtbl {
    pub query_interface: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult,
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
