//! D3D11 swap chain (`MTLD3D11SwapChain`, `d3d11_swapchain.cpp`).

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use std::sync::{Arc, Mutex, MutexGuard};

use dxmt_core::{
    frame_stats::{CompatibilityFlags, Container, ScalerType},
    hud::Hud,
    presenter::{Presenter, PresenterDesc},
    scaler::Scaler,
    settings::{Flags, settings},
};
#[cfg(not(test))]
use dxmt_util::wsi;
use dxmt_util::{
    GammaRamp,
    com::{Qi, QiMiss, qi_table},
    dxgi_vtbl::{IDXGIAdapterVtbl, IDXGIOutput1Vtbl},
    ext::{IID_IMTL_DXGI_DEVICE, IID_IMTL_DXGI_OUTPUT, IMTLDXGIOutputVtbl},
    private_data::{
        HasPrivateData, PrivateData, get_private_data, set_private_data, set_private_data_interface,
    },
    sync::{CpuFence, lock},
    wsi::{Rational as WsiRational, WindowState, WsiMode},
};
pub use dxmt_util::{
    com::{
        hresult::{
            DXGI_ERROR_INVALID_CALL, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE, DXGI_ERROR_NOT_FOUND,
            DXGI_ERROR_UNSUPPORTED, DXGI_STATUS_MODE_CHANGE_IN_PROGRESS, DXGI_STATUS_OCCLUDED,
        },
        iid::IID_IDXGI_OUTPUT_1,
    },
    dxgi_consts::{
        D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_UNORDERED_ACCESS,
        DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
        DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_B8G8R8X8_UNORM,
        DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM,
        DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_UNKNOWN,
        DXGI_HDR_METADATA_TYPE_HDR10, DXGI_HDR_METADATA_TYPE_NONE, DXGI_MAX_SWAP_CHAIN_BUFFERS,
        DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_SCANLINE_ORDER_LOWER_FIELD_FIRST,
        DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST, DXGI_PRESENT_TEST,
        DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT, DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH,
        DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT, DXGI_SWAP_EFFECT_DISCARD,
        DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
        DXGI_SWAP_EFFECT_SEQUENTIAL, DXGI_USAGE_SHADER_INPUT, DXGI_USAGE_UNORDERED_ACCESS,
    },
};
use winemetal_sys::{
    CGColorSpace_checkColorSpaceSupported, CreateMetalViewFromHWND, MetalLayer_getEDRValue,
    ReleaseMetalView,
    wire::{
        WMT_COLOR_SPACE_BT2020, WMT_COLOR_SPACE_HDR_PQ, WMT_COLOR_SPACE_HDR_SC_RGB,
        WMT_COLOR_SPACE_INVALID, WMT_COLOR_SPACE_SRGB, WMT_COLOR_SPACE_SRGB_LINEAR,
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB, WMT_PIXEL_FORMAT_INVALID,
        WMT_PIXEL_FORMAT_RGB10A2_UNORM, WMT_PIXEL_FORMAT_RGBA16_FLOAT, WMTEDRValue,
        WMTFXSpatialScalerInfo, WMTHDRMetadata,
    },
};

use crate::{
    abi::{
        IID_IDXGI_DEVICE_SUB_OBJECT, IID_IDXGI_OBJECT, IID_IDXGI_SWAP_CHAIN,
        IID_IDXGI_SWAP_CHAIN_1, IID_IDXGI_SWAP_CHAIN_2, IID_IDXGI_SWAP_CHAIN_3,
        IID_IDXGI_SWAP_CHAIN_4,
        ext::{IMTLD3D11DeviceContextVtbl, OutputDesc, Rect},
        records::{
            D3d11Texture2dDesc1, DxgiFrameStatistics, DxgiModeDesc, DxgiModeDesc1,
            DxgiPresentParameters, DxgiRational, DxgiRgba, DxgiSwapChainDesc, DxgiSwapChainDesc1,
            DxgiSwapChainFullscreenDesc,
        },
        vtbl::{ID3D11Device5Vtbl, IDXGIDevice3Vtbl, IDXGISwapChain4Vtbl, IUnknownVtbl},
    },
    com::{
        ComObject, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, Guid, Hresult,
        IID_IUNKNOWN, S_OK,
    },
    device::Device,
    resource,
};

/// Default swapchain maximum frame latency (`kSwapchainLatency`).
pub const DEFAULT_SWAPCHAIN_LATENCY: u32 = 1;

/// Converts a `DXGI_FORMAT` to its Metal swapchain pixel format (`ConvertSwapChainFormat`).
#[must_use]
pub const fn convert_swapchain_format(format: u32) -> u32 {
    match format {
        DXGI_FORMAT_B8G8R8A8_UNORM_SRGB | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => {
            WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB
        }
        DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM => WMT_PIXEL_FORMAT_BGRA8_UNORM,
        DXGI_FORMAT_R10G10B10A2_UNORM => WMT_PIXEL_FORMAT_RGB10A2_UNORM,
        DXGI_FORMAT_R16G16B16A16_FLOAT => WMT_PIXEL_FORMAT_RGBA16_FLOAT,
        _ => WMT_PIXEL_FORMAT_INVALID,
    }
}

/// Converts a `DXGI_COLOR_SPACE_TYPE` to its Metal color space (`ConvertColorSpace`).
#[must_use]
pub const fn convert_color_space(color_space: u32, hdr: bool) -> u64 {
    match color_space {
        DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709 => WMT_COLOR_SPACE_SRGB,
        DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709 => {
            if hdr {
                WMT_COLOR_SPACE_HDR_SC_RGB
            } else {
                WMT_COLOR_SPACE_SRGB_LINEAR
            }
        }
        DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 => WMT_COLOR_SPACE_HDR_PQ,
        DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020 => WMT_COLOR_SPACE_BT2020,
        _ => WMT_COLOR_SPACE_INVALID,
    }
}

/// Returns monitor bits-per-pixel for `format` (`GetMonitorFormatBpp`).
#[must_use]
pub const fn get_monitor_format_bpp(format: u32) -> u32 {
    match format {
        DXGI_FORMAT_R16G16B16A16_FLOAT => 64,
        _ => 32,
    }
}

/// Computes `vsync_duration` exactly as `d3d11_swapchain.cpp:797-801` does.
#[must_use]
pub fn compute_vsync_duration(
    sync_interval: u32,
    preferred_max_frame_rate: i32,
    init_refresh_rate: f64,
) -> f64 {
    let rate = if preferred_max_frame_rate != 0 {
        f64::from(preferred_max_frame_rate)
    } else {
        init_refresh_rate
    };
    let floor = if preferred_max_frame_rate != 0 {
        1.0 / f64::from(preferred_max_frame_rate)
    } else {
        0.0
    };
    (f64::from(sync_interval) / rate).max(floor)
}

/// Returns `true` when the VM guest paces its own frames (`GuestPaces`).
#[cfg(any(windows, test))]
#[must_use]
pub const fn guest_paces() -> bool {
    false
}

/// Returns `true` when the VM guest paces its own frames (`GuestPaces`).
#[cfg(all(not(windows), not(test)))]
#[must_use]
pub fn guest_paces() -> bool {
    wsi::hooked()
}

#[cfg(not(test))]
fn wsi_window_size(hwnd: *mut c_void) -> (u32, u32) {
    wsi::window_size(hwnd)
}

#[cfg(test)]
const fn wsi_window_size(_hwnd: *mut c_void) -> (u32, u32) {
    (0, 0)
}

#[cfg(not(test))]
fn wsi_window_monitor(hwnd: *mut c_void) -> *mut c_void {
    wsi::window_monitor(hwnd)
}

#[cfg(test)]
const fn wsi_window_monitor(_hwnd: *mut c_void) -> *mut c_void {
    ptr::null_mut()
}

#[cfg(not(test))]
fn wsi_current_display_mode(monitor: *mut c_void) -> Option<WsiMode> {
    wsi::current_display_mode(monitor)
}

#[cfg(test)]
const fn wsi_current_display_mode(_monitor: *mut c_void) -> Option<WsiMode> {
    None
}

#[cfg(not(test))]
fn wsi_is_window(hwnd: *mut c_void) -> bool {
    wsi::is_window(hwnd)
}

#[cfg(test)]
const fn wsi_is_window(hwnd: *mut c_void) -> bool {
    !hwnd.is_null()
}

#[cfg(not(test))]
fn wsi_is_minimized(hwnd: *mut c_void) -> bool {
    wsi::is_minimized(hwnd)
}

#[cfg(test)]
const fn wsi_is_minimized(_hwnd: *mut c_void) -> bool {
    false
}

#[cfg(not(test))]
fn wsi_is_foreground(hwnd: *mut c_void) -> bool {
    wsi::is_foreground(hwnd)
}

#[cfg(test)]
const fn wsi_is_foreground(_hwnd: *mut c_void) -> bool {
    false
}

#[cfg(not(test))]
fn wsi_is_process_foreground() -> bool {
    wsi::is_process_foreground()
}

#[cfg(test)]
const fn wsi_is_process_foreground() -> bool {
    true
}

#[cfg(not(test))]
fn wsi_minimize_window(hwnd: *mut c_void) {
    wsi::minimize_window(hwnd);
}

#[cfg(test)]
const fn wsi_minimize_window(_hwnd: *mut c_void) {}

#[cfg(not(test))]
fn wsi_save_window_state(hwnd: *mut c_void, state: &mut WindowState) {
    wsi::save_window_state(hwnd, state);
}

#[cfg(test)]
const fn wsi_save_window_state(_hwnd: *mut c_void, _state: &mut WindowState) {}

#[cfg(not(test))]
fn wsi_set_window_mode(monitor: *mut c_void, hwnd: *mut c_void, mode: &WsiMode) -> bool {
    wsi::set_window_mode(monitor, hwnd, mode)
}

#[cfg(test)]
const fn wsi_set_window_mode(_monitor: *mut c_void, _hwnd: *mut c_void, _mode: &WsiMode) -> bool {
    true
}

