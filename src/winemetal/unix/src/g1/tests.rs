use core::ffi::{CStr, c_void};

use objc2::{
    ffi::objc_autorelease,
    rc::{Retained, Weak, autoreleasepool},
    runtime::{AnyObject, NSObject},
};
use objc2_foundation::{NSArray, NSProcessInfo, NSString};
use winemetal_wire::{
    MemPtr, UnixcallDispatchdataCopybytes, UnixcallGenericObjNoret, UnixcallGenericObjObjNoret,
    UnixcallGenericObjObjRet, UnixcallGenericObjRet, UnixcallGenericObjUint64ObjRet,
    UnixcallGenericObjUint64Ret, UnixcallGenericObjUint64Uint64Ret, UnixcallGetOsVersion,
    UnixcallMtldeviceSampletimestamps, UnixcallNsstringGetcstring, UnixcallNsstringString,
};

use super::*;
use crate::{ffi::Handler, float_cast::u64_to_usize};

/// What a handler that wrote nothing leaves in a field.
const FILL: u64 = 0xA5A5_A5A5_A5A5_A5A5;

/// The status-checked call of a handler.
fn call<T>(handler: Handler, p: &mut T) {
    // SAFETY: `p` is the params block of the slot `handler` serves.
    let status = unsafe { handler(core::ptr::from_mut(p).cast::<c_void>()) };
    assert_eq!(status, STATUS_SUCCESS);
}

/// An object's address as a handle.
fn handle<T>(o: &T) -> u64 {
    core::ptr::from_ref(o).expose_provenance() as u64
}

fn retain_count(o: &NSObject) -> usize {
    use objc2::runtime::NSObjectProtocol;
    o.retainCount()
}

/// The bytes slot 8 writes for `string`, with `max` bytes of room in a buffer filled with 0xA5.
fn get_cstring(string: u64, max: u64, encoding: u64) -> (u32, [u8; 64]) {
    let mut buffer = [0xA5_u8; 64];
    let mut p = UnixcallNsstringGetcstring {
        str: string,
        buffer_ptr: buffer.as_mut_ptr().expose_provenance() as u64,
        max_length: max,
        encoding,
        ret: 0xA5A5_A5A5,
    };
    call(nsstring_get_cstring, &mut p);
    (p.ret, buffer)
}

const UTF8: u64 = 4;

#[test]
fn retain_and_release_move_the_count_by_one() {
    let o = NSObject::new();
    let mut p = UnixcallGenericObjNoret {
        handle: handle(&*o),
    };
    assert_eq!(retain_count(&o), 1);
    call(nsobject_retain, &mut p);
    assert_eq!(retain_count(&o), 2);
    call(nsobject_release, &mut p);
    assert_eq!(retain_count(&o), 1);
    let mut nil = UnixcallGenericObjNoret { handle: 0 };
    call(nsobject_retain, &mut nil);
    call(nsobject_release, &mut nil);
}

#[test]
fn a_pool_made_by_slot_10_drains_on_release() {
    let mut pool = UnixcallGenericObjRet { ret: FILL };
    call(nsautoreleasepool_alloc_init, &mut pool);
    assert_ne!(pool.ret, 0);
    assert_ne!(pool.ret, FILL);
    let object = NSObject::new();
    let weak = Weak::from_retained(&object);
    // SAFETY: the object's one reference moves to the pool, which now owns its release.
    unsafe { objc_autorelease(Retained::into_raw(object).cast::<AnyObject>()) };
    assert!(weak.load().is_some());
    let mut drain = UnixcallGenericObjNoret { handle: pool.ret };
    call(nsobject_release, &mut drain);
    assert!(weak.load().is_none());
}

#[test]
fn array_elements_are_plus_zero_and_counted() {
    let items = [NSObject::new(), NSObject::new(), NSObject::new()];
    let array = NSArray::from_retained_slice(&items);
    let mut count = UnixcallGenericObjUint64Ret {
        handle: handle(&*array),
        ret: FILL,
    };
    call(nsarray_count, &mut count);
    assert_eq!(count.ret, 3);
    let before = retain_count(&items[1]);
    let mut at = UnixcallGenericObjUint64ObjRet {
        handle: handle(&*array),
        arg: 1,
        ret: FILL,
    };
    call(nsarray_object, &mut at);
    assert_eq!(at.ret, handle(&*items[1]));
    assert_eq!(retain_count(&items[1]), before);
}

