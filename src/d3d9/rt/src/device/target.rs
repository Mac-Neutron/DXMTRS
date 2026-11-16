//! The colour target a core device draws to (DXMTRS C4b-3d1).
//!
//! The back buffer, or one level (and cube face) of a stored render-target texture. The device
//! keeps the target by reference ([`TargetRef`]) and resolves it at each use: a render pass, a
//! Clear and a read-back all go through [`CoreDevice::resolve`], so the three agree on the view,
//! its size and its format. A change of target ends the open render pass: a pass's attachments
//! are fixed when it begins. A target with an sRGB twin is attached through the twin's view by a
//! pass or Clear made under sRGB write, so the hardware encodes after the blender (DXMTRS
//! C4b-3d3); a read-back and Present keep the stored format's view.
//!
//! Three more targets are kept at index 1-3 ([`CoreDevice::set_extra_target`]). A draw's pass
//! attaches the pass set: target 0, and each of them that resolves, has target 0's size and is no
//! target already in the set; under sRGB write every member through its twin when every member
//! has one (DXMTRS C4b-3d4). A change of any of them ends the open render pass too.

use std::sync::Arc;

use dxmt_core::resource::{Texture, TextureViewKey};
use mtld3d_core::ids::TextureId;
use winemetal_sys::wire::{WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB};

use super::CoreDevice;

/// A colour target: the back buffer, or one level (and cube face) of a render-target texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetRef {
    BackBuffer,
    /// `face` is a cube's face (0-5), and `None` exactly for a texture that is not a cube.
    Texture {
        id: TextureId,
        level: u32,
        face: Option<u32>,
    },
}

/// A target's shape: what a draw checks and keys on, without holding the texture.
#[derive(Clone, Copy)]
pub(super) struct Shape {
    pub(super) width: u32,
    pub(super) height: u32,
    /// The `WMTPixelFormat` it is stored in.
    pub(super) pixel_format: u32,
    /// Its sRGB twin's `WMTPixelFormat`, when a pass can attach it through one; 0 otherwise.
    pub(super) srgb_format: u32,
    /// Bytes a texel, for a read-back's rows.
    pub(super) texel_bytes: u32,
    /// The level and slice (a cube's face) of the target's one subresource.
    pub(super) level: u32,
    pub(super) slice: u32,
}

impl Shape {
    /// The format a pass or Clear made under `srgb_write` attaches: the twin's when it has one.
    ///
    /// What the pipeline and the tile-barrier key then name.
    pub(super) const fn attached_format(&self, srgb_write: bool) -> u32 {
        if srgb_write && self.srgb_format != 0 {
            self.srgb_format
        } else {
            self.pixel_format
        }
    }
}

/// A target as a pass attaches it.
pub(super) struct Resolved {
    pub(super) texture: Arc<Texture>,
    /// The view of the target's one subresource.
    pub(super) key: TextureViewKey,
    pub(super) shape: Shape,
}

impl CoreDevice {
    /// `target`'s shape, when it names something the device can draw to.
    ///
    /// Every draw asks this (its key's format, its viewport's bounds), so it takes no reference.
    pub(super) fn shape(&self, target: TargetRef) -> Option<Shape> {
        match target {
            TargetRef::BackBuffer => self.back_buffer.as_ref().map(|back_buffer| Shape {
                width: back_buffer.width,
                height: back_buffer.height,
                pixel_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
                srgb_format: WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB,
                texel_bytes: 4,
                level: 0,
                slice: 0,
            }),
            TargetRef::Texture { id, level, face } => self.texture_shape(id, level, face),
        }
    }

    /// `target` as a pass attaches it through its view in `format`: its own, or its sRGB twin's.
    pub(super) fn resolve_as(&self, target: TargetRef, format: u32) -> Option<Resolved> {
        let mut resolved = self.resolve(target)?;
        resolved.key = resolved.key.with_format(format);
        Some(resolved)
    }

    /// `target` in its stored format, when it names something the device can draw to.
    pub(super) fn resolve(&self, target: TargetRef) -> Option<Resolved> {
        let shape = self.shape(target)?;
        match target {
            TargetRef::BackBuffer => self.back_buffer.as_ref().map(|back_buffer| Resolved {
                texture: Arc::clone(&back_buffer.texture),
                key: back_buffer.key,
                shape,
            }),
            TargetRef::Texture { id, .. } => self.texture_target(id, shape),
        }
    }