#[cfg(not(test))]
fn wsi_enter_fullscreen_mode(
    monitor: *mut c_void,
    hwnd: *mut c_void,
    state: &mut WindowState,
    mode_switch: bool,
) -> bool {
    wsi::enter_fullscreen_mode(monitor, hwnd, state, mode_switch)
}

#[cfg(test)]
const fn wsi_enter_fullscreen_mode(
    _monitor: *mut c_void,
    _hwnd: *mut c_void,
    _state: &mut WindowState,
    _mode_switch: bool,
) -> bool {
    true
}

#[cfg(not(test))]
fn wsi_leave_fullscreen_mode(
    hwnd: *mut c_void,
    state: &WindowState,
    restore_coordinates: bool,
) -> bool {
    wsi::leave_fullscreen_mode(hwnd, state, restore_coordinates)
}

#[cfg(test)]
const fn wsi_leave_fullscreen_mode(
    _hwnd: *mut c_void,
    _state: &WindowState,
    _restore_coordinates: bool,
) -> bool {
    true
}

const fn copy_window_state(w: &WindowState) -> WindowState {
    WindowState {
        style: w.style,
        exstyle: w.exstyle,
        rect: w.rect,
    }
}

#[cfg(not(test))]
fn wsi_restore_display_mode(monitor: *mut c_void) -> bool {
    wsi::restore_display_mode(monitor)
}

#[cfg(test)]
const fn wsi_restore_display_mode(_monitor: *mut c_void) -> bool {
    true
}

#[cfg(not(test))]
fn wsi_resize_window(hwnd: *mut c_void, state: &WindowState, width: u32, height: u32) {
    wsi::resize_window(hwnd, state, width, height);
}

#[cfg(test)]
const fn wsi_resize_window(_hwnd: *mut c_void, _state: &WindowState, _width: u32, _height: u32) {}

#[cfg(not(test))]
fn wsi_update_fullscreen_window(monitor: *mut c_void, hwnd: *mut c_void, force_topmost: bool) {
    wsi::update_fullscreen_window(monitor, hwnd, force_topmost);
}

#[cfg(test)]
const fn wsi_update_fullscreen_window(
    _monitor: *mut c_void,
    _hwnd: *mut c_void,
    _force_topmost: bool,
) {
}

const DUPLICATE_SAME_ACCESS: u32 = 2;

#[cfg(windows)]
unsafe extern "system" {
    fn CreateSemaphoreA(
        attributes: *mut c_void,
        initial_count: i32,
        maximum_count: i32,
        name: *const u8,
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
}

fn create_semaphore(initial: i32, max: i32) -> *mut c_void {
    #[cfg(windows)]
    {
        // SAFETY: null security attributes and name create an unnamed semaphore.
        unsafe { CreateSemaphoreA(ptr::null_mut(), initial, max, ptr::null()) }
    }
    #[cfg(not(windows))]
    {
        if cfg!(test) {
            return ptr::without_provenance_mut(1);
        }
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
        if cfg!(test) {
            return;
        }
        // SAFETY: `handle` is the swapchain's valid native semaphore handle.
        let _ =
            unsafe { winemetal_sys::wmt_native_release_semaphore(handle, count, ptr::null_mut()) };
    }
}

fn duplicate_same_access(handle: *mut c_void) -> *mut c_void {
    if handle.is_null() {
        return ptr::null_mut();
    }
    #[cfg(windows)]
    {
        let mut result: *mut c_void = ptr::null_mut();
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
        if cfg!(test) {
            return handle;
        }
        let mut result: *mut c_void = ptr::null_mut();
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
        if cfg!(test) {
            return;
        }
        // SAFETY: `handle` is a live native handle owned by the swapchain.
        let _ = unsafe { winemetal_sys::wmt_native_close_handle(handle) };
    }
}

/// Captures the frame-latency fence for a present command and signals on drop (`SyncFrameState`).
///
/// The present command closure stored in the chunk arena owns a [`SyncFrameState`], so `Drop`
/// runs when the chunk resets after retirement (`Commands::reset`), not when the present command
/// executes on the encode thread (Principle P8).
pub struct SyncFrameState {
    frame_id: u64,
    fence: Option<Arc<CpuFence>>,
}

impl SyncFrameState {
    /// Creates an empty [`SyncFrameState`] that signals no fence on drop.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            frame_id: 0,
            fence: None,
        }
    }

    /// Creates a [`SyncFrameState`] that signals `fence` at `frame_id` when dropped.
    #[must_use]
    pub const fn new(frame_id: u64, fence: Arc<CpuFence>) -> Self {
        Self {
            frame_id,
            fence: Some(fence),
        }
    }
}

impl Drop for SyncFrameState {
    fn drop(&mut self) {
        if let Some(fence) = self.fence.take() {
            fence.signal(self.frame_id);
        }
    }
}

/// Paces a frame against `fence` and returns its [`SyncFrameState`] (`SyncFrame`).
///
/// Waits for `(current_frame_id - frame_latency)` when `fence` is present and `!guest_paces`,
/// then returns a [`SyncFrameState`] that signals `current_frame_id` when dropped.
#[must_use]
pub fn sync_frame(
    fence: Option<&Arc<CpuFence>>,
    current_frame_id: u64,
    frame_latency: u32,
    guest_paces: bool,
) -> SyncFrameState {
    let Some(fence) = fence else {
        return SyncFrameState::none();
    };
    let latency = u64::from(frame_latency);
    if current_frame_id > latency && !guest_paces {
        fence.wait(current_frame_id - latency);
    }
    SyncFrameState::new(current_frame_id, Arc::clone(fence))
}

/// Evaluates the initial occlusion/fullscreen decision of `Present1`.
///
/// Returns `(hr, should_exit_fullscreen, return_immediately)`.
#[must_use]
pub const fn evaluate_present_prelude(
    sync_interval: u32,
    present_flags: u32,
    window_minimized: bool,
    extent: (u32, u32),
    swap_effect: u32,
    alt_tab: (bool, i32, bool),
) -> (Hresult, bool, bool) {
    if sync_interval > 4 {
        return (DXGI_ERROR_INVALID_CALL, false, true);
    }
    let (width, height) = extent;
    let (handle_alt_tab, windowed, is_foreground) = alt_tab;
    let should_exit_fs = handle_alt_tab && windowed == 0 && !window_minimized && !is_foreground;
    let occluded = ((window_minimized || width == 0 || height == 0)
        && swap_effect <= DXGI_SWAP_EFFECT_SEQUENTIAL)
        || should_exit_fs;
    let hr = if occluded { DXGI_STATUS_OCCLUDED } else { S_OK };
    if (present_flags & DXGI_PRESENT_TEST) != 0 {
        return (hr, false, true);
    }
    (hr, should_exit_fs, false)
}

/// Writes per-frame HUD lines from `statistics` for `frame_id` (`UpdateStatistics`).
pub fn update_statistics(hud: &mut Hud, statistics: &Container, frame_id: u64) {
    hud.begin();
    let frame = statistics.at(frame_id.wrapping_sub(1));
    let average = statistics.average();
    let max = statistics.max();
    let flags = frame.compatibility_flags;
    let mut text = *b"---------------------------";
    if flags.contains(CompatibilityFlags::UNSUPPORTED_GEOMETRY_DRAW) {
        text[3] = b'G';
    }
    if flags.contains(CompatibilityFlags::UNSUPPORTED_TESSELLATION_OUTPUT_PRIMITIVE) {
        text[5] = b'T';
        text[6] = b'O';
    }
    if flags.contains(CompatibilityFlags::UNSUPPORTED_INDIRECT_TESSELLATION_DRAW) {
        text[8] = b'I';
        text[9] = b'T';
    }
    if flags.contains(CompatibilityFlags::UNSUPPORTED_GEOMETRY_TESSELLATION_DRAW) {
        text[11] = b'G';
        text[12] = b'T';
    }
    if flags.contains(CompatibilityFlags::UNSUPPORTED_DRAW_AUTO) {
        text[14] = b'A';
    }
    if flags.contains(CompatibilityFlags::UNSUPPORTED_PREDICATION) {
        text[16] = b'P';
    }
    if flags.contains(CompatibilityFlags::UNSUPPORTED_STREAM_OUTPUT_APPENDING) {
        text[18] = b'S';
        text[19] = b'A';
    }
    if flags.contains(CompatibilityFlags::UNSUPPORTED_MULTIPLE_STREAM_OUTPUT) {
        text[21] = b'M';
        text[22] = b'S';
    }
    if let Ok(line) = core::str::from_utf8(&text) {
        hud.print_line(line);
    }
    let avg_commit = (average.commit_interval.as_secs_f64() * 1000.0).min(99.9);
    let max_commit = (max.commit_interval.as_secs_f64() * 1000.0).min(99.9);
    hud.print_line(&format!(
        "Commit: {:2} -{:4.1} -{:4.1}",
        frame.command_buffer_count.min(99),
        avg_commit,
        max_commit,
    ));
    let avg_sync = (average.sync_interval.as_secs_f64() * 1000.0).min(99.9);
    let avg_lat = (average.present_latency_interval.as_secs_f64() * 1000.0).min(99.9);
    hud.print_line(&format!(
        "Sync:   {:2} {:4.1}  {:2} {:4.1} {:2}",
        frame.sync_count.min(99),
        avg_sync,
        max.event_stall.min(99),
        avg_lat,
        frame.latency,
    ));
    let prep = (average.encode_prepare_interval.as_secs_f64() * 1000.0).min(99.9);
    let flush_non_block = (average
        .encode_flush_interval
        .saturating_sub(average.drawable_blocking_interval)
        .as_secs_f64()
        * 1000.0)
        .min(99.9);
    let block = (average.drawable_blocking_interval.as_secs_f64() * 1000.0).min(99.9);
    let total = ((average.encode_prepare_interval + average.encode_flush_interval).as_secs_f64()
        * 1000.0)
        .min(99.9);
    hud.print_line(&format!(
        "Encode: {prep:4.1}+{flush_non_block:4.1}+{block:4.1}={total:4.1}"
    ));
    hud.print_line(&format!(
        "Render:{:3}+{:<3} Clear:{:3}+{:<2}",
        frame
            .render_pass_count
            .saturating_sub(frame.render_pass_optimized)
            .min(999),
        frame.render_pass_optimized.min(999),
        frame
            .clear_pass_count
            .saturating_sub(frame.clear_pass_optimized)
            .min(999),
        frame.clear_pass_optimized.min(99),
    ));
    let info = &frame.last_scaler_info;
    match info.kind {
        ScalerType::Temporal => {
            let aexp = if info.auto_exposure { "AEXP" } else { "" };
            let hmv = if info.motion_vector_highres {
                "HMV"
            } else {
                ""
            };
            hud.print_line(&format!("MetalFX: Temporal {aexp} {hmv}"));
            hud.print_line(&format!(
                "Scale: {:4}x{:4}->{:4}x{:4}",
                info.input_width, info.input_height, info.output_width, info.output_height
            ));
        }
        ScalerType::Spatial => {
            hud.print_line("MetalFX: Spatial");
            hud.print_line(&format!(
                "Scale: {:4}x{:4}->{:4}x{:4}",
                info.input_width, info.input_height, info.output_width, info.output_height
            ));
        }
        ScalerType::None => {}
    }
    hud.end();
}

