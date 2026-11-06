use core::ffi::c_void;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use super::{
    ComObject, ComObjectWithInitialRef, E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, Qi, QiMiss,
    S_OK, qi_log, qi_refuse, qi_table,
};
use crate::log::Logger;

/// `IMTLDXGIAdapter`'s IID.
const IMTL_DXGI_ADAPTER: Guid = Guid::new(
    0xacdf_3ef1,
    0xb33a,
    0x4cb6,
    [0x97, 0xbd, 0x1c, 0x19, 0x74, 0x82, 0x7e, 0x6d],
);

#[test]
fn guid_display_is_com_guids_form() {
    assert_eq!(
        IMTL_DXGI_ADAPTER.to_string(),
        "acdf3ef1-b33a-4cb6-97bd-1c1974827e6d"
    );
    // Zero padding, and lowercase.
    assert_eq!(
        Guid::new(1, 2, 3, [4, 5, 0xab, 6, 7, 8, 9, 0xcd]).to_string(),
        "00000001-0002-0003-0405-ab06070809cd"
    );
}

/// Bytes that start on an 8-byte boundary, so one byte in is misaligned for any wider `T`.
#[repr(align(8))]
struct Bytes([u8; 24]);

impl Bytes {
    /// A pointer one byte in, where no `T` of two bytes or more is aligned.
    fn odd<T>(&mut self) -> *mut T {
        let odd = self.0[1..].as_mut_ptr().cast::<T>();
        assert!(!odd.is_aligned(), "the test needs a misaligned pointer");
        odd
    }
}

#[test]
fn guid_read_takes_an_odd_address() {
    let mut bytes = Bytes([0; 24]);
    let odd = bytes.odd::<Guid>();
    // SAFETY: `odd` addresses the 16 bytes after the first of `bytes`' 24.
    unsafe { odd.write_unaligned(IMTL_DXGI_ADAPTER) };
    // SAFETY: `odd` addresses the 16 initialised bytes written above.
    let read = unsafe { Guid::read(odd) };
    assert!(read == Some(IMTL_DXGI_ADAPTER));
}

#[test]
fn guid_read_of_null_is_none() {
    // SAFETY: a null pointer is the case `read` answers with none.
    let read = unsafe { Guid::read(core::ptr::null()) };
    assert!(read.is_none());
}

#[test]
fn guids_compare_by_value() {
    let copy = IMTL_DXGI_ADAPTER;
    assert!(copy == IMTL_DXGI_ADAPTER);
    assert!(copy != Guid::new(0, 0, 0, [0; 8]));
}

/// A test object whose `Drop` re-enters `ComObject` by taking and releasing a private reference.
struct ReentrantDrop {
    com: ComObject,
    drops: Arc<AtomicU32>,
    nested_free: Arc<AtomicU32>,
}

impl Drop for ReentrantDrop {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
        self.com.add_ref_private();
        if self.com.release_private() {
            self.nested_free.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[test]
fn counts_as_cpp() {
    let obj = ComObject::new();
    assert_eq!((obj.public_count(), obj.private_count()), (0, 0));

    // First public AddRef sets both public and private counts to 1.
    assert_eq!(obj.add_ref(), 1);
    assert_eq!((obj.public_count(), obj.private_count()), (1, 1));

    // Second public AddRef increments public to 2 while private stays at 1.
    assert_eq!(obj.add_ref(), 2);
    assert_eq!((obj.public_count(), obj.private_count()), (2, 1));

    // AddRefPrivate increments private to 2.
    obj.add_ref_private();
    assert_eq!((obj.public_count(), obj.private_count()), (2, 2));

    // Two public Releases bring public to 0 and private to 1 without freeing.
    assert_eq!(obj.release_and_free(), (1, false));
    assert_eq!((obj.public_count(), obj.private_count()), (1, 2));
    assert_eq!(obj.release_and_free(), (0, false));
    assert_eq!((obj.public_count(), obj.private_count()), (0, 1));

    // ReleasePrivate brings private to 0 and signals free once.
    assert!(obj.release_private());
    assert_eq!((obj.public_count(), obj.private_count()), (0, 0));

    // ComObjectWithInitialRef starts at (1, 1) and frees on the first public Release.
    let init = ComObjectWithInitialRef::new();
    assert_eq!((init.public_count(), init.private_count()), (1, 1));
    assert_eq!(init.release_and_free(), (0, true));
    assert_eq!((init.public_count(), init.private_count()), (0, 0));

    // A destructor that takes and releases a private ref frees once (0x80000000 guard).
    let drops = Arc::new(AtomicU32::new(0));
    let nested_free = Arc::new(AtomicU32::new(0));
    let raw = Box::into_raw(Box::new(ReentrantDrop {
        com: ComObject::new(),
        drops: Arc::clone(&drops),
        nested_free: Arc::clone(&nested_free),
    }));
    // SAFETY: `raw` is a live allocation from `Box::into_raw`.
    assert_eq!(unsafe { (*raw).com.add_ref() }, 1);
    // SAFETY: `raw` is still live; this releases the sole public reference.
    let (rem, free) = unsafe { (*raw).com.release_and_free() };
    assert_eq!((rem, free), (0, true));
    // SAFETY: `free` was true once, so we own `raw` and drop it here.
    drop(unsafe { Box::from_raw(raw) });
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(nested_free.load(Ordering::SeqCst), 0);
}

#[test]
fn private_hold_outlives_public() {
    let drops = Arc::new(AtomicU32::new(0));
    let nested_free = Arc::new(AtomicU32::new(0));
    let raw = Box::into_raw(Box::new(ReentrantDrop {
        com: ComObject::new(),
        drops: Arc::clone(&drops),
        nested_free: Arc::clone(&nested_free),
    }));
    // Internal hold taken before any public ref.
    // SAFETY: `raw` is a live allocation from `Box::into_raw`.
    unsafe { (*raw).com.add_ref_private() };
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.add_ref() }, 1);
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.private_count() }, 2);

    // Public 1 -> 0 drops the public hold's private ref, leaving the internal hold alive.
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.release() }, 0);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    // SAFETY: `raw` is still kept alive by the private hold.
    assert_eq!(unsafe { (*raw).com.private_count() }, 1);

    // Re-acquiring a public ref from 0 -> 1 adds a private ref again, and 1 -> 0 drops it.
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.add_ref() }, 1);
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.private_count() }, 2);
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.release_and_free() }, (0, false));

    // Releasing the private hold frees the object once.
    // SAFETY: `raw` is live.
    let free = unsafe { (*raw).com.release_private() };
    assert!(free);
    // SAFETY: the last private reference was just released.
    drop(unsafe { Box::from_raw(raw) });
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(nested_free.load(Ordering::SeqCst), 0);
}

