//! The depth-stencil a core device draws against (DXMTRS C4b-3d2).
//!
//! One level of a stored depth texture, kept by reference ([`DepthRef`]) beside the colour
//! target. [`CoreDevice::depth_format`] is the one fact the front end reads: the bound one's
//! format, whatever its size (a pass draws to the smaller of the colour target and it, DXMTRS
//! C4b-3d3). A draw's pipeline key then names the format it draws with, or 0, and the key alone
//! decides the pass, the pipeline and the depth-stencil state: a draw whose key names no format
//! draws in a pass without the depth-stencil even while one is bound. A change of depth-stencil
//! ends the open render pass, as a change of colour target does.

use std::sync::Arc;

use dxmt_core::{
    format::depth_stencil_planar_flags,
    resource::{Texture, TextureViewDescriptor, TextureViewKey},
};
use mtld3d_core::{
    depth_stencil_state::{
        DepthStencilSnapshot, StencilFaceDescription, description_from_snapshot, key_from_snapshot,
    },
    ids::TextureId,
};
use winemetal_sys::wire::{WMT_TEXTURE_TYPE2D, WMTDepthStencilInfo, WMTStencilInfo};

use super::{CoreDevice, LOG_TARGET};

/// A depth-stencil: one level of a stored depth texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DepthRef {
    pub id: TextureId,
    pub level: u32,
}

/// The depth-stencil as a pass attaches it and a Clear clears it.
pub(super) struct Resolved {
    pub(super) texture: Arc<Texture>,
    /// The view of the depth-stencil's one level.
    pub(super) key: TextureViewKey,
    pub(super) pixel_format: u32,
}

/// The planes (`depth 1, stencil 2`) of the depth-stencil `WMTPixelFormat` `format`; 0 for none.
pub(super) const fn planes(format: u32) -> u8 {
    depth_stencil_planar_flags(format).to_le_bytes()[0]
}

impl CoreDevice {
    /// Makes `depth` the current depth-stencil, or unbinds it.
    ///
    /// False, and nothing changed, when it names no level of a stored depth texture. A change
    /// ends the open render pass, whose attachments are fixed.
    pub fn set_depth(&mut self, depth: Option<DepthRef>) -> bool {
        if let Some(depth) = depth
            && self.depth_level(depth.id, depth.level).is_none()
        {
            return false;
        }
        if depth != self.depth {
            if self.render_open {
                self.close_pass();
            }
            self.depth = depth;
        }
        true
    }

    /// The current depth-stencil.
    #[must_use]
    pub const fn depth(&self) -> Option<DepthRef> {
        self.depth
    }

    /// The bound depth-stencil's `WMTPixelFormat`; `None` when none is bound.
    ///
    /// Every draw asks this once (its key's depth format), so it takes no reference.
    #[must_use]
    pub fn depth_format(&self) -> Option<u32> {
        let depth = self.depth?;
        self.depth_level(depth.id, depth.level)
            .map(|(_, _, format)| format)
    }

    /// The current depth-stencil's width and height; `None` when none is bound.
    #[must_use]
    pub fn depth_size(&self) -> Option<(u32, u32)> {
        let depth = self.depth?;
        self.depth_level(depth.id, depth.level)
            .map(|(width, height, _)| (width, height))
    }

    /// The area a pass on the current colour target draws to; `None` when it does not resolve.
    ///
    /// The target's, cut to the bound depth-stencil's when the pass attaches it (`with_depth`):
    /// Metal draws to the smallest attachment.
    pub(super) fn pass_extent(&self, with_depth: bool) -> Option<(u32, u32)> {
        let (width, height) = self.target_size()?;
        Some(match self.depth_size().filter(|_| with_depth) {
            Some((depth_width, depth_height)) => (width.min(depth_width), height.min(depth_height)),
            None => (width, height),
        })
    }

    /// The current depth-stencil as a pass attaches it, whatever its size.
    pub(super) fn resolve_depth(&self) -> Option<Resolved> {
        let depth = self.depth?;
        let (_, _, pixel_format) = self.depth_level(depth.id, depth.level)?;
        let texture = self.stored_texture(depth.id)?;
        let key = TextureViewDescriptor {
            format: pixel_format,
            texture_type: WMT_TEXTURE_TYPE2D,
            first_mip: depth.level,
            mip_count: 1,
            first_slice: 0,
            array_size: 1,
        }
        .key();
        Some(Resolved {
            texture,
            key,
            pixel_format,
        })
    }

