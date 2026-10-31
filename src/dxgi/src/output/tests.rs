use super::*;
use crate::{
    abi::{DXGI_FORMAT_R8G8B8A8_UNORM, E_NOINTERFACE, E_POINTER, IID_IDXGI_FACTORY},
    com::qi_log,
};

/// A stand-in adapter: an `IUnknown` with a count, which the output holds a reference to.
#[repr(C)]
struct Adapter {
    head: Head<IUnknownVtbl>,
}

/// The stand-in adapter's slots.
static ADAPTER_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: adapter_query_interface,
    add_ref: adapter_add_ref,
    release: adapter_release,
};

extern "system" fn adapter_query_interface(
    _this: *mut c_void,
    _riid: *const Guid,
    _out: *mut *mut c_void,
) -> Hresult {
    E_NOINTERFACE
}

extern "system" fn adapter_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live `Adapter`.
    unsafe { com::add_ref::<Adapter>(this) }
}

extern "system" fn adapter_release(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live `Adapter`, from `Box::into_raw`.
    unsafe { com::release::<Adapter>(this) }
}

/// An output of a monitor with no EDR headroom on a stand-in adapter: no winemetal call.
fn output() -> (*mut c_void, *mut c_void) {
    let adapter = Box::into_raw(Box::new(Adapter {
        head: Head::new(&ADAPTER_VTBL),
    }))
    .cast();
    let display = WMTDisplayDescription {
        red_primaries: [0.64, 0.33],
        blue_primaries: [0.15, 0.06],
        green_primaries: [0.3, 0.6],
        white_points: [0.3127, 0.329],
        maximum_edr_color_component_value: 1.0,
        maximum_potential_edr_color_component_value: 1.0,
        maximum_reference_edr_color_component_value: 1.0,
    };
    // SAFETY: `adapter` is a live COM object.
    let output = unsafe { new(adapter, core::ptr::null_mut(), false, display) };
    (output, adapter)
}

/// Releases the output's last reference, checks it gave back its adapter's, and frees the adapter.
fn finish(output: *mut c_void, adapter: *mut c_void) {
    assert_eq!(adapter_add_ref(adapter), 3);
    assert_eq!(adapter_release(adapter), 2);
    assert_eq!(output_release(output), 0);
    assert_eq!(adapter_add_ref(adapter), 2);
    assert_eq!(adapter_release(adapter), 1);
    assert_eq!(adapter_release(adapter), 0);
}

/// An RGB triple of one value.
const fn grey(value: f32) -> Rgb {
    Rgb {
        red: value,
        green: value,
        blue: value,
    }
}

/// A gamma control filled with `value`, scale and offset included.
fn filled(value: f32) -> GammaControl {
    GammaControl {
        scale: grey(value),
        offset: grey(value),
        gamma_curve: core::array::from_fn(|_| grey(value)),
    }
}

/// A point's three values, by their bits.
const fn bits(point: &Rgb) -> [u32; 3] {
    [
        point.red.to_bits(),
        point.green.to_bits(),
        point.blue.to_bits(),
    ]
}

/// A curve no output has: red `n / 2047`, green `1 - n / 1023`, blue 0.25; its 1025th point 9.
fn ramp() -> GammaControl {
    let mut control = filled(0.0);
    let mut n = 0.0_f32;
    for point in &mut control.gamma_curve {
        *point = Rgb {
            red: n / 2047.0,
            green: 1.0 - n / 1023.0,
            blue: 0.25,
        };
        n += 1.0;
    }
    control.gamma_curve[1024] = grey(9.0);
    control
}

#[test]
fn identity_ramp_is_i_over_1023() {
    let (output, adapter) = output();
    let ramp = output_get_gamma_ramp(output);
    assert_eq!(ramp, output_get_gamma_ramp(output));
    // SAFETY: the output's ramp, which lives as long as the output.
    let ramp = unsafe { &*ramp };
    let mut n = 0.0_f32;
    for ((red, green), blue) in ramp.red.iter().zip(&ramp.green).zip(&ramp.blue) {
        let want = (n / 1023.0).to_bits();
        let got = [red, green, blue].map(|value| value.load(Ordering::Relaxed));
        assert_eq!(got, [want; 3]);
        n += 1.0;
    }
    assert_eq!(n.to_bits(), 1024.0_f32.to_bits());
    assert_eq!(ramp.version.load(Ordering::Relaxed), 0);
    finish(output, adapter);
}

#[test]
fn set_keeps_point_1024_and_bumps_version() {
    let (output, adapter) = output();
    let set = ramp();
    assert_eq!(output_set_gamma_control(output, &raw const set), S_OK);
    assert_eq!(
        output_set_gamma_control(output, core::ptr::null()),
        DXGI_ERROR_INVALID_CALL
    );
    // SAFETY: the output's ramp, which lives as long as the output.
    let ramp = unsafe { &*output_get_gamma_ramp(output) };
    assert_eq!(ramp.version.load(Ordering::Relaxed), 1);
    let mut got = filled(7.0);
    assert_eq!(output_get_gamma_control(output, &raw mut got), S_OK);
    for (got, set) in got.gamma_curve.iter().zip(&set.gamma_curve).take(1024) {
        assert_eq!(bits(got), bits(set));
    }
    assert_eq!(bits(&got.gamma_curve[1024]), bits(&grey(7.0)));
    let red = ramp.red.iter().map(|value| value.load(Ordering::Relaxed));
    assert!(
        red.eq(set
            .gamma_curve
            .iter()
            .take(1024)
            .map(|point| point.red.to_bits()))
    );
    assert_eq!(output_set_gamma_control(output, &raw const set), S_OK);
    assert_eq!(ramp.version.load(Ordering::Relaxed), 2);
    finish(output, adapter);
}

