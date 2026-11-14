//! The colour target a core device draws to (DXMTRS C4b-3d1).
//!
//! The back buffer, or one level (and cube face) of a stored render-target texture. The device
//! keeps the target by reference ([`TargetRef`]) and resolves it at each use: a render pass, a
//! Clear and a read-back all go through [`CoreDevice::resolve`], so the three agree on the view,
//! its size and its format. A change of target ends the open render pass: a pass's attachments
//! are fixed when it begins. A target with an sRGB twin is attached through the twin's view by a
//! pass or Clear made under sRGB write, so the hardware encodes after the blender (DXMTRS
//! C4b-3d3); a read-back and Present keep the stored format's view.

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

    /// The current target's size and the `WMTPixelFormat` a draw attaches it in, in one look-up.
    ///
    /// Under `srgb_write` (`D3DRS_SRGBWRITEENABLE`) a target with an sRGB twin is attached through
    /// the twin, whose format this then is: the draw's key names it.
    #[must_use]
    pub fn target_desc(&self, srgb_write: bool) -> Option<((u32, u32), u32)> {
        self.shape(self.target)
            .map(|t| ((t.width, t.height), t.attached_format(srgb_write)))
    }
}

#[cfg(test)]
mod tests;