    /// The current depth-stencil's view and the planes of `planes` it has, for a clear.
    ///
    /// `None` with nothing to clear: none bound, or no plane left (D3D9's stencil Clear of a
    /// depth-only format), which the core would read as a colour clear.
    fn depth_clear(&self, planes: u32) -> Option<(Arc<Texture>, TextureViewKey, u32)> {
        let resolved = self.resolve_depth()?;
        let planes = planes & u32::from(self::planes(resolved.pixel_format));
        (planes != 0).then_some((resolved.texture, resolved.key, planes))
    }

    /// Clears `planes` (`depth 1, stencil 2`) of the whole current depth-stencil.
    ///
    /// Whatever the colour target's size, and nothing for planes its format has not. `depth` is
    /// clamped to `[0, 1]`, a NaN to 0. A core Clear pass, as [`CoreDevice::clear`]'s: the
    /// optimizer folds it into the next pass that attaches the view and draws to all of it.
    pub fn clear_depth_stencil(&mut self, planes: u32, depth: f32, stencil: u8) {
        let Some((texture, key, planes)) = self.depth_clear(planes) else {
            return;
        };
        let depth = clear_depth(depth);
        self.close_pass();
        self.emit(move |enc| enc.clear_depth_stencil(&texture, key, 0, planes, depth, stencil));
    }

    /// Clears `planes` of `rects` (`x, y, width, height`, each inside it) of the depth-stencil.
    ///
    /// The planes and `depth` as [`CoreDevice::clear_depth_stencil`]'s; nothing for no rect. The
    /// core's quad clear on the depth-stencil alone, as [`CoreDevice::clear_rects`]'s.
    pub fn clear_depth_stencil_rects(
        &mut self,
        rects: &[[u32; 4]],
        planes: u32,
        depth: f32,
        stencil: u8,
    ) {
        if rects.is_empty() {
            return;
        }
        let Some((texture, key, planes)) = self.depth_clear(planes) else {
            return;
        };
        let depth = clear_depth(depth);
        let rects = super::recorder::scissor_rects(rects);
        self.close_pass();
        self.emit(move |enc| {
            enc.with_clear_rtv(|quad, enc| {
                quad.begin(enc, Arc::clone(&texture), key, 0, planes);
                for &rect in &rects {
                    quad.clear_depth_stencil(enc, rect, depth, stencil);
                }
                quad.end(enc);
            });
        });
    }

    /// The handle of the depth-stencil state of `snapshot`, made at its first request.
    ///
    /// The device keeps it for its life: a recorded closure names it by handle. 0 (Metal's
    /// default state) when Metal makes none, which is logged and not kept.
    pub(super) fn depth_state_handle(&mut self, snapshot: &DepthStencilSnapshot) -> u64 {
        let key = key_from_snapshot(snapshot);
        if let Some(state) = self.depth_states.get(&key.raw()) {
            return state.handle();
        }
        let desc = description_from_snapshot(snapshot, key);
        let off = WMTStencilInfo {
            enabled: 0,
            depth_stencil_pass_op: 0,
            stencil_fail_op: 0,
            depth_fail_op: 0,
            stencil_compare_function: 0,
            write_mask: 0,
            read_mask: 0,
        };
        let (front_stencil, back_stencil) = desc.stencil.as_ref().map_or((off, off), |stencil| {
            let face = |face: &StencilFaceDescription| WMTStencilInfo {
                enabled: 1,
                depth_stencil_pass_op: face.pass_op as u8,
                stencil_fail_op: face.stencil_fail_op as u8,
                depth_fail_op: face.depth_fail_op as u8,
                stencil_compare_function: face.compare_func as u8,
                write_mask: stencil.write_mask.to_le_bytes()[0],
                read_mask: stencil.read_mask.to_le_bytes()[0],
            };
            (face(&stencil.front), face(&stencil.back))
        });
        let info = WMTDepthStencilInfo {
            depth_compare_function: desc.depth_compare_func as u8,
            depth_write_enabled: u8::from(desc.depth_write_enable),
            front_stencil,
            back_stencil,
        };
        let Some(state) = dxmt_core::library::depth_stencil_state(self.device.device(), &info)
        else {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: no depth-stencil state for {:#x}; its draws take Metal's default",
                key.raw()
            );
            return 0;
        };
        let handle = state.handle();
        self.depth_states.insert(key.raw(), state);
        handle
    }
}

/// A Clear's depth as Metal takes it: within `[0, 1]`, a NaN as 0.
const fn clear_depth(depth: f32) -> f32 {
    if depth.is_nan() {
        0.0
    } else {
        depth.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests;
