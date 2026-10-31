use core::cell::Cell;

use super::{Obj, Pool};

/// The handle the fake `NSAutoreleasePool_alloc_init` returns.
const POOL: u64 = 0x5001;

thread_local! {
    /// The handles the fakes were given, in call order: retains, releases and pools pushed.
    static RETAINED: Cell<[u64; 4]> = const { Cell::new([0; 4]) };
    static RELEASED: Cell<[u64; 4]> = const { Cell::new([0; 4]) };
    static POOLS: Cell<u32> = const { Cell::new(0) };
}

/// Appends `handle` to a log of the calling test's thread (each test runs on its own).
fn log(calls: &'static std::thread::LocalKey<Cell<[u64; 4]>>, handle: u64) {
    let mut seen = calls.get();
    if let Some(slot) = seen.iter_mut().find(|slot| **slot == 0) {
        *slot = handle;
    }
    calls.set(seen);
}

#[unsafe(export_name = "NSObject_retain")]
extern "C-unwind" fn fake_retain(object: u64) {
    log(&RETAINED, object);
}

#[unsafe(export_name = "NSObject_release")]
extern "C-unwind" fn fake_release(object: u64) {
    log(&RELEASED, object);
}

#[unsafe(export_name = "NSAutoreleasePool_alloc_init")]
extern "C-unwind" fn fake_pool() -> u64 {
    POOLS.set(POOLS.get() + 1);
    POOL
}

#[test]
fn adopt_releases_once() {
    // SAFETY: the fakes accept any handle; this one is given up to the `Obj`.
    let object = unsafe { Obj::adopt(7) }.expect("a non-null handle");
    assert_eq!(object.handle(), 7);
    assert_eq!((RETAINED.get(), RELEASED.get()), ([0; 4], [0; 4]));
    drop(object);
    assert_eq!((RETAINED.get(), RELEASED.get()), ([0; 4], [7, 0, 0, 0]));
}

#[test]
fn retain_retains_then_releases() {
    // SAFETY: the fakes accept any handle.
    let object = unsafe { Obj::retain(9) }.expect("a non-null handle");
    assert_eq!(object.handle(), 9);
    assert_eq!((RETAINED.get(), RELEASED.get()), ([9, 0, 0, 0], [0; 4]));
    drop(object);
    assert_eq!(
        (RETAINED.get(), RELEASED.get()),
        ([9, 0, 0, 0], [9, 0, 0, 0])
    );
}

#[test]
fn null_is_none() {
    // SAFETY: the null handle is never retained or released.
    assert!(unsafe { Obj::adopt(0) }.is_none());
    // SAFETY: as above.
    assert!(unsafe { Obj::retain(0) }.is_none());
    assert_eq!((RETAINED.get(), RELEASED.get()), ([0; 4], [0; 4]));
}

#[test]
fn pool_drains() {
    let pool = Pool::new();
    assert_eq!((POOLS.get(), RELEASED.get()), (1, [0; 4]));
    drop(pool);
    assert_eq!((POOLS.get(), RELEASED.get()), (1, [POOL, 0, 0, 0]));
}
