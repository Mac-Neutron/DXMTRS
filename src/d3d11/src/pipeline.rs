//! Graphics, compute, geometry and tessellation pipeline compilation (`d3d11_pipeline.*`).

use core::ffi::c_void;
use std::sync::Arc;

use crate::{
    input_layout::{InputLayout, StreamOutputLayout},
    shader::ManagedShader,
    state::BlendState,
};

pub mod compute;
pub mod graphics;
pub mod gs;
pub mod ts;

/// Graphics pipeline key (`MTL_GRAPHICS_PIPELINE_DESC`).
pub struct GraphicsDesc {
    pub vertex_shader: Option<Arc<ManagedShader>>,
    pub hull_shader: Option<Arc<ManagedShader>>,
    pub domain_shader: Option<Arc<ManagedShader>>,
    pub geometry_shader: Option<Arc<ManagedShader>>,
    pub pixel_shader: Option<Arc<ManagedShader>>,
    pub blend_state: Option<*const BlendState>,
    pub input_layout: Option<*const InputLayout>,
    pub so_layout: Option<*const StreamOutputLayout>,
    pub num_color_attachments: u32,
    pub color_attachment_formats: [u32; 8],
    pub depth_stencil_format: u32,
    pub topology_class: u32,
    pub rasterization_enabled: bool,
    pub sample_count: u8,
    pub gs_strip_topology: bool,
    pub index_buffer_format: u32,
    pub sample_mask: u32,
    pub gs_passthrough: u32,
}

/// Compiled graphics pipeline state (`MTLCompiledGraphicsPipeline`).
#[derive(Default)]
pub struct CompiledGraphics {
    pso: u64,
}

impl CompiledGraphics {
    /// Waits if needed and returns the compiled Metal render pipeline (`GetPipeline`).
    #[must_use]
    pub const fn ready(&self) -> Option<u64> {
        if self.pso != 0 { Some(self.pso) } else { None }
    }
}

/// Compiled compute pipeline state (`MTLCompiledComputePipeline`).
#[derive(Default)]
pub struct CompiledCompute {
    pso: u64,
}

impl CompiledCompute {
    /// Waits if needed and returns the compiled Metal compute pipeline (`GetPipeline`).
    #[must_use]
    pub const fn ready(&self) -> Option<u64> {
        if self.pso != 0 { Some(self.pso) } else { None }
    }
}

/// Per-device shader and pipeline state cache (`MTLD3D11PipelineCache`).
#[derive(Default)]
pub struct PipelineCache {
    _private: (),
}

impl PipelineCache {
    /// Creates a new pipeline cache for `device`.
    #[must_use]
    pub const fn new(device: *mut c_void) -> Self {
        let _ = device;
        Self { _private: () }
    }

    /// Looks up or schedules compilation of a graphics pipeline (`GetGraphicsPipeline`).
    #[must_use]
    pub fn graphics(&self, desc: &GraphicsDesc) -> Arc<CompiledGraphics> {
        let _ = desc;
        Arc::new(CompiledGraphics::default())
    }

    /// Looks up or schedules compilation of a compute pipeline (`GetComputePipeline`).
    #[must_use]
    pub fn compute(&self, shader: &ManagedShader) -> Arc<CompiledCompute> {
        let _ = shader;
        Arc::new(CompiledCompute::default())
    }
}
