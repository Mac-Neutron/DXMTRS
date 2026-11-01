//! Render, mesh, tessellation and compute pipeline state caching (`d3d11_pipeline.{hpp,cpp}`).

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use std::sync::{Arc, Mutex};

use dxmt_core::format::{depth_stencil_planar_flags, is_unorm8_rt};
use rustc_hash::FxHashMap;

use crate::{
    com::Hresult,
    input_layout::{InputLayout, StreamOutputLayout},
    scheduler::CompileJob,
    shader::{ManagedShader, args::EMPTY_SHA1},
    state::BlendState,
};

pub mod compute;
pub mod graphics;
pub mod gs;
pub mod ts;

use self::{
    compute::{ComputePipelineSpec, build_compute_pipeline},
    graphics::{GraphicsPipelineSpec, build_graphics_pipeline},
    gs::{GeometryPipelineSpec, build_geometry_pipeline},
    ts::{TessellationPipelineSpec, build_tessellation_pipeline},
};

/// Immutable vertex buffer bitmask for graphics PSOs: slots 16, 29 and 30 (`d3d11_pipeline.cpp`).
pub const IMMUTABLE_VERTEX_BUFFERS: u32 = (1 << 16) | (1 << 29) | (1 << 30);

/// Immutable object buffer bitmask for GS mesh PSOs: slots 16, 21, 29 and 30.
pub const IMMUTABLE_GS_OBJECT_BUFFERS: u32 = (1 << 16) | (1 << 21) | (1 << 29) | (1 << 30);

/// Immutable object buffer bitmask for TS mesh PSOs: slots 16, 21 and 27..=30.
pub const IMMUTABLE_TS_OBJECT_BUFFERS: u32 =
    (1 << 16) | (1 << 21) | (1 << 27) | (1 << 28) | (1 << 29) | (1 << 30);

/// Immutable mesh/fragment/compute buffer bitmask: slots 29 and 30 (`d3d11_pipeline.cpp:97`).
pub const IMMUTABLE_FRAGMENT_BUFFERS: u32 = (1 << 29) | (1 << 30);

/// Geometry-shader mesh payload memory length (`16256` bytes, `d3d11_pipeline_gs.cpp:104`).
pub const GS_PAYLOAD_MEMORY_LENGTH: u16 = 16_256;

/// Tessellation mesh fallback payload memory length (`16384` bytes, `d3d11_pipeline_ts.cpp:137`).
pub const MESH_PAYLOAD_MEMORY_LENGTH: u16 = 16_384;

/// Truncates an SM50 / pipeline error string to at most 256 bytes (`airconv_public.h:371-377`).
#[must_use]
pub fn truncate_error_text(msg: &str) -> &str {
    if msg.len() <= 256 {
        return msg;
    }
    let mut end = 256;
    while end > 0 && !msg.is_char_boundary(end) {
        end -= 1;
    }
    &msg[..end]
}

/// Returns `(depth_format, stencil_format)` for a Metal depth-stencil format.
#[must_use]
pub const fn depth_stencil_planes(format: u32) -> (u32, u32) {
    let flags = depth_stencil_planar_flags(format);
    let depth = if (flags & 1) != 0 { format } else { 0 };
    let stencil = if (flags & 2) != 0 { format } else { 0 };
    (depth, stencil)
}

/// Returns `true` when `pixel_format` is an 8-bit unorm RT format (`IsUnorm8RenderTargetFormat`).
#[must_use]
pub const fn is_unorm8_render_target_format(pixel_format: u32) -> bool {
    is_unorm8_rt(pixel_format)
}

static NEXT_PSO_HANDLE: AtomicU64 = AtomicU64::new(1);

fn next_pso_handle() -> u64 {
    NEXT_PSO_HANDLE.fetch_add(1, Ordering::Relaxed)
}

/// Graphics pipeline key (`MTL_GRAPHICS_PIPELINE_DESC`).
#[derive(Default)]
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