#[test]
fn qi_log_fires_once_per_pair() {
    const OBJ: Guid = Guid::new(0x7000_0001, 0, 0, [0; 8]);
    const IID_A: Guid = Guid::new(0x7000_0002, 0, 0, [0; 8]);
    const IID_B: Guid = Guid::new(0x7000_0003, 0, 0, [0; 8]);
    assert!(qi_log(&OBJ, &IID_A));
    assert!(!qi_log(&OBJ, &IID_A));
    assert!(qi_log(&OBJ, &IID_B));
}

/// The log the `QueryInterface` tests write their miss lines to; it opens no file.
static QI_LOG: Logger = Logger::new("no_test_log/util.log");

const OBJECT: Guid = Guid::new(0x7100_0000, 0, 0, [0; 8]);
const PRIMARY: Guid = Guid::new(0x7100_0001, 0, 0, [0; 8]);
const TEAR_OFF: Guid = Guid::new(0x7100_0002, 0, 0, [0; 8]);
const SOMETIMES: Guid = Guid::new(0x7100_0003, 0, 0, [0; 8]);

/// An object whose `QueryInterface` is a converted body's.
///
/// It answers `PRIMARY` with itself, `TEAR_OFF` with `tear_off` and `SOMETIMES` while `sometimes`
/// holds, refusing it silently otherwise, with `miss` for the rest.
struct Obj {
    com: ComObject,
    tear_off: u64,
    sometimes: bool,
}

impl Obj {
    fn this(&self) -> *mut c_void {
        core::ptr::from_ref(self).cast_mut().cast()
    }

    /// `QueryInterface` as a converted body writes it.
    fn query(&self, riid: *const c_void, out: *mut *mut c_void, miss: QiMiss<'_>) -> i32 {
        let tear_off = core::ptr::from_ref(&self.tear_off).cast_mut().cast();
        let sometimes: &[Guid] = if self.sometimes { &[SOMETIMES] } else { &[] };
        let silent: &[Guid] = if self.sometimes { &[] } else { &[SOMETIMES] };
        let qi = Qi {
            null_riid: E_INVALIDARG,
            silent,
            log: &QI_LOG,
            miss,
        };
        // SAFETY: the tests pass null or live `riid` and `out`.
        unsafe {
            qi_table(
                riid,
                out,
                &[
                    (&[PRIMARY], self.this()),
                    (&[TEAR_OFF], tear_off),
                    (sometimes, self.this()),
                ],
                || {
                    self.com.add_ref();
                },
                &qi,
            )
        }
    }
}

fn obj(sometimes: bool) -> Obj {
    let obj = Obj {
        com: ComObject::new(),
        tear_off: 0,
        sometimes,
    };
    obj.com.add_ref();
    obj
}

fn once() -> QiMiss<'static> {
    QiMiss::Once {
        object: &OBJECT,
        name: "Obj",
    }
}

