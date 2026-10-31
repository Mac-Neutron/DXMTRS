//! The factory, `IDXGIFactory6`, as `dxgi_factory.cpp`'s `MTLDXGIFactory`.
//!
//! It answers the adapters of winemetal's Metal devices, a fresh adapter each call, and makes swap
//! chains through the `IMTLSwapChainFactory` of the D3D11 or D3D12 device it is handed.

use core::{
    ffi::c_void,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

use dxmt_util::com::{IUnknownVtbl, query, vtbl};

use crate::{
    LOG,
    abi::{
        AdapterDesc, AdapterVtbl, DXGI_ALPHA_MODE_IGNORE, DXGI_ERROR_INVALID_CALL,
        DXGI_ERROR_NOT_FOUND, DXGI_ERROR_UNSUPPORTED, DXGI_FEATURE_PRESENT_ALLOW_TEARING,
        DXGI_MODE_SCALING_UNSPECIFIED, DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED, DXGI_SCALING_STRETCH,
        E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, FactoryVtbl, Guid, Hresult, Hwnd,
        IID_IDXGI_FACTORY, IID_IDXGI_FACTORY1, IID_IDXGI_FACTORY2, IID_IDXGI_FACTORY3,
        IID_IDXGI_FACTORY4, IID_IDXGI_FACTORY5, IID_IDXGI_FACTORY6, IID_IDXGI_OBJECT,
        IID_IMTL_SWAP_CHAIN_FACTORY, IID_IUNKNOWN, Luid, ObjectVtbl, Rational, S_OK, SwapChainDesc,
        SwapChainDesc1, SwapChainFactoryVtbl, SwapChainFullscreenDesc,
    },
    adapter,
    com::{Head, add_ref, query_interface, release, write_out},
    ffi,
    private_data::PrivateData,
    wsi,
};

/// The IIDs the factory answers with itself.
const ANSWERED: [Guid; 9] = [
    IID_IUNKNOWN,
    IID_IDXGI_OBJECT,
    IID_IDXGI_FACTORY,
    IID_IDXGI_FACTORY1,
    IID_IDXGI_FACTORY2,
    IID_IDXGI_FACTORY3,
    IID_IDXGI_FACTORY4,
    IID_IDXGI_FACTORY5,
    IID_IDXGI_FACTORY6,
];

static FACTORY_VTBL: FactoryVtbl = FactoryVtbl {
    object: ObjectVtbl {
        unknown: IUnknownVtbl {
            query_interface: factory_query_interface,
            add_ref: factory_add_ref,
            release: factory_release,
        },
        set_private_data: factory_set_private_data,
        set_private_data_interface: factory_set_private_data_interface,
        get_private_data: factory_get_private_data,
        get_parent: factory_get_parent,
    },
    enum_adapters: factory_enum_adapters,
    make_window_association: factory_make_window_association,
    get_window_association: factory_get_window_association,
    create_swap_chain: factory_create_swap_chain,
    create_software_adapter: factory_create_software_adapter,
    enum_adapters1: factory_enum_adapters1,
    is_current: factory_is_current,
    is_windowed_stereo_enabled: factory_is_windowed_stereo_enabled,
    create_swap_chain_for_hwnd: factory_create_swap_chain_for_hwnd,
    create_swap_chain_for_core_window: factory_create_swap_chain_for_core_window,
    get_shared_resource_adapter_luid: factory_get_shared_resource_adapter_luid,
    register_stereo_status_window: factory_register_stereo_status_window,
    register_stereo_status_event: factory_register_stereo_status_event,
    unregister_stereo_status: factory_unregister_stereo_status,
    register_occlusion_status_window: factory_register_occlusion_status_window,
    register_occlusion_status_event: factory_register_occlusion_status_event,
    unregister_occlusion_status: factory_unregister_occlusion_status,
    create_swap_chain_for_composition: factory_create_swap_chain_for_composition,
    get_creation_flags: factory_get_creation_flags,
    enum_adapter_by_luid: factory_enum_adapter_by_luid,
    enum_warp_adapter: factory_enum_warp_adapter,
    check_feature_support: factory_check_feature_support,
    enum_adapter_by_gpu_preference: factory_enum_adapter_by_gpu_preference,
};

/// A new factory with one reference, created with `flags` (`CreateDXGIFactory2`'s).
#[must_use]
pub fn create(flags: u32) -> *mut c_void {
    Box::into_raw(Box::new(Factory {
        head: Head::new(&FACTORY_VTBL),
        private_data: PrivateData::default(),
        flags,
        window: AtomicPtr::default(),
    }))
    .cast()
}

/// The factory object: `this` of every interface it answers.
#[repr(C)]
struct Factory {
    head: Head<FactoryVtbl>,
    private_data: PrivateData,
    flags: u32,
    /// `MakeWindowAssociation`'s window, only stored and answered.
    window: AtomicPtr<c_void>,
}

/// The factory behind a slot's `this`.
///
/// # Safety
///
/// `this` is a live factory: a slot of `FACTORY_VTBL` gets one.
const unsafe fn factory<'a>(this: *mut c_void) -> &'a Factory {
    // SAFETY: the caller's: a live `Factory`, which `create` boxed.
    unsafe { &*this.cast::<Factory>() }
}

/// `InitReturnPtr`: nulls the application's out slot unless it is null itself.
///
/// # Safety
///
/// `out` is null or addresses a writable pointer, with no alignment promised.
const unsafe fn init_return_ptr(out: *mut *mut c_void) {
    if !out.is_null() {
        // SAFETY: the caller's; `out` is non-null here.
        unsafe { write_out(out, ptr::null_mut()) };
    }
}

/// The answer of every slot DXMT does not implement, with its error line.
fn not_implemented() -> Hresult {
    LOG.err("Not implemented");
    E_NOTIMPL
}

/// Where `EnumAdapters1`'s adapter `index` sits in winemetal's device list.
///
/// With more than one device the last one without unified memory (a discrete GPU) is adapter 0,
/// and each device before it answers one index later.
fn adapter_order(unified: &[bool]) -> Vec<usize> {
    let preferred = unified.iter().rposition(|unified| !unified).unwrap_or(0);
    (0..unified.len())
        .map(|index| match index {
            0 => preferred,
            index if index <= preferred => index - 1,
            index => index,
        })
        .collect()
}

extern "system" fn factory_query_interface(
    this: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let log_as = (&IID_IDXGI_FACTORY2, "DXGIFactory");
    // SAFETY: the slot's `this` is a live factory whose `AddRef` is `factory_add_ref`; `riid` and
    // `out` are the application's, null or valid, which `query_interface` reads unaligned.
    unsafe { query_interface(this, riid, out, &ANSWERED, &[], log_as, factory_add_ref) }
}

extern "system" fn factory_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live `Factory`, which starts with its `Head`.
    unsafe { add_ref::<Factory>(this) }
}

