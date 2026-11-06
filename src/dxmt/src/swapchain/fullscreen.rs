//! The fullscreen and display-mode-set machine of a swap chain, behind a small host trait.
//!
//! `EnterFullscreenMode`, `LeaveFullscreenMode`, `ChangeDisplayMode`, `GetOutputFromMonitor`,
//! `SetFullscreenState`, `ResizeTarget` and `GetContainingOutput` of `d3d11_swapchain.cpp` and
//! `d3d12_swapchain.cpp`, once. What differs per API is the [`Host`]'s: where the adapter comes
//! from, which log the lines go to and whether the mode-set diagnostics are written (D3D12's),
//! what entering and leaving do to the API's own state, and the window system, which D3D11's tests
//! replace.
//!
//! A target output handed to [`enter_fullscreen_mode`] is consumed: its reference becomes the
//! state's, or is released when entering fails.

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use dxmt_util::{
    com::{
        Hresult,
        hresult::{
            DXGI_ERROR_INVALID_CALL, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE, DXGI_ERROR_NOT_FOUND,
            DXGI_STATUS_MODE_CHANGE_IN_PROGRESS, E_FAIL, S_OK,
        },
        iid::IID_IDXGI_OUTPUT1,
        vtbl, write_out,
    },
    dxgi_consts::{
        DXGI_FORMAT_UNKNOWN, DXGI_MODE_SCANLINE_ORDER_LOWER_FIELD_FIRST,
        DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST, DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH,
    },
    dxgi_vtbl::{IDXGIAdapterVtbl, IDXGIOutput1Vtbl},
    ext::{ModeDesc, ModeDesc1, OutputDesc, Rect},
    log::Logger,
    wsi::{self, Hmonitor, Hwnd, Rational, WindowState, WsiMode},
};

use super::{Core, State, monitor_format_bpp, monitor_format_bpp_quiet};

/// What the machine asks of the API's swap chain and of the window system.
///
/// The window-system functions default to [`dxmt_util::wsi`]'s; an API's tests override them.
pub trait Host {
    /// The API's own part of [`State`].
    type Api;

    /// The shared part of the swap chain.
    fn core(&self) -> &Core<Self::Api>;

    /// The swap chain's `IDXGIAdapter`, with a reference the caller releases, or `None`.
    fn adapter(&self) -> Option<*mut c_void>;

    /// The DLL's log, which the machine's lines go to.
    fn log() -> &'static Logger;

    /// Whether the mode-set diagnostics are written.
    ///
    /// D3D12's errors, warning and `Setting display mode` line; its display-mode bits per pixel
    /// also logs an unknown format.
    const DIAGNOSTICS: bool = false;

    /// Entering fullscreen is about to make `output` the state's target.
    ///
    /// # Safety
    ///
    /// `output` is a live `IDXGIOutput1`.
    unsafe fn target_changing(&self, _state: &mut State<Self::Api>, _output: *mut c_void) {}

    /// Leaving fullscreen released the target and recorded the window's monitor.
    ///
    /// `restored` says whether the display mode came back.
    fn left(&self, state: &mut State<Self::Api>, restored: bool);

