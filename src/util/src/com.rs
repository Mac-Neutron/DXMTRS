//! COM's `IUnknown`, the two-count `ComObject` of `com_object.hpp`, and `QueryInterface` helpers.

pub mod hresult;
pub mod iid;

use core::{
    ffi::c_void,
    fmt,
    ops::Deref,
    sync::atomic::{AtomicU32, Ordering},
};
use std::sync::{Mutex, PoisonError};

pub mod companion;
pub mod hold;

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
/// `DXGI_ERROR_NOT_FOUND`.
pub const DXGI_ERROR_NOT_FOUND: Hresult = 0x887a_0002_u32.cast_signed();
/// `DXGI_ERROR_MORE_DATA`.
pub const DXGI_ERROR_MORE_DATA: Hresult = 0x887a_0003_u32.cast_signed();

/// `IUnknown`'s IID: `00000000-0000-0000-c000-000000000046`.
pub const IID_IUNKNOWN: Guid = Guid::new(0, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);

/// The high-bit guard added to `priv_count` before a destructor runs (`com_object.hpp:64`).
///
/// If a destructor takes and releases a private reference on `self`, the release sees a count
/// above `1` and does not free the object a second time.
const DESTROYING_BIT: u32 = 0x8000_0000;

/// The two-count COM reference counter of `com_object.hpp`.
///
/// Public references (`AddRef` and `Release`) keep one private reference alive while `pub_count`
/// is non-zero: the 0 to 1 public transition adds a private reference, and the 1 to 0 public
/// transition releases it. The object is freed when `priv_count` reaches 0.
#[repr(C)]
pub struct ComObject {
    pub pub_count: AtomicU32,
    pub priv_count: AtomicU32,
}

impl Default for ComObject {
    fn default() -> Self {
        Self::new()
    }
}

