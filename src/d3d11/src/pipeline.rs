//! Render, mesh, tessellation and compute pipeline state caching (`d3d11_pipeline.{hpp,cpp}`).

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use std::sync::{Arc, Mutex, OnceLock};

use dxmt_core::format::{depth_stencil_planar_flags, is_unorm8_rt};
use rustc_hash::FxHashMap;
use winemetal_sys::{
    MTLDevice_newComputePipelineState, MTLDevice_newMeshRenderPipelineState,
    MTLDevice_newRenderPipelineState, MTLDevice_supportsFamily, Obj,
    wire::{
        WMTColorAttachmentBlendInfo, WMTComputePipelineInfo, WMTGPU_FAMILY_APPLE7,
        WMTGPU_FAMILY_APPLE9, WMTMeshRenderPipelineInfo, WMTRenderPipelineBlendInfo,
        WMTRenderPipelineInfo,
    },
};

use crate::{
    LOG,
    com::Hresult,
    device::Device,
    input_layout::{InputLayout, StreamOutputLayout},
    scheduler::{CompileJob, CompileScheduler},
    shader::{
        ManagedShader,
        args::{EMPTY_SHA1, IaLayoutChainInput, SoLayoutChainInput},
    },
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
    ts::{
        TessellationPipelineSpec, build_tessellation_pipeline,
        build_tessellation_pipeline_with_caps,
    },
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

/// Default `WMTRenderPipelineBlendInfo` (`WMT::InitializeRenderPipelineInfo` in `Metal.hpp`).
#[must_use]
pub const fn default_blend_info() -> WMTRenderPipelineBlendInfo {
    const ATTACHMENT: WMTColorAttachmentBlendInfo = WMTColorAttachmentBlendInfo {
        pixel_format: 0,
        blending_enabled: 0,
        write_mask: 15,
        rgb_blend_operation: 0,
        alpha_blend_operation: 0,
        src_rgb_blend_factor: 1,
        dst_rgb_blend_factor: 0,
        src_alpha_blend_factor: 1,
        dst_alpha_blend_factor: 0,
    };
    WMTRenderPipelineBlendInfo {
        colors: [ATTACHMENT; 8],
        alpha_to_coverage_enabled: 0,
        logic_operation_enabled: 0,
        logic_operation: 0,
    }
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
    pub blend_state_hash: u64,
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
            blend_state_hash: desc.blend_state.map_or(0, |p| {
                if p.is_null() {
                    return 0;
                }
                // SAFETY: `GraphicsDesc::blend_state` is a live `BlendState` pointer while this
                // descriptor is being turned into a key. Hashing the description, rather than the
                // address, keeps the key valid if an allocator reuses the object's address and
                // lets two equivalent states share a pipeline.
                let blend = unsafe { &*p.cast::<BlendState>() };
                // Copy field by field into zeroed storage: hashing the original record would
                // include its padding bytes, which the application's structure need not define.
                // SAFETY: `D3d11BlendDesc1` is a POD record of integers for which all zero bits
                // are a valid value.
                let mut desc =
                    unsafe { core::mem::zeroed::<crate::abi::records::D3d11BlendDesc1>() };
                desc.alpha_to_coverage_enable = blend.desc.alpha_to_coverage_enable;
                desc.independent_blend_enable = blend.desc.independent_blend_enable;
                for (dst, src) in desc.render_target.iter_mut().zip(&blend.desc.render_target) {
                    *dst = *src;
                }
                // SAFETY: `desc` is a plain record of exactly this many bytes, fully initialized.
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        (&raw const desc).cast::<u8>(),
                        core::mem::size_of::<crate::abi::records::D3d11BlendDesc1>(),
                    )
                };
                xxhash_rust::xxh3::xxh3_64(bytes)
            }),
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
static TS_GLOBAL_MUTEX: Mutex<()> = Mutex::new(());

