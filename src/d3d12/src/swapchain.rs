//! `IDXGISwapChain4` (`d3d12_swapchain.cpp`): D3D12 flip-model swap chain over
//!
//! [`dxmt_core::presenter::Presenter`].

use core::{ffi::c_void, ptr};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

#[cfg(test)]
use dxmt_core::swapchain::fullscreen::leave_fullscreen_background;
use dxmt_core::{
    hud::Hud,
    library::Library,
    presenter::{Presenter, PresenterDesc},
    swapchain::{
        Core, State as CoreState, convert_color_space, convert_format,
        fullscreen::{self, Host},
        mode_refresh_rate, render_scale,
    },
};
use dxmt_util::{
    com::{ComObject, Guid, IID_IUNKNOWN, IUnknownVtbl, vtbl},
    config::Config,
    dxgi_vtbl::IDXGISwapChain4Vtbl,
    log::Logger,
    private_data::{self, HasPrivateData, PrivateData},
    sync::lock,
    wsi::{self, Rational as WsiRational, WindowState},
};
use winemetal_sys::{
    CreateMetalViewFromHWND, Obj, ReleaseMetalView, wire::WMT_PIXEL_FORMAT_INVALID,
};

use crate::{
    abi::{
        barrier::{D3D12_HEAP_TYPE_CUSTOM, D3D12_HEAP_TYPE_DEFAULT},
        pipeline::SampleDesc as D3d12SampleDesc,
        resource::{
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT, D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
            D3D12_TEXTURE_LAYOUT_UNKNOWN, DXGI_FORMAT_UNKNOWN, HeapProperties, ResourceDesc,
        },
        swapchain::{
            D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES, DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709,
            DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_FORMAT_R16G16B16A16_FLOAT,
            DXGI_MAX_SWAP_CHAIN_BUFFERS, DXGI_PRESENT_TEST, DXGI_STATUS_OCCLUDED,
            DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT, DXGI_USAGE_UNORDERED_ACCESS,
            IID_IDXGI_ADAPTER, Matrix3x2f, PresentParameters, Rgba, SwapChainDesc1,
            SwapChainFullscreenDesc,
        },
    },
    com::{
        self, ComRefCounted, DXGI_ERROR_INVALID_CALL, DXGI_ERROR_NOT_FOUND, E_FAIL, E_INVALIDARG,
        E_NOINTERFACE, E_NOTIMPL, E_POINTER, Hresult, IID_ID3D12_RESOURCE,
        IID_IDXGI_DEVICE_SUB_OBJECT, IID_IDXGI_OBJECT, IID_IDXGI_SWAP_CHAIN, IID_IDXGI_SWAP_CHAIN1,
        IID_IDXGI_SWAP_CHAIN2, IID_IDXGI_SWAP_CHAIN3, IID_IDXGI_SWAP_CHAIN4, S_OK, release_com,
    },
    device::{self, Device},
    queue::Queue,
    resource::{self, Resource},
    slot,
};

/// Default maximum frame latency (`kSwapchainLatency`, `d3d12_swapchain.cpp:40`).
const SWAPCHAIN_LATENCY: u32 = 1;

/// `DUPLICATE_SAME_ACCESS` (`winnt.h`).
const DUPLICATE_SAME_ACCESS: u32 = 2;