static SWAPCHAIN_VTBL: IDXGISwapChain4Vtbl = IDXGISwapChain4Vtbl {
    query_interface: sc_query_interface,
    add_ref: sc_add_ref,
    release: sc_release,
    set_private_data: set_private_data::<SwapChain>,
    set_private_data_interface: set_private_data_interface::<SwapChain>,
    get_private_data: get_private_data::<SwapChain>,
    get_parent: sc_get_parent,
    get_device: sc_get_device,
    present: sc_present,
    get_buffer: sc_get_buffer,
    set_fullscreen_state: sc_set_fullscreen_state,
    get_fullscreen_state: sc_get_fullscreen_state,
    get_desc: sc_get_desc,
    resize_buffers: sc_resize_buffers,
    resize_target: sc_resize_target,
    get_containing_output: sc_get_containing_output,
    get_frame_statistics: sc_get_frame_statistics,
    get_last_present_count: sc_get_last_present_count,
    get_desc1: sc_get_desc1,
    get_fullscreen_desc: sc_get_fullscreen_desc,
    get_hwnd: sc_get_hwnd,
    get_core_window: sc_get_core_window,
    present1: sc_present1,
    is_temporary_mono_supported: sc_is_temporary_mono_supported,
    get_restrict_to_output: sc_get_restrict_to_output,
    set_background_color: sc_set_background_color,
    get_background_color: sc_get_background_color,
    set_rotation: sc_set_rotation,
    get_rotation: sc_get_rotation,
    set_source_size: sc_set_source_size,
    get_source_size: sc_get_source_size,
    set_maximum_frame_latency: sc_set_maximum_frame_latency,
    get_maximum_frame_latency: sc_get_maximum_frame_latency,
    get_frame_latency_waitable_object: sc_get_frame_latency_waitable_object,
    set_matrix_transform: sc_set_matrix_transform,
    get_matrix_transform: sc_get_matrix_transform,
    get_current_back_buffer_index: sc_get_current_back_buffer_index,
    check_color_space_support: sc_check_color_space_support,
    set_color_space1: sc_set_color_space1,
    resize_buffers1: sc_resize_buffers1,
    set_hdr_meta_data: sc_set_hdr_metadata,
};

struct SwapChainState {
    desc: DxgiSwapChainDesc1,
    fullscreen_desc: DxgiSwapChainFullscreenDesc,
    backbuffer_desc: D3d11Texture2dDesc1,
    backbuffer: *mut c_void,
    upscaled_backbuffer: *mut c_void,
    metalfx_scaler: Option<Arc<Scaler>>,
    target: *mut c_void,
    gamma_output: *mut c_void,
    monitor: *mut c_void,
    window_state: WindowState,
    presentation_count: u32,
    frame_latency: u32,
    color_space: u32,
    init_refresh_rate: f64,
    preferred_max_frame_rate: i32,
    scale_factor: f32,
    enable_metalfx: bool,
    handle_alt_tab: bool,
}

/// `IDXGISwapChain4` implementation (`MTLD3D11SwapChain`).
#[repr(C)]
pub struct SwapChain {
    vtbl: *const IDXGISwapChain4Vtbl,
    refs: ComObject,
    device: *mut c_void,
    factory: *mut c_void,
    dxgi_device: *mut c_void,
    device_context: *mut c_void,
    hwnd: *mut c_void,
    native_view: u64,
    layer_weak: u64,
    presenter: Option<Arc<Presenter>>,
    present_semaphore: *mut c_void,
    frame_latency_fence: Option<Arc<CpuFence>>,
    modeset_in_progress: AtomicBool,
    private_data: PrivateData,
    state: Mutex<SwapChainState>,
    /// Present calls are serialised even while the device lock is released for their waits.
    present_serial: Mutex<()>,
}

