//! The implicit swap chain on the core, and Present (C4-11).
//!
//! The back buffer, the window's layer and the presenter on it. A multisampled back buffer is a
//! multisampled texture beside the single-sample one, which draws and clears go to and Present
//! resolves into the single-sample one; the presenter stays at one sample (DXMTRS C4b-3d5).

use core::sync::atomic::{AtomicU64, Ordering};
use std::{
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use dxmt_core::{
    presenter::{D3d9PresentArgs, Presenter, PresenterDesc},
    resource::{Texture, TextureAllocationFlags, TextureViewKey, texture_info},
    scaler::Scaler,
};
use mtld3d_core::present::{PresentGeometry, geometry_settled};
use winemetal_sys::{
    MTLDevice_newSharedEvent, Obj, WMTAttachMetalView, WMTPresentClientSurface, WMTRetireMetalView,
    WMTSetCursorOverlay, WMTViewSyncAndPoll,
    wire::{
        MemPtr, WMT_CLIENT_SURFACE_REASON_VIEW_HIDDEN, WMT_PIXEL_FORMAT_BGRA8_UNORM,
        WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_RETIRE_METAL_VIEW_FLAG_PARK,
        WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW, WMT_TEXTURE_USAGE_RENDER_TARGET,
        WMT_TEXTURE_USAGE_SHADER_READ, WMT_TEXTURE_USAGE_SHADER_WRITE,
        WMT_VIEW_ATTACH_FLAG_COLOR_SPACE_ACCURATE, WMT_VIEW_ATTACH_FLAG_HDR_ENABLE,
        WMT_VIEW_ATTACH_FLAG_PRESENT_OCCLUDED, WMT_VIEW_ATTACH_FLAG_SOFTWARE_CURSOR_OFF,
        WMT_VIEW_ATTACH_FLAG_SOFTWARE_CURSOR_ON, WMT_VIEW_POLL_FLAG_CLIENT_SURFACE_DIRTY,
        WMT_VIEW_POLL_FLAG_COLORSPACE_CHANGED, WMT_VIEW_POLL_FLAG_CURSOR_KICK,
        WMT_VIEW_POLL_FLAG_DISPLAYS_NOTHING, WMT_VIEW_POLL_FLAG_HDR_ACTIVE,
        WMT_VIEW_POLL_FLAG_OCCLUDED, WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE,
        WMT_VIEW_SYNC_FLAG_FORCE_REFRESH, WMTCursorOverlayParams, WMTFXSpatialScalerInfo,
        WMTTextureInfo, WMTViewAttachDesc, WMTViewPollOut,
    },
};

use super::{
    ClearTarget, CoreDevice, CoreOptions, LOG_TARGET, TargetRef, metalfx_available, textures::two_d,
};

/// The back buffer: a BGRA8 texture of the swap chain's size, which the presenter shows.
///
/// Draws and clears go to it, or to its multisampled companion when it has one.
pub struct BackBuffer {
    pub texture: Arc<Texture>,
    pub key: TextureViewKey,
    pub width: u32,
    pub height: u32,
    /// The multisampled texture of a multisampled back buffer; none for one sample.
    pub companion: Option<Companion>,
}

/// A multisampled back buffer's drawn texture: BGRA8 of its size and count (DXMTRS C4b-3d5).
pub struct Companion {
    pub texture: Arc<Texture>,
    pub key: TextureViewKey,
    pub samples: u8,
    /// The `WMTPixelFormat` of the view the last pass or Clear wrote it through.
    ///
    /// BGRA8 or its sRGB twin; Present resolves it through that view.
    pub last_written: u32,
}

impl BackBuffer {
    /// The back buffer draws and clears go to: its texture and view, and its sample count.
    ///
    /// The companion when there is one: what a pass, a Clear, a target's shape and a resolve's
    /// source take.
    #[must_use]
    pub fn drawn(&self) -> (&Arc<Texture>, TextureViewKey, u8) {
        self.companion
            .as_ref()
            .map_or((&self.texture, self.key, 1), |companion| {
                (&companion.texture, companion.key, companion.samples)
            })
    }

    /// The back buffer the presenter shows: the single-sample texture and its view.
    #[must_use]
    pub const fn presented(&self) -> (&Arc<Texture>, TextureViewKey) {
        (&self.texture, self.key)
    }
}

/// Parameters for updating the host-side software cursor overlay via `WMTSetCursorOverlay`.
pub struct CursorOverlayUpdate<'a> {
    pub flags: u32,
    pub width: u32,
    pub height: u32,
    pub x_hotspot: u32,
    pub y_hotspot: u32,
    pub scale: u32,
    pub pixels: Option<&'a [u8]>,
}

/// Shared view handle and gamma LUT state used by cursor overlay updates.
pub struct OverlaySink {
    view: AtomicU64,
    gamma: Mutex<(u64, Option<Arc<[u16; 1024]>>)>,
}

impl OverlaySink {
    pub(super) const fn new() -> Self {
        Self {
            view: AtomicU64::new(0),
            gamma: Mutex::new((0, None)),
        }
    }

    pub(super) fn set_view(&self, view: u64) {
        self.view.store(view, Ordering::Release);
    }

    pub(super) fn clear_view(&self, expected: u64) {
        let _ = self
            .view
            .compare_exchange(expected, 0, Ordering::AcqRel, Ordering::Acquire);
    }

