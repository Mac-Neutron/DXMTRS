use super::{OK, Parameters, ParametersVtbl};
use crate::ngx::FAIL;
use core::{
    ffi::{c_char, c_void},
    mem::offset_of,
};

const SLOT: usize = size_of::<usize>();

#[test]
fn slots_in_nvngx_hpp_order() {
    assert_eq!(size_of::<ParametersVtbl>(), 17 * SLOT);
    let slots = [
        offset_of!(ParametersVtbl, set_ptr),
        offset_of!(ParametersVtbl, set_d3d12),
        offset_of!(ParametersVtbl, set_d3d11),
        offset_of!(ParametersVtbl, set_i32),
        offset_of!(ParametersVtbl, set_u32),
        offset_of!(ParametersVtbl, set_f64),
        offset_of!(ParametersVtbl, set_f32),
        offset_of!(ParametersVtbl, set_u64),
        offset_of!(ParametersVtbl, get_ptr),
        offset_of!(ParametersVtbl, get_d3d12),
        offset_of!(ParametersVtbl, get_f64),
        offset_of!(ParametersVtbl, get_u32),
        offset_of!(ParametersVtbl, get_i32),
        offset_of!(ParametersVtbl, get_d3d11),
        offset_of!(ParametersVtbl, get_f32),
        offset_of!(ParametersVtbl, get_u64),
        offset_of!(ParametersVtbl, reset),
    ];
    for (i, offset) in slots.into_iter().enumerate() {
        assert_eq!(offset, i * SLOT);
    }
}

/// The vtable of a map from `Parameters::allocate`, as a game reaches it.
fn table<'a>(p: *mut Parameters) -> &'a ParametersVtbl {
    // SAFETY: `p` came from `Parameters::allocate` and lives until the test destroys it.
    let map = unsafe { &*p };
    // SAFETY: the map's vtable pointer is the static vtable's.
    unsafe { &*map.vtbl }
}

type Get<T> = unsafe extern "system" fn(*const Parameters, *const c_char, *mut T) -> u32;

/// One getter's answer for `name`: `None` when it fails.
fn get<T>(p: *mut Parameters, name: &core::ffi::CStr, getter: Get<T>, init: T) -> Option<T> {
    let mut value = init;
    // SAFETY: a live map, a C string, and a local the getter writes.
    (unsafe { getter(p, name.as_ptr(), &raw mut value) } == OK).then_some(value)
}

/// What each getter answers for one stored value: `None` where `nvngx_parameter.hpp` refuses.
struct Answers {
    ptr: Option<usize>,
    d3d12: Option<usize>,
    f64: Option<f64>,
    u32: Option<u32>,
    i32: Option<i32>,
    d3d11: Option<usize>,
    f32: Option<f32>,
    u64: Option<u64>,
}

fn answers(p: *mut Parameters) -> Answers {
    let vt = table(p);
    let null = core::ptr::null_mut::<c_void>();
    Answers {
        ptr: get(p, c"v", vt.get_ptr, null).map(<*mut c_void>::addr),
        d3d12: get(p, c"v", vt.get_d3d12, null).map(<*mut c_void>::addr),
        f64: get(p, c"v", vt.get_f64, 0.0),
        u32: get(p, c"v", vt.get_u32, 0),
        i32: get(p, c"v", vt.get_i32, 0),
        d3d11: get(p, c"v", vt.get_d3d11, null).map(<*mut c_void>::addr),
        f32: get(p, c"v", vt.get_f32, 0.0),
        u64: get(p, c"v", vt.get_u64, 0),
    }
}

/// The answers for the value `set` stores under "v".
fn with(set: impl FnOnce(&ParametersVtbl, *mut Parameters)) -> Answers {
    let p = Parameters::allocate();
    set(table(p), p);
    let a = answers(p);
    Parameters::destroy(p);
    a
}

const ADDR: usize = 0x1000;

fn addr() -> *mut c_void {
    core::ptr::without_provenance_mut(ADDR)
}

