//! The depth-stencil a core device draws against (DXMTRS C4b-3d2).
//!
//! One level of a stored depth texture, kept by reference ([`DepthRef`]) beside the colour
//! target. [`CoreDevice::depth_state`] is the one fact the front end reads: none bound, one a
//! pass on the current colour target can attach, or one it cannot (another size). A draw's
//! pipeline key then names the format it draws with, or 0, and the key alone decides the pass,
//! the pipeline and the depth-stencil state: a draw whose key names no format draws in a pass
//! without the depth-stencil even while one is bound. A change of depth-stencil ends the open
//! render pass, as a change of colour target does.

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

/// What the bound depth-stencil is to a draw on the current colour target.
///
/// The formats are the depth-stencil's `WMTPixelFormat`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthState {
    /// None is bound.
    None,
    /// One of the colour target's size: a pass attaches it.
    Attached(u32),
    /// One no pass on the current colour target attaches: of another size, or with no target.
    Detached(u32),
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

    /// What the bound depth-stencil is to a draw on the current colour target.
    ///
    /// Every draw asks this once (its key's depth format), so it takes no reference.
    #[must_use]
    pub fn depth_state(&self) -> DepthState {
        let Some((width, height, format)) = self
            .depth
            .and_then(|depth| self.depth_level(depth.id, depth.level))
        else {
            return DepthState::None;
        };
        if self.target_size() == Some((width, height)) {
            DepthState::Attached(format)
        } else {
            DepthState::Detached(format)
        }
    }

    /// The current depth-stencil as a pass attaches it, when one on the colour target can.
    pub(super) fn resolve_depth(&self) -> Option<Resolved> {
        let DepthState::Attached(pixel_format) = self.depth_state() else {
            return None;
        };
        let depth = self.depth?;
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

    /// Clears `planes` (`depth 1, stencil 2`) of the whole current depth-stencil.
    ///
    /// Nothing unless a pass on the colour target can attach it, and nothing for planes its
    /// format has not (D3D9's stencil Clear of a depth-only format). `depth` is clamped to
    /// `[0, 1]`, a NaN to 0. A core Clear pass, as [`CoreDevice::clear`]'s.
    pub fn clear_depth_stencil(&mut self, planes: u32, depth: f32, stencil: u8) {
        let Some(resolved) = self.resolve_depth() else {
            return;
        };
        let planes = planes & u32::from(self::planes(resolved.pixel_format));
        if planes == 0 {
            // No plane left: the core would read 0 as a colour clear.
            return;
        }
        let depth = if depth.is_nan() {
            0.0
        } else {
            depth.clamp(0.0, 1.0)
        };
        let (texture, key) = (resolved.texture, resolved.key);
        self.close_pass();
        self.emit(move |enc| enc.clear_depth_stencil(&texture, key, 0, planes, depth, stencil));
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

#[cfg(test)]
mod tests;