    pub(super) fn set_gamma(&self, revision: u64, lut: Option<Arc<[u16; 1024]>>) {
        let mut guard = self.gamma.lock().unwrap_or_else(PoisonError::into_inner);
        *guard = (revision, lut);
    }

    /// Returns the attached Metal view handle, or `0` when headless or detached.
    #[must_use]
    pub fn view_handle(&self) -> u64 {
        self.view.load(Ordering::Acquire)
    }

    /// Sends a cursor overlay update (slot 164) with the active gamma LUT attached.
    #[must_use]
    pub fn set_cursor_overlay(&self, update: &CursorOverlayUpdate<'_>) -> bool {
        let view = self.view.load(Ordering::Acquire);
        if view == 0 {
            return false;
        }
        let (gamma_revision, gamma_lut_arc) = {
            let guard = self.gamma.lock().unwrap_or_else(PoisonError::into_inner);
            (guard.0, guard.1.clone())
        };
        let (pixels, pixels_len) = match update.pixels {
            Some(bytes) if !bytes.is_empty() => (
                MemPtr::from_ptr(bytes.as_ptr()),
                u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            ),
            _ => (MemPtr::NULL, 0),
        };
        let (gamma_lut, gamma_revision) = match gamma_lut_arc.as_deref() {
            Some(lut) if gamma_revision != 0 => (MemPtr::from_ptr(lut.as_ptr()), gamma_revision),
            _ => (MemPtr::NULL, 0),
        };
        let params = WMTCursorOverlayParams {
            view,
            pixels,
            gamma_lut,
            gamma_revision,
            pixels_len,
            width: update.width,
            height: update.height,
            x_hotspot: update.x_hotspot,
            y_hotspot: update.y_hotspot,
            scale: update.scale,
            flags: update.flags,
            reserved: 0,
        };
        // SAFETY: `view` is live or harmlessly missing on the host; `pixels` and `gamma_lut`
        // outlive this synchronous call.
        unsafe { WMTSetCursorOverlay(view, &raw const params) };
        true
    }
}

/// The window's Metal view (slot 159), adopted layer, presented shared event, and presenter.
pub struct Window {
    view: u64,
    layer: Option<Obj>,
    presenter: Option<Arc<Presenter>>,
    presented_event: Option<Obj>,
    next_signal: u64,
    park_on_retire: bool,
    overlay: Arc<OverlaySink>,
}

impl Drop for Window {
    fn drop(&mut self) {
        self.overlay.clear_view(self.view);
        // Drop the presenter first so any drawable handles and prefetchers stop using the layer.
        self.presenter = None;
        self.layer = None;
        let flags = if self.park_on_retire {
            WMT_RETIRE_METAL_VIEW_FLAG_PARK
        } else {
            0
        };
        // SAFETY: `self.view` came from `WMTAttachMetalView` and is retired once here.
        unsafe { WMTRetireMetalView(self.view, flags) };
        self.presented_event = None;
    }
}

/// Cached `MetalFX` spatial scaler and private scratch textures for one settled geometry.
pub(super) struct UpscaleResources {
    src: (u32, u32),
    dst: (u32, u32),
    scaler: Arc<Scaler>,
    upscaled_output: Arc<Texture>,
    gamma_scratch: Option<Arc<Texture>>,
}

/// Cached `MetalFX` spatial upscale state for one settled `(src, dst)` geometry.
pub(super) enum UpscaleCache {
    Ready(UpscaleResources),
    Unavailable { src: (u32, u32), dst: (u32, u32) },
}

impl UpscaleCache {
    const fn geometry(&self) -> ((u32, u32), (u32, u32)) {
        match self {
            Self::Ready(res) => (res.src, res.dst),
            Self::Unavailable { src, dst } => (*src, *dst),
        }
    }
}

/// Resolved `MetalFX` spatial upscale handles for a single present.
struct UpscaleStep {
    scaler: Arc<Scaler>,
    output: Arc<Texture>,
    gamma_scratch: Option<Arc<Texture>>,
}

