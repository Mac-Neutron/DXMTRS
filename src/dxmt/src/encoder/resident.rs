//! Residency: the resources a pass's argument tables reach, made resident once per pass and use.
//!
//! The C++'s `DXMT_RESOURCE_RESIDENCY` masks and `CheckResourceResidency` (`dxmt_residency.hpp`).

use std::sync::Arc;

use winemetal_sys::wire::{
    WMT_COMPUTE_COMMAND_USE_RESOURCE, WMT_RENDER_COMMAND_USE_RESOURCE, WMT_RENDER_STAGE_FRAGMENT,
    WMT_RENDER_STAGE_MESH, WMT_RENDER_STAGE_OBJECT, WMT_RENDER_STAGE_VERTEX,
    WMT_RESOURCE_USAGE_READ, WMT_RESOURCE_USAGE_WRITE, WmtcmdComputeUseresource,
    WmtcmdRenderUseresource,
};
use winemetal_wire::MemPtr;

use super::{Encoder, buffer_trackers, kind, stage, texture_trackers};
use crate::resource::{Buffer, BufferAllocation, BufferViewKey, TextureAllocation, TextureViewKey};

const VERTEX_READ: u32 = 1;
const VERTEX_WRITE: u32 = 1 << 1;
const FRAGMENT_READ: u32 = 1 << 2;
const FRAGMENT_WRITE: u32 = 1 << 3;
const OBJECT_READ: u32 = 1 << 4;
const OBJECT_WRITE: u32 = 1 << 5;
const MESH_READ: u32 = 1 << 6;
const MESH_WRITE: u32 = 1 << 7;
const MESH_GS_READ: u32 = 1 << 8;
const MESH_GS_WRITE: u32 = 1 << 9;
const OBJECT_GS_READ: u32 = 1 << 10;
const OBJECT_GS_WRITE: u32 = 1 << 11;
const READ: u32 =
    VERTEX_READ | FRAGMENT_READ | OBJECT_READ | MESH_READ | MESH_GS_READ | OBJECT_GS_READ;
const WRITE: u32 =
    VERTEX_WRITE | FRAGMENT_WRITE | OBJECT_WRITE | MESH_WRITE | MESH_GS_WRITE | OBJECT_GS_WRITE;

/// The last pass a resource was made resident for, and the stages and uses it was.
#[derive(Default)]
pub struct Residency {
    encoder: u64,
    mask: u32,
}

impl Residency {
    /// The mask to make the resource resident with for pass `id`, `None` when it is already.
    pub const fn check(&mut self, id: u64, requested: u32) -> Option<u32> {
        if id > self.encoder {
            self.encoder = id;
            self.mask = requested;
            return Some(requested);
        }
        if id == self.encoder && self.mask & requested != requested {
            self.mask |= requested;
            return Some(self.mask);
        }
        None
    }
}

/// What stage `S` of a pipeline of kind `K` reading or writing a resource needs resident.
///
/// A vertex shader runs as an object function in a tessellation or geometry pipeline, a hull
/// shader as one too, and the domain and geometry shaders as mesh functions.
pub const fn mask<const S: u8, const K: u8>(read: bool, write: bool) -> u32 {
    let (r, w) = match S {
        stage::VERTEX if K == kind::TESSELLATION => (OBJECT_READ, OBJECT_WRITE),
        stage::VERTEX if K == kind::GEOMETRY => (OBJECT_GS_READ, OBJECT_GS_WRITE),
        stage::VERTEX => (VERTEX_READ, VERTEX_WRITE),
        stage::PIXEL => (FRAGMENT_READ, FRAGMENT_WRITE),
        stage::HULL => (OBJECT_READ, OBJECT_WRITE),
        stage::GEOMETRY => (MESH_GS_READ, MESH_GS_WRITE),
        stage::DOMAIN => (MESH_READ, MESH_WRITE),
        _ => (READ, WRITE),
    };
    (if read { r } else { 0 }) | (if write { w } else { 0 })
}

const fn usage(mask: u32) -> u32 {
    (if mask & READ != 0 {
        WMT_RESOURCE_USAGE_READ
    } else {
        0
    }) | (if mask & WRITE != 0 {
        WMT_RESOURCE_USAGE_WRITE
    } else {
        0
    })
}

