//! The implicit swap chain on the core, and Present (C4-11).
//!
//! The back buffer, the window's layer and the presenter on it.

use std::sync::Arc;

use dxmt_core::{
    presenter::{Presenter, PresenterDesc},
    resource::{Texture, TextureAllocationFlags, TextureViewKey},
    swapchain::vsync_duration,
};
use winemetal_sys::{
    CreateMetalViewFromHWND, ReleaseMetalView,
    wire::{
        WMT_COLOR_SPACE_SRGB, WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_RENDER_TARGET, WMT_TEXTURE_USAGE_SHADER_READ,
        WMTTextureInfo,
    },
};

use super::{CoreDevice, LOG_TARGET, back_buffer_view};

/// `D3DPRESENT_INTERVAL_IMMEDIATE`.
const INTERVAL_IMMEDIATE: u32 = 0x8000_0000;

/// The back buffer: a BGRA8 texture of the swap chain's size, which draws and clears target.
pub struct BackBuffer {
    pub texture: Arc<Texture>,
    pub key: TextureViewKey,
    pub width: u32,
    pub height: u32,
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
    /// `hwnd` 0 is no window: the device then presents nothing.
    pub fn attach(&mut self, hwnd: isize, width: u32, height: u32) {
        let mtl = self.device.device();
        let info = WMTTextureInfo {
            pixel_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
            width,
            height,
            depth: 1,
            array_length: 1,
            bits: (WMT_TEXTURE_TYPE2D << WMTTextureInfo::TYPE_SHIFT)
                | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
                | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
                | ((WMT_TEXTURE_USAGE_RENDER_TARGET | WMT_TEXTURE_USAGE_SHADER_READ)
                    << WMTTextureInfo::USAGE_SHIFT),
            options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
            flags: 0,
            mach_port: 0,
            gpu_resource_id: 0,
        };
        let texture = Arc::new(Texture::new(info, mtl));
        texture.rename(texture.allocate(TextureAllocationFlags::GPU_PRIVATE));
        let key = texture.full_view();
        self.back_buffer = Some(BackBuffer {
            texture,
            key,
            width,
            height,
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
        // SAFETY: the device, the layer (held by `view` until the window's drop, after the
        // presenter's) and the command library (the device's) outlive the presenter.
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
    /// no signal or bound is counted that nothing would signal.
    pub fn present(&mut self, visible: bool, after: f64) {
        let presenter = self.window.as_ref().and_then(|w| w.presenter.clone());
        let (Some(presenter), Some(back_buffer), true) = (presenter, &self.back_buffer, visible)
        else {
            if self.dirty || self.render_open {
                self.commit();
            }
            return;
        };
        let (texture, _) = back_buffer_view(back_buffer);
        self.close_pass();
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
