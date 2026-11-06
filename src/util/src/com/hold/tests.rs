use core::{
    ptr::NonNull,
    sync::atomic::{AtomicU32, Ordering},
};
use std::sync::Arc;

use super::{
    ComHold, ComRefCounted, PrivateHold, add_ref_com, add_ref_private_com, release_com,
    release_private_com,
};
use crate::com::ComObject;

/// A boxed COM object that counts its drops.
struct Counted {
    com: ComObject,
    drops: Arc<AtomicU32>,
}

impl ComRefCounted for Counted {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

/// A new object with no reference, and its drop count.
fn counted() -> (*mut Counted, Arc<AtomicU32>) {
    let drops = Arc::new(AtomicU32::new(0));
    let raw = Box::into_raw(Box::new(Counted {
        com: ComObject::new(),
        drops: Arc::clone(&drops),
    }));
    (raw, drops)
}

/// The object's (public, private) counts.
fn counts(raw: *mut Counted) -> (u32, u32) {
    // SAFETY: the tests read the counts only while a reference keeps `raw` alive.
    let com = unsafe { &(*raw).com };
    (com.public_count(), com.private_count())
}

#[test]
fn add_ref_and_release_free_on_the_last_reference() {
    let (raw, drops) = counted();
    // SAFETY: `raw` is a live box.
    assert_eq!(unsafe { add_ref_com(raw) }, 1);
    // SAFETY: as above.
    assert_eq!(unsafe { add_ref_com(raw) }, 2);
    assert_eq!(counts(raw), (2, 1));
    // SAFETY: the test owns two public references.
    assert_eq!(unsafe { release_com(raw) }, 1);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    // SAFETY: as above, the last.
    assert_eq!(unsafe { release_com(raw) }, 0);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn private_reference_outlives_the_public_ones() {
    let (raw, drops) = counted();
    // SAFETY: `raw` is a live box.
    unsafe { add_ref_private_com(raw) };
    // SAFETY: as above.
    unsafe { add_ref_com(raw) };
    // SAFETY: the test's public reference.
    assert_eq!(unsafe { release_com(raw) }, 0);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(counts(raw), (0, 1));
    // SAFETY: the test's private reference, the last.
    unsafe { release_private_com(raw) };
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn holds_count_and_give_back() {
    let (raw, drops) = counted();
    let ptr = NonNull::new(raw).unwrap();
    // SAFETY: `raw` is a live box.
    let public = unsafe { ComHold::new(ptr) };
    // SAFETY: as above.
    let private = unsafe { PrivateHold::new(ptr) };
    assert_eq!(counts(raw), (1, 2));
    let second = public.clone();
    let private2 = private.clone();
    assert_eq!(counts(raw), (2, 3));
    assert_eq!(public.as_ptr(), raw);
    assert!(core::ptr::eq(private2.com(), second.com()));
    drop((public, second));
    assert_eq!(counts(raw), (0, 2));
    drop(private);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(private2);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
