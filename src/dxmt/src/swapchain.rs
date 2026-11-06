//! What the D3D11 and D3D12 swap chains share.
//!
//! Format, colour-space and render-scale helpers, the state both keep behind their lock
//! ([`State`] in a [`Core`]) and the `IDXGISwapChain` getters that read it alike.
//!
//! Mirrors `ConvertSwapChainFormat`, `ConvertColorSpace`, `GetMonitorFormatBpp` and `RenderScale`
//! in `d3d11_swapchain.cpp` and `d3d12_swapchain.cpp`, and the getters both files repeat. Each
//! API keeps its `Present`, `ResizeBuffers`, `GetBuffer` and the slots whose HRESULTs or logs
//! differ.

use core::{
    ffi::c_void,
    ops::{Deref, DerefMut},
    sync::atomic::AtomicBool,
};
use std::sync::{Arc, Mutex, MutexGuard};

use dxmt_util::{
    com::{
        Hresult,
        hresult::{E_INVALIDARG, E_POINTER, S_OK},
        write_out,
    },
    dxgi_consts::{
        DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
        DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P2020, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_B8G8R8X8_UNORM,
        DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM,
        DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_MAX_SWAP_CHAIN_BUFFERS,
        DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT,
        DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
    },
    ext::{FrameStatistics, ModeDesc, SwapChainDesc, SwapChainDesc1, SwapChainFullscreenDesc},
    sync::lock,
    wsi::{Hmonitor, Hwnd, WindowState},
};
use winemetal_sys::{
    CGColorSpace_checkColorSpaceSupported, MetalLayer_getEDRValue, wire::WMTEDRValue,
};
use winemetal_wire::{
    WMT_COLOR_SPACE_BT2020, WMT_COLOR_SPACE_HDR_PQ, WMT_COLOR_SPACE_HDR_SC_RGB,
    WMT_COLOR_SPACE_INVALID, WMT_COLOR_SPACE_SRGB, WMT_COLOR_SPACE_SRGB_LINEAR,
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB, WMT_PIXEL_FORMAT_INVALID,
    WMT_PIXEL_FORMAT_RGB10A2_UNORM, WMT_PIXEL_FORMAT_RGBA16_FLOAT,
};

use crate::presenter::Presenter;

pub mod fullscreen;

/// Maps a DXGI swap-chain format to its `WMTPixelFormat`, or `WMTPixelFormatInvalid` (0).
///
/// Both `B8G8R8A8` and `R8G8B8A8` map to `BGRA8` in the Metal presenter.
#[must_use]
pub const fn convert_format(format: u32) -> u32 {
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

/// Maps a `DXGI_COLOR_SPACE_TYPE` and HDR flag to its `WMTColorSpace`.
///
/// Linear G10/P709 selects `WMTColorSpaceHDR_scRGB` when `hdr` is true and
/// `WMTColorSpaceSRGBLinear` otherwise.
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

/// Bits per pixel for a display mode's `DXGI_FORMAT`: 64 for `R16G16B16A16_FLOAT`, 32 otherwise.
///
/// Logs `GetMonitorFormatBpp: Unknown format: <n>` when the format is outside the nine display
/// formats DXMT recognises.
#[must_use]
pub fn monitor_format_bpp(format: u32) -> u32 {
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM
        | DXGI_FORMAT_B8G8R8A8_UNORM
        | DXGI_FORMAT_B8G8R8X8_UNORM
        | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
        | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
        | DXGI_FORMAT_B8G8R8X8_UNORM_SRGB
        | DXGI_FORMAT_R10G10B10A2_UNORM
        | DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM => 32,
        DXGI_FORMAT_R16G16B16A16_FLOAT => 64,
        _ => {
            crate::log::warn(&format!("GetMonitorFormatBpp: Unknown format: {format}"));
            32
        }
    }
}

/// The frame pacing interval in seconds (`vsync_duration`).
///
/// `sync_interval` refreshes at `init_refresh_rate`, no shorter than the application's
/// `preferred_max_frame_rate` allows. `d3d11_swapchain.cpp:797-801` and
/// `d3d12_swapchain.cpp:803-806`.
#[must_use]
pub fn vsync_duration(
    sync_interval: u32,
    preferred_max_frame_rate: i32,
    init_refresh_rate: f64,
) -> f64 {
    let (rate, floor) = if preferred_max_frame_rate == 0 {
        (init_refresh_rate, 0.0)
    } else {
        let preferred = f64::from(preferred_max_frame_rate);
        (preferred, 1.0 / preferred)
    };
    (f64::from(sync_interval) / rate).max(floor)
}