#[cfg(windows)]
unsafe extern "system" {
    fn CreateSemaphoreW(
        attributes: *mut c_void,
        initial_count: i32,
        maximum_count: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn ReleaseSemaphore(
        semaphore: *mut c_void,
        release_count: i32,
        previous_count: *mut i32,
    ) -> i32;
    fn CloseHandle(object: *mut c_void) -> i32;
    fn GetCurrentProcess() -> *mut c_void;
    fn DuplicateHandle(
        source_process_handle: *mut c_void,
        source_handle: *mut c_void,
        target_process_handle: *mut c_void,
        target_handle: *mut *mut c_void,
        desired_access: u32,
        inherit_handle: i32,
        options: u32,
    ) -> i32;
    fn Sleep(milliseconds: u32);
}

fn create_semaphore(initial: i32, max: i32) -> *mut c_void {
    #[cfg(windows)]
    {
        // SAFETY: null security attributes and name create an unnamed semaphore.
        unsafe { CreateSemaphoreW(ptr::null_mut(), initial, max, ptr::null()) }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: creates a native Win32 semaphore with `initial` and `max` counts.
        unsafe { winemetal_sys::wmt_native_create_semaphore(initial, max) }
    }
}

fn release_semaphore(handle: *mut c_void, count: i32) {
    if handle.is_null() {
        return;
    }
    #[cfg(windows)]
    {
        // SAFETY: `handle` is the swapchain's valid `present_semaphore`.
        let _ = unsafe { ReleaseSemaphore(handle, count, ptr::null_mut()) };
    }
    #[cfg(not(windows))]
    {
        // SAFETY: `handle` is the swapchain's valid native semaphore handle.
        let _ =
            unsafe { winemetal_sys::wmt_native_release_semaphore(handle, count, ptr::null_mut()) };
    }
}

fn duplicate_same_access(handle: *mut c_void) -> *mut c_void {
    if handle.is_null() {
        return ptr::null_mut();
    }
    let mut result: *mut c_void = ptr::null_mut();
    #[cfg(windows)]
    {
        // SAFETY: `GetCurrentProcess` takes no arguments and returns a pseudo-handle.
        let proc = unsafe { GetCurrentProcess() };
        // SAFETY: `proc` and `handle` are valid handles; `result` is a writable output slot.
        let ok = unsafe {
            DuplicateHandle(
                proc,
                handle,
                proc,
                &raw mut result,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 { ptr::null_mut() } else { result }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: `handle` is a native semaphore handle and `result` is a writable output slot.
        let ok = unsafe {
            winemetal_sys::wmt_native_duplicate_handle(
                handle,
                &raw mut result,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 { ptr::null_mut() } else { result }
    }
}

fn close_handle(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    #[cfg(windows)]
    {
        // SAFETY: `handle` is a live kernel handle owned by the swapchain.
        let _ = unsafe { CloseHandle(handle) };
    }
    #[cfg(not(windows))]
    {
        // SAFETY: `handle` is a live native handle owned by the swapchain.
        let _ = unsafe { winemetal_sys::wmt_native_close_handle(handle) };
    }
}

fn sleep_ms(ms: u32) {
    #[cfg(windows)]
    {
        // SAFETY: `Sleep` suspends the calling thread for `ms` milliseconds.
        unsafe { Sleep(ms) };
    }
    #[cfg(not(windows))]
    {
        std::thread::sleep(std::time::Duration::from_millis(u64::from(ms)));
    }
}

/// Runs `sleep` on the minimised-present skip path (`d3d12_swapchain.cpp:794-799`).
fn minimised_sleep<F: FnOnce(u32)>(without_hooks: bool, sleep: F) {
    if without_hooks {
        sleep(16);
    }
}

/// Retained swap-chain backbuffer [`Resource`] (`Com<Resource>` in `d3d12_swapchain.cpp:173`).
struct Backbuffer(*mut Resource);

// SAFETY: `Resource` uses atomic COM reference counting across threads.
unsafe impl Send for Backbuffer {}

impl Backbuffer {
    /// Adopts the initial public COM reference on a non-null `Resource` pointer.
    const unsafe fn adopt(ptr: *mut Resource) -> Self {
        Self(ptr)
    }

    const fn as_ptr(&self) -> *mut Resource {
        self.0
    }

    const fn as_ref(&self) -> &Resource {
        // SAFETY: `self.0` is non-null and kept alive by this `Backbuffer`'s public COM reference.
        unsafe { &*self.0 }
    }
}

impl Clone for Backbuffer {
    fn clone(&self) -> Self {
        // SAFETY: `self.0` is a live `Resource` kept alive by `self`.
        unsafe { com::add_ref_com(self.0) };
        Self(self.0)
    }
}

impl Drop for Backbuffer {
    fn drop(&mut self) {
        // SAFETY: `self.0` owns one public COM reference acquired by `adopt` or `clone`.
        unsafe { release_com(self.0) };
    }
}

/// D3D12's part of the swap-chain state: the backbuffers.
pub struct Api {
    backbuffer_desc: ResourceDesc,
    backbuffers: Vec<Backbuffer>,
}

/// The swap chain's state behind its lock.
type State = CoreState<Api>;

/// Updates `init_refresh_rate` from the monitor's current display mode.
///
/// `ReadRefreshRate`, `d3d12_swapchain.cpp:488-492`.
fn read_refresh_rate(state: &mut State) {
    if let Some(rate) = mode_refresh_rate(wsi::current_display_mode(state.monitor)) {
        state.init_refresh_rate = rate;
    }
}

/// `SwapChain` (`d3d12_swapchain.cpp:146`).
#[repr(C)]
pub struct SwapChain {
    pub vtbl: &'static IDXGISwapChain4Vtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub queue: *const Queue,
    /// The shared part, declared before `library`.
    ///
    /// Its presenter drops while the library it was made with is alive.
    core: Core<Api>,
    pub hud: Option<Hud>,
    factory: *mut c_void,
    native_view: u64,
    present_semaphore: *mut c_void,
    render_scale: f64,
    preferred_max_frame_rate: i32,
    library: Option<Library>,
    /// Present calls are serialised even while the queue lock is released for their waits.
    present_serial: Mutex<()>,
}

// SAFETY: `SwapChain` synchronises mutable state with `Mutex`/`AtomicBool`, and its raw pointers
// (`device`, `queue`, `factory`, `present_semaphore`, the core's `hwnd`) are thread-safe COM/OS
// handles.
unsafe impl Send for SwapChain {}
// SAFETY: see `Send` above.
unsafe impl Sync for SwapChain {}

impl ComRefCounted for SwapChain {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for SwapChain {
    fn drop(&mut self) {
        if self.native_view != 0 {
            // SAFETY: `self.native_view` was created by `CreateMetalViewFromHWND`.
            unsafe { ReleaseMetalView(self.native_view) };
            self.native_view = 0;
        }
        close_handle(self.present_semaphore);
        let state = self
            .core
            .state
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        // SAFETY: `state.target` owns a COM reference when non-null.
        unsafe { state.set_target(ptr::null_mut()) };
        // SAFETY: `self.factory` was retained with `add_ref` in `create`.
        unsafe { dxmt_util::com::release(self.factory) };
    }
}

const _: [(); size_of::<Obj>()] = [(); size_of::<Library>()];
const _: [(); align_of::<Obj>()] = [(); align_of::<Library>()];

/// Extracts the `MTLLibrary` handle from [`Library`].
#[must_use]
const fn library_handle(lib: &Library) -> u64 {
    // SAFETY: `Library` has a single `Obj` field of identical size and alignment.
    unsafe { (*ptr::from_ref(lib).cast::<Obj>()).handle() }
}

impl SwapChain {
    fn lock_state(&self) -> std::sync::MutexGuard<'_, State> {
        self.core.lock()
    }

    /// `ApplyLayerProps` (`d3d12_swapchain.cpp:647-658`).
    fn apply_layer_props(&self, state: &State) {
        let cs = if state.desc.format == DXGI_FORMAT_R16G16B16A16_FLOAT {
            DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709
        } else {
            state.color_space
        };
        let target_color_space = convert_color_space(cs, self.core.layer_support_edr());
        if let Some(presenter) = self.core.presenter.as_ref() {
            let _ = presenter.change_layer_properties(
                convert_format(state.desc.format),
                target_color_space,
                f64::from(state.desc.width),
                f64::from(state.desc.height),
                u8::try_from(state.desc.sample_desc.count).unwrap_or(1),
            );
        }
    }
}

impl Host for SwapChain {
    type Api = Api;

    fn core(&self) -> &Core<Api> {
        &self.core
    }

    fn adapter(&self) -> Option<*mut c_void> {
        // SAFETY: `self.device` points to the parent `Device`.
        let dev = unsafe { &*self.device };
        // SAFETY: `dev.adapter` is the device's `IMTLDXGIAdapter` COM pointer.
        unsafe { dxmt_util::com::query(dev.adapter, &IID_IDXGI_ADAPTER) }
    }

    fn log() -> &'static Logger {
        &crate::LOG
    }

    const DIAGNOSTICS: bool = true;

    fn left(&self, state: &mut State, _restored: bool) {
        read_refresh_rate(state);
    }
}

/// `IDXGISwapChain::QueryInterface` (`d3d12_swapchain.cpp:257-276`).
unsafe fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::scope!("swapchain.QueryInterface");
    let qi = com::Qi {
        null_riid: E_NOINTERFACE,
        silent: &[],
        log: &crate::LOG,
        miss: com::QiMiss::Once {
            object: &IID_IDXGI_SWAP_CHAIN1,
            name: "DXGISwapChain",
        },
    };
    let add_ref = || {
        // SAFETY: `this` is a live `SwapChain` pointer.
        unsafe { com::add_ref_com(this.cast::<SwapChain>()) };
    };
    // SAFETY: the application's `riid` and `out`, null or valid; `this` is a live `SwapChain`.
    unsafe {
        com::qi_table(
            riid,
            out,
            &[(
                &[
                    IID_IUNKNOWN,
                    IID_IDXGI_OBJECT,
                    IID_IDXGI_DEVICE_SUB_OBJECT,
                    IID_IDXGI_SWAP_CHAIN,
                    IID_IDXGI_SWAP_CHAIN1,
                    IID_IDXGI_SWAP_CHAIN2,
                    IID_IDXGI_SWAP_CHAIN3,
                    IID_IDXGI_SWAP_CHAIN4,
                ],
                this,
            )],
            add_ref,
            &qi,
        )
    }
}

/// `IDXGIObject::GetParent` (`d3d12_swapchain.cpp:280-283`).
unsafe fn get_parent(this: *mut c_void, riid: *const c_void, parent: *mut *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetParent");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    if sc.factory.is_null() {
        return E_NOINTERFACE;
    }
    // SAFETY: `sc.factory` is a live `IDXGIFactory1` retained by `sc`.
    let unk = unsafe { vtbl::<IUnknownVtbl>(sc.factory) };
    // SAFETY: forwards `riid` and `parent` to the factory's `QueryInterface`.
    unsafe { (unk.query_interface)(sc.factory, riid.cast::<Guid>(), parent) }
}

/// `IDXGIDeviceSubObject::GetDevice` (`dxgi_object.hpp:34-36`).
unsafe fn get_device(this: *mut c_void, riid: *const c_void, device: *mut *mut c_void) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let dev_ptr = sc.device.cast::<c_void>();
    if dev_ptr.is_null() {
        return E_NOINTERFACE;
    }
    // SAFETY: `dev_ptr` is a live `Device` whose first field is its vtable.
    let unk = unsafe { vtbl::<IUnknownVtbl>(dev_ptr) };
    // SAFETY: forwards `riid` and `device` to the device's `QueryInterface`.
    unsafe { (unk.query_interface)(dev_ptr, riid.cast::<Guid>(), device) }
}

/// `IDXGISwapChain::Present` (`d3d12_swapchain.cpp:288-291`).
unsafe fn present(this: *mut c_void, sync_interval: u32, flags: u32) -> Hresult {
    crate::scope!("swapchain.Present");
    // SAFETY: forwards to `present1` with null `present_parameters`.
    unsafe { present1(this, sync_interval, flags, ptr::null()) }
}

/// `IDXGISwapChain::GetBuffer` (`d3d12_swapchain.cpp:295-300`).
unsafe fn get_buffer(
    this: *mut c_void,
    buffer: u32,
    riid: *const c_void,
    surface: *mut *mut c_void,
) -> Hresult {
    crate::scope!("swapchain.GetBuffer");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let Some(backbuffer) = sc.lock_state().backbuffers.get(buffer as usize).cloned() else {
        return DXGI_ERROR_NOT_FOUND;
    };
    let res_ptr = backbuffer.as_ptr().cast::<c_void>();
    // SAFETY: `res_ptr` is a live `Resource` whose vtable starts with `IUnknownVtbl`.
    let unk = unsafe { vtbl::<IUnknownVtbl>(res_ptr) };
    // SAFETY: forwards `riid` and `surface` to the backbuffer's `QueryInterface`.
    unsafe { (unk.query_interface)(res_ptr, riid.cast::<Guid>(), surface) }
}

/// `IDXGISwapChain::SetFullscreenState` (`d3d12_swapchain.cpp:305-328`).
unsafe fn set_fullscreen_state(this: *mut c_void, fullscreen: i32, target: *mut c_void) -> Hresult {
    crate::scope!("swapchain.SetFullscreenState");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: `target` is null or the application's live output.
    unsafe { fullscreen::set_fullscreen_state(sc, fullscreen, target) }
}

/// `IDXGISwapChain::GetFullscreenState` (`d3d12_swapchain.cpp:507-518`).
unsafe fn get_fullscreen_state(
    this: *mut c_void,
    fullscreen: *mut i32,
    target: *mut *mut c_void,
) -> Hresult {
    crate::scope!("swapchain.GetFullscreenState");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: the pointers are the application's.
    unsafe { sc.core.get_fullscreen_state(fullscreen, target) }
}

/// `IDXGISwapChain::GetDesc` (`d3d12_swapchain.cpp:522-541`).
unsafe fn get_desc(this: *mut c_void, desc: *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetDesc");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: `desc` is the application's.
    unsafe { sc.core.get_desc(desc) }
}

/// Scales `dimension` by `scale`, rounding to the nearest integer and clamping to `>= 1`
///
/// (`d3d12_swapchain.cpp:563-564`).
fn scale_dimension(dimension: u32, scale: f64) -> u32 {
    let scaled = (f64::from(dimension) * scale).round();
    if !(1.0..=f64::from(u32::MAX)).contains(&scaled) {
        return 1;
    }
    let mut lo = 1_u32;
    let mut hi = dimension.max(1);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if f64::from(mid) < scaled {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// `IDXGISwapChain::ResizeBuffers` (`d3d12_swapchain.cpp:546-604`).
unsafe fn resize_buffers(
    this: *mut c_void,
    buffer_count: u32,
    width: u32,
    height: u32,
    format: u32,
    flags: u32,
) -> Hresult {
    crate::scope!("swapchain.ResizeBuffers");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let mut state = sc.lock_state();
    if flags != 0 {
        state.desc.flags = (flags & !DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT)
            | (state.desc.flags & DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT);
    }
    if state.fullscreen_desc.windowed != 0 {
        state.monitor = wsi::window_monitor(sc.core.hwnd);
    }
    read_refresh_rate(&mut state);
    if width == 0 || height == 0 {
        let (w, h) = wsi::window_size(sc.core.hwnd);
        state.desc.width = w;
        state.desc.height = h;
    } else {
        state.desc.width = width;
        state.desc.height = height;
    }
    if (sc.render_scale - 1.0).abs() > f64::EPSILON
        && state.desc.width != 0
        && state.desc.height != 0
    {
        state.desc.width = scale_dimension(state.desc.width, sc.render_scale);
        state.desc.height = scale_dimension(state.desc.height, sc.render_scale);
    }
    if format != DXGI_FORMAT_UNKNOWN && convert_format(format) != WMT_PIXEL_FORMAT_INVALID {
        state.desc.format = format;
    }
    if state.desc.width == 0 || state.desc.height == 0 {
        state.backbuffer_desc.width = 1;
        state.backbuffer_desc.height = 1;
    } else {
        state.backbuffer_desc.width = u64::from(state.desc.width);
        state.backbuffer_desc.height = state.desc.height;
    }
    sc.apply_layer_props(&state);
    state.backbuffer_desc.format = state.desc.format;
    state.backbuffers.clear();
    let count = if buffer_count == 0 {
        state.desc.buffer_count
    } else {
        state.desc.buffer_count = buffer_count;
        buffer_count
    };
    let mut heap_props = HeapProperties {
        r#type: D3D12_HEAP_TYPE_CUSTOM,
        cpu_page_property: 0,
        memory_pool_preference: 0,
        creation_node_mask: 1,
        visible_node_mask: 1,
    };
    // SAFETY: `sc.device` is a live `Device` and `heap_props` is a local struct.
    unsafe {
        device::features::get_custom_heap_properties(
            sc.device.cast(),
            (&raw mut heap_props).cast(),
            0,
            D3D12_HEAP_TYPE_DEFAULT,
        );
    }
    for _ in 0..count {
        let mut raw_res: *mut c_void = ptr::null_mut();
        // SAFETY: `sc.device` is live, `heap_props` and `state.backbuffer_desc` are valid
        // descriptors, and `raw_res` is a local output pointer.
        let hr = unsafe {
            resource::texture::create(
                sc.device,
                (&heap_props, D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES),
                &state.backbuffer_desc,
                ptr::null(),
                None,
                &[],
                (ptr::from_ref(&IID_ID3D12_RESOURCE).cast(), &raw mut raw_res),
            )
        };
        if hr < 0 || raw_res.is_null() {
            state.backbuffers.clear();
            return E_FAIL;
        }
        // SAFETY: `raw_res` is a non-null `Resource` returned with one public reference.
        let bb = unsafe { Backbuffer::adopt(raw_res.cast::<Resource>()) };
        state.backbuffers.push(bb);
    }
    drop(state);
    S_OK
}

/// `IDXGISwapChain::ResizeTarget` (`d3d12_swapchain.cpp:609-644`).
unsafe fn resize_target(this: *mut c_void, desc_ptr: *const c_void) -> Hresult {
    crate::scope!("swapchain.ResizeTarget");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: `desc_ptr` is null or the application's `DXGI_MODE_DESC`.
    unsafe { fullscreen::resize_target(sc, desc_ptr) }
}

/// `IDXGISwapChain::GetContainingOutput` (`d3d12_swapchain.cpp:687-707`).
unsafe fn get_containing_output(this: *mut c_void, out: *mut *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetContainingOutput");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: `out` is null or the application's.
    unsafe { fullscreen::get_containing_output(sc, out, E_INVALIDARG) }
}

/// `IDXGISwapChain::GetFrameStatistics` (`d3d12_swapchain.cpp:711-720`).
unsafe fn get_frame_statistics(this: *mut c_void, stats: *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetFrameStatistics");
    crate::debug!("DXGISwapChain::GetFrameStatistics: stub");
    if stats.is_null() {
        return E_POINTER;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: `stats` is the application's, non-null.
    unsafe { sc.core.write_frame_statistics(stats) };
    S_OK
}

/// `IDXGISwapChain::GetLastPresentCount` (`d3d12_swapchain.cpp:724-731`).
unsafe fn get_last_present_count(this: *mut c_void, last_present_count: *mut u32) -> Hresult {
    crate::scope!("swapchain.GetLastPresentCount");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: the pointer is the application's.
    unsafe { sc.core.get_last_present_count(last_present_count) }
}

/// `IDXGISwapChain1::GetDesc1` (`d3d12_swapchain.cpp:735-742`).
unsafe fn get_desc1(this: *mut c_void, desc: *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetDesc1");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: `desc` is the application's.
    unsafe { sc.core.get_desc1(desc) }
}

/// `IDXGISwapChain1::GetFullscreenDesc` (`d3d12_swapchain.cpp:746-753`).
unsafe fn get_fullscreen_desc(this: *mut c_void, desc: *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetFullscreenDesc");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: `desc` is the application's.
    unsafe { sc.core.get_fullscreen_desc(desc) }
}

/// `IDXGISwapChain1::GetHwnd` (`d3d12_swapchain.cpp:757-764`).
unsafe fn get_hwnd(this: *mut c_void, hwnd: *mut *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetHwnd");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: `hwnd` is the application's.
    unsafe { sc.core.get_hwnd(hwnd) }
}

/// `IDXGISwapChain1::GetCoreWindow` (`d3d12_swapchain.cpp:768`).
///
/// # Safety
///
/// `this`, `refiid`, and `unk` are caller-supplied pointers.
pub unsafe fn get_core_window(
    _this: *mut c_void,
    _refiid: *const c_void,
    _unk: *mut *mut c_void,
) -> Hresult {
    crate::scope!("swapchain.GetCoreWindow");
    crate::err!("Not implemented");
    E_NOTIMPL
}

/// `IDXGISwapChain1::Present1` (`d3d12_swapchain.cpp:777-815`).
unsafe fn present1(
    this: *mut c_void,
    sync_interval: u32,
    present_flags: u32,
    _present_parameters: *const c_void,
) -> Hresult {
    crate::scope!("swapchain.Present1");
    let _ = size_of::<PresentParameters>();
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let _present = lock(&sc.present_serial);
    let (mut hr, init_refresh_rate, backbuffer) = {
        let mut state = sc.lock_state();
        let hr = if state.desc.width == 0 || state.desc.height == 0 {
            DXGI_STATUS_OCCLUDED
        } else {
            S_OK
        };
        if present_flags & DXGI_PRESENT_TEST != 0 {
            return hr;
        }
        if wsi::is_minimized(sc.core.hwnd) {
            crate::count!("#presents skipped (window minimised)", 1);
            release_semaphore(sc.present_semaphore, 1);
            state.presentation_count = state.presentation_count.wrapping_add(1);
            drop(state);
            crate::stat_frame!();
            minimised_sleep(cfg!(windows) || !wsi::hooked(), sleep_ms);
            return hr;
        }
        let idx = (state.presentation_count as usize) % state.backbuffers.len();
        let bb = state.backbuffers[idx].clone();
        (hr, state.init_refresh_rate, bb)
    };
    let vsync_duration = dxmt_core::swapchain::vsync_duration(
        sync_interval,
        sc.preferred_max_frame_rate,
        init_refresh_rate,
    );
    let refresh = if init_refresh_rate < 1000.0 {
        init_refresh_rate
    } else {
        0.0
    };
    if let Some(presenter) = sc.core.presenter.as_ref() {
        // SAFETY: `sc.queue` points to the parent `Queue`.
        let queue = unsafe { &*sc.queue };
        hr = queue.present(
            presenter,
            backbuffer.as_ref(),
            sc.present_semaphore,
            vsync_duration,
            refresh,
        );
    }
    let mut state = sc.lock_state();
    state.presentation_count = state.presentation_count.wrapping_add(1);
    hr
}

/// `IDXGISwapChain1::GetRestrictToOutput` (`d3d12_swapchain.cpp:825-828`).
fn get_restrict_to_output(_this: *mut c_void, _restrict_to_output: *mut *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetRestrictToOutput");
    crate::not_implemented!("IDXGISwapChain1::GetRestrictToOutput")
}

/// `IDXGISwapChain1::SetBackgroundColor` (`d3d12_swapchain.cpp:832-835`).
fn set_background_color(_this: *mut c_void, _color: *const c_void) -> Hresult {
    crate::scope!("swapchain.SetBackgroundColor");
    let _ = size_of::<Rgba>();
    crate::not_implemented!("IDXGISwapChain1::SetBackgroundColor")
}

/// `IDXGISwapChain1::GetBackgroundColor` (`d3d12_swapchain.cpp:839-842`).
fn get_background_color(_this: *mut c_void, _color: *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetBackgroundColor");
    crate::not_implemented!("IDXGISwapChain1::GetBackgroundColor")
}

/// `IDXGISwapChain1::SetRotation` (`d3d12_swapchain.cpp:846-849`).
fn set_rotation(_this: *mut c_void, _rotation: u32) -> Hresult {
    crate::scope!("swapchain.SetRotation");
    crate::not_implemented!("IDXGISwapChain1::SetRotation")
}

/// `IDXGISwapChain1::GetRotation` (`d3d12_swapchain.cpp:853-856`).
fn get_rotation(_this: *mut c_void, _rotation: *mut u32) -> Hresult {
    crate::scope!("swapchain.GetRotation");
    crate::not_implemented!("IDXGISwapChain1::GetRotation")
}

/// `IDXGISwapChain2::SetSourceSize` (`d3d12_swapchain.cpp:859-863`).
fn set_source_size(_this: *mut c_void, _width: u32, _height: u32) -> Hresult {
    crate::scope!("swapchain.SetSourceSize");
    crate::not_implemented!("IDXGISwapChain2::SetSourceSize", S_OK)
}

/// `IDXGISwapChain2::GetSourceSize` (`d3d12_swapchain.cpp:866-870`).
fn get_source_size(_this: *mut c_void, _width: *mut u32, _height: *mut u32) -> Hresult {
    crate::scope!("swapchain.GetSourceSize");
    crate::not_implemented!("IDXGISwapChain2::GetSourceSize", S_OK)
}

/// `IDXGISwapChain2::SetMaximumFrameLatency` (`d3d12_swapchain.cpp:873-884`).
unsafe fn set_maximum_frame_latency(this: *mut c_void, max_latency: u32) -> Hresult {
    crate::scope!("swapchain.SetMaximumFrameLatency");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    sc.core.set_maximum_frame_latency(max_latency, |delta| {
        release_semaphore(sc.present_semaphore, delta);
    })
}

/// `IDXGISwapChain2::GetMaximumFrameLatency` (`d3d12_swapchain.cpp:887-893`).
unsafe fn get_maximum_frame_latency(this: *mut c_void, max_latency: *mut u32) -> Hresult {
    crate::scope!("swapchain.GetMaximumFrameLatency");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: the pointer is the application's.
    unsafe { sc.core.get_maximum_frame_latency(max_latency) }
}

/// `IDXGISwapChain2::GetFrameLatencyWaitableObject` (`d3d12_swapchain.cpp:896-910`).
unsafe fn get_frame_latency_waitable_object(this: *mut c_void) -> *mut c_void {
    crate::scope!("swapchain.GetFrameLatencyWaitableObject");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    sc.core
        .frame_latency_waitable_object(|| duplicate_same_access(sc.present_semaphore))
}

/// `IDXGISwapChain3::GetCurrentBackBufferIndex` (`d3d12_swapchain.cpp:936-939`).
unsafe fn get_current_back_buffer_index(this: *mut c_void) -> u32 {
    crate::scope!("swapchain.GetCurrentBackBufferIndex");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let state = sc.lock_state();
    let count = u32::try_from(state.backbuffers.len()).unwrap_or(1).max(1);
    state.presentation_count % count
}

/// `IDXGISwapChain3::CheckColorSpaceSupport` (`d3d12_swapchain.cpp:925-933`).
unsafe fn check_color_space_support(
    _this: *mut c_void,
    color_space: u32,
    color_space_support: *mut u32,
) -> Hresult {
    crate::scope!("swapchain.CheckColorSpaceSupport");
    // SAFETY: `color_space_support` is null or the application's.
    unsafe { dxmt_core::swapchain::check_color_space_support(color_space, color_space_support) }
}

/// `IDXGISwapChain3::SetColorSpace1` (`d3d12_swapchain.cpp:953-961`).
unsafe fn set_color_space1(this: *mut c_void, color_space: u32) -> Hresult {
    crate::scope!("swapchain.SetColorSpace1");
    // SAFETY: `this` is a live `SwapChain`.
    let _ = unsafe { &*this.cast::<SwapChain>() }
        .core
        .set_color_space1(color_space);
    S_OK
}

/// Static `IDXGISwapChain4` vtable for [`SwapChain`].
impl HasPrivateData for SwapChain {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `SwapChain`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

pub static SWAPCHAIN_VTBL: IDXGISwapChain4Vtbl = IDXGISwapChain4Vtbl {
    query_interface: slot!(
        (this: *mut c_void, riid: *const c_void, ppv_object: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { query_interface(this, riid, ppv_object) }
        }
    ),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `SwapChain` COM pointer.
        unsafe { com::add_ref_com(this.cast::<SwapChain>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `SwapChain` COM pointer.
        unsafe { com::release_com(this.cast::<SwapChain>()) }
    }),
    set_private_data: private_data::set_private_data::<SwapChain>,
    set_private_data_interface: private_data::set_private_data_interface::<SwapChain>,
    get_private_data: private_data::get_private_data::<SwapChain>,
    get_parent: slot!(
        (this: *mut c_void, riid: *const c_void, parent: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { get_parent(this, riid, parent) }
        }
    ),
    get_device: slot!(
        (this: *mut c_void, riid: *const c_void, device: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { get_device(this, riid, device) }
        }
    ),
    present: slot!((this: *mut c_void, sync_interval: u32, flags: u32) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { present(this, sync_interval, flags) }
    }),
    get_buffer: slot!(
        (this: *mut c_void, buffer: u32, riid: *const c_void, surface: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { get_buffer(this, buffer, riid, surface) }
        }
    ),
    set_fullscreen_state: slot!(
        (this: *mut c_void, fullscreen: i32, target: *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { set_fullscreen_state(this, fullscreen, target) }
        }
    ),
    get_fullscreen_state: slot!(
        (this: *mut c_void, fullscreen: *mut i32, target: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { get_fullscreen_state(this, fullscreen, target) }
        }
    ),
    get_desc: slot!((this: *mut c_void, desc: *mut c_void) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { get_desc(this, desc) }
    }),
    resize_buffers: slot!(
        (
            this: *mut c_void,
            buffer_count: u32,
            width: u32,
            height: u32,
            new_format: u32,
            swap_chain_flags: u32
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { resize_buffers(this, buffer_count, width, height, new_format, swap_chain_flags) }
        }
    ),
    resize_target: slot!(
        (this: *mut c_void, new_target_parameters: *const c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { resize_target(this, new_target_parameters) }
        }
    ),
    get_containing_output: slot!(
        (this: *mut c_void, output: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { get_containing_output(this, output) }
        }
    ),
    get_frame_statistics: slot!((this: *mut c_void, stats: *mut c_void) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { get_frame_statistics(this, stats) }
    }),
    get_last_present_count: slot!(
        (this: *mut c_void, last_present_count: *mut u32) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { get_last_present_count(this, last_present_count) }
        }
    ),
    get_desc1: slot!((this: *mut c_void, desc: *mut c_void) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { get_desc1(this, desc) }
    }),
    get_fullscreen_desc: slot!((this: *mut c_void, desc: *mut c_void) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { get_fullscreen_desc(this, desc) }
    }),
    get_hwnd: slot!((this: *mut c_void, hwnd: *mut *mut c_void) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { get_hwnd(this, hwnd) }
    }),
    get_core_window: slot!(
        (this: *mut c_void, refiid: *const c_void, unk: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { get_core_window(this, refiid, unk) }
        }
    ),
    present1: slot!(
        (
            this: *mut c_void,
            sync_interval: u32,
            present_flags: u32,
            present_parameters: *const c_void
        ) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { present1(this, sync_interval, present_flags, present_parameters) }
        }
    ),
    is_temporary_mono_supported: slot!(
        "swapchain.IsTemporaryMonoSupported",
        (_this: *mut c_void) -> i32 => 0
    ),
    get_restrict_to_output: slot!(
        (this: *mut c_void, restrict_to_output: *mut *mut c_void) -> Hresult => {
            get_restrict_to_output(this, restrict_to_output)
        }
    ),
    set_background_color: slot!((this: *mut c_void, color: *const c_void) -> Hresult => {
        set_background_color(this, color)
    }),
    get_background_color: slot!((this: *mut c_void, color: *mut c_void) -> Hresult => {
        get_background_color(this, color)
    }),
    set_rotation: slot!((this: *mut c_void, rotation: u32) -> Hresult => {
        set_rotation(this, rotation)
    }),
    get_rotation: slot!((this: *mut c_void, rotation: *mut u32) -> Hresult => {
        get_rotation(this, rotation)
    }),
    set_source_size: slot!((this: *mut c_void, width: u32, height: u32) -> Hresult => {
        set_source_size(this, width, height)
    }),
    get_source_size: slot!((this: *mut c_void, width: *mut u32, height: *mut u32) -> Hresult => {
        get_source_size(this, width, height)
    }),
    set_maximum_frame_latency: slot!((this: *mut c_void, max_latency: u32) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { set_maximum_frame_latency(this, max_latency) }
    }),
    get_maximum_frame_latency: slot!((this: *mut c_void, max_latency: *mut u32) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { get_maximum_frame_latency(this, max_latency) }
    }),
    get_frame_latency_waitable_object: slot!((this: *mut c_void) -> *mut c_void => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { get_frame_latency_waitable_object(this) }
    }),
    set_matrix_transform: slot!(
        "swapchain.SetMatrixTransform",
        (_this: *mut c_void, _matrix: *const c_void) -> Hresult => {
            let _ = size_of::<Matrix3x2f>();
            DXGI_ERROR_INVALID_CALL
        }
    ),
    get_matrix_transform: slot!(
        "swapchain.GetMatrixTransform",
        (_this: *mut c_void, _matrix: *mut c_void) -> Hresult => DXGI_ERROR_INVALID_CALL
    ),
    get_current_back_buffer_index: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { get_current_back_buffer_index(this) }
    }),
    check_color_space_support: slot!(
        (this: *mut c_void, color_space: u32, color_space_support: *mut u32) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { check_color_space_support(this, color_space, color_space_support) }
        }
    ),
    resize_buffers1: slot!(
        "swapchain.ResizeBuffers1",
        (
            this: *mut c_void,
            buffer_count: u32,
            width: u32,
            height: u32,
            format: u32,
            swap_chain_flags: u32,
            _creation_node_mask: *const u32,
            _present_queue: *const *mut c_void
        ) -> Hresult => {
            crate::warn!("DXGISwapChain3::ResizeBuffers1: ignoring d3d12 related parameters");
            // SAFETY: caller forwards COM slot arguments.
            unsafe { resize_buffers(this, buffer_count, width, height, format, swap_chain_flags) }
        }
    ),
    set_color_space1: slot!((this: *mut c_void, color_space: u32) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { set_color_space1(this, color_space) }
    }),
    set_hdr_meta_data: slot!(
        "swapchain.SetHDRMetaData",
        (_this: *mut c_void, _type: u32, _size: u32, _meta_data: *mut c_void) -> Hresult => S_OK
    ),
};