#[test]
fn nil_receivers_write_the_zeros_a_send_to_nil_answers() {
    let mut obj_ret = UnixcallGenericObjUint64ObjRet {
        handle: 0,
        arg: 0,
        ret: FILL,
    };
    call(nsarray_object, &mut obj_ret);
    assert_eq!(obj_ret.ret, 0);
    let mut sized = UnixcallGenericObjUint64Ret {
        handle: 0,
        ret: FILL,
    };
    call(mtldevice_recommended_max_working_set_size, &mut sized);
    assert_eq!(sized.ret, 0);
    let mut name = UnixcallGenericObjObjRet {
        handle: 0,
        ret: FILL,
    };
    call(mtldevice_name, &mut name);
    assert_eq!(name.ret, 0);
    let (ret, buffer) = get_cstring(0, 64, UTF8);
    assert_eq!(ret, 0);
    assert_eq!(buffer, [0xA5; 64]);
    let mut stamps = UnixcallMtldeviceSampletimestamps {
        device: 0,
        cpu: FILL,
        gpu: FILL,
    };
    call(mtldevice_sample_timestamps, &mut stamps);
    assert_eq!((stamps.cpu, stamps.gpu), (0, 0));
    let mut label = UnixcallGenericObjObjNoret { handle: 0, arg: 0 };
    call(mtlcommandencoder_set_label, &mut label);
}

#[test]
fn strings_round_trip_and_get_cstring_leaves_the_tail() {
    autoreleasepool(|_| {
        let text = c"h\xc3\xa9llo w\xc3\xb6rld \xe2\x9c\x93 and some more";
        let bytes = text.to_bytes().len();
        let mut made = UnixcallNsstringString {
            buffer_ptr: MemPtr {
                ptr: text.as_ptr().expose_provenance() as u64,
            },
            encoding: UTF8,
            ret: FILL,
        };
        call(nsstring_string, &mut made);
        let mut owned = made;
        owned.ret = FILL;
        call(nsstring_alloc_init, &mut owned);
        for string in [made.ret, owned.ret] {
            assert_ne!(string, 0);
            let mut len = UnixcallGenericObjUint64Uint64Ret {
                handle: string,
                arg: UTF8,
                ret: FILL,
            };
            call(nsstring_length_of_bytes, &mut len);
            assert_eq!(len.ret, bytes as u64);
            let (ret, buffer) = get_cstring(string, bytes as u64 + 1, UTF8);
            assert_eq!(ret, 1);
            assert_eq!(CStr::from_bytes_until_nul(&buffer), Ok(text));
            assert_eq!(buffer[bytes + 1], 0xA5);
            let (ret, buffer) = get_cstring(string, bytes as u64, UTF8);
            assert_eq!(ret, 0);
            assert_eq!(buffer[bytes + 1], 0xA5);
        }
        let object = core::ptr::with_exposed_provenance_mut(u64_to_usize(owned.ret));
        // SAFETY: slot 61's +1 reference, ours to give back.
        unsafe { objc2::ffi::objc_release(object) };
    });
}

#[test]
fn bytes_the_encoding_rejects_answer_nil() {
    autoreleasepool(|_| {
        let text = c"bytes \xff\xfe that are not utf-8";
        for slot in [nsstring_string as Handler, nsstring_alloc_init] {
            let mut p = UnixcallNsstringString {
                buffer_ptr: MemPtr {
                    ptr: text.as_ptr().expose_provenance() as u64,
                },
                encoding: UTF8,
                ret: FILL,
            };
            call(slot, &mut p);
            assert_eq!(p.ret, 0);
        }
    });
}

/// `copy_bytes` of slot 149 over `data` with `capacity`: the size it reports and the buffer.
fn copy_out(data: u64, capacity: u64) -> (u64, [u8; 64]) {
    let mut out = [0xA5_u8; 64];
    let mut p = UnixcallDispatchdataCopybytes {
        data,
        buffer: MemPtr {
            ptr: out.as_mut_ptr().expose_provenance() as u64,
        },
        capacity,
        ret_size: FILL,
    };
    call(dispatchdata_copy_bytes, &mut p);
    (p.ret_size, out)
}