/// Parses a `DXMT_RENDER_SCALE` value, clamping positive numbers to `[0.25, 1.0]`.
///
/// Empty, non-positive or unparsable strings answer `1.0`.
#[must_use]
pub fn parse_render_scale(value: &str) -> f64 {
    match value.trim().parse::<f64>() {
        Ok(scale) if scale.is_finite() && scale > 0.0 => scale.clamp(0.25, 1.0),
        _ => 1.0,
    }
}

/// Reads `DXMT_RENDER_SCALE` from the environment and clamps it to `[0.25, 1.0]`.
///
/// Answers `1.0` when unset, non-positive or unparsable.
#[must_use]
pub fn render_scale() -> f64 {
    std::env::var("DXMT_RENDER_SCALE").map_or(1.0, |v| parse_render_scale(&v))
}

/// Bits per pixel for a display mode's `DXGI_FORMAT`, logging nothing.
///
/// 64 for `R16G16B16A16_FLOAT`, 32 otherwise: D3D11's `GetMonitorFormatBpp`.
#[must_use]
pub const fn monitor_format_bpp_quiet(format: u32) -> u32 {
    match format {
        DXGI_FORMAT_R16G16B16A16_FLOAT => 64,
        _ => 32,
    }
}

/// A display mode's refresh rate in hertz, when it has one (`ReadRefreshRate`).
#[must_use]
pub fn mode_refresh_rate(mode: Option<dxmt_util::wsi::WsiMode>) -> Option<f64> {
    let mode = mode?;
    (mode.refresh.denominator != 0 && mode.refresh.numerator != 0)
        .then(|| f64::from(mode.refresh.numerator) / f64::from(mode.refresh.denominator))
}

/// The swap-chain state D3D11 and D3D12 both keep behind their lock.
///
/// `X` is the API's own part, which the state dereferences to.
pub struct State<X> {
    /// `DXGI_SWAP_CHAIN_DESC1`, as `GetDesc1` answers it.
    pub desc: SwapChainDesc1,
    /// `DXGI_SWAP_CHAIN_FULLSCREEN_DESC`; `windowed` is 0 while fullscreen.
    pub fullscreen_desc: SwapChainFullscreenDesc,
    /// The monitor the window is on, or the fullscreen output's.
    pub monitor: Hmonitor,
    /// The fullscreen output (`IDXGIOutput1`) with a reference of its own, or null.
    pub target: *mut c_void,
    /// The windowed styles and rectangle saved on entering fullscreen.
    pub window_state: WindowState,
    /// Presents so far (`GetLastPresentCount`).
    pub presentation_count: u32,
    /// `SetMaximumFrameLatency`'s value.
    pub frame_latency: u32,
    /// `SetColorSpace1`'s `DXGI_COLOR_SPACE_TYPE`.
    pub color_space: u32,
    /// The display's refresh rate in hertz, `f64::MAX` when unknown.
    pub init_refresh_rate: f64,
    /// The API's own fields.
    pub api: X,
}

// SAFETY: `monitor` and `target` are a window-system handle and a COM pointer, both usable from
// any thread; the state lives behind its `Mutex`.
unsafe impl<X: Send> Send for State<X> {}

impl<X> State<X> {
    /// Replaces the fullscreen output, releasing the previous one's reference.
    ///
    /// # Safety
    ///
    /// `target` is null or a live `IDXGIOutput1` whose reference the state takes over.
    pub unsafe fn set_target(&mut self, target: *mut c_void) {
        let old = core::mem::replace(&mut self.target, target);
        // SAFETY: the state held a reference on `old` when non-null.
        unsafe { dxmt_util::com::release(old) };
    }
}

impl<X> Deref for State<X> {
    type Target = X;

    fn deref(&self) -> &X {
        &self.api
    }
}

impl<X> DerefMut for State<X> {
    fn deref_mut(&mut self) -> &mut X {
        &mut self.api
    }
}

/// The part of a swap chain both APIs share: its window, layer, presenter and [`State`].
pub struct Core<X> {
    /// The application's window.
    pub hwnd: Hwnd,
    /// The window's `CAMetalLayer`, borrowed (0 when there is none).
    pub layer: u64,
    /// The presenter, when there is a Metal device.
    pub presenter: Option<Arc<Presenter>>,
    /// Set while a fullscreen mode change runs (`ModeSetGuard`).
    modeset_in_progress: AtomicBool,
    /// The state, behind the swap chain's lock.
    pub state: Mutex<State<X>>,
}

