use std::sync::{Arc, atomic::AtomicU32};

use super::*;
use crate::abi::{E_NOINTERFACE, E_POINTER, S_OK};

/// A test object: its `Head`, then a count of its drops.
#[repr(C)]
struct Probe {
    head: Head<()>,
    drops: Arc<AtomicU32>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}

/// A boxed `Probe` with one reference, as a COM `this`.
fn probe(drops: &Arc<AtomicU32>) -> *mut c_void {
    Box::into_raw(Box::new(Probe {
        head: Head::new(&()),
        drops: Arc::clone(drops),
    }))
    .cast()
}

/// `Probe`'s `AddRef` slot.
extern "system" fn probe_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live `Probe`.
    unsafe { add_ref::<Probe>(this) }
}

#[test]
fn starts_at_one() {
    let drops = Arc::default();
    let this = probe(&drops);
    // SAFETY: `this` is a live `Probe`.
    assert_eq!(unsafe { add_ref::<Probe>(this) }, 2);
    // SAFETY: as above.
    assert_eq!(unsafe { release::<Probe>(this) }, 1);
    // SAFETY: as above, from `Box::into_raw`.
    assert_eq!(unsafe { release::<Probe>(this) }, 0);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn release_frees_once_on_the_last() {
    let drops = Arc::default();
    let this = probe(&drops);
    for _ in 0..2 {
        // SAFETY: `this` is a live `Probe`.
        unsafe { add_ref::<Probe>(this) };
    }
    for left in [2, 1] {
        // SAFETY: as above.
        assert_eq!(unsafe { release::<Probe>(this) }, left);
        assert_eq!(drops.load(Ordering::Relaxed), 0);
    }
    // SAFETY: as above, from `Box::into_raw`.
    assert_eq!(unsafe { release::<Probe>(this) }, 0);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn release_past_zero_answers_zero() {
    let drops = Arc::default();
    let this = probe(&drops);
    // SAFETY: `this` is a live `Probe`.
    unsafe { refs::<Probe>(this) }.store(0, Ordering::Relaxed);
    // SAFETY: as above.
    assert_eq!(unsafe { release::<Probe>(this) }, 0);
    // SAFETY: as above.
    assert_eq!(unsafe { refs::<Probe>(this) }.load(Ordering::Relaxed), 0);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    // SAFETY: `this` came from `Box::into_raw` and nothing else holds it.
    drop(unsafe { Box::from_raw(this.cast::<Probe>()) });
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn eight_threads_balance() {
    let drops = Arc::default();
    let address = probe(&drops).expose_provenance();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                let this = core::ptr::with_exposed_provenance_mut::<c_void>(address);
                for _ in 0..10_000 {
                    // SAFETY: `this` is a live `Probe`: the test holds its first reference.
                    unsafe { add_ref::<Probe>(this) };
                    // SAFETY: as above; this gives back the reference just taken.
                    unsafe { release::<Probe>(this) };
                }
            });
        }
    });
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    let this = core::ptr::with_exposed_provenance_mut::<c_void>(address);
    // SAFETY: the test's reference to a live `Probe`, from `Box::into_raw`.
    assert_eq!(unsafe { release::<Probe>(this) }, 0);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}

#[test]
fn query_interface_answers_this_and_nulls_the_rest() {
    const ANSWERED: Guid = Guid::new(0x5e1f_0001, 0, 0, [0; 8]);
    const SILENT: Guid = Guid::new(0x5e1f_0002, 0, 0, [0; 8]);
    const OTHER: Guid = Guid::new(0x5e1f_0003, 0, 0, [0; 8]);
    // Logged already, so the miss below writes no log file beside the test.
    assert!(qi_log(&ANSWERED, &OTHER));
    let drops = Arc::default();
    let this = probe(&drops);
    let ask = |riid: *const Guid, out: *mut *mut c_void| {
        let log_as = (&ANSWERED, "Probe");
        // SAFETY: `this` is a live `Probe`; `riid` and `out` are null or the test's locals.
        unsafe {
            query_interface(
                this,
                riid,
                out,
                &[ANSWERED],
                &[SILENT],
                log_as,
                probe_add_ref,
            )
        }
    };
    assert_eq!(
        ask(core::ptr::from_ref(&ANSWERED), core::ptr::null_mut()),
        E_POINTER
    );
    let mut out = this;
    assert_eq!(ask(core::ptr::null(), &raw mut out), E_POINTER);
    assert!(out.is_null());
    assert_eq!(ask(core::ptr::from_ref(&ANSWERED), &raw mut out), S_OK);
    assert_eq!(out, this);
    for riid in [SILENT, OTHER] {
        assert_eq!(ask(&raw const riid, &raw mut out), E_NOINTERFACE);
        assert!(out.is_null());
    }
    // SAFETY: `this` is a live `Probe`: the reference QueryInterface took.
    assert_eq!(unsafe { release::<Probe>(this) }, 1);
    // SAFETY: as above, the last one, from `Box::into_raw`.
    assert_eq!(unsafe { release::<Probe>(this) }, 0);
}

#[test]
fn a_miss_is_logged_once_per_pair() {
    const OBJECT: Guid = Guid::new(0x5e1f_0010, 0, 0, [0; 8]);
    const ASKED: Guid = Guid::new(0x5e1f_0011, 0, 0, [0; 8]);
    const ELSE: Guid = Guid::new(0x5e1f_0012, 0, 0, [0; 8]);
    assert!(qi_log(&OBJECT, &ASKED));
    assert!(!qi_log(&OBJECT, &ASKED));
    assert!(qi_log(&OBJECT, &ELSE));
    assert!(qi_log(&ELSE, &ASKED));
}