    /// `wsi::getWindowSize`.
    #[must_use]
    fn window_size(hwnd: Hwnd) -> (u32, u32) {
        wsi::window_size(hwnd)
    }
    /// `wsi::getWindowMonitor`.
    #[must_use]
    fn window_monitor(hwnd: Hwnd) -> Hmonitor {
        wsi::window_monitor(hwnd)
    }
    /// `wsi::getCurrentDisplayMode`.
    #[must_use]
    fn current_display_mode(monitor: Hmonitor) -> Option<WsiMode> {
        wsi::current_display_mode(monitor)
    }
    /// `wsi::isWindow`.
    #[must_use]
    fn is_window(hwnd: Hwnd) -> bool {
        wsi::is_window(hwnd)
    }
    /// `wsi::isMinimized`.
    #[must_use]
    fn is_minimized(hwnd: Hwnd) -> bool {
        wsi::is_minimized(hwnd)
    }
    /// `wsi::isForeground`.
    #[must_use]
    fn is_foreground(hwnd: Hwnd) -> bool {
        wsi::is_foreground(hwnd)
    }
    /// `wsi::isProcessForeground`.
    #[must_use]
    fn is_process_foreground() -> bool {
        wsi::is_process_foreground()
    }
    /// `wsi::minimizeWindow`.
    fn minimize_window(hwnd: Hwnd) {
        wsi::minimize_window(hwnd);
    }
    /// `wsi::saveWindowState`.
    fn save_window_state(hwnd: Hwnd, state: &mut WindowState) {
        wsi::save_window_state(hwnd, state);
    }
    /// `wsi::setWindowMode`.
    #[must_use]
    fn set_window_mode(monitor: Hmonitor, hwnd: Hwnd, mode: &WsiMode) -> bool {
        wsi::set_window_mode(monitor, hwnd, mode)
    }
    /// `wsi::enterFullscreenMode`.
    #[must_use]
    fn enter_fullscreen_mode(
        monitor: Hmonitor,
        hwnd: Hwnd,
        state: &mut WindowState,
        mode_switch: bool,
    ) -> bool {
        wsi::enter_fullscreen_mode(monitor, hwnd, state, mode_switch)
    }
    /// `wsi::leaveFullscreenMode`.
    #[must_use]
    fn leave_fullscreen_mode(hwnd: Hwnd, state: &WindowState, restore_coordinates: bool) -> bool {
        wsi::leave_fullscreen_mode(hwnd, state, restore_coordinates)
    }
    /// `wsi::restoreDisplayMode`.
    #[must_use]
    fn restore_display_mode(monitor: Hmonitor) -> bool {
        wsi::restore_display_mode(monitor)
    }
    /// `wsi::resizeWindow`.
    fn resize_window(hwnd: Hwnd, state: &WindowState, width: u32, height: u32) {
        wsi::resize_window(hwnd, state, width, height);
    }
    /// `wsi::updateFullscreenWindow`.
    fn update_fullscreen_window(monitor: Hmonitor, hwnd: Hwnd, force_topmost: bool) {
        wsi::update_fullscreen_window(monitor, hwnd, force_topmost);
    }
}

/// The log line of leaving fullscreen while the process is in the background.
pub const BACKGROUND_LEAVE_LOG: &str = "Leaving fullscreen in the background: minimising";

/// Minimises `hwnd` when leaving fullscreen in the background, unless switching outputs.
///
/// `d3d12_swapchain.cpp:420-423`.
pub fn leave_fullscreen_background<IsFg, Log, Min>(
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

/// Holds the mode-set flag for one change (`ModeSetGuard`).
struct ModeSet<'a> {
    flag: &'a AtomicBool,
    busy: bool,
}

impl<'a> ModeSet<'a> {
    fn enter(flag: &'a AtomicBool) -> Self {
        let busy = flag.swap(true, Ordering::AcqRel);
        Self { flag, busy }
    }
}

impl Drop for ModeSet<'_> {
    fn drop(&mut self) {
        if !self.busy {
            self.flag.store(false, Ordering::Release);
        }
    }
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

/// `output`'s `DXGI_OUTPUT_DESC`.
unsafe fn output_desc(output: *mut c_void) -> OutputDesc {
    let mut desc = empty_output_desc();
    // SAFETY: the caller's live `IDXGIOutput1`.
    let output_vtbl = unsafe { vtbl::<IDXGIOutput1Vtbl>(output) };
    // SAFETY: `output` is live and `desc` a local.
    let _ = unsafe { (output_vtbl.get_desc)(output, (&raw mut desc).cast()) };
    desc
}