/// Allocates a private `BGRA8Unorm` 2D texture for `MetalFX` output or pre-upscale gamma.
fn alloc_private_bgra8(
    device: &winemetal_sys::Obj,
    width: u32,
    height: u32,
    usage: u32,
) -> Option<Arc<Texture>> {
    let info = WMTTextureInfo {
        pixel_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
        width,
        height,
        depth: 1,
        array_length: 1,
        bits: texture_info::info_bits(two_d(1), 1, 1, usage),
        options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let texture = Arc::new(Texture::new(info, device));
    let alloc = texture.allocate(TextureAllocationFlags::GPU_PRIVATE);
    if alloc.texture() == 0 {
        return None;
    }
    texture.rename(alloc);
    Some(texture)
}

impl CoreDevice {
    /// Makes the `width` x `height` back buffer, and the window's Metal view and presenter.
    ///
    /// `hwnd` 0 is no window: the device then presents nothing. With `samples` above one the
    /// back buffer has a multisampled companion of that count, zeroed, which draws and clears go
    /// to (DXMTRS C4b-3d5); the single-sample texture is made first either way.
    pub fn attach(&mut self, hwnd: isize, width: u32, height: u32, samples: u8) {
        self.attach_scaled(hwnd, width, height, width, height, samples);
    }

    /// Makes a scaled back buffer and presents into a Metal layer.
    ///
    /// Creates a `render_w` x `render_h` back buffer and presents into a `drawable_w` x
    /// `drawable_h` layer.
    pub fn attach_scaled(
        &mut self,
        hwnd: isize,
        render_w: u32,
        render_h: u32,
        drawable_w: u32,
        drawable_h: u32,
        samples: u8,
    ) {
        if self.back_buffer.is_some() || self.window.is_some() {
            self.drain();
        }
        self.window = None;
        self.drawable_size = (drawable_w, drawable_h);
        self.settled_geometry = None;
        self.upscale = None;
        self.make_back_buffer(render_w, render_h, samples);
        self.window = self.open_window(hwnd, drawable_w, drawable_h);
        self.hud.get_or_insert_with(|| {
            let mut hud = dxmt_core::hud::Hud::instance();
            hud.initialize(&dxmt_core::info::version_text(
                9,
                0x9300,
                env!("DXMT_VERSION"),
            ));
            hud
        });
    }

    /// Whether the device currently holds a live back buffer.
    #[must_use]
    pub const fn has_back_buffer(&self) -> bool {
        self.back_buffer.is_some()
    }

    /// Makes the back buffer again, `width` x `height` of `samples`, in place: a `Reset`'s.
    ///
    /// Returns `false` without updating the layer when `width == 0`, `height == 0`, or Metal
    /// refuses an allocation.
    #[must_use]
    pub fn resize(&mut self, width: u32, height: u32, samples: u8) -> bool {
        self.resize_scaled(width, height, width, height, samples)
    }

    /// Rebuilds the back buffer and updates the layer drawable size.
    ///
    /// Rebuilds at `render_w` x `render_h` and sets the layer drawable to `drawable_w` x
    /// `drawable_h`. Drains queued GPU work and drops cached upscaler state before allocating the
    /// new back buffer.
    #[must_use]
    pub fn resize_scaled(
        &mut self,
        render_w: u32,
        render_h: u32,
        drawable_w: u32,
        drawable_h: u32,
        samples: u8,
    ) -> bool {
        self.drain();
        self.last_paced = None;
        self.upscale = None;
        self.settled_geometry = None;
        self.clear_history.forget(ClearTarget::BackBuffer);
        self.make_back_buffer(render_w, render_h, samples);
        if self.back_buffer.is_none() {
            return false;
        }
        self.drawable_size = (drawable_w, drawable_h);
        if let Some(w) = self.window.as_ref() {
            let mut poll = WMTViewPollOut {
                drawable_width: drawable_w,
                drawable_height: drawable_h,
                backing_scale: self.backing_scale,
                edr_current: 1.0,
                edr_potential: 1.0,
                panel_max_hz: 0,
                flags: 0,
                reserved: 0,
            };
            let layer = w.layer.as_ref().map_or(0, Obj::handle);
            // SAFETY: `w.view` and `layer` remain live while `w` is attached.
            unsafe {
                WMTViewSyncAndPoll(
                    w.view,
                    layer,
                    WMT_VIEW_SYNC_FLAG_FORCE_REFRESH,
                    &raw mut poll,
                );
            };
            if poll.backing_scale != 0 {
                self.backing_scale = poll.backing_scale.clamp(1, 8);
            }
            self.options.set(
                CoreOptions::SOFTWARE_CURSOR_ACTIVE,
                poll.flags & WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE != 0,
            );
            let dw = if poll.drawable_width > 0 {
                poll.drawable_width
            } else {
                drawable_w
            };
            let dh = if poll.drawable_height > 0 {
                poll.drawable_height
            } else {
                drawable_h
            };
            let hdr_active = self.options.contains(CoreOptions::HDR_ENABLE)
                && ((poll.flags & WMT_VIEW_POLL_FLAG_HDR_ACTIVE != 0)
                    || (poll.edr_potential > 1.0 && poll.edr_potential.is_finite()));
            let colorspace_changed = poll.flags & WMT_VIEW_POLL_FLAG_COLORSPACE_CHANGED != 0;
            if let Some(presenter) = w.presenter.as_deref() {
                presenter.sync_d3d9_managed(dw, dh, hdr_active, colorspace_changed);
            }
        }
        true
    }

    /// Moves the device to window `hwnd`, its back buffer kept: a `Reset` naming another window.
    ///
    /// The work recorded so far is drained first; the old window's presenter and then its Metal
    /// view are dropped, and a view and a presenter are made on `hwnd` at the drawable size.
    /// True when the device can present afterwards. False for `hwnd` 0, for a window winemetal
    /// gives no view or layer (logged) and for a device without a back buffer: the device then
    /// has no window and presents nothing, as one whose [`CoreDevice::attach`] made no view. Each
    /// call makes a view, and winemac keeps a client surface for every view made on a window
    /// until the window is destroyed: a caller moves the device only to another window.
    pub fn retarget(&mut self, hwnd: isize) -> bool {
        self.drain();
        self.last_paced = None;
        self.upscale = None;
        self.settled_geometry = None;
        self.window = None;
        let Some((width, height)) = self.back_buffer.as_ref().map(|b| (b.width, b.height)) else {
            return false;
        };
        let (dw, dh) = if self.drawable_size.0 > 0 && self.drawable_size.1 > 0 {
            self.drawable_size
        } else {
            (width, height)
        };
        self.window = self.open_window(hwnd, dw, dh);
        self.window.is_some()
    }

    /// Logs once when `MetalFX` spatial upscaler setup or allocation fails.
    fn log_upscale_unavailable(&mut self, src: (u32, u32), dst: (u32, u32)) {
        if !self.options.contains(CoreOptions::LOGGED_UPSCALE_FAIL) {
            self.options.insert(CoreOptions::LOGGED_UPSCALE_FAIL);
            log::warn!(
                target: LOG_TARGET,
                "d3d9 core: MetalFX spatial scaler unavailable ({}x{} -> {}x{}), using bilinear present",
                src.0,
                src.1,
                dst.0,
                dst.1
            );
        }
    }

    /// Ensures cached `MetalFX` upscale state matches `(src, dst)` when geometry has settled.
    fn prepare_upscale_resources(
        &mut self,
        src: (u32, u32),
        dst: (u32, u32),
        need_gamma_scratch: bool,
    ) -> Option<UpscaleStep> {
        let stale = self
            .upscale
            .as_ref()
            .is_some_and(|u| u.geometry() != (src, dst));
        if stale {
            self.upscale = None;
        }
        if matches!(self.upscale, Some(UpscaleCache::Unavailable { .. })) {
            return None;
        }
        if self.upscale.is_none() {
            let mtl = self.device.device();
            let info = WMTFXSpatialScalerInfo {
                color_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
                output_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
                input_width: src.0,
                input_height: src.1,
                output_width: dst.0,
                output_height: dst.1,
            };
            // SAFETY: `mtl` holds a live `MTLDevice` handle.
            let scaler = unsafe { Scaler::spatial(mtl.handle(), &info) };
            let scaler_ok = scaler.scaler().is_some() && scaler.fence().is_some();
            let upscaled_output = scaler_ok.then(|| {
                alloc_private_bgra8(
                    mtl,
                    dst.0,
                    dst.1,
                    WMT_TEXTURE_USAGE_RENDER_TARGET
                        | WMT_TEXTURE_USAGE_SHADER_READ
                        | WMT_TEXTURE_USAGE_SHADER_WRITE,
                )
            });
            let Some(upscaled_output) = upscaled_output.flatten() else {
                self.log_upscale_unavailable(src, dst);
                self.upscale = Some(UpscaleCache::Unavailable { src, dst });
                return None;
            };
            log::log!(
                target: LOG_TARGET,
                super::gate_level(),
                "d3d9 core: MetalFX spatial scaler engaged ({}x{} -> {}x{})",
                src.0,
                src.1,
                dst.0,
                dst.1
            );
            self.upscale = Some(UpscaleCache::Ready(UpscaleResources {
                src,
                dst,
                scaler: Arc::new(scaler),
                upscaled_output,
                gamma_scratch: None,
            }));
        }
        let gamma_scratch = if need_gamma_scratch {
            let existing = match self.upscale.as_ref() {
                Some(UpscaleCache::Ready(res)) => res.gamma_scratch.as_ref().map(Arc::clone),
                _ => None,
            };
            let scratch = existing.or_else(|| {
                let allocated = alloc_private_bgra8(
                    self.device.device(),
                    src.0,
                    src.1,
                    WMT_TEXTURE_USAGE_RENDER_TARGET | WMT_TEXTURE_USAGE_SHADER_READ,
                )?;
                if let Some(UpscaleCache::Ready(res)) = self.upscale.as_mut() {
                    res.gamma_scratch = Some(Arc::clone(&allocated));
                }
                Some(allocated)
            });
            let Some(scratch) = scratch else {
                self.log_upscale_unavailable(src, dst);
                self.upscale = Some(UpscaleCache::Unavailable { src, dst });
                return None;
            };
            Some(scratch)
        } else {
            None
        };
        let Some(UpscaleCache::Ready(res)) = self.upscale.as_ref() else {
            return None;
        };
        Some(UpscaleStep {
            scaler: Arc::clone(&res.scaler),
            output: Arc::clone(&res.upscaled_output),
            gamma_scratch,
        })
    }

    /// Presents the back buffer using `panel_max_hz` from `WMTViewSyncAndPoll` for pacing.
    ///
    /// Computes `(after, cap_after)` from the host screen's `panel_max_hz` when non-zero, falling
    /// back to `fallback_hz`. Combines `extra_cs_reasons` with
    /// `WMT_VIEW_POLL_FLAG_CLIENT_SURFACE_DIRTY` into one `WMTPresentClientSurface` call.
    pub fn present_paced(
        &mut self,
        visible: bool,
        interval: u32,
        fallback_hz: u32,
        max_fps: u32,
        extra_cs_reasons: u32,
    ) {
        let poll = self.poll_window(0, extra_cs_reasons);
        let hz = poll.as_ref().map_or(fallback_hz, |p| {
            if p.panel_max_hz > 0 {
                p.panel_max_hz
            } else {
                fallback_hz
            }
        });
        let pacing = mtld3d_core::present::layer_pacing_for(interval, hz, max_fps);
        let (after, cap_after) = present_pacing(pacing, hz);
        self.present_with_poll(poll, visible, after, cap_after, pacing.display_sync);
    }

    /// Presents the back buffer `after` seconds after the last present (0: at once).
    ///
    /// D3D11's order: the present closure and the frame-latency signal in the chunk, the commit,
    /// then the frame's bound, which waits for the frame `max_latency` back. A hidden window
    /// (`visible` false), or an `OCCLUDED` window without `PRESENT_OCCLUDED`, skips drawable
    /// acquisition and paces on the CPU clock; a `DISPLAYS_NOTHING` window keeps acquiring and
    /// presenting drawables with slot 163 attached while pacing on the CPU clock.
    pub fn present(&mut self, visible: bool, after: f64, cap_after: f64, display_sync: bool) {
        let poll = self.poll_window(0, 0);
        self.present_with_poll(poll, visible, after, cap_after, display_sync);
    }

    fn poll_window(&mut self, sync_flags: u32, extra_cs_reasons: u32) -> Option<WMTViewPollOut> {
        let w = self.window.as_ref()?;
        let mut poll = WMTViewPollOut {
            drawable_width: self.drawable_size.0,
            drawable_height: self.drawable_size.1,
            backing_scale: self.backing_scale,
            edr_current: 1.0,
            edr_potential: 1.0,
            panel_max_hz: 0,
            flags: 0,
            reserved: 0,
        };
        let layer = w.layer.as_ref().map_or(0, Obj::handle);
        // SAFETY: `w.view` and `layer` remain live while `w` is attached.
        unsafe { WMTViewSyncAndPoll(w.view, layer, sync_flags, &raw mut poll) };
        if poll.backing_scale != 0 {
            self.backing_scale = poll.backing_scale.clamp(1, 8);
        }
        self.options.set(
            CoreOptions::SOFTWARE_CURSOR_ACTIVE,
            poll.flags & WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE != 0,
        );
        if poll.flags & WMT_VIEW_POLL_FLAG_CURSOR_KICK != 0 {
            self.options.insert(CoreOptions::CURSOR_KICK_PENDING);
        }
        let mut cs_reasons = extra_cs_reasons;
        if poll.flags & WMT_VIEW_POLL_FLAG_CLIENT_SURFACE_DIRTY != 0 {
            cs_reasons |= WMT_CLIENT_SURFACE_REASON_VIEW_HIDDEN;
        }
        if cs_reasons != 0 {
            // SAFETY: `w.view` is live while `w` is attached.
            unsafe { WMTPresentClientSurface(w.view, cs_reasons) };
        }
        Some(poll)
    }

    fn present_with_poll(
        &mut self,
        poll: Option<WMTViewPollOut>,
        visible: bool,
        after: f64,
        cap_after: f64,
        display_sync: bool,
    ) {
        let occluded = poll
            .as_ref()
            .is_some_and(|p| p.flags & WMT_VIEW_POLL_FLAG_OCCLUDED != 0);
        let displays_nothing = poll
            .as_ref()
            .is_some_and(|p| p.flags & WMT_VIEW_POLL_FLAG_DISPLAYS_NOTHING != 0);
        let colorspace_changed = poll
            .as_ref()
            .is_some_and(|p| p.flags & WMT_VIEW_POLL_FLAG_COLORSPACE_CHANGED != 0);
        let allow_present =
            visible && (!occluded || self.options.contains(CoreOptions::PRESENT_OCCLUDED));
        let bb_dims = self.back_buffer.as_ref().map(|bb| (bb.width, bb.height));
        let has_window = self.window.as_ref().is_some_and(|w| w.presenter.is_some());
        if colorspace_changed && allow_present && has_window && bb_dims.is_some() {
            self.drain();
        }
        if let (Some((bb_w, bb_h)), true, true) = (bb_dims, allow_present, has_window) {
            let (dw, dh) = match poll.as_ref() {
                Some(p) if p.drawable_width > 0 && p.drawable_height > 0 => {
                    (p.drawable_width, p.drawable_height)
                }
                _ if self.drawable_size.0 > 0 && self.drawable_size.1 > 0 => self.drawable_size,
                _ => (bb_w, bb_h),
            };
            let hdr_enable = self.options.contains(CoreOptions::HDR_ENABLE);
            let hdr_active = hdr_enable
                && poll.as_ref().is_some_and(|p| {
                    (p.flags & WMT_VIEW_POLL_FLAG_HDR_ACTIVE != 0)
                        || (p.edr_potential > 1.0 && p.edr_potential.is_finite())
                });
            let peak_edr = poll
                .as_ref()
                .filter(|p| hdr_active && p.edr_current > 1.0 && p.edr_current.is_finite())
                .map_or(1.0, |p| p.edr_current);
            let (presenter, view_handle, event_handle, signal_value) = {
                let w = self.window.as_mut().expect("window checked present");
                w.next_signal += 1;
                (
                    Arc::clone(w.presenter.as_ref().expect("presenter checked present")),
                    w.view,
                    w.presented_event.as_ref().map_or(0, Obj::handle),
                    w.next_signal,
                )
            };
            presenter.sync_d3d9_managed(dw, dh, hdr_active, colorspace_changed);
            let paced = display_sync || after > 0.0 || cap_after > 0.0;
            if self.back_buffer.is_some() {
                let geometry = PresentGeometry {
                    src: (
                        usize::try_from(bb_w).unwrap_or(usize::MAX),
                        usize::try_from(bb_h).unwrap_or(usize::MAX),
                    ),
                    dst: (
                        usize::try_from(dw).unwrap_or(usize::MAX),
                        usize::try_from(dh).unwrap_or(usize::MAX),
                    ),
                };
                let settled = geometry_settled(&mut self.settled_geometry, geometry);
                let upscale_bundle =
                    if geometry.enlarges() && settled && metalfx_available(self.device.device()) {
                        self.prepare_upscale_resources((bb_w, bb_h), (dw, dh), self.gamma.is_some())
                    } else {
                        if !geometry.enlarges() {
                            self.upscale = None;
                        }
                        None
                    };
                if let Some(texture) = self
                    .back_buffer
                    .as_ref()
                    .map(|bb| Arc::clone(bb.presented().0))
                {
                    self.resolve_presented();
                    let queue = self.device.queue_mut();
                    let frame = queue.current_frame_seq();
                    queue.chunk().signal_frame_latency(frame);
                    if let Some(UpscaleStep {
                        scaler,
                        output: upscaled_output,
                        gamma_scratch,
                    }) = upscale_bundle
                    {
                        let upscale_input = if let (Some(lut), Some(scratch)) =
                            (self.gamma.clone(), gamma_scratch)
                        {
                            let gamma_presenter = Arc::clone(&presenter);
                            let src = Arc::clone(&texture);
                            let dst = Arc::clone(&scratch);
                            self.emit(move |enc| {
                                enc.d3d9_gamma_pass(
                                    &src,
                                    &dst,
                                    Arc::clone(&gamma_presenter),
                                    Arc::clone(&lut),
                                );
                            });
                            scratch
                        } else {
                            texture
                        };
                        let out = Arc::clone(&upscaled_output);
                        self.emit(move |enc| {
                            enc.upscale(&upscale_input, &out, Arc::clone(&scaler));
                        });
                        let args = D3d9PresentArgs {
                            gamma: None,
                            peak_edr,
                            view: view_handle,
                            presented_event: event_handle,
                            presented_value: signal_value,
                            paced,
                        };
                        self.emit(move |enc| {
                            enc.present_d3d9(
                                &upscaled_output,
                                Arc::clone(&presenter),
                                after,
                                args.clone(),
                            );
                        });
                    } else {
                        let args = D3d9PresentArgs {
                            gamma: self.gamma.clone(),
                            peak_edr,
                            view: view_handle,
                            presented_event: event_handle,
                            presented_value: signal_value,
                            paced,
                        };
                        self.emit(move |enc| {
                            enc.present_d3d9(&texture, Arc::clone(&presenter), after, args.clone());
                        });
                    }
                    self.commit();
                    self.device.queue().present_boundary(true);
                    #[cfg(all(debug_assertions, not(feature = "perf-build")))]
                    if let Some(hud) = self.hud.as_mut() {
                        let statistics = self.device.queue().statistics();
                        hud.update_statistics(&statistics, frame);
                        drop(statistics);
                    }
                } else if self.dirty || self.render_open {
                    self.commit();
                }
            } else if self.dirty || self.render_open {
                self.commit();
            }
        } else if self.dirty || self.render_open {
            self.commit();
        }
        self.end_frame_clear_tracking();
        crate::stats::frame();
        let cpu_pace = if !allow_present || displays_nothing {
            cap_after.max(after)
        } else {
            cap_after
        };
        if cpu_pace > 0.0 && cpu_pace.is_finite() {
            let dur = Duration::from_secs_f64(cpu_pace);
            let now = Instant::now();
            let deadline = next_paced_deadline(self.last_paced, now, dur);
            if let Some(wait) = deadline.checked_duration_since(now)
                && !wait.is_zero()
            {
                std::thread::sleep(wait);
            }
            self.last_paced = Some(deadline);
        } else {
            self.last_paced = None;
        }
    }

    /// Sets or clears the inline `[u16; 1024]` gamma lookup table applied from the next present.
    pub fn set_gamma_ramp(&mut self, lut: Option<[u16; 1024]>) {
        let arc = lut.map(Arc::new);
        let rev = if arc.is_some() {
            let next = self.gamma_revision.wrapping_add(1);
            self.gamma_revision = if next == 0 { 1 } else { next };
            self.gamma_revision
        } else {
            0
        };
        self.gamma.clone_from(&arc);
        self.overlay.set_gamma(rev, arc);
    }

    /// Returns the attached Metal view handle (`0` when headless or detached).
    #[must_use]
    pub fn view_handle(&self) -> u64 {
        self.window.as_ref().map_or(0, |w| w.view)
    }

    /// Returns the shared cursor overlay sink attached to this device.
    #[must_use]
    pub fn overlay_sink(&self) -> Arc<OverlaySink> {
        Arc::clone(&self.overlay)
    }

    /// Updates the host-side software cursor overlay (slot 164) with the current gamma LUT.
    #[must_use]
    pub fn set_cursor_overlay(&self, update: &CursorOverlayUpdate<'_>) -> bool {
        self.overlay.set_cursor_overlay(update)
    }

    /// Returns the display backing scale (`1..=8`) published by `WMTAttachMetalView` / poll.
    #[must_use]
    pub const fn backing_scale(&self) -> u32 {
        self.backing_scale
    }

    /// Returns whether the host reported software cursor overlay mode active.
    #[must_use]
    pub const fn software_cursor_active(&self) -> bool {
        self.options.contains(CoreOptions::SOFTWARE_CURSOR_ACTIVE)
    }

    /// Consumes the pending cursor re-apply kick latched by `WMTAttachMetalView` / poll.
    pub fn take_cursor_kick(&mut self) -> bool {
        let kicked = self.options.contains(CoreOptions::CURSOR_KICK_PENDING);
        self.options.remove(CoreOptions::CURSOR_KICK_PENDING);
        kicked
    }

    /// Calls `WMTPresentClientSurface` (slot 162) on the attached view with `reasons`.
    pub fn present_client_surface(&self, reasons: u32) {
        if reasons != 0
            && let Some(w) = self.window.as_ref()
        {
            // SAFETY: `w.view` is live while `w` is attached.
            unsafe { WMTPresentClientSurface(w.view, reasons) };
        }
    }

    /// Ends the open pass, then resolves a multisampled back buffer into the presented texture.
    ///
    /// The companion through the view its last pass or Clear wrote it in (DXMTRS C4b-3d5): folded
    /// into the last pass when that pass drew it, a resolve pass of its own otherwise. Nothing
    /// more for a single-sample back buffer, whose drawn texture is the presented one. What
    /// Present records every frame, and a read of the back buffer as Present shows it before its
    /// copy (DXMTRS C4c-3a).
    pub(super) fn resolve_presented(&mut self) {
        self.close_pass();
        let Some(back_buffer) = &self.back_buffer else {
            return;
        };
        let Some(companion) = &back_buffer.companion else {
            return;
        };
        let (shown, key) = back_buffer.presented();
        let (resolved, companion_key) = (Arc::clone(shown), companion.key);
        let companion = Arc::clone(&companion.texture);
        let format = self
            .last_written(TargetRef::BackBuffer)
            .unwrap_or(WMT_PIXEL_FORMAT_BGRA8_UNORM);
        let (source, destination) = (companion_key.with_format(format), key.with_format(format));
        self.emit(move |enc| enc.resolve_texture(&companion, source, &resolved, destination));
    }

    /// Makes the `width` x `height` back buffer of `samples`, in place of the one before.
    ///
    /// The single-sample texture first, then for `samples` above one the companion. Rejects zero
    /// dimensions or a null Metal allocation before queueing any zero-init work or setting `dirty`.
    fn make_back_buffer(&mut self, width: u32, height: u32, samples: u8) {
        if width == 0 || height == 0 {
            self.back_buffer = None;
            self.target_gen = self.target_gen.wrapping_add(1);
            return;
        }
        let mtl = self.device.device();
        let info = |samples: u8| WMTTextureInfo {
            pixel_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
            width,
            height,
            depth: 1,
            array_length: 1,
            bits: texture_info::info_bits(
                two_d(samples),
                1,
                u32::from(samples),
                // The view usage: sRGB write attaches the sRGB twin's view (DXMTRS C4b-3d3).
                WMT_TEXTURE_USAGE_RENDER_TARGET
                    | WMT_TEXTURE_USAGE_SHADER_READ
                    | WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW,
            ),
            options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
            flags: 0,
            mach_port: 0,
            gpu_resource_id: 0,
        };
        let texture = Arc::new(Texture::new(info(1), mtl));
        let alloc = texture.allocate(TextureAllocationFlags::GPU_PRIVATE);
        if alloc.texture() == 0 {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: failed allocating back buffer {width}x{height}"
            );
            self.back_buffer = None;
            self.target_gen = self.target_gen.wrapping_add(1);
            return;
        }
        texture.rename(alloc);
        let key = texture.full_view();
        // Zeroed as a render-target texture is, ahead of the next chunk.
        let companion = if samples > 1 {
            let texture = Arc::new(Texture::new(info(samples), mtl));
            let comp_alloc = texture.allocate(TextureAllocationFlags::GPU_PRIVATE);
            if comp_alloc.texture() == 0 {
                log::error!(
                    target: LOG_TARGET,
                    "d3d9 core: failed allocating {samples}x back buffer companion {width}x{height}"
                );
                self.back_buffer = None;
                self.target_gen = self.target_gen.wrapping_add(1);
                return;
            }
            texture.rename(Arc::clone(&comp_alloc));
            let initializer = self.device.queue().initializer();
            let _ = initializer.init_texture_zero(&texture, &comp_alloc, 0, 0);
            Some(Companion {
                key: texture.full_view(),
                texture,
                samples,
                last_written: WMT_PIXEL_FORMAT_BGRA8_UNORM,
            })
        } else {
            None
        };
        self.dirty |= companion.is_some();
        self.back_buffer = Some(BackBuffer {
            texture,
            key,
            width,
            height,
            companion,
        });
        self.target_gen = self.target_gen.wrapping_add(1);
    }

    /// Attaches a Metal view on window `hwnd` and builds a managed presenter on its layer.
    ///
    /// None for `hwnd` 0, and for a window winemetal gives no view or layer (logged).
    fn open_window(&mut self, hwnd: isize, width: u32, height: u32) -> Option<Window> {
        if hwnd == 0 {
            return None;
        }
        let mtl = self.device.device();
        let mut attach_flags = 0u32;
        if self.options.contains(CoreOptions::HDR_ENABLE) {
            attach_flags |= WMT_VIEW_ATTACH_FLAG_HDR_ENABLE;
        }
        if self.options.contains(CoreOptions::COLOR_SPACE_ACCURATE) {
            attach_flags |= WMT_VIEW_ATTACH_FLAG_COLOR_SPACE_ACCURATE;
        }
        if self.options.contains(CoreOptions::SOFTWARE_CURSOR_ON) {
            attach_flags |= WMT_VIEW_ATTACH_FLAG_SOFTWARE_CURSOR_ON;
        }
        if self.options.contains(CoreOptions::SOFTWARE_CURSOR_OFF) {
            attach_flags |= WMT_VIEW_ATTACH_FLAG_SOFTWARE_CURSOR_OFF;
        }
        if self.options.contains(CoreOptions::PRESENT_OCCLUDED) {
            attach_flags |= WMT_VIEW_ATTACH_FLAG_PRESENT_OCCLUDED;
        }
        let attach_desc = WMTViewAttachDesc {
            width,
            height,
            flags: attach_flags,
            reserved: 0,
        };
        let mut poll = WMTViewPollOut {
            drawable_width: width,
            drawable_height: height,
            backing_scale: 1,
            edr_current: 1.0,
            edr_potential: 1.0,
            panel_max_hz: 0,
            flags: 0,
            reserved: 0,
        };
        let mut ret_layer = 0;
        // SAFETY: winemetal's slot 159; `attach_desc`, `poll`, and `ret_layer` are valid.
        let view = unsafe {
            WMTAttachMetalView(
                hwnd,
                mtl.handle(),
                &raw const attach_desc,
                &raw mut poll,
                &raw mut ret_layer,
            )
        };
        if view == 0 || ret_layer == 0 {
            if ret_layer != 0 {
                // SAFETY: `ret_layer` is +1 if non-zero.
                drop(unsafe { Obj::adopt(ret_layer) });
            }
            if view != 0 {
                // SAFETY: `view` was just attached and is retired due to missing layer.
                unsafe { WMTRetireMetalView(view, 0) };
            }
            log::error!(target: LOG_TARGET, "d3d9 core: no Metal view for window {hwnd:#x}");
            return None;
        }
        // SAFETY: `WMTAttachMetalView` returned a +1 retained `CAMetalLayer`.
        let Some(layer) = (unsafe { Obj::adopt(ret_layer) }) else {
            // SAFETY: `view` was just attached and has no usable layer.
            unsafe { WMTRetireMetalView(view, 0) };
            return None;
        };
        // SAFETY: `mtl` holds a live `MTLDevice`.
        let raw_event = unsafe { MTLDevice_newSharedEvent(mtl.handle()) };
        // SAFETY: `MTLDevice_newSharedEvent` returns a +1 retained `MTLSharedEvent` or null.
        let presented_event = unsafe { Obj::adopt(raw_event) };
        let init_w = if poll.drawable_width > 0 {
            poll.drawable_width
        } else {
            width
        };
        let init_h = if poll.drawable_height > 0 {
            poll.drawable_height
        } else {
            height
        };
        let desc = PresenterDesc {
            device: mtl.handle(),
            layer: layer.handle(),
            library: self.device.cmd_library_handle(),
            scale_factor: 1.0,
            sample_count: 1,
        };
        let hdr_enable = self.options.contains(CoreOptions::HDR_ENABLE);
        // SAFETY: the device and command library outlive every encoded present; `layer` stays
        // retained on `Window` and drops after `presenter` in `Window::drop`.
        let presenter = unsafe { Presenter::new_d3d9_managed(&desc, init_w, init_h, hdr_enable) };
        if poll.backing_scale != 0 {
            self.backing_scale = poll.backing_scale.clamp(1, 8);
        }
        self.options.set(
            CoreOptions::SOFTWARE_CURSOR_ACTIVE,
            poll.flags & WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE != 0,
        );
        self.options.set(
            CoreOptions::CURSOR_KICK_PENDING,
            poll.flags & WMT_VIEW_POLL_FLAG_CURSOR_KICK != 0,
        );
        self.overlay.set_view(view);
        if poll.flags & WMT_VIEW_POLL_FLAG_CLIENT_SURFACE_DIRTY != 0 {
            // SAFETY: `view` was just attached and is live.
            unsafe { WMTPresentClientSurface(view, WMT_CLIENT_SURFACE_REASON_VIEW_HIDDEN) };
        }
        Some(Window {
            view,
            layer: Some(layer),
            presenter: Some(presenter),
            presented_event,
            next_signal: 0,
            park_on_retire: true,
            overlay: Arc::clone(&self.overlay),
        })
    }
}