#[test]
fn a_pointer_reads_as_either_resource_or_u64() {
    // SAFETY: a live map and a C string.
    let a = with(|vt, p| unsafe { (vt.set_ptr)(p, c"v".as_ptr(), addr()) });
    assert_eq!((a.ptr, a.d3d12, a.d3d11, a.u64), (Some(ADDR), Some(ADDR), Some(ADDR), Some(0x1000)));
    assert_eq!((a.f64, a.u32, a.i32, a.f32), (None, None, None, None));
}

#[test]
fn a_resource_reads_as_itself_or_a_pointer() {
    // SAFETY: a live map and a C string.
    let a = with(|vt, p| unsafe { (vt.set_d3d12)(p, c"v".as_ptr(), addr()) });
    assert_eq!((a.ptr, a.d3d12, a.d3d11, a.u64), (Some(ADDR), Some(ADDR), None, None));
    // SAFETY: a live map and a C string.
    let a = with(|vt, p| unsafe { (vt.set_d3d11)(p, c"v".as_ptr(), addr()) });
    assert_eq!((a.ptr, a.d3d12, a.d3d11, a.u64), (Some(ADDR), None, Some(ADDR), None));
}

#[test]
fn integers_convert_as_castable_does() {
    // SAFETY: a live map and a C string.
    let a = with(|vt, p| unsafe { (vt.set_i32)(p, c"v".as_ptr(), -1) });
    assert_eq!((a.i32, a.u32, a.u64), (Some(-1), Some(u32::MAX), Some(u64::MAX)));
    assert_eq!(a.f32.map(f32::to_bits), Some((-1.0_f32).to_bits()));
    assert_eq!((a.ptr, a.f64), (None, None));
    // SAFETY: a live map and a C string.
    let a = with(|vt, p| unsafe { (vt.set_u32)(p, c"v".as_ptr(), 3_000_000_000) });
    assert_eq!((a.u32, a.i32, a.u64), (Some(3_000_000_000), Some(-1_294_967_296), Some(3_000_000_000)));
    assert_eq!(a.f32.map(f32::to_bits), Some(3e9_f32.to_bits()));
    assert_eq!((a.ptr, a.f64), (None, None));
    // SAFETY: a live map and a C string.
    let a = with(|vt, p| unsafe { (vt.set_u64)(p, c"v".as_ptr(), 0x1_0000_0005) });
    assert_eq!((a.u64, a.i32, a.u32, a.ptr), (Some(0x1_0000_0005), Some(5), Some(5), Some(0x1_0000_0005)));
    assert_eq!((a.f32, a.f64, a.d3d12), (None, None, None));
}

#[test]
fn floats_convert_as_castable_does() {
    // SAFETY: a live map and a C string.
    let a = with(|vt, p| unsafe { (vt.set_f32)(p, c"v".as_ptr(), 2.75) });
    assert_eq!((a.f32.map(f32::to_bits), a.f64.map(f64::to_bits)), (Some(2.75_f32.to_bits()), Some(2.75_f64.to_bits())));
    assert_eq!((a.i32, a.u32), (Some(2), Some(2)));
    assert_eq!((a.u64, a.ptr), (None, None));
    // SAFETY: a live map and a C string.
    let a = with(|vt, p| unsafe { (vt.set_f64)(p, c"v".as_ptr(), 0.5) });
    assert_eq!((a.f64.map(f64::to_bits), a.f32.map(f32::to_bits)), (Some(0.5_f64.to_bits()), Some(0.5_f32.to_bits())));
    assert_eq!((a.i32, a.u32, a.u64, a.ptr), (None, None, None, None));
}

#[test]
fn missing_and_reset() {
    let p = Parameters::allocate();
    let vt = table(p);
    assert_eq!(get(p, c"absent", vt.get_u32, 0), None);
    // SAFETY: a live map and a C string.
    unsafe { (vt.set_u32)(p, c"v".as_ptr(), 7) };
    // SAFETY: as above.
    unsafe { (vt.set_u32)(p, c"v".as_ptr(), 8) };
    assert_eq!(get(p, c"v", vt.get_u32, 0), Some(8));
    // SAFETY: a live map.
    unsafe { (vt.reset)(p) };
    assert_eq!(get(p, c"v", vt.get_u32, 0), None);
    Parameters::destroy(p);
    let _ = FAIL;
}