impl SwapChain {
    /// Recovers `&SwapChain` from an `IDXGISwapChain*` pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: `iface` is non-null and its first word holds the COM vtable pointer.
        let vtbl = unsafe { *iface.cast::<*const IDXGISwapChain4Vtbl>() };
        if !ptr::eq(vtbl, &raw const SWAPCHAIN_VTBL) {
            return None;
        }
        // SAFETY: `vtbl` matches `SWAPCHAIN_VTBL`, so `iface` points to a live `SwapChain`.
        Some(unsafe { &*iface.cast::<Self>() })
    }

    fn lock_state(&self) -> MutexGuard<'_, SwapChainState> {
        lock(&self.state)
    }

    fn layer_support_edr(&self) -> bool {
        if self.layer_weak == 0 {
            return false;
        }
        let mut edr = WMTEDRValue {
            maximum_edr_color_component_value: 1.0,
            maximum_potential_edr_color_component_value: 1.0,
        };
        // SAFETY: `self.layer_weak` is a live `CAMetalLayer` handle and `edr` is valid stack memory.
        unsafe { MetalLayer_getEDRValue(self.layer_weak, &raw mut edr) };
        edr.maximum_potential_edr_color_component_value > 1.0
    }

    fn wait_until_gpu_idle(&self) {
        if self.device_context.is_null() {
            return;
        }
        // SAFETY: `self.device_context` is a live `IMTLD3D11DeviceContext` COM pointer.
        let ctx_vtbl =
            unsafe { crate::com::vtbl::<IMTLD3D11DeviceContextVtbl>(self.device_context) };
        // SAFETY: `self.device_context` is valid.
        unsafe { (ctx_vtbl.wait_until_gpu_idle)(self.device_context) };
    }

    fn apply_layer_props(&self, st: &SwapChainState) {
        let Some(presenter) = &self.presenter else {
            return;
        };
        let cs = if st.desc.format == DXGI_FORMAT_R16G16B16A16_FLOAT {
            DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709
        } else {
            st.color_space
        };
        let target_cs = convert_color_space(cs, self.layer_support_edr());
        let scale = if st.enable_metalfx {
            st.scale_factor
        } else {
            1.0
        };
        let scaled_w = f64::from(st.desc.width) * f64::from(scale);
        let scaled_h = f64::from(st.desc.height) * f64::from(scale);
        let sample_count = u8::try_from(st.desc.sample_desc.count.max(1)).unwrap_or(1);
        let changed = presenter.change_layer_properties(
            convert_swapchain_format(st.desc.format),
            target_cs,
            scaled_w,
            scaled_h,
            sample_count,
        );
        if changed {
            self.wait_until_gpu_idle();
        }
    }

    fn get_output_from_monitor(&self, monitor: *mut c_void) -> Result<*mut c_void, Hresult> {
        if self.dxgi_device.is_null() {
            return Err(E_FAIL);
        }
        // SAFETY: `self.dxgi_device` is a live `IDXGIDevice3` COM pointer.
        let dev_vtbl = unsafe { crate::com::vtbl::<IDXGIDevice3Vtbl>(self.dxgi_device) };
        let mut adapter = ptr::null_mut();
        // SAFETY: `self.dxgi_device` and `adapter` are valid.
        let hr = unsafe { (dev_vtbl.get_adapter)(self.dxgi_device, &raw mut adapter) };
        if hr < 0 || adapter.is_null() {
            return Err(E_FAIL);
        }
        // SAFETY: `adapter` is a live `IDXGIAdapter` COM pointer.
        let ad_vtbl = unsafe { crate::com::vtbl::<IDXGIAdapterVtbl>(adapter) };
        let mut index = 0_u32;
        loop {
            let mut output = ptr::null_mut();
            // SAFETY: `adapter` and `output` are valid.
            let enum_hr = unsafe { (ad_vtbl.enum_outputs)(adapter, index, &raw mut output) };
            if enum_hr < 0 || output.is_null() {
                break;
            }
            // SAFETY: `output` is a live `IDXGIOutput` COM pointer.
            let out_vtbl = unsafe { crate::com::vtbl::<IDXGIOutput1Vtbl>(output) };
            let mut out_desc = OutputDesc {
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
            };
            // SAFETY: `output` and `out_desc` are valid.
            let _ = unsafe { (out_vtbl.get_desc)(output, (&raw mut out_desc).cast::<c_void>()) };
            if ptr::eq(out_desc.monitor, monitor) {
                let mut out1 = ptr::null_mut();
                let iid_out1 = IID_IDXGI_OUTPUT_1;
                // SAFETY: `output` is valid.
                let qi_hr = unsafe {
                    (out_vtbl.query_interface)(
                        output,
                        (&raw const iid_out1).cast::<c_void>(),
                        &raw mut out1,
                    )
                };
                // SAFETY: release `output` reference.
                unsafe { crate::com::release(output) };
                // SAFETY: `adapter` was retained by `get_adapter`.
                unsafe { crate::com::release(adapter) };
                if qi_hr < 0 || out1.is_null() {
                    return Err(qi_hr);
                }
                return Ok(out1);
            }
            // SAFETY: `output` was retained by `enum_outputs`.
            unsafe { crate::com::release(output) };
            index += 1;
        }
        // SAFETY: `adapter` was retained by `get_adapter`.
        unsafe { crate::com::release(adapter) };
        Err(DXGI_ERROR_NOT_FOUND)
    }

    fn change_display_mode(
        &self,
        output: *mut c_void,
        mode: &mut DxgiModeDesc1,
        flags: u32,
        fallback_format: u32,
    ) -> (*mut c_void, Option<f64>, Hresult) {
        if output.is_null() {
            return (ptr::null_mut(), None, DXGI_ERROR_INVALID_CALL);
        }
        let mut preferred = *mode;
        if (flags & DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH) == 0 {
            preferred.width = 0;
            preferred.height = 0;
        }
        if preferred.format == DXGI_FORMAT_UNKNOWN {
            preferred.format = fallback_format;
        }
        let mut selected = DxgiModeDesc1 {
            width: 0,
            height: 0,
            refresh_rate: DxgiRational {
                numerator: 0,
                denominator: 0,
            },
            format: 0,
            scanline_ordering: 0,
            scaling: 0,
            stereo: 0,
        };
        // SAFETY: `output` is a live `IDXGIOutput1` COM pointer.
        let out_vtbl = unsafe { crate::com::vtbl::<IDXGIOutput1Vtbl>(output) };
        // SAFETY: `output`, `preferred`, and `selected` are valid.
        let hr = unsafe {
            (out_vtbl.find_closest_matching_mode1)(
                output,
                (&raw const preferred).cast::<c_void>(),
                (&raw mut selected).cast::<c_void>(),
                ptr::null_mut(),
            )
        };
        if hr < 0 {
            return (ptr::null_mut(), None, hr);
        }
        if selected.refresh_rate.denominator == 0 {
            selected.refresh_rate.denominator = 1;
        }
        let mut out_desc = OutputDesc {
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
        };
        // SAFETY: `output` and `out_desc` are valid.
        let _ = unsafe { (out_vtbl.get_desc)(output, (&raw mut out_desc).cast::<c_void>()) };
        let wsi_mode = WsiMode {
            width: selected.width,
            height: selected.height,
            refresh: WsiRational {
                numerator: selected.refresh_rate.numerator,
                denominator: selected.refresh_rate.denominator,
            },
            bits_per_pixel: get_monitor_format_bpp(selected.format),
            interlaced: selected.scanline_ordering == DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST
                || selected.scanline_ordering == DXGI_MODE_SCANLINE_ORDER_LOWER_FIELD_FIRST,
        };
        if !wsi_set_window_mode(out_desc.monitor, self.hwnd, &wsi_mode) {
            return (out_desc.monitor, None, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE);
        }
        *mode = selected;
        let rate = f64::from(selected.refresh_rate.numerator)
            / f64::from(selected.refresh_rate.denominator);
        (out_desc.monitor, Some(rate), S_OK)
    }

    fn enter_fullscreen_mode(&self, target_arg: *mut c_void) -> Hresult {
        if self.modeset_in_progress.swap(true, Ordering::AcqRel) {
            return DXGI_STATUS_MODE_CHANGE_IN_PROGRESS;
        }
        let res = self.enter_fullscreen_inner(target_arg);
        self.modeset_in_progress.store(false, Ordering::Release);
        res
    }

    fn enter_fullscreen_inner(&self, target_arg: *mut c_void) -> Hresult {
        if !wsi_is_window(self.hwnd) {
            return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
        }
        let output = if target_arg.is_null() {
            match self.get_output_from_monitor(wsi_window_monitor(self.hwnd)) {
                Ok(out) => out,
                Err(_) => return E_FAIL,
            }
        } else {
            // SAFETY: `target_arg` is a valid COM pointer; retain our own reference.
            unsafe { crate::com::add_ref(target_arg) };
            target_arg
        };

        let (mut saved, mut preferred, flags, format) = {
            let st = self.lock_state();
            (
                copy_window_state(&st.window_state),
                DxgiModeDesc1 {
                    width: st.desc.width,
                    height: st.desc.height,
                    refresh_rate: st.fullscreen_desc.refresh_rate,
                    format: st.desc.format,
                    scanline_ordering: 0,
                    scaling: 0,
                    stereo: 0,
                },
                st.desc.flags,
                st.desc.format,
            )
        };
        wsi_save_window_state(self.hwnd, &mut saved);
        self.lock_state().window_state = copy_window_state(&saved);
        let (monitor, new_rate, mode_hr) =
            self.change_display_mode(output, &mut preferred, flags, format);
        if mode_hr < 0 {
            // SAFETY: `output` was retained above.
            unsafe { crate::com::release(output) };
            return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
        }
        let mut st = self.lock_state();
        if let Some(r) = new_rate {
            st.init_refresh_rate = r;
        }
        st.fullscreen_desc.windowed = 0;
        let mode_switch = (st.desc.flags & DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH) != 0;
        st.monitor = monitor;

        if !st.gamma_output.is_null() {
            // SAFETY: `st.gamma_output` is a valid COM pointer or null.
            unsafe { crate::com::release(st.gamma_output) };
            st.gamma_output = ptr::null_mut();
        }
        // SAFETY: `output` is a live `IDXGIOutput1` COM pointer.
        let out_vtbl = unsafe { crate::com::vtbl::<IUnknownVtbl>(output) };
        let mut gamma_out = ptr::null_mut();
        let iid_gamma = IID_IMTL_DXGI_OUTPUT;
        // SAFETY: `output` and `gamma_out` are valid.
        let _ = unsafe {
            (out_vtbl.query_interface)(
                output,
                (&raw const iid_gamma).cast::<c_void>(),
                &raw mut gamma_out,
            )
        };
        st.gamma_output = gamma_out;

        if !st.target.is_null() {
            // SAFETY: `st.target` was previously retained.
            unsafe { crate::com::release(st.target) };
        }
        st.target = output;
        drop(st);

        let ok = wsi_enter_fullscreen_mode(monitor, self.hwnd, &mut saved, mode_switch);
        self.lock_state().window_state = saved;
        if !ok {
            return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
        }
        S_OK
    }

    fn leave_fullscreen_mode(&self, switching_output: bool) -> Hresult {
        if self.modeset_in_progress.swap(true, Ordering::AcqRel) {
            return DXGI_STATUS_MODE_CHANGE_IN_PROGRESS;
        }
        let res = self.leave_fullscreen_inner(switching_output);
        self.modeset_in_progress.store(false, Ordering::Release);
        res
    }

    fn leave_fullscreen_inner(&self, switching_output: bool) -> Hresult {
        let old_monitor = self.lock_state().monitor;
        let restored = !old_monitor.is_null() && wsi_restore_display_mode(old_monitor);
        let new_monitor = wsi_window_monitor(self.hwnd);
        let window_state = {
            let mut st = self.lock_state();
            if restored {
                st.init_refresh_rate = f64::MAX;
            }
            st.fullscreen_desc.windowed = 1;
            if !st.target.is_null() {
                // SAFETY: `st.target` was retained when entering fullscreen.
                unsafe { crate::com::release(st.target) };
                st.target = ptr::null_mut();
            }
            if !st.gamma_output.is_null() {
                // SAFETY: `st.gamma_output` was retained when entering fullscreen.
                unsafe { crate::com::release(st.gamma_output) };
                st.gamma_output = ptr::null_mut();
            }
            st.monitor = new_monitor;
            copy_window_state(&st.window_state)
        };
        if !wsi_is_window(self.hwnd) {
            return S_OK;
        }
        let ok = wsi_leave_fullscreen_mode(self.hwnd, &window_state, true);
        if !ok {
            return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
        }
        if !switching_output && !wsi_is_process_foreground() {
            crate::LOG.info("Leaving fullscreen in the background: minimising");
            wsi_minimize_window(self.hwnd);
        }
        if let Some(presenter) = &self.presenter {
            presenter.change_gamma_ramp(None);
        }
        S_OK
    }

    fn set_fullscreen_state_impl(&self, fullscreen: i32, target: *mut c_void) -> Hresult {
        let mut target_out1 = ptr::null_mut();
        if !target.is_null() {
            // SAFETY: `target` is a live `IDXGIOutput` COM pointer.
            let vtbl = unsafe { crate::com::vtbl::<IDXGIOutput1Vtbl>(target) };
            let iid_out1 = IID_IDXGI_OUTPUT_1;
            // SAFETY: `target` and `target_out1` are valid.
            let _ = unsafe {
                (vtbl.query_interface)(
                    target,
                    (&raw const iid_out1).cast::<c_void>(),
                    &raw mut target_out1,
                )
            };
            if !target_out1.is_null() {
                let mut desc = OutputDesc {
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
                };
                // SAFETY: `target_out1` is a live `IDXGIOutput1`.
                let out1_vtbl = unsafe { crate::com::vtbl::<IDXGIOutput1Vtbl>(target_out1) };
                // SAFETY: `target_out1` and `desc` are valid.
                let _ =
                    unsafe { (out1_vtbl.get_desc)(target_out1, (&raw mut desc).cast::<c_void>()) };
                let (windowed, cur_monitor) = {
                    let st = self.lock_state();
                    (st.fullscreen_desc.windowed, st.monitor)
                };
                if windowed == 0 && fullscreen != 0 && !ptr::eq(cur_monitor, desc.monitor) {
                    let hr = self.leave_fullscreen_mode(true);
                    if hr < 0 {
                        // SAFETY: `target_out1` was retained by `query_interface`.
                        unsafe { crate::com::release(target_out1) };
                        return hr;
                    }
                }
            }
        }
        let windowed = self.lock_state().fullscreen_desc.windowed;
        let hr = if windowed != 0 && fullscreen != 0 {
            self.enter_fullscreen_mode(target_out1)
        } else if windowed == 0 && fullscreen == 0 {
            self.leave_fullscreen_mode(false)
        } else {
            S_OK
        };
        if !target_out1.is_null() {
            // SAFETY: `target_out1` was retained by `query_interface`.
            unsafe { crate::com::release(target_out1) };
        }
        hr
    }

    fn resize_buffers_impl(
        &self,
        _buffer_count: u32,
        width: u32,
        height: u32,
        format: u32,
        _flags: u32,
    ) -> Hresult {
        let mut st = self.lock_state();
        if width == 0 || height == 0 {
            let (w, h) = wsi_window_size(self.hwnd);
            st.desc.width = w;
            st.desc.height = h;
        } else {
            st.desc.width = width;
            st.desc.height = height;
        }
        if format != DXGI_FORMAT_UNKNOWN
            && convert_swapchain_format(format) != WMT_PIXEL_FORMAT_INVALID
        {
            st.desc.format = format;
        }
        if !st.backbuffer.is_null() {
            // SAFETY: `st.backbuffer` was retained when created.
            unsafe { crate::com::release(st.backbuffer) };
            st.backbuffer = ptr::null_mut();
        }
        if !st.upscaled_backbuffer.is_null() {
            // SAFETY: `st.upscaled_backbuffer` was retained when created.
            unsafe { crate::com::release(st.upscaled_backbuffer) };
            st.upscaled_backbuffer = ptr::null_mut();
        }
        if st.desc.width == 0 || st.desc.height == 0 {
            st.backbuffer_desc.width = 1;
            st.backbuffer_desc.height = 1;
        } else {
            st.backbuffer_desc.width = st.desc.width;
            st.backbuffer_desc.height = st.desc.height;
        }
        self.apply_layer_props(&st);
        st.backbuffer_desc.format = st.desc.format;

        if !self.device.is_null() {
            let mut bb = ptr::null_mut();
            let hr = resource::create_texture2d1(
                self.device,
                &raw const st.backbuffer_desc,
                ptr::null(),
                &raw mut bb,
            );
            let bb = if hr >= 0 && !bb.is_null() {
                bb
            } else {
                // SAFETY: `self.device` is a live `ID3D11Device5` COM pointer.
                let dev_vtbl = unsafe { crate::com::vtbl::<ID3D11Device5Vtbl>(self.device) };
                let mut vtbl_bb = ptr::null_mut();
                // SAFETY: `self.device` and `st.backbuffer_desc` are valid.
                let vtbl_hr = unsafe {
                    (dev_vtbl.create_texture2d1)(
                        self.device,
                        (&raw const st.backbuffer_desc).cast::<c_void>(),
                        ptr::null(),
                        &raw mut vtbl_bb,
                    )
                };
                if vtbl_hr < 0 {
                    return E_FAIL;
                }
                vtbl_bb
            };
            st.backbuffer = bb;

            if st.enable_metalfx {
                let mut up_desc = st.backbuffer_desc;
                let factor = dxmt_util::float_cast::f32_to_u32(st.scale_factor.max(1.0));
                up_desc.width = up_desc.width.saturating_mul(factor);
                up_desc.height = up_desc.height.saturating_mul(factor);
                let mut up_bb = ptr::null_mut();
                let up_hr = resource::create_texture2d1(
                    self.device,
                    &raw const up_desc,
                    ptr::null(),
                    &raw mut up_bb,
                );
                if up_hr >= 0 && !up_bb.is_null() {
                    st.upscaled_backbuffer = up_bb;
                }
                if let Some(dev) = Device::from_iface(self.device) {
                    let color_format = resource::common(bb)
                        .and_then(|c| c.texture.as_ref())
                        .map_or_else(
                            || convert_swapchain_format(st.desc.format),
                            |t| t.info().pixel_format,
                        );
                    let output_format = resource::common(up_bb)
                        .and_then(|c| c.texture.as_ref())
                        .map_or_else(
                            || convert_swapchain_format(st.desc.format),
                            |t| t.info().pixel_format,
                        );
                    let info = WMTFXSpatialScalerInfo {
                        input_height: st.desc.height,
                        input_width: st.desc.width,
                        output_height: up_desc.height,
                        output_width: up_desc.width,
                        color_format,
                        output_format,
                    };
                    // SAFETY: `dev.core().device()` is a live `MTLDevice`.
                    let scaler = unsafe { Scaler::spatial(dev.core().device().handle(), &info) };
                    st.metalfx_scaler = Some(Arc::new(scaler));
                }
            }
        }
        S_OK
    }

    fn present1_impl(
        &self,
        sync_interval: u32,
        present_flags: u32,
        _present_parameters: *const DxgiPresentParameters,
    ) -> Hresult {
        let _present = lock(&self.present_serial);
        let window_minimized = wsi_is_minimized(self.hwnd);
        let is_fg = wsi_is_foreground(self.hwnd);
        let (width, height, swap_effect, handle_alt_tab, windowed) = {
            let st = self.lock_state();
            (
                st.desc.width,
                st.desc.height,
                st.desc.swap_effect,
                st.handle_alt_tab,
                st.fullscreen_desc.windowed,
            )
        };
        let (hr, should_exit_fs, early_return) = evaluate_present_prelude(
            sync_interval,
            present_flags,
            window_minimized,
            (width, height),
            swap_effect,
            (handle_alt_tab, windowed, is_fg),
        );
        if early_return {
            return hr;
        }
        if should_exit_fs {
            let _ = self.set_fullscreen_state_impl(0, ptr::null_mut());
        }

        let paces = guest_paces();
        let occluded = hr == DXGI_STATUS_OCCLUDED;
        let mut sync_state = None;
        let mut present = None;
        if !occluded {
            let (
                frame_id,
                latency,
                vsync_duration,
                gamma_output,
                backbuffer,
                upscaled_backbuffer,
                enable_metalfx,
                scaler,
            ) = {
                let mut st = self.lock_state();
                st.presentation_count = st.presentation_count.wrapping_add(1);
                let vsync = compute_vsync_duration(
                    sync_interval,
                    st.preferred_max_frame_rate,
                    st.init_refresh_rate,
                );
                (
                    u64::from(st.presentation_count),
                    st.frame_latency,
                    vsync,
                    st.gamma_output,
                    st.backbuffer,
                    st.upscaled_backbuffer,
                    st.enable_metalfx,
                    st.metalfx_scaler.clone(),
                )
            };

            if !gamma_output.is_null()
                && let Some(presenter) = &self.presenter
            {
                // SAFETY: `gamma_output` is a live `IMTLDXGIOutput` COM pointer.
                let g_vtbl = unsafe { crate::com::vtbl::<IMTLDXGIOutputVtbl>(gamma_output) };
                // SAFETY: `gamma_output` is valid.
                let ramp_ptr = unsafe { (g_vtbl.get_gamma_ramp)(gamma_output) }.cast::<GammaRamp>();
                if !ramp_ptr.is_null() {
                    // SAFETY: `ramp_ptr` points to a valid `GammaRamp` owned by the output.
                    presenter.change_gamma_ramp(Some(unsafe { &*ramp_ptr }));
                }
            }

            // Both waits can block for a whole frame. Do them before taking the device
            // lock so a waiter parks instead of keeping every other immediate-context
            // thread out of the device.
            sync_state = Some(sync_frame(
                self.frame_latency_fence.as_ref(),
                frame_id,
                latency,
                paces,
            ));

            if let Some(dev) = Device::from_iface(self.device)
                && dev.core().device().handle() > 0x1000
                && let Some(presenter) = self.presenter.clone()
                && let Some(bb_tex) = resource::common(backbuffer).and_then(|c| c.texture.clone())
            {
                let (pres_frame_id, metadata) = presenter.synchronize();
                let up_tex = if enable_metalfx {
                    resource::common(upscaled_backbuffer).and_then(|c| c.texture.clone())
                } else {
                    None
                };
                present = Some((
                    presenter,
                    pres_frame_id,
                    metadata,
                    bb_tex,
                    up_tex,
                    scaler,
                    vsync_duration,
                ));
            }
        }

        if let Some(dev) = Device::from_iface(self.device) {
            dev.mutex.lock();
        }
        if !self.device_context.is_null() {
            // SAFETY: `self.device_context` is a live `IMTLD3D11DeviceContext`.
            let ctx_vtbl =
                unsafe { crate::com::vtbl::<IMTLD3D11DeviceContextVtbl>(self.device_context) };
            // SAFETY: `self.device_context` is valid.
            unsafe { (ctx_vtbl.prepare_flush)(self.device_context) };
            if occluded {
                // SAFETY: `self.device_context` is valid.
                unsafe { (ctx_vtbl.commit)(self.device_context) };
                if let Some(dev) = Device::from_iface(self.device) {
                    dev.mutex.unlock();
                }
                return hr;
            }
        } else if occluded {
            if let Some(dev) = Device::from_iface(self.device) {
                dev.mutex.unlock();
            }
            return hr;
        }

        if let Some((presenter, pres_frame_id, metadata, bb_tex, up_tex, scaler, vsync_duration)) =
            present
        {
            // SAFETY: `self.device` is a live `Device` while locked by `dev.mutex`.
            let dev_mut = unsafe { &mut *self.device.cast::<Device>() };
            let frame_seq = dev_mut.core().queue().current_frame_seq();
            dev_mut
                .core_mut()
                .queue_mut()
                .chunk()
                .signal_frame_latency(frame_seq);
            dev_mut
                .core_mut()
                .queue_mut()
                .chunk()
                .retain(sync_state.take().unwrap_or_else(SyncFrameState::none));
            let sem_addr = self.present_semaphore.expose_provenance();
            dev_mut.core_mut().queue_mut().chunk().emit(move |enc| {
                if let (Some(up), Some(sc)) = (&up_tex, &scaler) {
                    enc.upscale(&bb_tex, up, sc.clone());
                    enc.present(up, presenter.clone(), vsync_duration, metadata);
                } else {
                    enc.present(&bb_tex, presenter.clone(), vsync_duration, metadata);
                }
                release_semaphore(ptr::with_exposed_provenance_mut(sem_addr), 1);
                presenter.frame_done(pres_frame_id);
            });
        }

        if !self.device_context.is_null() {
            // SAFETY: `self.device_context` is a live `IMTLD3D11DeviceContext`.
            let ctx_vtbl =
                unsafe { crate::com::vtbl::<IMTLD3D11DeviceContextVtbl>(self.device_context) };
            // SAFETY: `self.device_context` is valid.
            unsafe { (ctx_vtbl.commit)(self.device_context) };
        }
        if let Some(dev) = Device::from_iface(self.device) {
            dev.mutex.unlock();
            dev.queue().present_boundary(!paces);
        }
        hr
    }
}

