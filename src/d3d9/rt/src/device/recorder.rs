//! The recorder: closures into the core's chunk, and the render pass they leave open.
//!
//! The core's `Encoder::begin` overwrites an open pass, so every helper that begins a pass of
//! its own (a Clear, a Present, a read-back blit) closes the open render pass first
//! ([`CoreDevice::close_pass`]). A draw opens one on the colour targets its key names (target 0
//! and extras of the pass set, DXMTRS C4b-3d4), with the depth-stencil when its key names one,
//! loading and storing them, and leaves it open for the next draw of that kind (C4-12). The pass
//! draws to the smaller of target 0 and the depth-stencil (DXMTRS C4b-3d3).

use std::sync::Arc;

use dxmt_core::{
    deptrack::Access,
    encoder::{Encoder, stage},
};
use winemetal_sys::wire::{
    WMT_LOAD_ACTION_LOAD, WMT_STORE_ACTION_STORE, WMTClearColor, WMTScissorRect,
};

use super::{CoreDevice, depth};

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
            self.open_depth = None;
            self.open_colors = [0; 4];
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

    /// Opens a render pass on the current colour target alone, loading and storing it.
    ///
    /// Nothing when a render pass is open. An open blit pass ends first: the core's `begin` holds
    /// one pass at a time.
    pub fn open_render_pass(&mut self) {
        if !self.render_open {
            let format = self.target_format().unwrap_or_default();
            self.open_pass(false, [format, 0, 0, 0]);
        }
    }

    /// Opens the render pass of a draw: on `colors`' slots, with the depth-stencil or without.
    ///
    /// Nothing when a pass of that kind on those colours is open; any other ends first.
    /// `with_depth` asks for the bound depth-stencil (the caller checked
    /// [`CoreDevice::depth_format`] before opening); its planes load and store, and `end_pass`
    /// records their write. `colors[slot]` is the `WMTPixelFormat` slot `slot` attaches in (its
    /// target's own or its sRGB twin's), 0 for an empty slot: the caller checked them against
    /// [`CoreDevice::target_desc`], and a slot outside the pass set is never attached. A new pass
    /// has recorded no scissor: this is the one place that forgets the last one.
    pub(super) fn open_pass(&mut self, with_depth: bool, colors: [u32; 4]) {
        if self.render_open && self.open_depth.is_some() == with_depth && self.open_colors == colors
        {
            return;
        }
        self.close_pass();
        self.pass_scissor = None;
        let set = self.pass_set();
        debug_assert!(
            colors.iter().zip(&set).all(|(&format, shape)| {
                format == 0
                    || shape.is_some_and(|s| format == s.pixel_format || format == s.srgb_format)
            }),
            "a pass attaches its set's slots, each in its format or its twin's"
        );
        let [e1, e2, e3] = self.extras;
        let slots = [Some(self.target), e1, e2, e3];
        // Each attached slot's view in its format; a hole and a slot outside the set have none.
        let views: [_; 4] = core::array::from_fn(|slot| {
            let format = colors[slot];
            let target = slots[slot].filter(|_| format != 0 && set[slot].is_some())?;
            self.resolve_as(target, format)
                .map(|resolved| (resolved.texture, resolved.key))
        });
        let Some(first) = set[0].filter(|_| views[0].is_some()) else {
            return;
        };
        // The optimizer compares and folds the first `count` slots: up to the highest attached.
        let count = colors
            .iter()
            .zip(1_u8..)
            .filter(|(format, _)| **format != 0)
            .map(|(_, count)| count)
            .last()
            .unwrap_or(0);
        let depth = if with_depth {
            self.resolve_depth()
        } else {
            None
        };
        let attached = depth.is_some();
        // Metal draws to the smallest attachment; the area is said, so a Clear folds into this
        // pass only when the pass reaches all of the cleared view. Every set member has target
        // 0's size.
        let (width, height) = self
            .pass_extent(attached)
            .unwrap_or((first.width, first.height));
        self.emit(move |enc| {
            let planes = depth.as_ref().map_or(0, |d| depth::planes(d.pixel_format));
            enc.start_render_pass(planes, 0, count, None);
            let attachments = views.each_ref().map(|view| {
                view.as_ref().and_then(|(texture, key)| {
                    enc.attachment::<{ stage::PIXEL }>(texture, *key, Access::READ | Access::WRITE)
                })
            });
            // One attachment a plane, both of the one view (the core's are not shared).
            let mut plane = |bit: u8| {
                depth.as_ref().filter(|_| planes & bit != 0).and_then(|d| {
                    enc.attachment::<{ stage::PIXEL }>(&d.texture, d.key, Access::READ)
                })
            };
            let (depth_plane, stencil_plane) = (plane(1), plane(2));
            let Some(targets) = enc.render_targets() else {
                return;
            };
            if planes & 1 != 0 {
                targets.depth.target.attachment = depth_plane;
                targets.depth.target.load_action = WMT_LOAD_ACTION_LOAD;
                targets.depth.target.store_action = WMT_STORE_ACTION_STORE;
            }
            if planes & 2 != 0 {
                targets.stencil.target.attachment = stencil_plane;
                targets.stencil.target.load_action = WMT_LOAD_ACTION_LOAD;
                targets.stencil.target.store_action = WMT_STORE_ACTION_STORE;
            }
            // Each slot at its own index, a hole left empty in the pass and in the barrier's key.
            for (slot, attachment) in attachments.into_iter().enumerate() {
                if views[slot].is_none() {
                    continue;
                }
                let color = &mut targets.colors[slot];
                color.target.attachment = attachment;
                color.target.load_action = WMT_LOAD_ACTION_LOAD;
                color.target.store_action = WMT_STORE_ACTION_STORE;
                targets.tile_barrier_pso_key[slot] = colors[slot];
            }
            targets.tile_barrier_pso_key[8] = 1;
            targets.render_target_width = width;
            targets.render_target_height = height;
        });
        self.render_open = true;
        self.open_colors = colors;
        self.open_depth = self.depth.filter(|_| attached);
    }

    /// The current colour target through the view a Clear made under `srgb_write` clears.
    fn clear_target(&self, srgb_write: bool) -> Option<super::target::Resolved> {
        let format = self.shape(self.target)?.attached_format(srgb_write);
        self.resolve_as(self.target, format)
    }

    /// Clears the whole current colour target to `rgba`.
    ///
    /// A core Clear pass, which the optimizer folds into the next render pass on the same view as
    /// that pass's load action, and which runs as a pass of its own when none follows. Under
    /// `srgb_write` a target with an sRGB twin is cleared through it: `rgba` is then linear, and
    /// only a pass on that view takes the Clear as its load action.
    pub fn clear(&mut self, rgba: [f32; 4], srgb_write: bool) {
        let Some(target) = self.clear_target(srgb_write) else {
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

    /// Clears `rects` (`x, y, width, height`, each inside it) of the current colour target.
    ///
    /// Nothing for no rect. The core's quad clear (`ClearRtv`, D3D11's `ClearView`): a render
    /// pass of its own on the target alone, a scissored triangle a rect, which takes nothing of
    /// the draws' state (blending, the write mask, the scissor). `srgb_write` as
    /// [`CoreDevice::clear`]'s.
    pub fn clear_rects(&mut self, rects: &[[u32; 4]], rgba: [f32; 4], srgb_write: bool) {
        if rects.is_empty() {
            return;
        }
        let Some(target) = self.clear_target(srgb_write) else {
            return;
        };
        let (texture, key) = (target.texture, target.key);
        let rects = scissor_rects(rects);
        self.close_pass();
        self.emit(move |enc| {
            enc.with_clear_rtv(|quad, enc| {
                quad.begin(enc, Arc::clone(&texture), key, 0, 0);
                for &rect in &rects {
                    quad.clear_color(enc, rect, 0, rgba);
                }
                quad.end(enc);
            });
        });
    }
}

/// `rects` (`x, y, width, height`) as the quad clear's scissor rects.
pub(super) fn scissor_rects(rects: &[[u32; 4]]) -> Vec<WMTScissorRect> {
    rects
        .iter()
        .map(|&[x, y, width, height]| WMTScissorRect {
            x: u64::from(x),
            y: u64::from(y),
            width: u64::from(width),
            height: u64::from(height),
        })
        .collect()
}

#[cfg(test)]
mod tests;