pub enum GraphicsPipelineKind {
    Standard(Box<GraphicsPipelineSpec>),
    GeometryMesh(Box<GeometryPipelineSpec>),
    Tessellation(Box<TessellationPipelineSpec>),
}

/// Compiled graphics pipeline state (`MTLCompiledGraphicsPipeline`).
pub struct CompiledGraphics {
    handle: AtomicU64,
    pso: Mutex<Option<Obj>>,
    job: Mutex<Option<CompileJob>>,
    pub kind: Arc<GraphicsPipelineKind>,
}

impl Default for CompiledGraphics {
    fn default() -> Self {
        let desc = GraphicsDesc::default();
        Self {
            handle: AtomicU64::new(0),
            pso: Mutex::new(None),
            job: Mutex::new(None),
            kind: Arc::new(GraphicsPipelineKind::Standard(Box::new(
                build_graphics_pipeline(&desc),
            ))),
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
        let job = self
            .job
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(job) = job {
            job.wait();
        }
        let h2 = self.handle.load(Ordering::Acquire);
        (h2 != 0).then_some(h2)
    }
}

/// Compiled compute pipeline state (`MTLCompiledComputePipeline`).
pub struct CompiledCompute {
    handle: AtomicU64,
    pso: Mutex<Option<Obj>>,
    job: Mutex<Option<CompileJob>>,
    pub spec: Arc<ComputePipelineSpec>,
}

impl Default for CompiledCompute {
    fn default() -> Self {
        Self {
            handle: AtomicU64::new(0),
            pso: Mutex::new(None),
            job: Mutex::new(None),
            spec: Arc::new(ComputePipelineSpec {
                variant: crate::shader::args::ShaderVariant::Default,
                threadgroup_size: [1, 1, 1],
            }),
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
        let job = self
            .job
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(job) = job {
            job.wait();
        }
        let h2 = self.handle.load(Ordering::Acquire);
        (h2 != 0).then_some(h2)
    }
}

/// Per-device shader and pipeline state cache (`MTLD3D11PipelineCache`).
pub struct PipelineCache {
    device: *mut c_void,
    shaders: Mutex<FxHashMap<[u8; 20], Arc<ManagedShader>>>,
    graphics_psos: Mutex<FxHashMap<GraphicsKey, Arc<CompiledGraphics>>>,
    compute_psos: Mutex<FxHashMap<[u8; 20], Arc<CompiledCompute>>>,
    scheduler: OnceLock<CompileScheduler>,
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
            device,
            shaders: Mutex::new(FxHashMap::default()),
            graphics_psos: Mutex::new(FxHashMap::default()),
            compute_psos: Mutex::new(FxHashMap::default()),
            scheduler: OnceLock::new(),
        }
    }

    /// Associates `device` with this pipeline cache and opens the shader cache DB if live.
    pub fn set_device(&mut self, device: *mut c_void) {
        self.device = device;
        if let Some(dev) = Device::from_iface(device)
            && dev.core().device().handle() > 0x1000
        {
            let _ = dxmt_core::shader_cache::instance(
                dev.core().metal_version(),
                crate::shader::shader_cache_version(),
            );
        }
    }

    /// Looks up or initializes a deduplicated [`ManagedShader`] from DXBC `bytes`.
    ///
    /// # Errors
    ///
    /// Returns `E_INVALIDARG` or `E_FAIL` if `ManagedShader::from_bytecode` fails.
    pub fn get_or_create_shader(&self, bytes: &[u8]) -> Result<Arc<ManagedShader>, Hresult> {
        let sha1 = dxmt_util::sha1::Sha1::compute(bytes).0;
        {
            let map = self
                .shaders
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(existing) = map.get(&sha1) {
                return Ok(Arc::clone(existing));
            }
        }
        // Parse outside the map lock: two creators may race, but the second one is
        // discarded by the insert below and no unrelated shader waits behind the parse.
        let created = ManagedShader::from_bytecode_with_sha1(bytes, sha1)?;
        let mut map = self
            .shaders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = map.get(&sha1) {
            return Ok(Arc::clone(existing));
        }
        map.insert(sha1, Arc::clone(&created));
        drop(map);
        Ok(created)
    }

