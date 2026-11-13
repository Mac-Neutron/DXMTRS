//! The colour target a core device draws to (DXMTRS C4b-3d1).
//!
//! The back buffer, or one level (and cube face) of a stored render-target texture. The device
//! keeps the target by reference ([`TargetRef`]) and resolves it at each use: a render pass, a
//! Clear and a read-back all go through [`CoreDevice::resolve`], so the three agree on the view,
//! its size and its format. A change of target ends the open render pass: a pass's attachments
//! are fixed when it begins.

use std::sync::Arc;

use dxmt_core::resource::{Texture, TextureViewKey};
use mtld3d_core::ids::TextureId;
use winemetal_sys::wire::WMT_PIXEL_FORMAT_BGRA8_UNORM;

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

/// A target as a pass attaches it.
pub(super) struct Resolved {
    pub(super) texture: Arc<Texture>,
    /// The view of the target's one subresource.
    pub(super) key: TextureViewKey,
    pub(super) width: u32,
    pub(super) height: u32,
    /// The `WMTPixelFormat` the pipeline and the tile-barrier key name.
    pub(super) pixel_format: u32,
    /// Bytes a texel, for a read-back's rows.
    pub(super) texel_bytes: u32,
}

impl CoreDevice {
    /// `target` as a pass attaches it, when it names something the device can draw to.
    pub(super) fn resolve(&self, target: TargetRef) -> Option<Resolved> {
        match target {
            TargetRef::BackBuffer => self.back_buffer.as_ref().map(|back_buffer| Resolved {
                texture: Arc::clone(&back_buffer.texture),
                key: back_buffer.key,
                width: back_buffer.width,
                height: back_buffer.height,
                pixel_format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
                texel_bytes: 4,
            }),
            TargetRef::Texture { id, level, face } => self.texture_target(id, level, face),
        }
    }

    /// Makes `target` the current colour target.
    ///
    /// False, and nothing changed, when it does not resolve. A change ends the open render pass,
    /// whose attachment is the old target.
    pub fn set_target(&mut self, target: TargetRef) -> bool {
        if self.resolve(target).is_none() {
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
        self.resolve(self.target).map(|t| (t.width, t.height))
    }

    /// The current target's `WMTPixelFormat`; `None` when it does not resolve.
    #[must_use]
    pub fn target_format(&self) -> Option<u32> {
        self.resolve(self.target).map(|t| t.pixel_format)
    }
}

#[cfg(test)]
mod tests;