#[test]
fn qi_table_answers_entries_and_takes_one_reference_each() {
    let obj = obj(true);
    let mut out = core::ptr::null_mut();
    for (iid, expected) in [
        (PRIMARY, obj.this()),
        (
            TEAR_OFF,
            core::ptr::from_ref(&obj.tear_off).cast_mut().cast(),
        ),
        (SOMETIMES, obj.this()),
    ] {
        let before = obj.com.public_count();
        assert_eq!(
            obj.query((&raw const iid).cast(), &raw mut out, once()),
            S_OK
        );
        assert_eq!(out, expected);
        assert_eq!(obj.com.public_count(), before + 1);
    }
    // Each answer is released as the application would: back to the creator's reference.
    for _ in 0..3 {
        obj.com.release();
    }
    assert_eq!((obj.com.public_count(), obj.com.private_count()), (1, 1));
}

#[test]
fn qi_table_argument_errors() {
    let obj = obj(true);
    // A null `out` is `E_POINTER` whatever the IID.
    assert_eq!(
        obj.query(
            core::ptr::from_ref(&PRIMARY).cast(),
            core::ptr::null_mut(),
            once()
        ),
        E_POINTER
    );
    // A null `riid` is the object's own answer, with `*out` nulled.
    let mut out = core::ptr::without_provenance_mut::<c_void>(0xdead);
    assert_eq!(
        obj.query(core::ptr::null(), &raw mut out, once()),
        E_INVALIDARG
    );
    assert!(out.is_null());
    assert_eq!(obj.com.public_count(), 1);
}

#[test]
fn qi_table_takes_odd_addresses() {
    let obj = obj(true);
    let mut riid_bytes = Bytes([0; 24]);
    let riid = riid_bytes.odd::<Guid>();
    // SAFETY: `riid` addresses 16 of `riid_bytes`' bytes.
    unsafe { riid.write_unaligned(PRIMARY) };
    let mut out_bytes = Bytes([0; 24]);
    let out = out_bytes.odd::<*mut c_void>();
    assert_eq!(obj.query(riid.cast_const().cast(), out, once()), S_OK);
    // SAFETY: `out` addresses a pointer's worth of `out_bytes`, which `query` wrote.
    assert_eq!(unsafe { out.read_unaligned() }, obj.this());
}

#[test]
fn qi_table_misses_log_as_the_object_says() {
    const MISS_ONCE: Guid = Guid::new(0x7100_0010, 0, 0, [0; 8]);
    const MISS_EVERY: Guid = Guid::new(0x7100_0011, 0, 0, [0; 8]);
    const SPLIT: Guid = Guid::new(0x7100_0012, 0, 0, [0; 8]);
    let mut out = core::ptr::null_mut();

    // `Once` latches the pair, as `qi_log` does.
    let obj = obj(true);
    let riid = core::ptr::from_ref(&MISS_ONCE).cast();
    assert_eq!(obj.query(riid, &raw mut out, once()), E_NOINTERFACE);
    assert!(!qi_log(&OBJECT, &MISS_ONCE));

    // `Every`, `Quiet` and a silent IID leave the latch alone.
    let riid = core::ptr::from_ref(&MISS_EVERY).cast();
    assert_eq!(
        obj.query(riid, &raw mut out, QiMiss::Every("Obj")),
        E_NOINTERFACE
    );
    assert_eq!(obj.query(riid, &raw mut out, QiMiss::Quiet), E_NOINTERFACE);
    let off = self::obj(false);
    let riid = core::ptr::from_ref(&SOMETIMES).cast();
    assert_eq!(off.query(riid, &raw mut out, once()), E_NOINTERFACE);
    assert!(out.is_null());
    assert!(qi_log(&OBJECT, &SOMETIMES));
    assert_eq!(off.com.public_count(), 1);

    // `OnceSplit` latches too.
    let split = QiMiss::OnceSplit {
        object: &OBJECT,
        name: "Obj::QueryInterface",
    };
    assert_eq!(
        obj.query(core::ptr::from_ref(&SPLIT).cast(), &raw mut out, split),
        E_NOINTERFACE
    );
    assert!(!qi_log(&OBJECT, &SPLIT));
    assert!(qi_log(&OBJECT, &MISS_EVERY));
}

#[test]
fn qi_refuse_nulls_out() {
    // SAFETY: a null `out` is answered before anything is written.
    let hr = unsafe { qi_refuse(core::ptr::null_mut(), E_INVALIDARG) };
    assert_eq!(hr, E_POINTER);
    let mut out = core::ptr::without_provenance_mut::<c_void>(0xdead);
    // SAFETY: `out` is a local.
    let hr = unsafe { qi_refuse(&raw mut out, E_INVALIDARG) };
    assert_eq!(hr, E_INVALIDARG);
    assert!(out.is_null());
}
