//! The recorder: closures into the core's chunk, and the render pass they leave open.
//!
//! The core's `Encoder::begin` overwrites an open pass, so every helper that begins a pass of
//! its own (a Clear, a Present, a read-back blit) closes the open render pass first
//! ([`CoreDevice::close_pass`]). A draw opens one on the current colour target, loading and
//! storing it, and leaves it open for the next draw (C4-12).

use dxmt_core::{
    deptrack::Access,
    encoder::{Encoder, stage},
};
use winemetal_sys::wire::{WMT_LOAD_ACTION_LOAD, WMT_STORE_ACTION_STORE, WMTClearColor};

use super::CoreDevice;

impl CoreDevice {
    /// Records `f`, which the encode thread runs once at the chunk's flush.
    pub fn emit(&mut self, f: impl Fn(&mut Encoder) + Send + 'static) {
        self.device.queue_mut().chunk().emit(f);
        self.dirty = true;
    }

    /// Ends the render or blit pass the last closure left open, if one is.
    pub fn close_pass(&mut self) {
        if self.render_open || self.blit_open {
            self.render_open = false;
            self.blit_open = false;
            self.emit(Encoder::end_pass);
        }
    }

    /// Opens a blit pass, closing an open render pass, unless one is open: uploads share it.
    pub fn open_blit_pass(&mut self) {
        if self.blit_open {
            return;
        }
        self.close_pass();
        self.emit(Encoder::start_blit_pass);
        self.blit_open = true;
    }

    /// Opens a render pass on the current colour target, loading and storing it.
    ///
    /// Nothing when one is open. An open blit pass ends first: the core's `begin` holds one pass
    /// at a time.
    pub fn open_render_pass(&mut self) {
        if self.render_open {
            return;
        }
        self.close_pass();
        let Some(target) = self.resolve(self.target) else {
            return;
        };
        let (width, height, pixel_format) = (target.width, target.height, target.pixel_format);
        let (texture, key) = (target.texture, target.key);
        self.emit(move |enc| {
            enc.start_render_pass(0, 0, 1, None);
            let attachment =
                enc.attachment::<{ stage::PIXEL }>(&texture, key, Access::READ | Access::WRITE);
            let Some(targets) = enc.render_targets() else {
                return;
            };
            let color = &mut targets.colors[0];
            color.target.attachment = attachment;
            color.target.load_action = WMT_LOAD_ACTION_LOAD;
            color.target.store_action = WMT_STORE_ACTION_STORE;
            targets.tile_barrier_pso_key[0] = pixel_format;
            targets.tile_barrier_pso_key[8] = 1;
            targets.render_target_width = width;
            targets.render_target_height = height;
        });
        self.render_open = true;
    }

    /// Clears the whole current colour target to `rgba`.
    ///
    /// A core Clear pass, which the optimizer folds into the next render pass on the same view as
    /// that pass's load action, and which runs as a pass of its own when none follows.
    pub fn clear(&mut self, rgba: [f32; 4]) {
        let Some(target) = self.resolve(self.target) else {
            return;
        };
        let (texture, key) = (target.texture, target.key);
        self.close_pass();
        let color = WMTClearColor {
            r: f64::from(rgba[0]),
            g: f64::from(rgba[1]),
            b: f64::from(rgba[2]),
            a: f64::from(rgba[3]),
        };
        // Array length 0: a target's view is no array, as a render pass on it says (and D3D11's
        // views of a plain 2D texture), which lets the optimizer fold the clear into that pass.
        self.emit(move |enc| enc.clear_color(&texture, key, 0, color));
    }
}

#[cfg(test)]
mod tests;
