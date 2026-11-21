//! The implicit swap chain on the core, and Present (C4-11).
//!
//! The back buffer, the window's layer and the presenter on it. A multisampled back buffer is a
//! multisampled texture beside the single-sample one, which draws and clears go to and Present
//! resolves into the single-sample one; the presenter stays at one sample (DXMTRS C4b-3d5).

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use dxmt_core::{
    presenter::{D3d9PresentArgs, Presenter, PresenterDesc},
    resource::{Texture, TextureAllocationFlags, TextureViewKey, texture_info},
    scaler::Scaler,
};
use mtld3d_core::present::{PresentGeometry, geometry_settled};
use winemetal_sys::{
    CreateMetalViewFromHWND, ReleaseMetalView,
    wire::{
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW, WMT_TEXTURE_USAGE_RENDER_TARGET,
        WMT_TEXTURE_USAGE_SHADER_READ, WMT_TEXTURE_USAGE_SHADER_WRITE, WMTFXSpatialScalerInfo,
        WMTTextureInfo,
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

/// The window's Metal view (slot 72) and the presenter on its layer.
pub struct Window {
    view: u64,
    presenter: Option<Arc<Presenter>>,
}

impl Drop for Window {
    fn drop(&mut self) {
        // The presenter uses the layer the view holds: it goes first.
        self.presenter = None;
        // SAFETY: the view `CreateMetalViewFromHWND` made, released once.
        unsafe { ReleaseMetalView(self.view) };
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
        self.drawable_size = (drawable_w, drawable_h);
        self.settled_geometry = None;
        self.upscale = None;
        self.make_back_buffer(render_w, render_h, samples);
        self.window = self.open_window(hwnd, drawable_w, drawable_h);
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
        let presenter = self.window.as_ref().and_then(|w| w.presenter.as_deref());
        if let Some(presenter) = presenter {
            presenter.set_d3d9_drawable_size(drawable_w, drawable_h);
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
            .is_some_and(|u| u.src != src || u.dst != dst);
        if stale {
            self.upscale = None;
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
                return None;
            };
            log::info!(
                target: LOG_TARGET,
                "d3d9 core: MetalFX spatial scaler engaged ({}x{} -> {}x{})",
                src.0,
                src.1,
                dst.0,
                dst.1
            );
            self.upscale = Some(UpscaleResources {
                src,
                dst,
                scaler: Arc::new(scaler),
                upscaled_output,
                gamma_scratch: None,
            });
        }
        let res = self.upscale.as_mut()?;
        let gamma_scratch = if need_gamma_scratch {
            if res.gamma_scratch.is_none() {
                res.gamma_scratch = alloc_private_bgra8(
                    self.device.device(),
                    src.0,
                    src.1,
                    WMT_TEXTURE_USAGE_RENDER_TARGET | WMT_TEXTURE_USAGE_SHADER_READ,
                );
            }
            let scratch = res.gamma_scratch.as_ref().map(Arc::clone)?;
            Some(scratch)
        } else {
            None
        };
        Some(UpscaleStep {
            scaler: Arc::clone(&res.scaler),
            output: Arc::clone(&res.upscaled_output),
            gamma_scratch,
        })
    }

    /// Presents the back buffer `after` seconds after the last present (0: at once).
    ///
    /// D3D11's order: the present closure and the frame-latency signal in the chunk, the commit,
    /// then the frame's bound, which waits for the frame `max_latency` back. A hidden window
    /// (`visible` false), or a device without one, presents nothing: its work is committed, and
    /// no signal or bound is counted that nothing would signal. A multisampled back buffer's
    /// companion is resolved into the presented texture first, every frame, drawn or not,
    /// through the view its last pass or Clear wrote it in (DXMTRS C4b-3d5): folded into the
    /// frame's last pass when that pass drew it, a resolve pass of its own otherwise. When settled
    /// enlargement uses `MetalFX` spatial upscaling and a gamma ramp is active, gamma is applied
    /// into a render-sized scratch texture ahead of scaling and the final present omits gamma.
    pub fn present(&mut self, visible: bool, after: f64, cap_after: f64, display_sync: bool) {
        let presenter = self.window.as_ref().and_then(|w| w.presenter.clone());
        let bb_dims = self.back_buffer.as_ref().map(|bb| (bb.width, bb.height));
        if let (Some(presenter), Some((bb_w, bb_h)), true) = (presenter, bb_dims, visible) {
            let (dw, dh) = if self.drawable_size.0 > 0 && self.drawable_size.1 > 0 {
                self.drawable_size
            } else {
                (bb_w, bb_h)
            };
            let peak_edr = presenter.prepare_d3d9_present(dw, dh, display_sync, || self.drain());
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
                        };
                        self.emit(move |enc| {
                            enc.present_d3d9(&texture, Arc::clone(&presenter), after, args.clone());
                        });
                    }
                    self.commit();
                    self.device.queue().present_boundary(true);
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
        if cap_after > 0.0 && cap_after.is_finite() {
            let dur = Duration::from_secs_f64(cap_after);
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
        self.gamma = lut.map(Arc::new);
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

    /// A Metal view on window `hwnd` and a presenter on its layer, sized `width` x `height`.
    ///
    /// None for `hwnd` 0, and for a window winemetal gives no view or layer (logged).
    fn open_window(&self, hwnd: isize, width: u32, height: u32) -> Option<Window> {
        if hwnd == 0 {
            return None;
        }
        let mtl = self.device.device();
        let mut layer = 0;
        // SAFETY: winemetal's call; `layer` is writable for it.
        let view = unsafe { CreateMetalViewFromHWND(hwnd, mtl.handle(), &raw mut layer) };
        if view == 0 || layer == 0 {
            log::error!(target: LOG_TARGET, "d3d9 core: no Metal view for window {hwnd:#x}");
            return None;
        }
        let desc = PresenterDesc {
            device: mtl.handle(),
            layer,
            library: self.device.cmd_library_handle(),
            scale_factor: 1.0,
            sample_count: 1,
        };
        let hdr_enable = self.options.contains(CoreOptions::HDR_ENABLE);
        // SAFETY: the device and the command library outlive every present the presenter
        // encodes (the queue that encodes them drops first, with the device); the layer is held
        // by `view`, released in the window's drop after the presenter.
        let presenter = unsafe { Presenter::new_d3d9(&desc, width, height, hdr_enable) };
        Some(Window {
            view,
            presenter: Some(presenter),
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