const fn stages(mask: u32) -> u8 {
    let mut stages = 0;
    if mask & (FRAGMENT_READ | FRAGMENT_WRITE) != 0 {
        stages |= WMT_RENDER_STAGE_FRAGMENT;
    }
    if mask & (VERTEX_READ | VERTEX_WRITE) != 0 {
        stages |= WMT_RENDER_STAGE_VERTEX;
    }
    if mask & (OBJECT_READ | OBJECT_WRITE | OBJECT_GS_READ | OBJECT_GS_WRITE) != 0 {
        stages |= WMT_RENDER_STAGE_OBJECT;
    }
    if mask & (MESH_READ | MESH_WRITE | MESH_GS_READ | MESH_GS_WRITE) != 0 {
        stages |= WMT_RENDER_STAGE_MESH;
    }
    stages
}

impl Encoder {
    /// `resource` used by the pass as `mask` says: a use-resource record, made unconditionally.
    pub(super) fn use_resource<const S: u8>(&mut self, resource: u64, mask: u32) {
        if S == stage::COMPUTE {
            self.encode_compute_command(WmtcmdComputeUseresource {
                r#type: WMT_COMPUTE_COMMAND_USE_RESOURCE,
                reserved: [0; 3],
                next: MemPtr::NULL,
                resource,
                usage: usage(mask),
            });
        } else {
            self.encode_render_command(WmtcmdRenderUseresource {
                r#type: WMT_RENDER_COMMAND_USE_RESOURCE,
                reserved: [0; 3],
                next: MemPtr::NULL,
                resource,
                usage: usage(mask),
                stages: stages(mask),
            });
        }
    }

    /// A buffer allocation made resident for the pass, unless it already is for this use.
    pub(super) fn make_allocation_resident<const S: u8, const K: u8>(
        &mut self,
        allocation: &Arc<BufferAllocation>,
        read: bool,
        write: bool,
    ) {
        let Some(id) = self.pass.as_ref().map(|p| p.id) else {
            return;
        };
        let used = self
            .buffers_used
            .entry(allocation, buffer_trackers(allocation));
        if let Some(mask) = used.residency.check(id, mask::<S, K>(read, write)) {
            self.use_resource::<S>(allocation.buffer(), mask);
        }
    }

    /// A buffer's texel view made resident for the pass, unless it already is for this use.
    pub(super) fn make_buffer_view_resident<const S: u8, const K: u8>(
        &mut self,
        buffer: &Buffer,
        allocation: &Arc<BufferAllocation>,
        key: BufferViewKey,
        read: bool,
        write: bool,
    ) {
        let Some(id) = self.pass.as_ref().map(|p| p.id) else {
            return;
        };
        let used = self
            .buffers_used
            .entry(allocation, buffer_trackers(allocation));
        if let Some(mask) =
            view_residency(&mut used.views, key).check(id, mask::<S, K>(read, write))
        {
            let view = buffer.view(allocation, key);
            self.use_resource::<S>(view.texture, mask);
        }
    }

    /// A texture view (whose Metal texture is `texture`) made resident for the pass.
    pub(super) fn make_texture_view_resident<const S: u8, const K: u8>(
        &mut self,
        allocation: &Arc<TextureAllocation>,
        key: TextureViewKey,
        texture: u64,
        read: bool,
        write: bool,
    ) {
        let Some(id) = self.pass.as_ref().map(|p| p.id) else {
            return;
        };
        let used = self
            .textures_used
            .entry(allocation, texture_trackers(allocation));
        if let Some(mask) =
            view_residency(&mut used.views, key).check(id, mask::<S, K>(read, write))
        {
            self.use_resource::<S>(texture, mask);
        }
    }
}

/// View `key`'s residency, made the first time.
fn view_residency<K: PartialEq + Copy>(views: &mut Vec<(K, Residency)>, key: K) -> &mut Residency {
    let at = views
        .iter()
        .position(|(k, _)| *k == key)
        .unwrap_or_else(|| {
            views.push((key, Residency::default()));
            views.len() - 1
        });
    &mut views[at].1
}
