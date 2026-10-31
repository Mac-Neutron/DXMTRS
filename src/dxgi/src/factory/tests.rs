use core::cell::{Cell, RefCell};

use super::*;
use crate::abi::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R10G10B10A2_UNORM,
    DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE, IID_IDXGI_FACTORY7, ModeDesc, SampleDesc,
};

/// The window the tests hand the factory: never dereferenced.
const WINDOW: Hwnd = ptr::without_provenance_mut(0x5a5a);
/// The swap chain the fake device answers: never dereferenced.
const SWAP_CHAIN: *mut c_void = ptr::without_provenance_mut(0xc4a1);

/// The factory's slots, read from the object as an application reaches them.
fn slots<'a>(factory: *mut c_void) -> &'a FactoryVtbl {
    // SAFETY: `create`'s object, live while the test holds a reference, starts with its vtable.
    unsafe { vtbl::<FactoryVtbl>(factory) }
}

/// `QueryInterface` on `factory`; `riid` and `out` may be null.
fn query_slot(factory: *mut c_void, riid: *const Guid, out: *mut *mut c_void) -> Hresult {
    // SAFETY: a live factory; `riid` and `out` are null or the test's locals.
    unsafe { (slots(factory).object.unknown.query_interface)(factory, riid, out) }
}

/// `AddRef` on `factory`.
fn add_ref_slot(factory: *mut c_void) -> u32 {
    // SAFETY: a live factory.
    unsafe { (slots(factory).object.unknown.add_ref)(factory) }
}

/// `Release` on `factory`: the last frees it.
fn release_slot(factory: *mut c_void) -> u32 {
    // SAFETY: a live factory, whose reference the test gives back.
    unsafe { (slots(factory).object.unknown.release)(factory) }
}

/// `GetWindowAssociation` on `factory`.
fn window_of(factory: *mut c_void, window: *mut Hwnd) -> Hresult {
    // SAFETY: a live factory; `window` is null or the test's local.
    unsafe { (slots(factory).get_window_association)(factory, window) }
}

/// `CheckFeatureSupport` on `factory`.
fn check_feature(factory: *mut c_void, feature: u32, data: *mut c_void, size: u32) -> Hresult {
    // SAFETY: a live factory; `data` is null or has room for `size` bytes.
    unsafe { (slots(factory).check_feature_support)(factory, feature, data, size) }
}

/// What the fake device's `CreateSwapChain` was handed.
struct Made {
    factory: *mut c_void,
    window: Hwnd,
    desc: SwapChainDesc1,
    fullscreen: SwapChainFullscreenDesc,
    device_refs: u32,
}

/// A D3D11 device as the factory sees one: `IUnknown` and `IMTLSwapChainFactory` in one object.
///
/// It counts its references, records the IIDs it is asked and what `CreateSwapChain` got, and
/// answers `SWAP_CHAIN`.
#[repr(C)]
struct FakeDevice {
    vtbl: &'static SwapChainFactoryVtbl,
    refs: Cell<u32>,
    asked: RefCell<Vec<Guid>>,
    made: RefCell<Vec<Made>>,
}

static FAKE_DEVICE_VTBL: SwapChainFactoryVtbl = SwapChainFactoryVtbl {
    unknown: IUnknownVtbl {
        query_interface: device_query_interface,
        add_ref: device_add_ref,
        release: device_release,
    },
    create_swap_chain: device_create_swap_chain,
};

impl FakeDevice {
    /// A device with one reference, the test's.
    fn new() -> Self {
        Self {
            vtbl: &FAKE_DEVICE_VTBL,
            refs: Cell::new(1),
            asked: RefCell::default(),
            made: RefCell::default(),
        }
    }

    fn this(&self) -> *mut c_void {
        ptr::from_ref(self).cast_mut().cast()
    }
}

/// The fake behind a slot's `this`.
fn device<'a>(this: *mut c_void) -> &'a FakeDevice {
    // SAFETY: the fake's slots get a `this` that is a live `FakeDevice`, which the test keeps.
    unsafe { &*this.cast::<FakeDevice>() }
}