/// The adapter's output on `monitor` as `IDXGIOutput1`, with a reference for the caller.
///
/// `GetOutputFromMonitor`.
///
/// `E_FAIL` without an adapter, `DXGI_ERROR_NOT_FOUND` when no output is on `monitor`, else the
/// query's HRESULT when it fails.
///
/// # Safety
///
/// `host.adapter()` answers a live adapter.
unsafe fn output_for_monitor<H: Host>(host: &H, monitor: Hmonitor) -> Result<*mut c_void, Hresult> {
    let Some(adapter) = host.adapter() else {
        return Err(E_FAIL);
    };
    // SAFETY: `adapter` is a live `IDXGIAdapter`.
    let adapter_vtbl = unsafe { vtbl::<IDXGIAdapterVtbl>(adapter) };
    let mut index = 0_u32;
    let found = loop {
        let mut output = ptr::null_mut();
        // SAFETY: `adapter` is live; `output` is a local.
        let hr = unsafe { (adapter_vtbl.enum_outputs)(adapter, index, &raw mut output) };
        if hr < 0 || output.is_null() {
            break Err(DXGI_ERROR_NOT_FOUND);
        }
        // SAFETY: `enum_outputs` answered a live output.
        if unsafe { output_desc(output) }.monitor == monitor {
            let mut output1 = ptr::null_mut();
            // SAFETY: `output` is a live `IDXGIOutput`.
            let output_vtbl = unsafe { vtbl::<IDXGIOutput1Vtbl>(output) };
            // SAFETY: `output` is live; the IID and `output1` are locals.
            let hr = unsafe {
                (output_vtbl.query_interface)(
                    output,
                    ptr::from_ref(&IID_IDXGI_OUTPUT1).cast(),
                    &raw mut output1,
                )
            };
            // SAFETY: gives back `enum_outputs`' reference.
            unsafe { dxmt_util::com::release(output) };
            break if hr < 0 || output1.is_null() {
                Err(hr)
            } else {
                Ok(output1)
            };
        }
        // SAFETY: gives back `enum_outputs`' reference.
        unsafe { dxmt_util::com::release(output) };
        index = index.wrapping_add(1);
    };
    // SAFETY: gives back `adapter()`'s reference.
    unsafe { dxmt_util::com::release(adapter) };
    found
}