    /// Looks up or enqueues a graphics pipeline for `desc` (`GetGraphicsPipeline`).
    #[must_use]
    pub fn graphics(&self, desc: &GraphicsDesc) -> Arc<CompiledGraphics> {
        let key = GraphicsKey::from_desc(desc);
        {
            let map = self
                .graphics_psos
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(existing) = map.get(&key) {
                return Arc::clone(existing);
            }
        }

        let live_dev =
            Device::from_iface(self.device).filter(|dev| dev.core().device().handle() > 0x1000);

        let kind = if desc.hull_shader.is_some() && desc.domain_shader.is_some() {
            let spec = live_dev.map_or_else(
                || build_tessellation_pipeline(desc),
                |dev| {
                    let mtl = dev.core().device().handle();
                    // SAFETY: `mtl` is a live `WMTDevice` handle.
                    let a7 = unsafe { MTLDevice_supportsFamily(mtl, WMTGPU_FAMILY_APPLE7) };
                    // SAFETY: `mtl` is a live `WMTDevice` handle.
                    let a9 = unsafe { MTLDevice_supportsFamily(mtl, WMTGPU_FAMILY_APPLE9) };
                    build_tessellation_pipeline_with_caps(desc, a7, a9)
                },
            );
            GraphicsPipelineKind::Tessellation(Box::new(spec))
        } else if desc.geometry_shader.is_some() {
            GraphicsPipelineKind::GeometryMesh(Box::new(build_geometry_pipeline(desc)))
        } else {
            GraphicsPipelineKind::Standard(Box::new(build_graphics_pipeline(desc)))
        };
        let kind = Arc::new(kind);

        let compiled = if live_dev.is_some() {
            let data = GraphicsCompileData::from_desc(desc);
            let compiled = Arc::new(CompiledGraphics {
                handle: AtomicU64::new(0),
                pso: Mutex::new(None),
                job: Mutex::new(None),
                kind: Arc::clone(&kind),
            });
            let job = {
                let weak = Arc::downgrade(&compiled);
                let device_ptr = self.device as usize;
                let kind = Arc::clone(&kind);
                CompileJob::from_fn(move || {
                    let Some(compiled) = weak.upgrade() else {
                        return;
                    };
                    // SAFETY: the pipeline cache's owner waits for schedulers to become idle
                    // before the device (and this raw pointer) is dropped.
                    let Some(dev) = Device::from_iface(device_ptr as *mut c_void) else {
                        return;
                    };
                    let pso = data.compile(dev, &kind);
                    let handle = pso.as_ref().map_or(0, Obj::handle);
                    *compiled
                        .pso
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = pso;
                    compiled.handle.store(handle, Ordering::Release);
                })
            };
            *compiled
                .job
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(job);
            compiled
        } else {
            Arc::new(CompiledGraphics {
                handle: AtomicU64::new(next_pso_handle()),
                pso: Mutex::new(None),
                job: Mutex::new(None),
                kind,
            })
        };

        // The entry is visible only after its job is installed, so a cache hit never observes a
        // placeholder that `ready` cannot wait on.
        let mut map = self
            .graphics_psos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = map.get(&key) {
            return Arc::clone(existing);
        }
        let job = compiled
            .job
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        map.insert(key, Arc::clone(&compiled));
        drop(map);
        if let Some(job) = job {
            self.scheduler().submit(job);
        }
        compiled
    }

    fn scheduler(&self) -> &CompileScheduler {
        self.scheduler.get_or_init(CompileScheduler::default)
    }