impl Drop for SwapChain {
    fn drop(&mut self) {
        self.wait_until_gpu_idle();
        let st = self.lock_state();
        if !st.backbuffer.is_null() {
            // SAFETY: `st.backbuffer` was retained when created.
            unsafe { crate::com::release(st.backbuffer) };
        }
        if !st.upscaled_backbuffer.is_null() {
            // SAFETY: `st.upscaled_backbuffer` was retained when created.
            unsafe { crate::com::release(st.upscaled_backbuffer) };
        }
        if !st.target.is_null() {
            // SAFETY: `st.target` was retained when entering fullscreen.
            unsafe { crate::com::release(st.target) };
        }
        if !st.gamma_output.is_null() {
            // SAFETY: `st.gamma_output` was retained when entering fullscreen.
            unsafe { crate::com::release(st.gamma_output) };
        }
        drop(st);
        if self.native_view != 0 {
            // SAFETY: `self.native_view` was created by `CreateMetalViewFromHWND`.
            unsafe { ReleaseMetalView(self.native_view) };
        }
        close_handle(self.present_semaphore);
        // SAFETY: `self.device_context` is null or a valid COM pointer.
        unsafe { crate::com::release(self.device_context) };
        // SAFETY: `self.dxgi_device` is null or a valid COM pointer.
        unsafe { crate::com::release(self.dxgi_device) };
        // SAFETY: `self.factory` is null or a valid COM pointer.
        unsafe { crate::com::release(self.factory) };
        // SAFETY: `self.device` is null or a valid COM pointer.
        unsafe { crate::com::release(self.device) };
    }
}