/// Switches the display to the mode closest to `mode` on `output` (`ChangeDisplayMode`).
///
/// On success `mode` becomes that mode, the state's refresh rate its rate, and the output's
/// monitor is answered.
///
/// # Safety
///
/// `output` is null or a live `IDXGIOutput1`.
unsafe fn change_display_mode<H: Host>(
    host: &H,
    output: *mut c_void,
    mode: &mut ModeDesc1,
) -> Result<Hmonitor, Hresult> {
    if output.is_null() {
        return Err(DXGI_ERROR_INVALID_CALL);
    }
    let core = host.core();
    let (flags, format) = {
        let st = core.lock();
        (st.desc.flags, st.desc.format)
    };
    let mut preferred = *mode;
    if flags & DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH == 0 {
        preferred.width = 0;
        preferred.height = 0;
    }
    if preferred.format == DXGI_FORMAT_UNKNOWN {
        preferred.format = format;
    }
    let mut selected = ModeDesc1::default();
    // SAFETY: the caller's live output.
    let output_vtbl = unsafe { vtbl::<IDXGIOutput1Vtbl>(output) };
    // SAFETY: `output` is live; both modes are locals.
    let hr = unsafe {
        (output_vtbl.find_closest_matching_mode1)(
            output,
            (&raw const preferred).cast(),
            (&raw mut selected).cast(),
            ptr::null_mut(),
        )
    };
    if hr < 0 {
        if H::DIAGNOSTICS {
            dxmt_util::err!(
                H::log(),
                "DXGI: Failed to query closest mode:\n  Format: {}\n  Mode:   {}x{}@{}",
                preferred.format,
                preferred.width,
                preferred.height,
                preferred.refresh_rate.numerator / preferred.refresh_rate.denominator.max(1)
            );
        }
        return Err(hr);
    }
    if selected.refresh_rate.denominator == 0 {
        selected.refresh_rate.denominator = 1;
    }
    // SAFETY: `output` is live.
    let monitor = unsafe { output_desc(output) }.monitor;
    let wsi_mode = WsiMode {
        width: selected.width,
        height: selected.height,
        refresh: Rational {
            numerator: selected.refresh_rate.numerator,
            denominator: selected.refresh_rate.denominator,
        },
        bits_per_pixel: if H::DIAGNOSTICS {
            monitor_format_bpp(selected.format)
        } else {
            monitor_format_bpp_quiet(selected.format)
        },
        interlaced: selected.scanline_ordering == DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST
            || selected.scanline_ordering == DXGI_MODE_SCANLINE_ORDER_LOWER_FIELD_FIRST,
    };
    if !H::set_window_mode(monitor, core.hwnd, &wsi_mode) {
        return Err(DXGI_ERROR_NOT_CURRENTLY_AVAILABLE);
    }
    if H::DIAGNOSTICS {
        let hz = if wsi_mode.refresh.numerator == 0 {
            0
        } else {
            wsi_mode.refresh.numerator / wsi_mode.refresh.denominator.max(1)
        };
        dxmt_util::info!(
            H::log(),
            "Setting display mode: {}x{}@{}",
            wsi_mode.width,
            wsi_mode.height,
            hz
        );
    }
    *mode = selected;
    core.lock().init_refresh_rate =
        f64::from(selected.refresh_rate.numerator) / f64::from(selected.refresh_rate.denominator);
    Ok(monitor)
}

/// `EnterFullscreenMode`: the window goes fullscreen on `target`.
///
/// On its own monitor's output when `target` is null. Consumes `target`'s reference.
///
/// # Safety
///
/// `target` is null or a live `IDXGIOutput1` with a reference the call takes over.
pub unsafe fn enter_fullscreen_mode<H: Host>(host: &H, target: *mut c_void) -> Hresult {
    let core = host.core();
    let guard = ModeSet::enter(&core.modeset_in_progress);
    if guard.busy {
        // SAFETY: the caller's reference.
        unsafe { dxmt_util::com::release(target) };
        return DXGI_STATUS_MODE_CHANGE_IN_PROGRESS;
    }
    if !H::is_window(core.hwnd) {
        // SAFETY: the caller's reference.
        unsafe { dxmt_util::com::release(target) };
        return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
    }
    let output = if target.is_null() {
        // SAFETY: the host's adapter is live.
        let Ok(output) = (unsafe { output_for_monitor(host, H::window_monitor(core.hwnd)) }) else {
            if H::DIAGNOSTICS {
                dxmt_util::err!(
                    H::log(),
                    "DXGI: EnterFullscreenMode: Cannot query containing output"
                );
            }
            return E_FAIL;
        };
        output
    } else {
        target
    };
    let mut saved = core.lock().window_state;
    H::save_window_state(core.hwnd, &mut saved);
    let mut preferred = {
        let mut st = core.lock();
        st.window_state = saved;
        ModeDesc1 {
            width: st.desc.width,
            height: st.desc.height,
            refresh_rate: st.fullscreen_desc.refresh_rate,
            format: st.desc.format,
            ..ModeDesc1::default()
        }
    };
    // SAFETY: `output` is a live `IDXGIOutput1` this call holds.
    let Ok(monitor) = (unsafe { change_display_mode(host, output, &mut preferred) }) else {
        if H::DIAGNOSTICS {
            dxmt_util::err!(
                H::log(),
                "DXGI: EnterFullscreenMode: Failed to change display mode"
            );
        }
        // SAFETY: the reference this call holds.
        unsafe { dxmt_util::com::release(output) };
        return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
    };
    let mode_switch = {
        let mut st = core.lock();
        st.fullscreen_desc.windowed = 0;
        st.monitor = monitor;
        // SAFETY: `output` is a live `IDXGIOutput1`.
        unsafe { host.target_changing(&mut st, output) };
        // SAFETY: `output`'s reference becomes the state's.
        unsafe { st.set_target(output) };
        st.desc.flags & DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH != 0
    };
    let entered = H::enter_fullscreen_mode(monitor, core.hwnd, &mut saved, mode_switch);
    core.lock().window_state = saved;
    if !entered {
        if H::DIAGNOSTICS {
            dxmt_util::err!(
                H::log(),
                "DXGI: EnterFullscreenMode: Failed to enter fullscreen mode"
            );
        }
        return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
    }
    S_OK
}

