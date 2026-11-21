//! The implicit swap chain on the core, and Present (C4-11).
//!
//! The back buffer, the window's layer and the presenter on it. A multisampled back buffer is a
//! multisampled texture beside the single-sample one, which draws and clears go to and Present
//! resolves into the single-sample one; the presenter stays at one sample (DXMTRS C4b-3d5).

use std::sync::Arc;

use dxmt_core::{
    presenter::{D3d9PresentArgs, Presenter, PresenterDesc},
    resource::{Texture, TextureAllocationFlags, TextureViewKey, texture_info},
};
use winemetal_sys::{
    CreateMetalViewFromHWND, ReleaseMetalView,
    wire::{
        WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW, WMT_TEXTURE_USAGE_RENDER_TARGET,
        WMT_TEXTURE_USAGE_SHADER_READ, WMTTextureInfo,
    },
};

use super::{ClearTarget, CoreDevice, CoreOptions, LOG_TARGET, TargetRef, textures::two_d};

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

impl CoreDevice {
    /// Makes the `width` x `height` back buffer, and the window's Metal view and presenter.
    ///
    /// `hwnd` 0 is no window: the device then presents nothing. With `samples` above one the
    /// back buffer has a multisampled companion of that count, zeroed, which draws and clears go
    /// to (DXMTRS C4b-3d5); the single-sample texture is made first either way.
    pub fn attach(&mut self, hwnd: isize, width: u32, height: u32, samples: u8) {
        self.drawable_size = (width, height);
        self.make_back_buffer(width, height, samples);
        self.window = self.open_window(hwnd, width, height);
    }

    /// Whether the device currently holds a live back buffer.
    #[must_use]
    pub const fn has_back_buffer(&self) -> bool {
        self.back_buffer.is_some()
    }

    /// Makes the back buffer again, `width` x `height` of `samples`, in place: a `Reset`'s.
    ///
    /// The work recorded so far is drained first, so no recorded closure is left to name the old
    /// back buffer, which then drops with its companion. The new one is made as
    /// [`CoreDevice::attach`] makes it, at any size and count, the old one's included, and
    /// [`TargetRef::BackBuffer`], the presented texture and the companion name it from here on.
    /// The window's Metal view and presenter are kept, the presenter's layer taking the new size
    /// while preserving its current HDR or SDR mode once the new back buffer succeeds; a device
    /// without a window makes the back buffer alone. Returns `false` without updating the layer
    /// when `width == 0`, `height == 0`, or Metal refuses an allocation.
    #[must_use]
    pub fn resize(&mut self, width: u32, height: u32, samples: u8) -> bool {
        self.drain();
        self.clear_history.forget(ClearTarget::BackBuffer);
        self.make_back_buffer(width, height, samples);
        if self.back_buffer.is_none() {
            return false;
        }
        self.drawable_size = (width, height);
        let presenter = self.window.as_ref().and_then(|w| w.presenter.as_deref());
        if let Some(presenter) = presenter {
            presenter.set_d3d9_drawable_size(width, height);
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

    /// Presents the back buffer `after` seconds after the last present (0: at once).
    ///
    /// D3D11's order: the present closure and the frame-latency signal in the chunk, the commit,
    /// then the frame's bound, which waits for the frame `max_latency` back. A hidden window
    /// (`visible` false), or a device without one, presents nothing: its work is committed, and
    /// no signal or bound is counted that nothing would signal. A multisampled back buffer's
    /// companion is resolved into the presented texture first, every frame, drawn or not,
    /// through the view its last pass or Clear wrote it in (DXMTRS C4b-3d5): folded into the
    /// frame's last pass when that pass drew it, a resolve pass of its own otherwise. The inline
    /// gamma LUT and current EDR headroom are snapshotted per frame so queued presents never
    /// stall or race with subsequent ramp changes.
    pub fn present(&mut self, visible: bool, after: f64, display_sync: bool) {
        let presenter = self.window.as_ref().and_then(|w| w.presenter.clone());
        let (Some(presenter), Some(back_buffer), true) = (presenter, &self.back_buffer, visible)
        else {
            if self.dirty || self.render_open {
                self.commit();
            }
            self.end_frame_clear_tracking();
            return;
        };
        let (dw, dh) = if self.drawable_size.0 > 0 && self.drawable_size.1 > 0 {
            self.drawable_size
        } else {
            (back_buffer.width, back_buffer.height)
        };
        let peak_edr = presenter.prepare_d3d9_present(dw, dh, display_sync);
        let args = D3d9PresentArgs {
            gamma: self.gamma.clone(),
            peak_edr,
        };
        let texture = Arc::clone(back_buffer.presented().0);
        self.resolve_presented();
        let queue = self.device.queue_mut();
        let frame = queue.current_frame_seq();
        queue.chunk().signal_frame_latency(frame);
        self.emit(move |enc| {
            enc.present_d3d9(&texture, Arc::clone(&presenter), after, args.clone());
        });
        self.commit();
        self.device.queue().present_boundary(true);
        self.end_frame_clear_tracking();
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

/// The seconds a present at D3D9 presentation `interval` waits after the last.
///
/// Uses `mtld3d_core::present::layer_pacing_for` with `rate = 60` when `refresh_hz == 0`, taking
/// the stricter of the vsync period and the frame-rate cap.
#[must_use]
pub fn present_after(interval: u32, refresh_hz: u32, max_fps: u32) -> f64 {
    let rate = if refresh_hz == 0 { 60 } else { refresh_hz };
    let pacing = mtld3d_core::present::layer_pacing_for(interval, rate, max_fps);
    let vsync = if pacing.display_sync {
        1.0 / f64::from(rate)
    } else {
        0.0
    };
    let cap = if pacing.max_fps > 0 {
        1.0 / f64::from(pacing.max_fps)
    } else {
        0.0
    };
    vsync.max(cap)
}

#[cfg(test)]
mod tests;
