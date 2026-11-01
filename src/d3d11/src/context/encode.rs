//! Pass state machine (`CommandBufferState`, `InvalidateCurrentPass`, `SwitchTo*Encoder`).

use super::{Context, Mode};

/// Active encoder pass state of a context (`CommandBufferState`).
#[derive(Default, PartialEq, Eq)]
pub enum CommandBufferState {
    #[default]
    Idle,
    RenderPipelineReady,
    ComputePipelineReady,
    RenderEncoderActive,
    ComputeEncoderActive,
    BlitEncoderActive,
}

impl<M: Mode> Context<M> {
    /// Ends the active pass and optionally commits the chunk (`InvalidateCurrentPass`).
    pub const fn invalidate_current_pass(&mut self, promote_flush: bool) {
        let _ = promote_flush;
        self.cmdbuf_state = CommandBufferState::Idle;
    }

    /// Transitions the context into an active render pass (`SwitchToRenderEncoder`).
    pub const fn switch_to_render_encoder(&mut self) {
        self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
    }

    /// Transitions the context into an active compute pass (`SwitchToComputeEncoder`).
    pub const fn switch_to_compute_encoder(&mut self) {
        self.cmdbuf_state = CommandBufferState::ComputeEncoderActive;
    }

    /// Transitions the context into an active blit pass (`SwitchToBlitEncoder`).
    pub const fn switch_to_blit_encoder(&mut self) {
        self.cmdbuf_state = CommandBufferState::BlitEncoderActive;
    }
}
