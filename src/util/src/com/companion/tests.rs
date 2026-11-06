use core::{
    ffi::c_void,
    mem::offset_of,
    sync::atomic::{AtomicU32, Ordering},
};

use super::{
    TearOff, companion_add_ref, companion_release, from_companion, iface_of, to_companion,
};
use crate::com::{ComObject, Guid, IUnknownVtbl, S_OK, hold::ComRefCounted, write_out};

/// An object with a companion vtable field and a tear-off, each a second interface.
#[repr(C)]
struct Outer {
    vtbl: *const IUnknownVtbl,
    com: ComObject,
    ext_vtbl: *const u8,
    tear_off: TearOff<u8>,
}

const EXT: usize = offset_of!(Outer, ext_vtbl);

impl ComRefCounted for Outer {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

/// The outer object behind an `Outer` interface pointer.
///
/// # Safety
///
/// `this` is a live `Outer`.
const unsafe fn outer<'a>(this: *mut c_void) -> &'a Outer {
    // SAFETY: the caller's.
    unsafe { &*this.cast::<Outer>() }
}

/// How many `QueryInterface` calls reached the outer object.
static OUTER_QI: AtomicU32 = AtomicU32::new(0);

extern "system" fn outer_qi(this: *mut c_void, _: *const Guid, out: *mut *mut c_void) -> i32 {
    OUTER_QI.fetch_add(1, Ordering::SeqCst);
    // SAFETY: the slot's `this` is a live `Outer`.
    unsafe { outer(this) }.com.add_ref();
    // SAFETY: the caller's local.
    unsafe { write_out(out, this) };
    S_OK
}

extern "system" fn outer_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live `Outer`.
    unsafe { outer(this) }.com.add_ref()
}

extern "system" fn outer_release(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live `Outer`.
    unsafe { outer(this) }.com.release()
}

static OUTER_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: outer_qi,
    add_ref: outer_add_ref,
    release: outer_release,
};

static EXT_VTBL: u8 = 0;

/// A boxed `Outer` with the creator's public reference, its tear-off pointing back at it.
fn new_outer() -> *mut Outer {
    let raw = Box::into_raw(Box::new(Outer {
        vtbl: &raw const OUTER_VTBL,
        com: ComObject::new(),
        ext_vtbl: &raw const EXT_VTBL,
        tear_off: TearOff::new(&EXT_VTBL, core::ptr::null_mut()),
    }));
    // SAFETY: `raw` is the live box just made.
    let obj = unsafe { &mut *raw };
    obj.tear_off.outer = raw.cast();
    obj.com.add_ref();
    raw
}

#[test]
fn companion_pointers_round_trip_and_count_on_the_object() {
    let raw = new_outer();
    // SAFETY: `EXT` is the companion field's offset in the live `raw`.
    let ext = unsafe { to_companion(raw, EXT) };
    // SAFETY: as above.
    let ext_field = unsafe { &(*raw).ext_vtbl };
    assert_eq!(ext, iface_of(ext_field));
    // SAFETY: `ext` is the companion field of the live `raw`.
    assert_eq!(unsafe { from_companion::<Outer>(ext, EXT) }, raw);
    assert_eq!(crate::container_of!(ext, Outer, ext_vtbl), raw);

    // SAFETY: `ext` is the companion of a live boxed `Outer`.
    assert_eq!(unsafe { companion_add_ref::<Outer, EXT>(ext) }, 2);
    // SAFETY: the reference just taken.
    assert_eq!(unsafe { companion_release::<Outer, EXT>(ext) }, 1);
    // SAFETY: the creator's reference, the last: the box is freed.
    assert_eq!(unsafe { companion_release::<Outer, EXT>(ext) }, 0);
}

#[test]
fn tear_off_forwards_iunknown_to_the_outer_count() {
    let raw = new_outer();
    // SAFETY: `raw` is live while the creator's reference is held.
    let tear_off = unsafe { (*raw).tear_off.as_iface() };
    // SAFETY: `tear_off` is a live `TearOff` whose outer is `raw`.
    assert_eq!(unsafe { TearOff::<()>::add_ref(tear_off) }, 2);
    let mut out = core::ptr::null_mut();
    let before = OUTER_QI.load(Ordering::SeqCst);
    // SAFETY: as above; `out` is a local.
    let hr = unsafe { TearOff::<()>::query_interface(tear_off, core::ptr::null(), &raw mut out) };
    assert_eq!((hr, out), (S_OK, raw.cast()));
    assert!(OUTER_QI.load(Ordering::SeqCst) > before);
    // SAFETY: as above.
    let com = unsafe { &(*raw).com };
    assert_eq!((com.public_count(), com.private_count()), (3, 1));
    // SAFETY: the two references taken through the tear-off.
    assert_eq!(unsafe { TearOff::<()>::release(tear_off) }, 2);
    // SAFETY: as above.
    assert_eq!(unsafe { TearOff::<()>::release(tear_off) }, 1);
    // SAFETY: the creator's reference: the outer count reaches 0 and frees its private one.
    let (rem, free) = unsafe { (*raw).com.release_and_free() };
    assert_eq!((rem, free), (0, true));
    // SAFETY: the last private reference is gone.
    drop(unsafe { Box::from_raw(raw) });
}
