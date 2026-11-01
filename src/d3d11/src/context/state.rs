//! Context internal state and dirty flags (`d3d11_context_state.hpp`).

use super::{Context, Mode};

bitflags::bitflags! {
    /// Dirty state flags for the context (`DirtyState`).
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    pub struct DirtyFlags: u32 {
        const BLEND_FACTOR_AND_SAMPLE_MASK = 1 << 0;
        const DEPTH_STENCIL_STATE = 1 << 1;
        const RASTERIZER_STATE = 1 << 2;
        const VIEWPORTS = 1 << 3;
        const SCISSOR_RECTS = 1 << 4;
        const INDEX_BUFFER = 1 << 5;
        const VERTEX_BUFFERS = 1 << 6;
    }
}

/// Bound pipeline state for a D3D11 context (`D3D11ContextState`).
#[derive(Default)]
pub struct ContextState {
    pub sample_mask: u32,
    pub stencil_ref: u32,
    pub blend_factor: [f32; 4],
    pub topology: u32,
}

impl<M: Mode> Context<M> {
    /// `ID3D11DeviceContext::ClearState`.
    pub const fn clear_state(&mut self) {
        self.state.sample_mask = 0xffff_ffff;
    }
}
