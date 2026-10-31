//! The residency mask: which encoders have made a resource resident.
//!
//! An encoder makes a resource resident once per set of stages and accesses: a
//! [`ResidencyState`] remembers the last encoder and what it asked for, so a repeated request
//! costs no `useResource`. The C++'s `CheckResourceResidency` and its two mask helpers.
//!
//! Written ahead for the switches of `d3d11.dll` and `d3d12.dll`, which are its first callers.

use winemetal_sys::wire::{
    WMT_RENDER_STAGE_FRAGMENT, WMT_RENDER_STAGE_MESH, WMT_RENDER_STAGE_OBJECT,
    WMT_RENDER_STAGE_VERTEX, WMT_RESOURCE_USAGE_READ, WMT_RESOURCE_USAGE_WRITE,
};

bitflags::bitflags! {
    /// The stages and accesses a resource is resident for: the C++'s `DXMT_RESOURCE_RESIDENCY`.
    #[derive(Clone, Copy, PartialEq, Eq, Default)]
    pub struct ResidencyMask: u32 {
        const VERTEX_READ = 1;
        const VERTEX_WRITE = 1 << 1;
        const FRAGMENT_READ = 1 << 2;
        const FRAGMENT_WRITE = 1 << 3;
        const OBJECT_READ = 1 << 4;
        const OBJECT_WRITE = 1 << 5;
        const MESH_READ = 1 << 6;
        const MESH_WRITE = 1 << 7;
        const MESH_GS_READ = 1 << 8;
        const MESH_GS_WRITE = 1 << 9;
        const OBJECT_GS_READ = 1 << 10;
        const OBJECT_GS_WRITE = 1 << 11;
        const READ = Self::VERTEX_READ.bits() | Self::FRAGMENT_READ.bits()
            | Self::OBJECT_READ.bits() | Self::MESH_READ.bits() | Self::MESH_GS_READ.bits()
            | Self::OBJECT_GS_READ.bits();
        const WRITE = Self::VERTEX_WRITE.bits() | Self::FRAGMENT_WRITE.bits()
            | Self::OBJECT_WRITE.bits() | Self::MESH_WRITE.bits() | Self::MESH_GS_WRITE.bits()
            | Self::OBJECT_GS_WRITE.bits();
    }
}

impl ResidencyMask {
    /// The `WMTResourceUsage` for `useResource`.
    #[must_use]
    pub const fn usage(self) -> u32 {
        let mut usage = 0;
        if self.intersects(Self::READ) {
            usage |= WMT_RESOURCE_USAGE_READ;
        }
        if self.intersects(Self::WRITE) {
            usage |= WMT_RESOURCE_USAGE_WRITE;
        }
        usage
    }

    /// The `WMTRenderStages` for `useResource`.
    #[must_use]
    pub const fn stages(self) -> u8 {
        let stages = [
            (
                Self::FRAGMENT_READ.union(Self::FRAGMENT_WRITE),
                WMT_RENDER_STAGE_FRAGMENT,
            ),
            (
                Self::VERTEX_READ.union(Self::VERTEX_WRITE),
                WMT_RENDER_STAGE_VERTEX,
            ),
            (
                Self::OBJECT_READ
                    .union(Self::OBJECT_WRITE)
                    .union(Self::OBJECT_GS_READ)
                    .union(Self::OBJECT_GS_WRITE),
                WMT_RENDER_STAGE_OBJECT,
            ),
            (
                Self::MESH_READ
                    .union(Self::MESH_WRITE)
                    .union(Self::MESH_GS_READ)
                    .union(Self::MESH_GS_WRITE),
                WMT_RENDER_STAGE_MESH,
            ),
        ];
        let mut out = 0;
        let mut i = 0;
        while i < stages.len() {
            if self.intersects(stages[i].0) {
                out |= stages[i].1;
            }
            i += 1;
        }
        out
    }
}

/// The last encoder that made a resource resident, and for what.
#[derive(Default)]
pub struct ResidencyState {
    encoder: u64,
    mask: ResidencyMask,
}

impl ResidencyState {
    /// What encoder `encoder` makes the resource resident for when it asks for `requested`.
    ///
    /// Everything it asked for so far, or `None` when that is resident already (or `encoder` is
    /// older than the last one).
    pub fn check(&mut self, encoder: u64, requested: ResidencyMask) -> Option<ResidencyMask> {
        if encoder > self.encoder {
            self.encoder = encoder;
            self.mask = requested;
            return Some(requested);
        }
        if encoder == self.encoder && !self.mask.contains(requested) {
            self.mask |= requested;
            return Some(self.mask);
        }
        None
    }
}

#[cfg(test)]
mod tests;