    /// Looks up or enqueues a compute pipeline for `shader` (`GetComputePipeline`).
    #[must_use]
    pub fn compute(&self, shader: &Arc<ManagedShader>) -> Arc<CompiledCompute> {
        {
            let map = self
                .compute_psos
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(existing) = map.get(&shader.sha1) {
                return Arc::clone(existing);
            }
        }
        let spec = Arc::new(build_compute_pipeline(shader));

        let live_dev =
            Device::from_iface(self.device).filter(|dev| dev.core().device().handle() > 0x1000);
        let compiled = if live_dev.is_some() {
            let compiled = Arc::new(CompiledCompute {
                handle: AtomicU64::new(0),
                pso: Mutex::new(None),
                job: Mutex::new(None),
                spec: Arc::clone(&spec),
            });
            let job = {
                let weak = Arc::downgrade(&compiled);
                let device_ptr = self.device as usize;
                let shader = Arc::clone(shader);
                let spec = Arc::clone(&spec);
                CompileJob::from_fn(move || {
                    let Some(compiled) = weak.upgrade() else {
                        return;
                    };
                    // SAFETY: as in `graphics`: device teardown waits for scheduler idle.
                    let Some(dev) = Device::from_iface(device_ptr as *mut c_void) else {
                        return;
                    };
                    let pso = compile_compute_pso(dev, &shader, &spec);
                    let handle = pso.as_ref().map_or(0, Obj::handle);
                    *compiled
                        .pso
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = pso;
                    compiled.handle.store(handle, Ordering::Release);
                })
            };
            *compiled
                .job
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(job);
            compiled
        } else {
            Arc::new(CompiledCompute {
                handle: AtomicU64::new(next_pso_handle()),
                pso: Mutex::new(None),
                job: Mutex::new(None),
                spec,
            })
        };

        let mut map = self
            .compute_psos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = map.get(&shader.sha1) {
            return Arc::clone(existing);
        }
        let job = compiled
            .job
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        map.insert(shader.sha1, Arc::clone(&compiled));
        drop(map);
        if let Some(job) = job {
            self.scheduler().submit(job);
        }
        compiled
    }

