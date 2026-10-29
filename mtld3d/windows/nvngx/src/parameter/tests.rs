use super::{OK, Parameters, ParametersVtbl};
use crate::ngx::FAIL;
use core::{ffi::c_void, mem::offset_of};

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

/// What each Get answers for one stored value: `None` where nvngx_parameter.hpp's castable refuses.
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
    let name = c"v".as_ptr();
    // SAFETY: `p` came from `Parameters::allocate` and lives until the test destroys it; each out pointer is a local.
    unsafe {
        let vt = &*(*p).vtbl;
        let mut ptr: *mut c_void = core::ptr::null_mut();
        let mut d3d12 = ptr;
        let mut d3d11 = ptr;
        let (mut f64v, mut u32v, mut i32v, mut f32v, mut u64v) = (0.0_f64, 0_u32, 0_i32, 0.0_f32, 0_u64);
        let ok = |r: u32| r == OK;
        Answers {
            ptr: ok((vt.get_ptr)(p, name, &raw mut ptr)).then_some(ptr as usize),
            d3d12: ok((vt.get_d3d12)(p, name, &raw mut d3d12)).then_some(d3d12 as usize),
            f64: ok((vt.get_f64)(p, name, &raw mut f64v)).then_some(f64v),
            u32: ok((vt.get_u32)(p, name, &raw mut u32v)).then_some(u32v),
            i32: ok((vt.get_i32)(p, name, &raw mut i32v)).then_some(i32v),
            d3d11: ok((vt.get_d3d11)(p, name, &raw mut d3d11)).then_some(d3d11 as usize),
            f32: ok((vt.get_f32)(p, name, &raw mut f32v)).then_some(f32v),
            u64: ok((vt.get_u64)(p, name, &raw mut u64v)).then_some(u64v),
        }
    }
}

fn with(set: impl FnOnce(&ParametersVtbl, *mut Parameters)) -> Answers {
    let p = Parameters::allocate();
    // SAFETY: as in `answers`.
    unsafe { set(&*(*p).vtbl, p) };
    let a = answers(p);
    Parameters::destroy(p);
    a
}

const ADDR: usize = 0x1000;

#[test]
fn a_pointer_reads_as_either_resource_or_u64() {
    let a = with(|vt, p| unsafe { (vt.set_ptr)(p, c"v".as_ptr(), ADDR as *mut c_void) });
    assert_eq!((a.ptr, a.d3d12, a.d3d11, a.u64), (Some(ADDR), Some(ADDR), Some(ADDR), Some(ADDR as u64)));
    assert_eq!((a.f64, a.u32, a.i32, a.f32), (None, None, None, None));
}

#[test]
fn a_resource_reads_as_itself_or_a_pointer() {
    let a = with(|vt, p| unsafe { (vt.set_d3d12)(p, c"v".as_ptr(), ADDR as *mut c_void) });
    assert_eq!((a.ptr, a.d3d12, a.d3d11, a.u64), (Some(ADDR), Some(ADDR), None, None));
    let a = with(|vt, p| unsafe { (vt.set_d3d11)(p, c"v".as_ptr(), ADDR as *mut c_void) });
    assert_eq!((a.ptr, a.d3d12, a.d3d11, a.u64), (Some(ADDR), None, Some(ADDR), None));
}

#[test]
fn integers_convert_as_castable_does() {
    let a = with(|vt, p| unsafe { (vt.set_i32)(p, c"v".as_ptr(), -1) });
    assert_eq!((a.i32, a.u32, a.f32, a.u64), (Some(-1), Some(u32::MAX), Some(-1.0), Some(u64::MAX)));
    assert_eq!((a.ptr, a.f64), (None, None));
    let a = with(|vt, p| unsafe { (vt.set_u32)(p, c"v".as_ptr(), 3_000_000_000) });
    assert_eq!((a.u32, a.i32, a.f32, a.u64), (Some(3_000_000_000), Some(-1_294_967_296), Some(3e9), Some(3_000_000_000)));
    assert_eq!((a.ptr, a.f64), (None, None));
    let a = with(|vt, p| unsafe { (vt.set_u64)(p, c"v".as_ptr(), 0x1_0000_0005) });
    assert_eq!((a.u64, a.i32, a.u32, a.ptr), (Some(0x1_0000_0005), Some(5), Some(5), Some(0x1_0000_0005)));
    assert_eq!((a.f32, a.f64, a.d3d12), (None, None, None));
}

#[test]
fn floats_convert_as_castable_does() {
    let a = with(|vt, p| unsafe { (vt.set_f32)(p, c"v".as_ptr(), 2.75) });
    assert_eq!((a.f32, a.f64, a.i32, a.u32), (Some(2.75), Some(2.75), Some(2), Some(2)));
    assert_eq!((a.u64, a.ptr), (None, None));
    let a = with(|vt, p| unsafe { (vt.set_f64)(p, c"v".as_ptr(), 0.5) });
    assert_eq!((a.f64, a.f32), (Some(0.5), Some(0.5)));
    assert_eq!((a.i32, a.u32, a.u64, a.ptr), (None, None, None, None));
}

#[test]
fn missing_and_reset() {
    let p = Parameters::allocate();
    // SAFETY: as in `answers`.
    unsafe {
        let vt = &*(*p).vtbl;
        let mut v = 0_u32;
        assert_eq!((vt.get_u32)(p, c"absent".as_ptr(), &raw mut v), FAIL);
        (vt.set_u32)(p, c"v".as_ptr(), 7);
        (vt.set_u32)(p, c"v".as_ptr(), 8);
        assert_eq!(((vt.get_u32)(p, c"v".as_ptr(), &raw mut v), v), (OK, 8));
        (vt.reset)(p);
        assert_eq!((vt.get_u32)(p, c"v".as_ptr(), &raw mut v), FAIL);
    }
    Parameters::destroy(p);
}
