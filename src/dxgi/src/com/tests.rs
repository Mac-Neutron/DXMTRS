use std::sync::{Arc, atomic::AtomicU32};

use super::*;

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