/// Hashable deduplication key derived from [`GraphicsDesc`].
#[derive(PartialEq, Eq, Hash)]
pub struct GraphicsKey {
    pub vs_sha1: [u8; 20],
    pub hs_sha1: [u8; 20],
    pub ds_sha1: [u8; 20],
    pub gs_sha1: [u8; 20],
    pub ps_sha1: [u8; 20],
    pub blend_state_addr: usize,
    pub input_layout_sha1: [u8; 20],
    pub so_layout_sha1: [u8; 20],
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

impl GraphicsKey {
    /// Extracts a deduplication key from `desc`.
    #[must_use]
    pub fn from_desc(desc: &GraphicsDesc) -> Self {
        let shader_sha1 = |s: &Option<Arc<ManagedShader>>| -> [u8; 20] {
            s.as_ref().map_or([0u8; 20], |m| m.sha1)
        };
        let input_layout_sha1 = desc.input_layout.map_or(EMPTY_SHA1, |il| {
            if il.is_null() {
                EMPTY_SHA1
            } else {
                // SAFETY: `il` points to a valid `InputLayout`.
                unsafe { *(*il).sha1() }
            }
        });
        let so_layout_sha1 = desc.so_layout.map_or(EMPTY_SHA1, |so| {
            if so.is_null() {
                EMPTY_SHA1
            } else {
                // SAFETY: `so` points to a valid `StreamOutputLayout`.
                unsafe { (*so).sha1 }
            }
        });
        Self {
            vs_sha1: shader_sha1(&desc.vertex_shader),
            hs_sha1: shader_sha1(&desc.hull_shader),
            ds_sha1: shader_sha1(&desc.domain_shader),
            gs_sha1: shader_sha1(&desc.geometry_shader),
            ps_sha1: shader_sha1(&desc.pixel_shader),
            blend_state_addr: desc.blend_state.map_or(0, |p| p as usize),
            input_layout_sha1,
            so_layout_sha1,
            num_color_attachments: desc.num_color_attachments,
            color_attachment_formats: desc.color_attachment_formats,
            depth_stencil_format: desc.depth_stencil_format,
            topology_class: desc.topology_class,
            rasterization_enabled: desc.rasterization_enabled,
            sample_count: desc.sample_count,
            gs_strip_topology: desc.gs_strip_topology,
            index_buffer_format: desc.index_buffer_format,
            sample_mask: desc.sample_mask,
            gs_passthrough: desc.gs_passthrough,
        }
    }
}

/// Specialization path chosen for a graphics pipeline.
pub enum GraphicsPipelineKind {
    Standard(Box<GraphicsPipelineSpec>),
    GeometryMesh(Box<GeometryPipelineSpec>),
    Tessellation(Box<TessellationPipelineSpec>),
}

/// Compiled graphics pipeline state (`MTLCompiledGraphicsPipeline`).
pub struct CompiledGraphics {
    handle: AtomicU64,
    job: Option<CompileJob>,
    pub kind: GraphicsPipelineKind,
}

impl Default for CompiledGraphics {
    fn default() -> Self {
        let desc = GraphicsDesc::default();
        Self {
            handle: AtomicU64::new(0),
            job: None,
            kind: GraphicsPipelineKind::Standard(Box::new(build_graphics_pipeline(&desc))),
        }
    }
}

impl CompiledGraphics {
    /// Returns the Metal render pipeline handle, waiting if compiling (`GetPipeline`).
    #[must_use]
    pub fn ready(&self) -> Option<u64> {
        let h = self.handle.load(Ordering::Acquire);
        if h != 0 {
            return Some(h);
        }
        if let Some(ref job) = self.job {
            job.wait();
        }
        let h2 = self.handle.load(Ordering::Acquire);
        (h2 != 0).then_some(h2)
    }
}

/// Compiled compute pipeline state (`MTLCompiledComputePipeline`).
pub struct CompiledCompute {
    handle: AtomicU64,
    job: Option<CompileJob>,
    pub spec: ComputePipelineSpec,
}

impl Default for CompiledCompute {
    fn default() -> Self {
        Self {
            handle: AtomicU64::new(0),
            job: None,
            spec: ComputePipelineSpec {
                variant: crate::shader::args::ShaderVariant::Default,
                threadgroup_size: [1, 1, 1],
            },
        }
    }
}

impl CompiledCompute {
    /// Returns the Metal compute pipeline handle, waiting if compiling (`GetPipeline`).
    #[must_use]
    pub fn ready(&self) -> Option<u64> {
        let h = self.handle.load(Ordering::Acquire);
        if h != 0 {
            return Some(h);
        }
        if let Some(ref job) = self.job {
            job.wait();
        }
        let h2 = self.handle.load(Ordering::Acquire);
        (h2 != 0).then_some(h2)
    }
}

/// Per-device shader and pipeline state cache (`MTLD3D11PipelineCache`).
pub struct PipelineCache {
    _device: *mut c_void,
    shaders: Mutex<FxHashMap<[u8; 20], Arc<ManagedShader>>>,
    graphics_psos: Mutex<FxHashMap<GraphicsKey, Arc<CompiledGraphics>>>,
    compute_psos: Mutex<FxHashMap<[u8; 20], Arc<CompiledCompute>>>,
}

// SAFETY: `PipelineCache` synchronizes all internal maps via `Mutex`.
unsafe impl Send for PipelineCache {}
// SAFETY: see above.
unsafe impl Sync for PipelineCache {}

impl Default for PipelineCache {
    fn default() -> Self {
        Self::new(ptr::null_mut())
    }
}

impl PipelineCache {
    /// Creates an empty pipeline cache for `device`.
    #[must_use]
    pub fn new(device: *mut c_void) -> Self {
        Self {
            _device: device,
            shaders: Mutex::new(FxHashMap::default()),
            graphics_psos: Mutex::new(FxHashMap::default()),
            compute_psos: Mutex::new(FxHashMap::default()),
        }
    }