impl ComObject {
    /// A counter starting at 0 public and 0 private references (`ComObject` in `com_object.hpp`).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pub_count: AtomicU32::new(0),
            priv_count: AtomicU32::new(0),
        }
    }

    /// A counter starting at 1 public and 1 private reference (`ComObjectWithInitialRef`).
    #[must_use]
    pub const fn with_initial_ref() -> Self {
        Self {
            pub_count: AtomicU32::new(1),
            priv_count: AtomicU32::new(1),
        }
    }

    /// `AddRef`: increments the public count, taking a private reference on 0 to 1.
    pub fn add_ref(&self) -> u32 {
        let prev = self.pub_count.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            self.add_ref_private();
        }
        prev + 1
    }

    /// `Release`: decrements the public count, releasing the public hold's private reference at 0.
    pub fn release(&self) -> u32 {
        self.release_and_free().0
    }

    /// `Release`, also answering whether the last private reference was just released.
    pub fn release_and_free(&self) -> (u32, bool) {
        let remaining = self.pub_count.fetch_sub(1, Ordering::SeqCst) - 1;
        let free = if remaining == 0 {
            self.release_private()
        } else {
            false
        };
        (remaining, free)
    }

    /// `AddRefPrivate`: increments the private count.
    pub fn add_ref_private(&self) {
        self.priv_count.fetch_add(1, Ordering::SeqCst);
    }

    /// `ReleasePrivate`: decrements the private count; `true` at 1 to 0 so the caller frees `self`.
    #[must_use]
    pub fn release_private(&self) -> bool {
        let prev = self.priv_count.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            self.priv_count.fetch_add(DESTROYING_BIT, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// The live private reference count (excluding the destruction-in-progress guard bit).
    #[must_use]
    pub fn private_count(&self) -> u32 {
        self.priv_count.load(Ordering::SeqCst) & !DESTROYING_BIT
    }

    /// The live public reference count.
    #[must_use]
    pub fn public_count(&self) -> u32 {
        self.pub_count.load(Ordering::SeqCst)
    }
}

/// `ComObjectWithInitialRef` of `com_object.hpp`: starts with 1 public and 1 private reference.
///
/// `MTLDXGIObject` (and through it `MTLD3D11SwapChain` and `MTLD3D12SwapChain`) inherits from it.
#[repr(transparent)]
pub struct ComObjectWithInitialRef(pub ComObject);

impl Default for ComObjectWithInitialRef {
    fn default() -> Self {
        Self::new()
    }
}

impl ComObjectWithInitialRef {
    /// A counter starting at 1 public and 1 private reference.
    #[must_use]
    pub const fn new() -> Self {
        Self(ComObject::with_initial_ref())
    }
}

impl Deref for ComObjectWithInitialRef {
    type Target = ComObject;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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

/// Takes a reference to `object`; null does nothing.
///
/// # Safety
///
/// `object` is a live COM object or null.
pub unsafe fn add_ref(object: *mut c_void) {
    if !object.is_null() {
        // SAFETY: every COM interface starts with `IUnknown`'s slots.
        let unknown = unsafe { vtbl::<IUnknownVtbl>(object) };
        // SAFETY: `object` is live.
        unsafe { (unknown.add_ref)(object) };
    }
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

/// The line a `QueryInterface` miss writes to the calling DLL's log, as each object has it.
pub enum QiMiss<'a> {
    /// `<name>: Unknown interface query <riid>` at warn, once per `object` and `riid` (`qi_log`).
    Once { object: &'a Guid, name: &'a str },
    /// `<name>: Unknown interface query` and then `<riid>`, two warnings, once per pair.
    OnceSplit { object: &'a Guid, name: &'a str },
    /// `<name>: Unknown interface query <riid>` at warn, at every miss.
    Every(&'a str),
    /// No line.
    Quiet,
}

/// What an object's `QueryInterface` does besides its table, each object's own choice.
///
/// `null_riid` is the answer to a null `riid` (`E_NOINTERFACE`, `E_INVALIDARG`, `E_POINTER` or
/// `DXGI_ERROR_INVALID_CALL`: each body has its own); `silent` lists the IIDs refused with
/// `E_NOINTERFACE` and no line; `log` is the calling DLL's log, where `miss` writes.
pub struct Qi<'a> {
    pub null_riid: Hresult,
    pub silent: &'a [Guid],
    pub log: &'a crate::log::Logger,
    pub miss: QiMiss<'a>,
}

/// `QueryInterface` over `entries`, a table the caller builds per call.
///
/// A null `out` is `E_POINTER`; otherwise `*out` is nulled first, and a null `riid` answers
/// `qi.null_riid`. The first entry whose IIDs hold `riid` takes a reference through `add_ref` and
/// writes its pointer; an entry the object answers only sometimes is listed with no IIDs when it
/// does not. Any other IID is `E_NOINTERFACE`, logged as `qi.miss` says unless it is in
/// `qi.silent`. `riid` and `out` are read and written unaligned.
///
/// # Safety
///
/// `riid` is null or addresses a GUID; `out` is null or addresses a writable pointer.
pub unsafe fn qi_table(
    riid: *const c_void,
    out: *mut *mut c_void,
    entries: &[(&[Guid], *mut c_void)],
    add_ref: impl FnOnce(),
    qi: &Qi<'_>,
) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: the caller's: `out` is non-null here and writable; the write assumes no alignment.
    unsafe { write_out(out, core::ptr::null_mut()) };
    // SAFETY: the caller's: `riid` is null or addresses a GUID; the read assumes no alignment.
    let Some(riid) = (unsafe { Guid::read(riid.cast()) }) else {
        return qi.null_riid;
    };
    if let Some(&(_, iface)) = entries.iter().find(|(iids, _)| iids.contains(&riid)) {
        add_ref();
        // SAFETY: as above.
        unsafe { write_out(out, iface) };
        return S_OK;
    }
    if !qi.silent.contains(&riid) {
        qi.miss.log(qi.log, &riid);
    }
    E_NOINTERFACE
}

/// `QueryInterface` refusing before its table (a null `this`, say): `hr`, with `*out` nulled.
///
/// A null `out` is `E_POINTER`, as in [`qi_table`].
///
/// # Safety
///
/// `out` is null or addresses a writable pointer, with no alignment promised.
pub const unsafe fn qi_refuse(out: *mut *mut c_void, hr: Hresult) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: the caller's: `out` is non-null here and writable; the write assumes no alignment.
    unsafe { write_out(out, core::ptr::null_mut()) };
    hr
}

impl QiMiss<'_> {
    /// Writes the line for a miss of `riid` to `log`.
    fn log(&self, log: &crate::log::Logger, riid: &Guid) {
        match *self {
            Self::Once { object, name } => {
                if qi_log(object, riid) {
                    log.warn(&format!("{name}: Unknown interface query {riid}"));
                }
            }
            Self::OnceSplit { object, name } => {
                if qi_log(object, riid) {
                    log.warn(&format!("{name}: Unknown interface query"));
                    log.warn(&riid.to_string());
                }
            }
            Self::Every(name) => log.warn(&format!("{name}: Unknown interface query {riid}")),
            Self::Quiet => {}
        }
    }
}

#[cfg(test)]
mod tests;