/// Creates a D3D12 [`SwapChain`] (`CreateSwapChain`, `d3d12_swapchain.cpp:981`).
///
/// # Safety
///
/// `factory`, `hwnd`, `desc`, `fullscreen_desc`, and `out` are caller-supplied pointers.
pub unsafe fn create(
    factory: *mut c_void,
    device: &Device,
    queue: &Queue,
    hwnd: *mut c_void,
    desc: *const c_void,
    fullscreen_desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    static CONFIG: LazyLock<Config> = LazyLock::new(|| Config::load(&crate::LOG));

    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` was checked non-null.
    let mut sc_desc = unsafe { desc.cast::<SwapChainDesc1>().read_unaligned() };
    let fs_desc = if fullscreen_desc.is_null() {
        SwapChainFullscreenDesc {
            refresh_rate: WsiRational {
                numerator: 0,
                denominator: 0,
            },
            scanline_ordering: 0,
            scaling: 0,
            windowed: 1,
        }
    } else {
        // SAFETY: `fullscreen_desc` is non-null.
        unsafe {
            fullscreen_desc
                .cast::<SwapChainFullscreenDesc>()
                .read_unaligned()
        }
    };

    let mtl_dev_handle = device.mtl.as_ref().map_or(0, Obj::handle);
    let mut layer_weak = 0_u64;
    // SAFETY: `hwnd` and `mtl_dev_handle` are forwarded to `CreateMetalViewFromHWND`.
    let native_view = unsafe {
        CreateMetalViewFromHWND(
            hwnd.expose_provenance().cast_signed(),
            mtl_dev_handle,
            &raw mut layer_weak,
        )
    };
    if native_view == 0 {
        crate::err!(
            "Failed to create metal view, it seems like your Wine has no exported symbols needed by DXMT."
        );
        std::process::abort();
    }

    let library = device.mtl.as_ref().and_then(|d| match Library::new(d) {
        Ok(lib) => Some(lib),
        Err(err) => {
            crate::err!("Failed to create internal command library: {err}");
            None
        }
    });
    let lib_handle = library
        .as_ref()
        .or(device.library.as_ref())
        .map_or(0, library_handle);
    let presenter_desc = PresenterDesc {
        device: mtl_dev_handle,
        layer: layer_weak,
        library: lib_handle,
        scale_factor: 1.0,
        sample_count: u8::try_from(sc_desc.sample_desc.count).unwrap_or(1),
    };
    // SAFETY: `mtl_dev_handle`, `layer_weak`, and `lib_handle` outlive the presenter.
    let presenter = Some(Arc::new(unsafe { Presenter::new(&presenter_desc) }));

    let present_semaphore = create_semaphore(
        i32::try_from(SWAPCHAIN_LATENCY).unwrap_or(1),
        i32::try_from(DXGI_MAX_SWAP_CHAIN_BUFFERS).unwrap_or(16),
    );

    if sc_desc.width == 0 || sc_desc.height == 0 {
        let (w, h) = wsi::window_size(hwnd);
        sc_desc.width = w;
        sc_desc.height = h;
    }

    let preferred_max_frame_rate = CONFIG.get_i32("d3d12.preferredMaxFrameRate", 0);

    let mut hud = Hud::instance();
    hud.initialize(&dxmt_core::info::version_text(
        12,
        device.min_feature_level,
        env!("DXMT_VERSION"),
    ));

    let mut flags = D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;
    if sc_desc.buffer_usage & DXGI_USAGE_UNORDERED_ACCESS != 0 {
        flags |= D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
    }
    let backbuffer_desc = ResourceDesc {
        dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        alignment: D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
        width: u64::from(sc_desc.width),
        height: sc_desc.height,
        depth_or_array_size: 1,
        mip_levels: 1,
        format: sc_desc.format,
        sample_desc: D3d12SampleDesc {
            count: 1,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        flags,
    };

    let initial_width = sc_desc.width;
    let initial_height = sc_desc.height;
    let mut state = State {
        monitor: wsi::window_monitor(hwnd),
        target: ptr::null_mut(),
        window_state: WindowState::default(),
        presentation_count: 0,
        desc: sc_desc,
        fullscreen_desc: fs_desc,
        frame_latency: SWAPCHAIN_LATENCY,
        color_space: DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
        init_refresh_rate: f64::MAX,
        api: Api {
            backbuffer_desc,
            backbuffers: Vec::new(),
        },
    };
    read_refresh_rate(&mut state);

    // SAFETY: retains `factory` for the swapchain's lifetime (`Com<IDXGIFactory1> factory_`).
    unsafe { dxmt_util::com::add_ref(factory) };

    let raw_sc = Box::into_raw(Box::new(SwapChain {
        vtbl: &SWAPCHAIN_VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: ptr::from_ref(device).cast_mut(),
        queue: ptr::from_ref(queue),
        hud: Some(hud),
        factory,
        native_view,
        present_semaphore,
        render_scale: render_scale(),
        preferred_max_frame_rate,
        library,
        core: Core::new(hwnd, layer_weak, presenter, state),
        present_serial: Mutex::new(()),
    }));

    // SAFETY: `raw_sc` is a live `SwapChain` allocated above.
    let resize_hr = unsafe {
        resize_buffers(
            raw_sc.cast(),
            0,
            initial_width,
            initial_height,
            DXGI_FORMAT_UNKNOWN,
            0,
        )
    };
    let hr = if resize_hr < 0 {
        resize_hr
    } else {
        // SAFETY: `raw_sc` is live and `out` is the caller's output pointer.
        unsafe {
            query_interface(
                raw_sc.cast(),
                ptr::from_ref(&IID_IDXGI_SWAP_CHAIN1).cast(),
                out,
            )
        }
    };
    // SAFETY: releases the initial reference from `ComObject::with_initial_ref()`.
    unsafe { release_com(raw_sc) };
    hr
}

#[cfg(test)]
mod tests;