#[test]
fn get_gamma_leaves_point_1024() {
    let (output, adapter) = output();
    let mut got = filled(7.0);
    assert_eq!(output_get_gamma_control(output, &raw mut got), S_OK);
    assert_eq!(
        output_get_gamma_control(output, core::ptr::null_mut()),
        DXGI_ERROR_INVALID_CALL
    );
    assert_eq!(bits(&got.scale), bits(&grey(1.0)));
    assert_eq!(bits(&got.offset), bits(&grey(0.0)));
    let mut n = 0.0_f32;
    for point in got.gamma_curve.iter().take(1024) {
        assert_eq!(bits(point), bits(&grey(n / 1023.0)));
        n += 1.0;
    }
    assert_eq!(bits(&got.gamma_curve[1024]), bits(&grey(7.0)));
    finish(output, adapter);
}

#[test]
fn qi_answers_output_one_to_six_and_mtl() {
    let (output, adapter) = output();
    for iid in [
        IID_IUNKNOWN,
        IID_IDXGI_OBJECT,
        IID_IDXGI_OUTPUT,
        IID_IDXGI_OUTPUT1,
        IID_IDXGI_OUTPUT2,
        IID_IDXGI_OUTPUT3,
        IID_IDXGI_OUTPUT4,
        IID_IDXGI_OUTPUT5,
        IID_IDXGI_OUTPUT6,
        IID_IMTL_DXGI_OUTPUT,
    ] {
        let mut out = core::ptr::null_mut();
        assert_eq!(
            output_query_interface(output, &raw const iid, &raw mut out),
            S_OK
        );
        assert_eq!(out, output);
        assert_eq!(output_release(output), 1);
    }
    // Logged already, so the miss below writes no log file beside the test.
    let factory = IID_IDXGI_FACTORY;
    qi_log(&IID_IDXGI_OUTPUT, &factory);
    let mut out = output;
    assert_eq!(
        output_query_interface(output, &raw const factory, &raw mut out),
        E_NOINTERFACE
    );
    assert!(out.is_null());
    finish(output, adapter);
}

#[test]
fn qi_null_riid_is_e_pointer() {
    let (output, adapter) = output();
    let mut out = output;
    assert_eq!(
        output_query_interface(output, core::ptr::null(), &raw mut out),
        E_POINTER
    );
    assert!(out.is_null());
    let iid = IID_IDXGI_OUTPUT;
    assert_eq!(
        output_query_interface(output, &raw const iid, core::ptr::null_mut()),
        E_POINTER
    );
    finish(output, adapter);
}

#[test]
fn find_closest_unknown_without_device_is_invalid_call() {
    let (output, adapter) = output();
    let unknown = ModeDesc1::default();
    let mut found = ModeDesc1 {
        width: 7,
        ..ModeDesc1::default()
    };
    let none = core::ptr::null_mut();
    assert_eq!(
        output_find_closest_matching_mode1(output, &raw const unknown, &raw mut found, none),
        DXGI_ERROR_INVALID_CALL
    );
    assert_eq!(found.width, 7);
    assert_eq!(
        output_find_closest_matching_mode1(output, core::ptr::null(), &raw mut found, none),
        DXGI_ERROR_INVALID_CALL
    );
    assert_eq!(
        output_find_closest_matching_mode1(output, &raw const unknown, core::ptr::null_mut(), none),
        DXGI_ERROR_INVALID_CALL
    );
    let mode = ModeDesc {
        width: 0,
        height: 0,
        refresh_rate: Rational::default(),
        format: DXGI_FORMAT_UNKNOWN,
        scanline_ordering: 0,
        scaling: 0,
    };
    let mut out = ModeDesc { width: 7, ..mode };
    assert_eq!(
        output_find_closest_matching_mode(output, &raw const mode, &raw mut out, none),
        DXGI_ERROR_INVALID_CALL
    );
    assert_eq!(out.width, 7);
    finish(output, adapter);
}

#[test]
fn find_closest_width_without_height_is_invalid_call() {
    let (output, adapter) = output();
    // A device the output only checks for null.
    let device = core::ptr::NonNull::<c_void>::dangling().as_ptr();
    for (width, height) in [(640, 0), (0, 480)] {
        let target = ModeDesc1 {
            width,
            height,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            ..ModeDesc1::default()
        };
        let mut found = ModeDesc1 {
            width: 7,
            ..ModeDesc1::default()
        };
        assert_eq!(
            output_find_closest_matching_mode1(output, &raw const target, &raw mut found, device),
            DXGI_ERROR_INVALID_CALL
        );
        assert_eq!(found.width, 7);
    }
    finish(output, adapter);
}
