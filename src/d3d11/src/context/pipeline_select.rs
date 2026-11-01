//! Pipeline state finalization (`FinalizeGraphicsPipeline`, `FinalizeComputePipeline`).

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    /// Selects and binds the active graphics pipeline (`FinalizeGraphicsPipeline`).
    #[must_use]
    pub const fn finalize_graphics_pipeline(&mut self) -> bool {
        let _ = &self.state;
        false
    }

    /// Selects and binds the active compute pipeline (`FinalizeComputePipeline`).
    #[must_use]
    pub const fn finalize_compute_pipeline(&mut self) -> bool {
        let _ = &self.state;
        false
    }
}
