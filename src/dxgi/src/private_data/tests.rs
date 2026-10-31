use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use dxmt_util::com::IUnknownVtbl;

use super::*;
use crate::abi::{DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_FOUND, E_INVALIDARG, S_FALSE};

const NAME: Guid = Guid::new(0x1234_5678, 0x9abc, 0xdef0, [1, 2, 3, 4, 5, 6, 7, 8]);
const OTHER: Guid = Guid::new(0x1234_5678, 0x9abc, 0xdef0, [1, 2, 3, 4, 5, 6, 7, 9]);

/// A fake application interface: `IUnknown`'s slots, its count, and its test's call log.
#[repr(C)]
struct Fake {
    vtbl: &'static IUnknownVtbl,
    refs: AtomicU32,
    name: char,
    log: Arc<Mutex<Vec<String>>>,
}

static FAKE_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: fake_query_interface,
    add_ref: fake_add_ref,
    release: fake_release,
};

extern "system" fn fake_query_interface(
    _: *mut c_void,
    _: *const Guid,
    _: *mut *mut c_void,
) -> i32 {
    crate::abi::E_NOINTERFACE
}

/// The fake behind a slot's `this`.
fn fake<'a>(this: *mut c_void) -> &'a Fake {
    // SAFETY: a fake's slots get a `this` that is a live `Fake`, which the test keeps alive.
    unsafe { &*this.cast::<Fake>() }
}

extern "system" fn fake_add_ref(this: *mut c_void) -> u32 {
    let fake = fake(this);
    fake.log
        .lock()
        .unwrap()
        .push(format!("{} AddRef", fake.name));
    fake.refs.fetch_add(1, Ordering::Relaxed) + 1
}

extern "system" fn fake_release(this: *mut c_void) -> u32 {
    let fake = fake(this);
    fake.log
        .lock()
        .unwrap()
        .push(format!("{} Release", fake.name));
    fake.refs.fetch_sub(1, Ordering::Relaxed) - 1
}

impl Fake {
    /// A fake with one reference, the test's.
    fn new(name: char, log: &Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            vtbl: &FAKE_VTBL,
            refs: AtomicU32::new(1),
            name,
            log: Arc::clone(log),
        }
    }

    fn this(&self) -> *mut c_void {
        core::ptr::from_ref(self).cast_mut().cast()
    }

    fn refs(&self) -> u32 {
        self.refs.load(Ordering::Relaxed)
    }
}

/// Stores `size` bytes of `bytes` under `guid`.
fn set_bytes(store: &PrivateData, guid: Guid, bytes: &[u8]) -> i32 {
    let size = u32::try_from(bytes.len()).unwrap();
    // SAFETY: `bytes` has `size` readable bytes.
    unsafe { store.set_data(guid, size, bytes.as_ptr().cast()) }
}

/// `get_data` into `buffer`, its size `size`: the result and the size written back.
fn get(store: &PrivateData, guid: Guid, size: u32, buffer: *mut c_void) -> (i32, u32) {
    let mut size = size;
    // SAFETY: `size` is a local; `buffer` is null or has room for `size` bytes.
    let hr = unsafe { store.get_data(guid, &raw mut size, buffer) };
    (hr, size)
}

/// `set_interface` with `unknown`, a fake or null.
fn set_interface(store: &PrivateData, guid: Guid, unknown: *mut c_void) -> i32 {
    // SAFETY: `unknown` is null or a live fake.
    unsafe { store.set_interface(guid, unknown) }
}

/// The interface stored under `guid`, with a reference the store took for the caller.
fn get_interface(store: &PrivateData, guid: Guid) -> (i32, *mut c_void) {
    let mut out = core::ptr::without_provenance_mut::<c_void>(0xdead);
    let (hr, size) = get(store, guid, POINTER_SIZE, (&raw mut out).cast());
    assert_eq!(size, POINTER_SIZE);
    (hr, out)
}

#[test]
fn too_small_writes_nothing_but_the_size() {
    let store = PrivateData::default();
    assert_eq!(set_bytes(&store, NAME, &[1, 2, 3, 4]), S_OK);
    let mut buffer = [0xaa_u8; 8];
    assert_eq!(
        get(&store, NAME, 3, buffer.as_mut_ptr().cast()),
        (DXGI_ERROR_MORE_DATA, 4)
    );
    assert_eq!(buffer, [0xaa; 8]);
}

#[test]
fn big_buffer_shrinks_the_size() {
    let store = PrivateData::default();
    assert_eq!(set_bytes(&store, NAME, &[1, 2, 3, 4]), S_OK);
    let mut buffer = [0xaa_u8; 8];
    assert_eq!(get(&store, NAME, 8, buffer.as_mut_ptr().cast()), (S_OK, 4));
    assert_eq!(buffer, [1, 2, 3, 4, 0xaa, 0xaa, 0xaa, 0xaa]);
}

#[test]
fn null_data_returns_size() {
    let store = PrivateData::default();
    assert_eq!(set_bytes(&store, NAME, &[1, 2, 3, 4, 5]), S_OK);
    assert_eq!(get(&store, NAME, 0, core::ptr::null_mut()), (S_OK, 5));
    // SAFETY: a null size pointer is checked before anything else.
    let hr = unsafe { store.get_data(NAME, core::ptr::null_mut(), core::ptr::null_mut()) };
    assert_eq!(hr, E_INVALIDARG);
}

