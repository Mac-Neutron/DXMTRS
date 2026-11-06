//! `IDXGISwapChain4` (`d3d12_swapchain.cpp`): D3D12 flip-model swap chain over
//!
//! [`dxmt_core::presenter::Presenter`].

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use dxmt_core::{
    hud::Hud,
    library::Library,
    presenter::{Presenter, PresenterDesc},
    swapchain::{convert_color_space, convert_format, monitor_format_bpp, render_scale},
};
use dxmt_util::{
    com::{ComObject, Guid, IID_IUNKNOWN, IUnknownVtbl, vtbl, write_out},
    config::Config,
    dxgi_vtbl::IDXGISwapChain4Vtbl,
    private_data::{self, HasPrivateData, PrivateData},
    sync::lock,
    wsi::{self, Rational as WsiRational, Rect, WindowState, WsiMode},
};
use winemetal_sys::{
    CGColorSpace_checkColorSpaceSupported, CreateMetalViewFromHWND, MetalLayer_getEDRValue, Obj,
    ReleaseMetalView,
    wire::{WMT_PIXEL_FORMAT_INVALID, WMTEDRValue},
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
            AdapterVtbl, D3D12_HEAP_FLAG_ALLOW_ONLY_RT_DS_TEXTURES,
            DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
            DXGI_ERROR_NOT_CURRENTLY_AVAILABLE, DXGI_FORMAT_R16G16B16A16_FLOAT,
            DXGI_MAX_SWAP_CHAIN_BUFFERS, DXGI_MODE_SCALING_UNSPECIFIED,
            DXGI_MODE_SCANLINE_ORDER_LOWER_FIELD_FIRST, DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
            DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST, DXGI_PRESENT_TEST,
            DXGI_STATUS_MODE_CHANGE_IN_PROGRESS, DXGI_STATUS_OCCLUDED,
            DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT,
            DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH,
            DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT, DXGI_USAGE_UNORDERED_ACCESS,
            FrameStatistics, Hmonitor, Hwnd, IID_IDXGI_ADAPTER, IID_IDXGI_OUTPUT1, Matrix3x2f,
            ModeDesc, ModeDesc1, OutputDesc, OutputVtbl, PresentParameters, Rgba, SwapChainDesc,
            SwapChainDesc1, SwapChainFullscreenDesc,
        },
    },
    com::{
        self, ComRefCounted, DXGI_ERROR_INVALID_CALL, DXGI_ERROR_NOT_FOUND, E_FAIL, E_INVALIDARG,
        E_NOINTERFACE, E_NOTIMPL, E_POINTER, Hresult, IID_ID3D12_RESOURCE,
        IID_IDXGI_DEVICE_SUB_OBJECT, IID_IDXGI_OBJECT, IID_IDXGI_SWAP_CHAIN, IID_IDXGI_SWAP_CHAIN1,
        IID_IDXGI_SWAP_CHAIN2, IID_IDXGI_SWAP_CHAIN3, IID_IDXGI_SWAP_CHAIN4, S_OK, init_out_ptr,
        release_com,
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

/// Log message emitted when leaving fullscreen while the process is in the background.
const BACKGROUND_LEAVE_LOG: &str = "Leaving fullscreen in the background: minimising";

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

/// Minimises `hwnd` when leaving fullscreen in the background (`d3d12_swapchain.cpp:420-423`).
fn leave_fullscreen_background<IsFg, Log, Min>(
    hwnd: Hwnd,
    switching_output: bool,
    is_foreground: IsFg,
    log: Log,
    minimize: Min,
) where
    IsFg: FnOnce() -> bool,
    Log: FnOnce(&'static str),
    Min: FnOnce(Hwnd),
{
    if !switching_output && !is_foreground() {
        log(BACKGROUND_LEAVE_LOG);
        minimize(hwnd);
    }
}

/// RAII guard for `ModeSetGuard` (`d3d12_swapchain.cpp:124-144`).
struct ModeSetInProgress<'a> {
    flag: &'a AtomicBool,
    already_in_progress: bool,
}

impl<'a> ModeSetInProgress<'a> {
    fn enter(flag: &'a AtomicBool) -> Self {
        let already_in_progress = flag.swap(true, Ordering::SeqCst);
        Self {
            flag,
            already_in_progress,
        }
    }
}

impl Drop for ModeSetInProgress<'_> {
    fn drop(&mut self) {
        if !self.already_in_progress {
            self.flag.store(false, Ordering::SeqCst);
        }
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

/// Mutable swap-chain state protected by `SwapChain::state`.
struct State {
    monitor: Hmonitor,
    target: *mut c_void,
    saved_window: WindowState,
    presentation_count: u32,
    desc: SwapChainDesc1,
    backbuffer_desc: ResourceDesc,
    fullscreen_desc: SwapChainFullscreenDesc,
    frame_latency: u32,
    colorspace: u32,
    init_refresh_rate: f64,
    backbuffers: Vec<Backbuffer>,
}

// SAFETY: `monitor` and `target` are thread-safe OS/COM handles protected by `SwapChain::state`.
unsafe impl Send for State {}

impl State {
    /// Updates `init_refresh_rate` from `monitor`'s current display mode (`ReadRefreshRate`,
    ///
    /// `d3d12_swapchain.cpp:488-492`).
    fn read_refresh_rate(&mut self) {
        if let Some(mode) = wsi::current_display_mode(self.monitor)
            && mode.refresh.denominator != 0
            && mode.refresh.numerator != 0
        {
            self.init_refresh_rate =
                f64::from(mode.refresh.numerator) / f64::from(mode.refresh.denominator);
        }
    }

    /// Replaces `self.target`, releasing the previous `IDXGIOutput1` reference if any.
    unsafe fn set_target(&mut self, new_target: *mut c_void) {
        let old = core::mem::replace(&mut self.target, new_target);
        // SAFETY: `self.target` owns a COM reference when non-null.
        unsafe { dxmt_util::com::release(old) };
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
    pub presenter: Option<Arc<Presenter>>,
    pub hud: Option<Hud>,
    factory: *mut c_void,
    hwnd: Hwnd,
    native_view: u64,
    layer_weak: u64,
    present_semaphore: *mut c_void,
    render_scale: f64,
    preferred_max_frame_rate: i32,
    modeset_in_progress: AtomicBool,
    library: Option<Library>,
    state: Mutex<State>,
    /// Present calls are serialised even while the queue lock is released for their waits.
    present_serial: Mutex<()>,
}

// SAFETY: `SwapChain` synchronises mutable state with `Mutex`/`AtomicBool`, and its raw pointers
// (`device`, `queue`, `factory`, `hwnd`, `present_semaphore`) are thread-safe COM/OS handles.
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
        let state = self.state.get_mut().unwrap_or_else(PoisonError::into_inner);
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

const fn empty_output_desc() -> OutputDesc {
    OutputDesc {
        device_name: [0; 32],
        desktop_coordinates: Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        attached_to_desktop: 0,
        rotation: 0,
        monitor: ptr::null_mut(),
    }
}

const fn copy_mode_desc1(m: &ModeDesc1) -> ModeDesc1 {
    ModeDesc1 {
        width: m.width,
        height: m.height,
        refresh_rate: m.refresh_rate,
        format: m.format,
        scanline_ordering: m.scanline_ordering,
        scaling: m.scaling,
        stereo: m.stereo,
    }
}

const fn copy_desc1(d: &SwapChainDesc1) -> SwapChainDesc1 {
    SwapChainDesc1 {
        width: d.width,
        height: d.height,
        format: d.format,
        stereo: d.stereo,
        sample_desc: d.sample_desc,
        buffer_usage: d.buffer_usage,
        buffer_count: d.buffer_count,
        scaling: d.scaling,
        swap_effect: d.swap_effect,
        alpha_mode: d.alpha_mode,
        flags: d.flags,
    }
}

const fn copy_fullscreen_desc(d: &SwapChainFullscreenDesc) -> SwapChainFullscreenDesc {
    SwapChainFullscreenDesc {
        refresh_rate: d.refresh_rate,
        scanline_ordering: d.scanline_ordering,
        scaling: d.scaling,
        windowed: d.windowed,
    }
}

const fn copy_window_state(w: &WindowState) -> WindowState {
    WindowState {
        style: w.style,
        exstyle: w.exstyle,
        rect: w.rect,
    }
}

impl SwapChain {
    fn lock_state(&self) -> std::sync::MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// `LayerSupportEDR` (`d3d12_swapchain.cpp:176-180`).
    fn layer_support_edr(&self) -> bool {
        let mut edr_value = WMTEDRValue {
            maximum_edr_color_component_value: 1.0,
            maximum_potential_edr_color_component_value: 1.0,
        };
        // SAFETY: `self.layer_weak` is the Metal layer handle from `CreateMetalViewFromHWND`.
        unsafe { MetalLayer_getEDRValue(self.layer_weak, &raw mut edr_value) };
        edr_value.maximum_potential_edr_color_component_value > 1.0
    }

    /// `ApplyLayerProps` (`d3d12_swapchain.cpp:647-658`).
    fn apply_layer_props(&self, state: &State) {
        let cs = if state.desc.format == DXGI_FORMAT_R16G16B16A16_FLOAT {
            DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709
        } else {
            state.colorspace
        };
        let target_color_space = convert_color_space(cs, self.layer_support_edr());
        if let Some(presenter) = self.presenter.as_ref() {
            let _ = presenter.change_layer_properties(
                convert_format(state.desc.format),
                target_color_space,
                f64::from(state.desc.width),
                f64::from(state.desc.height),
                u8::try_from(state.desc.sample_desc.count).unwrap_or(1),
            );
        }
    }

    /// `GetOutputFromMonitor` (`d3d12_swapchain.cpp:660-683`).
    unsafe fn get_output_from_monitor(&self, monitor: Hmonitor, out: *mut *mut c_void) -> Hresult {
        if out.is_null() {
            return DXGI_ERROR_INVALID_CALL;
        }
        // SAFETY: `self.device` points to the parent `Device`.
        let dev = unsafe { &*self.device };
        // SAFETY: `dev.adapter` is the device's `IMTLDXGIAdapter` COM pointer.
        let Some(adapter) = (unsafe { dxmt_util::com::query(dev.adapter, &IID_IDXGI_ADAPTER) })
        else {
            return E_FAIL;
        };
        let mut index = 0_u32;
        let hr = loop {
            let mut output: *mut c_void = ptr::null_mut();
            // SAFETY: `adapter` is a live `IDXGIAdapter` queried above.
            let adapter_vtbl = unsafe { vtbl::<AdapterVtbl>(adapter) };
            // SAFETY: `adapter` is live and `output` is a local output pointer.
            let enum_hr = unsafe { (adapter_vtbl.enum_outputs)(adapter, index, &raw mut output) };
            if enum_hr < 0 || output.is_null() {
                break DXGI_ERROR_NOT_FOUND;
            }
            let mut desc = empty_output_desc();
            // SAFETY: `output` is a live `IDXGIOutput` returned by `enum_outputs`.
            let output_vtbl = unsafe { vtbl::<OutputVtbl>(output) };
            // SAFETY: `output` is live and `desc` is a local `OutputDesc`.
            unsafe { (output_vtbl.get_desc)(output, &raw mut desc) };
            if desc.monitor == monitor {
                // SAFETY: `output` is live and `out` is the caller's non-null output slot.
                let qi_hr = unsafe {
                    (output_vtbl.object.unknown.query_interface)(output, &IID_IDXGI_OUTPUT1, out)
                };
                // SAFETY: releases the `enum_outputs` reference on `output`.
                unsafe { dxmt_util::com::release(output) };
                break qi_hr;
            }
            // SAFETY: releases the `enum_outputs` reference on `output`.
            unsafe { dxmt_util::com::release(output) };
            index = index.wrapping_add(1);
        };
        // SAFETY: releases the `query` reference on `adapter`.
        unsafe { dxmt_util::com::release(adapter) };
        hr
    }

    /// `ChangeDisplayMode` (`d3d12_swapchain.cpp:431-481`).
    unsafe fn change_display_mode(
        &self,
        output: *mut c_void,
        display_mode: &mut ModeDesc1,
    ) -> Hresult {
        if output.is_null() {
            return DXGI_ERROR_INVALID_CALL;
        }
        let (desc_flags, desc_format) = {
            let state = self.lock_state();
            (state.desc.flags, state.desc.format)
        };
        let mut preferred_mode = copy_mode_desc1(display_mode);
        let mut selected_mode = ModeDesc1::default();
        if desc_flags & DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH == 0 {
            preferred_mode.width = 0;
            preferred_mode.height = 0;
        }
        if preferred_mode.format == DXGI_FORMAT_UNKNOWN {
            preferred_mode.format = desc_format;
        }
        // SAFETY: `output` is a live `IDXGIOutput1` pointer.
        let output_vtbl = unsafe { vtbl::<OutputVtbl>(output) };
        // SAFETY: `output` is live; `preferred_mode` and `selected_mode` are local structs.
        let hr = unsafe {
            (output_vtbl.find_closest_matching_mode1)(
                output,
                &raw const preferred_mode,
                &raw mut selected_mode,
                ptr::null_mut(),
            )
        };
        if hr < 0 {
            let hz = preferred_mode.refresh_rate.numerator
                / preferred_mode.refresh_rate.denominator.max(1);
            crate::err!(
                "DXGI: Failed to query closest mode:\n  Format: {}\n  Mode:   {}x{}@{}",
                preferred_mode.format,
                preferred_mode.width,
                preferred_mode.height,
                hz
            );
            return hr;
        }
        if selected_mode.refresh_rate.denominator == 0 {
            selected_mode.refresh_rate.denominator = 1;
        }
        let mut output_desc = empty_output_desc();
        // SAFETY: `output` is live and `output_desc` is a local `OutputDesc`.
        unsafe { (output_vtbl.get_desc)(output, &raw mut output_desc) };
        let wsi_mode = WsiMode {
            width: selected_mode.width,
            height: selected_mode.height,
            refresh: WsiRational {
                numerator: selected_mode.refresh_rate.numerator,
                denominator: selected_mode.refresh_rate.denominator,
            },
            bits_per_pixel: monitor_format_bpp(selected_mode.format),
            interlaced: selected_mode.scanline_ordering
                == DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST
                || selected_mode.scanline_ordering == DXGI_MODE_SCANLINE_ORDER_LOWER_FIELD_FIRST,
        };
        let hz = if wsi_mode.refresh.numerator != 0 {
            wsi_mode.refresh.numerator / wsi_mode.refresh.denominator.max(1)
        } else {
            0
        };
        if !wsi::set_window_mode(output_desc.monitor, self.hwnd, &wsi_mode) {
            return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
        }
        crate::info!(
            "Setting display mode: {}x{}@{}",
            wsi_mode.width,
            wsi_mode.height,
            hz
        );
        *display_mode = copy_mode_desc1(&selected_mode);
        self.lock_state().init_refresh_rate = f64::from(selected_mode.refresh_rate.numerator)
            / f64::from(selected_mode.refresh_rate.denominator);
        S_OK
    }

    /// `RestoreDisplayMode` (`d3d12_swapchain.cpp:494-503`).
    fn restore_display_mode(monitor: Hmonitor) -> Hresult {
        if monitor.is_null() {
            return DXGI_ERROR_INVALID_CALL;
        }
        if !wsi::restore_display_mode(monitor) {
            return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
        }
        S_OK
    }

    /// `EnterFullscreenMode` (`d3d12_swapchain.cpp:331-387`).
    ///
    /// Consumes the caller's reference on `target_in` (when non-null).
    unsafe fn enter_fullscreen_mode(&self, target_in: *mut c_void) -> Hresult {
        let guard = ModeSetInProgress::enter(&self.modeset_in_progress);
        if guard.already_in_progress {
            // SAFETY: drops the caller's reference on `target_in`.
            unsafe { dxmt_util::com::release(target_in) };
            return DXGI_STATUS_MODE_CHANGE_IN_PROGRESS;
        }
        if !wsi::is_window(self.hwnd) {
            // SAFETY: drops the caller's reference on `target_in`.
            unsafe { dxmt_util::com::release(target_in) };
            return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
        }
        let mut output = target_in;
        if output.is_null() {
            // SAFETY: `output` is a local pointer receiving a new `IDXGIOutput1` reference.
            let hr = unsafe {
                self.get_output_from_monitor(wsi::window_monitor(self.hwnd), &raw mut output)
            };
            if hr < 0 {
                crate::err!("DXGI: EnterFullscreenMode: Cannot query containing output");
                return E_FAIL;
            }
        }
        let mut saved_window = copy_window_state(&self.lock_state().saved_window);
        wsi::save_window_state(self.hwnd, &mut saved_window);
        let mut preferred_display_mode = {
            let mut state = self.lock_state();
            state.saved_window = copy_window_state(&saved_window);
            ModeDesc1 {
                width: state.desc.width,
                height: state.desc.height,
                refresh_rate: state.fullscreen_desc.refresh_rate,
                format: state.desc.format,
                scanline_ordering: DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
                scaling: DXGI_MODE_SCALING_UNSPECIFIED,
                stereo: 0,
            }
        };
        // SAFETY: `output` is a non-null `IDXGIOutput1` owned by this scope.
        let mode_hr = unsafe { self.change_display_mode(output, &mut preferred_display_mode) };
        if mode_hr < 0 {
            crate::err!("DXGI: EnterFullscreenMode: Failed to change display mode");
            // SAFETY: releases `output` on error.
            unsafe { dxmt_util::com::release(output) };
            return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
        }
        let mut out_desc = empty_output_desc();
        // SAFETY: `output` is a live `IDXGIOutput1`.
        let output_vtbl = unsafe { vtbl::<OutputVtbl>(output) };
        // SAFETY: `output` is live and `out_desc` is a local `OutputDesc`.
        unsafe { (output_vtbl.get_desc)(output, &raw mut out_desc) };
        let mode_switch = {
            let mut state = self.lock_state();
            state.fullscreen_desc.windowed = 0;
            let mode_switch = (state.desc.flags & DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH) != 0;
            state.monitor = out_desc.monitor;
            // SAFETY: transfers ownership of `output` into `state.target`.
            unsafe { state.set_target(output) };
            mode_switch
        };
        let enter_ok =
            wsi::enter_fullscreen_mode(out_desc.monitor, self.hwnd, &mut saved_window, mode_switch);
        self.lock_state().saved_window = saved_window;
        if !enter_ok {
            crate::err!("DXGI: EnterFullscreenMode: Failed to enter fullscreen mode");
            return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
        }
        S_OK
    }

    /// `LeaveFullscreenMode` (`d3d12_swapchain.cpp:390-428`).
    unsafe fn leave_fullscreen_mode(&self, switching_output: bool) -> Hresult {
        let guard = ModeSetInProgress::enter(&self.modeset_in_progress);
        if guard.already_in_progress {
            return DXGI_STATUS_MODE_CHANGE_IN_PROGRESS;
        }
        let old_monitor = self.lock_state().monitor;
        if Self::restore_display_mode(old_monitor) < 0 {
            crate::warn!("DXGI: LeaveFullscreenMode: Failed to restore display mode");
        }
        let new_monitor = wsi::window_monitor(self.hwnd);
        let saved_window = {
            let mut state = self.lock_state();
            state.fullscreen_desc.windowed = 1;
            // SAFETY: releases the held `IDXGIOutput1` reference if any.
            unsafe { state.set_target(ptr::null_mut()) };
            state.monitor = new_monitor;
            state.read_refresh_rate();
            copy_window_state(&state.saved_window)
        };
        if !wsi::is_window(self.hwnd) {
            return S_OK;
        }
        if !wsi::leave_fullscreen_mode(self.hwnd, &saved_window, true) {
            crate::err!("DXGI: LeaveFullscreenMode: Failed to exit fullscreen mode");
            return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
        }
        leave_fullscreen_background(
            self.hwnd,
            switching_output,
            wsi::is_process_foreground,
            |msg| crate::info!("{msg}"),
            wsi::minimize_window,
        );
        if let Some(presenter) = self.presenter.as_ref() {
            presenter.change_gamma_ramp(None);
        }
        S_OK
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
unsafe fn set_fullscreen_state(
    this: *mut c_void,
    fullscreen: i32,
    target_in: *mut c_void,
) -> Hresult {
    crate::scope!("swapchain.SetFullscreenState");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let want_fullscreen = fullscreen != 0;
    let mut target: *mut c_void = ptr::null_mut();
    if !target_in.is_null() {
        // SAFETY: `target_in` is a caller-supplied `IDXGIOutput` COM pointer.
        if let Some(q) = unsafe { dxmt_util::com::query(target_in, &IID_IDXGI_OUTPUT1) } {
            target = q;
            let mut desc = empty_output_desc();
            // SAFETY: `target` is a live `IDXGIOutput1`.
            let output_vtbl = unsafe { vtbl::<OutputVtbl>(target) };
            // SAFETY: `target` is live and `desc` is a local `OutputDesc`.
            unsafe { (output_vtbl.get_desc)(target, &raw mut desc) };
            let need_switch = {
                let state = sc.lock_state();
                state.fullscreen_desc.windowed == 0
                    && want_fullscreen
                    && state.monitor != desc.monitor
            };
            if need_switch {
                // SAFETY: `sc` is live.
                let hr = unsafe { sc.leave_fullscreen_mode(true) };
                if hr < 0 {
                    // SAFETY: drops the queried `target` reference on early return.
                    unsafe { dxmt_util::com::release(target) };
                    return hr;
                }
            }
        }
    }
    let currently_windowed = sc.lock_state().fullscreen_desc.windowed != 0;
    if currently_windowed && want_fullscreen {
        // SAFETY: `enter_fullscreen_mode` adopts ownership of `target`.
        return unsafe { sc.enter_fullscreen_mode(target) };
    }
    // SAFETY: drops `target` when not entering fullscreen.
    unsafe { dxmt_util::com::release(target) };
    if !currently_windowed && !want_fullscreen {
        // SAFETY: `sc` is live.
        return unsafe { sc.leave_fullscreen_mode(false) };
    }
    S_OK
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
    let state = sc.lock_state();
    if !fullscreen.is_null() {
        let is_fs = i32::from(state.fullscreen_desc.windowed == 0);
        // SAFETY: `fullscreen` is non-null.
        unsafe { write_out(fullscreen, is_fs) };
    }
    if !target.is_null() {
        // SAFETY: `state.target` is null or a live `IDXGIOutput1`.
        unsafe { dxmt_util::com::add_ref(state.target) };
        // SAFETY: `target` is non-null.
        unsafe { write_out(target, state.target) };
    }
    S_OK
}

/// `IDXGISwapChain::GetDesc` (`d3d12_swapchain.cpp:522-541`).
unsafe fn get_desc(this: *mut c_void, desc: *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetDesc");
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let out = {
        let state = sc.lock_state();
        SwapChainDesc {
            buffer_desc: ModeDesc {
                width: state.desc.width,
                height: state.desc.height,
                refresh_rate: state.fullscreen_desc.refresh_rate,
                format: state.desc.format,
                scanline_ordering: state.fullscreen_desc.scanline_ordering,
                scaling: state.fullscreen_desc.scaling,
            },
            sample_desc: state.desc.sample_desc,
            buffer_usage: state.desc.buffer_usage,
            buffer_count: state.desc.buffer_count,
            output_window: sc.hwnd,
            windowed: state.fullscreen_desc.windowed,
            swap_effect: state.desc.swap_effect,
            flags: state.desc.flags,
        }
    };
    // SAFETY: `desc` was checked non-null.
    unsafe { write_out(desc.cast::<SwapChainDesc>(), out) };
    S_OK
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
        state.monitor = wsi::window_monitor(sc.hwnd);
    }
    state.read_refresh_rate();
    if width == 0 || height == 0 {
        let (w, h) = wsi::window_size(sc.hwnd);
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
    if desc_ptr.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    if !wsi::is_window(sc.hwnd) {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `desc_ptr` was checked non-null.
    let desc = unsafe { desc_ptr.cast::<ModeDesc>().read_unaligned() };
    let mut new_display_mode = ModeDesc1 {
        width: desc.width,
        height: desc.height,
        refresh_rate: desc.refresh_rate,
        format: desc.format,
        scanline_ordering: desc.scanline_ordering,
        scaling: desc.scaling,
        stereo: 0,
    };
    let (windowed, saved_window, target, monitor) = {
        let mut state = sc.lock_state();
        if new_display_mode.refresh_rate.numerator != 0 {
            state.fullscreen_desc.refresh_rate = new_display_mode.refresh_rate;
        }
        state.fullscreen_desc.scanline_ordering = new_display_mode.scanline_ordering;
        state.fullscreen_desc.scaling = new_display_mode.scaling;
        if !state.target.is_null() {
            // SAFETY: retains `state.target` across the unlocked `change_display_mode` call.
            unsafe { dxmt_util::com::add_ref(state.target) };
        }
        (
            state.fullscreen_desc.windowed != 0,
            copy_window_state(&state.saved_window),
            state.target,
            state.monitor,
        )
    };
    if windowed {
        // SAFETY: releases the temporary `target` reference if any.
        unsafe { dxmt_util::com::release(target) };
        wsi::resize_window(
            sc.hwnd,
            &saved_window,
            new_display_mode.width,
            new_display_mode.height,
        );
    } else {
        // SAFETY: `target` was retained above for the duration of this call.
        let _ = unsafe { sc.change_display_mode(target, &mut new_display_mode) };
        // SAFETY: releases the temporary `target` reference.
        unsafe { dxmt_util::com::release(target) };
        wsi::update_fullscreen_window(monitor, sc.hwnd, false);
    }
    S_OK
}

/// `IDXGISwapChain::GetContainingOutput` (`d3d12_swapchain.cpp:687-707`).
unsafe fn get_containing_output(this: *mut c_void, out: *mut *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetContainingOutput");
    // SAFETY: `init_out_ptr` checks for null.
    unsafe { init_out_ptr(out) };
    if out.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    if !wsi::is_window(sc.hwnd) {
        return DXGI_ERROR_INVALID_CALL;
    }
    let existing_target = {
        let state = sc.lock_state();
        if state.target.is_null() {
            None
        } else {
            // SAFETY: `state.target` is a live `IDXGIOutput1`.
            unsafe { dxmt_util::com::add_ref(state.target) };
            Some(state.target)
        }
    };
    if let Some(target) = existing_target {
        // SAFETY: `out` was checked non-null.
        unsafe { write_out(out, target) };
        return S_OK;
    }
    let mut output: *mut c_void = ptr::null_mut();
    // SAFETY: `output` is a local pointer.
    let hr = unsafe { sc.get_output_from_monitor(wsi::window_monitor(sc.hwnd), &raw mut output) };
    if hr < 0 {
        return hr;
    }
    // SAFETY: `out` was checked non-null; transfers `output`'s reference to `*out`.
    unsafe { write_out(out, output) };
    S_OK
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
    let count = sc.lock_state().presentation_count;
    let out = FrameStatistics {
        present_count: count,
        present_refresh_count: count,
        sync_refresh_count: count,
        sync_qpc_time: 0,
        sync_gpu_time: 0,
    };
    // SAFETY: `stats` was checked non-null.
    unsafe { write_out(stats.cast::<FrameStatistics>(), out) };
    S_OK
}

/// `IDXGISwapChain::GetLastPresentCount` (`d3d12_swapchain.cpp:724-731`).
unsafe fn get_last_present_count(this: *mut c_void, last_present_count: *mut u32) -> Hresult {
    crate::scope!("swapchain.GetLastPresentCount");
    if last_present_count.is_null() {
        return E_POINTER;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let count = sc.lock_state().presentation_count;
    // SAFETY: `last_present_count` was checked non-null.
    unsafe { write_out(last_present_count, count) };
    S_OK
}

/// `IDXGISwapChain1::GetDesc1` (`d3d12_swapchain.cpp:735-742`).
unsafe fn get_desc1(this: *mut c_void, desc: *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetDesc1");
    if desc.is_null() {
        return E_POINTER;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let d = copy_desc1(&sc.lock_state().desc);
    // SAFETY: `desc` was checked non-null.
    unsafe { write_out(desc.cast::<SwapChainDesc1>(), d) };
    S_OK
}

/// `IDXGISwapChain1::GetFullscreenDesc` (`d3d12_swapchain.cpp:746-753`).
unsafe fn get_fullscreen_desc(this: *mut c_void, desc: *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetFullscreenDesc");
    if desc.is_null() {
        return E_POINTER;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let d = copy_fullscreen_desc(&sc.lock_state().fullscreen_desc);
    // SAFETY: `desc` was checked non-null.
    unsafe { write_out(desc.cast::<SwapChainFullscreenDesc>(), d) };
    S_OK
}

/// `IDXGISwapChain1::GetHwnd` (`d3d12_swapchain.cpp:757-764`).
unsafe fn get_hwnd(this: *mut c_void, hwnd: *mut *mut c_void) -> Hresult {
    crate::scope!("swapchain.GetHwnd");
    if hwnd.is_null() {
        return E_POINTER;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    // SAFETY: `hwnd` was checked non-null.
    unsafe { write_out(hwnd, sc.hwnd) };
    S_OK
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
        if wsi::is_minimized(sc.hwnd) {
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
    if let Some(presenter) = sc.presenter.as_ref() {
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
    if max_latency == 0 || max_latency > DXGI_MAX_SWAP_CHAIN_BUFFERS {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let mut state = sc.lock_state();
    if max_latency > state.frame_latency {
        let delta = i32::try_from(max_latency - state.frame_latency).unwrap_or(0);
        release_semaphore(sc.present_semaphore, delta);
    }
    state.frame_latency = max_latency;
    S_OK
}

/// `IDXGISwapChain2::GetMaximumFrameLatency` (`d3d12_swapchain.cpp:887-893`).
unsafe fn get_maximum_frame_latency(this: *mut c_void, max_latency: *mut u32) -> Hresult {
    crate::scope!("swapchain.GetMaximumFrameLatency");
    if !max_latency.is_null() {
        // SAFETY: `this` is a live `SwapChain`.
        let sc = unsafe { &*this.cast::<SwapChain>() };
        let latency = sc.lock_state().frame_latency;
        // SAFETY: `max_latency` is non-null.
        unsafe { write_out(max_latency, latency) };
    }
    S_OK
}

/// `IDXGISwapChain2::GetFrameLatencyWaitableObject` (`d3d12_swapchain.cpp:896-910`).
unsafe fn get_frame_latency_waitable_object(this: *mut c_void) -> *mut c_void {
    crate::scope!("swapchain.GetFrameLatencyWaitableObject");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let flags = sc.lock_state().desc.flags;
    if flags & DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT == 0 {
        return ptr::null_mut();
    }
    duplicate_same_access(sc.present_semaphore)
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
    if color_space_support.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `CGColorSpace_checkColorSpaceSupported` queries CoreGraphics for `WMTColorSpace`.
    let supported =
        unsafe { CGColorSpace_checkColorSpaceSupported(convert_color_space(color_space, false)) };
    let flags = if supported {
        DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT
    } else {
        0
    };
    // SAFETY: `color_space_support` was checked non-null.
    unsafe { write_out(color_space_support, flags) };
    S_OK
}

/// `IDXGISwapChain3::SetColorSpace1` (`d3d12_swapchain.cpp:953-961`).
unsafe fn set_color_space1(this: *mut c_void, color_space: u32) -> Hresult {
    crate::scope!("swapchain.SetColorSpace1");
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let target_color_space = convert_color_space(color_space, sc.layer_support_edr());
    if let Some(presenter) = sc.presenter.as_ref() {
        let _ = presenter.change_layer_color_space(target_color_space);
    }
    sc.lock_state().colorspace = color_space;
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
        saved_window: WindowState::default(),
        presentation_count: 0,
        desc: sc_desc,
        backbuffer_desc,
        fullscreen_desc: fs_desc,
        frame_latency: SWAPCHAIN_LATENCY,
        colorspace: DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
        init_refresh_rate: f64::MAX,
        backbuffers: Vec::new(),
    };
    state.read_refresh_rate();

    // SAFETY: retains `factory` for the swapchain's lifetime (`Com<IDXGIFactory1> factory_`).
    unsafe { dxmt_util::com::add_ref(factory) };

    let raw_sc = Box::into_raw(Box::new(SwapChain {
        vtbl: &SWAPCHAIN_VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: ptr::from_ref(device).cast_mut(),
        queue: ptr::from_ref(queue),
        presenter,
        hud: Some(hud),
        factory,
        hwnd,
        native_view,
        layer_weak,
        present_semaphore,
        render_scale: render_scale(),
        preferred_max_frame_rate,
        modeset_in_progress: AtomicBool::new(false),
        library,
        state: Mutex::new(state),
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