    /// Releases every cached Metal object before the device's Metal device goes away.
    pub fn clear(&self) {
        self.graphics_psos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.compute_psos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.shaders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

/// A copy of the parts of [`GraphicsDesc`] a background compile may touch.
///
/// The descriptor itself carries application pointers for its input and stream-output layouts;
/// a worker cannot use those after the caller returns, so it owns clones of the two small layout
/// records and rebuilds a descriptor that points at them while it runs.
struct GraphicsCompileData {
    vertex_shader: Option<Arc<ManagedShader>>,
    hull_shader: Option<Arc<ManagedShader>>,
    domain_shader: Option<Arc<ManagedShader>>,
    geometry_shader: Option<Arc<ManagedShader>>,
    pixel_shader: Option<Arc<ManagedShader>>,
    input_layout: Option<InputLayout>,
    so_layout: Option<StreamOutputLayout>,
    num_color_attachments: u32,
    color_attachment_formats: [u32; 8],
    depth_stencil_format: u32,
    topology_class: u32,
    rasterization_enabled: bool,
    sample_count: u8,
    gs_strip_topology: bool,
    index_buffer_format: u32,
    sample_mask: u32,
    gs_passthrough: u32,
}

impl GraphicsCompileData {
    fn from_desc(desc: &GraphicsDesc) -> Self {
        // SAFETY: the caller keeps each layout alive for the duration of `graphics`; the clone is
        // taken here, before the descriptor can be reused.
        let input_layout = desc
            .input_layout
            .filter(|layout| !layout.is_null())
            .map(|layout| unsafe { (*layout).clone() });
        // SAFETY: as above.
        let so_layout = desc
            .so_layout
            .filter(|layout| !layout.is_null())
            .map(|layout| unsafe { (*layout).clone() });
        Self {
            vertex_shader: desc.vertex_shader.clone(),
            hull_shader: desc.hull_shader.clone(),
            domain_shader: desc.domain_shader.clone(),
            geometry_shader: desc.geometry_shader.clone(),
            pixel_shader: desc.pixel_shader.clone(),
            input_layout,
            so_layout,
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

    fn desc(&self) -> GraphicsDesc {
        GraphicsDesc {
            vertex_shader: self.vertex_shader.clone(),
            hull_shader: self.hull_shader.clone(),
            domain_shader: self.domain_shader.clone(),
            geometry_shader: self.geometry_shader.clone(),
            pixel_shader: self.pixel_shader.clone(),
            blend_state: None,
            input_layout: self.input_layout.as_ref().map(core::ptr::from_ref),
            so_layout: self.so_layout.as_ref().map(core::ptr::from_ref),
            num_color_attachments: self.num_color_attachments,
            color_attachment_formats: self.color_attachment_formats,
            depth_stencil_format: self.depth_stencil_format,
            topology_class: self.topology_class,
            rasterization_enabled: self.rasterization_enabled,
            sample_count: self.sample_count,
            gs_strip_topology: self.gs_strip_topology,
            index_buffer_format: self.index_buffer_format,
            sample_mask: self.sample_mask,
            gs_passthrough: self.gs_passthrough,
        }
    }

    fn compile(&self, dev: &Device, kind: &GraphicsPipelineKind) -> Option<Obj> {
        compile_graphics_pso(dev, &self.desc(), kind)
    }
}

fn compile_graphics_pso(
    dev: &Device,
    desc: &GraphicsDesc,
    kind: &GraphicsPipelineKind,
) -> Option<Obj> {
    let mtl_dev = dev.core().device().handle();
    let ia_input = desc.input_layout.and_then(|il| {
        if il.is_null() {
            None
        } else {
            // SAFETY: `il` points to a valid `InputLayout`.
            let il_ref = unsafe { &*il };
            Some(IaLayoutChainInput {
                slot_mask: il_ref.slot_mask(),
                elements: il_ref.elements(),
            })
        }
    });
    let so_input = desc.so_layout.and_then(|so| {
        if so.is_null() {
            None
        } else {
            // SAFETY: `so` points to a valid `StreamOutputLayout`.
            let so_ref = unsafe { &*so };
            Some(SoLayoutChainInput {
                elements: &so_ref.elements,
                strides: so_ref.strides,
            })
        }
    });

    match *kind {
        GraphicsPipelineKind::Standard(ref spec) => {
            let vs_fn = desc.vertex_shader.as_ref().map_or(0, |vs| {
                vs.compile_variant(dev, &spec.vs_variant, ia_input, so_input, None)
            });
            if vs_fn == 0 {
                return None;
            }
            let ps_fn = match (desc.pixel_shader.as_ref(), spec.ps_variant.as_ref()) {
                (Some(ps), Some(pv)) => {
                    let h = ps.compile_variant(dev, pv, None, None, None);
                    if h == 0 {
                        return None;
                    }
                    h
                }
                _ => 0,
            };
            // SAFETY: `WMTRenderPipelineInfo` is a C ABI struct of plain integers.
            let mut info: WMTRenderPipelineInfo = unsafe { core::mem::zeroed() };
            info.colors = spec.blend_info.colors;
            info.alpha_to_coverage_enabled = spec.blend_info.alpha_to_coverage_enabled;
            info.vertex_function = vs_fn;
            info.fragment_function = ps_fn;
            info.rasterization_enabled = u8::from(desc.rasterization_enabled);
            info.depth_pixel_format = spec.depth_pixel_format;
            info.stencil_pixel_format = spec.stencil_pixel_format;
            info.input_primitive_topology = desc.topology_class;
            info.raster_sample_count = desc.sample_count;
            info.immutable_vertex_buffers = spec.immutable_vertex_buffers;
            info.immutable_fragment_buffers = spec.immutable_fragment_buffers;
            info.max_tessellation_factor = 16;
            info.tessellation_factor_step = 1;

            let mut err_raw = 0u64;
            // SAFETY: `mtl_dev` is live and `info` is valid.
            let pso_raw = unsafe {
                MTLDevice_newRenderPipelineState(mtl_dev, &raw mut info, &raw mut err_raw)
            };
            // SAFETY: adopts returned error reference if non-zero.
            let _err_obj = unsafe { Obj::adopt(err_raw) };
            // SAFETY: adopts +1 reference from `MTLDevice_newRenderPipelineState`.
            let pso_obj = unsafe { Obj::adopt(pso_raw) };
            if pso_obj.is_none() {
                LOG.err("Failed to create PSO");
            }
            pso_obj
        }
        GraphicsPipelineKind::GeometryMesh(ref spec) => {
            let (Some(vs), Some(gs)) = (desc.vertex_shader.as_ref(), desc.geometry_shader.as_ref())
            else {
                return None;
            };
            let vs_fn = vs.compile_variant(dev, &spec.vs_variant, ia_input, None, Some(gs));
            if vs_fn == 0 {
                LOG.err("Failed to create mesh PSO: Invalid vertex shader.");
                return None;
            }
            let gs_fn = gs.compile_variant(dev, &spec.gs_variant, None, None, Some(vs));
            if gs_fn == 0 {
                LOG.err("Failed to create mesh PSO: Invalid geometry shader.");
                return None;
            }
            let ps_fn = match (desc.pixel_shader.as_ref(), spec.ps_variant.as_ref()) {
                (Some(ps), Some(pv)) => {
                    let h = ps.compile_variant(dev, pv, None, None, None);
                    if h == 0 {
                        LOG.err("Failed to create mesh PSO: Invalid pixel shader.");
                        return None;
                    }
                    h
                }
                _ => 0,
            };
            // SAFETY: `WMTMeshRenderPipelineInfo` is a C ABI struct of plain integers.
            let mut info: WMTMeshRenderPipelineInfo = unsafe { core::mem::zeroed() };
            info.colors = spec.blend_info.colors;
            info.alpha_to_coverage_enabled = spec.blend_info.alpha_to_coverage_enabled;
            info.object_function = vs_fn;
            info.mesh_function = gs_fn;
            info.fragment_function = ps_fn;
            info.payload_memory_length = spec.payload_memory_length;
            info.immutable_object_buffers = spec.immutable_object_buffers;
            info.immutable_mesh_buffers = spec.immutable_mesh_buffers;
            info.immutable_fragment_buffers = spec.immutable_fragment_buffers;
            info.rasterization_enabled = u8::from(desc.rasterization_enabled);
            info.depth_pixel_format = spec.depth_pixel_format;
            info.stencil_pixel_format = spec.stencil_pixel_format;
            info.raster_sample_count = desc.sample_count;

            let mut err_raw = 0u64;
            // SAFETY: `mtl_dev` is live and `info` is valid.
            let pso_raw = unsafe {
                MTLDevice_newMeshRenderPipelineState(mtl_dev, &raw mut info, &raw mut err_raw)
            };
            // SAFETY: adopts returned error reference if non-zero.
            let _err_obj = unsafe { Obj::adopt(err_raw) };
            // SAFETY: adopts +1 reference from `MTLDevice_newMeshRenderPipelineState`.
            let pso_obj = unsafe { Obj::adopt(pso_raw) };
            if pso_obj.is_none() {
                LOG.err("Failed to create mesh PSO");
            }
            pso_obj
        }
        GraphicsPipelineKind::Tessellation(ref spec) => {
            let (Some(vs), Some(hs), Some(ds)) = (
                desc.vertex_shader.as_ref(),
                desc.hull_shader.as_ref(),
                desc.domain_shader.as_ref(),
            ) else {
                return None;
            };
            let vhs_fn = hs.compile_variant(dev, &spec.vshs_variant, ia_input, None, Some(vs));
            if vhs_fn == 0 {
                LOG.err("Failed to create tess-mesh PSO: Invalid vertex-hull shader.");
                return None;
            }
            let ds_fn = ds.compile_variant(dev, &spec.ds_variant, None, None, Some(hs));
            if ds_fn == 0 {
                LOG.err("Failed to create tess-mesh PSO: Invalid domain shader.");
                return None;
            }
            let ps_fn = match (desc.pixel_shader.as_ref(), spec.ps_variant.as_ref()) {
                (Some(ps), Some(pv)) => {
                    let h = ps.compile_variant(dev, pv, None, None, None);
                    if h == 0 {
                        LOG.err("Failed to create mesh PSO: Invalid pixel shader.");
                        return None;
                    }
                    h
                }
                _ => 0,
            };
            // SAFETY: `WMTMeshRenderPipelineInfo` is a C ABI struct of plain integers.
            let mut info: WMTMeshRenderPipelineInfo = unsafe { core::mem::zeroed() };
            info.colors = spec.blend_info.colors;
            info.alpha_to_coverage_enabled = spec.blend_info.alpha_to_coverage_enabled;
            info.object_function = vhs_fn;
            info.mesh_function = ds_fn;
            info.fragment_function = ps_fn;
            info.payload_memory_length = spec.payload_memory_length;
            info.immutable_object_buffers = spec.immutable_object_buffers;
            info.immutable_mesh_buffers = spec.immutable_mesh_buffers;
            info.immutable_fragment_buffers = spec.immutable_fragment_buffers;
            info.rasterization_enabled = u8::from(desc.rasterization_enabled);
            info.depth_pixel_format = spec.depth_pixel_format;
            info.stencil_pixel_format = spec.stencil_pixel_format;
            info.raster_sample_count = desc.sample_count;
            info.mesh_tgsize_is_multiple_of_sgwidth = 1;
            info.object_tgsize_is_multiple_of_sgwidth = 1;

            let mut err_raw = 0u64;
            let pso_raw = {
                let _lock = TS_GLOBAL_MUTEX
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                // SAFETY: `mtl_dev` is live and `info` is valid.
                unsafe {
                    MTLDevice_newMeshRenderPipelineState(mtl_dev, &raw mut info, &raw mut err_raw)
                }
            };
            // SAFETY: adopts returned error reference if non-zero.
            let _err_obj = unsafe { Obj::adopt(err_raw) };
            // SAFETY: adopts +1 reference from `MTLDevice_newMeshRenderPipelineState`.
            let pso_obj = unsafe { Obj::adopt(pso_raw) };
            if pso_obj.is_none() {
                LOG.err("Failed to create tessellation raster PSO");
            }
            pso_obj
        }
    }
}

fn compile_compute_pso(
    dev: &Device,
    shader: &ManagedShader,
    spec: &ComputePipelineSpec,
) -> Option<Obj> {
    let cs_fn = shader.compile_variant(dev, &spec.variant, None, None, None);
    if cs_fn == 0 {
        return None;
    }
    let mtl_dev = dev.core().device().handle();
    let total_tg = spec.threadgroup_size[0] * spec.threadgroup_size[1] * spec.threadgroup_size[2];
    // SAFETY: `WMTComputePipelineInfo` is a C ABI struct of plain integers.
    let mut info: WMTComputePipelineInfo = unsafe { core::mem::zeroed() };
    info.compute_function = cs_fn;
    info.tgsize_is_multiple_of_sgwidth = u8::from(total_tg.is_multiple_of(32));
    info.immutable_buffers = IMMUTABLE_FRAGMENT_BUFFERS;
    let mut err_raw = 0u64;
    // SAFETY: `mtl_dev` is live and `info` is valid.
    let pso_raw =
        unsafe { MTLDevice_newComputePipelineState(mtl_dev, &raw mut info, &raw mut err_raw) };
    // SAFETY: adopts returned error reference if non-zero.
    let _err_obj = unsafe { Obj::adopt(err_raw) };
    // SAFETY: adopts +1 reference from `MTLDevice_newComputePipelineState`.
    let pso_obj = unsafe { Obj::adopt(pso_raw) };
    if pso_obj.is_none() {
        LOG.err("Failed to create compute PSO");
    }
    pso_obj
}

#[cfg(test)]
mod tests;
