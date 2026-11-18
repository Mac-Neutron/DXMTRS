//! The implicit swap chain on the core, and Present (C4-11).
//!
//! The back buffer, the window's layer and the presenter on it. A multisampled back buffer is a
//! multisampled texture beside the single-sample one, which draws and clears go to and Present
//! resolves into the single-sample one; the presenter stays at one sample (DXMTRS C4b-3d5).

use std::sync::Arc;

use dxmt_core::{
    presenter::{Presenter, PresenterDesc},
    resource::{Texture, TextureAllocationFlags, TextureViewKey, texture_info},
    swapchain::vsync_duration,
};
use winemetal_sys::{
    CreateMetalViewFromHWND, ReleaseMetalView,
    wire::{
        WMT_COLOR_SPACE_SRGB, WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW, WMT_TEXTURE_USAGE_RENDER_TARGET,
        WMT_TEXTURE_USAGE_SHADER_READ, WMTTextureInfo,
    },
};

use super::{CoreDevice, LOG_TARGET, TargetRef, textures::two_d};

/// `D3DPRESENT_INTERVAL_IMMEDIATE`.
const INTERVAL_IMMEDIATE: u32 = 0x8000_0000;

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
        texture.rename(texture.allocate(TextureAllocationFlags::GPU_PRIVATE));
        let key = texture.full_view();
        // Zeroed as a render-target texture is, ahead of the next chunk.
        let companion = (samples > 1).then(|| {
            let texture = Arc::new(Texture::new(info(samples), mtl));
            let allocation = texture.allocate(TextureAllocationFlags::GPU_PRIVATE);
            texture.rename(Arc::clone(&allocation));
            let initializer = self.device.queue().initializer();
            let _ = initializer.init_texture_zero(&texture, &allocation, 0, 0);
            Companion {
                key: texture.full_view(),
                texture,
                samples,
                last_written: WMT_PIXEL_FORMAT_BGRA8_UNORM,
            }
        });
        self.dirty |= companion.is_some();
        self.back_buffer = Some(BackBuffer {
            texture,
            key,
            width,
            height,
            companion,
        });
        if hwnd == 0 {
            return;
        }
        let mut layer = 0;
        // SAFETY: winemetal's call; `layer` is writable for it.
        let view = unsafe { CreateMetalViewFromHWND(hwnd, mtl.handle(), &raw mut layer) };
        if view == 0 || layer == 0 {
            log::error!(target: LOG_TARGET, "d3d9 core: no Metal view for window {hwnd:#x}");
            return;
        }
        let desc = PresenterDesc {
            device: mtl.handle(),
            layer,
            library: self.device.cmd_library_handle(),
            scale_factor: 1.0,
            sample_count: 1,
        };
        // SAFETY: the device and the command library outlive every present the presenter
        // encodes (the queue that encodes them drops first, with the device); the layer is held
        // by `view`, released in the window's drop after the presenter.
        let presenter = unsafe { Presenter::new(&desc) };
        presenter.change_layer_properties(
            WMT_PIXEL_FORMAT_BGRA8_UNORM,
            WMT_COLOR_SPACE_SRGB,
            f64::from(width),
            f64::from(height),
            1,
        );
        self.window = Some(Window {
            view,
            presenter: Some(Arc::new(presenter)),
        });
    }

    /// Presents the back buffer `after` seconds after the last present (0: at once).
    ///
    /// D3D11's order: the present closure and the frame-latency signal in the chunk, the commit,
    /// then the frame's bound, which waits for the frame `max_latency` back. A hidden window
    /// (`visible` false), or a device without one, presents nothing: its work is committed, and
    /// no signal or bound is counted that nothing would signal. A multisampled back buffer's
    /// companion is resolved into the presented texture first, every frame, drawn or not,
    /// through the view its last pass or Clear wrote it in (DXMTRS C4b-3d5): folded into the
    /// frame's last pass when that pass drew it, a resolve pass of its own otherwise.
    pub fn present(&mut self, visible: bool, after: f64) {
        let presenter = self.window.as_ref().and_then(|w| w.presenter.clone());
        let (Some(presenter), Some(back_buffer), true) = (presenter, &self.back_buffer, visible)
        else {
            if self.dirty || self.render_open {
                self.commit();
            }
            return;
        };
        let texture = Arc::clone(back_buffer.presented().0);
        self.resolve_presented();
        let (frame_id, metadata) = presenter.synchronize();
        let queue = self.device.queue_mut();
        let frame = queue.current_frame_seq();
        queue.chunk().signal_frame_latency(frame);
        self.emit(move |enc| {
            enc.present(&texture, Arc::clone(&presenter), after, metadata);
            presenter.frame_done(frame_id);
        });
        self.commit();
        self.device.queue().present_boundary(true);
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
}

/// The seconds a present at D3D9 presentation `interval` waits after the last.
///
/// At `refresh_hz` (60 when unknown): `IMMEDIATE` none, `DEFAULT` and `ONE` one refresh, `TWO`
/// to `FOUR` that many.
#[must_use]
pub fn present_after(interval: u32, refresh_hz: u32) -> f64 {
    let refreshes = match interval {
        INTERVAL_IMMEDIATE => return 0.0,
        2 => 2,
        4 => 3,
        8 => 4,
        _ => 1,
    };
    let rate = if refresh_hz == 0 { 60 } else { refresh_hz };
    vsync_duration(refreshes, 0, f64::from(rate))
}

#[cfg(test)]
mod tests;