/// The IIDs a swap chain answers.
const SWAPCHAIN_IIDS: [Guid; 8] = [
    IID_IUNKNOWN,
    IID_IDXGI_OBJECT,
    IID_IDXGI_DEVICE_SUB_OBJECT,
    IID_IDXGI_SWAP_CHAIN,
    IID_IDXGI_SWAP_CHAIN_1,
    IID_IDXGI_SWAP_CHAIN_2,
    IID_IDXGI_SWAP_CHAIN_3,
    IID_IDXGI_SWAP_CHAIN_4,
];

fn create_impl(
    factory: *mut c_void,
    device: *mut c_void,
    hwnd: *mut c_void,
    desc: *const DxgiSwapChainDesc1,
    fullscreen: *const DxgiSwapChainFullscreenDesc,
    restrict_to_output: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = restrict_to_output;
    if desc.is_null() || out.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `out` is non-null and writable.
    unsafe { *out = ptr::null_mut() };
    // SAFETY: `desc` is non-null.
    let mut d = unsafe { *desc };
    if d.width == 0 || d.height == 0 {
        let (w, h) = wsi_window_size(hwnd);
        d.width = w;
        d.height = h;
    }
    let fs_desc = if fullscreen.is_null() {
        DxgiSwapChainFullscreenDesc {
            refresh_rate: DxgiRational {
                numerator: 0,
                denominator: 0,
            },
            scanline_ordering: 0,
            scaling: 0,
            windowed: 1,
        }
    } else {
        // SAFETY: `fullscreen` is non-null.
        unsafe { *fullscreen }
    };

    let mut dxgi_device = ptr::null_mut();
    let mut device_context = ptr::null_mut();
    let mut native_view = 0_u64;
    let mut layer_weak = 0_u64;
    if !device.is_null() {
        // SAFETY: `device` is a live COM pointer.
        let unk_vtbl = unsafe { crate::com::vtbl::<IUnknownVtbl>(device) };
        let iid_dev = IID_IMTL_DXGI_DEVICE;
        // SAFETY: `device` and `dxgi_device` are valid.
        let qi_hr = unsafe {
            (unk_vtbl.query_interface)(
                device,
                (&raw const iid_dev).cast::<c_void>(),
                &raw mut dxgi_device,
            )
        };
        if qi_hr < 0 || dxgi_device.is_null() {
            return E_FAIL;
        }
        if let Some(dev) = Device::from_iface(device) {
            let mtl_dev = dev.core().device().handle();
            // SAFETY: `hwnd`, `mtl_dev`, and `layer_weak` are valid.
            native_view = unsafe {
                CreateMetalViewFromHWND(hwnd.addr().cast_signed(), mtl_dev, &raw mut layer_weak)
            };
            if native_view == 0 {
                // SAFETY: `dxgi_device` was retained by `query_interface`.
                unsafe { crate::com::release(dxgi_device) };
                return E_FAIL;
            }
        }
        // SAFETY: `device` is a live `ID3D11Device5` COM pointer.
        let dev5_vtbl = unsafe { crate::com::vtbl::<ID3D11Device5Vtbl>(device) };
        // SAFETY: `device` and `device_context` are valid.
        unsafe { (dev5_vtbl.get_immediate_context)(device, &raw mut device_context) };
    }

    let mut bind_flags = D3D11_BIND_RENDER_TARGET;
    if (d.buffer_usage & DXGI_USAGE_SHADER_INPUT) != 0 {
        bind_flags |= D3D11_BIND_SHADER_RESOURCE;
    }
    if (d.buffer_usage & DXGI_USAGE_UNORDERED_ACCESS) != 0 {
        bind_flags |= D3D11_BIND_UNORDERED_ACCESS;
    }
    let backbuffer_desc = D3d11Texture2dDesc1 {
        width: d.width.max(1),
        height: d.height.max(1),
        mip_levels: 1,
        array_size: 1,
        format: d.format,
        sample_desc: d.sample_desc,
        usage: 0,
        bind_flags,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: 0,
    };

    let monitor = wsi_window_monitor(hwnd);
    let init_refresh_rate = if let Some(current_mode) = wsi_current_display_mode(monitor)
        && current_mode.refresh.denominator != 0
        && current_mode.refresh.numerator != 0
    {
        f64::from(current_mode.refresh.numerator) / f64::from(current_mode.refresh.denominator)
    } else {
        f64::MAX
    };

    let enable_metalfx = settings().enabled(Flags::METALFX_SPATIAL_SWAPCHAIN);
    let scale_factor = if enable_metalfx { 2.0 } else { 1.0 };
    let presenter = if let Some(dev) = Device::from_iface(device)
        && layer_weak != 0
        && dev.core().device().handle() > 0x1000
    {
        let pdesc = PresenterDesc {
            device: dev.core().device().handle(),
            layer: layer_weak,
            library: dev.core().cmd_library_handle(),
            scale_factor,
            sample_count: u8::try_from(d.sample_desc.count.max(1)).unwrap_or(1),
        };
        // SAFETY: `device`, `layer`, and `library` are live Metal handles.
        Some(Arc::new(unsafe { Presenter::new(&pdesc) }))
    } else {
        None
    };
    let present_semaphore = create_semaphore(
        DEFAULT_SWAPCHAIN_LATENCY.cast_signed(),
        DXGI_MAX_SWAP_CHAIN_BUFFERS.cast_signed(),
    );
    let frame_latency_fence = ((d.flags & DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT) != 0)
        .then(|| Arc::new(CpuFence::new(0)));

    // SAFETY: `device` is null or a valid COM pointer.
    unsafe { crate::com::add_ref(device) };
    // SAFETY: `factory` is null or a valid COM pointer.
    unsafe { crate::com::add_ref(factory) };

    let sc = Box::new(SwapChain {
        vtbl: &raw const SWAPCHAIN_VTBL,
        refs: ComObject::with_initial_ref(),
        device,
        factory,
        dxgi_device,
        device_context,
        hwnd,
        native_view,
        layer_weak,
        presenter,
        present_semaphore,
        frame_latency_fence,
        modeset_in_progress: AtomicBool::new(false),
        private_data: PrivateData::default(),
        present_serial: Mutex::new(()),
        state: Mutex::new(SwapChainState {
            desc: d,
            fullscreen_desc: fs_desc,
            backbuffer_desc,
            backbuffer: ptr::null_mut(),
            upscaled_backbuffer: ptr::null_mut(),
            metalfx_scaler: None,
            target: ptr::null_mut(),
            gamma_output: ptr::null_mut(),
            monitor,
            window_state: WindowState::default(),
            presentation_count: 0,
            frame_latency: DEFAULT_SWAPCHAIN_LATENCY,
            color_space: DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
            init_refresh_rate,
            preferred_max_frame_rate: 0,
            scale_factor: if enable_metalfx { 2.0 } else { 1.0 },
            enable_metalfx,
            handle_alt_tab: false,
        }),
    });
    let _ = sc.resize_buffers_impl(0, d.width, d.height, DXGI_FORMAT_UNKNOWN, d.flags);
    if fs_desc.windowed == 0 {
        let _ = sc.enter_fullscreen_mode(ptr::null_mut());
    }
    let raw = Box::into_raw(sc);
    // SAFETY: `out` is non-null.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// `IMTLDXGIDevice::CreateSwapChain`.
pub fn create(
    factory: *mut c_void,
    device: *mut c_void,
    hwnd: *mut c_void,
    desc: *const DxgiSwapChainDesc1,
    fullscreen: *const DxgiSwapChainFullscreenDesc,
    restrict_to_output: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    create_impl(
        factory,
        device,
        hwnd,
        desc,
        fullscreen,
        restrict_to_output,
        out,
    )
}

/// `IMTLSwapChainFactory::CreateSwapChain` (`d3d11_swapchain.cpp:1149-1180`).
#[must_use]
pub fn create_swap_chain(
    device: *mut c_void,
    factory: *mut c_void,
    hwnd: *mut c_void,
    desc: *const DxgiSwapChainDesc1,
    fs_desc: *const DxgiSwapChainFullscreenDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_impl(factory, device, hwnd, desc, fs_desc, ptr::null_mut(), out)
}

unsafe extern "system-unwind" fn sc_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> Hresult {
    let take_ref = || {
        // SAFETY: `this` is a live `SwapChain`.
        let _ = unsafe { (*this.cast::<SwapChain>()).refs.add_ref() };
    };
    let qi = Qi {
        null_riid: E_NOINTERFACE,
        silent: &[],
        log: &crate::LOG,
        miss: QiMiss::Once {
            object: &IID_IDXGI_SWAP_CHAIN_1,
            name: "DXGISwapChain",
        },
    };
    // SAFETY: the application's `riid` and `ppv_object`, null or valid.
    unsafe { qi_table(riid, ppv_object, &[(&SWAPCHAIN_IIDS, this)], take_ref, &qi) }
}

unsafe extern "system-unwind" fn sc_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `SwapChain`.
    unsafe { (*this.cast::<SwapChain>()).refs.add_ref() }
}