extern "system" fn device_query_interface(
    this: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the factory asks with a GUID of its own.
    let Some(riid) = (unsafe { Guid::read(riid) }) else {
        return E_POINTER;
    };
    device(this).asked.borrow_mut().push(riid);
    let answer = if riid == IID_IUNKNOWN || riid == IID_IMTL_SWAP_CHAIN_FACTORY {
        device_add_ref(this);
        this
    } else {
        ptr::null_mut()
    };
    // SAFETY: the factory's out slot, a local of its own.
    unsafe { out.write_unaligned(answer) };
    if answer.is_null() {
        E_NOINTERFACE
    } else {
        S_OK
    }
}

extern "system" fn device_add_ref(this: *mut c_void) -> u32 {
    let refs = &device(this).refs;
    refs.set(refs.get() + 1);
    refs.get()
}

extern "system" fn device_release(this: *mut c_void) -> u32 {
    let refs = &device(this).refs;
    refs.set(refs.get() - 1);
    refs.get()
}

extern "system-unwind" fn device_create_swap_chain(
    this: *mut c_void,
    factory: *mut c_void,
    window: Hwnd,
    desc: *const SwapChainDesc1,
    fullscreen: *const SwapChainFullscreenDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let device = device(this);
    device.made.borrow_mut().push(Made {
        factory,
        window,
        // SAFETY: the factory's local desc.
        desc: unsafe { desc.read_unaligned() },
        // SAFETY: the factory's local fullscreen desc.
        fullscreen: unsafe { fullscreen.read_unaligned() },
        device_refs: device.refs.get(),
    });
    // SAFETY: the application's out slot, which the factory checked for null.
    unsafe { out.write_unaligned(SWAP_CHAIN) };
    S_OK
}

/// A `DXGI_SWAP_CHAIN_DESC1` with a non-zero size, so the window system is not asked.
const fn desc1() -> SwapChainDesc1 {
    SwapChainDesc1 {
        width: 320,
        height: 200,
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        stereo: 0,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        buffer_usage: 0x20,
        buffer_count: 2,
        scaling: 1,
        swap_effect: 4,
        alpha_mode: 1,
        flags: 0x800,
    }
}

/// A `DXGI_SWAP_CHAIN_DESC` with a non-zero size on `WINDOW`.
const fn desc() -> SwapChainDesc {
    SwapChainDesc {
        buffer_desc: ModeDesc {
            width: 320,
            height: 200,
            refresh_rate: Rational {
                numerator: 50,
                denominator: 1,
            },
            format: DXGI_FORMAT_R10G10B10A2_UNORM,
            scanline_ordering: DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE,
            scaling: 2,
        },
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        buffer_usage: 0x30,
        buffer_count: 3,
        output_window: WINDOW,
        windowed: 0,
        swap_effect: 3,
        flags: 2,
    }
}

#[test]
fn one_gpu_is_index_zero() {
    assert_eq!(adapter_order(&[false]), [0]);
    assert_eq!(adapter_order(&[true]), [0]);
}

#[test]
fn discrete_last_without_unified_memory_first() {
    assert_eq!(adapter_order(&[true, false]), [1, 0]);
    assert_eq!(adapter_order(&[false, true, false]), [2, 0, 1]);
    assert_eq!(adapter_order(&[false, true]), [0, 1]);
    assert_eq!(adapter_order(&[true, true]), [0, 1]);
}

