//! The COM plumbing of dxgi's objects: the reference count, `QueryInterface` and out pointers.
//!
//! An object is `#[repr(C)]` with a `Head` first, boxed, and its address is the `this` of every
//! interface it answers. Calls on objects dxgi did not make (an application's, d3d11's) go through
//! `dxmt_util::com::{query, release, vtbl}` with a partial vtable.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering, fence},
};
use std::sync::{Mutex, PoisonError};

use crate::abi::{E_NOINTERFACE, E_POINTER, Guid, Hresult, S_OK};

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

/// `QueryInterface` on an object that answers `answered` with itself.
///
/// A null `out` is `E_POINTER`; otherwise `*out` is nulled first, and a null `riid` is `E_POINTER`
/// too (the C++ reads through it). A match takes a reference to `this` through `add_ref` (the
/// object's slot) and writes it. Any other IID is `E_NOINTERFACE`, logged at warn once per object
/// and IID as `<name>: Unknown interface query <iid>` unless it is in `silent`; `log_as` is the
/// object's IID and name for that line.
///
/// # Safety
///
/// `this` is a live object whose `AddRef` is `add_ref`; `riid` is null or addresses a GUID; `out`
/// is null or addresses a writable pointer. Neither of the last two needs to be aligned.
pub unsafe fn query_interface(
    this: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
    answered: &[Guid],
    silent: &[Guid],
    log_as: (&Guid, &str),
    add_ref: extern "system" fn(*mut c_void) -> u32,
) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: the caller's: `out` is non-null here and writable.
    unsafe { write_out(out, core::ptr::null_mut()) };
    // SAFETY: the caller's: `riid` is null or addresses a GUID.
    let Some(riid) = (unsafe { Guid::read(riid) }) else {
        return E_POINTER;
    };
    if answered.contains(&riid) {
        add_ref(this);
        // SAFETY: as above.
        unsafe { write_out(out, this) };
        return S_OK;
    }
    if !silent.contains(&riid) && qi_log(log_as.0, &riid) {
        crate::LOG.warn(&format!("{}: Unknown interface query {riid}", log_as.1));
    }
    E_NOINTERFACE
}

/// Whether a miss of `riid` on `object` is the first, so the caller logs it.
///
/// `com_guid.cpp`'s `logQueryInterfaceError`.
/// The C++'s level check before it is the log's own: a warning below `DXMT_LOG_LEVEL` is dropped
/// by `Logger` either way.
pub fn qi_log(object: &Guid, riid: &Guid) -> bool {
    // ponytail: a linear scan, as the pairs are a handful per process; a set if one ever logs many.
    /// The pairs already logged.
    ///
    /// A per-call-site log-once latch (`com_guid.cpp`'s `g_loggedQueryInterfaceErrors`): it carries
    /// nothing beyond "fired", for every object of the process alike.
    static LOGGED: Mutex<Vec<(Guid, Guid)>> = Mutex::new(Vec::new());
    let mut logged = LOGGED.lock().unwrap_or_else(PoisonError::into_inner);
    let pair = (*object, *riid);
    if logged.contains(&pair) {
        return false;
    }
    logged.push(pair);
    true
}

/// Writes `value` to `out`, which promises no alignment.
///
/// # Safety
///
/// `out` is non-null and addresses a writable `P`; the caller checks it for null.
pub const unsafe fn write_out<P>(out: *mut P, value: P) {
    // SAFETY: the caller's; the write assumes no alignment.
    unsafe { out.write_unaligned(value) };
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