extern "system" fn factory_release(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live `Factory` from `Box::into_raw` (`create`).
    unsafe { release::<Factory>(this) }
}

extern "system" fn factory_set_private_data(
    this: *mut c_void,
    name: *const Guid,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: the application's GUID, null or 16 bytes; the read assumes no alignment.
    let Some(name) = (unsafe { Guid::read(name) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: the slot's `this` is a live factory.
    let factory = unsafe { factory(this) };
    // SAFETY: the application's `data`, null or `size` readable bytes.
    unsafe { factory.private_data.set_data(name, size, data) }
}

extern "system" fn factory_set_private_data_interface(
    this: *mut c_void,
    name: *const Guid,
    unknown: *mut c_void,
) -> Hresult {
    // SAFETY: the application's GUID, null or 16 bytes; the read assumes no alignment.
    let Some(name) = (unsafe { Guid::read(name) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: the slot's `this` is a live factory.
    let factory = unsafe { factory(this) };
    // SAFETY: the application's interface, null or a live COM object.
    unsafe { factory.private_data.set_interface(name, unknown) }
}

extern "system" fn factory_get_private_data(
    this: *mut c_void,
    name: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: the application's GUID, null or 16 bytes; the read assumes no alignment.
    let Some(name) = (unsafe { Guid::read(name) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: the slot's `this` is a live factory.
    let factory = unsafe { factory(this) };
    // SAFETY: the application's `size` and `data`, null or valid, which `get_data` reaches
    // unaligned.
    unsafe { factory.private_data.get_data(name, size, data) }
}

extern "system" fn factory_get_parent(
    _this: *mut c_void,
    riid: *const Guid,
    parent: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's out slot, null or writable.
    unsafe { init_return_ptr(parent) };
    // SAFETY: the application's IID, null or 16 bytes; the read assumes no alignment.
    let Some(riid) = (unsafe { Guid::read(riid) }) else {
        return E_POINTER;
    };
    LOG.warn(&format!(
        "DXGIFactory::GetParent: Unknown interface query {riid}"
    ));
    E_NOINTERFACE
}

extern "system" fn factory_enum_adapters(
    this: *mut c_void,
    index: u32,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's out slot, null or writable.
    unsafe { init_return_ptr(out) };
    if out.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    let mut adapter = ptr::null_mut();
    let hr = factory_enum_adapters1(this, index, &raw mut adapter);
    // SAFETY: the application's out slot, non-null here.
    unsafe { write_out(out, adapter) };
    hr
}

extern "system" fn factory_make_window_association(
    this: *mut c_void,
    window: Hwnd,
    flags: u32,
) -> Hresult {
    if flags != 0 {
        LOG.warn(&format!("MakeWindowAssociation: Ignoring flags {flags}"));
    }
    // SAFETY: the slot's `this` is a live factory.
    let factory = unsafe { factory(this) };
    factory.window.store(window, Ordering::Relaxed);
    S_OK
}

extern "system" fn factory_get_window_association(this: *mut c_void, window: *mut Hwnd) -> Hresult {
    if window.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: the slot's `this` is a live factory.
    let factory = unsafe { factory(this) };
    // SAFETY: the application's out slot, non-null here.
    unsafe { write_out(window, factory.window.load(Ordering::Relaxed)) };
    S_OK
}

extern "system" fn factory_create_swap_chain(
    this: *mut c_void,
    device: *mut c_void,
    desc: *mut SwapChainDesc,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() || desc.is_null() || device.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: the application's desc, non-null here; the read assumes no alignment.
    let desc = unsafe { desc.read_unaligned() };
    let mode = &desc.buffer_desc;
    let desc1 = SwapChainDesc1 {
        width: mode.width,
        height: mode.height,
        format: mode.format,
        stereo: i32::from(false),
        sample_desc: desc.sample_desc,
        buffer_usage: desc.buffer_usage,
        buffer_count: desc.buffer_count,
        scaling: DXGI_SCALING_STRETCH,
        swap_effect: desc.swap_effect,
        alpha_mode: DXGI_ALPHA_MODE_IGNORE,
        flags: desc.flags,
    };
    let fullscreen = SwapChainFullscreenDesc {
        refresh_rate: mode.refresh_rate,
        scanline_ordering: mode.scanline_ordering,
        scaling: mode.scaling,
        windowed: desc.windowed,
    };
    let mut swap_chain = ptr::null_mut();
    let hr = factory_create_swap_chain_for_hwnd(
        this,
        device,
        desc.output_window,
        &raw const desc1,
        &raw const fullscreen,
        ptr::null_mut(),
        &raw mut swap_chain,
    );
    // SAFETY: the application's out slot, non-null here; written on failure too, as the C++.
    unsafe { write_out(out, swap_chain) };
    hr
}

extern "system" fn factory_create_software_adapter(
    _this: *mut c_void,
    _module: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's out slot, null or writable.
    unsafe { init_return_ptr(out) };
    if out.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    LOG.err("Software adapters not supported");
    DXGI_ERROR_UNSUPPORTED
}

extern "system" fn factory_enum_adapters1(
    this: *mut c_void,
    index: u32,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's out slot, null or writable.
    unsafe { init_return_ptr(out) };
    // SAFETY: winemetal's call; the array it answers is this function's to release.
    let devices = unsafe { ffi::WMTCopyAllDevices() };
    let hr = enum_adapter(this, devices, index, out);
    if devices != 0 {
        // SAFETY: the reference `WMTCopyAllDevices` gave.
        unsafe { ffi::NSObject_release(devices) };
    }
    hr
}

extern "system" fn factory_is_current(_this: *mut c_void) -> i32 {
    i32::from(true)
}

extern "system" fn factory_is_windowed_stereo_enabled(_this: *mut c_void) -> i32 {
    i32::from(false)
}

extern "system" fn factory_create_swap_chain_for_hwnd(
    this: *mut c_void,
    device: *mut c_void,
    window: Hwnd,
    desc: *const SwapChainDesc1,
    fullscreen_desc: *const SwapChainFullscreenDesc,
    _restrict_to_output: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's out slot, null or writable.
    unsafe { init_return_ptr(out) };
    if out.is_null() || desc.is_null() || window.is_null() || device.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: the application's device, non-null here and live.
    let Some(swap_chain_factory) = (unsafe { query(device, &IID_IMTL_SWAP_CHAIN_FACTORY) }) else {
        LOG.err("Unsupported device type");
        return DXGI_ERROR_UNSUPPORTED;
    };
    // SAFETY: the application's desc, non-null here; the read assumes no alignment.
    let mut desc = unsafe { desc.read_unaligned() };
    if desc.width == 0 || desc.height == 0 {
        let (width, height) = wsi::window_size(window);
        if desc.width == 0 {
            desc.width = width;
        }
        if desc.height == 0 {
            desc.height = height;
        }
    }
    let fullscreen = if fullscreen_desc.is_null() {
        SwapChainFullscreenDesc {
            refresh_rate: Rational::default(),
            scanline_ordering: DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
            scaling: DXGI_MODE_SCALING_UNSPECIFIED,
            windowed: i32::from(true),
        }
    } else {
        // SAFETY: the application's fullscreen desc, non-null here; the read assumes no alignment.
        unsafe { fullscreen_desc.read_unaligned() }
    };
    // SAFETY: the query answered an `IMTLSwapChainFactory`, which this function holds.
    let slots = unsafe { vtbl::<SwapChainFactoryVtbl>(swap_chain_factory) };
    // SAFETY: the interface is live; the descs are locals; `out` is the application's non-null
    // slot, which the swap chain factory writes.
    let hr = unsafe {
        (slots.create_swap_chain)(
            swap_chain_factory,
            this,
            window,
            &raw const desc,
            &raw const fullscreen,
            out,
        )
    };
    // SAFETY: the reference the query gave; the interface is not used after it.
    unsafe { dxmt_util::com::release(swap_chain_factory) };
    hr
}

extern "system" fn factory_create_swap_chain_for_core_window(
    _this: *mut c_void,
    _device: *mut c_void,
    _window: *mut c_void,
    _desc: *const SwapChainDesc1,
    _restrict_to_output: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's out slot, null or writable.
    unsafe { init_return_ptr(out) };
    not_implemented()
}

extern "system" fn factory_get_shared_resource_adapter_luid(
    _this: *mut c_void,
    _resource: *mut c_void,
    _luid: *mut Luid,
) -> Hresult {
    not_implemented()
}

extern "system" fn factory_register_stereo_status_window(
    _this: *mut c_void,
    _window: Hwnd,
    _message: u32,
    _cookie: *mut u32,
) -> Hresult {
    not_implemented()
}

extern "system" fn factory_register_stereo_status_event(
    _this: *mut c_void,
    _event: *mut c_void,
    _cookie: *mut u32,
) -> Hresult {
    not_implemented()
}

extern "system" fn factory_unregister_stereo_status(_this: *mut c_void, _cookie: u32) {
    not_implemented();
}

extern "system" fn factory_register_occlusion_status_window(
    _this: *mut c_void,
    _window: Hwnd,
    _message: u32,
    _cookie: *mut u32,
) -> Hresult {
    not_implemented()
}

extern "system" fn factory_register_occlusion_status_event(
    _this: *mut c_void,
    _event: *mut c_void,
    _cookie: *mut u32,
) -> Hresult {
    not_implemented()
}

extern "system" fn factory_unregister_occlusion_status(_this: *mut c_void, _cookie: u32) {
    not_implemented();
}

extern "system" fn factory_create_swap_chain_for_composition(
    _this: *mut c_void,
    _device: *mut c_void,
    _desc: *const SwapChainDesc1,
    _restrict_to_output: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's out slot, null or writable.
    unsafe { init_return_ptr(out) };
    not_implemented()
}

const extern "system" fn factory_get_creation_flags(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live factory.
    unsafe { factory(this) }.flags
}

extern "system" fn factory_enum_adapter_by_luid(
    this: *mut c_void,
    luid: Luid,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's out slot, null or writable.
    unsafe { init_return_ptr(out) };
    // The LUID is not looked up: Apple silicon has one GPU, so adapter 0 answers.
    query_adapter(this, 0, riid, out, |adapter| {
        let mut desc = MaybeUninit::<AdapterDesc>::zeroed();
        // SAFETY: an adapter of this crate, live while `query_adapter` holds it.
        let slots = unsafe { vtbl::<AdapterVtbl>(adapter) };
        // SAFETY: the adapter is live; `desc` is a local.
        unsafe { (slots.get_desc)(adapter, desc.as_mut_ptr()) };
        // SAFETY: all-zero bytes are an `AdapterDesc` (integers only), which `GetDesc` overwrote.
        let found = unsafe { desc.assume_init_ref() }.adapter_luid;
        if found.high_part != luid.high_part || found.low_part != luid.low_part {
            LOG.warn("EnumAdapterByLuid: returning an adapter with mismatched luid");
        }
    })
}

extern "system" fn factory_enum_warp_adapter(
    this: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's out slot, null or writable.
    unsafe { init_return_ptr(out) };
    // No WARP on Metal: the first adapter (3DMark's Steel Nomad crashed on a null one).
    query_adapter(this, 0, riid, out, |_| {
        LOG.warn("DXGIFactory::EnumWarpAdapter: no WARP, returning the first adapter");
    })
}

extern "system" fn factory_check_feature_support(
    _this: *mut c_void,
    feature: u32,
    data: *mut c_void,
    size: u32,
) -> Hresult {
    if feature != DXGI_FEATURE_PRESENT_ALLOW_TEARING {
        LOG.err(&format!(
            "DXGIFactory::CheckFeatureSupport: unknown feature {feature}"
        ));
        return E_INVALIDARG;
    }
    if size as usize != size_of::<i32>() || data.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: the application's `BOOL`, non-null here; the write assumes no alignment.
    unsafe { data.cast::<i32>().write_unaligned(i32::from(true)) };
    S_OK
}

extern "system" fn factory_enum_adapter_by_gpu_preference(
    this: *mut c_void,
    index: u32,
    _preference: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    // The preference is not looked at: Apple silicon has one GPU. `out` is not nulled first.
    query_adapter(this, index, riid, out, |_| {})
}

/// `EnumAdapters1`'s work on winemetal's device list `devices`.
///
/// Past the last device is `DXGI_ERROR_NOT_FOUND`, before a null `out` is refused.
fn enum_adapter(this: *mut c_void, devices: u64, index: u32, out: *mut *mut c_void) -> Hresult {
    // SAFETY: winemetal's array, or nil, which counts none.
    let count = unsafe { ffi::NSArray_count(devices) };
    if u64::from(index) >= count {
        return DXGI_ERROR_NOT_FOUND;
    }
    if out.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    let device = |at: u64| {
        // SAFETY: winemetal's array; `at` is below its count.
        unsafe { ffi::NSArray_object(devices, at) }
    };
    let unified: Vec<bool> = (0..count)
        .map(|at| {
            // SAFETY: a device of winemetal's array, which holds it.
            unsafe { ffi::MTLDevice_hasUnifiedMemory(device(at)) }
        })
        .collect();
    let Some(&at) = adapter_order(&unified).get(index as usize) else {
        return DXGI_ERROR_NOT_FOUND;
    };
    // SAFETY: `this` is the slot's live factory; the adapter retains the device.
    let adapter = unsafe { adapter::create(device(at as u64), this) };
    // SAFETY: the application's out slot, non-null here.
    unsafe { write_out(out, adapter) };
    S_OK
}

/// Adapter `index` asked for `riid` into `out`, after `inspect` saw it.
///
/// A failed `EnumAdapters1` is returned with `out` untouched; a null `riid` is `E_POINTER`, also
/// with `out` untouched.
fn query_adapter(
    this: *mut c_void,
    index: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
    inspect: impl FnOnce(*mut c_void),
) -> Hresult {
    let mut adapter = ptr::null_mut();
    let hr = factory_enum_adapters1(this, index, &raw mut adapter);
    if hr < 0 {
        return hr;
    }
    inspect(adapter);
    let hr = if riid.is_null() {
        E_POINTER
    } else {
        // SAFETY: an adapter of this crate, live while this function holds its reference.
        let unknown = unsafe { vtbl::<IUnknownVtbl>(adapter) };
        // SAFETY: the adapter is live; `riid` and `out` are the application's, which its
        // `QueryInterface` reads and writes unaligned.
        unsafe { (unknown.query_interface)(adapter, riid, out) }
    };
    // SAFETY: the reference `EnumAdapters1` gave; a successful query holds its own.
    unsafe { dxmt_util::com::release(adapter) };
    hr
}

#[cfg(test)]
mod tests;