    /// Makes `target` the current colour target.
    ///
    /// False, and nothing changed, when it does not resolve. A change ends the open render pass,
    /// whose attachment is the old target.
    pub fn set_target(&mut self, target: TargetRef) -> bool {
        if self.shape(target).is_none() {
            return false;
        }
        if target != self.target {
            if self.render_open {
                self.close_pass();
            }
            self.target = target;
        }
        true
    }

    /// The current colour target.
    #[must_use]
    pub const fn target(&self) -> TargetRef {
        self.target
    }

    /// The current target's width and height; `None` when it does not resolve.
    #[must_use]
    pub fn target_size(&self) -> Option<(u32, u32)> {
        self.shape(self.target).map(|t| (t.width, t.height))
    }

    /// The `WMTPixelFormat` the current target is stored in; `None` when it does not resolve.
    #[must_use]
    pub fn target_format(&self) -> Option<u32> {
        self.shape(self.target).map(|t| t.pixel_format)
    }

    /// The size of the colour target bound at `slot` (0-3), in the pass set or not (C4b-3d4).
    ///
    /// `None` for an empty slot, or one whose target does not resolve.
    #[must_use]
    pub fn color_size(&self, slot: usize) -> Option<(u32, u32)> {
        let shape = self.shape(self.slot_target(slot)?)?;
        Some((shape.width, shape.height))
    }

    /// The target bound at colour slot `slot` (0-3): target 0 or an extra; none for an empty one.
    pub(super) fn slot_target(&self, slot: usize) -> Option<TargetRef> {
        slot.checked_sub(1).map_or(Some(self.target), |index| {
            self.extras.get(index).copied().flatten()
        })
    }

    /// Binds `target` at colour slot `slot` (1-3), or unbinds the slot (DXMTRS C4b-3d4).
    ///
    /// False, and nothing changed, for another slot or a target that does not resolve. A change
    /// ends the open render pass, whose attachments are fixed. A bound target need not join the
    /// pass set: one of another size than target 0 is kept, and skipped by draws.
    #[must_use]
    pub fn set_extra_target(&mut self, slot: usize, target: Option<TargetRef>) -> bool {
        let Some(index) = slot.checked_sub(1).filter(|&i| i < self.extras.len()) else {
            return false;
        };
        if target.is_some_and(|target| self.shape(target).is_none()) {
            return false;
        }
        if self.extras[index] != target {
            if self.render_open {
                self.close_pass();
            }
            self.extras[index] = target;
        }
        true
    }

    /// Target 0's size and the `WMTPixelFormat` each slot of the pass set attaches.
    ///
    /// 0 for a slot not in the set; `None` when target 0 does not resolve. Under `srgb_write`
    /// (`D3DRS_SRGBWRITEENABLE`) every member is attached through its sRGB twin when every member
    /// has one, and in its stored format otherwise, as mtld3d decides it: the draw's key names
    /// these formats.
    #[must_use]
    pub fn target_desc(&self, srgb_write: bool) -> Option<((u32, u32), [u32; 4])> {
        let set = self.pass_set();
        let first = set[0]?;
        let twins = srgb_write && set.iter().flatten().all(|shape| shape.srgb_format != 0);
        let formats = set.map(|shape| shape.map_or(0, |shape| shape.attached_format(twins)));
        Some(((first.width, first.height), formats))
    }

    /// The pass set: the shape of each colour slot a draw's pass attaches, none for the others.
    ///
    /// Target 0, and each extra, in slot order, that resolves, has target 0's width and height
    /// (mtld3d's rule: one of another size is skipped by draws) and is not the `TargetRef` of
    /// target 0 or of a lower slot in the set (Metal is never handed one view twice). All none
    /// when target 0 does not resolve. The one place the set is decided.
    pub(super) fn pass_set(&self) -> [Option<Shape>; 4] {
        let mut set = [None; 4];
        let Some(first) = self.shape(self.target) else {
            return set;
        };
        set[0] = Some(first);
        let mut members = [Some(self.target), None, None, None];
        for (slot, extra) in (1..).zip(self.extras) {
            let Some(target) = extra.filter(|target| !members.contains(&Some(*target))) else {
                continue;
            };
            let Some(shape) = self
                .shape(target)
                .filter(|shape| (shape.width, shape.height) == (first.width, first.height))
            else {
                continue;
            };
            set[slot] = Some(shape);
            members[slot] = Some(target);
        }
        set
    }
}

#[cfg(test)]
mod tests;