// SAFETY: `hwnd` is a window handle any thread may use, `layer` a handle Metal shares; the
// presenter synchronises itself and the state lives behind its `Mutex`.
unsafe impl<X: Send> Send for Core<X> {}
// SAFETY: as for `Send`.
unsafe impl<X: Send> Sync for Core<X> {}

impl<X> Core<X> {
    /// A swap chain's shared part over `state`.
    #[must_use]
    pub const fn new(
        hwnd: Hwnd,
        layer: u64,
        presenter: Option<Arc<Presenter>>,
        state: State<X>,
    ) -> Self {
        Self {
            hwnd,
            layer,
            presenter,
            modeset_in_progress: AtomicBool::new(false),
            state: Mutex::new(state),
        }
    }

    /// Locks the state.
    pub fn lock(&self) -> MutexGuard<'_, State<X>> {
        lock(&self.state)
    }

    /// Whether the layer can show extended dynamic range (`LayerSupportEDR`).
    #[must_use]
    pub fn layer_support_edr(&self) -> bool {
        if self.layer == 0 {
            return false;
        }
        let mut edr = WMTEDRValue {
            maximum_edr_color_component_value: 1.0,
            maximum_potential_edr_color_component_value: 1.0,
        };
        // SAFETY: `self.layer` is the window's live `CAMetalLayer`; `edr` is a local.
        unsafe { MetalLayer_getEDRValue(self.layer, &raw mut edr) };
        edr.maximum_potential_edr_color_component_value > 1.0
    }

    /// `IDXGISwapChain::GetDesc`: `E_INVALIDARG` on a null `out`.
    ///
    /// # Safety
    ///
    /// `out` is null or addresses a writable `DXGI_SWAP_CHAIN_DESC`.
    pub unsafe fn get_desc(&self, out: *mut c_void) -> Hresult {
        if out.is_null() {
            return E_INVALIDARG;
        }
        let desc = {
            let st = self.lock();
            SwapChainDesc {
                buffer_desc: ModeDesc {
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
                output_window: self.hwnd,
                windowed: st.fullscreen_desc.windowed,
                swap_effect: st.desc.swap_effect,
                flags: st.desc.flags,
            }
        };
        // SAFETY: `out` is non-null and writable, the caller's.
        unsafe { write_out(out.cast::<SwapChainDesc>(), desc) };
        S_OK
    }

    /// `IDXGISwapChain1::GetDesc1`: `E_POINTER` on a null `out`.
    ///
    /// # Safety
    ///
    /// `out` is null or addresses a writable `DXGI_SWAP_CHAIN_DESC1`.
    pub unsafe fn get_desc1(&self, out: *mut c_void) -> Hresult {
        if out.is_null() {
            return E_POINTER;
        }
        let desc = self.lock().desc;
        // SAFETY: `out` is non-null and writable, the caller's.
        unsafe { write_out(out.cast::<SwapChainDesc1>(), desc) };
        S_OK
    }

    /// `IDXGISwapChain1::GetFullscreenDesc`: `E_POINTER` on a null `out`.
    ///
    /// # Safety
    ///
    /// `out` is null or addresses a writable `DXGI_SWAP_CHAIN_FULLSCREEN_DESC`.
    pub unsafe fn get_fullscreen_desc(&self, out: *mut c_void) -> Hresult {
        if out.is_null() {
            return E_POINTER;
        }
        let desc = self.lock().fullscreen_desc;
        // SAFETY: `out` is non-null and writable, the caller's.
        unsafe { write_out(out.cast::<SwapChainFullscreenDesc>(), desc) };
        S_OK
    }

    /// `IDXGISwapChain1::GetHwnd`: `E_POINTER` on a null `out`.
    ///
    /// # Safety
    ///
    /// `out` is null or writable.
    pub const unsafe fn get_hwnd(&self, out: *mut Hwnd) -> Hresult {
        if out.is_null() {
            return E_POINTER;
        }
        // SAFETY: `out` is non-null and writable, the caller's.
        unsafe { write_out(out, self.hwnd) };
        S_OK
    }

    /// `IDXGISwapChain::GetLastPresentCount`: `E_POINTER` on a null `out`.
    ///
    /// # Safety
    ///
    /// `out` is null or writable.
    pub unsafe fn get_last_present_count(&self, out: *mut u32) -> Hresult {
        if out.is_null() {
            return E_POINTER;
        }
        let count = self.lock().presentation_count;
        // SAFETY: `out` is non-null and writable, the caller's.
        unsafe { write_out(out, count) };
        S_OK
    }

    /// `IDXGISwapChain::GetFrameStatistics`' answer.
    ///
    /// Every count is the present count, the times zero. Each API checks `out` for null with its
    /// own HRESULT first.
    ///
    /// # Safety
    ///
    /// `out` is non-null and addresses a writable `DXGI_FRAME_STATISTICS`.
    pub unsafe fn write_frame_statistics(&self, out: *mut c_void) {
        let count = self.lock().presentation_count;
        let stats = FrameStatistics {
            present_count: count,
            present_refresh_count: count,
            sync_refresh_count: count,
            sync_qpc_time: 0,
            sync_gpu_time: 0,
        };
        // SAFETY: the caller's.
        unsafe { write_out(out.cast::<FrameStatistics>(), stats) };
    }

    /// `IDXGISwapChain::GetFullscreenState`.
    ///
    /// Fullscreen or not, and the output with a reference for the caller. Either pointer may be
    /// null.
    ///
    /// # Safety
    ///
    /// Each pointer is null or writable.
    pub unsafe fn get_fullscreen_state(
        &self,
        fullscreen: *mut i32,
        target: *mut *mut c_void,
    ) -> Hresult {
        let st = self.lock();
        if !fullscreen.is_null() {
            // SAFETY: non-null and writable, the caller's.
            unsafe { write_out(fullscreen, i32::from(st.fullscreen_desc.windowed == 0)) };
        }
        if !target.is_null() {
            // SAFETY: `st.target` is null or a live output this state holds.
            unsafe { dxmt_util::com::add_ref(st.target) };
            // SAFETY: non-null and writable, the caller's.
            unsafe { write_out(target, st.target) };
        }
        S_OK
    }

    /// `IDXGISwapChain2::SetMaximumFrameLatency`: `E_INVALIDARG` outside `1..=16`.
    ///
    /// A raise hands the extra count to `release` (the present semaphore's), under the lock.
    pub fn set_maximum_frame_latency(
        &self,
        max_latency: u32,
        release: impl FnOnce(i32),
    ) -> Hresult {
        if max_latency == 0 || max_latency > DXGI_MAX_SWAP_CHAIN_BUFFERS {
            return E_INVALIDARG;
        }
        let mut st = self.lock();
        if max_latency > st.frame_latency {
            release(i32::try_from(max_latency - st.frame_latency).unwrap_or(0));
        }
        st.frame_latency = max_latency;
        S_OK
    }

    /// `IDXGISwapChain2::GetMaximumFrameLatency`: a null `out` is answered `S_OK`.
    ///
    /// # Safety
    ///
    /// `out` is null or writable.
    pub unsafe fn get_maximum_frame_latency(&self, out: *mut u32) -> Hresult {
        if !out.is_null() {
            let latency = self.lock().frame_latency;
            // SAFETY: non-null and writable, the caller's.
            unsafe { write_out(out, latency) };
        }
        S_OK
    }

    /// `IDXGISwapChain2::GetFrameLatencyWaitableObject`.
    ///
    /// `duplicate`'s handle when the swap chain was made with `FRAME_LATENCY_WAITABLE_OBJECT`,
    /// else null.
    pub fn frame_latency_waitable_object(
        &self,
        duplicate: impl FnOnce() -> *mut c_void,
    ) -> *mut c_void {
        let flags = self.lock().desc.flags;
        if flags & DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT == 0 {
            return core::ptr::null_mut();
        }
        duplicate()
    }

    /// `IDXGISwapChain3::SetColorSpace1`: retargets the layer, then records `color_space`.
    ///
    /// Answers whether the layer changed.
    pub fn set_color_space1(&self, color_space: u32) -> bool {
        let target = convert_color_space(color_space, self.layer_support_edr());
        let changed = self
            .presenter
            .as_ref()
            .is_some_and(|presenter| presenter.change_layer_color_space(target));
        self.lock().color_space = color_space;
        changed
    }
}

/// `IDXGISwapChain3::CheckColorSpaceSupport`: `E_INVALIDARG` on a null `out`.
///
/// # Safety
///
/// `out` is null or writable.
pub unsafe fn check_color_space_support(color_space: u32, out: *mut u32) -> Hresult {
    if out.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: CoreGraphics answers for a `WMTColorSpace` value.
    let supported =
        unsafe { CGColorSpace_checkColorSpaceSupported(convert_color_space(color_space, false)) };
    let flags = if supported {
        DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT
    } else {
        0
    };
    // SAFETY: non-null and writable, the caller's.
    unsafe { write_out(out, flags) };
    S_OK
}

#[cfg(test)]
mod tests;
