//! Per-pass bound Metal render-pass state for deduplicating redundant draw commands.

use std::sync::Arc;

use dxmt_core::{
    resource::{Texture, TextureViewKey},
    sampler::Sampler,
};
use mtld3d_core::ids::BufferId;

use super::draw::Stage;

/// A stored vertex buffer binding cached at a Metal vertex slot.
#[derive(PartialEq, Eq)]
struct BoundBuffer {
    id: BufferId,
    generation: u64,
    offset: u64,
}

/// Cached inline bytes at a stage buffer slot, keeping the `Vec` capacity warm across passes.
struct BytesSlot {
    valid: bool,
    data: Vec<u8>,
}

impl BytesSlot {
    const EMPTY: Self = Self {
        valid: false,
        data: Vec::new(),
    };
}

/// A texture pointer and view key cached at a stage texture slot.
#[derive(PartialEq, Eq)]
struct BoundTex {
    texture: usize,
    key: TextureViewKey,
}

/// Bound state of the currently open render pass, reset whenever a pass opens or closes.
pub(super) struct PassBound {
    pub(super) last_draw_gen: u64,
    pub(super) pso: u64,
    pub(super) dsso: Option<(u64, u8)>,
    pub(super) blend_color: Option<[u32; 4]>,
    pub(super) viewport: Option<[u64; 6]>,
    pub(super) scissor: Option<[u32; 4]>,
    pub(super) rasterizer: Option<(u8, u8, u32)>,
    vertex_buffers: [Option<BoundBuffer>; 32],
    vertex_bytes: [BytesSlot; 32],
    pixel_bytes: [BytesSlot; 32],
    vertex_textures: [Option<BoundTex>; 4],
    pixel_textures: [Option<BoundTex>; 16],
    vertex_samplers: [Option<usize>; 4],
    pixel_samplers: [Option<usize>; 16],
}

impl Default for PassBound {
    fn default() -> Self {
        Self::new()
    }
}

impl PassBound {
    /// Creates an empty tracker with no bound state.
    pub(super) const fn new() -> Self {
        Self {
            last_draw_gen: 0,
            pso: 0,
            dsso: None,
            blend_color: None,
            viewport: None,
            scissor: None,
            rasterizer: None,
            vertex_buffers: [const { None }; 32],
            vertex_bytes: [const { BytesSlot::EMPTY }; 32],
            pixel_bytes: [const { BytesSlot::EMPTY }; 32],
            vertex_textures: [const { None }; 4],
            pixel_textures: [const { None }; 16],
            vertex_samplers: [const { None }; 4],
            pixel_samplers: [const { None }; 16],
        }
    }

    /// Clears all tracked pass state while keeping `BytesSlot` capacities warm.
    pub(super) fn reset(&mut self) {
        self.last_draw_gen = 0;
        self.pso = 0;
        self.dsso = None;
        self.blend_color = None;
        self.viewport = None;
        self.scissor = None;
        self.rasterizer = None;
        self.vertex_buffers.fill_with(|| None);
        for slot in &mut self.vertex_bytes {
            slot.valid = false;
        }
        for slot in &mut self.pixel_bytes {
            slot.valid = false;
        }
        self.vertex_textures.fill_with(|| None);
        self.pixel_textures.fill_with(|| None);
        self.vertex_samplers.fill(None);
        self.pixel_samplers.fill(None);
    }

    /// Invalidates any vertex buffer slot currently tracking `id`.
    pub(super) fn forget_buffer(&mut self, id: BufferId) {
        for entry in &mut self.vertex_buffers {
            if entry.as_ref().is_some_and(|bound| bound.id == id) {
                *entry = None;
            }
        }
    }

    /// Records `(id, generation, offset)` at vertex `slot` and returns whether it must be emitted.
    pub(super) fn should_bind_vertex_buffer(
        &mut self,
        slot: u8,
        id: BufferId,
        generation: u64,
        offset: u64,
    ) -> bool {
        let idx = usize::from(slot);
        let next = BoundBuffer {
            id,
            generation,
            offset,
        };
        let Some(entry) = self.vertex_buffers.get_mut(idx) else {
            return true;
        };
        if entry.as_ref() == Some(&next) {
            return false;
        }
        *entry = Some(next);
        if let Some(bytes) = self.vertex_bytes.get_mut(idx) {
            bytes.valid = false;
        }
        true
    }

    /// Updates tracking for a vertex stream byte bind (`Bytes` or `Zeros`) at `slot`.
    pub(super) fn should_bind_stream_bytes(
        &mut self,
        slot: u8,
        bytes: &[u8],
        unconditional: bool,
    ) -> bool {
        if unconditional {
            let idx = usize::from(slot);
            if let Some(entry) = self.vertex_buffers.get_mut(idx) {
                *entry = None;
            }
            if let Some(slot_bytes) = self.vertex_bytes.get_mut(idx) {
                slot_bytes.valid = false;
            }
            return true;
        }
        self.should_bind_bytes(Stage::Vertex, slot, bytes)
    }

    /// Records `bytes` at `stage`'s `slot` and returns whether it must be emitted.
    pub(super) fn should_bind_bytes(&mut self, stage: Stage, slot: u8, bytes: &[u8]) -> bool {
        let idx = usize::from(slot);
        let slots = match stage {
            Stage::Vertex => &mut self.vertex_bytes,
            Stage::Pixel => &mut self.pixel_bytes,
        };
        let Some(entry) = slots.get_mut(idx) else {
            return true;
        };
        if entry.valid && entry.data.as_slice() == bytes {
            return false;
        }
        entry.data.clear();
        entry.data.extend_from_slice(bytes);
        entry.valid = true;
        if matches!(stage, Stage::Vertex)
            && let Some(vb) = self.vertex_buffers.get_mut(idx)
        {
            *vb = None;
        }
        true
    }

    /// Records `(texture, key)` at `stage`'s `slot` and returns whether it must be emitted.
    pub(super) fn should_bind_texture(
        &mut self,
        stage: Stage,
        slot: u8,
        texture: &Arc<Texture>,
        key: TextureViewKey,
    ) -> bool {
        let idx = usize::from(slot);
        let slots: &mut [Option<BoundTex>] = match stage {
            Stage::Vertex => &mut self.vertex_textures,
            Stage::Pixel => &mut self.pixel_textures,
        };
        let Some(entry) = slots.get_mut(idx) else {
            return true;
        };
        let next = BoundTex {
            texture: Arc::as_ptr(texture) as usize,
            key,
        };
        if entry.as_ref() == Some(&next) {
            return false;
        }
        *entry = Some(next);
        true
    }

    /// Records `sampler` at `stage`'s `slot` and returns whether it must be emitted.
    pub(super) fn should_bind_sampler(
        &mut self,
        stage: Stage,
        slot: u8,
        sampler: Option<&Arc<Sampler>>,
    ) -> bool {
        let idx = usize::from(slot);
        let slots: &mut [Option<usize>] = match stage {
            Stage::Vertex => &mut self.vertex_samplers,
            Stage::Pixel => &mut self.pixel_samplers,
        };
        let Some(entry) = slots.get_mut(idx) else {
            return true;
        };
        let next = sampler.map_or(0, |s| Arc::as_ptr(s) as usize);
        if *entry == Some(next) {
            return false;
        }
        *entry = Some(next);
        true
    }
}