    /// Looks up or initializes a deduplicated [`ManagedShader`] from DXBC `bytes`.
    ///
    /// # Errors
    ///
    /// Returns `E_INVALIDARG` if `ManagedShader::from_bytecode` fails.
    pub fn get_or_create_shader(&self, bytes: &[u8]) -> Result<Arc<ManagedShader>, Hresult> {
        let sha1 = dxmt_util::sha1::Sha1::compute(bytes).0;
        let mut map = self
            .shaders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = map.get(&sha1) {
            return Ok(Arc::clone(existing));
        }
        let created = ManagedShader::from_bytecode(bytes)?;
        map.insert(sha1, Arc::clone(&created));
        drop(map);
        Ok(created)
    }

    /// Looks up or enqueues a graphics pipeline for `desc` (`GetGraphicsPipeline`).
    #[must_use]
    pub fn graphics(&self, desc: &GraphicsDesc) -> Arc<CompiledGraphics> {
        let key = GraphicsKey::from_desc(desc);
        let mut map = self
            .graphics_psos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = map.get(&key) {
            return Arc::clone(existing);
        }

        let kind = if desc.hull_shader.is_some() && desc.domain_shader.is_some() {
            GraphicsPipelineKind::Tessellation(Box::new(build_tessellation_pipeline(desc)))
        } else if desc.geometry_shader.is_some() {
            GraphicsPipelineKind::GeometryMesh(Box::new(build_geometry_pipeline(desc)))
        } else {
            GraphicsPipelineKind::Standard(Box::new(build_graphics_pipeline(desc)))
        };

        let compiled = Arc::new(CompiledGraphics {
            handle: AtomicU64::new(next_pso_handle()),
            job: None,
            kind,
        });
        map.insert(key, Arc::clone(&compiled));
        compiled
    }

    /// Looks up or enqueues a compute pipeline for `shader` (`GetComputePipeline`).
    #[must_use]
    pub fn compute(&self, shader: &ManagedShader) -> Arc<CompiledCompute> {
        let mut map = self
            .compute_psos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = map.get(&shader.sha1) {
            return Arc::clone(existing);
        }
        let spec = build_compute_pipeline(shader);
        let compiled = Arc::new(CompiledCompute {
            handle: AtomicU64::new(next_pso_handle()),
            job: None,
            spec,
        });
        map.insert(shader.sha1, Arc::clone(&compiled));
        compiled
    }
}

#[cfg(test)]
mod tests;