#[test]
fn dispatch_data_is_a_copy_cut_at_the_capacity() {
    let mut source: Vec<u8> = (0..40_u8)
        .map(|i| i.wrapping_mul(7).wrapping_add(1))
        .collect();
    let want = source.clone();
    let mut made = UnixcallGenericObjUint64ObjRet {
        handle: source.as_ptr().expose_provenance() as u64,
        arg: 40,
        ret: FILL,
    };
    call(dispatchdata_alloc_init, &mut made);
    source.fill(0xEE);
    for capacity in [40, 64, 16, 1, 0] {
        let (size, out) = copy_out(made.ret, capacity);
        let written = want.len().min(u64_to_usize(capacity));
        assert_eq!(size, 40);
        assert_eq!(out[..written], want[..written]);
        assert!(
            out[written..].iter().all(|&b| b == 0xA5),
            "capacity {capacity}"
        );
    }
    let mut release = UnixcallGenericObjNoret { handle: made.ret };
    call(nsobject_release, &mut release);
}

#[test]
fn dispatch_data_regions_stop_at_the_capacity() {
    let first = DispatchData::from_bytes(&[1; 16]);
    let second = DispatchData::from_bytes(&[2; 24]);
    let both = first.concat(&second);
    let both = DispatchRetained::as_ptr(&both).as_ptr().expose_provenance() as u64;
    let cases = [
        (40, 40),
        (41, 40),
        (39, 39),
        (16, 16),
        (17, 17),
        (15, 15),
        (0, 0),
    ];
    for (capacity, written) in cases {
        let (size, out) = copy_out(both, capacity);
        assert_eq!(size, 40);
        let ones = written.min(16);
        assert!(out[..ones].iter().all(|&b| b == 1), "capacity {capacity}");
        assert!(
            out[ones..written].iter().all(|&b| b == 2),
            "capacity {capacity}"
        );
        assert!(
            out[written..].iter().all(|&b| b == 0xA5),
            "capacity {capacity}"
        );
    }
}

#[test]
fn dispatch_data_without_bytes_is_the_empty_data() {
    for (bytes, size) in [(0, 0), (0, 9), (1, 0)] {
        let mut made = UnixcallGenericObjUint64ObjRet {
            handle: bytes,
            arg: size,
            ret: FILL,
        };
        call(dispatchdata_alloc_init, &mut made);
        assert_eq!(made.ret, handle(DispatchData::empty()));
        let (size, out) = copy_out(made.ret, 64);
        assert_eq!(size, 0);
        assert_eq!(out, [0xA5; 64]);
    }
    let (size, out) = copy_out(0, 64);
    assert_eq!(size, FILL, "a nil data writes nothing");
    assert_eq!(out, [0xA5; 64]);
}

#[test]
fn the_os_version_is_process_infos() {
    let want = NSProcessInfo::processInfo().operatingSystemVersion();
    let mut p = UnixcallGetOsVersion {
        ret_major: FILL,
        ret_minor: FILL,
        ret_patch: FILL,
    };
    call(wmt_get_os_version, &mut p);
    assert_eq!(p.ret_major, want.majorVersion.cast_unsigned() as u64);
    assert_eq!(p.ret_minor, want.minorVersion.cast_unsigned() as u64);
    assert_eq!(p.ret_patch, want.patchVersion.cast_unsigned() as u64);
}

#[test]
fn a_label_is_set_and_a_zero_label_removes_it() {
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice, MTLResource, MTLResourceOptions};
    let Some(device) = MTLCreateSystemDefaultDevice() else {
        return;
    };
    let buffer = device
        .newBufferWithLength_options(64, MTLResourceOptions::StorageModeShared)
        .expect("a 64-byte buffer");
    let label = NSString::from_str("a label with spaces");
    let mut set = UnixcallGenericObjObjNoret {
        handle: handle(&*buffer),
        arg: handle(&*label),
    };
    call(mtlcommandencoder_set_label, &mut set);
    assert_eq!(
        buffer.label().map(|l| l.to_string()).as_deref(),
        Some("a label with spaces")
    );
    let mut clear = UnixcallGenericObjObjNoret {
        handle: handle(&*buffer),
        arg: 0,
    };
    call(mtlcommandencoder_set_label, &mut clear);
    assert!(buffer.label().is_none());
    let mut count = UnixcallGenericObjUint64Uint64Ret {
        handle: handle(&*device),
        arg: 4,
        ret: FILL,
    };
    call(mtldevice_supports_texture_sample_count, &mut count);
    assert_eq!(count.ret, u64::from(device.supportsTextureSampleCount(4)));
}
