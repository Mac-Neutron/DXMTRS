//! Compute pipeline compilation (`d3d11_pipeline.cpp`).

use crate::shader::{ManagedShader, args::ShaderVariant};

/// Resolved compute pipeline specialization and threadgroup dimensions.
pub struct ComputePipelineSpec {
    pub variant: ShaderVariant,
    pub threadgroup_size: [u32; 3],
}

/// Builds the compute pipeline spec from `shader`.
#[must_use]
pub fn build_compute_pipeline(shader: &ManagedShader) -> ComputePipelineSpec {
    // SAFETY: `shader` is a compute shader so `u.threadgroup_size` is initialized.
    let tg = unsafe { shader.reflection.u.threadgroup_size };
    let threadgroup_size = [tg[0].max(1), tg[1].max(1), tg[2].max(1)];
    ComputePipelineSpec {
        variant: ShaderVariant::Default,
        threadgroup_size,
    }
}