unsafe extern "system-unwind" fn sc_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `SwapChain`.
    let (rem, free) = unsafe { (*this.cast::<SwapChain>()).refs.release_and_free() };
    if free {
        // SAFETY: the last reference was released; `this` was allocated via `Box::into_raw`.
        drop(unsafe { Box::from_raw(this.cast::<SwapChain>()) });
    }
    rem
}

impl HasPrivateData for SwapChain {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `SwapChain`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

unsafe extern "system-unwind" fn sc_get_parent(
    this: *mut c_void,
    riid: *const c_void,
    pp_parent: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    let factory = unsafe { (*this.cast::<SwapChain>()).factory };
    if factory.is_null() {
        if !pp_parent.is_null() {
            // SAFETY: `pp_parent` is non-null.
            unsafe { *pp_parent = ptr::null_mut() };
        }
        return E_NOINTERFACE;
    }
    // SAFETY: `factory` is a live COM pointer.
    let vtbl = unsafe { crate::com::vtbl::<IUnknownVtbl>(factory) };
    // SAFETY: `factory`, `riid`, and `pp_parent` are forwarded to `QueryInterface`.
    unsafe { (vtbl.query_interface)(factory, riid, pp_parent) }
}

unsafe extern "system-unwind" fn sc_get_device(
    this: *mut c_void,
    riid: *const c_void,
    pp_device: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    let dev = unsafe { (*this.cast::<SwapChain>()).device };
    if dev.is_null() {
        if !pp_device.is_null() {
            // SAFETY: `pp_device` is non-null.
            unsafe { *pp_device = ptr::null_mut() };
        }
        return E_NOINTERFACE;
    }
    // SAFETY: `dev` is a live COM pointer.
    let vtbl = unsafe { crate::com::vtbl::<IUnknownVtbl>(dev) };
    // SAFETY: `dev`, `riid`, and `pp_device` are forwarded to `QueryInterface`.
    unsafe { (vtbl.query_interface)(dev, riid, pp_device) }
}

unsafe extern "system-unwind" fn sc_present(
    this: *mut c_void,
    sync_interval: u32,
    flags: u32,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    unsafe { (*this.cast::<SwapChain>()).present1_impl(sync_interval, flags, ptr::null()) }
}

unsafe extern "system-unwind" fn sc_get_buffer(
    this: *mut c_void,
    buffer: u32,
    riid: *const c_void,
    pp_surface: *mut *mut c_void,
) -> Hresult {
    if buffer != 0 {
        return DXGI_ERROR_UNSUPPORTED;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let bb = sc.lock_state().backbuffer;
    if bb.is_null() {
        if !pp_surface.is_null() {
            // SAFETY: `pp_surface` is non-null.
            unsafe { *pp_surface = ptr::null_mut() };
        }
        return E_FAIL;
    }
    // SAFETY: `bb` is a live COM pointer.
    let vtbl = unsafe { crate::com::vtbl::<IUnknownVtbl>(bb) };
    // SAFETY: `bb`, `riid`, and `pp_surface` are forwarded to `QueryInterface`.
    unsafe { (vtbl.query_interface)(bb, riid, pp_surface) }
}

unsafe extern "system-unwind" fn sc_set_fullscreen_state(
    this: *mut c_void,
    fullscreen: i32,
    p_target: *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    unsafe { (*this.cast::<SwapChain>()).set_fullscreen_state_impl(fullscreen, p_target) }
}

unsafe extern "system-unwind" fn sc_get_fullscreen_state(
    this: *mut c_void,
    p_fullscreen: *mut i32,
    pp_target: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let should_exit = {
        let st = sc.lock_state();
        st.handle_alt_tab && st.fullscreen_desc.windowed == 0 && !wsi_is_foreground(sc.hwnd)
    };
    if should_exit {
        let _ = sc.set_fullscreen_state_impl(0, ptr::null_mut());
    }
    let st = sc.lock_state();
    if !p_fullscreen.is_null() {
        // SAFETY: `p_fullscreen` is non-null.
        unsafe { *p_fullscreen = i32::from(st.fullscreen_desc.windowed == 0) };
    }
    if !pp_target.is_null() {
        // SAFETY: `st.target` is null or a valid COM pointer.
        unsafe { crate::com::add_ref(st.target) };
        // SAFETY: `pp_target` is non-null.
        unsafe { *pp_target = st.target };
    }
    S_OK
}

unsafe extern "system-unwind" fn sc_get_desc(this: *mut c_void, p_desc: *mut c_void) -> Hresult {
    if p_desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let st = sc.lock_state();
    let out = DxgiSwapChainDesc {
        buffer_desc: DxgiModeDesc {
            width: st.desc.width,
            height: st.desc.height,
            refresh_rate: st.fullscreen_desc.refresh_rate,
            format: st.desc.format,
            scanline_ordering: st.fullscreen_desc.scanline_ordering,
            scaling: st.fullscreen_desc.scaling,
        },
        sample_desc: st.desc.sample_desc,
        buffer_usage: st.desc.buffer_usage,
        buffer_count: st.desc.buffer_count,
        output_window: sc.hwnd,
        windowed: st.fullscreen_desc.windowed,
        swap_effect: st.desc.swap_effect,
        flags: st.desc.flags,
    };
    drop(st);
    // SAFETY: `p_desc` is non-null and points to a `DxgiSwapChainDesc`.
    unsafe { p_desc.cast::<DxgiSwapChainDesc>().write_unaligned(out) };
    S_OK
}

unsafe extern "system-unwind" fn sc_resize_buffers(
    this: *mut c_void,
    buffer_count: u32,
    width: u32,
    height: u32,
    new_format: u32,
    swap_chain_flags: u32,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    unsafe {
        (*this.cast::<SwapChain>()).resize_buffers_impl(
            buffer_count,
            width,
            height,
            new_format,
            swap_chain_flags,
        )
    }
}

unsafe extern "system-unwind" fn sc_resize_target(
    this: *mut c_void,
    p_new_target_parameters: *const c_void,
) -> Hresult {
    if p_new_target_parameters.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    if !wsi_is_window(sc.hwnd) {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `p_new_target_parameters` is non-null and points to a `DxgiModeDesc`.
    let p = unsafe { *p_new_target_parameters.cast::<DxgiModeDesc>() };
    let mut new_mode = DxgiModeDesc1 {
        width: p.width,
        height: p.height,
        refresh_rate: p.refresh_rate,
        format: p.format,
        scanline_ordering: p.scanline_ordering,
        scaling: p.scaling,
        stereo: 0,
    };
    let (windowed, ws, target, flags, fmt) = {
        let mut st = sc.lock_state();
        if new_mode.refresh_rate.numerator != 0 {
            st.fullscreen_desc.refresh_rate = new_mode.refresh_rate;
        }
        st.fullscreen_desc.scanline_ordering = new_mode.scanline_ordering;
        st.fullscreen_desc.scaling = new_mode.scaling;
        if !st.target.is_null() {
            // SAFETY: retains `st.target` across the unlocked `change_display_mode` call.
            unsafe { crate::com::add_ref(st.target) };
        }
        (
            st.fullscreen_desc.windowed != 0,
            copy_window_state(&st.window_state),
            st.target,
            st.desc.flags,
            st.desc.format,
        )
    };
    if windowed {
        if !target.is_null() {
            // SAFETY: releases the temporary `target` reference.
            unsafe { crate::com::release(target) };
        }
        wsi_resize_window(sc.hwnd, &ws, new_mode.width, new_mode.height);
    } else {
        let (_, new_rate, _) = sc.change_display_mode(target, &mut new_mode, flags, fmt);
        if !target.is_null() {
            // SAFETY: releases the temporary `target` reference.
            unsafe { crate::com::release(target) };
        }
        let monitor = {
            let mut st = sc.lock_state();
            if let Some(r) = new_rate {
                st.init_refresh_rate = r;
            }
            st.monitor
        };
        wsi_update_fullscreen_window(monitor, sc.hwnd, false);
    }
    S_OK
}

unsafe extern "system-unwind" fn sc_get_containing_output(
    this: *mut c_void,
    pp_output: *mut *mut c_void,
) -> Hresult {
    if pp_output.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: `pp_output` is non-null.
    unsafe { *pp_output = ptr::null_mut() };
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    if !wsi_is_window(sc.hwnd) {
        return DXGI_ERROR_INVALID_CALL;
    }
    let target = sc.lock_state().target;
    if !target.is_null() {
        // SAFETY: `target` is a valid COM pointer.
        unsafe { crate::com::add_ref(target) };
        // SAFETY: `pp_output` is non-null.
        unsafe { *pp_output = target };
        return S_OK;
    }
    match sc.get_output_from_monitor(wsi_window_monitor(sc.hwnd)) {
        Ok(out) => {
            // SAFETY: `pp_output` is non-null.
            unsafe { *pp_output = out };
            S_OK
        }
        Err(hr) => hr,
    }
}

unsafe extern "system-unwind" fn sc_get_frame_statistics(
    this: *mut c_void,
    p_stats: *mut c_void,
) -> Hresult {
    if p_stats.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let count = sc.lock_state().presentation_count;
    let stats = DxgiFrameStatistics {
        present_count: count,
        present_refresh_count: count,
        sync_refresh_count: count,
        sync_qpc_time: 0,
        sync_gpu_time: 0,
    };
    // SAFETY: `p_stats` is non-null and points to a `DxgiFrameStatistics`.
    unsafe { p_stats.cast::<DxgiFrameStatistics>().write_unaligned(stats) };
    S_OK
}

unsafe extern "system-unwind" fn sc_get_last_present_count(
    this: *mut c_void,
    p_last_present_count: *mut u32,
) -> Hresult {
    if p_last_present_count.is_null() {
        return E_POINTER;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let count = sc.lock_state().presentation_count;
    // SAFETY: `p_last_present_count` is non-null.
    unsafe { *p_last_present_count = count };
    S_OK
}

unsafe extern "system-unwind" fn sc_get_desc1(this: *mut c_void, p_desc: *mut c_void) -> Hresult {
    if p_desc.is_null() {
        return E_POINTER;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let d = sc.lock_state().desc;
    // SAFETY: `p_desc` is non-null and points to a `DxgiSwapChainDesc1`.
    unsafe { p_desc.cast::<DxgiSwapChainDesc1>().write_unaligned(d) };
    S_OK
}

unsafe extern "system-unwind" fn sc_get_fullscreen_desc(
    this: *mut c_void,
    p_desc: *mut c_void,
) -> Hresult {
    if p_desc.is_null() {
        return E_POINTER;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let fd = sc.lock_state().fullscreen_desc;
    // SAFETY: `p_desc` is non-null and points to a `DxgiSwapChainFullscreenDesc`.
    unsafe {
        p_desc
            .cast::<DxgiSwapChainFullscreenDesc>()
            .write_unaligned(fd);
    }
    S_OK
}

unsafe extern "system-unwind" fn sc_get_hwnd(
    this: *mut c_void,
    p_hwnd: *mut *mut c_void,
) -> Hresult {
    if p_hwnd.is_null() {
        return E_POINTER;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let hwnd = unsafe { (*this.cast::<SwapChain>()).hwnd };
    // SAFETY: `p_hwnd` is non-null.
    unsafe { *p_hwnd = hwnd };
    S_OK
}

const unsafe extern "system-unwind" fn sc_get_core_window(
    _this: *mut c_void,
    _refiid: *const c_void,
    _pp_unk: *mut *mut c_void,
) -> Hresult {
    E_NOTIMPL
}

unsafe extern "system-unwind" fn sc_present1(
    this: *mut c_void,
    sync_interval: u32,
    present_flags: u32,
    p_present_parameters: *const c_void,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    unsafe {
        (*this.cast::<SwapChain>()).present1_impl(
            sync_interval,
            present_flags,
            p_present_parameters.cast::<DxgiPresentParameters>(),
        )
    }
}

const unsafe extern "system-unwind" fn sc_is_temporary_mono_supported(_this: *mut c_void) -> i32 {
    0
}

unsafe extern "system-unwind" fn sc_get_restrict_to_output(
    _this: *mut c_void,
    pp_restrict_to_output: *mut *mut c_void,
) -> Hresult {
    if !pp_restrict_to_output.is_null() {
        // SAFETY: `pp_restrict_to_output` is non-null.
        unsafe { *pp_restrict_to_output = ptr::null_mut() };
    }
    E_NOTIMPL
}

const unsafe extern "system-unwind" fn sc_set_background_color(
    _this: *mut c_void,
    _p_color: *const c_void,
) -> Hresult {
    E_NOTIMPL
}

const unsafe extern "system-unwind" fn sc_get_background_color(
    _this: *mut c_void,
    p_color: *mut c_void,
) -> Hresult {
    if p_color.is_null() {
        return E_INVALIDARG;
    }
    let zero = DxgiRgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    // SAFETY: `p_color` is non-null and points to a `DxgiRgba`.
    unsafe { p_color.cast::<DxgiRgba>().write_unaligned(zero) };
    S_OK
}

const unsafe extern "system-unwind" fn sc_set_rotation(
    _this: *mut c_void,
    _rotation: u32,
) -> Hresult {
    E_NOTIMPL
}

unsafe extern "system-unwind" fn sc_get_rotation(
    _this: *mut c_void,
    p_rotation: *mut u32,
) -> Hresult {
    if p_rotation.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_rotation` is non-null.
    unsafe { *p_rotation = DXGI_MODE_ROTATION_IDENTITY };
    S_OK
}

const unsafe extern "system-unwind" fn sc_set_source_size(
    _this: *mut c_void,
    _width: u32,
    _height: u32,
) -> Hresult {
    E_NOTIMPL
}

unsafe extern "system-unwind" fn sc_get_source_size(
    this: *mut c_void,
    p_width: *mut u32,
    p_height: *mut u32,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let st = sc.lock_state();
    if !p_width.is_null() {
        // SAFETY: `p_width` is non-null.
        unsafe { *p_width = st.desc.width };
    }
    if !p_height.is_null() {
        // SAFETY: `p_height` is non-null.
        unsafe { *p_height = st.desc.height };
    }
    S_OK
}

unsafe extern "system-unwind" fn sc_set_maximum_frame_latency(
    this: *mut c_void,
    max_latency: u32,
) -> Hresult {
    if max_latency == 0 || max_latency > DXGI_MAX_SWAP_CHAIN_BUFFERS {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let mut st = sc.lock_state();
    if max_latency > st.frame_latency {
        let delta = i32::try_from(max_latency - st.frame_latency).unwrap_or(0);
        release_semaphore(sc.present_semaphore, delta);
    }
    st.frame_latency = max_latency;
    drop(st);
    S_OK
}

unsafe extern "system-unwind" fn sc_get_maximum_frame_latency(
    this: *mut c_void,
    p_max_latency: *mut u32,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    if !p_max_latency.is_null() {
        let lat = sc.lock_state().frame_latency;
        // SAFETY: `p_max_latency` is non-null.
        unsafe { *p_max_latency = lat };
    }
    S_OK
}

unsafe extern "system-unwind" fn sc_get_frame_latency_waitable_object(
    this: *mut c_void,
) -> *mut c_void {
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let flags = sc.lock_state().desc.flags;
    if (flags & DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT) == 0 {
        return ptr::null_mut();
    }
    duplicate_same_access(sc.present_semaphore)
}

const unsafe extern "system-unwind" fn sc_set_matrix_transform(
    _this: *mut c_void,
    _p_matrix: *const c_void,
) -> Hresult {
    E_NOTIMPL
}

const unsafe extern "system-unwind" fn sc_get_matrix_transform(
    _this: *mut c_void,
    _p_matrix: *mut c_void,
) -> Hresult {
    E_NOTIMPL
}

const unsafe extern "system-unwind" fn sc_get_current_back_buffer_index(_this: *mut c_void) -> u32 {
    0
}

unsafe extern "system-unwind" fn sc_check_color_space_support(
    _this: *mut c_void,
    color_space: u32,
    p_color_space_support: *mut u32,
) -> Hresult {
    if p_color_space_support.is_null() {
        return E_INVALIDARG;
    }
    let wmt_cs = convert_color_space(color_space, false);
    // SAFETY: `CGColorSpace_checkColorSpaceSupported` takes a `WMTColorSpace` value.
    let supported = unsafe { CGColorSpace_checkColorSpaceSupported(wmt_cs) };
    let flags = if supported {
        DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT
    } else {
        0
    };
    // SAFETY: `p_color_space_support` is non-null.
    unsafe { *p_color_space_support = flags };
    S_OK
}

unsafe extern "system-unwind" fn sc_set_color_space1(
    this: *mut c_void,
    color_space: u32,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    let target_cs = convert_color_space(color_space, sc.layer_support_edr());
    if let Some(presenter) = &sc.presenter
        && presenter.change_layer_color_space(target_cs)
    {
        sc.wait_until_gpu_idle();
    }
    sc.lock_state().color_space = color_space;
    S_OK
}

unsafe extern "system-unwind" fn sc_resize_buffers1(
    this: *mut c_void,
    buffer_count: u32,
    width: u32,
    height: u32,
    format: u32,
    swap_chain_flags: u32,
    _p_creation_node_mask: *const u32,
    _pp_present_queue: *const *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    unsafe {
        (*this.cast::<SwapChain>()).resize_buffers_impl(
            buffer_count,
            width,
            height,
            format,
            swap_chain_flags,
        )
    }
}

unsafe extern "system-unwind" fn sc_set_hdr_metadata(
    this: *mut c_void,
    r#type: u32,
    size: u32,
    p_meta_data: *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a live `SwapChain`.
    let sc = unsafe { &*this.cast::<SwapChain>() };
    match r#type {
        DXGI_HDR_METADATA_TYPE_NONE => {
            if let Some(presenter) = &sc.presenter {
                presenter.change_hdr_metadata(None);
            }
            S_OK
        }
        DXGI_HDR_METADATA_TYPE_HDR10 => {
            if size as usize != size_of::<WMTHDRMetadata>() || p_meta_data.is_null() {
                return E_INVALIDARG;
            }
            if let Some(presenter) = &sc.presenter {
                // SAFETY: `p_meta_data` is non-null and `size == size_of::<WMTHDRMetadata>()`.
                let meta = unsafe { &*p_meta_data.cast::<WMTHDRMetadata>() };
                presenter.change_hdr_metadata(Some(meta));
            }
            S_OK
        }
        _ => DXGI_ERROR_UNSUPPORTED,
    }
}

#[cfg(test)]
mod tests;