#[test]
fn qi_answers_factory_one_to_six() {
    let factory = create(0);
    for iid in [
        IID_IUNKNOWN,
        IID_IDXGI_OBJECT,
        IID_IDXGI_FACTORY,
        IID_IDXGI_FACTORY1,
        IID_IDXGI_FACTORY2,
        IID_IDXGI_FACTORY3,
        IID_IDXGI_FACTORY4,
        IID_IDXGI_FACTORY5,
        IID_IDXGI_FACTORY6,
    ] {
        let mut out = ptr::null_mut();
        assert_eq!(query_slot(factory, &raw const iid, &raw mut out), S_OK);
        assert_eq!(out, factory);
        assert_eq!(release_slot(factory), 1);
    }
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn qi_refuses_factory_seven() {
    // Logged already, so the miss writes no log file beside the test.
    crate::com::qi_log(&IID_IDXGI_FACTORY2, &IID_IDXGI_FACTORY7);
    let factory = create(0);
    let mut out = factory;
    let riid = ptr::from_ref(&IID_IDXGI_FACTORY7);
    assert_eq!(query_slot(factory, riid, &raw mut out), E_NOINTERFACE);
    assert!(out.is_null());
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn qi_null_out_is_e_pointer() {
    let factory = create(0);
    let riid = ptr::from_ref(&IID_IDXGI_FACTORY);
    assert_eq!(query_slot(factory, riid, ptr::null_mut()), E_POINTER);
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn qi_null_riid_is_e_pointer() {
    let factory = create(0);
    let mut out = factory;
    assert_eq!(query_slot(factory, ptr::null(), &raw mut out), E_POINTER);
    assert!(out.is_null());
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn add_ref_release_count() {
    let factory = create(0);
    assert_eq!(add_ref_slot(factory), 2);
    assert_eq!(add_ref_slot(factory), 3);
    assert_eq!(release_slot(factory), 2);
    assert_eq!(release_slot(factory), 1);
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn get_parent_refuses() {
    let factory = create(0);
    let mut out = factory;
    // A null riid, refused without the warning a GUID gets (a host test's log line would write
    // `_dxgi.log` beside the crate); the warning's path is pinned by the probe's `getparent` lines.
    // SAFETY: a live factory; `out` is the test's local.
    let hr = unsafe { (slots(factory).object.get_parent)(factory, ptr::null(), &raw mut out) };
    assert_eq!(hr, E_POINTER);
    assert!(out.is_null());
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn window_association_round_trip() {
    let factory = create(1);
    // SAFETY: a live factory.
    assert_eq!(unsafe { (slots(factory).get_creation_flags)(factory) }, 1);
    let mut window = WINDOW;
    assert_eq!(window_of(factory, &raw mut window), S_OK);
    assert!(window.is_null());
    // SAFETY: a live factory; the window is only stored.
    let made = unsafe { (slots(factory).make_window_association)(factory, WINDOW, 0) };
    assert_eq!(made, S_OK);
    // An odd address: the out slot promises no alignment.
    let mut bytes = [0u8; 1 + size_of::<Hwnd>()];
    let odd = bytes
        .as_mut_ptr()
        .wrapping_add(1)
        .cast::<c_void>()
        .cast::<Hwnd>();
    assert_eq!(window_of(factory, odd), S_OK);
    // SAFETY: `odd` addresses a pointer's bytes inside `bytes`; the read assumes no alignment.
    assert_eq!(unsafe { odd.read_unaligned() }, WINDOW);
    // SAFETY: a live factory.
    let cleared = unsafe { (slots(factory).make_window_association)(factory, ptr::null_mut(), 0) };
    assert_eq!(cleared, S_OK);
    assert_eq!(window_of(factory, &raw mut window), S_OK);
    assert!(window.is_null());
    assert_eq!(window_of(factory, ptr::null_mut()), DXGI_ERROR_INVALID_CALL);
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn tearing_is_supported() {
    let factory = create(0);
    let mut bytes = [0xa5u8; 9];
    // An odd address: the data promises no alignment.
    let odd = bytes.as_mut_ptr().wrapping_add(1).cast::<c_void>();
    let tearing = DXGI_FEATURE_PRESENT_ALLOW_TEARING;
    assert_eq!(check_feature(factory, tearing, odd, 8), E_INVALIDARG);
    assert_eq!(bytes, [0xa5; 9]);
    assert_eq!(check_feature(factory, tearing, odd, 4), S_OK);
    assert_eq!(bytes[1..5], 1i32.to_ne_bytes());
    assert_eq!((bytes[0], &bytes[5..]), (0xa5, &[0xa5; 4][..]));
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn check_feature_null_data_is_invalid_arg() {
    let factory = create(0);
    let tearing = DXGI_FEATURE_PRESENT_ALLOW_TEARING;
    assert_eq!(
        check_feature(factory, tearing, ptr::null_mut(), 4),
        E_INVALIDARG
    );
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn create_swap_chain_translates_the_desc() {
    let factory = create(0);
    let device = FakeDevice::new();
    let mut desc = desc();
    let mut out = ptr::null_mut();
    // SAFETY: a live factory; the device, the desc and `out` are the test's.
    let hr = unsafe {
        (slots(factory).create_swap_chain)(factory, device.this(), &raw mut desc, &raw mut out)
    };
    assert_eq!(hr, S_OK);
    assert_eq!(out, SWAP_CHAIN);
    let made = device.made.borrow();
    let [made] = made.as_slice() else {
        panic!("{} calls", made.len());
    };
    assert_eq!((made.factory, made.window), (factory, WINDOW));
    let got = &made.desc;
    assert_eq!((got.width, got.height), (320, 200));
    assert_eq!((got.format, got.stereo), (DXGI_FORMAT_R10G10B10A2_UNORM, 0));
    assert_eq!((got.sample_desc.count, got.sample_desc.quality), (1, 0));
    assert_eq!((got.buffer_usage, got.buffer_count), (0x30, 3));
    assert_eq!(got.scaling, DXGI_SCALING_STRETCH);
    assert_eq!(
        (got.swap_effect, got.alpha_mode, got.flags),
        (3, DXGI_ALPHA_MODE_IGNORE, 2)
    );
    let fullscreen = &made.fullscreen;
    let rate = fullscreen.refresh_rate;
    assert_eq!((rate.numerator, rate.denominator), (50, 1));
    assert_eq!(
        fullscreen.scanline_ordering,
        DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE
    );
    assert_eq!((fullscreen.scaling, fullscreen.windowed), (2, 0));
    assert_eq!(device.refs.get(), 1);
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn create_swap_chain_writes_out_on_failure() {
    let factory = create(0);
    let device = FakeDevice::new();
    let mut desc = SwapChainDesc {
        output_window: ptr::null_mut(),
        ..desc()
    };
    let mut out = SWAP_CHAIN;
    // SAFETY: a live factory; the device, the desc and `out` are the test's.
    let hr = unsafe {
        (slots(factory).create_swap_chain)(factory, device.this(), &raw mut desc, &raw mut out)
    };
    assert_eq!(hr, DXGI_ERROR_INVALID_CALL);
    assert!(out.is_null());
    assert!(device.made.borrow().is_empty());
    assert_eq!(device.refs.get(), 1);
    assert_eq!(release_slot(factory), 0);
}

#[test]
fn swap_chain_factory_released_after_the_call() {
    let factory = create(0);
    let device = FakeDevice::new();
    let desc = desc1();
    let mut out = ptr::null_mut();
    let restrict = ptr::null_mut();
    // SAFETY: a live factory; the device, the desc and `out` are the test's.
    let hr = unsafe {
        let slot = slots(factory).create_swap_chain_for_hwnd;
        slot(
            factory,
            device.this(),
            WINDOW,
            &raw const desc,
            ptr::null(),
            restrict,
            &raw mut out,
        )
    };
    assert_eq!(hr, S_OK);
    assert_eq!(out, SWAP_CHAIN);
    assert!(*device.asked.borrow() == [IID_IMTL_SWAP_CHAIN_FACTORY]);
    let made = device.made.borrow();
    let [made] = made.as_slice() else {
        panic!("{} calls", made.len());
    };
    assert_eq!(made.device_refs, 2);
    assert_eq!((made.factory, made.window), (factory, WINDOW));
    assert_eq!(
        (made.desc.width, made.desc.height, made.desc.flags),
        (320, 200, 0x800)
    );
    let fullscreen = &made.fullscreen;
    let rate = fullscreen.refresh_rate;
    assert_eq!((rate.numerator, rate.denominator), (0, 0));
    assert_eq!((fullscreen.scanline_ordering, fullscreen.scaling), (0, 0));
    assert_eq!(fullscreen.windowed, 1);
    assert_eq!(device.refs.get(), 1);
    assert_eq!(release_slot(factory), 0);
}
