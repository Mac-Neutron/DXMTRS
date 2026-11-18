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
    resource::{Texture, TextureViewKey},
};
use mtld3d_core::convert::linear_to_srgb_rgba;
use winemetal_sys::wire::{
    WMT_LOAD_ACTION_LOAD, WMT_STORE_ACTION_STORE, WMTClearColor, WMTScissorRect,
};

use super::{CoreDevice, TargetRef, depth, target::Resolved};

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
    /// For the device's tests: nothing checks it against the pass set, as a draw's pass is
    /// ([`CoreDevice::draw`]). Nothing when a render pass is open. An open blit pass ends
    /// first: the core's `begin` holds one pass at a time.
    #[cfg(test)]
    pub(super) fn open_render_pass(&mut self) {
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
        // Each attached slot's view in its format; a hole and a slot outside the set have none.
        let views: [_; 4] = core::array::from_fn(|slot| {
            let format = colors[slot];
            let target = self
                .slot_target(slot)
                .filter(|_| format != 0 && set[slot].is_some())?;
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
        // Every attachment has target 0's sample count (the set's rule), which the barrier takes.
        let samples = u32::from(first.samples);
        // The view each attached target is written through, which a resolve of it reads.
        for (slot, view) in views.iter().enumerate() {
            if view.is_some()
                && let Some(target) = self.slot_target(slot)
            {
                self.note_written(target, colors[slot]);
            }
        }
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
            targets.tile_barrier_pso_key[8] = samples;
            targets.render_target_width = width;
            targets.render_target_height = height;
        });
        self.render_open = true;
        self.open_colors = colors;
        self.open_depth = self.depth.filter(|_| attached);
    }

    /// `slot`'s target as a Clear under `srgb_write` clears it, and the colour stored for `linear`.
    ///
    /// A member of the pass set is cleared through the view the set attaches it in, so the Clear
    /// folds into the set's pass; a bound target outside the set through its own twin when it has
    /// one, as mtld3d's pass that binds it alone. Through an sRGB view the colour is stored linear
    /// and Metal encodes it; under `srgb_write` through any other view it is stored encoded, as
    /// mtld3d's Clear stores it: each target is encoded exactly once (DXMTRS C4b-3d4).
    fn clear_target(
        &self,
        slot: usize,
        linear: [f32; 4],
        srgb_write: bool,
    ) -> Option<(Resolved, [f32; 4])> {
        let target = self.slot_target(slot)?;
        let shape = self.shape(target)?;
        let format = self
            .target_desc(srgb_write)
            .and_then(|(_, formats)| formats.get(slot).copied())
            .filter(|&format| format != 0)
            .unwrap_or_else(|| shape.attached_format(srgb_write));
        let rgba = if srgb_write && format == shape.pixel_format {
            linear_to_srgb_rgba(linear)
        } else {
            linear
        };
        Some((self.resolve_as(target, format)?, rgba))
    }

    /// Clears the whole colour target bound at `slot` (0-3) to `linear`, in the pass set or not.
    ///
    /// A core Clear pass of that one view, which the optimizer folds into the next render pass
    /// that attaches the view, as its load action at that slot, and which runs as a pass of its
    /// own when none follows. Under `srgb_write` the view and the stored colour are chosen so the
    /// colour is encoded exactly once: a member of the pass set through the view the set attaches
    /// it in, a target outside the set through its own sRGB twin when it has one; through an
    /// sRGB view `linear` is stored as given and Metal encodes, through any other it is stored
    /// encoded. Nothing for an empty slot.
    pub fn clear(&mut self, slot: usize, linear: [f32; 4], srgb_write: bool) {
        let Some((target, rgba)) = self.clear_target(slot, linear, srgb_write) else {
            return;
        };
        let (texture, key) = (target.texture, target.key);
        self.note_cleared(slot, key.format());
        self.close_pass();
        let color = WMTClearColor {
            r: f64::from(rgba[0]),
            g: f64::from(rgba[1]),
            b: f64::from(rgba[2]),
            a: f64::from(rgba[3]),
        };
        self.emit_clear(texture, key, color);
    }

    /// Clears `rects` (`x, y, width, height`, each inside it) of the target bound at `slot`.
    ///
    /// Nothing for no rect or an empty slot. The core's quad clear (`ClearRtv`, D3D11's
    /// `ClearView`): a render pass of its own on that one view, a scissored triangle a rect,
    /// which takes nothing of the draws' state (blending, the write mask, the scissor). The view
    /// and the stored colour of `linear` under `srgb_write` are [`CoreDevice::clear`]'s.
    pub fn clear_rects(
        &mut self,
        slot: usize,
        rects: &[[u32; 4]],
        linear: [f32; 4],
        srgb_write: bool,
    ) {
        if rects.is_empty() {
            return;
        }
        let Some((target, rgba)) = self.clear_target(slot, linear, srgb_write) else {
            return;
        };
        let (texture, key) = (target.texture, target.key);
        self.note_cleared(slot, key.format());
        let rects = scissor_rects(rects);
        self.close_pass();
        self.emit_quad_clear(texture, key, rects, rgba);
    }

    /// Fills `target`'s rect `[x, y, width, height]`, or the whole of it, with `rgba` as given.
    ///
    /// `ColorFill` on any colour target the device resolves, bound or not, multisampled or not,
    /// through the view in its stored format, never its sRGB twin: `rgba` is stored as given, as
    /// a Clear under no sRGB write stores it. The rect is in the texels of the target's level (a
    /// cube's face being one). The open pass ends; the whole view is then [`CoreDevice::clear`]'s
    /// Clear pass of that view, a rect [`CoreDevice::clear_rects`]'s quad clear. The draws' state
    /// is untouched: the next draw opens its pass anew, loading every slot. False, and nothing
    /// recorded, when `target` does not resolve, or the rect is empty or leaves the target.
    #[must_use]
    pub fn fill(&mut self, target: TargetRef, rect: Option<[u32; 4]>, rgba: [f32; 4]) -> bool {
        let Some(Resolved {
            texture,
            key,
            shape,
        }) = self.resolve(target)
        else {
            return false;
        };
        let inside = |start: u32, extent: u32, edge: u32| {
            start.checked_add(extent).is_some_and(|end| end <= edge)
        };
        if rect.is_some_and(|[x, y, width, height]| {
            width == 0
                || height == 0
                || !inside(x, width, shape.width)
                || !inside(y, height, shape.height)
        }) {
            return false;
        }
        self.note_written(target, key.format());
        self.close_pass();
        if let Some(rect) = rect {
            self.emit_quad_clear(texture, key, scissor_rects(&[rect]), rgba);
        } else {
            let [r, g, b, a] = rgba.map(f64::from);
            self.emit_clear(texture, key, WMTClearColor { r, g, b, a });
        }
        true
    }

    /// Records that a Clear writes the target bound at `slot` through its view in `format`.
    fn note_cleared(&mut self, slot: usize, format: u32) {
        if let Some(target) = self.slot_target(slot) {
            self.note_written(target, format);
        }
    }

    /// Records a core Clear pass of the whole view `key` of `texture` to `color`.
    fn emit_clear(&mut self, texture: Arc<Texture>, key: TextureViewKey, color: WMTClearColor) {
        // Array length 0: a target's view is no array, as a render pass on it says (and D3D11's
        // views of a plain 2D texture), which lets the optimizer fold the clear into that pass.
        self.emit(move |enc| enc.clear_color(&texture, key, 0, color));
    }

    /// Records the core's quad clear of `rects` of the view `key` of `texture` to `rgba`.
    ///
    /// A render pass of its own on that one view, a scissored triangle a rect.
    fn emit_quad_clear(
        &mut self,
        texture: Arc<Texture>,
        key: TextureViewKey,
        rects: Vec<WMTScissorRect>,
        rgba: [f32; 4],
    ) {
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