/// Next CPU-paced present deadline, keeping cadence without bursting after stalls.
pub(super) fn next_paced_deadline(
    last: Option<Instant>,
    now: Instant,
    duration: Duration,
) -> Instant {
    last.and_then(|t| t.checked_add(duration))
        .map_or(now, |at| at.max(now))
}

/// Computes `(after, cap_after)` in seconds from `pacing` and `refresh_hz`.
///
/// Uses `rate = 60` only for the vsync period when `refresh_hz == 0` and `pacing.display_sync` is
/// enabled, leaving divided-interval capping strictly governed by `pacing.max_fps`.
#[must_use]
pub fn present_pacing(pacing: mtld3d_core::present::LayerPacing, refresh_hz: u32) -> (f64, f64) {
    let rate = if refresh_hz == 0 { 60 } else { refresh_hz };
    let vsync = if pacing.display_sync {
        1.0 / f64::from(rate)
    } else {
        0.0
    };
    let cap_after = if pacing.max_fps > 0 {
        1.0 / f64::from(pacing.max_fps)
    } else {
        0.0
    };
    (vsync.max(cap_after), cap_after)
}

/// Computes `(after, cap_after)` in seconds for D3D9 `interval`, `refresh_hz`, and `max_fps`.
#[must_use]
pub fn present_durations(interval: u32, refresh_hz: u32, max_fps: u32) -> (f64, f64) {
    present_pacing(
        mtld3d_core::present::layer_pacing_for(interval, refresh_hz, max_fps),
        refresh_hz,
    )
}

/// The seconds a present at D3D9 presentation `interval` waits after the last.
///
/// Uses `mtld3d_core::present::layer_pacing_for` with raw `refresh_hz` and falls back to `rate =
/// 60` only for the vsync period when `refresh_hz == 0`.
#[must_use]
pub fn present_after(interval: u32, refresh_hz: u32, max_fps: u32) -> f64 {
    present_durations(interval, refresh_hz, max_fps).0
}

#[cfg(test)]
mod tests;
