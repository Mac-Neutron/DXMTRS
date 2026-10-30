use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2_foundation::NSArray;
use winemetal_wire::{MemPtr, UnixcallEnumerate, UnixcallGenericObjUint64Uint64Ret};

use super::*;

const FILL: u64 = 0xA5A5_A5A5_A5A5_A5A5;

fn handle<T>(p: *const T) -> u64 {
    p.expose_provenance() as u64
}

/// Slot 91 over `array` from `start`, `size` items, into a filled buffer of 4 (or none).
fn enumerate(array: &NSArray<NSObject>, start: u64, size: u64, buffer: bool) -> (u64, [u64; 4]) {
    let mut out = [FILL; 4];
    let mut p = UnixcallEnumerate {
        enumerable: handle(core::ptr::from_ref(array)),
        start,
        buffer_size: size,
        buffer: MemPtr { ptr: if buffer { handle(out.as_mut_ptr()) } else { 0 } },
        ret_read: FILL,
    };
    // SAFETY: a params block of slot 91's, over a live array and a buffer of 4.
    let status = unsafe { log_container_enumerate(core::ptr::from_mut(&mut p).cast()) };
    assert_eq!(status, STATUS_SUCCESS);
    (p.ret_read, out)
}

fn array_of(n: usize) -> (Retained<NSArray<NSObject>>, Vec<u64>) {
    let items: Vec<_> = (0..n).map(|_| NSObject::new()).collect();
    let handles = items.iter().map(|o| handle(Retained::as_ptr(o))).collect();
    (NSArray::from_retained_slice(&items), handles)
}

#[test]
fn enumerate_reads_the_window() {
    let (array, items) = array_of(5);
    assert_eq!(enumerate(&array, 1, 2, true), (2, [items[1], items[2], FILL, FILL]));
    assert_eq!(enumerate(&array, 4, 2, true), (1, [items[4], FILL, FILL, FILL]));
    assert_eq!(enumerate(&array, 5, 2, true), (0, [FILL; 4]));
}

#[test]
fn enumerate_window_end_wraps_as_the_c() {
    let (array, _) = array_of(5);
    // `start + buffer_size` wraps to 2: item 3 is past the window's end, so none is read.
    assert_eq!(enumerate(&array, 3, u64::MAX, true), (0, [FILL; 4]));
}

#[test]
fn enumerate_null_buffer_writes_nothing() {
    let (array, _) = array_of(5);
    assert_eq!(enumerate(&array, 0, 2, false), (2, [FILL; 4]));
}

#[test]
fn property_of_nil_or_unknown_is_zero() {
    for arg in [0, 1, 2, 3, 99, u64::MAX] {
        let mut p = UnixcallGenericObjUint64Uint64Ret { handle: 0, arg, ret: FILL };
        // SAFETY: a params block of slot 130's, with the null handle.
        unsafe { command_buffer_property(core::ptr::from_mut(&mut p).cast()) };
        assert_eq!(p.ret, 0, "arg {arg}");
    }
}