#[test]
fn null_interface_is_stored() {
    let store = PrivateData::default();
    assert_eq!(set_interface(&store, NAME, core::ptr::null_mut()), S_OK);
    assert_eq!(get_interface(&store, NAME), (S_OK, core::ptr::null_mut()));
}

#[test]
fn replace_adds_before_it_releases() {
    let log = Arc::default();
    let (a, b) = (Fake::new('a', &log), Fake::new('b', &log));
    let store = PrivateData::default();
    assert_eq!(set_interface(&store, NAME, a.this()), S_OK);
    assert_eq!(set_interface(&store, NAME, b.this()), S_OK);
    assert_eq!(*log.lock().unwrap(), ["a AddRef", "b AddRef", "a Release"]);
    assert_eq!((a.refs(), b.refs()), (1, 2));
    drop(store);
    assert_eq!(b.refs(), 1);
}

#[test]
fn missing_is_not_found_size_zero() {
    let store = PrivateData::default();
    assert_eq!(set_bytes(&store, OTHER, &[1]), S_OK);
    let mut buffer = [0xaa_u8; 4];
    assert_eq!(
        get(&store, NAME, 4, buffer.as_mut_ptr().cast()),
        (DXGI_ERROR_NOT_FOUND, 0)
    );
    assert_eq!(buffer, [0xaa; 4]);
}

#[test]
fn erase_then_s_false() {
    let store = PrivateData::default();
    assert_eq!(set_bytes(&store, NAME, &[1, 2]), S_OK);
    // SAFETY: a null `data` erases; nothing is read.
    assert_eq!(unsafe { store.set_data(NAME, 2, core::ptr::null()) }, S_OK);
    // SAFETY: as above.
    let again = unsafe { store.set_data(NAME, 2, core::ptr::null()) };
    assert_eq!(again, S_FALSE);
    assert_eq!(
        get(&store, NAME, 0, core::ptr::null_mut()),
        (DXGI_ERROR_NOT_FOUND, 0)
    );
}

#[test]
fn erase_releases_interface() {
    let log = Arc::default();
    let a = Fake::new('a', &log);
    let store = PrivateData::default();
    assert_eq!(set_interface(&store, NAME, a.this()), S_OK);
    assert_eq!(a.refs(), 2);
    // SAFETY: a null `data` erases; nothing is read.
    assert_eq!(unsafe { store.set_data(NAME, 0, core::ptr::null()) }, S_OK);
    assert_eq!(a.refs(), 1);
}

#[test]
fn get_adds_a_reference() {
    let log = Arc::default();
    let a = Fake::new('a', &log);
    let store = PrivateData::default();
    assert_eq!(set_interface(&store, NAME, a.this()), S_OK);
    assert_eq!(get_interface(&store, NAME), (S_OK, a.this()));
    assert_eq!(a.refs(), 3);
    // A buffer too small for a pointer: MORE_DATA, no reference.
    let mut small = [0_u8; 2];
    assert_eq!(
        get(&store, NAME, 2, small.as_mut_ptr().cast()),
        (DXGI_ERROR_MORE_DATA, POINTER_SIZE)
    );
    assert_eq!(a.refs(), 3);
}

#[test]
fn drop_releases_interfaces() {
    let log = Arc::default();
    let (a, b) = (Fake::new('a', &log), Fake::new('b', &log));
    let store = PrivateData::default();
    assert_eq!(set_interface(&store, NAME, a.this()), S_OK);
    assert_eq!(set_interface(&store, OTHER, b.this()), S_OK);
    assert_eq!(set_bytes(&store, Guid::new(1, 2, 3, [0; 8]), &[9]), S_OK);
    drop(store);
    assert_eq!((a.refs(), b.refs()), (1, 1));
}

#[test]
fn two_threads_set_get_balance() {
    let log = Arc::default();
    let (a, b) = (Fake::new('a', &log), Fake::new('b', &log));
    let store = PrivateData::default();
    let fakes = [a.this().expose_provenance(), b.this().expose_provenance()];
    std::thread::scope(|scope| {
        for address in fakes {
            let store = &store;
            scope.spawn(move || {
                let mine = core::ptr::with_exposed_provenance_mut::<c_void>(address);
                for _ in 0..2_000 {
                    assert_eq!(set_interface(store, NAME, mine), S_OK);
                    let (hr, got) = get_interface(store, NAME);
                    assert_eq!(hr, S_OK);
                    let got = fake(got);
                    assert!(got.name == 'a' || got.name == 'b');
                    // SAFETY: `got` is a live fake; this gives back the reference `get_data` took.
                    unsafe { dxmt_util::com::release(got.this()) };
                }
            });
        }
    });
    drop(store);
    assert_eq!((a.refs(), b.refs()), (1, 1));
    let log = log.lock().unwrap();
    let count = |what: &str| log.iter().filter(|line| line.ends_with(what)).count();
    assert_eq!(count("AddRef"), count("Release"));
}