/// `LeaveFullscreenMode`: restores the display mode and the window.
///
/// Minimises the window when the process is in the background, unless `switching_output`.
pub fn leave_fullscreen_mode<H: Host>(host: &H, switching_output: bool) -> Hresult {
    let core = host.core();
    let guard = ModeSet::enter(&core.modeset_in_progress);
    if guard.busy {
        return DXGI_STATUS_MODE_CHANGE_IN_PROGRESS;
    }
    let old_monitor = core.lock().monitor;
    let restored = !old_monitor.is_null() && H::restore_display_mode(old_monitor);
    if !restored && H::DIAGNOSTICS {
        dxmt_util::warn!(
            H::log(),
            "DXGI: LeaveFullscreenMode: Failed to restore display mode"
        );
    }
    let new_monitor = H::window_monitor(core.hwnd);
    let saved = {
        let mut st = core.lock();
        st.fullscreen_desc.windowed = 1;
        // SAFETY: releases the state's target, if any.
        unsafe { st.set_target(ptr::null_mut()) };
        st.monitor = new_monitor;
        host.left(&mut st, restored);
        st.window_state
    };
    if !H::is_window(core.hwnd) {
        return S_OK;
    }
    if !H::leave_fullscreen_mode(core.hwnd, &saved, true) {
        if H::DIAGNOSTICS {
            dxmt_util::err!(
                H::log(),
                "DXGI: LeaveFullscreenMode: Failed to exit fullscreen mode"
            );
        }
        return DXGI_ERROR_NOT_CURRENTLY_AVAILABLE;
    }
    leave_fullscreen_background(
        core.hwnd,
        switching_output,
        H::is_process_foreground,
        |line| H::log().info(line),
        H::minimize_window,
    );
    if let Some(presenter) = &core.presenter {
        presenter.change_gamma_ramp(None);
    }
    S_OK
}

/// `IDXGISwapChain::SetFullscreenState`: enters or leaves fullscreen as asked.
///
/// A fullscreen on another monitor than `target`'s is left first.
///
/// # Safety
///
/// `target` is null or a live `IDXGIOutput` the caller keeps.
pub unsafe fn set_fullscreen_state<H: Host>(
    host: &H,
    fullscreen: i32,
    target: *mut c_void,
) -> Hresult {
    let core = host.core();
    let want_fullscreen = fullscreen != 0;
    let mut output = ptr::null_mut();
    // SAFETY: `target` is null or a live output.
    if let Some(queried) = unsafe { dxmt_util::com::query(target, &IID_IDXGI_OUTPUT1) } {
        output = queried;
        // SAFETY: `output` is live.
        let monitor = unsafe { output_desc(output) }.monitor;
        let switching = {
            let st = core.lock();
            st.fullscreen_desc.windowed == 0 && want_fullscreen && st.monitor != monitor
        };
        if switching {
            let hr = leave_fullscreen_mode(host, true);
            if hr < 0 {
                // SAFETY: the query's reference.
                unsafe { dxmt_util::com::release(output) };
                return hr;
            }
        }
    }
    let windowed = core.lock().fullscreen_desc.windowed != 0;
    if windowed && want_fullscreen {
        // SAFETY: `output` is null or holds the query's reference, which the call takes over.
        return unsafe { enter_fullscreen_mode(host, output) };
    }
    // SAFETY: the query's reference, if any.
    unsafe { dxmt_util::com::release(output) };
    if !windowed && !want_fullscreen {
        return leave_fullscreen_mode(host, false);
    }
    S_OK
}

/// `IDXGISwapChain::ResizeTarget`.
///
/// Records the mode's refresh rate, scanline order and scaling, then resizes the window, or
/// changes the display mode while fullscreen.
///
/// # Safety
///
/// `params` is null or addresses a `DXGI_MODE_DESC`.
pub unsafe fn resize_target<H: Host>(host: &H, params: *const c_void) -> Hresult {
    let core = host.core();
    if params.is_null() || !H::is_window(core.hwnd) {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: non-null, the caller's; read unaligned.
    let params = unsafe { params.cast::<ModeDesc>().read_unaligned() };
    let mut mode = ModeDesc1 {
        width: params.width,
        height: params.height,
        refresh_rate: params.refresh_rate,
        format: params.format,
        scanline_ordering: params.scanline_ordering,
        scaling: params.scaling,
        stereo: 0,
    };
    let (windowed, saved, target, monitor) = {
        let mut st = core.lock();
        if mode.refresh_rate.numerator != 0 {
            st.fullscreen_desc.refresh_rate = mode.refresh_rate;
        }
        st.fullscreen_desc.scanline_ordering = mode.scanline_ordering;
        st.fullscreen_desc.scaling = mode.scaling;
        // SAFETY: keeps the target alive across the unlocked mode change.
        unsafe { dxmt_util::com::add_ref(st.target) };
        (
            st.fullscreen_desc.windowed != 0,
            st.window_state,
            st.target,
            st.monitor,
        )
    };
    if windowed {
        // SAFETY: the reference taken above.
        unsafe { dxmt_util::com::release(target) };
        H::resize_window(core.hwnd, &saved, mode.width, mode.height);
    } else {
        // SAFETY: `target` is null or held above.
        let _ = unsafe { change_display_mode(host, target, &mut mode) };
        // SAFETY: the reference taken above.
        unsafe { dxmt_util::com::release(target) };
        H::update_fullscreen_window(monitor, core.hwnd, false);
    }
    S_OK
}

/// `IDXGISwapChain::GetContainingOutput`: `null_out` when `out` is null.
///
/// Else the fullscreen output or the window's monitor's, with a reference for the caller.
///
/// # Safety
///
/// `out` is null or writable.
pub unsafe fn get_containing_output<H: Host>(
    host: &H,
    out: *mut *mut c_void,
    null_out: Hresult,
) -> Hresult {
    if out.is_null() {
        return null_out;
    }
    // SAFETY: non-null and writable, the caller's.
    unsafe { write_out(out, ptr::null_mut()) };
    let core = host.core();
    if !H::is_window(core.hwnd) {
        return DXGI_ERROR_INVALID_CALL;
    }
    let target = {
        let st = core.lock();
        // SAFETY: null or a live output the state holds; the caller gets a reference of its own.
        unsafe { dxmt_util::com::add_ref(st.target) };
        st.target
    };
    if !target.is_null() {
        // SAFETY: non-null and writable, the caller's.
        unsafe { write_out(out, target) };
        return S_OK;
    }
    // SAFETY: the host's adapter is live.
    match unsafe { output_for_monitor(host, H::window_monitor(core.hwnd)) } {
        Ok(output) => {
            // SAFETY: as above; `output`'s reference becomes the caller's.
            unsafe { write_out(out, output) };
            S_OK
        }
        Err(hr) => hr,
    }
}

#[cfg(test)]
mod tests;
