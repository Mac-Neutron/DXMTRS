use core::{ffi::c_void, ops::DerefMut, ptr::NonNull};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering},
};

use log::{debug, error, info, trace, warn};
pub use mtld3d_core::encoder_data::StretchSurfaceFlags;

mod core_arms;
mod core_draw;
mod extended;
mod mem_watch;

use mtld3d_core::{
    api_lock::{ApiGuard, ApiLock},
    caps,
    config::Mtld3dConfig,
    convert::{self, fvf_to_elements},
    dirty_rect::DirtyRect,
    dxso::operand_token_count,
    ff_state::{FfState, FfVsDirty, TssWriteFeeds, tss_write_feeds},
    format::{
        FormatMapping, compute_mip_count, compute_mip_size, compute_volume_mip_count,
        is_dxt_format, linear_mip_size, linear_row_pitch, map_d3d_format, resolve_mip_levels,
    },
    ids::{ProgramId, TextureId},
    page_box::PageBox,
    perf::{ApiPerfState, ApiPerfStorage, ApiTimer, BindSubCategory, DeviceSubCategory, KeysGate},
    readback::{ReadbackDestination, ReadbackReject, ReadbackSource},
    render_scale::TargetExtent,
    render_state::{RsClass, rs_classify},
    reset_summary::{
        ResetDone, ResetFailed, ResetFailureEffect, ResetMethod, ResetSummary,
        changes_presentation, failure_warns, success_logs_at_info,
    },
    shader_constants::{int_bool_rows, window_in_range},
    streams::validate_stream_freq,
    texture_flags::TextureFlags,
    validate_device::{StageFilterVerdict, stage_filter_verdict},
};
use mtld3d_shared::{InPtr, InPtrMut, OutPtr, ValueIn, VtableThis};
use mtld3d_types::{
    D3D_MAX_SIMULTANEOUS_RENDERTARGETS, D3DCAPS9, D3DDEVICE_CREATION_PARAMETERS, D3DDISPLAYMODE,
    D3DERR_UNSUPPORTEDTEXTUREFILTER, D3DFMT_ATI1, D3DFMT_UYVY, D3DFMT_YUY2, D3DGAMMARAMP,
    D3DLIGHT9, D3DMATERIAL9, D3DMATRIX, D3DPOOL_DEFAULT, D3DPOOL_MANAGED, D3DPOOL_MANAGED_EX,
    D3DPOOL_SCRATCH, D3DPOOL_SYSTEMMEM, D3DPRESENT_PARAMETERS, D3DPRESENTFLAG_LOCKABLE_BACKBUFFER,
    D3DRECT, D3DRS_ALPHAFUNC, D3DRS_ALPHAREF, D3DRS_ALPHATESTENABLE, D3DRS_AMBIENT,
    D3DRS_AMBIENTMATERIALSOURCE, D3DRS_CLIPPING, D3DRS_CLIPPLANEENABLE, D3DRS_COLORVERTEX,
    D3DRS_DEPTHBIAS, D3DRS_DIFFUSEMATERIALSOURCE, D3DRS_EMISSIVEMATERIALSOURCE, D3DRS_FOGCOLOR,
    D3DRS_FOGDENSITY, D3DRS_FOGENABLE, D3DRS_FOGEND, D3DRS_FOGSTART, D3DRS_FOGTABLEMODE,
    D3DRS_FOGVERTEXMODE, D3DRS_INDEXEDVERTEXBLENDENABLE, D3DRS_LIGHTING, D3DRS_LOCALVIEWER,
    D3DRS_NORMALIZENORMALS, D3DRS_POINTSCALE_A, D3DRS_POINTSCALE_B, D3DRS_POINTSCALE_C,
    D3DRS_POINTSCALEENABLE, D3DRS_POINTSIZE, D3DRS_POINTSIZE_MAX, D3DRS_POINTSIZE_MIN,
    D3DRS_POINTSPRITEENABLE, D3DRS_RANGEFOGENABLE, D3DRS_SHADEMODE, D3DRS_SPECULARENABLE,
    D3DRS_SPECULARMATERIALSOURCE, D3DRS_SRGBWRITEENABLE, D3DRS_TEXTUREFACTOR, D3DRS_VERTEXBLEND,
    D3DRTYPE_CUBETEXTURE, D3DTEXF_LINEAR, D3DTEXF_NONE, D3DTEXF_POINT, D3DUSAGE_AUTOGENMIPMAP,
    D3DUSAGE_DEPTHSTENCIL, D3DUSAGE_DMAP, D3DUSAGE_DONOTCLIP, D3DUSAGE_DYNAMIC, D3DUSAGE_NONSECURE,
    D3DUSAGE_NPATCHES, D3DUSAGE_POINTS, D3DUSAGE_QUERY_FILTER, D3DUSAGE_RENDERTARGET,
    D3DUSAGE_RTPATCHES, D3DUSAGE_SOFTWAREPROCESSING, D3DUSAGE_WRITEONLY, D3DVIEWPORT9, Guid,
    IDirect3DDevice9ExVtbl, IDirect3DDevice9Vtbl, RENDER_STATE_COUNT, SAMPLER_STATE_COUNT,
    render_state_defaults,
};

use super::{
    D3D_OK, D3DERR_INVALIDCALL, D3DERR_NOTAVAILABLE, E_FAIL, LOG_TARGET,
    bound_buffers::BoundBuffers,
    bound_rt::{BoundRt, RENDER_TARGET_SLOTS},
    com_ref::{Bound, CachedComPtr},
    cursor::{self, CursorState},
    direct3d9::is_depth_stencil_format,
    index_buffer::{Direct3DIndexBuffer9, IndexBufferCreateInfo},
    null_out,
    pixel_shader::Direct3DPixelShader9,
    shader_bindings::{
        BOOL_CONSTANT_COUNT, CONSTANT_ROWS, INT_CONSTANT_ROWS, PS_FLOAT_CONSTANT_LIMIT,
        ShaderBindings,
    },
    stage_bindings::{STAGE_COUNT, StageBindings, TextureSwapDelta},
    state_block::{RecordingStateBlock, StateOp},
    surface::{ColorTargetCreateInfo, Direct3DSurface9, SurfaceMultiSample, SystemMemoryDst},
    texture::{CUBE_FACE_COUNT, Direct3DTexture9, SourceImage, TextureCreateInfo, TextureInner},
    vertex_buffer::{Direct3DVertexBuffer9, VertexBufferCreateInfo},
    vertex_decl::{Direct3DVertexDeclaration9, VertexDeclCreateInfo},
    vertex_shader::Direct3DVertexShader9,
};

/// Describes a surface endpoint for `StretchRect` and `ColorFill` validation and execution.
pub struct StretchSurfaceInfo {
    pub is_texture: bool,
    pub width: u32,
    pub height: u32,
    pub texture_size: (u32, u32),
    pub scale: mtld3d_core::render_scale::RenderScale,
    pub format: u32,
    pub mip_level: u32,
    pub slice: Option<u32>,
    pub pool: u32,
    pub flags: StretchSurfaceFlags,
    pub autogen_texture_id: Option<TextureId>,
    pub sample_count: u8,
}

impl StretchSurfaceInfo {
    const fn class_name(&self) -> &'static str {
        if self.flags.contains(StretchSurfaceFlags::IS_DEPTH_STENCIL) {
            "depth-stencil"
        } else if self.is_texture {
            "texture level"
        } else if self
            .flags
            .contains(StretchSurfaceFlags::IS_OFFSCREEN_PLAIN_DEFAULT)
        {
            "offscreen plain"
        } else {
            "render target"
        }
    }
}

/// Logs one process-wide warning per `feature` for a D3D9 capability the core runtime refuses.
pub fn refuse_under_core(feature: &'static str) {
    mtld3d_shared::log_once_warn_by!(
        target: LOG_TARGET,
        key: feature.as_ptr() as u64,
        "d3d9 core: {feature} is not implemented \u{2192} D3DERR_NOTAVAILABLE"
    );
}

/// Sub-target for the once-per-distinct texture-create diagnostic in `device_create_texture`.
///
/// Permanent probe (zero-cost when off); gated under its own sub-target so
/// a texture investigation can `RUST_LOG=mtld3d::d3d9::tex=trace` without
/// flipping the rest of the d3d9 logger.
const TEX_TRACE_TARGET: &str = "mtld3d::d3d9::tex";

/// The vtable every device carries, extended or not.
///
/// A plain device answers `E_NOINTERFACE` for `IID_IDirect3DDevice9Ex`, so a
/// caller that follows COM never reaches past the base slots on one.
static DIRECT3D_DEVICE9_VTBL: IDirect3DDevice9ExVtbl = IDirect3DDevice9ExVtbl {
    base: IDirect3DDevice9Vtbl {
        query_interface: device_query_interface,
        add_ref: device_add_ref,
        release: device_release,
        test_cooperative_level: device_test_cooperative_level,
        get_available_texture_mem: device_get_available_texture_mem,
        evict_managed_resources: device_evict_managed_resources,
        get_direct3d: device_get_direct3d,
        get_device_caps: device_get_device_caps,
        get_display_mode: device_get_display_mode,
        get_creation_parameters: device_get_creation_parameters,
        set_cursor_properties: cursor::device_set_cursor_properties,
        set_cursor_position: cursor::device_set_cursor_position,
        show_cursor: cursor::device_show_cursor,
        create_additional_swap_chain: device_create_additional_swap_chain,
        get_swap_chain: device_get_swap_chain,
        get_number_of_swap_chains: device_get_number_of_swap_chains,
        reset: device_reset,
        present: device_present,
        get_back_buffer: device_get_back_buffer,
        get_raster_status: device_get_raster_status,
        set_dialog_box_mode: device_set_dialog_box_mode,
        set_gamma_ramp: device_set_gamma_ramp,
        get_gamma_ramp: device_get_gamma_ramp,
        create_texture: device_create_texture,
        create_volume_texture: device_create_volume_texture,
        create_cube_texture: device_create_cube_texture,
        create_vertex_buffer: device_create_vertex_buffer,
        create_index_buffer: device_create_index_buffer,
        create_render_target: device_create_render_target,
        create_depth_stencil_surface: device_create_depth_stencil_surface,
        update_surface: device_update_surface,
        update_texture: device_update_texture,
        get_render_target_data: device_get_render_target_data,
        get_front_buffer_data: device_get_front_buffer_data,
        stretch_rect: device_stretch_rect,
        color_fill: device_color_fill,
        create_offscreen_plain_surface: device_create_offscreen_plain_surface,
        set_render_target: device_set_render_target,
        get_render_target: device_get_render_target,
        set_depth_stencil_surface: device_set_depth_stencil_surface,
        get_depth_stencil_surface: device_get_depth_stencil_surface,
        begin_scene: device_begin_scene,
        end_scene: device_end_scene,
        clear: device_clear,
        set_transform: device_set_transform,
        get_transform: device_get_transform,
        multiply_transform: device_multiply_transform,
        set_viewport: device_set_viewport,
        get_viewport: device_get_viewport,
        set_material: device_set_material,
        get_material: device_get_material,
        set_light: device_set_light,
        get_light: device_get_light,
        light_enable: device_light_enable,
        get_light_enable: device_get_light_enable,
        set_clip_plane: device_set_clip_plane,
        get_clip_plane: device_get_clip_plane,
        set_render_state: device_set_render_state,
        get_render_state: device_get_render_state,
        create_state_block: device_create_state_block,
        begin_state_block: device_begin_state_block,
        end_state_block: device_end_state_block,
        set_clip_status: device_set_clip_status,
        get_clip_status: device_get_clip_status,
        get_texture: device_get_texture,
        set_texture: device_set_texture,
        get_texture_stage_state: device_get_texture_stage_state,
        set_texture_stage_state: device_set_texture_stage_state,
        get_sampler_state: device_get_sampler_state,
        set_sampler_state: device_set_sampler_state,
        validate_device: device_validate_device,
        set_palette_entries: device_set_palette_entries,
        get_palette_entries: device_get_palette_entries,
        set_current_texture_palette: device_set_current_texture_palette,
        get_current_texture_palette: device_get_current_texture_palette,
        set_scissor_rect: device_set_scissor_rect,
        get_scissor_rect: device_get_scissor_rect,
        set_software_vertex_processing: device_set_software_vertex_processing,
        get_software_vertex_processing: device_get_software_vertex_processing,
        set_npatch_mode: device_set_npatch_mode,
        get_npatch_mode: device_get_npatch_mode,
        draw_primitive: device_draw_primitive,
        draw_indexed_primitive: device_draw_indexed_primitive,
        draw_primitive_up: device_draw_primitive_up,
        draw_indexed_primitive_up: device_draw_indexed_primitive_up,
        process_vertices: device_process_vertices,
        create_vertex_declaration: device_create_vertex_declaration,
        set_vertex_declaration: device_set_vertex_declaration,
        get_vertex_declaration: device_get_vertex_declaration,
        set_fvf: device_set_fvf,
        get_fvf: device_get_fvf,
        create_vertex_shader: device_create_vertex_shader,
        set_vertex_shader: device_set_vertex_shader,
        get_vertex_shader: device_get_vertex_shader,
        set_vertex_shader_constant_f: device_set_vertex_shader_constant_f,
        get_vertex_shader_constant_f: device_get_vertex_shader_constant_f,
        set_vertex_shader_constant_i: device_set_vertex_shader_constant_i,
        get_vertex_shader_constant_i: device_get_vertex_shader_constant_i,
        set_vertex_shader_constant_b: device_set_vertex_shader_constant_b,
        get_vertex_shader_constant_b: device_get_vertex_shader_constant_b,
        set_stream_source: device_set_stream_source,
        get_stream_source: device_get_stream_source,
        set_stream_source_freq: device_set_stream_source_freq,
        get_stream_source_freq: device_get_stream_source_freq,
        set_indices: device_set_indices,
        get_indices: device_get_indices,
        create_pixel_shader: device_create_pixel_shader,
        set_pixel_shader: device_set_pixel_shader,
        get_pixel_shader: device_get_pixel_shader,
        set_pixel_shader_constant_f: device_set_pixel_shader_constant_f,
        get_pixel_shader_constant_f: device_get_pixel_shader_constant_f,
        set_pixel_shader_constant_i: device_set_pixel_shader_constant_i,
        get_pixel_shader_constant_i: device_get_pixel_shader_constant_i,
        set_pixel_shader_constant_b: device_set_pixel_shader_constant_b,
        get_pixel_shader_constant_b: device_get_pixel_shader_constant_b,
        draw_rect_patch: device_draw_rect_patch,
        draw_tri_patch: device_draw_tri_patch,
        delete_patch: device_delete_patch,
        create_query: device_create_query,
    },
    set_convolution_mono_kernel: extended::set_convolution_mono_kernel,
    compose_rects: extended::compose_rects,
    present_ex: extended::present_ex,
    get_gpu_thread_priority: extended::get_gpu_thread_priority,
    set_gpu_thread_priority: extended::set_gpu_thread_priority,
    wait_for_vblank: extended::wait_for_vblank,
    check_resource_residency: extended::check_resource_residency,
    set_maximum_frame_latency: extended::set_maximum_frame_latency,
    get_maximum_frame_latency: extended::get_maximum_frame_latency,
    check_device_state: extended::check_device_state,
    create_render_target_ex: extended::create_render_target_ex,
    create_offscreen_plain_surface_ex: extended::create_offscreen_plain_surface_ex,
    create_depth_stencil_surface_ex: extended::create_depth_stencil_surface_ex,
    reset_ex: extended::reset_ex,
    get_display_mode_ex: extended::get_display_mode_ex,
};

/// Number of user-clip-plane storage slots: `D3DCAPS9::MaxUserClipPlanes`.
///
/// D3D9 aliases every index at or past `MaxUserClipPlanes - 1` onto the last
/// slot, for `SetClipPlane` and `GetClipPlane` alike (Wine's
/// `test_clip_planes_limits` probes `0..2*D3DMAXUSERCLIPPLANES`), so the store
/// is exactly the planes the GPU can apply.
const CLIP_PLANE_SLOTS: usize = mtld3d_core::vs_draw::MAX_CLIP_PLANES;

// ── DeviceInner — non-repr(C) state behind the inner pointer ──

bitflags::bitflags! {
    /// Assorted per-device boolean state.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct DeviceFlags: u8 {
        /// Set after the app called `SetDepthStencilSurface(NULL)` — depth is explicitly absent.
        ///
        /// Distinguishes "no override, use the auto depth" from "explicitly
        /// unbound" so the pipeline's depth/stencil-format snapshot matches
        /// the actual render-pass attachment. Cleared by `reset_to_defaults`
        /// (which restores the default bindings).
        const DEPTH_EXPLICITLY_UNBOUND = 1 << 0;
        /// Set between a successful `BeginScene` and its `EndScene`.
        ///
        /// D3D9 pairs them strictly: `BeginScene` while already in a scene and
        /// `EndScene` without an open scene both return `D3DERR_INVALIDCALL`.
        /// Rendering does not otherwise depend on scene state.
        const IN_SCENE = 1 << 1;
        /// Set when the implicit presentation resources could not be rebuilt.
        ///
        /// `Present` and `TestCooperativeLevel` report
        /// `D3DERR_DEVICENOTRESET` until a later `Reset` succeeds, which is
        /// how an app learns it must retry (after releasing any
        /// `D3DPOOL_DEFAULT` resources that blocked it).
        const NOT_RESET = 1 << 2;
        /// Set once the last `Release` has begun tearing the device down.
        ///
        /// The teardown hands a fullscreen window back before the cursor
        /// subclass comes off, and the restore's `WM_SIZE` reaches the
        /// subclass with the device no longer fullscreen. Answering it
        /// would rebuild a back buffer whose handles the teardown has
        /// already captured and is about to release, so the resize path
        /// checks this and stands down.
        const RELEASING = 1 << 3;
        /// Independent coverage request controlled only by A2M1/A2M0.
        const A2M_ENABLED = 1 << 4;
        /// Created by an interface `Direct3DCreate9Ex` made: a `D3D9Ex` device.
        ///
        /// Seeded at creation and never cleared, `reset_to_defaults` included.
        /// It refuses `D3DPOOL_MANAGED`, reads a system-memory `pSharedHandle`
        /// as user memory, keeps its state and its default-pool resources
        /// across `Reset`, and answers `IID_IDirect3DDevice9Ex`.
        const EXTENDED = 1 << 5;
        /// Set on an extended device once a failed `Reset` has warned, until one succeeds.
        ///
        /// A plain device's rejected or failed `Reset` leaves it waiting for
        /// another (`NOT_RESET`), which is what keeps a game's per-frame
        /// retries at debug. An extended device's rejected `Reset` changes
        /// nothing and owes nothing, so this is what quiets its retries; the
        /// next success clears it and says so at info.
        const RESET_FAILURE_LOGGED = 1 << 6;
        /// Set at device creation for `debug.failNextSubmit`; consumed on the first Present.
        const FAIL_NEXT_SUBMIT = 1 << 7;
    }
}

pub struct DeviceInner {
    depth_stencil_format: u32,
    /// `D3DMULTISAMPLE_TYPE` the swap chain was created with.
    ///
    /// Reported by `GetDesc` on the implicit back buffer and depth-stencil
    /// surfaces, and by the implicit swap chain's present parameters. Also
    /// decides whether `D3DRS_MULTISAMPLEMASK` applies: D3D9 defines the mask
    /// only for the maskable levels.
    backbuffer_multi_sample_type: u32,
    /// `MultiSampleQuality` the swap chain was created with.
    backbuffer_multi_sample_quality: u32,
    /// Sample count the back buffer and the implicit depth surface carry, 1 for none.
    backbuffer_sample_count: u8,
    /// Scene / depth-binding boolean state (`DEPTH_EXPLICITLY_UNBOUND` / `IN_SCENE`).
    ///
    /// See [`DeviceFlags`].
    flags: DeviceFlags,
    /// Window state saved when this device took its window fullscreen.
    ///
    /// `Some` exactly while the device is fullscreen. Restored (and cleared)
    /// by a windowed `Reset` or by device destruction.
    fullscreen: Option<crate::fullscreen::SavedWindow>,
    backbuffer_width: u32,
    backbuffer_height: u32,
    /// Fraction of the logical back buffer that is actually rasterized.
    ///
    /// `backbuffer_width`/`backbuffer_height` above stay logical — the size
    /// D3D9 reports and the space every game-supplied rect lives in. This
    /// converts those to the Metal texture's own resolution at the point a
    /// value becomes a Metal command. Identity unless `render.scale` is set.
    render_scale: mtld3d_core::render_scale::RenderScale,
    /// Currently-bound vertex declaration (null = none).
    ///
    /// Uses the `Bound` ownership marker — swaps bump the wrapper's
    /// `private_refcount` inline rather than going through the COM vtable's
    /// `AddRef`/`Release` thunks. It also carries the FVF: `SetFVF` binds
    /// the declaration it built for the FVF, which reports that FVF, so the
    /// most recent of `SetFVF` and `SetVertexDeclaration` wins and no other
    /// path can set one without the other.
    vertex_decl: CachedComPtr<Direct3DVertexDeclaration9, Bound>,
    /// Implicit vertex declarations synthesised by `SetFVF`, keyed by FVF.
    ///
    /// D3D9 converts a non-zero FVF into a declaration that
    /// `GetVertexDeclaration` returns; the same FVF always maps to the same
    /// cached object. Each entry is held by one `Bound` (private) refcount so
    /// the public refcount a game observes via `GetVertexDeclaration` reflects
    /// only its own `AddRef`s. Released when `DeviceInner` drops (`HashMap`
    /// value `Drop` runs `K::on_drop`).
    fvf_decl_cache: rustc_hash::FxHashMap<u32, CachedComPtr<Direct3DVertexDeclaration9, Bound>>,
    /// The FVF `bind_fvf_decl` last bound, with its implicit declaration from `fvf_decl_cache`.
    ///
    /// Only compared against the bound pointer, never dereferenced. The cache
    /// holds every entry until the device drops, so no other declaration can
    /// take this address while the pair names it.
    last_fvf_bind: (u32, *mut Direct3DVertexDeclaration9),
    /// `IDirect3D9`* that created this device.
    ///
    /// Kept so `GetDirect3D` can hand back the parent interface (with
    /// `AddRef`) instead of the `D3DERR_INVALIDCALL` any D3D9 title would
    /// treat as a fatal init failure.
    direct3d: u64,
    /// The owning `Direct3DDevice9`* COM wrapper.
    ///
    /// Stamped after the wrapper is boxed in `CreateDevice`. Stored as `u64`
    /// to mirror `direct3d` and leave `DeviceInner`'s auto-traits unchanged.
    /// Resource `GetDevice` thunks return it (`AddRef`'d) so callers — e.g.
    /// the conformance readback path — get a usable device pointer instead of
    /// an uninitialised out-param.
    device_wrapper: u64,
    /// Saved creation parameters, served verbatim by `GetCreationParameters`.
    creation_adapter: u32,
    creation_device_type: u32,
    creation_behavior_flags: u32,
    creation_focus_window: usize,
    /// The device's entry-point lock, `Some` under `D3DCREATE_MULTITHREADED`.
    ///
    /// Leaked at creation rather than owned here: a child `Release` can drive
    /// the device to zero and free this struct while the guard the child's
    /// thunk took is still live, and that guard releases through the lock on
    /// its way out, and a texture keeps entering it after the device is gone
    /// ([`Self::api_lock_ptr`]). One small allocation per flagged device
    /// creation. `None` is the whole of "not multithreaded": every thunk then
    /// takes the no-op guard.
    api_lock: Option<&'static ApiLock>,
    /// Normalised present parameters the implicit swapchain reports.
    ///
    /// Served through `GetSwapChain(0)` / `GetPresentParameters`: dimensions
    /// resolved, back-buffer count clamped to >= 1. Refreshed on `Reset`.
    /// `windowed` also gates `CreateAdditionalSwapChain` (no additional
    /// swapchains while the device is fullscreen).
    present_params: D3DPRESENT_PARAMETERS,
    /// Lazily-created implicit swapchain handed out by `GetSwapChain(0)`.
    ///
    /// Stored as `u64` (like [`direct3d`](Self::direct3d)/`device_wrapper`) so
    /// a raw pointer doesn't change `DeviceInner`'s auto-traits. The device
    /// owns it (the app may keep using it after its own `Release`), so the
    /// shell is leaked at teardown like the device wrapper. `0` until the
    /// first `GetSwapChain`.
    implicit_swapchain: u64,

    /// Lazily-created device-owned implicit render target == backbuffer surface.
    ///
    /// `GetRenderTarget(0)` / `GetBackBuffer(0)` / implicit
    /// `GetSwapChain(0).GetBackBuffer(0)` all return this one object. Stored as
    /// `u64` like [`implicit_swapchain`](Self::implicit_swapchain); created at
    /// refcount 0, finalized at device teardown. `0` until first requested.
    implicit_render_target: u64,

    /// Lazily-created device-owned implicit depth-stencil surface (`GetDepthStencilSurface`).
    ///
    /// Same lifecycle as [`implicit_render_target`](Self::implicit_render_target).
    /// `0` until first requested (and never created when the device has no auto
    /// depth-stencil).
    implicit_depth_stencil: u64,

    /// Cached resolved state for consecutive core draws.
    core_draw_cache: core_draw::DrawCache,
    /// The core runtime's device.
    core: Box<crate::rt::device::CoreDevice>,
    /// The auto depth-stencil's core texture id (DXMTRS C4b-3d2), `None` when made without one.
    core_auto_depth: Option<mtld3d_core::ids::TextureId>,
    /// Most recent texture-backed depth surface and mip size for `depth.aliasSameSize`.
    last_sized_depth: Option<(crate::rt::device::DepthRef, u32, u32)>,
    /// Interned shader token arrays by id, so draws compare programs by pointer.
    programs: rustc_hash::FxHashMap<ProgramId, std::sync::Weak<[u32]>>,
    /// Core texture IDs that migrated to another device, drained on the owning device's API thread.
    departed_textures: Mutex<Vec<TextureId>>,
    /// First latched device failure (`D3D_OK` until a failure).
    core_failure: AtomicI32,
    /// Address-space watch state: this device's present-sampling counter and its watch thread.
    ///
    /// See `mem_watch::MemWatchState`; the thresholds it reports against stay
    /// process-wide because the address space is. Dropping it waits for the
    /// thread, which a 32-bit build starts with the device.
    mem_watch: mem_watch::MemWatchState,
    /// Running total of bytes occupied by live `D3DPOOL_DEFAULT` resources.
    ///
    /// `GetAvailableTextureMem` reports `VRAM_BUDGET - this`, so the value
    /// visibly decreases as the app allocates GPU resources.
    vram_bytes_used: Arc<AtomicU64>,
    /// Number of `D3DPOOL_DEFAULT` resources and implicit surfaces the app holds a reference to.
    ///
    /// Maintained by the COM engine on each such object's public 0↔1
    /// refcount edge (`ComChild::blocks_reset_while_referenced`); the
    /// device's own bind slots never count. `Reset` fails with
    /// `D3DERR_INVALIDCALL` while it is non-zero, as D3D9 requires.
    outstanding_reset_blockers: AtomicU32,
    /// The frame-latency bound `Present` enforces, and what `GetMaximumFrameLatency` reports.
    frame_latency: mtld3d_core::frame_latency::FrameLatency,
    /// All API-thread telemetry.
    perf: ApiPerfStorage,
    /// The configuration of the `IDirect3D9` that created this device.
    config: Arc<Mtld3dConfig>,
    render_states: [u32; RENDER_STATE_COUNT],
    /// Last numeric POINTSIZE, preserved across A2M and RESZ controls.
    point_size: u32,
    /// Per-slot "have we warned about this unsupported RS write yet?" latch.
    rs_warn_fired: [u64; RENDER_STATE_COUNT.div_ceil(64)],
    ff_state: FfState,
    /// `SetScissorRect`'s rect as the game wrote it, which `GetScissorRect` hands back.
    scissor_rect: D3DRECT,
    /// Viewport set by `SetViewport`, served back by `GetViewport`.
    viewport: D3DVIEWPORT9,
    /// User clip planes set by `SetClipPlane`, served back by `GetClipPlane`.
    clip_planes: [[f32; 4]; CLIP_PLANE_SLOTS],
    /// Per-draw vertex uniform builder (point state, cached inverse view, clip planes).
    vs_draw: mtld3d_core::vs_draw::VsDrawState,
    /// Fixed-function vertex layout of the most recent core draw.
    cached_ff_vs_layout: mtld3d_core::convert::FfVsLayout,

    // Submodule-owned state. Each group's fields are private to its own
    // submodule; only `group()` / `group_mut()` cross the boundary.
    cursor: CursorState,
    bound_rt: BoundRt,
    bound_buffers: BoundBuffers,
    shader_bindings: ShaderBindings,
    stage_bindings: StageBindings,
    /// Vertex texture fetch slots (`D3DVERTEXTEXTURESAMPLER0..3`).
    vertex_textures: [CachedComPtr<crate::texture::Direct3DTexture9, Bound>; 4],
    /// Sampler state for the vertex slots, for `GetSamplerState`.
    vertex_sampler_states: [[u32; SAMPLER_STATE_COUNT]; 4],
    /// In-progress `BeginStateBlock` recording.
    recording_state_block: Option<Box<RecordingStateBlock>>,
    /// The gamma ramp the application last set, identity until it sets one.
    ///
    /// Kept verbatim so `GetGammaRamp` answers with what `SetGammaRamp` was
    /// given: D3D9 stores the ramp on the swap chain and reports it back
    /// unchanged.
    gamma_ramp: Box<D3DGAMMARAMP>,
    /// Texture id of the autogen render-target bound at each slot, if any.
    ///
    /// When a slot changes away from it the mip chain is regenerated.
    cur_autogen_rt_ids: [Option<TextureId>; RENDER_TARGET_SLOTS],
    /// Live `IDirect3DTexture9` objects.
    live_textures: Mutex<rustc_hash::FxHashMap<TextureId, *mut TextureInner>>,
}

/// Per-RS-index dirty mask.
///
/// The `SetRenderState` thunk feeds `rs_dirty_mask(index)` to
/// `mark_snapshot_dirty` after the `set_render_state` mutator updates the
/// slot (the mutator itself only marks the FF-VS const rows a write touches).
/// Most RS only flip the `RS` bit; the ones listed below also dirty derived
/// snapshot pieces:
///
/// - alpha test/ref → `ALPHA_REF` (bytes), `VARIANT` (`alpha_func` bit),
///   `PS_SOURCE` (PS variant key changes)
/// - fog enable/mode → `FOG_COLOR`, `VARIANT`, `VS_SOURCE`, `VS_CONST`,
///   `PS_SOURCE` (FF VS reads fog table mode + variant)
/// - fog color → `FOG_COLOR` only
/// - fog start/end/density → `FOG_COLOR`, `VS_CONST` (FF VS computes
///   vertex fog)
/// - texture factor → `PS_CONST` (FF PS reads it)
/// - lighting / material source / vertex blend → `VS_SOURCE`,
///   `VS_CONST`, `PS_SOURCE` (FF VS/PS keys + builder read these)
pub const fn rs_dirty_mask(state: u32) -> SnapshotDirty {
    let rs = SnapshotDirty::RS;
    match state {
        D3DRS_ALPHAREF => rs.union(SnapshotDirty::ALPHA_REF),
        D3DRS_ALPHAFUNC | D3DRS_ALPHATESTENABLE => rs
            .union(SnapshotDirty::ALPHA_REF)
            .union(SnapshotDirty::VARIANT)
            .union(SnapshotDirty::PS_SOURCE),
        // DEPTHBIAS rides the fog_data params row (table fog sources the
        // post-bias fragment depth), so the slot-13 bytes must rebuild — the
        // same `FOG_COLOR` dirty set as an explicit fog-colour change.
        D3DRS_FOGCOLOR | D3DRS_DEPTHBIAS => rs.union(SnapshotDirty::FOG_COLOR),
        D3DRS_FOGENABLE | D3DRS_FOGTABLEMODE | D3DRS_FOGVERTEXMODE => rs
            .union(SnapshotDirty::FOG_COLOR)
            .union(SnapshotDirty::VARIANT)
            .union(SnapshotDirty::VS_SOURCE)
            .union(SnapshotDirty::VS_CONST)
            .union(SnapshotDirty::PS_SOURCE),
        D3DRS_FOGSTART | D3DRS_FOGEND | D3DRS_FOGDENSITY => rs
            .union(SnapshotDirty::FOG_COLOR)
            .union(SnapshotDirty::VS_CONST),
        D3DRS_TEXTUREFACTOR => rs.union(SnapshotDirty::PS_CONST),
        // FLAT vs GOURAUD flips the PS `[[flat]]` varying qualifier (VariantKey
        // flat_shade), SRGBWRITEENABLE carries `D3DRS_SRGBWRITEENABLE` to the
        // encoder (VariantKey srgb_write) and POINTSPRITEENABLE flips
        // VariantFlags::POINT_SPRITE; all rebuild the PS source + variant key.
        D3DRS_SHADEMODE | D3DRS_SRGBWRITEENABLE | D3DRS_POINTSPRITEENABLE => rs
            .union(SnapshotDirty::VARIANT)
            .union(SnapshotDirty::PS_SOURCE),
        D3DRS_LIGHTING
        | D3DRS_AMBIENT
        | D3DRS_AMBIENTMATERIALSOURCE
        | D3DRS_DIFFUSEMATERIALSOURCE
        | D3DRS_SPECULARMATERIALSOURCE
        | D3DRS_EMISSIVEMATERIALSOURCE
        | D3DRS_SPECULARENABLE
        | D3DRS_NORMALIZENORMALS
        | D3DRS_COLORVERTEX
        | D3DRS_RANGEFOGENABLE
        | D3DRS_VERTEXBLEND
        | D3DRS_INDEXEDVERTEXBLENDENABLE => rs
            .union(SnapshotDirty::VS_SOURCE)
            .union(SnapshotDirty::VS_CONST)
            .union(SnapshotDirty::PS_SOURCE),
        // Key-only: LOCALVIEWER flips FfVsFlags::LOCAL_VIEWER (specular
        // view-vector model) and POINTSCALEENABLE flips
        // FfVsFlags::POINT_SCALE (eye-distance attenuation); no constant
        // section reads either.
        D3DRS_LOCALVIEWER | D3DRS_POINTSCALEENABLE => rs.union(SnapshotDirty::VS_SOURCE),
        // The point size, its clamp and the scale factors travel in the
        // per-draw VsDraw uniform every vertex shader reads.
        D3DRS_POINTSIZE | D3DRS_POINTSIZE_MIN | D3DRS_POINTSIZE_MAX | D3DRS_POINTSCALE_A
        | D3DRS_POINTSCALE_B | D3DRS_POINTSCALE_C => rs.union(SnapshotDirty::VS_DRAW),
        // The enabled-plane set repacks the VsDraw clip rows and its count
        // keys both vertex-shader sources (the programmable side is added
        // past `ff_aware_mask` by the SetRenderState thunk).
        D3DRS_CLIPPLANEENABLE | D3DRS_CLIPPING => rs
            .union(SnapshotDirty::VS_DRAW)
            .union(SnapshotDirty::VS_SOURCE),
        _ => rs,
    }
}

bitflags::bitflags! {
    /// Draw-cache dirty bits set by state mutators.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SnapshotDirty: u32 {
        const RS          = 1 << 0;
        const STAGES      = 1 << 1;
        const RT_DS       = 1 << 3;
        const VDECL       = 1 << 4;
        const VARIANT     = 1 << 5;
        const VS_SOURCE   = 1 << 6;
        const PS_SOURCE   = 1 << 7;
        const VS_CONST    = 1 << 8;
        const PS_CONST    = 1 << 9;
        const ALPHA_REF   = 1 << 10;
        const FOG_COLOR   = 1 << 11;
        const BUMP_ENV    = 1 << 12;
        const VS_CONST_I  = 1 << 13;
        const VS_DRAW     = 1 << 14;
        const VS_CONST_B  = 1 << 15;
        const PS_CONST_I  = 1 << 16;
        const PS_CONST_B  = 1 << 17;
    }
}

impl DeviceInner {
    /// Read the implicit front buffer into a validated caller-owned surface.
    ///
    /// Both front-buffer entry points use the persistent backbuffer and the
    /// same extent/format policy. The caller holds this device's API lock. On a
    /// core device the core reads the back buffer as Present would show it now,
    /// a multisampled one resolved first (DXMTRS C4c-3a).
    pub fn read_front_buffer(&mut self, dst_surf: &Direct3DSurface9) -> i32 {
        let Some(dst_desc) = dst_surf.system_memory_blit_dst() else {
            warn_readback_rejected("GetFrontBufferData", ReadbackReject::NotSystemMemory);
            return D3DERR_INVALIDCALL;
        };
        // Front-buffer reads are approximated by the persistent back-buffer
        // texture, whose extent and layout the destination is measured against.
        let source = ReadbackSource {
            width: self.backbuffer_width,
            height: self.backbuffer_height,
            format: mtld3d_core::format::backbuffer_surface_format(
                self.present_params.back_buffer_format,
            ),
        };
        if reject_readback("GetFrontBufferData", &source, &dst_desc).is_some() {
            return D3DERR_INVALIDCALL;
        }
        if let Err(hr) = self.encoder_status() {
            return hr;
        }
        let whole = DirtyRect::full(source.width, source.height);
        let read = core_arms::core_read_pages(&dst_desc, |bytes, pitch| {
            self.core.read_presented(whole, bytes, pitch)
        });
        if let Err(hr) = self.encoder_status() {
            return hr;
        }
        if !read {
            error!(target: LOG_TARGET,
                "d3d9 core: GetFrontBufferData did not read the back buffer → INVALIDCALL");
            return D3DERR_INVALIDCALL;
        }
        dst_surf.note_system_memory_written();
        D3D_OK
    }

    /// The scale a game-created render target or depth-stencil of this size inherits.
    ///
    /// A surface created at exactly the resolution D3D9 reports for the back
    /// buffer is the game's main view: either it renders the scene there and
    /// blits to the back buffer, or it is the depth buffer paired with it.
    /// Either way it belongs to the same image, so it scales with it — which is
    /// both what makes `render.scale` save anything for a game that does not
    /// draw straight to the back buffer, and what keeps a colour/depth pair the
    /// same size once one of them shrinks.
    ///
    /// Anything sized differently is an intermediate the game picked a
    /// resolution for (a shadow map, a glow chain, a fixed-size scratch
    /// target); its coordinates are its own and must pass through untouched.
    ///
    /// Only render targets and depth-stencils qualify. A surface the game
    /// uploads pixels into has a CPU-side layout that must keep matching what
    /// D3D9 reports, so its caller passes `false`. A lockable render target is
    /// not one of those: it is still bound and drawn into, so it takes the
    /// scale and its staging resamples past its texture, the way the lockable
    /// back buffer's does.
    pub const fn scale_for_created_target(
        &self,
        width: u32,
        height: u32,
        is_target: bool,
    ) -> mtld3d_core::render_scale::RenderScale {
        if is_target && width == self.backbuffer_width && height == self.backbuffer_height {
            self.render_scale
        } else {
            mtld3d_core::render_scale::RenderScale::IDENTITY
        }
    }

    pub const fn scissor_rect(&self) -> D3DRECT {
        self.scissor_rect
    }

    pub const fn set_scissor_rect(&mut self, r: D3DRECT) {
        self.scissor_rect = r;
    }

    pub const fn viewport(&self) -> D3DVIEWPORT9 {
        self.viewport
    }

    pub const fn set_viewport(&mut self, v: D3DVIEWPORT9) {
        self.viewport = v;
        self.invalidate_core_draw();
    }

    /// Store a user clip plane set via `SetClipPlane`.
    ///
    /// The first `vs_draw::MAX_CLIP_PLANES` feed the per-draw `VsDraw`
    /// uniform when `D3DRS_CLIPPLANEENABLE` names them, so the uniform is
    /// marked for a rebuild. The index is clamped into range so an
    /// out-of-range plane aliases the last slot rather than panicking.
    pub fn set_clip_plane(&mut self, index: u32, plane: [f32; 4]) {
        let slot = (index as usize).min(CLIP_PLANE_SLOTS - 1);
        self.clip_planes[slot] = plane;
        self.mark_snapshot_dirty(SnapshotDirty::VS_DRAW);
    }

    /// Read back a user clip plane for `GetClipPlane`.
    ///
    /// Unset slots read back the zero-initialised default.
    pub fn clip_plane(&self, index: u32) -> [f32; 4] {
        let slot = (index as usize).min(CLIP_PLANE_SLOTS - 1);
        self.clip_planes[slot]
    }

    pub const fn ff_state(&self) -> &FfState {
        &self.ff_state
    }

    pub const fn ff_state_mut(&mut self) -> &mut FfState {
        &mut self.ff_state
    }

    pub const fn cursor(&self) -> &CursorState {
        &self.cursor
    }

    pub const fn cursor_mut(&mut self) -> &mut CursorState {
        &mut self.cursor
    }

    pub const fn bound_rt(&self) -> &BoundRt {
        &self.bound_rt
    }

    pub const fn bound_rt_mut(&mut self) -> &mut BoundRt {
        &mut self.bound_rt
    }

    pub const fn bound_buffers(&self) -> &BoundBuffers {
        &self.bound_buffers
    }

    pub const fn bound_buffers_mut(&mut self) -> &mut BoundBuffers {
        &mut self.bound_buffers
    }

    pub const fn shader_bindings(&self) -> &ShaderBindings {
        &self.shader_bindings
    }

    pub const fn shader_bindings_mut(&mut self) -> &mut ShaderBindings {
        &mut self.shader_bindings
    }

    fn uses_automatic_fog(&self) -> bool {
        let shader = self.shader_bindings().pixel_shader();
        shader.is_null() || {
            // SAFETY: the non-null shader is kept live by the device binding.
            unsafe { (*shader).uses_automatic_fog() }
        }
    }

    pub const fn vertex_decl(&self) -> *mut Direct3DVertexDeclaration9 {
        self.vertex_decl.raw()
    }

    /// Bind `new` as the current vertex declaration. Pass null to clear.
    ///
    /// `CachedComPtr::adopt` `AddRefs` the new pointer (no-op for null);
    /// assignment Drops the old slot, which Releases the previous refcount.
    ///
    /// Returns whether the bound pointer changed. A bound decl is kept
    /// alive by its slot refcount, so identical pointers mean the same
    /// immutable object (same elements/hash) — callers gate the
    /// expensive VDECL re-resolve on this.
    pub fn replace_vertex_decl(&mut self, new: *mut Direct3DVertexDeclaration9) -> bool {
        let changed = self.vertex_decl.raw() != new;
        // SAFETY: `new` is null or a live IDirect3DVertexDeclaration9 from
        // a Set* thunk / state-block apply; AddRef/Release thunks valid
        // for our lifetime.
        self.vertex_decl = unsafe { CachedComPtr::adopt(new) };
        if changed {
            // VDECL change can flip vs_key.has_rhw (row 0 layout switches
            // between XYZRHW viewport and WV transposed) and
            // vs_key.vertex_blend_count (gates PALETTE rows 95+).
            // `has_rhw` also feeds `lit = !has_rhw && D3DRS_LIGHTING != 0`,
            // so it indirectly changes MATERIAL/LIGHTS extent. Under
            // per-section emit, mark every section that depends on the
            // layout. TT is the one section unaffected (per-stage TTFF
            // gates it, not VDECL).
            self.ff_state.mark_ff_vs_dirty(
                mtld3d_core::ff_state::FfVsDirty::WV
                    | mtld3d_core::ff_state::FfVsDirty::PROJ
                    | mtld3d_core::ff_state::FfVsDirty::FOG
                    | mtld3d_core::ff_state::FfVsDirty::AMBIENT
                    | mtld3d_core::ff_state::FfVsDirty::MATERIAL
                    | mtld3d_core::ff_state::FfVsDirty::LIGHTS
                    | mtld3d_core::ff_state::FfVsDirty::PALETTE,
            );
        }
        changed
    }

    /// Get (or lazily synthesise + cache) the implicit vertex declaration for a non-zero `fvf`.
    ///
    /// The returned pointer is borrowed: the cache keeps the object alive via
    /// a `Bound` (private) refcount, so the public refcount stays zero until a
    /// game `Get`s it. Returns null only if the FVF produced an unbindable
    /// element array (should not happen for a real FVF).
    fn get_or_create_fvf_decl(&mut self, fvf: u32) -> *mut Direct3DVertexDeclaration9 {
        if let Some(cached) = self.fvf_decl_cache.get(&fvf) {
            return cached.raw();
        }
        let (mut elements, _stride) = fvf_to_elements(fvf);
        elements.push(mtld3d_types::D3DDECL_END);
        let device_inner = std::ptr::from_mut::<Self>(self);
        let Some(decl) = Direct3DVertexDeclaration9::new(&VertexDeclCreateInfo {
            device_inner,
            elements: &elements,
            fvf,
        }) else {
            return core::ptr::null_mut();
        };
        let decl_ptr = Box::into_raw(Box::new(decl));
        // The wrapper is born with public refcount 1. Register its device
        // reference like any child, then hand that public reference to the cache
        // as a `Bound` (private) refcount and drop the public one — so the public
        // count reflects only the game's own GetVertexDeclaration AddRefs and
        // the device-forward stays balanced: the
        // register here is matched by the public release below (and re-acquired
        // on a game `Get`).
        // SAFETY: `decl_ptr` is a freshly created, live declaration at refcount 1.
        unsafe { crate::com_ref::com_register_child(decl_ptr) };
        // SAFETY: `decl_ptr` is a freshly-boxed, live wrapper.
        let cached = unsafe { CachedComPtr::<Direct3DVertexDeclaration9, Bound>::adopt(decl_ptr) };
        // SAFETY: `decl_ptr` is live and `vtbl()` returns its installed vtable.
        let release = unsafe { (*decl_ptr).vtbl().release };
        // SAFETY: `release` is the matching Release thunk; the `Bound` ref
        // adopted above keeps the wrapper alive as public drops 1 -> 0.
        unsafe { release(decl_ptr.cast::<c_void>()) };
        self.fvf_decl_cache.insert(fvf, cached);
        decl_ptr
    }

    /// Apply D3D9 `SetFVF` semantics for a non-zero `fvf`.
    ///
    /// Bind the FVF's implicit declaration, which carries the FVF, as the
    /// current vertex declaration (the most-recent of `SetFVF` /
    /// `SetVertexDeclaration` wins). `fvf == 0` is a no-op on the binding,
    /// matching the driver.
    /// Returns whether the bound declaration changed (callers gate snapshot
    /// dirtying on this), or `None` when the declaration cannot be built,
    /// leaving the binding as it was. A call that would change nothing
    /// returns before the cache lookup (see [`Self::fvf_bind_is_redundant`]).
    #[inline]
    pub fn bind_fvf_decl(&mut self, fvf: u32) -> Option<bool> {
        if fvf == 0 || self.fvf_bind_is_redundant(fvf) {
            return Some(false);
        }
        self.bind_fvf_decl_uncached(fvf)
    }

    /// Whether binding `fvf` would leave the bound declaration as it is.
    ///
    /// True exactly when the bound declaration is `fvf`'s implicit one,
    /// answered without the cache lookup or a dereference: the bound pointer
    /// is compared against the one `bind_fvf_decl` last bound, and the FVF
    /// against the one it bound it for (`last_fvf_bind`). Each FVF has one
    /// cached declaration, so a match on both halves is the declaration the
    /// lookup would produce, and every dirty mark the caller gates on a change
    /// would be skipped anyway. Any other path that binds a declaration (a
    /// `SetVertexDeclaration`, a state-block apply) moves the pointer and
    /// fails the first half.
    #[inline]
    fn fvf_bind_is_redundant(&self, fvf: u32) -> bool {
        let (bound_fvf, bound_decl) = self.last_fvf_bind;
        self.vertex_decl.raw() == bound_decl && bound_fvf == fvf
    }

    /// The [`Self::bind_fvf_decl`] path that looks the declaration up and binds it.
    #[inline(never)]
    fn bind_fvf_decl_uncached(&mut self, fvf: u32) -> Option<bool> {
        let decl = self.get_or_create_fvf_decl(fvf);
        if decl.is_null() {
            return None;
        }
        self.last_fvf_bind = (fvf, decl);
        Some(self.replace_vertex_decl(decl))
    }

    /// Release every vertex fetch slot's bound-texture refcount at device teardown.
    pub fn teardown_vertex_textures(&mut self) {
        self.vertex_textures = [const { CachedComPtr::null() }; 4];
    }

    /// One vertex-slot sampler state value, for `GetSamplerState`.
    #[must_use]
    pub const fn vertex_sampler_state(&self, slot: usize, type_: usize) -> u32 {
        self.vertex_sampler_states[slot][type_]
    }

    /// The texture bound at a vertex fetch slot, for `GetTexture` and core draws.
    #[must_use]
    pub const fn vertex_texture(&self, slot: usize) -> *mut crate::texture::Direct3DTexture9 {
        self.vertex_textures[slot].raw()
    }

    /// Store one vertex-slot sampler state and invalidate the draw cache.
    pub const fn set_vertex_sampler_slot_state(&mut self, slot: usize, type_: usize, value: u32) {
        self.vertex_sampler_states[slot][type_] = value;
        self.invalidate_core_draw();
    }

    /// Bind `tex` to vertex texture fetch slot `slot` (0..4).
    pub fn set_vertex_texture_slot(
        &mut self,
        slot: usize,
        tex: *mut crate::texture::Direct3DTexture9,
    ) {
        // SAFETY: `tex` is null or a live IDirect3DTexture9 supplied by the
        // calling D3D9 vtable thunk; AddRef/Release valid for our lifetime.
        self.vertex_textures[slot] = unsafe { CachedComPtr::adopt(tex) };
        self.clear_core_draw_cache();
    }

    /// Invalidate the draw cache when `SetLOD` updates a vertex-bound texture.
    pub fn refresh_vertex_texture_lod(&mut self, tex: *const crate::texture::Direct3DTexture9) {
        for slot in 0..self.vertex_textures.len() {
            if core::ptr::eq(self.vertex_textures[slot].raw(), tex) {
                self.invalidate_core_draw();
            }
        }
    }

    /// Whether `tex`'s format forces unfiltered point sampling on this device.
    pub fn samples_unfiltered(&self, tex: &Direct3DTexture9) -> bool {
        let float32_filtering =
            crate::direct3d9::float32_filtering_supported(self.config.deny_float32_filtering);
        let native_packed16 =
            crate::direct3d9::native_packed16_supported(self.config.expand_packed16);
        !mtld3d_core::format::supports_usage_query(
            tex.d3d_format(),
            D3DUSAGE_QUERY_FILTER,
            float32_filtering,
            native_packed16,
        )
    }

    /// Volume and cube texture masks across vertex sampler slots `0..4`.
    pub fn vertex_texture_kinds(&self) -> mtld3d_core::dxso::VsSamplerKinds {
        let mut kinds = mtld3d_core::dxso::VsSamplerKinds::default();
        for slot in 0..self.vertex_textures.len() {
            let tex = self.vertex_textures[slot].raw();
            if tex.is_null() {
                continue;
            }
            // SAFETY: `self.vertex_textures[slot]` holds a bound reference keeping `tex` live.
            let tex = unsafe { &*tex };
            let bit = 1u8 << slot;
            if tex.is_volume() {
                kinds.volume_mask |= bit;
            }
            if tex.is_cube() {
                kinds.cube_mask |= bit;
            }
        }
        kinds
    }

    pub const fn stage_bindings(&self) -> &StageBindings {
        &self.stage_bindings
    }

    pub const fn stage_bindings_mut(&mut self) -> &mut StageBindings {
        &mut self.stage_bindings
    }

    /// The FVF `GetFVF` reports: the bound declaration's, 0 with none bound.
    ///
    /// Only the declaration `SetFVF` built for an FVF carries one; a
    /// declaration the game created reports 0.
    pub fn fvf(&self) -> u32 {
        let decl = self.vertex_decl.raw();
        if decl.is_null() {
            return 0;
        }
        // SAFETY: non-null; the bound slot's refcount keeps the declaration live.
        unsafe { (*decl).inner().fvf() }
    }

    /// In-progress `BeginStateBlock` recording, if any.
    ///
    /// Every state-change COM vtable entry point checks this: when `Some`,
    /// the change is recorded into the block and the live device is left
    /// untouched.
    pub fn recording_state_block_mut(&mut self) -> Option<&mut RecordingStateBlock> {
        self.recording_state_block.as_deref_mut()
    }

    /// Clear the draw cache after a reset or state-block apply.
    pub fn mark_snapshot_dirty_all(&mut self) {
        self.clear_core_draw_cache();
    }

    /// Invalidate the draw cache on a live state change.
    pub const fn mark_snapshot_dirty(&mut self, _bits: SnapshotDirty) {
        self.invalidate_core_draw();
    }

    /// Strip FF-only dirty bits when the corresponding shader stage is programmable.
    pub fn ff_aware_mask(&self, mask: SnapshotDirty) -> SnapshotDirty {
        let mut result = mask;
        if !self.shader_bindings.vertex_shader().is_null() {
            result.remove(SnapshotDirty::VS_SOURCE | SnapshotDirty::VS_CONST);
        }
        if !self.shader_bindings.pixel_shader().is_null() {
            result.remove(SnapshotDirty::PS_SOURCE | SnapshotDirty::PS_CONST);
        }
        result
    }

    /// Dirty bits for a transform or light write to the fixed-function state.
    ///
    /// The write always changes the FF VS constants. It changes the FF VS
    /// source only when `source_moved`: a transform write never does, since
    /// a matrix reaches neither the key nor the row count, and a light thunk
    /// compares [`FfState::vs_source_light_inputs`] across the write, since a
    /// light's parameters alone never reach the key while a light enable or a
    /// light type change can. The result then goes through
    /// [`Self::ff_aware_mask`].
    fn ff_vs_write_mask(&self, source_moved: bool) -> SnapshotDirty {
        let mut mask = SnapshotDirty::VS_CONST;
        if source_moved {
            mask |= SnapshotDirty::VS_SOURCE;
        }
        self.ff_aware_mask(mask)
    }

    /// Dirty bits outside the FF VS source and constants that a write to transform `state` needs.
    ///
    /// `SetTransform` and `MultiplyTransform` both add these to
    /// [`Self::ff_vs_write_mask`], since each rewrites the matrix in place.
    /// Active table fog keys its Z-versus-W source on the projection matrix's
    /// 4th column (`VariantKey::fog_source_w`), so a PROJECTION write rebuilds
    /// the variant; the gate on live table fog keeps vertex-fog-only games
    /// from churning the variant on every per-frame projection update. The
    /// fixed-function clip planes walk back from eye space through the
    /// inverse view the `VsDraw` uniform carries, so a VIEW write rebuilds it.
    fn transform_write_side_dirty(&self, state: u32) -> SnapshotDirty {
        match state {
            mtld3d_types::D3DTS_PROJECTION
                if self.render_states[D3DRS_FOGENABLE as usize] != 0
                    && matches!(self.render_states[D3DRS_FOGTABLEMODE as usize], 1..=3) =>
            {
                SnapshotDirty::VARIANT | SnapshotDirty::PS_SOURCE
            }
            mtld3d_types::D3DTS_VIEW => SnapshotDirty::VS_DRAW,
            _ => SnapshotDirty::empty(),
        }
    }

    /// Start a new recording.
    ///
    /// `BeginStateBlock`-only path — returns `false` if a recording is
    /// already in progress (D3D9 spec reject).
    pub fn begin_state_block_recording(&mut self) -> bool {
        if self.recording_state_block.is_some() {
            return false;
        }
        self.recording_state_block = Some(Box::new(RecordingStateBlock::new()));
        true
    }

    /// Finish the in-progress recording and hand ownership back to the caller.
    ///
    /// Returns `None` if no recording was active.
    pub const fn end_state_block_recording(&mut self) -> Option<Box<RecordingStateBlock>> {
        self.recording_state_block.take()
    }

    /// True while a `BeginStateBlock` recording is open.
    ///
    /// D3D9 rejects `Apply`/`Capture`/`CreateStateBlock` with `INVALIDCALL`
    /// during recording.
    pub const fn is_state_block_recording(&self) -> bool {
        self.recording_state_block.is_some()
    }

    /// Reconstruct a `&mut DeviceInner` from the opaque `inner: u64` field.
    pub fn from_ptr(ptr: u64) -> &'static mut Self {
        // SAFETY: `ptr` is the `Direct3DDevice9::inner` field — a stable
        // `*mut DeviceInner` produced when the device wrapper was created.
        // The inner outlives the wrapper, so the borrow is valid.
        unsafe { &mut *(ptr as *mut Self) }
    }

    /// The configuration of the `IDirect3D9` that created this device.
    pub fn config(&self) -> &Mtld3dConfig {
        &self.config
    }

    /// Unbind render target `slot` (1..=3): `SetRenderTarget(slot, NULL)`.
    ///
    /// Regenerates the mip chain of an autogen texture that was bound there,
    /// drops the persistent binding, and updates the core target slot.
    fn unbind_extra_render_target(&mut self, slot: usize) {
        let old_autogen = self.cur_autogen_rt_ids[slot].take();
        if let Some(old_id) = old_autogen {
            self.core.texture_generate_mips(old_id);
        }
        let unbound = self.core.set_extra_target(slot, None);
        debug_assert!(unbound, "slot 1-3 takes an unbind");
        self.invalidate_core_draw();
        self.bound_rt_mut()
            .replace_render_target(slot, core::ptr::null_mut(), 0, 0);
    }

    /// Present the current back buffer and advance per-frame state.
    pub fn present(&mut self) -> i32 {
        if let Err(hr) = self.encoder_status() {
            return hr;
        }
        if self.needs_reset() {
            return mtld3d_types::D3DERR_DEVICENOTRESET;
        }
        if self.flags.contains(DeviceFlags::FAIL_NEXT_SUBMIT) {
            self.flags.remove(DeviceFlags::FAIL_NEXT_SUBMIT);
            error!(
                target: LOG_TARGET,
                "d3d9 core: debug.failNextSubmit refused the frame submission"
            );
            return self.record_core_failure(
                mtld3d_types::D3DERR_DEVICELOST,
                "debug.failNextSubmit refused the submission",
            );
        }
        self.drain_departed_textures();
        let pe_cs_reasons = self.cursor.sinks().take_client_surface_request().bits();
        let _ = core_arms::present(
            &mut self.core,
            &self.present_params,
            &self.config,
            pe_cs_reasons,
        );
        let cursor_scale = self.config().cursor_scale;
        let backing_scale = self.core.backing_scale();
        let cursor_kick = self.core.take_cursor_kick();
        if backing_scale != 0 {
            self.cursor_mut().sinks().set_backing_scale(backing_scale);
        }
        if cursor_kick {
            self.cursor_mut().sinks().set_cursor_kick();
        }
        self.cursor_mut().follow_published_scale(cursor_scale);
        self.cursor_mut().sync_native_visibility();
        self.cursor_mut().note_present();
        // Outside the frame submission. The address-space watch walks on its own
        // thread; this only hands it a sample and answers its request for figures.
        self.mem_watch_present();
        self.encoder_status()
            .map_or_else(|hr| hr, |()| mtld3d_types::D3D_OK)
    }

    pub fn perf_mut(&mut self) -> impl DerefMut<Target = ApiPerfState> + '_ {
        self.perf.state_mut()
    }

    /// Borrow the storage while constructing a timer that retains it.
    ///
    /// # Safety
    /// A non-null device must be live for the returned borrow, with its API
    /// lock held when multithreaded. No counter borrow may span the timer.
    pub unsafe fn perf_storage_of<'a>(dev: *mut Self) -> Option<&'a ApiPerfStorage> {
        // SAFETY: the caller guarantees the device's lifetime and API lock.
        unsafe { dev.as_ref() }.map(|dev| &dev.perf)
    }

    pub fn encoder_status(&self) -> Result<(), i32> {
        mtld3d_core::encoder_failure::known_status(&self.core_failure)?;
        if self.core.has_error() {
            return Err(self.record_core_failure(
                mtld3d_types::D3DERR_DEVICELOST,
                "a Metal command buffer completed with an error",
            ));
        }
        Ok(())
    }

    /// Stop a bound draw or non-error command after a failure the device has already latched.
    pub fn latched_encoder_status(&self) -> Result<(), i32> {
        mtld3d_core::encoder_failure::known_status(&self.core_failure)
    }

    fn record_core_failure(&self, status: i32, cause: &str) -> i32 {
        mtld3d_core::encoder_failure::record_failure(&self.core_failure, status, cause)
    }

    /// Register a freshly-created `TextureInner` in the live-texture registry.
    ///
    /// So `evict_managed_resources` can iterate live textures. The pointer
    /// is the same `Box::into_raw` result that backs the COM wrapper's
    /// `inner` field. Single-threaded API contract: lock contention is
    /// zero in steady state.
    pub fn register_texture(&self, ti: *mut TextureInner) {
        // SAFETY: `ti` is a freshly-built (or rehydrating) live `TextureInner`.
        let tex = unsafe { &*ti };
        if tex.is_default_pool() {
            self.vram_bytes_used
                .fetch_add(tex.allocated_bytes(), Ordering::AcqRel);
        }
        self.live_textures
            .lock()
            .expect("live_textures mutex poisoned")
            .insert(tex.texture_id(), ti);
    }

    /// Drop a `TextureInner` from the live-texture registry.
    ///
    /// Called from `texture_release`'s rc->0 path before the inner Box is
    /// freed, so the registry never holds a dangling pointer.
    pub fn deregister_texture(&self, ti: *mut TextureInner) {
        // SAFETY: deregistration runs before the live inner Box is freed or
        // transferred to another device; its texture id never changes.
        let tex = unsafe { &*ti };
        let mut live = self
            .live_textures
            .lock()
            .expect("live_textures mutex poisoned");
        if live.remove(&tex.texture_id()).is_some() {
            // Release the registry lock before the VRAM accounting below;
            // it touches only the atomic, not the texture registry.
            drop(live);
            if tex.is_default_pool() {
                self.vram_bytes_used
                    .fetch_sub(tex.allocated_bytes(), Ordering::AcqRel);
            }
        }
    }

    /// Record a texture ID that migrated away so its core copy can be dropped.
    pub fn push_departed_texture(&self, id: TextureId) {
        self.departed_textures
            .lock()
            .expect("departed_textures mutex poisoned")
            .push(id);
    }

    /// Cancel pending core cleanup when a departed texture returns to this device.
    pub fn remove_departed_texture(&self, id: TextureId) {
        let mut departed = self
            .departed_textures
            .lock()
            .expect("departed_textures mutex poisoned");
        departed.retain(|&entry| entry != id);
    }

    /// Drop core-side copies of textures that migrated to another device.
    pub fn drain_departed_textures(&mut self) {
        let departed = {
            let mut guard = self
                .departed_textures
                .lock()
                .expect("departed_textures mutex poisoned");
            if guard.is_empty() {
                return;
            }
            core::mem::take(&mut *guard)
        };
        for id in departed {
            self.core.forget_texture(id);
        }
    }

    /// `IDirect3DDevice9::EvictManagedResources` body.
    ///
    /// Walks the live-textures registry, marks every previously-uploaded mip
    /// of a `D3DPOOL_MANAGED` texture dirty (via `texture::evict_mark_dirty`),
    /// and pushes one `destroy_cached_texture` operation per affected texture.
    /// The next bind-time `flush_dirty_mips` repopulates fresh `MTLTextures`
    /// from the still-alive PE-side staging Arc, which is the spec contract
    /// "evict from VRAM, runtime re-uploads on next use". A texture of any
    /// other pool is filtered out by `evict_mark_dirty`, which is where the
    /// reason lives; its cache entry stays intact.
    ///
    /// A core device marks the same levels and destroys nothing: it keeps its GPU copies, and
    /// the marks alone re-publish each level at its texture's next bind (`core_flush_dirty`).
    pub fn evict_managed_resources(&mut self) {
        let live: Vec<*mut TextureInner> = self
            .live_textures
            .lock()
            .expect("live_textures mutex poisoned")
            .values()
            .copied()
            .collect();
        let mut to_evict: Vec<TextureId> = Vec::new();
        for ti_ptr in live {
            // SAFETY: `ti_ptr` is a snapshot from `live_textures`; entries
            // are removed on `TextureInner` drop, so the pointer is live
            // for the duration of this loop iteration.
            let ti = unsafe { &mut *ti_ptr };
            if let Some(tex_id) = crate::texture::evict_mark_dirty(ti) {
                to_evict.push(tex_id);
            }
        }
        self.clear_core_draw_cache();
        mtld3d_shared::log_once_info!(
            target: TEX_TRACE_TARGET,
            "EvictManagedResources: marked {} textures dirty (GPU copies kept)",
            to_evict.len()
        );
    }

    pub const fn render_state(&self, index: usize) -> u32 {
        self.render_states[index]
    }

    /// Returns whether raw or derived state changed.
    ///
    /// Callers gate `mark_snapshot_dirty` on this. POINTSIZE also compares
    /// its numeric value and coverage latch: a state-block restore can leave
    /// either different from the component a same-raw write would select.
    ///
    /// A write of the value the slot already holds returns here, before the
    /// call into [`Self::write_render_state`]. The silent-write audit has
    /// nothing to say about it: a stored value is one the device starts or
    /// resets with, all of them defaults or consumed, or it came through the
    /// audit, which latched the slot if the value is not consumed. Only the
    /// state trace lists such a write, so the early return is taken while
    /// the trace is off.
    #[inline]
    pub fn set_render_state(&mut self, index: usize, value: u32) -> bool {
        if index != D3DRS_POINTSIZE as usize
            && self.render_states[index] == value
            && !mtld3d_core::state_trace::enabled()
        {
            return false;
        }
        self.write_render_state(index, value)
    }

    /// The [`Self::set_render_state`] path that audits the write and stores it.
    ///
    /// Out of line so the setter's callers inline only the same-value test.
    #[inline(never)]
    fn write_render_state(&mut self, index: usize, value: u32) -> bool {
        if index == D3DRS_POINTSIZE as usize {
            return self.set_point_size_render_state(value);
        }
        self.warn_rs_non_default_once(index, value);
        let prev = self.render_states[index];
        self.render_states[index] = value;
        let changed = prev != value;
        if changed {
            // RS-driven FF VS const-buffer rows under per-section
            // emit. The encoder mirror has to catch each change before
            // the next FF draw reads from it. Two flavors here:
            //
            //   (a) RS values that supply the *contents* of a row.
            //       FOGSTART/END/DENSITY feed row 8; AMBIENT feeds row 9.
            //
            //   (b) RS values that change the *extent* the shader reads
            //       OR change whether a section is gated on/off. Because
            //       per-section emit writes only the section that changed,
            //       these must be marked explicitly — nothing else rewrites
            //       the full extent to cover them.
            //
            //       - FOGENABLE/VERTEXMODE/TABLEMODE flip vs_key.fog_mode,
            //         which toggles row 8 between zero-fill and the
            //         actual fog params.
            //       - LIGHTING flips vs_key.lighting_enabled, which
            //         changes both the MATERIAL extent (1 row unlit vs
            //         4-5 rows lit) and whether LIGHTS rows are read.
            //       - SPECULARENABLE flips vs_key.specular_enable,
            //         which adds row 14 (material.power) to the read
            //         extent.
            //       - VERTEXBLEND/INDEXEDVERTEXBLENDENABLE gates the
            //         palette section (rows 95+).
            //
            // Other RS writes don't feed the FF VS const buffer or
            // change which sections the shader reads.
            let bits = match u32::try_from(index).ok() {
                Some(
                    D3DRS_FOGSTART | D3DRS_FOGEND | D3DRS_FOGDENSITY | D3DRS_FOGENABLE
                    | D3DRS_FOGVERTEXMODE | D3DRS_FOGTABLEMODE,
                ) => FfVsDirty::FOG,
                Some(D3DRS_AMBIENT) => FfVsDirty::AMBIENT,
                Some(D3DRS_LIGHTING) => FfVsDirty::MATERIAL | FfVsDirty::LIGHTS,
                Some(D3DRS_SPECULARENABLE) => FfVsDirty::MATERIAL,
                Some(D3DRS_VERTEXBLEND | D3DRS_INDEXEDVERTEXBLENDENABLE) => FfVsDirty::PALETTE,
                _ => FfVsDirty::empty(),
            };
            if !bits.is_empty() {
                self.ff_state.mark_ff_vs_dirty(bits);
            }
        }
        changed
    }

    /// The POINTSIZE arm of [`Self::write_render_state`], which also decodes the A2M token.
    ///
    /// Out of line so the render-state write stays small.
    #[inline(never)]
    fn set_point_size_render_state(&mut self, value: u32) -> bool {
        self.set_point_size_state(
            value,
            mtld3d_core::multisample::numeric_point_size(value),
            mtld3d_core::multisample::a2m_control(value),
        )
    }

    pub const fn point_size(&self) -> u32 {
        self.point_size
    }

    pub const fn a2m_enabled(&self) -> bool {
        self.flags.contains(DeviceFlags::A2M_ENABLED)
    }

    /// Restore raw POINTSIZE and only the hidden components included by the caller.
    ///
    /// Recorded Capture retains membership separately from the raw value, so
    /// a refreshed numeric DWORD cannot erase a captured latch or vice versa.
    pub fn set_point_size_state(
        &mut self,
        raw: u32,
        numeric: Option<u32>,
        coverage: Option<bool>,
    ) -> bool {
        self.warn_rs_non_default_once(D3DRS_POINTSIZE as usize, raw);
        let mut changed = self.render_states[D3DRS_POINTSIZE as usize] != raw;
        self.render_states[D3DRS_POINTSIZE as usize] = raw;
        if let Some(size) = numeric {
            changed |= self.point_size != size;
            self.point_size = size;
        }
        if let Some(enabled) = coverage {
            changed |= self.a2m_enabled() != enabled;
            self.flags.set(DeviceFlags::A2M_ENABLED, enabled);
        }
        changed
    }

    /// Returns whether the once-per-slot RS warn latch for `index` has fired.
    const fn rs_warn_fired(&self, index: usize) -> bool {
        (self.rs_warn_fired[index / 64] & (1u64 << (index % 64))) != 0
    }

    /// Sets the once-per-slot RS warn latch for `index`.
    const fn mark_rs_warn(&mut self, index: usize) {
        self.rs_warn_fired[index / 64] |= 1u64 << (index % 64);
    }

    /// The silent-write audit of one render-state write.
    ///
    /// The tests stay inline, so the setter can inline this; the latch mark
    /// and every log line live in cold functions.
    #[inline]
    fn warn_rs_non_default_once(&mut self, index: usize, value: u32) {
        static RS_DEFAULTS: [u32; RENDER_STATE_COUNT] = render_state_defaults();

        if index >= RENDER_STATE_COUNT {
            return;
        }
        let default = RS_DEFAULTS[index];
        if value == default {
            if mtld3d_core::state_trace::enabled() {
                Self::trace_default_rs(index, value);
            }
            return;
        }
        if self.rs_warn_fired(index) {
            return;
        }
        let state = u32::try_from(index).expect("D3DRS index fits u32 by RENDER_STATE_COUNT bound");
        if matches!(rs_classify(state, value), RsClass::Consumed) {
            if mtld3d_core::state_trace::enabled() {
                Self::trace_consumed_rs(index, value, default);
            }
            return;
        }
        self.log_unconsumed_rs_once(index, value, default);
    }

    /// Keep trace formatting off the ordinary render-state setter stack.
    #[cold]
    #[inline(never)]
    fn trace_default_rs(index: usize, value: u32) {
        log::trace!(
            target: mtld3d_core::state_trace::TARGET,
            "D3DRS_{index} = {value:#x} (default — write suppressed in warn machinery)"
        );
    }

    /// Keep consumed-state trace formatting off the ordinary setter stack.
    #[cold]
    #[inline(never)]
    fn trace_consumed_rs(index: usize, value: u32, default: u32) {
        log::trace!(
            target: mtld3d_core::state_trace::TARGET,
            "D3DRS_{index} Consumed = {value:#x} (default {default:#x})"
        );
    }

    /// Mark the once-per-slot latch of an unconsumed write, then format its diagnostic.
    #[cold]
    #[inline(never)]
    fn log_unconsumed_rs_once(&mut self, index: usize, value: u32, default: u32) {
        self.mark_rs_warn(index);
        let state = u32::try_from(index).expect("D3DRS index fits u32 by RENDER_STATE_COUNT bound");
        match rs_classify(state, value) {
            RsClass::Consumed => {} // unreachable given early-return above
            RsClass::Obsolete(reason) => {
                info!(
                    target: LOG_TARGET,
                    "D3DRS_{index} = {value:#x} (default {default:#x}) no Metal analog — {reason}"
                );
            }
            RsClass::NotImplemented => {
                warn!(
                    target: LOG_TARGET,
                    "D3DRS_{index} = {value:#x} (default {default:#x}) written but not consumed"
                );
            }
        }
    }

    pub const fn render_states(&self) -> &[u32; RENDER_STATE_COUNT] {
        &self.render_states
    }

    /// `IDirect3DDevice9::Reset` analog of the destruction path.
    ///
    /// Returns the device to the state a fresh `CreateDevice` would have
    /// produced, minus the cursor subclass (per-spec, cursor settings survive
    /// Reset) and the silent-write warn latches (those are process-lifetime
    /// telemetry, not device state).
    ///
    /// Caller is responsible for replacing the implicit backbuffer +
    /// depth/stencil `MTLTextures` *before* calling this — the new handles
    /// flow into the next frame via `fresh_frame`, but the viewport push
    /// here references the new dimensions.
    pub fn reset_to_defaults(&mut self) {
        self.reset_targets_to_implicit();
        self.bound_buffers.teardown();
        self.stage_bindings
            .reset_to_defaults(&[mtld3d_types::sampler_state_defaults(); STAGE_COUNT]);
        for slot in 0..self.vertex_textures.len() {
            if !self.vertex_textures[slot].raw().is_null() {
                self.set_vertex_texture_slot(slot, core::ptr::null_mut());
            }
        }
        for slot in 0..self.vertex_sampler_states.len() {
            self.vertex_sampler_states[slot] = mtld3d_types::sampler_state_defaults();
        }
        self.replace_vertex_decl(core::ptr::null_mut());
        self.shader_bindings
            .replace_vertex_shader(core::ptr::null_mut());
        self.shader_bindings
            .replace_pixel_shader(core::ptr::null_mut());

        self.render_states = render_state_defaults();
        self.point_size = self.render_states[D3DRS_POINTSIZE as usize];
        self.flags.remove(DeviceFlags::A2M_ENABLED);
        self.ff_state = FfState::new();
        self.clip_planes = [[0.0; 4]; CLIP_PLANE_SLOTS];
        self.vs_draw = mtld3d_core::vs_draw::VsDrawState::new();
        self.cached_ff_vs_layout = mtld3d_core::convert::FfVsLayout::default();
        #[cfg(perf_tracking)]
        self.perf.state_mut().advance_reset_epoch();
        // Reset abandons any open scene; a following EndScene must fail.
        self.flags.remove(DeviceFlags::IN_SCENE);
        self.scissor_rect = mtld3d_core::render_state::full_target_scissor(
            self.backbuffer_width,
            self.backbuffer_height,
        );

        let viewport = D3DVIEWPORT9 {
            x: 0,
            y: 0,
            width: self.backbuffer_width,
            height: self.backbuffer_height,
            min_z: 0.0,
            max_z: 1.0,
        };
        self.set_viewport(viewport);
        self.clear_core_draw_cache();
    }

    /// Return the targets to the implicit surfaces, as every `Reset` does.
    ///
    /// Render target 0 goes back to the back buffer, 1 to 3 are unbound, and
    /// the depth stencil goes back to the implicit surface, or to none on a
    /// swap chain without one.
    fn reset_targets_to_implicit(&mut self) {
        self.drain_departed_textures();
        let autogen = core::mem::replace(&mut self.cur_autogen_rt_ids, [None; RENDER_TARGET_SLOTS]);
        for old_id in autogen.into_iter().flatten() {
            self.core.texture_generate_mips(old_id);
        }
        let depth = self
            .core_auto_depth
            .map(|id| crate::rt::device::DepthRef { id, level: 0 });
        let bound = self.core.reset_targets(depth);
        debug_assert!(
            bound || !self.core.has_back_buffer(),
            "the back buffer and the auto depth-stencil bind"
        );
        self.bound_rt.teardown();
        self.last_sized_depth = None;
        self.flags.remove(DeviceFlags::DEPTH_EXPLICITLY_UNBOUND);
    }

    /// What an extended device's `Reset` changes: its targets and the viewport's extent.
    ///
    /// The targets return to the implicit surfaces as on a plain device. The
    /// viewport and the scissor rect cover the new back buffer, and the
    /// viewport keeps its depth range. Render states, bindings, shaders, an
    /// open scene and a recording state block all survive.
    fn rebind_after_extended_reset(&mut self) {
        self.reset_targets_to_implicit();
        let viewport = D3DVIEWPORT9 {
            x: 0,
            y: 0,
            width: self.backbuffer_width,
            height: self.backbuffer_height,
            min_z: self.viewport.min_z,
            max_z: self.viewport.max_z,
        };
        self.set_viewport(viewport);
        self.scissor_rect = mtld3d_core::render_state::full_target_scissor(
            self.backbuffer_width,
            self.backbuffer_height,
        );
        self.clear_core_draw_cache();
    }

    /// Adopt the multisample configuration a `Reset` re-specified.
    ///
    /// Set before the back buffer and implicit depth surface are recreated, so
    /// both are made at the new count, and read by `GetDesc` on the implicit
    /// surfaces afterwards.
    pub const fn set_backbuffer_multi_sample(
        &mut self,
        multi_sample_type: u32,
        multi_sample_quality: u32,
        sample_count: u8,
    ) {
        self.backbuffer_multi_sample_type = multi_sample_type;
        self.backbuffer_multi_sample_quality = multi_sample_quality;
        self.backbuffer_sample_count = sample_count;
    }

    /// Update the device's backbuffer dimensions after `device_reset` honours a resize.
    ///
    /// Every other consumer reads dims off `DeviceInner` (viewport
    /// defaults, `GetBackBuffer`, `GetRenderTarget`), so
    /// this single setter propagates everywhere.
    pub const fn set_backbuffer_dims(&mut self, width: u32, height: u32) {
        self.backbuffer_width = width;
        self.backbuffer_height = height;
    }

    /// The window this device took fullscreen, or `None` when it is windowed.
    pub fn fullscreen_window(&self) -> Option<*mut c_void> {
        self.fullscreen
            .as_ref()
            .map(crate::fullscreen::SavedWindow::window)
    }

    /// Take `hwnd` fullscreen: set `mode`, then borderless, covering the monitor.
    ///
    /// The z-order is left alone. The mode-set keeps the client rect and the
    /// back buffer in one space; present scales the back buffer to the
    /// display. A device created with `D3DCREATE_NOWINDOWCHANGES` leaves the
    /// window untouched, which is what the flag asks for, and still sets the
    /// mode. Called on a device that is already fullscreen it moves the
    /// session onto `hwnd`, leaving the window it was on covering the
    /// monitor.
    pub fn enter_fullscreen(
        &mut self,
        hwnd: *mut c_void,
        mode: Option<mtld3d_core::display_mode::ModeRequest>,
    ) {
        self.fullscreen = Some(crate::fullscreen::enter(hwnd, self.manages_window(), mode));
    }

    /// `true` unless the app took window management over itself.
    ///
    /// `D3DCREATE_NOWINDOWCHANGES` is the app saying it owns the device
    /// window's style, rect and visibility.
    pub const fn manages_window(&self) -> bool {
        self.creation_behavior_flags & mtld3d_types::D3DCREATE_NOWINDOWCHANGES == 0
    }

    /// Re-apply mode and window rect for a device that stays fullscreen across a `Reset`.
    ///
    /// No-op for a device that is not fullscreen: the caller decides the
    /// transition, this only carries it out.
    pub fn update_fullscreen(&mut self, mode: Option<mtld3d_core::display_mode::ModeRequest>) {
        if let Some(saved) = self.fullscreen.as_mut() {
            crate::fullscreen::update(saved, mode);
        }
    }

    /// Re-assert the mode and re-cover the monitor after the app is activated.
    ///
    /// The deferred half of the cursor subclass's `WM_ACTIVATEAPP TRUE`
    /// handling, run when the posted message is processed; a no-op when the
    /// device is no longer fullscreen.
    pub fn reactivate_fullscreen(&mut self) {
        if let Some(saved) = self.fullscreen.as_mut() {
            crate::fullscreen::reactivate(saved);
        }
    }

    /// Re-cover the monitor after an external resize of the fullscreen window.
    ///
    /// The deferred half of the cursor subclass's `WM_SIZE` handling: runs
    /// when the posted re-assert message is processed, which is native
    /// D3D9's cadence. Bounded by the guard in `fullscreen::reassert_cover`;
    /// a no-op when the device is no longer fullscreen.
    pub fn reassert_fullscreen_cover(&mut self, new_width: u32, new_height: u32) {
        if let Some(saved) = self.fullscreen.as_mut() {
            crate::fullscreen::reassert_cover(saved, (new_width, new_height));
        }
    }

    /// Give the window back. No-op unless the device is fullscreen.
    pub fn leave_fullscreen(&mut self, reason: &mtld3d_core::fullscreen_log::LeaveReason) {
        if let Some(saved) = self.fullscreen.take() {
            crate::fullscreen::leave(&saved, self.leave_kind(), reason);
        }
    }

    /// How leaving fullscreen puts this device's window back.
    pub const fn leave_kind(&self) -> crate::fullscreen::LeaveKind {
        if self.is_extended() {
            crate::fullscreen::LeaveKind::Extended
        } else {
            crate::fullscreen::LeaveKind::Plain
        }
    }

    /// Apply an implicit backbuffer resize triggered by a chrome-shrink `WM_SIZE`.
    ///
    /// Mirrors `device_reset`'s size-change pipeline (drain → destroy
    /// old textures → adopt new dims → push `drawableSize` → recreate
    /// textures → reseed `current_frame` → re-push default viewport while
    /// render target 0 is the back buffer) but **skips**
    /// `reset_to_defaults`: the game didn't request a Reset, so its render
    /// states / textures / bindings must survive.
    /// No-op when dims already match, and for a fullscreen device: its
    /// logical size is the mode the game requested, decoupled from the
    /// window, and only a `Reset` may change it. Caller drives this from
    /// the cursor subclass wndproc on the API thread; encoder is paused
    /// inside `flush_current_frame_blocking` for the destroy/create
    /// span so no in-flight cmdbuf references the freed handles.
    ///
    /// On a core device, `apply_core_auto_resize` resizes the core's back
    /// buffer, recreates its auto depth-stencil and updates viewport and
    /// scissor when target 0 is the back buffer. The runtime check is the
    /// recorder's absence (`current_frame.get().is_none()`), which holds
    /// through device release after `core` has been taken.
    pub fn apply_auto_resize(&mut self, new_width: u32, new_height: u32) {
        if new_width == 0 || new_height == 0 {
            return;
        }
        if self.flags.contains(DeviceFlags::RELEASING) {
            debug!(
                target: LOG_TARGET,
                "WM_SIZE ({new_width}x{new_height}) during the device's release ignored; the \
                 back buffer is being destroyed, not resized",
            );
            return;
        }
        if self.fullscreen.is_some() {
            debug!(
                target: LOG_TARGET,
                "WM_SIZE ({new_width}x{new_height}) on a fullscreen device ignored; the back \
                 buffer keeps the requested {}x{}",
                self.backbuffer_width, self.backbuffer_height,
            );
            return;
        }
        if new_width == self.backbuffer_width && new_height == self.backbuffer_height {
            return;
        }
        debug!(
            target: LOG_TARGET,
            "apply_auto_resize: backbuffer {}x{} → {new_width}x{new_height} (WM_SIZE-driven)",
            self.backbuffer_width, self.backbuffer_height,
        );
        self.apply_core_auto_resize(new_width, new_height);
    }

    /// Core-runtime implementation of [`Self::apply_auto_resize`].
    fn apply_core_auto_resize(&mut self, new_width: u32, new_height: u32) {
        if self.encoder_status().is_err() {
            return;
        }
        let old_auto_depth = self.core_auto_depth;
        let rebind_depth = old_auto_depth.is_some_and(|id| {
            self.core.depth() == Some(crate::rt::device::DepthRef { id, level: 0 })
        });
        let rt0_is_back_buffer = self.core.target() == crate::rt::device::TargetRef::BackBuffer;
        let samples = self.backbuffer_sample_count;
        let (render_w, render_h) = self.render_scale.extent(new_width, new_height);
        if !self
            .core
            .resize_scaled(render_w, render_h, new_width, new_height, samples)
        {
            if let Some(id) = self.core_auto_depth.take() {
                self.core.forget_texture(id);
            }
            self.depth_stencil_format = 0;
            self.flags.insert(DeviceFlags::NOT_RESET);
            self.clear_core_draw_cache();
            return;
        }
        self.set_backbuffer_dims(new_width, new_height);
        let mut pp = self.present_params;
        pp.back_buffer_width = new_width;
        pp.back_buffer_height = new_height;
        self.set_present_params(pp);
        let _ = remake_core_auto_depth(self, self.depth_stencil_format);
        if rebind_depth {
            self.core.set_depth(
                self.core_auto_depth
                    .map(|id| crate::rt::device::DepthRef { id, level: 0 }),
            );
        }
        if rt0_is_back_buffer {
            let (min_z, max_z) = if self.is_extended() {
                (self.viewport.min_z, self.viewport.max_z)
            } else {
                (0.0, 1.0)
            };
            self.set_viewport(D3DVIEWPORT9 {
                x: 0,
                y: 0,
                width: new_width,
                height: new_height,
                min_z,
                max_z,
            });
            self.scissor_rect =
                mtld3d_core::render_state::full_target_scissor(new_width, new_height);
        }
        self.clear_core_draw_cache();
    }
}

/// The implicit surfaces an extended device's `Reset` finds the application holding.
struct HeldImplicitSurfaces {
    back_buffer: Option<crate::surface::HeldSurface>,
    depth: Option<crate::surface::HeldSurface>,
}

// ── IDirect3DDevice9 COM object ──

/// Parameters for `Direct3DDevice9::new`.
///
/// Grouped so the constructor doesn't take a dozen positional arguments.
pub struct DeviceCreateInfo {
    /// Sample count of the back buffer and the implicit depth surface, 1 for none.
    pub backbuffer_sample_count: u8,
    pub backbuffer_width: u32,
    pub backbuffer_height: u32,
    /// Resolved `render.scale`, already forced to identity where unusable.
    pub render_scale: mtld3d_core::render_scale::RenderScale,
    /// The core runtime's device.
    pub core: Box<crate::rt::device::CoreDevice>,
    /// Shared view and gamma handle for core cursor overlay updates.
    pub core_overlay: Arc<crate::rt::device::OverlaySink>,
    pub render_states: [u32; RENDER_STATE_COUNT],
    pub sampler_states: [[u32; SAMPLER_STATE_COUNT]; STAGE_COUNT],
    pub direct3d: u64,
    pub creation_adapter: u32,
    pub creation_device_type: u32,
    pub creation_behavior_flags: u32,
    pub creation_focus_window: usize,
    /// Normalised present parameters served by the implicit swapchain and refreshed on `Reset`.
    pub present_params: D3DPRESENT_PARAMETERS,
    /// HWND the Metal layer is attached to.
    pub hwnd: *mut c_void,
    /// Integer multiplier applied to the Win32 HCURSOR bitmap.
    pub cursor_scale: u32,
    /// Whether this device draws its cursor through the overlay window.
    pub software_cursor: bool,
    /// Display sinks owned by the device's cursor state.
    pub display_sinks: Box<crate::cursor::DisplaySinks>,
    /// Window state saved before a fullscreen `CreateDevice` took the window over.
    pub fullscreen: Option<crate::fullscreen::SavedWindow>,
    /// The configuration of the creating `IDirect3D9`.
    pub config: Arc<Mtld3dConfig>,
    /// The flags the device starts with: `DeviceFlags::EXTENDED` for an extended device.
    pub initial_flags: DeviceFlags,
}

#[repr(C)]
pub struct Direct3DDevice9 {
    vtbl: *const IDirect3DDevice9Vtbl,
    refcount: u32,
    inner: *mut DeviceInner,
}

impl Direct3DDevice9 {
    pub fn new(info: DeviceCreateInfo) -> Self {
        let viewport = D3DVIEWPORT9 {
            x: 0,
            y: 0,
            width: info.backbuffer_width,
            height: info.backbuffer_height,
            min_z: 0.0,
            max_z: 1.0,
        };

        if info.config.present_max_frame_latency != 0 {
            log::info!(
                target: LOG_TARGET,
                "frame latency: present.maxFrameLatency = {latency} replaces what the game \
                 asks for; this device keeps at most {latency} presented frames outstanding",
                latency = info.config.present_max_frame_latency,
            );
        }

        let inner = Box::into_raw(Box::new(DeviceInner {
            backbuffer_multi_sample_type: info.present_params.multi_sample_type,
            backbuffer_multi_sample_quality: info.present_params.multi_sample_quality,
            backbuffer_sample_count: info.backbuffer_sample_count,
            depth_stencil_format: 0,
            flags: info.initial_flags,
            frame_latency: mtld3d_core::frame_latency::FrameLatency::new(
                info.config.present_max_frame_latency,
            ),
            backbuffer_width: info.backbuffer_width,
            backbuffer_height: info.backbuffer_height,
            render_scale: info.render_scale,
            vertex_decl: CachedComPtr::null(),
            fvf_decl_cache: rustc_hash::FxHashMap::default(),
            last_fvf_bind: (0, core::ptr::null_mut()),
            direct3d: info.direct3d,
            device_wrapper: 0,
            creation_adapter: info.creation_adapter,
            creation_device_type: info.creation_device_type,
            creation_behavior_flags: info.creation_behavior_flags,
            creation_focus_window: info.creation_focus_window,
            api_lock: (info.creation_behavior_flags & mtld3d_types::D3DCREATE_MULTITHREADED != 0)
                .then(|| &*Box::leak(Box::new(ApiLock::new()))),
            present_params: info.present_params,
            implicit_swapchain: 0,
            implicit_render_target: 0,
            implicit_depth_stencil: 0,
            core_draw_cache: core_draw::DrawCache::new(),
            core: info.core,
            core_auto_depth: None,
            last_sized_depth: None,
            departed_textures: Mutex::new(Vec::new()),
            programs: rustc_hash::FxHashMap::default(),
            core_failure: AtomicI32::new(D3D_OK),
            mem_watch: mem_watch::MemWatchState::new(),
            vram_bytes_used: Arc::new(AtomicU64::new(0)),
            outstanding_reset_blockers: AtomicU32::new(0),
            perf: ApiPerfStorage::new(),
            config: info.config,
            point_size: info.render_states[D3DRS_POINTSIZE as usize],
            render_states: info.render_states,
            rs_warn_fired: [0; RENDER_STATE_COUNT.div_ceil(64)],
            ff_state: FfState::new(),
            scissor_rect: mtld3d_core::render_state::full_target_scissor(
                info.backbuffer_width,
                info.backbuffer_height,
            ),
            viewport,
            clip_planes: [[0.0; 4]; CLIP_PLANE_SLOTS],
            vs_draw: mtld3d_core::vs_draw::VsDrawState::new(),
            cached_ff_vs_layout: mtld3d_core::convert::FfVsLayout::default(),
            cursor: CursorState::new(
                info.hwnd,
                info.cursor_scale,
                info.software_cursor,
                info.display_sinks,
                info.core_overlay,
            ),
            fullscreen: info.fullscreen,
            bound_rt: BoundRt::new(info.backbuffer_width, info.backbuffer_height),
            bound_buffers: BoundBuffers::new(),
            shader_bindings: ShaderBindings::new(),
            stage_bindings: StageBindings::new(&info.sampler_states),
            vertex_textures: [const { CachedComPtr::null() }; 4],
            vertex_sampler_states: [mtld3d_types::sampler_state_defaults(); 4],
            recording_state_block: None,
            gamma_ramp: Box::new(mtld3d_core::gamma::identity_ramp()),
            cur_autogen_rt_ids: [None; RENDER_TARGET_SLOTS],
            live_textures: Mutex::new(rustc_hash::FxHashMap::default()),
        }));
        Self {
            vtbl: &raw const DIRECT3D_DEVICE9_VTBL.base,
            refcount: 1,
            inner,
        }
    }

    pub fn inner(&self) -> &'static mut DeviceInner {
        // SAFETY: `self.inner` was installed by `Self::new` as a
        // `Box::into_raw` and is dropped only in `device_release` at
        // refcount zero, so it stays live for every live wrapper
        // reference.
        unsafe { &mut *self.inner }
    }

    /// Raw `DeviceInner` pointer.
    ///
    /// Used by resource-wrapper constructors (and `ApiTimer` guards
    /// inside them) that need a stable back-ref without holding a Rust
    /// reference across the resource's lifetime.
    pub const fn inner_ptr(&self) -> *mut DeviceInner {
        self.inner
    }

    /// COM `AddRef` on the device wrapper, taken on a child object's behalf.
    ///
    /// Bumps the wrapper refcount directly; access is exclusive (D3D9 objects
    /// are single-threaded, or serialised by the device `ApiLock` under
    /// `D3DCREATE_MULTITHREADED`), so this matches `device_add_ref`'s effect
    /// without routing another module through the vtable thunk.
    pub const fn add_ref_self(&mut self) -> u32 {
        self.refcount += 1;
        self.refcount
    }
}

impl DeviceInner {
    /// Mutable access to the core runtime device.
    pub fn core_mut(&mut self) -> &mut crate::rt::device::CoreDevice {
        &mut self.core
    }

    /// Marks the core draw cache dirty so the next draw resolves fresh state.
    pub const fn invalidate_core_draw(&mut self) {
        self.core_draw_cache.invalidate();
    }

    /// Clears cached core draw bindings and marks the cache dirty.
    pub fn clear_core_draw_cache(&mut self) {
        self.core_draw_cache.clear();
    }

    /// Owning `Direct3DDevice9`* wrapper, or null until `CreateDevice` stamps it.
    ///
    /// Stamped via [`set_device_wrapper`](Self::set_device_wrapper).
    /// Returned (`AddRef`'d) by resource `GetDevice` thunks.
    pub const fn device_wrapper(&self) -> *mut c_void {
        self.device_wrapper as *mut c_void
    }

    /// Stamp the owning wrapper pointer once the COM object is boxed in `CreateDevice`.
    pub fn set_device_wrapper(&mut self, wrapper: *mut c_void) {
        self.device_wrapper = wrapper as u64;
    }

    /// The device's API lock as the pointer a texture keeps; null without the multithreaded flag.
    ///
    /// The lock is leaked at creation, so the pointer stays valid after this
    /// device is gone; a `D3DPOOL_MANAGED` texture that outlives the device
    /// keeps entering it.
    pub fn api_lock_ptr(&self) -> *mut ApiLock {
        self.api_lock.map_or(core::ptr::null_mut(), |lock| {
            core::ptr::from_ref(lock).cast_mut()
        })
    }

    /// Sample count of the back buffer and the implicit depth surface, 1 for none.
    #[must_use]
    pub const fn backbuffer_sample_count(&self) -> u8 {
        self.backbuffer_sample_count
    }

    /// `D3DMULTISAMPLE_TYPE` the swap chain was created with.
    #[must_use]
    pub const fn backbuffer_multi_sample_type(&self) -> u32 {
        self.backbuffer_multi_sample_type
    }

    /// `MultiSampleQuality` the swap chain was created with.
    #[must_use]
    pub const fn backbuffer_multi_sample_quality(&self) -> u32 {
        self.backbuffer_multi_sample_quality
    }

    pub const fn backbuffer_width(&self) -> u32 {
        self.backbuffer_width
    }

    pub const fn backbuffer_height(&self) -> u32 {
        self.backbuffer_height
    }

    /// The scale the back buffer and everything sized with it is rasterized at.
    ///
    /// Read by the implicit surfaces when sizing rasterized targets.
    pub const fn render_scale(&self) -> mtld3d_core::render_scale::RenderScale {
        self.render_scale
    }

    /// Whether a depth-stencil surface is currently bound to the device.
    ///
    /// Distinct from "the device has an auto depth-stencil": an explicit
    /// `SetDepthStencilSurface(NULL)` unbinds depth even though the auto
    /// texture still exists, and a custom depth surface bound for an
    /// offscreen render target is reflected in `bound_rt` rather than in the
    /// auto `depth_stencil_handle`. Used by `Clear` to reject
    /// `D3DCLEAR_ZBUFFER`/`_STENCIL` with no depth attachment.
    pub const fn depth_stencil_bound(&self) -> bool {
        if self.flags.contains(DeviceFlags::DEPTH_EXPLICITLY_UNBOUND) {
            return false;
        }
        // A custom depth surface bound via `SetDepthStencilSurface` counts as
        // bound regardless of the auto handle; otherwise the device default
        // auto depth-stencil is in effect iff its handle exists.
        !self.bound_rt.depth_stencil().is_null() || self.has_auto_depth()
    }

    /// Whether the device has an auto depth-stencil.
    const fn has_auto_depth(&self) -> bool {
        self.core_auto_depth.is_some()
    }

    /// A core device's auto depth-stencil, `None` when the device has none.
    pub const fn core_auto_depth(&self) -> Option<mtld3d_core::ids::TextureId> {
        self.core_auto_depth
    }

    /// Records a core device's auto depth-stencil and its `D3DFMT_*` (`CreateDevice`, `Reset`).
    pub const fn set_core_auto_depth(&mut self, id: mtld3d_core::ids::TextureId, format: u32) {
        self.core_auto_depth = Some(id);
        self.depth_stencil_format = format;
    }

    /// The device's default depth-stencil format (`D3DFMT_*`), or `0` when none.
    pub const fn depth_stencil_format(&self) -> u32 {
        self.depth_stencil_format
    }

    /// Normalised present parameters the implicit swapchain reports.
    pub const fn present_params(&self) -> &D3DPRESENT_PARAMETERS {
        &self.present_params
    }

    /// `true` when presentation must wait for a successful `Reset`.
    pub const fn needs_reset(&self) -> bool {
        self.flags.contains(DeviceFlags::NOT_RESET)
    }

    /// `true` for a device an interface `Direct3DCreate9Ex` made created.
    pub const fn is_extended(&self) -> bool {
        self.flags.contains(DeviceFlags::EXTENDED)
    }

    /// The code this device reports for a call that produced `hr`.
    ///
    /// [`mtld3d_core::extended::state_code`] for this kind of device.
    pub const fn state_code(&self, hr: i32) -> i32 {
        mtld3d_core::extended::state_code(hr, self.is_extended())
    }

    /// The device's frame-latency bound.
    pub const fn frame_latency(&self) -> &mtld3d_core::frame_latency::FrameLatency {
        &self.frame_latency
    }

    /// The device's frame-latency bound, for `SetMaximumFrameLatency` and `Present`.
    pub const fn frame_latency_mut(&mut self) -> &mut mtld3d_core::frame_latency::FrameLatency {
        &mut self.frame_latency
    }

    /// `true` when the device is fullscreen.
    ///
    /// `CreateAdditionalSwapChain` is rejected in that mode.
    pub const fn is_fullscreen(&self) -> bool {
        self.present_params.windowed == 0
    }

    /// The ramp the application last set, for `GetGammaRamp`.
    pub fn gamma_ramp(&self) -> D3DGAMMARAMP {
        D3DGAMMARAMP {
            red: self.gamma_ramp.red,
            green: self.gamma_ramp.green,
            blue: self.gamma_ramp.blue,
        }
    }

    /// Keep a validated ramp and queue what the present pass should do with it.
    pub fn set_gamma_ramp(&mut self, ramp: &D3DGAMMARAMP) {
        *self.gamma_ramp = D3DGAMMARAMP {
            red: ramp.red,
            green: ramp.green,
            blue: ramp.blue,
        };
        if !self.is_fullscreen() && !mtld3d_core::gamma::is_identity(&self.gamma_ramp) {
            // Native ramps the whole desktop for a windowed device, which is
            // not ours to touch, so the ramp is kept for `GetGammaRamp` and
            // changes nothing on screen. It applies as soon as a `Reset`
            // takes the device fullscreen, on either runtime.
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "SetGammaRamp on a windowed device: the ramp is stored and reported back, \
                 but only a fullscreen device applies one"
            );
        }
        self.queue_gamma();
    }

    /// Queue the table the layer should carry, when it is not the one it has.
    ///
    /// A non-identity ramp on a fullscreen device is queued every time it is
    /// set, since two different ramps both want applying; a ramp that stops
    /// applying is queued once, as a removal. A core device hands its
    /// presenter the ramp on the same rule, or none, from the next present
    /// (DXMTRS C4c-7d-1), and queues nothing.
    fn queue_gamma(&mut self) {
        let apply = self.is_fullscreen() && !mtld3d_core::gamma::is_identity(&self.gamma_ramp);
        if apply {
            mtld3d_shared::log_once_info!(
                target: crate::LOG_TARGET,
                "d3d9 core: the gamma ramp is applied inline by the presenter of a fullscreen \
                 device"
            );
        }
        self.core
            .set_gamma_ramp(apply.then(|| mtld3d_core::gamma::to_lut(&self.gamma_ramp)));
        if self.cursor.is_software() {
            self.cursor.push_overlay_state();
        }
    }

    /// Re-evaluate the gamma ramp after a fullscreen/windowed edge.
    pub fn reapply_gamma(&mut self) {
        self.queue_gamma();
    }

    /// The device's presentation window.
    ///
    /// The `device_window` it was created with, falling back to the focus
    /// window — the default target a `CreateAdditionalSwapChain` request
    /// resolves its dimensions against.
    pub const fn window(&self) -> usize {
        if self.present_params.device_window != 0 {
            self.present_params.device_window
        } else {
            self.creation_focus_window
        }
    }

    /// Get-or-create the device-owned implicit swapchain (`GetSwapChain(0)`), cached as a `u64`.
    ///
    /// Created at refcount 0 — the caller AddRef-forwards it so the first
    /// hand-out bumps the device. The shell is leaked at teardown.
    pub fn get_or_create_implicit_swapchain(
        &mut self,
    ) -> *mut crate::swapchain::Direct3DSwapChain9 {
        if self.implicit_swapchain == 0 {
            let sc = crate::swapchain::Direct3DSwapChain9::new_implicit(
                core::ptr::from_mut(self),
                self.present_params,
            );
            self.implicit_swapchain = Box::into_raw(Box::new(sc)) as u64;
        }
        self.implicit_swapchain as *mut crate::swapchain::Direct3DSwapChain9
    }

    /// Capture the implicit surfaces the application holds, for an extended device's `Reset`.
    ///
    /// Empty on a plain device, whose `Reset` refuses while the application
    /// holds either. Taken before the `Reset` adopts its new back-buffer size,
    /// multisample configuration or auto depth-stencil format, so each
    /// capture describes the surface the application holds.
    fn hold_implicit_surfaces(&self) -> HeldImplicitSurfaces {
        if !self.is_extended() {
            return HeldImplicitSurfaces {
                back_buffer: None,
                depth: None,
            };
        }
        HeldImplicitSurfaces {
            // SAFETY: the field is 0 or the device's live cached implicit surface.
            back_buffer: unsafe {
                crate::surface::hold_implicit_surface(self.implicit_render_target)
            },
            // SAFETY: as above.
            depth: unsafe { crate::surface::hold_implicit_surface(self.implicit_depth_stencil) },
        }
    }

    /// Detach the held implicit surfaces `held` names, for an extended device's `Reset`.
    ///
    /// Each becomes a standalone surface that keeps the textures it names,
    /// and the device forgets both the surface and its own handles to those
    /// textures, so nothing it destroys or recreates afterwards reaches them
    /// and the next `GetBackBuffer` or `GetDepthStencilSurface` hands out a
    /// new object.
    fn detach_held_implicit_surfaces(&mut self, held: HeldImplicitSurfaces) {
        if let Some(back_buffer) = held.back_buffer {
            // SAFETY: captured from the live, held cached back buffer, which
            // the device forgets just below.
            unsafe { crate::surface::detach_implicit_surface(&back_buffer) };
            self.implicit_render_target = 0;
        }
        if let Some(depth) = held.depth {
            // SAFETY: as above, for the cached depth surface.
            unsafe { crate::surface::detach_implicit_surface(&depth) };
            self.implicit_depth_stencil = 0;
            // The detached surface took the auto depth texture over under its id (DXMTRS C4c-6c).
            self.core_auto_depth = None;
        }
    }

    /// Get-or-create the device-owned implicit render target == backbuffer surface.
    ///
    /// Cached as a `u64`. `GetRenderTarget(0)`, `GetBackBuffer(0)` and
    /// the implicit `GetSwapChain(0).GetBackBuffer(0)` all return this one
    /// object (the `pRenderTarget == pBackBuffer` identity the suite
    /// checks). Its container is the implicit swapchain. Created at
    /// refcount 0.
    pub fn get_or_create_implicit_render_target(
        &mut self,
    ) -> *mut crate::surface::Direct3DSurface9 {
        if self.implicit_render_target == 0 {
            let container = self.get_or_create_implicit_swapchain() as u64;
            let surf = crate::surface::Direct3DSurface9::new_implicit_backbuffer(
                core::ptr::from_mut(self),
                container,
            );
            self.implicit_render_target = Box::into_raw(Box::new(surf)) as u64;
        }
        self.implicit_render_target as *mut crate::surface::Direct3DSurface9
    }

    /// Get-or-create the device-owned implicit depth-stencil surface (`GetDepthStencilSurface`).
    ///
    /// Cached as a `u64`. Its container is the device. Returns null when
    /// the device has no auto depth-stencil (the caller maps that to
    /// `D3DERR_NOTFOUND`). Created at refcount 0.
    pub fn get_or_create_implicit_depth_stencil(
        &mut self,
    ) -> *mut crate::surface::Direct3DSurface9 {
        if !self.has_auto_depth() {
            return core::ptr::null_mut();
        }
        if self.implicit_depth_stencil == 0 {
            let container = self.device_wrapper;
            let surf = crate::surface::Direct3DSurface9::new_implicit_depth_stencil(
                core::ptr::from_mut(self),
                container,
            );
            self.implicit_depth_stencil = Box::into_raw(Box::new(surf)) as u64;
        }
        self.implicit_depth_stencil as *mut crate::surface::Direct3DSurface9
    }

    /// Refresh the stored present parameters after Reset or a backbuffer resize.
    ///
    /// Back-buffer count clamped to >= 1, matching `CreateDevice`.
    pub fn set_present_params(&mut self, mut pp: D3DPRESENT_PARAMETERS) {
        pp.back_buffer_count = pp.back_buffer_count.max(1);
        let was_fullscreen = self.is_fullscreen();
        self.present_params = pp;
        // A gamma ramp only applies fullscreen, and a layer the Reset
        // re-attached carries no table, so the ramp the device holds is sent
        // again on both edges rather than assumed live.
        if was_fullscreen != self.is_fullscreen() {
            self.reapply_gamma();
        }
        // Keep the cached implicit swapchain (if it has already been handed
        // out) in lockstep: GetSwapChain(0).GetPresentParameters must reflect
        // the live geometry, not the values captured when it was created.
        if self.implicit_swapchain != 0 {
            let sc = self.implicit_swapchain as *mut crate::swapchain::Direct3DSwapChain9;
            // SAFETY: `implicit_swapchain` is a device-owned `Box::into_raw`,
            // freed only at device teardown, so it is live here.
            unsafe { (*sc).set_present_params(pp) };
        }
    }
}

// ── RtBinding — attachment info captured on API thread for pass break ──
//
// The `SetRenderTarget` operation runs on the encoder thread and needs to
// either (1) create/fetch a Metal texture for a texture-backed RT surface,
// or (2) restore the backbuffer handle for a standalone surface. We
// capture everything it needs at call time since the surface pointer may
// be released before the operation runs.

// ── IUnknown implementation (IDirect3DDevice9) ──

/// Whether the device a child resource records as its `DeviceInner` is extended.
///
/// # Safety
/// `device_inner` is 0 or the address of a live `DeviceInner`.
pub unsafe fn device_inner_is_extended(device_inner: u64) -> bool {
    // SAFETY: the caller's contract: 0 or a live `DeviceInner`.
    unsafe { (device_inner as *const DeviceInner).as_ref() }.is_some_and(DeviceInner::is_extended)
}

/// Whether the device behind a vtable `this` is extended; `false` for a null `this`.
pub fn device_is_extended(this: *mut c_void) -> bool {
    // SAFETY: vtable thunk argument; `this` is null or *mut Direct3DDevice9 per
    // the IDirect3DDevice9 ABI.
    unsafe { InPtr::<Direct3DDevice9>::opt(this) }.is_some_and(|obj| obj.inner().is_extended())
}

/// Apply the `pSharedHandle` rules to a create, `Err` with the refusal's code.
///
/// `Ok(Some)` is the application's pointer to the level's packed pixels, which
/// only an extended device's single-level system-memory texture or offscreen
/// plain surface reads; `Ok(None)` proceeds as before. A pointer to a null
/// pointer is accepted as user memory with nothing in it, warned once.
fn shared_handle_gate(
    this: *mut c_void,
    kind: &mtld3d_core::extended::CreateKind,
    pool: u32,
    shared_handle: *mut c_void,
    entry_point: &str,
) -> Result<Option<NonNull<u8>>, i32> {
    use mtld3d_core::extended::{SharedHandleVerdict, shared_handle_verdict};
    let extended = device_is_extended(this);
    match shared_handle_verdict(kind, pool, !shared_handle.is_null(), extended) {
        SharedHandleVerdict::Proceed => Ok(None),
        SharedHandleVerdict::UserMemory => {
            // SAFETY: an extended device's user-memory contract: `shared_handle`
            // is non-null (the verdict says so) and points to the application's
            // pixel pointer.
            let memory =
                unsafe { ValueIn::<*mut c_void>::read_opt(shared_handle.cast_const().cast()) };
            let memory = memory.and_then(|memory| NonNull::new(memory.cast::<u8>()));
            if memory.is_none() {
                mtld3d_shared::log_once_warn!(target: LOG_TARGET,
                    "{entry_point}: user memory names a null pointer; the level starts \
                     uninitialised");
            }
            Ok(memory)
        }
        SharedHandleVerdict::Refuse(refusal) => {
            mtld3d_shared::log_once_warn_by!(
                target: LOG_TARGET,
                key: (create_kind_key(kind) << 48)
                    | (u64::from(pool) << 32)
                    | u64::from(refusal.hresult().cast_unsigned()),
                "reject {entry_point}(pool={pool}) with a pSharedHandle → {:#010x}: {}",
                refusal.hresult(),
                refusal.reason()
            );
            Err(refusal.hresult())
        }
    }
}

/// The pool a create in `pool` makes its resource in, `None` when the device refuses it.
///
/// An extended device refuses `D3DPOOL_MANAGED`, warned once per kind; both
/// kinds of device take `D3DPOOL_MANAGED_EX` as the managed pool.
fn create_pool(
    this: *mut c_void,
    kind: &mtld3d_core::extended::CreateKind,
    pool: u32,
    entry_point: &str,
) -> Option<u32> {
    if mtld3d_core::pool::refused_on_extended(pool, device_is_extended(this)) {
        mtld3d_shared::log_once_warn_by!(
            target: LOG_TARGET,
            key: create_kind_key(kind),
            "reject {entry_point}(D3DPOOL_MANAGED) on an extended device → INVALIDCALL"
        );
        return None;
    }
    Some(mtld3d_core::pool::resolve(pool))
}

/// A distinct log-latch key per create kind.
const fn create_kind_key(kind: &mtld3d_core::extended::CreateKind) -> u64 {
    use mtld3d_core::extended::CreateKind;
    match kind {
        CreateKind::Texture { .. } => 0,
        CreateKind::CubeTexture => 1,
        CreateKind::VolumeTexture => 2,
        CreateKind::VertexBuffer => 3,
        CreateKind::IndexBuffer => 4,
        CreateKind::RenderTarget => 5,
        CreateKind::DepthStencil => 6,
        CreateKind::OffscreenPlain => 7,
    }
}

/// Copy an extended create's user memory into the level that backs it.
///
/// `rows` is the level's packed extent and `pitch` the row pitch of `level`.
/// A level too small for the rows is left as it was, warned once.
fn copy_user_memory(
    memory: NonNull<u8>,
    rows: &mtld3d_core::extended::PackedRows,
    level: &mut [u8],
    pitch: usize,
) {
    // SAFETY: an extended device's user-memory contract: the application's
    // pointer covers the level's tightly packed rows, which `rows` sizes from
    // the create's own extent and format, and it stays valid for the create.
    let src =
        unsafe { core::slice::from_raw_parts(memory.as_ptr().cast_const(), rows.total_bytes()) };
    if !mtld3d_core::extended::copy_packed_rows(src, rows, level, pitch) {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET,
            "user memory: {} rows of {} bytes do not fit a level of pitch {pitch} and {} bytes; \
             the level starts uninitialised",
            rows.rows, rows.row_bytes, level.len());
    }
}

/// Hold the device's API lock for the thunk; a no-op unless the app asked for `MULTITHREADED`.
///
/// Every `IDirect3DDevice9` entry point binds this as its first statement,
/// ahead of its `ApiTimer`, so the timer's drop runs under the lock and a
/// wait for the lock counts as API time, which is what the app pays. The
/// shell is leaked at teardown and its inner is not, so a refcount of zero
/// means there is no inner to read: a `Release` past zero, like a null
/// `this`, gets the no-op guard. A texture reaches the lock through the
/// pointer it carries instead ([`crate::com_ref::ComChild::enter_api_lock`]),
/// because a managed one can be called while the device is in its final
/// `Release` or gone. The unflagged fast path is a null test, a refcount
/// load, one pointer chase and a discriminant test.
#[inline]
pub fn device_api_lock(this: *mut c_void) -> ApiGuard {
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return ApiGuard::NOOP;
    };
    if obj.refcount == 0 {
        return ApiGuard::NOOP;
    }
    let Some(lock) = obj.inner().api_lock else {
        return ApiGuard::NOOP;
    };
    // SAFETY: the lock was leaked at device creation and lives for the rest
    // of the process, so it outlives the guard.
    unsafe { lock.enter() }
}

/// Constructs an `ApiTimer` keyed to this device's `ApiPerfState`.
///
/// Hot path: every `IDirect3DDevice9` vtable entry calls it, the cursor
/// thunks in `cursor.rs` included. `#[inline]` suffices — release uses
/// thin-LTO so the null-check + handoff folds into the caller. The `sub`
/// arg picks which `DeviceSubCategory` bucket the elapsed cycles land in
/// for the per-sub breakdown in the 2-second summary; the top-level
/// `Device` bucket is bumped in parallel under one `rdtsc()` delta.
#[inline]
pub fn device_timer(this: *mut c_void, sub: DeviceSubCategory) -> ApiTimer {
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let storage = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }).and_then(|obj| {
        // SAFETY: the entry point holds the API lock and the device is live at timer entry.
        unsafe { DeviceInner::perf_storage_of(obj.inner) }
    });
    ApiTimer::start_device(storage, sub)
}

/// Same shape as `device_timer` but for entry points whose `DeviceSubCategory` would be `Bind`.
///
/// Tags the timer with a `BindSubCategory` so the 2-second summary can
/// decompose the `Bind` row into per-Setter-family rows. The single
/// `rdtsc()` delta bumps Device top + `Bind` device-sub + the chosen
/// `BindSubCategory` in one pass.
#[inline]
fn bind_timer(this: *mut c_void, sub: BindSubCategory) -> ApiTimer {
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let storage = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }).and_then(|obj| {
        // SAFETY: the entry point holds the API lock and the device is live at timer entry.
        unsafe { DeviceInner::perf_storage_of(obj.inner) }
    });
    ApiTimer::start_bind(storage, sub)
}

extern "system" fn device_query_interface(
    this: *mut c_void,
    riid: *const Guid,
    ppv: *mut *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    let accepted: &[Guid] = if device_is_extended(this) {
        &[
            mtld3d_types::IID_IUNKNOWN,
            mtld3d_types::IID_IDIRECT3DDEVICE9,
            mtld3d_types::IID_IDIRECT3DDEVICE9EX,
        ]
    } else {
        &[
            mtld3d_types::IID_IUNKNOWN,
            mtld3d_types::IID_IDIRECT3DDEVICE9,
        ]
    };
    // SAFETY: vtable thunk; `this`, `riid` and `ppv` are the caller's per the
    // IUnknown::QueryInterface ABI.
    unsafe {
        crate::com_ref::com_query_interface(
            this,
            riid,
            ppv,
            accepted,
            device_add_ref,
            "IDirect3DDevice9",
        )
    }
}

extern "system" fn device_add_ref(this: *mut c_void) -> u32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // SAFETY: D3D9 AddRef — `this` is a caller-owned Direct3DDevice9* obtained
    // from a prior interface-returning method. Null `this` is UB per spec; we
    // preserve that crash semantic so refcount miscounts surface as a
    // null-deref rather than silent corruption.
    // SAFETY: IDirect3DDevice9 IUnknown thunk; D3D9 ABI guarantees `this` is *mut Direct3DDevice9.
    let mut wrap = unsafe { VtableThis::<Direct3DDevice9>::new(this) };
    let obj: &mut Direct3DDevice9 = &mut wrap;
    obj.refcount += 1;
    obj.refcount
}

extern "system" fn device_release(this: *mut c_void) -> u32 {
    let _api = device_api_lock(this);
    // A stray Release of a device an earlier final Release tore down is the
    // no-op the refcount guard below makes it, and is not timed: the timer
    // reads the device's counters, which went with it.
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let live = unsafe { InPtr::<Direct3DDevice9>::opt(this) }.is_some_and(|obj| obj.refcount != 0);
    let _timer = live.then(|| device_timer(this, DeviceSubCategory::Misc));
    // SAFETY: D3D9 Release — same contract as AddRef above; null `this` is UB
    // per spec.
    // SAFETY: IDirect3DDevice9 IUnknown thunk; D3D9 ABI guarantees `this` is *mut Direct3DDevice9.
    let mut wrap = unsafe { VtableThis::<Direct3DDevice9>::new(this) };
    let obj: &mut Direct3DDevice9 = &mut wrap;
    // Defensive against a stray double-Release. The wrapper shell is
    // intentionally leaked at teardown (below) rather than freed, so this read
    // stays valid; an over-release on an already-torn-down device is then a
    // no-op returning 0 instead of underflowing the refcount and dereferencing
    // freed memory. A stray double-Release can drive the refcount past zero;
    // D3D9 tolerates Release-past-zero.
    if obj.refcount == 0 {
        return 0;
    }
    obj.refcount -= 1;
    let rc = obj.refcount;
    if rc == 0 {
        // Snapshot handles + parent pointer before tearing down DeviceInner —
        // the Metal destroy + parent Release both need fields that live inside
        // the inner box.
        // SAFETY: refcount reached zero; `obj.inner` is the original
        // `Box::into_raw(DeviceInner)` from `Direct3DDevice9::new` and
        // no other reference can survive a zero refcount.
        let mut device_inner = unsafe { Box::from_raw(obj.inner) };

        // From here on a window message that reaches the cursor subclass
        // finds a device being torn down, and the resize it may ask for
        // must not rebuild what the steps below destroy.
        device_inner.flags.insert(DeviceFlags::RELEASING);

        // D3DPOOL_MANAGED textures the game still holds outlive this device.
        // Their `device_inner`/`device_handle` are about to dangle. Zero
        // them out so accessor sites detect "between devices" and bail
        // safely; `rehydrate_for_device` will repoint on first bind under
        // the next device.
        {
            let live = device_inner
                .live_textures
                .lock()
                .expect("live_textures mutex poisoned");
            for &ti_ptr in live.values() {
                // SAFETY: entries in `live_textures` are removed on
                // `TextureInner` drop, so the pointer is live for the
                // duration of this loop iteration.
                let ti = unsafe { &mut *ti_ptr };
                ti.detach_from_device();
            }
        }
        let parent = device_inner.direct3d as *mut c_void;

        // Hand the window back before the subclass goes: the restore issues a
        // `SetWindowPos`, and the game's own wndproc should see it exactly as
        // it sees any other window change.
        device_inner.leave_fullscreen(&mtld3d_core::fullscreen_log::LeaveReason::Release);

        // Leave the window's subclass *before* freeing DeviceInner; the
        // registry's back-pointer becomes dangling once we drop. The game's
        // procedure comes back with the last device on the window.
        let device_ptr = std::ptr::from_mut::<DeviceInner>(&mut device_inner);
        device_inner.cursor().uninstall_subclass(device_ptr);
        // The HCURSORs the device built die with it, now that no message can
        // realize one of them again.
        device_inner.cursor_mut().destroy_handles();

        // Release bound surfaces + buffers + textures (if any) before teardown,
        // and the objects an open `BeginStateBlock` recording holds: a
        // device-internal reference can be the last one on its object, whose
        // finalization still reaches this device.
        device_inner.recording_state_block = None;
        device_inner.bound_rt_mut().teardown();
        device_inner.bound_buffers_mut().teardown();
        device_inner.stage_bindings_mut().teardown();
        device_inner.teardown_vertex_textures();
        device_inner.replace_vertex_decl(core::ptr::null_mut());

        // Finalize the device-owned implicit RT + depth-stencil surfaces. They
        // are never finalized by `surface_release` (they outlive the app's
        // Release), so this is where their `SurfaceInner` drops — releasing any
        // `SetPrivateData(D3DSPD_IUNKNOWN)` callback object exactly at device
        // destroy (per the D3D9 device-destroy contract). Runs AFTER `bound_rt.teardown`
        // (which only decrements the implicit RT's private refcount) and BEFORE
        // `drop(device_inner)`. The implicit swapchain carries no private data,
        // so its shell is left leaked like the device wrapper.
        let implicit_rt = device_inner.implicit_render_target;
        let implicit_ds = device_inner.implicit_depth_stencil;
        // SAFETY: both are `0` or live implicit-surface wrappers created by this
        // device; finalized once here, the app having released its references.
        unsafe { crate::surface::finalize_implicit_surface(implicit_rt) };
        // SAFETY: as above.
        unsafe { crate::surface::finalize_implicit_surface(implicit_ds) };

        device_inner.drain_departed_textures();
        device_inner.core_draw_cache.clear();
        drop(device_inner);
        obj.refcount = 0;
        release_parent(parent);
    }
    rc
}

/// Give back the reference a device holds on the `IDirect3D9` that created it.
///
/// Null for a device whose parent was never recorded.
fn release_parent(parent: *mut c_void) {
    if parent.is_null() {
        return;
    }
    // SAFETY: `parent` is non-null (checked above) and was AddRef'd during
    // `Direct3D9::CreateDevice`; the parent's refcount has kept it alive until
    // this Release.
    let parent_obj = unsafe { &*(parent as *const ParentIUnknown) };
    // SAFETY: `parent_obj.vtbl` is the `'static` parent vtable.
    let vtbl = unsafe { &*parent_obj.vtbl };
    (vtbl.release)(parent);
}

/// The owning `Direct3DDevice9`* wrapper for a child's `device_inner` pointer.
///
/// Null if the pointer is null. Used by child wrappers to resolve their
/// device-forward target ([`crate::com_ref::ComChild::device_forward_target`]).
#[must_use]
pub fn device_wrapper_from(inner: *mut DeviceInner) -> *mut c_void {
    if inner.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: a non-null child `device_inner` is the live owning device, alive
    // past its children per D3D9 lifetime rules.
    unsafe { (*inner).device_wrapper() }
}

/// Take one reference on a device wrapper on behalf of a child object.
///
/// Two callers. The implicit swapchain and the implicit render-target /
/// depth-stencil surfaces are device-owned: each holds exactly one reference
/// on the device while its own public refcount is non-zero, acquired here on
/// the child's 0→1 transition. `GetDevice` takes one as well, on behalf of the
/// application, which releases it. `wrapper` is the `Direct3DDevice9`* from
/// [`DeviceInner::device_wrapper`]; a null wrapper (device not yet stamped) is
/// a no-op.
pub fn device_wrapper_add_ref(wrapper: *mut c_void) {
    if wrapper.is_null() {
        return;
    }
    // SAFETY: `wrapper` is the live `Direct3DDevice9` that owns the forwarding
    // child; access is exclusive (D3D9 objects are single-threaded, or
    // serialised by the device `ApiLock` under `D3DCREATE_MULTITHREADED`), so
    // the transient exclusive borrow to bump the refcount is sound.
    unsafe { (*wrapper.cast::<Direct3DDevice9>()).add_ref_self() };
}

/// Record that the app acquired (`true`) or dropped (`false`) its reference to a `Reset` blocker.
///
/// Fired by the COM engine on the public 0↔1 edge of a child whose
/// `blocks_reset_while_referenced` is set; `wrapper` is that child's
/// forwarding device (null when the child has none, a no-op).
pub fn device_wrapper_note_reset_blocker(wrapper: *mut c_void, acquired: bool) {
    if wrapper.is_null() {
        return;
    }
    // SAFETY: `wrapper` is the live `Direct3DDevice9` that owns the forwarding
    // child; the counter is atomic, so a shared borrow suffices.
    let counter = unsafe {
        &(*wrapper.cast::<Direct3DDevice9>())
            .inner()
            .outstanding_reset_blockers
    };
    if acquired {
        counter.fetch_add(1, Ordering::AcqRel);
    } else {
        counter.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Forward an implicit child object's `Release` to its owning device wrapper.
///
/// Fires on the child's 1→0 transition. Counterpart to
/// [`device_wrapper_add_ref`]; routes through the full [`device_release`]
/// thunk so the device tears down when its last reference — possibly this
/// forwarded one — drops.
pub fn device_wrapper_release(wrapper: *mut c_void) {
    if wrapper.is_null() {
        return;
    }
    device_release(wrapper);
}

/// Minimal `IUnknown` shape for calling Release on the parent `IDirect3D9`.
///
/// Without needing the full vtable type in scope.
#[repr(C)]
struct ParentIUnknown {
    vtbl: *const ParentIUnknownVtbl,
}
#[repr(C)]
struct ParentIUnknownVtbl {
    _query_interface: extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
    add_ref: extern "system" fn(*mut c_void) -> u32,
    release: extern "system" fn(*mut c_void) -> u32,
}

// ── IDirect3DDevice9 methods ──

extern "system" fn device_test_cooperative_level(this: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // No exclusive mode is ever taken, so focus changes never lose the device.
    // The non-OK answers are the failure latch a step the layer could not
    // complete leaves behind (a failed encode, submission or capture) and the
    // DEVICENOTRESET a failed implicit-resource rebuild leaves.
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let object = unsafe { InPtr::<Direct3DDevice9>::opt(this) };
    // D3D9Ex answers `TestCooperativeLevel` with `D3D_OK` on an extended
    // device. Its failure latch reaches the application through
    // `CheckDeviceState` and the presents; a `Reset` the layer could not
    // complete shows only as the DEVICENOTRESET that `Present`, `PresentEx`
    // and the swap chain's `Present` answer.
    if object.as_ref().is_some_and(|obj| obj.inner().is_extended()) {
        return D3D_OK;
    }
    if let Some(obj) = &object
        && let Err(hr) = obj.inner().encoder_status()
    {
        return hr;
    }
    let not_reset = object.is_some_and(|obj| obj.inner().needs_reset());
    if not_reset {
        mtld3d_types::D3DERR_DEVICENOTRESET
    } else {
        D3D_OK
    }
}

extern "system" fn device_get_available_texture_mem(this: *mut c_void) -> u32 {
    // Unified memory has no dedicated-VRAM answer; modern IHV drivers report a
    // configured budget here. Report `BUDGET - live DEFAULT-pool bytes` so the
    // value visibly decreases as the app allocates GPU resources, while
    // staying generous enough never to starve a real workload. 2 GiB budget
    // (fits u32; well above any title's needs).
    //
    // `memory.vramBudgetMB` lowers that ceiling, and defaults to doing so on a
    // 32-bit guest. An engine of this era sizes its streaming pool from what
    // this call reports, so an unrestricted figure invites a title to commit
    // past the process address space; what fails then is not an allocation it
    // could handle but a Metal command buffer, out of memory, whose rendering
    // is discarded.
    const VRAM_BUDGET: u64 = 2 * 1024 * 1024 * 1024;
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return 0;
    };
    let dev = obj.inner();
    let budget = match dev.config().vram_budget_cap_bytes {
        0 => VRAM_BUDGET,
        cap => VRAM_BUDGET.min(cap),
    };
    // A core device's standalone render targets and depth-stencil surfaces are not in `used`:
    // their core creators return before the charge, so only its textures lower the figure.
    let used = dev.vram_bytes_used.load(Ordering::Acquire);
    // An extended device pages its default-pool memory, so its figure does
    // not shrink as the application allocates.
    let available = mtld3d_core::extended::available_texture_mem(budget, used, dev.is_extended());
    // Games size their texture budgets from this call or from DXGI; the
    // one-time line tells which path a title took when its settings menu
    // shows a surprising video-memory figure.
    mtld3d_shared::log_once_info!(
        target: LOG_TARGET,
        "IDirect3DDevice9::GetAvailableTextureMem → {} MiB (first call)",
        available >> 20
    );
    available
}

extern "system" fn device_evict_managed_resources(this: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // Spec contract: "evict managed-pool resources from VRAM; the
    // runtime re-uploads on next use." On unified memory there is no
    // separate VRAM, but games (notably WoW after Release+
    // CreateDevice) call this expecting the re-upload contract to
    // fire. Lazy texture upload makes that trivial: walk every live
    // texture, mark previously-uploaded mips dirty, drop the cache
    // entries — bind-time `flush_dirty_mips` does the rest.
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    obj.inner().evict_managed_resources();
    0 // S_OK
}

extern "system" fn device_get_direct3d(this: *mut c_void, ppd3d9: *mut *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    trace!(target: LOG_TARGET, "IDirect3DDevice9::GetDirect3D()");
    if ppd3d9.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let parent = obj.inner().direct3d as *mut c_void;
    if parent.is_null() {
        warn!(target: LOG_TARGET, "reject GetDirect3D() → INVALIDCALL (no parent)");
        return D3DERR_INVALIDCALL;
    }
    // AddRef per COM rules — caller owns one reference on return.
    // SAFETY: `parent` is non-null (checked above) and is the stashed
    // `Direct3D9*` pointer from `Direct3D9::CreateDevice`; it lives as
    // long as the device wrapper.
    let parent_obj = unsafe { &*(parent as *const ParentIUnknown) };
    // SAFETY: `parent_obj.vtbl` is the `'static`
    // `DIRECT3D9_VTBL` installed when the parent was constructed.
    let vtbl = unsafe { &*parent_obj.vtbl };
    (vtbl.add_ref)(parent);
    // SAFETY: `ppd3d9` is non-null (checked above) and per the D3D9
    // ABI points to a writable `*mut c_void` slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(ppd3d9, parent) };
    D3D_OK
}

extern "system" fn device_get_device_caps(this: *mut c_void, caps: *mut D3DCAPS9) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    trace!(target: LOG_TARGET, "IDirect3DDevice9::GetDeviceCaps()");
    if caps.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let filled = caps::build(
        obj.inner().config().caps_all,
        crate::direct3d9::sampler_border_supported(),
    );
    // SAFETY: `caps` is non-null (checked above) and per the D3D9 ABI points to
    // a writable `D3DCAPS9` slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(caps, filled) };
    0 // S_OK
}

extern "system" fn device_get_display_mode(
    this: *mut c_void,
    swap_chain: u32,
    mode: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    trace!(target: LOG_TARGET, "IDirect3DDevice9::GetDisplayMode(swap_chain={swap_chain})");
    if mode.is_null() || swap_chain != 0 {
        warn!(target: LOG_TARGET, "reject GetDisplayMode(swap_chain={swap_chain}) → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: `mode` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `D3DDISPLAYMODE` slot owned by the caller; alignment is not assumed.
    unsafe {
        OutPtr::write_opt(
            mode.cast::<D3DDISPLAYMODE>(),
            crate::direct3d9::reported_display_mode(obj.inner().present_params()),
        );
    }
    D3D_OK
}

extern "system" fn device_get_creation_parameters(this: *mut c_void, params: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    trace!(target: LOG_TARGET, "IDirect3DDevice9::GetCreationParameters()");
    if params.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: `params` is non-null (checked above) and per the D3D9
    // ABI points to a writable `D3DDEVICE_CREATION_PARAMETERS` slot
    // owned by the caller; alignment is not assumed.
    unsafe {
        OutPtr::write_opt(
            params.cast::<D3DDEVICE_CREATION_PARAMETERS>(),
            D3DDEVICE_CREATION_PARAMETERS {
                adapter_ordinal: obj.inner().creation_adapter,
                device_type: obj.inner().creation_device_type,
                focus_window: obj.inner().creation_focus_window,
                behavior_flags: obj.inner().creation_behavior_flags,
            },
        );
    }
    D3D_OK
}

/// Report resolved back-buffer geometry through the caller's present parameters.
///
/// `CreateDevice`, `CreateAdditionalSwapChain` and `Reset` write the resolved
/// width, height, format and count back and leave every other field alone, so
/// a field the application changes while the call runs (from a `WM_SIZE` the
/// window-mode code sends it, say) keeps the application's value. The caller's
/// struct promises no alignment, so each of the four fields is written on its
/// own with an unaligned store, and no reference to the caller's memory is
/// formed.
///
/// # Safety
/// `present_params` is non-null and writable for one `D3DPRESENT_PARAMETERS`
/// for the call.
pub unsafe fn report_resolved_geometry(
    present_params: *mut c_void,
    resolved: &D3DPRESENT_PARAMETERS,
) {
    let base = present_params.cast::<u8>();
    for (offset, value) in [
        (
            core::mem::offset_of!(D3DPRESENT_PARAMETERS, back_buffer_width),
            resolved.back_buffer_width,
        ),
        (
            core::mem::offset_of!(D3DPRESENT_PARAMETERS, back_buffer_height),
            resolved.back_buffer_height,
        ),
        (
            core::mem::offset_of!(D3DPRESENT_PARAMETERS, back_buffer_format),
            resolved.back_buffer_format,
        ),
        (
            core::mem::offset_of!(D3DPRESENT_PARAMETERS, back_buffer_count),
            resolved.back_buffer_count,
        ),
    ] {
        let field = base.wrapping_add(offset);
        // SAFETY: the caller's contract makes the `u32` field at `offset` writable;
        // alignment is not assumed.
        unsafe { field.cast::<u32>().write_unaligned(value) };
    }
}

extern "system" fn device_create_additional_swap_chain(
    this: *mut c_void,
    present_params: *mut c_void,
    swap_chain: *mut *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // Nowhere to hand the chain to: refused before it is made, since a chain
    // nobody holds would keep its device reference for good.
    if swap_chain.is_null() {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET,
            "reject CreateAdditionalSwapChain(ppSwapChain = NULL) → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    null_out(swap_chain);
    // SAFETY: vtable in/out-param; `present_params` is *mut D3DPRESENT_PARAMETERS
    // per the IDirect3DDevice9 ABI — read for the request, written back with the
    // resolved dimensions/count; alignment is not assumed.
    let Some(pp_in) =
        (unsafe { ValueIn::<D3DPRESENT_PARAMETERS>::read_opt(present_params.cast_const()) })
    else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let mut pp = pp_in;
    // D3D9 only allows windowed additional swapchains, and none at all while
    // the device itself is fullscreen (the implicit swapchain owns the screen).
    if pp.windowed == 0 || dev.is_fullscreen() {
        warn!(
            target: LOG_TARGET,
            "reject CreateAdditionalSwapChain(windowed={}, device_fullscreen={}) → INVALIDCALL",
            pp.windowed, dev.is_fullscreen()
        );
        return D3DERR_INVALIDCALL;
    }
    // The swap effect, back-buffer count and interval follow the rules
    // `CreateDevice` and `Reset` apply for this kind of device.
    if !mtld3d_core::present::present_params_are_valid(&pp, dev.is_extended()) {
        warn!(
            target: LOG_TARGET,
            "reject CreateAdditionalSwapChain: invalid present params (swap_effect={}, \
             bb_count={}, interval={:#x}) → INVALIDCALL",
            pp.swap_effect, pp.back_buffer_count, pp.presentation_interval,
        );
        return D3DERR_INVALIDCALL;
    }
    // Resolve a zero-dimension windowed request against the target window's
    // client rect (the device window when device_window is NULL), and clamp a
    // zero back-buffer count to one — then report both back to the caller.
    let target_window: usize = if pp.device_window != 0 {
        pp.device_window
    } else {
        dev.window()
    };
    crate::direct3d9::resolve_backbuffer_dims(target_window as u64, &mut pp);
    if pp.back_buffer_format == 0 {
        pp.back_buffer_format = crate::direct3d9::adapter_display_format();
    }
    pp.back_buffer_count = pp.back_buffer_count.max(1);
    // SAFETY: `present_params` is the non-null pointer the request was read from, writable
    // per the ABI; alignment is not assumed.
    unsafe { report_resolved_geometry(present_params, &pp) };
    // The stored copy resolves hDeviceWindow so GetPresentParameters reports the
    // real target window even when the caller passed NULL.
    pp.device_window = target_window;
    let sc = crate::swapchain::Direct3DSwapChain9::new(obj.inner_ptr(), pp);
    // SAFETY: vtable out-param; `swap_chain` is *mut *mut c_void per the ABI.
    let sc_ptr = Box::into_raw(Box::new(sc));
    // SAFETY: `sc_ptr` is a freshly created, live additional swapchain at
    // refcount 1.
    unsafe { crate::com_ref::com_register_child(sc_ptr) };
    // SAFETY: vtable out-param; `swap_chain` is *mut *mut c_void per the ABI.
    unsafe { OutPtr::write_opt(swap_chain, sc_ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_get_swap_chain(
    this: *mut c_void,
    swap_chain: u32,
    out: *mut *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    null_out(out);
    // Only the implicit swapchain (index 0) is exposed; GetSwapChain never
    // returns CreateAdditionalSwapChain results.
    if swap_chain != 0 {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // The implicit swapchain is device-owned and created once: the app may keep
    // using it after its own Release, so a fresh per-call object would dangle.
    let sc_ptr = obj.inner().get_or_create_implicit_swapchain();
    // AddRef for the caller — they own one reference on return. The engine's
    // forwarding AddRef bumps the device on the implicit swapchain's 0→1
    // transition, so the first `GetSwapChain` raises the device refcount (D3D9
    // implicit-object model) while subsequent calls on a live swapchain do not.
    // SAFETY: `sc_ptr` is the live, device-owned implicit swapchain just
    // created or cached above.
    unsafe { crate::com_ref::com_add_ref::<crate::swapchain::Direct3DSwapChain9>(sc_ptr.cast()) };
    // SAFETY: vtable out-param; `out` is *mut *mut c_void per the ABI.
    unsafe { OutPtr::write_opt(out, sc_ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_get_number_of_swap_chains(this: *mut c_void) -> u32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    trace!(target: LOG_TARGET, "IDirect3DDevice9::GetNumberOfSwapChains()");
    1
}

extern "system" fn device_reset(this: *mut c_void, present_params: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    let hr = reset_impl(this, present_params, ResetCall::Reset);
    reported_state_code(this, hr)
}

/// The code the device behind `this` reports for a call that produced `hr`.
///
/// [`DeviceInner::state_code`] for a live device, `hr` itself otherwise.
fn reported_state_code(this: *mut c_void, hr: i32) -> i32 {
    // SAFETY: vtable thunk argument; `this` is null or *mut Direct3DDevice9
    // per the IDirect3DDevice9 ABI.
    unsafe { InPtr::<Direct3DDevice9>::opt(this) }.map_or(hr, |obj| obj.inner().state_code(hr))
}

/// Which entry point a `Reset` came through.
#[derive(Clone, Copy)]
pub enum ResetCall {
    /// The base `Reset` slot, on either kind of device.
    Reset,
    /// `ResetEx`, with the size of the display mode it named, if any.
    ResetEx { mode: Option<(u32, u32)> },
}

/// What every line one `Reset` writes shares.
struct ResetLog {
    /// `IDirect3DDevice9` on a plain device, `IDirect3DDevice9Ex` on an extended one.
    interface: &'static str,
    method: ResetMethod,
    /// The back-buffer size before the call.
    old_size: (u32, u32),
    /// The device was waiting for a successful `Reset` when the call came.
    recovering: bool,
}

impl ResetLog {
    /// The line for this call with `pp` as resolved so far and its `outcome`.
    const fn summary<'a, O: core::fmt::Display>(
        &self,
        pp: &'a mtld3d_types::D3DPRESENT_PARAMETERS,
        outcome: O,
    ) -> ResetSummary<'a, O> {
        ResetSummary::new(self.interface, self.old_size, pp, outcome).via(self.method)
    }
}

/// The body `Reset` and `ResetEx` share; the caller holds the API lock.
///
/// A plain device's `Reset` refuses while the application references a
/// default-pool resource or an implicit surface, returns every state to its
/// default, and leaves a rejected or failed device requiring another `Reset`.
/// An extended device's skips the reference check, keeps its state, its
/// default-pool resources and an open scene, rebinds only its targets and the
/// viewport extent, and answers a rejected request without changing anything.
fn reset_impl(this: *mut c_void, present_params: *mut c_void, call: ResetCall) -> i32 {
    // SAFETY: vtable in/out-param; per the D3D9 ABI `present_params` points to a
    // readable+writable `D3DPRESENT_PARAMETERS` owned by the caller (Reset
    // resolves and reports the effective geometry back through it; alignment is
    // not assumed).
    let Some(pp_in) =
        (unsafe { ValueIn::<D3DPRESENT_PARAMETERS>::read_opt(present_params.cast_const()) })
    else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // What the line a Reset writes compares against, and whether the device
    // already waits for a successful Reset: a game retries a failing one
    // every frame, and a success then is a recovery.
    let log = ResetLog {
        interface: if dev.is_extended() {
            "IDirect3DDevice9Ex"
        } else {
            "IDirect3DDevice9"
        },
        method: match call {
            ResetCall::Reset => ResetMethod::Reset,
            ResetCall::ResetEx { .. } => ResetMethod::ResetEx,
        },
        old_size: (dev.backbuffer_width, dev.backbuffer_height),
        recovering: dev.needs_reset(),
    };
    let previous = *dev.present_params();
    // `ResetEx` names a display mode exactly when it asks for fullscreen, of
    // the back buffer's size; one that disagrees is refused before anything
    // else is looked at.
    if let ResetCall::ResetEx { mode } = call
        && !mtld3d_core::extended::reset_ex_mode_valid(
            pp_in.windowed != 0,
            mode,
            (pp_in.back_buffer_width, pp_in.back_buffer_height),
        )
    {
        let pp = pp_in;
        let reason = match mode {
            Some((width, height)) if pp.windowed != 0 => {
                format!("a windowed request names display mode {width}x{height}")
            }
            Some((width, height)) => format!(
                "display mode {width}x{height} is not the back buffer's {}x{}",
                pp.back_buffer_width, pp.back_buffer_height
            ),
            None => String::from("a fullscreen request names no display mode"),
        };
        log_failed_reset(
            dev,
            &log,
            &pp,
            format_args!("rejected, {reason}"),
            ResetFailureEffect::Unchanged,
        );
        return D3DERR_INVALIDCALL;
    }
    if let Err(hr) = dev.encoder_status() {
        return hr;
    }

    // Resolve the request on a local copy. A windowed Reset may pass zero
    // dimensions ("use the device window's client rect") and
    // `D3DFMT_UNKNOWN` ("use the display format"); a zero back-buffer count
    // resolves to one. D3D9 reports the resolved values back to the caller.
    let mut pp = pp_in;
    // Reject invalid swap-effect / back-buffer-count / presentation-interval
    // before touching any device state, so a rejected Reset leaves the device
    // intact and resettable.
    let extended = dev.is_extended();
    if !mtld3d_core::present::present_params_are_valid(&pp, extended) {
        log_failed_reset(
            dev,
            &log,
            &pp,
            format_args!(
                "rejected, the swap effect, back buffer count or presentation interval is invalid"
            ),
            ResetFailureEffect::Unchanged,
        );
        return D3DERR_INVALIDCALL;
    }
    // A Reset with well-formed parameters ends an open `BeginStateBlock`
    // recording whether or not it goes on to succeed, before it looks for
    // outstanding references: the recording dies with the state it was
    // recording against. Every rejection from here on goes through
    // `reject_reset`, which restores the state defaults as well. An extended
    // device keeps the recording, as it keeps the state it records against.
    if !extended {
        dev.recording_state_block = None;
    }
    // A fullscreen request must still be well-formed even though its size is
    // not used: the D3D9 "zero means the client area" rule is windowed-only,
    // so zero dimensions here are a malformed request. Checked before the
    // window moves, so a rejected Reset leaves the window as it was.
    if pp.windowed == 0 && (pp.back_buffer_width == 0 || pp.back_buffer_height == 0) {
        log_failed_reset(
            dev,
            &log,
            &pp,
            format_args!("rejected, a fullscreen request may not carry zero dimensions"),
            ResetFailureEffect::Unchanged,
        );
        return reject_reset(dev);
    }
    // Reset rejects any outstanding app reference to a `D3DPOOL_DEFAULT`
    // resource or an implicit surface: those are backed by the device memory
    // the Reset recreates, and D3D9 makes the app release them first. A state
    // block that holds such a resource keeps it outstanding too, since the
    // resource lives as long as the block; the device's bindings do not count,
    // so they need not be released before the count is read.
    // An extended device keeps its default-pool resources and implicit
    // surfaces alive across `Reset`, so nothing it holds blocks one.
    let blockers = dev.outstanding_reset_blockers.load(Ordering::Acquire);
    if blockers != 0 && !extended {
        log_failed_reset(
            dev,
            &log,
            &pp,
            format_args!(
                "rejected, {blockers} D3DPOOL_DEFAULT resource(s) or implicit surface(s) still \
                 referenced"
            ),
            ResetFailureEffect::Unchanged,
        );
        return reject_reset(dev);
    }
    if pp.windowed != 0 && pp.back_buffer_format == 0 {
        pp.back_buffer_format = crate::direct3d9::adapter_display_format();
    }
    crate::direct3d9::warn_unsupported_backbuffer_format(pp.back_buffer_format);
    // `Reset` re-specifies the swap chain, multisample configuration
    // included. Its validation depends on the format and device caps alone,
    // so reject it before changing the window or display mode.
    let Ok(new_sample_count) = mtld3d_core::multisample::resolve_sample_count(
        pp.multi_sample_type,
        pp.multi_sample_quality,
        pp.back_buffer_format,
        crate::direct3d9::device_caps_flags(),
    )
    .map(|count| u8::try_from(count).expect("sample count <= 16 fits u8")) else {
        log_failed_reset(
            dev,
            &log,
            &pp,
            format_args!(
                "rejected, the multisample type and quality are not available on this back-buffer \
                 format"
            ),
            ResetFailureEffect::Unchanged,
        );
        return reject_reset(dev);
    };
    // The auto depth-stencil format answers to the same depth and multisample
    // rules as at `CreateDevice`, also before the window or the mode moves.
    if pp.enable_auto_depth_stencil != 0
        && pp.auto_depth_stencil_format != 0
        && !mtld3d_core::multisample::auto_depth_stencil_accepts(
            pp.auto_depth_stencil_format,
            pp.multi_sample_type,
            pp.multi_sample_quality,
            crate::direct3d9::device_caps_flags(),
        )
    {
        log_failed_reset(
            dev,
            &log,
            &pp,
            format_args!(
                "rejected, the auto depth-stencil format at this multisample type is no \
                 depth-stencil the device offers"
            ),
            ResetFailureEffect::Unchanged,
        );
        return reject_reset(dev);
    }
    if !matches!(
        pp.back_buffer_format,
        mtld3d_types::D3DFMT_X8R8G8B8 | mtld3d_types::D3DFMT_A8R8G8B8
    ) {
        crate::device::refuse_under_core(
            "IDirect3DDevice9::Reset (a back-buffer format the core does not make)",
        );
        return mtld3d_types::D3DERR_NOTAVAILABLE;
    }
    let target_window = if pp.device_window == 0 {
        dev.window()
    } else {
        pp.device_window
    };
    let retargeted = target_window != dev.window();
    if !resolve_reset_window_mode(dev, target_window, &mut pp) {
        log_failed_reset(
            dev,
            &log,
            &pp,
            format_args!("rejected, no usable windowed client area"),
            ResetFailureEffect::Unchanged,
        );
        return reject_reset(dev);
    }
    pp.back_buffer_count = pp.back_buffer_count.max(1);
    warn_present_params_fields_once(&pp);

    // Report resolved geometry only after validation. The caller's window
    // and mode flags stay as supplied.
    // SAFETY: `present_params` is the non-null pointer the request was read from, writable
    // per the ABI; alignment is not assumed.
    unsafe { report_resolved_geometry(present_params, &pp) };

    // Captured before the configuration below changes what the held
    // surfaces resolve to.
    let held = dev.hold_implicit_surfaces();
    let detached = (held.back_buffer.is_some(), held.depth.is_some());
    let multi_sample_changed = new_sample_count != dev.backbuffer_sample_count;
    dev.set_backbuffer_multi_sample(
        pp.multi_sample_type,
        pp.multi_sample_quality,
        new_sample_count,
    );
    let missing_back_buffer = !dev.core.has_back_buffer();
    let recreated_for_a_change = pp.back_buffer_width != dev.backbuffer_width
        || pp.back_buffer_height != dev.backbuffer_height
        || multi_sample_changed
        || missing_back_buffer;
    let resized = recreated_for_a_change || held.back_buffer.is_some() || held.depth.is_some();
    let new_depth_format = if pp.enable_auto_depth_stencil != 0 {
        pp.auto_depth_stencil_format
    } else {
        0
    };

    if resized {
        // reset_recreate_resources rebuilds the depth from depth_stencil_format,
        // so adopt the new auto-DS format before it runs.
        dev.depth_stencil_format = new_depth_format;
        if let Err(hr) = reset_recreate_resources(dev, &pp, held) {
            log_failed_reset(
                dev,
                &log,
                &pp,
                format_args!("failed recreating the back buffer ({hr:#010x})"),
                ResetFailureEffect::StateKeptResetOwed,
            );
            return fail_reset(dev, hr);
        }
    } else if let Err(hr) = reconcile_implicit_depth(dev, new_depth_format) {
        log_failed_reset(
            dev,
            &log,
            &pp,
            format_args!("failed recreating the auto depth-stencil ({hr:#010x})"),
            ResetFailureEffect::StateKeptResetOwed,
        );
        return fail_reset(dev, hr);
    }
    if retargeted {
        retarget_device_window(dev, target_window);
    }

    // Refresh the device + cached implicit swapchain present parameters.
    let mut stored = pp;
    if stored.device_window == 0 {
        stored.device_window = dev.window();
    }
    dev.set_present_params(stored);
    dev.core
        .set_discard_back_buffer(stored.swap_effect == mtld3d_types::D3DSWAPEFFECT_DISCARD);
    dev.core.reset_clear_history();

    if retargeted {
        dev.reapply_gamma();
    }

    // Reset device state to D3D9 defaults. Cursor + silent-write
    // warn latches survive (per-spec / process-lifetime telemetry). An
    // extended device keeps its state and rebinds only its targets.
    if extended {
        dev.rebind_after_extended_reset();
    } else {
        dev.reset_to_defaults();
    }
    dev.flags.remove(DeviceFlags::NOT_RESET);

    let mut done = ResetDone::empty();
    done.set(ResetDone::RESIZED, resized);
    done.set(ResetDone::RECOVERED, log.recovering);
    done.set(ResetDone::EXTENDED, extended);
    done.set(ResetDone::BACK_BUFFER_DETACHED, detached.0);
    done.set(ResetDone::DEPTH_DETACHED, detached.1);
    done.set(
        ResetDone::FAILURES_ENDED,
        !log.recovering && dev.flags.contains(DeviceFlags::RESET_FAILURE_LOGGED),
    );
    dev.flags.remove(DeviceFlags::RESET_FAILURE_LOGGED);
    let changed = recreated_for_a_change || changes_presentation(&previous, &stored);
    let level = if success_logs_at_info(changed, done) {
        log::Level::Info
    } else {
        log::Level::Debug
    };
    log::log!(target: LOG_TARGET, level, "{}", log.summary(&pp, done));
    D3D_OK
}

/// Log a `Reset` that did not complete, with the request as far as it was resolved.
///
/// At warn for the first failure since the device last reset successfully,
/// and at debug after it: a game retries a failing `Reset` every frame, and
/// the first line already said why it fails. A plain device is past its
/// first failure while it waits for a successful `Reset`; a rejection of
/// malformed present parameters leaves it as it was, so that one warns each
/// time unless an earlier failure is still pending. An extended device owes
/// no `Reset` after a rejection, so it marks its first warning instead and
/// keeps later rejections at debug, but a failure that leaves it needing a
/// `Reset` warns regardless (`failure_warns`). Its line ends with `effect`,
/// what the failure left behind.
fn log_failed_reset(
    dev: &mut DeviceInner,
    log: &ResetLog,
    pp: &mtld3d_types::D3DPRESENT_PARAMETERS,
    reason: core::fmt::Arguments<'_>,
    effect: ResetFailureEffect,
) {
    let extended = dev.is_extended();
    let failure_logged = dev.flags.contains(DeviceFlags::RESET_FAILURE_LOGGED);
    let level = if failure_warns(log.recovering, failure_logged, &effect) {
        log::Level::Warn
    } else {
        log::Level::Debug
    };
    let effect = if extended {
        dev.flags.insert(DeviceFlags::RESET_FAILURE_LOGGED);
        effect
    } else {
        ResetFailureEffect::Unstated
    };
    log::log!(
        target: LOG_TARGET,
        level,
        "{}",
        log.summary(pp, ResetFailed::new(reason, effect)),
    );
}

/// Answer a rejected `Reset`: on a plain device, with the device state at its defaults.
///
/// An extended device answers `D3DERR_INVALIDCALL` and changes nothing: it
/// keeps its state, bindings, scene and frame, and owes no `Reset`. On a
/// plain device D3D9 resets the device state before it checks what can make a
/// `Reset` fail, so a rejected one still releases every binding and returns
/// every state to its default, leaving what a successful `Reset` leaves: render
/// target 0 on the back buffer, the depth stencil on the implicit surface,
/// no recording and no open scene. The frame in flight is delivered first,
/// as a same-size `Reset` does, and [`fail_reset`] replaces it, so the
/// attachments it records follow the defaults as well; without that, a draw
/// after the rejection would be built for the back buffer while its pass
/// still carried the targets the application had bound.
fn reject_reset(dev: &mut DeviceInner) -> i32 {
    if dev.is_extended() {
        return D3DERR_INVALIDCALL;
    }
    fail_reset(dev, D3DERR_INVALIDCALL)
}

/// End a failed `Reset`, answering `hr`.
fn fail_reset(dev: &mut DeviceInner, hr: i32) -> i32 {
    dev.flags.insert(DeviceFlags::NOT_RESET);
    if dev.is_extended() {
        dev.rebind_after_extended_reset();
    } else {
        dev.reset_to_defaults();
    }
    hr
}

/// Resolve geometry after the window transition, undoing a rejected fullscreen exit.
///
/// Only windowed requests can still have zero dimensions here. Leaving
/// fullscreen must precede their client-area query, but the fullscreen
/// session stays owned until that query has produced a usable extent.
fn resolve_reset_window_mode(
    dev: &mut DeviceInner,
    target: usize,
    pp: &mut mtld3d_types::D3DPRESENT_PARAMETERS,
) -> bool {
    if pp.windowed != 0
        && (pp.back_buffer_width == 0 || pp.back_buffer_height == 0)
        && let Some(saved) = dev.fullscreen.as_ref()
    {
        let accepted = crate::fullscreen::try_leave(saved, dev.leave_kind(), || {
            crate::direct3d9::resolve_backbuffer_dims(target as u64, pp);
            pp.back_buffer_width != 0 && pp.back_buffer_height != 0
        });
        if accepted {
            dev.fullscreen = None;
        }
        return accepted;
    }
    apply_reset_window_mode(dev, pp);
    crate::direct3d9::resolve_backbuffer_dims(target as u64, pp);
    pp.back_buffer_width != 0 && pp.back_buffer_height != 0
}

/// Carry out the windowed/fullscreen transition a `Reset` asks for.
///
/// All four combinations are legal and a game picks freely between them:
/// entering fullscreen, leaving it, staying fullscreen (possibly on a new
/// device window), and staying windowed. A refused display mode uses the
/// existing monitor-covering fallback.
fn apply_reset_window_mode(dev: &mut DeviceInner, pp: &mtld3d_types::D3DPRESENT_PARAMETERS) {
    if pp.windowed != 0 {
        dev.leave_fullscreen(&mtld3d_core::fullscreen_log::LeaveReason::WindowedReset);
        return;
    }
    let target = if pp.device_window == 0 {
        dev.window()
    } else {
        pp.device_window
    } as *mut c_void;
    let mode = crate::direct3d9::fullscreen_mode_request(pp);
    if dev.fullscreen_window() == Some(target) {
        dev.update_fullscreen(mode);
        return;
    }
    // A Reset may retarget the device at another window. The session is
    // handed over rather than given back: the window left behind keeps the
    // borderless style and the monitor rect it was put in, since the app that
    // retargeted its device did not ask for a screen to be uncovered behind
    // it, and the new window is taken over exactly as a fullscreen create
    // takes one over, so the eventual leave gives back the window the device
    // presents into.
    dev.enter_fullscreen(target, mode);
}

/// Move the presentation surface and the cursor subclass to a new device window.
fn retarget_device_window(dev: &mut DeviceInner, hwnd: usize) {
    if !dev.core.retarget(isize::try_from(hwnd).unwrap_or_default()) {
        warn!(
            target: LOG_TARGET,
            "Reset: no Metal layer on the new device window 0x{hwnd:x}; the device presents \
             nowhere until a later Reset attaches one",
        );
    }
    let (cursor_scale, _origin) =
        crate::direct3d9::resolve_cursor_scale(dev.core.backing_scale(), dev.config.cursor_scale);
    let dev_ptr = std::ptr::from_mut::<DeviceInner>(dev);
    dev.cursor.retarget(hwnd as *mut c_void, dev_ptr);
    dev.cursor.follow_scale(cursor_scale);
}

/// Steps 1-6 of the Reset protocol.
fn reset_recreate_resources(
    dev: &mut DeviceInner,
    pp: &mtld3d_types::D3DPRESENT_PARAMETERS,
    held: HeldImplicitSurfaces,
) -> Result<(), i32> {
    dev.detach_held_implicit_surfaces(held);
    dev.set_backbuffer_dims(pp.back_buffer_width, pp.back_buffer_height);
    let (samples, format) = (dev.backbuffer_sample_count, dev.depth_stencil_format);
    let (render_w, render_h) = dev
        .render_scale
        .extent(pp.back_buffer_width, pp.back_buffer_height);
    let resized = dev.core.resize_scaled(
        render_w,
        render_h,
        pp.back_buffer_width,
        pp.back_buffer_height,
        samples,
    );
    dev.encoder_status()?;
    if !resized {
        if let Some(id) = dev.core_auto_depth.take() {
            dev.core.forget_texture(id);
        }
        dev.depth_stencil_format = 0;
        return Err(D3DERR_INVALIDCALL);
    }
    remake_core_auto_depth(dev, format)
}

/// Reconcile the implicit depth-stencil to `new_depth_format` (0 = none).
fn reconcile_implicit_depth(dev: &mut DeviceInner, new_depth_format: u32) -> Result<(), i32> {
    let had_depth = dev.core_auto_depth.is_some();
    if new_depth_format == dev.depth_stencil_format && (new_depth_format != 0) == had_depth {
        return Ok(());
    }
    remake_core_auto_depth(dev, new_depth_format)
}

/// Makes the auto depth-stencil again for a `Reset`, of `format` (0 = none).
///
/// The old one is forgotten and a new one created at the back buffer size and
/// sample count.
fn remake_core_auto_depth(dev: &mut DeviceInner, format: u32) -> Result<(), i32> {
    let old = dev.core_auto_depth.take();
    dev.depth_stencil_format = 0;
    let (width, height, samples, scale) = (
        dev.backbuffer_width,
        dev.backbuffer_height,
        dev.backbuffer_sample_count,
        dev.render_scale,
    );
    let (render_w, render_h) = scale.extent(width, height);
    if let Some(id) = old {
        dev.core.forget_texture(id);
    }
    if format == 0 {
        return Ok(());
    }
    let Some(pixel_format) = mtld3d_core::format::map_d3d_depth_format(format) else {
        error!(target: LOG_TARGET, "Reset: AutoDepthStencilFormat {format} has no Metal mapping");
        return Err(D3DERR_INVALIDCALL);
    };
    let id = mtld3d_core::ids::TextureId::new_unique();
    let desc = crate::rt::device::CoreTextureDesc {
        pixel_format: pixel_format as u32,
        kind: crate::rt::device::TextureKind::Tex2d,
        width: render_w,
        height: render_h,
        depth: 1,
        levels: 1,
        flags: crate::rt::device::CoreTextureFlags::RENDER_TARGET,
        srgb_format: 0,
        block: (1, 1, 4),
        samples,
    };
    dev.core.texture_create(id, &desc);
    if !scale.is_identity() {
        dev.core.set_texture_logical_size(id, width, height);
    }
    dev.set_core_auto_depth(id, format);
    Ok(())
}

extern "system" fn device_present(
    this: *mut c_void,
    src_rect: *const c_void,
    dst_rect: *const c_void,
    dst_window_override: *mut c_void,
    dirty_region: *const c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Frame);
    present_impl(
        this,
        src_rect,
        dst_rect,
        dst_window_override,
        dirty_region,
        0,
    )
}

/// The body `Present` and `PresentEx` share; the caller holds the API lock.
///
/// Every argument but `this` is accepted and ignored, each warned once.
fn present_impl(
    this: *mut c_void,
    src_rect: *const c_void,
    dst_rect: *const c_void,
    dst_window_override: *mut c_void,
    dirty_region: *const c_void,
    flags: u32,
) -> i32 {
    warn_ignored_present_arguments(
        src_rect,
        dst_rect,
        !dst_window_override.is_null(),
        dirty_region,
    );
    warn_ignored_present_flags(flags);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();

    mtld3d_shared::crumb!("d3d9:present");
    let hr = dev.present();
    dev.state_code(hr)
}

/// Warn once for each `D3DPRESENT_*` flag bit a present carries, none of which is honoured.
///
/// The frame presents as it would without the flag: `DONOTWAIT` still waits,
/// `FORCEIMMEDIATE` still follows the presentation interval, and the overlay
/// bits have no overlay to act on. Shared by `PresentEx` and
/// `IDirect3DSwapChain9::Present`.
pub fn warn_ignored_present_flags(flags: u32) {
    for bit in mtld3d_core::extended::present_flag_bits(flags) {
        mtld3d_shared::log_once_warn_by!(
            target: LOG_TARGET,
            key: u64::from(bit),
            "Present: {} ({bit:#x}) is not honoured; the frame presents as it would without it",
            mtld3d_core::extended::present_flag_name(bit)
        );
    }
}

/// Warn once for each optional `Present` argument, none of which is honoured.
///
/// Presentation always shows the whole back buffer across the device window:
/// a source or destination rect, a destination window override and a dirty
/// region are accepted and ignored. Shared by `IDirect3DDevice9::Present` and
/// `IDirect3DSwapChain9::Present`.
#[inline]
pub fn warn_ignored_present_arguments(
    source_rect: *const c_void,
    dest_rect: *const c_void,
    window_override: bool,
    dirty_region: *const c_void,
) {
    if source_rect.is_null() && dest_rect.is_null() && !window_override && dirty_region.is_null() {
        return;
    }
    warn_present_arguments(source_rect, dest_rect, window_override, dirty_region);
}

/// The warnings themselves, out of line: nearly every `Present` passes no optional argument.
#[cold]
#[inline(never)]
fn warn_present_arguments(
    source_rect: *const c_void,
    dest_rect: *const c_void,
    window_override: bool,
    dirty_region: *const c_void,
) {
    if !source_rect.is_null() {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET,
            "Present: a source rect is ignored; the whole back buffer is presented");
    }
    if !dest_rect.is_null() {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET,
            "Present: a destination rect is ignored; the frame fills the device window");
    }
    if window_override {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET,
            "Present: a destination window override is ignored; the frame goes to the device \
             window");
    }
    if !dirty_region.is_null() {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET,
            "Present: a dirty region is ignored; the whole back buffer is presented");
    }
}

extern "system" fn device_get_back_buffer(
    this: *mut c_void,
    swap_chain: u32,
    back_buffer: u32,
    _type_: u32,
    surface: *mut *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    trace!(
        target: LOG_TARGET,
        "IDirect3DDevice9::GetBackBuffer(swap_chain={swap_chain}, back_buffer={back_buffer})"
    );
    if surface.is_null() || swap_chain != 0 {
        warn!(
            target: LOG_TARGET,
            "reject GetBackBuffer(swap_chain={swap_chain}, back_buffer={back_buffer}) → INVALIDCALL"
        );
        null_out(surface);
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        null_out(surface);
        return D3DERR_INVALIDCALL;
    };
    let inner = obj.inner();
    // We model one Metal drawable, so every in-range `back_buffer` index aliases
    // the single backbuffer. An out-of-range index fails like D3D9 — e.g. index
    // 1 on a single-buffered swapchain, or any index on a 3-buffered one beyond
    // its count — instead of being silently clamped to 0.
    if back_buffer >= inner.present_params().back_buffer_count {
        mtld3d_shared::log_once_warn_by!(
            target: LOG_TARGET, key: u64::from(back_buffer),
            "reject GetBackBuffer(back_buffer={back_buffer}) ≥ count {} → INVALIDCALL",
            inner.present_params().back_buffer_count
        );
        null_out(surface);
        return D3DERR_INVALIDCALL;
    }
    // The backbuffer IS the implicit render target (`GetRenderTarget(0) ==
    // GetBackBuffer(0)`): return the same device-owned cached surface.
    let surf = inner.get_or_create_implicit_render_target();
    // SAFETY: `surf` is the live cached implicit RT surface; its AddRef thunk
    // forwards to the device on the 0→1 transition.
    let add_ref = unsafe { (*surf).vtbl().add_ref };
    // SAFETY: calling the surface's AddRef thunk; D3D9 mandates AddRef on return.
    unsafe { add_ref(surf.cast::<c_void>()) };
    // SAFETY: vtable out-param; `surface` is *mut *mut c_void per IDirect3DDevice9 ABI; alignment
    // is not assumed.
    unsafe { OutPtr::write_opt(surface, surf.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_get_raster_status(
    this: *mut c_void,
    _swap_chain: u32,
    _status: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "stub IDirect3DDevice9::GetRasterStatus → INVALIDCALL");
    D3DERR_INVALIDCALL
}

extern "system" fn device_set_dialog_box_mode(this: *mut c_void, _enable: i32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "stub IDirect3DDevice9::SetDialogBoxMode → INVALIDCALL");
    D3DERR_INVALIDCALL
}

extern "system" fn device_set_gamma_ramp(
    this: *mut c_void,
    swap_chain: u32,
    flags: u32,
    ramp: *const c_void,
) {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // The call returns nothing, so every rejection below is a warn and a
    // no-op: there is no channel to answer through.
    if ramp.is_null() {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "SetGammaRamp: null ramp → ignored"
        );
        return;
    }
    if swap_chain != 0 {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "SetGammaRamp: only the implicit swap chain carries a ramp; \
             swap chain {swap_chain} is ignored"
        );
        return;
    }
    if flags & mtld3d_types::D3DSGR_CALIBRATE != 0 {
        // The cap that promises calibration, D3DCAPS2_CANCALIBRATEGAMMA, is
        // not advertised, so applying the ramp as given is the conformant
        // answer to the flag rather than a shortcut.
        mtld3d_shared::log_once_info!(
            target: crate::LOG_TARGET,
            "SetGammaRamp: D3DSGR_CALIBRATE without CANCALIBRATEGAMMA → the ramp is \
             applied uncalibrated"
        );
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return;
    };
    // SAFETY: per the ABI `ramp` points at one readable `D3DGAMMARAMP` for
    // the call; it is non-null per the branch above and copied, never retained.
    // `read_unaligned` assumes no alignment: the ramp is not `Copy`, so it does
    // not go through `ValueIn`.
    let ramp = unsafe { core::ptr::read_unaligned(ramp.cast::<D3DGAMMARAMP>()) };
    let ramp = &ramp;
    if !mtld3d_core::gamma::is_usable(ramp) {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "SetGammaRamp: no channel of the ramp rises end to end → ignored, the \
             display keeps the ramp it has"
        );
        return;
    }
    let dev = obj.inner();
    dev.set_gamma_ramp(ramp);
}

extern "system" fn device_get_gamma_ramp(this: *mut c_void, swap_chain: u32, ramp: *mut c_void) {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if ramp.is_null() {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "GetGammaRamp: null ramp → ignored"
        );
        return;
    }
    if swap_chain != 0 {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "GetGammaRamp: only the implicit swap chain carries a ramp; \
             swap chain {swap_chain} answers nothing"
        );
        return;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return;
    };
    // SAFETY: per the ABI `ramp` points at one writable `D3DGAMMARAMP` for
    // the call; it is non-null per the branch above, and alignment is not assumed.
    unsafe { OutPtr::write_opt(ramp.cast::<D3DGAMMARAMP>(), obj.inner().gamma_ramp()) };
}

extern "system" fn device_create_texture(
    this: *mut c_void,
    width: u32,
    height: u32,
    levels: u32,
    usage: u32,
    format: u32,
    pool: u32,
    texture: *mut *mut c_void,
    shared_handle: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    create_texture_path(&TextureCreateArgs {
        this,
        width,
        height,
        levels,
        usage,
        format,
        pool,
        texture,
        shared_handle,
        offscreen_plain: false,
    })
}

/// Vtable-shaped args bundle for [`create_texture_path`].
#[derive(Clone, Copy)]
struct TextureCreateArgs {
    this: *mut c_void,
    width: u32,
    height: u32,
    levels: u32,
    usage: u32,
    format: u32,
    pool: u32,
    texture: *mut *mut c_void,
    shared_handle: *mut c_void,
    /// The texture backs a `CreateOffscreenPlainSurface` surface.
    ///
    /// Carried in by the caller rather than stamped on the finished texture:
    /// the flag gates the staging-drop loop that runs inside the texture
    /// constructor, and a plain keeps its staging because the game may lock it.
    offscreen_plain: bool,
}

/// Resolve a create's `Levels` argument against the chain its extent allows.
///
/// Shared by the colour, depth, cube and volume create paths, which all take
/// `0` as a request for the full chain and any other value as the level count.
/// A count above the natural chain is capped at it rather than refused: the
/// levels past the chain would each repeat the last one, and D3D9 hands the
/// caller the chain its dimensions do have. Passing such a count through is
/// not an option either, because Metal's descriptor validation refuses a
/// `mipmapLevelCount` above `floor(log2(max_dim)) + 1` with an abort rather
/// than a returned error. The clamp is warned once per distinct
/// `(requested, natural)` pair, naming the entry point the first request came
/// through.
fn resolve_create_levels(entry_point: &str, requested: u32, natural: u32) -> u32 {
    if requested > natural {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: (u64::from(requested) << 32) | u64::from(natural),
            "{entry_point} levels={requested} is past the {natural}-level chain the extent allows; \
             clamped to {natural}"
        );
    }
    resolve_mip_levels(requested, natural)
}

/// Refuse a create whose system-memory copy the process cannot allocate.
///
/// The staging a resource gets at creation is the one allocation that may
/// fail; D3D9 answers such a create `E_OUTOFMEMORY` and hands back no object.
/// What was allocated before the failure has already been dropped by the
/// caller.
fn refuse_unallocatable_staging(entry_point: &str, out: *mut *mut c_void) -> i32 {
    warn!(target: LOG_TARGET,
        "reject {entry_point}: the resource's system-memory copy cannot be allocated → \
         E_OUTOFMEMORY");
    null_out(out);
    mtld3d_types::E_OUTOFMEMORY
}

/// The body of `CreateTexture`, plus the intent the vtable signature cannot carry.
///
/// `CreateOffscreenPlainSurface` backs a `D3DPOOL_DEFAULT` plain with a
/// texture created through here, so both entry points share one create path.
fn create_texture_path(info: &TextureCreateArgs) -> i32 {
    let TextureCreateArgs {
        this,
        width,
        height,
        levels,
        usage,
        format,
        pool,
        texture,
        shared_handle,
        offscreen_plain,
    } = *info;
    trace!(
        target: LOG_TARGET,
        "IDirect3DDevice9::CreateTexture({width}x{height}, levels={levels}, usage={usage:#x}, format={format})"
    );

    warn_unused_usage_and_pool_once("Texture", usage, pool);

    if width == 0 || height == 0 || texture.is_null() {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // A plain device refuses a non-null pSharedHandle with E_NOTIMPL; an
    // extended one reads it as user memory or a shared resource.
    let kind = if offscreen_plain {
        mtld3d_core::extended::CreateKind::OffscreenPlain
    } else {
        mtld3d_core::extended::CreateKind::Texture { levels }
    };
    let user_memory = match shared_handle_gate(this, &kind, pool, shared_handle, "CreateTexture") {
        Ok(memory) => memory,
        Err(hr) => {
            null_out(texture);
            return hr;
        }
    };
    let Some(pool) = create_pool(this, &kind, pool, "CreateTexture") else {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    };
    // D3DUSAGE_WRITEONLY is a vertex/index-buffer-only flag; on a texture it is
    // INVALIDCALL.
    if usage & D3DUSAGE_WRITEONLY != 0 {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }

    // D3D9 usage/pool rules: RENDERTARGET and
    // DEPTHSTENCIL textures must be D3DPOOL_DEFAULT, the two usages are mutually
    // exclusive, and DYNAMIC cannot combine with either. (The format-specific
    // rules — RT only on a colour format, DS only on a depth format — are
    // enforced by the colour/depth create paths.) WoW's RT/DS/dynamic textures
    // are all DEFAULT-pool and single-usage, so this never fires in-game.
    let usage_rt = usage & D3DUSAGE_RENDERTARGET != 0;
    let usage_ds = usage & D3DUSAGE_DEPTHSTENCIL != 0;
    if (usage_rt && usage_ds)
        || ((usage_rt || usage_ds) && pool != D3DPOOL_DEFAULT)
        || (usage & D3DUSAGE_DYNAMIC != 0 && (usage_rt || usage_ds))
    {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // Format-vs-usage mismatch: a colour format
    // cannot carry D3DUSAGE_DEPTHSTENCIL and a depth format cannot carry
    // D3DUSAGE_RENDERTARGET. WoW pairs RT with colour formats and DS with depth
    // formats, so this never fires in-game.
    let is_depth_fmt = mtld3d_core::format::is_depth_format(format);
    if (usage_ds && !is_depth_fmt) || (usage_rt && is_depth_fmt) {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }

    // Past the extent the device reports, a texture Metal backs is refused:
    // Metal itself aborts the process on a descriptor past its own limit. A
    // system-memory or scratch texture has no Metal texture until it is bound
    // for sampling, and creates at any extent.
    if !mtld3d_core::pool::is_cpu_only(pool) && !caps::texture_extent_fits(width, height) {
        let entry_point = if offscreen_plain {
            "CreateOffscreenPlainSurface"
        } else {
            "CreateTexture"
        };
        mtld3d_shared::log_once_warn_by!(target: LOG_TARGET, key: u64::from(offscreen_plain),
            "reject {entry_point}({width}x{height}, pool={pool}) → NOTAVAILABLE (past the {} \
             texel extent the device reports)", caps::MAX_TEXTURE_DIM);
        null_out(texture);
        return D3DERR_NOTAVAILABLE;
    }

    // Offscreen plain surfaces must be lockable. A GPU-only depth texture
    // cannot provide the staging that this internal surface path requires.
    if is_depth_fmt && offscreen_plain {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET,
            "reject CreateOffscreenPlainSurface depth format → INVALIDCALL (no CPU staging)");
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }

    // Depth-format CreateTexture is the D3D9 sampleable-shadow-map idiom:
    // game asks for a texture in a depth format with D3DUSAGE_DEPTHSTENCIL,
    // binds its surface as the depth target during the shadow pass, and
    // samples it as a regular texture during the lit pass. Mapping table
    // `map_d3d_format` is color-only — depth formats route through
    // `map_d3d_depth_format` and a create path of their own: no CPU staging,
    // and a mip chain, when one is asked for, that only the GPU can fill.
    if mtld3d_core::format::is_depth_format(format) {
        return create_depth_texture_path(&DepthTextureCreateInfo {
            this,
            width,
            height,
            levels,
            usage,
            format,
            pool,
            texture,
        });
    }

    // Packed 4:2:2 YUV has no sampling decode, so the device answers no to the
    // texture query and only the runtime's CPU-only SCRATCH form creates, the
    // rule cube and volume textures follow too. The DEFAULT-pool offscreen
    // plain `StretchRect` decodes keeps the texture behind it.
    if !offscreen_plain
        && mtld3d_core::stretch_rect::is_packed_yuv(format)
        && pool != D3DPOOL_SCRATCH
    {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: (u64::from(pool) << 32) | u64::from(format),
            "reject CreateTexture(format={}, pool={pool}) → INVALIDCALL (a packed YUV texture is \
             a D3DPOOL_SCRATCH resource only)",
            mtld3d_core::format::format_name(format)
        );
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    };
    let expand_packed16 = obj.inner().config().expand_packed16;
    let Some((fmt, planar_layout)) =
        resolve_texture_storage(format, (width, height), offscreen_plain, expand_packed16)
    else {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    };
    if mtld3d_core::pool::usage_conflicts_with_pool(usage, pool) {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: (u64::from(pool) << 32) | u64::from(format),
            "reject CreateTexture(format={format}, DYNAMIC, pool={pool}) → INVALIDCALL"
        );
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // A create's usage has to agree with the answer `CheckDeviceFormat` gives
    // for the same format, so a format that is not colour-renderable cannot
    // carry D3DUSAGE_RENDERTARGET; this rejects the caller that skipped the
    // probe. Two groups fail it: the sampling-only formats, block-compressed
    // ones included, which no D3D9 device rendered into, and the packed 16-bit
    // members on a device that expansion-backs them, where rendering into the
    // BGRA8 backing would break Lock/readback fidelity. Neither can be passed
    // through, because Metal's render-pipeline validation refuses a
    // non-renderable colour attachment with an abort rather than a returned
    // error: a lenient create costs the process at the first draw into the
    // texture instead of costing the application one handled failure here.
    if usage_rt && !crate::direct3d9::is_render_target_format_on_device(format, expand_packed16) {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: u64::from(format),
            "reject CreateTexture(format={format}, RENDERTARGET) → INVALIDCALL (format is not colour-renderable on this device)"
        );
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // D3D9 size-checks block-compressed (DXTn) textures at creation: the top
    // mip's width/height must be block-aligned (a multiple of 4), else
    // INVALIDCALL. WoW's DXT content is power-of-two and
    // thus always aligned, so this never fires in-game.
    if is_dxt_format(format)
        && (!width.is_multiple_of(fmt.block_width()) || !height.is_multiple_of(fmt.block_height()))
    {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }

    // D3DUSAGE_AUTOGENMIPMAP: the runtime owns the mip chain. Honor the flag
    // strictly per spec — the flag is never applied implicitly, because layering
    // aniso + box-filter mip chains onto textures a game intended bilinear-only
    // produces *more* distance shimmer than the original 1:1 mapping, not less.
    // Compressed formats can't be regenerated by Metal's `generateMipmaps`, so
    // mask the flag for BC1/BC2/BC3; callers get a plain single-mip texture and
    // the on-bind sampler honors that. `CheckDeviceFormat` already rejects
    // `AUTOGENMIPMAP` on those formats so a well-behaved game won't hit the
    // masking branch.
    //
    // AUTOGENMIPMAP requires Levels <= 1 — the runtime owns the chain, so an
    // explicit multi-level request is rejected per the D3D9 spec (0 = full
    // chain and 1 are both accepted).
    if (usage & D3DUSAGE_AUTOGENMIPMAP) != 0 && levels > 1 {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // AUTOGENMIPMAP needs a render-targetable, GPU-resident texture (the runtime
    // re-renders each downsampled level), so it is valid only for DEFAULT and
    // MANAGED — SYSTEMMEM/SCRATCH are INVALIDCALL.
    if (usage & D3DUSAGE_AUTOGENMIPMAP) != 0 && pool != D3DPOOL_DEFAULT && pool != D3DPOOL_MANAGED {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // The app-visible level count (`GetLevelCount`) is 1 whenever AUTOGENMIPMAP is
    // requested — the runtime owns the chain — regardless of format (a DXT5
    // autogen texture still reports 1). So the AUTOGEN
    // flag tracks the usage bit alone. Only the *backing* chain depends on format:
    // Metal's `generateMipmaps` can't regenerate block-compressed levels, so
    // compressed autogen textures get a single backing level (the GPU mip-gen op
    // is format-guarded on the unix side) while uncompressed ones get a full chain
    // to downsample. V16U16 retains public usage but uses one level without
    // the internal generation flag, matching its NOAUTOGEN query answer.
    let autogen_requested = usage & D3DUSAGE_AUTOGENMIPMAP != 0;
    let noautogen = autogen_requested && mtld3d_core::format::uses_noautogen_fallback(format);
    let autogen_mipmap = autogen_requested && !noautogen;
    let autogen_full_chain = autogen_mipmap && !fmt.is_compressed();
    let natural_levels = compute_mip_count(width, height);
    let actual_levels = if noautogen {
        1
    } else if autogen_full_chain {
        natural_levels
    } else {
        resolve_create_levels("CreateTexture", levels, natural_levels)
    };

    // Trace probe: one line per distinct (format, dims, levels, usage, pool)
    // combo, so the mip-chain depth a title actually requests is visible.
    let tex_diag_key = (u64::from(format) & 0xFFFF) << 48
        | (u64::from(width) & 0xFFFF) << 32
        | (u64::from(height) & 0xFFFF) << 16
        | (u64::from(actual_levels) & 0xFF) << 8
        | (u64::from(usage) & 0xFF);
    mtld3d_shared::log_once_trace_by!(
        target: TEX_TRACE_TARGET, key: tex_diag_key,
        "tex create fmt={format:#x} {width}x{height} levels={actual_levels} usage={usage:#x} pool={pool}"
    );

    // Allocate per-mip staging buffers as independent page-aligned
    // heap blocks. Each becomes an `Arc<PageBox>` inside `TextureInner`
    // so the upload operation can hand the encoder thread a refcount bump
    // (no memcpy) at `UnlockRect` time. Contents are zero-initialized so
    // a partial lock or sub-rect upload never exposes stale allocator or
    // recycled-pool bytes in unwritten texels, matching Metal's zeroed
    // texture memory.
    // Page alignment satisfies `newBufferWithBytesNoCopy:`'s contract
    // on non-UMA Macs (Intel/AMD). Each level pops a parked box of its
    // padded size from the page-box pool before it falls back to the
    // allocator; the device counts both outcomes once per create.
    let mut flags = TextureFlags::empty();
    flags.set(TextureFlags::AUTOGEN_MIPMAP, autogen_mipmap);
    flags.set(TextureFlags::OFFSCREEN_PLAIN, offscreen_plain);
    let lazy_staging = mtld3d_core::texture_staging::staging_lazy_class(pool, usage, flags, 1);

    let mut staging: Vec<PageBox> = Vec::with_capacity(if lazy_staging {
        0
    } else {
        actual_levels as usize
    });
    let mut mip_widths = Vec::with_capacity(actual_levels as usize);
    let mut mip_heights = Vec::with_capacity(actual_levels as usize);
    let mut mip_bytes_per_row = Vec::with_capacity(actual_levels as usize);

    // The staging is the one allocation of a create that may fail: a level the
    // process has no room for refuses the create with `E_OUTOFMEMORY`.
    let mut out_of_memory = false;
    if let Some(layout) = &planar_layout {
        // One level whose allocation is the planar layout's, not pitch times
        // height: the chroma planes follow the luma rows in the same box, and
        // the lock pitch strides all of them. The per-level arrays keep the
        // logical extent, which is what `GetDesc` and rect validation read.
        // Every offset a planar lock, upload or decode forms is derived from
        // the layout, so the allocation has to be exactly the layout's size.
        if !lazy_staging {
            match PageBox::try_new_zeroed(layout.total_bytes()) {
                Some(page) => {
                    debug_assert_eq!(page.logical_len(), layout.total_bytes());
                    staging.push(page);
                }
                None => out_of_memory = true,
            }
        }
        mip_widths.push(width);
        mip_heights.push(height);
        mip_bytes_per_row.push(layout.pitch());
        debug_assert_eq!(actual_levels, 1);
    } else {
        for level in 0..actual_levels {
            let (mw, mh, size, bpr) = compute_mip_size(width, height, level, &fmt);
            if !lazy_staging {
                let Some(page) = PageBox::try_new_zeroed(size as usize) else {
                    out_of_memory = true;
                    break;
                };
                staging.push(page);
            }
            mip_widths.push(mw);
            mip_heights.push(mh);
            mip_bytes_per_row.push(bpr);
        }
    }
    if out_of_memory {
        return refuse_unallocatable_staging("CreateTexture", texture);
    }
    // An extended device's single-level system-memory texture starts with the
    // application's pixels, packed row after row, copied into the level once.
    if let Some(memory) = user_memory {
        let rows = mtld3d_core::extended::PackedRows::of_level(
            width,
            height,
            fmt.bytes_per_pixel(),
            (fmt.block_width(), fmt.block_height(), fmt.block_bytes()),
        );
        let pitch = usize::try_from(mip_bytes_per_row[0]).expect("a row pitch fits usize");
        copy_user_memory(memory, &rows, staging[0].as_mut_slice(), pitch);
    }
    // A render target the game created at the reported back-buffer size is its
    // main view and shares the back buffer's scale. A texture it uploads pixels
    // into never does: its staging layout is keyed to the reported size.
    let render_scale =
        obj.inner()
            .scale_for_created_target(width, height, usage & D3DUSAGE_RENDERTARGET != 0);
    let tex = Direct3DTexture9::new(TextureCreateInfo {
        texture_id: TextureId::new_unique(),
        device_inner: obj.inner as u64,
        width,
        height,
        render_scale,
        depth: 1,
        levels: actual_levels,
        d3d_format: format,
        metal_pixel_format: fmt.metal_pixel_format(),
        flags,
        swizzle: fmt.swizzle(),
        d3d_usage: usage,
        d3d_pool: pool,
        bytes_per_pixel: fmt.bytes_per_pixel(),
        block_w: fmt.block_width(),
        block_h: fmt.block_height(),
        block_bytes: fmt.block_bytes(),
        staging,
        mip_widths,
        mip_heights,
        mip_bytes_per_row,
    });

    let tex_ptr = Box::into_raw(Box::new(tex));
    // SAFETY: `tex_ptr` is a freshly created, live texture at refcount 1.
    unsafe { crate::com_ref::com_register_child(tex_ptr) };
    // Target textures (render target / depth-stencil usage) are rare and
    // long-lived, and which of them a game keeps, releases, or re-creates
    // decides how cross-pass data flows; log their lifecycle unconditionally.
    if usage & (D3DUSAGE_RENDERTARGET | D3DUSAGE_DEPTHSTENCIL) != 0 {
        // SAFETY: `tex_ptr` is the freshly created, live texture from above.
        let id = unsafe { (*tex_ptr).texture_id() };
        debug!(
            target: LOG_TARGET,
            "target texture created: {id:?} fmt={format:#x} {width}x{height} usage={usage:#x} \
             ptr={tex_ptr:p}"
        );
    }
    // SAFETY: vtable out-param; `texture` is *mut *mut c_void per IDirect3DDevice9 ABI.
    unsafe { OutPtr::write_opt(texture, tex_ptr.cast::<c_void>()) };
    0 // S_OK
}

/// Storage a 2D colour create uses: the Metal mapping, plus the planar layout if it has one.
///
/// Every format but the planar YUV pair answers from the device-aware format
/// lookup. `YV12` and `NV12` are absent from that lookup on purpose, because a
/// planar level is taller than its logical height and no generic consumer of a
/// mapping sizes for that. They are accepted here for the one create that
/// sizes the level from the planar layout, the default-pool offscreen plain,
/// and only at an extent the layout defines. `None` has been warned about and
/// is the caller's `D3DERR_INVALIDCALL`.
fn resolve_texture_storage(
    format: u32,
    (width, height): (u32, u32),
    offscreen_plain: bool,
    expand_packed16: bool,
) -> Option<(
    FormatMapping,
    Option<mtld3d_core::planar_yuv::PlanarYuvLayout>,
)> {
    if !mtld3d_core::stretch_rect::is_planar_yuv(format) {
        let mapping = crate::direct3d9::map_for_device(format, expand_packed16);
        if mapping.is_none() {
            mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                "reject CreateTexture(format={format}) → INVALIDCALL (no format mapping)");
        }
        return mapping.map(|fmt| (fmt, None));
    }
    if !offscreen_plain {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "reject CreateTexture(format={}) → INVALIDCALL (planar YUV exists only as a \
             D3DPOOL_DEFAULT offscreen plain surface)",
            mtld3d_core::format::format_name(format));
        return None;
    }
    let Some(layout) = mtld3d_core::planar_yuv::planar_yuv_layout(format, width, height) else {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: u64::from(format),
            "reject CreateOffscreenPlainSurface({width}x{height}, format={}) → INVALIDCALL (a \
             YV12 surface needs an even height, and the luma and chroma rows together have to \
             fit the {} texture limit)",
            mtld3d_core::format::format_name(format),
            caps::MAX_TEXTURE_DIM
        );
        return None;
    };
    Some((
        mtld3d_core::format::planar_yuv_storage_mapping(),
        Some(layout),
    ))
}

/// Vtable-shaped args bundle for `create_depth_texture_path`.
#[derive(Clone, Copy)]
struct DepthTextureCreateInfo {
    this: *mut c_void,
    width: u32,
    height: u32,
    levels: u32,
    usage: u32,
    format: u32,
    pool: u32,
    texture: *mut *mut c_void,
}

/// Create sampleable depth textures, optionally with packed dynamic staging.
///
/// DEPTHSTENCIL textures can attach through their level surfaces. Plain
/// textures are sampling/RESZ resources; their surface usage rejects attachment
/// binds. DYNAMIC admits only D16, D24X8 and D24S8 plain DEFAULT-pool resources
/// and retains packed CPU staging for each mip. Every other depth resource
/// stays GPU-only. Color render-target usage and CPU pools remain rejected.
///
/// Levels follow [`resolve_create_levels`], including full explicit chains.
/// AUTOGEN is the existing NOAUTOGEN one-level fallback, with no native mip
/// generation. Dynamic resources use logical dimensions, even at render.scale;
/// RESZ transfers convert the attachment's physical extent when necessary.
fn create_depth_texture_path(info: &DepthTextureCreateInfo) -> i32 {
    let DepthTextureCreateInfo {
        this,
        width,
        height,
        levels,
        usage,
        format,
        pool,
        texture,
    } = *info;

    // The lockable depth formats create as a depth-stencil texture, with no
    // `LockRect` of its levels, and not as a plain one: a plain depth texture
    // is a sampling resource the format queries would have to offer.
    if usage & D3DUSAGE_DEPTHSTENCIL == 0 && !is_depth_stencil_format(format) {
        mtld3d_shared::log_once_warn_by!(target: LOG_TARGET, key: u64::from(format),
            "reject CreateTexture plain depth format={format} → INVALIDCALL (unsupported format)");
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    if pool != D3DPOOL_DEFAULT {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: u64::from(pool),
            "reject CreateTexture depth pool={pool} → INVALIDCALL (depth textures must be D3DPOOL_DEFAULT)"
        );
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    let dynamic = usage & D3DUSAGE_DYNAMIC != 0;
    let dynamic_supported = mtld3d_core::depth_texture::PackedDepth::from_d3d(format).is_some()
        && usage & D3DUSAGE_DEPTHSTENCIL == 0;
    let bad_usage_bits = usage & D3DUSAGE_RENDERTARGET
        | if dynamic && !dynamic_supported {
            D3DUSAGE_DYNAMIC
        } else {
            0
        };
    if bad_usage_bits != 0 {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: u64::from(bad_usage_bits),
            "reject CreateTexture depth: incompatible usage bits {bad_usage_bits:#x} → INVALIDCALL"
        );
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    let Some(metal_pixel_format) = mtld3d_core::format::map_d3d_depth_format(format) else {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: u64::from(format),
            "reject CreateTexture depth format={format} → INVALIDCALL (no Metal mapping)"
        );
        null_out(texture);
        return D3DERR_INVALIDCALL;
    };

    // A mip chain on a depth texture is an engine's depth pyramid: each level
    // is rendered into through `GetSurfaceLevel(n)` bound as the depth
    // attachment and sampled back at a coarser resolution.
    let actual_levels = if usage & D3DUSAGE_AUTOGENMIPMAP != 0 {
        if levels > 1 {
            mtld3d_shared::log_once_warn!(target: LOG_TARGET,
                "reject CreateTexture depth AUTOGENMIPMAP levels={levels} → INVALIDCALL (expected 0 or 1)");
            null_out(texture);
            return D3DERR_INVALIDCALL;
        }
        // NOAUTOGEN is a successful single-level fallback. Leave the internal
        // AUTOGEN_MIPMAP flag unset: no path may generate depth mipmaps.
        1
    } else {
        resolve_create_levels(
            "CreateTexture depth",
            levels,
            compute_mip_count(width, height),
        )
    };
    // The per-pixel size the mip chain is charged at against the
    // `GetAvailableTextureMem` budget, in the application's own currency: its
    // D3D9 format, not the wider Metal one a 24-bit depth format is promoted
    // to. Every format with a Metal depth mapping has an entry, so the
    // fallback is unreachable.
    let bytes_per_pixel =
        mtld3d_core::format::depth_format_bytes_per_pixel(format).unwrap_or_else(|| {
            mtld3d_shared::log_once_warn_by!(
                target: crate::LOG_TARGET,
                key: u64::from(format),
                "CreateTexture depth format={format} has a Metal mapping but no byte size; \
                 its levels are charged 0 bytes of texture memory"
            );
            0
        });

    trace!(
        target: LOG_TARGET,
        "CreateTexture depth {width}x{height} levels={actual_levels} format={format} → sampleable shadow map"
    );

    // Per-level dimensions so `GetLevelDesc` reports each mip, and a per-level
    // row pitch so `TextureInner::allocated_bytes` charges the chain on the
    // formula a colour chain is charged on: the host-visible stride of the
    // level's width, not a tight one. Dynamic depth also uses that pitch for
    // packed CPU staging. All three stay
    // in the dimensions the application asked for, which is what `GetLevelDesc`
    // answers with; a chain rasterized at `render.scale` is charged at the
    // extent its Metal levels hold instead, which `allocated_bytes` measures
    // from the scale the texture carries.
    let mut mip_widths = Vec::with_capacity(actual_levels as usize);
    let mut mip_heights = Vec::with_capacity(actual_levels as usize);
    let mut mip_bytes_per_row = Vec::with_capacity(actual_levels as usize);
    for level in 0..actual_levels {
        let (mw, mh, _, bpr) = linear_mip_size(width, height, level, bytes_per_pixel);
        mip_widths.push(mw);
        mip_heights.push(mh);
        mip_bytes_per_row.push(bpr);
    }

    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // Sized to the back buffer, this is the depth buffer for the main view and
    // has to keep matching the colour attachment it is bound with; any other
    // size is a shadow map with its own resolution.
    let render_scale = if dynamic {
        mtld3d_core::render_scale::RenderScale::IDENTITY
    } else {
        obj.inner().scale_for_created_target(width, height, true)
    };
    // The staging is the one allocation of a create that may fail.
    let staging = if dynamic {
        let staging: Option<Vec<PageBox>> = mip_bytes_per_row
            .iter()
            .zip(&mip_heights)
            .map(|(&pitch, &rows)| {
                PageBox::try_new_zeroed((pitch as usize).saturating_mul(rows as usize))
            })
            .collect();
        let Some(staging) = staging else {
            return refuse_unallocatable_staging("CreateTexture", texture);
        };
        staging
    } else {
        Vec::new()
    };
    let tex = Direct3DTexture9::new(TextureCreateInfo {
        texture_id: TextureId::new_unique(),
        device_inner: obj.inner as u64,
        width,
        height,
        render_scale,
        depth: 1,
        levels: actual_levels,
        d3d_format: format,
        metal_pixel_format,
        flags: TextureFlags::DEPTH_FORMAT,
        swizzle: None,
        d3d_usage: usage,
        d3d_pool: pool,
        bytes_per_pixel,
        block_w: 1,
        block_h: 1,
        block_bytes: bytes_per_pixel,
        staging,
        mip_widths,
        mip_heights,
        mip_bytes_per_row,
    });

    let tex_ptr = Box::into_raw(Box::new(tex));
    // SAFETY: `tex_ptr` is a freshly created, live texture at refcount 1.
    unsafe { crate::com_ref::com_register_child(tex_ptr) };
    // Mirror of the colour path's "target texture created" line; the depth
    // path returns before that one runs.
    {
        // SAFETY: `tex_ptr` is the freshly created, live texture from above.
        let id = unsafe { (*tex_ptr).texture_id() };
        debug!(
            target: LOG_TARGET,
            "target texture created: {id:?} fmt={format:#x} {width}x{height} usage={usage:#x} \
             ptr={tex_ptr:p}"
        );
    }
    // SAFETY: vtable out-param; `texture` is *mut *mut c_void per IDirect3DDevice9 ABI.
    unsafe { OutPtr::write_opt(texture, tex_ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_create_volume_texture(
    this: *mut c_void,
    width: u32,
    height: u32,
    depth: u32,
    levels: u32,
    usage: u32,
    format: u32,
    pool: u32,
    texture: *mut *mut c_void,
    shared_handle: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    trace!(
        target: LOG_TARGET,
        "IDirect3DDevice9::CreateVolumeTexture({width}x{height}x{depth}, fmt={format})"
    );
    if width == 0 || height == 0 || depth == 0 || texture.is_null() {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // A plain device refuses a non-null pSharedHandle with E_NOTIMPL; an
    // extended one reads it as a shared resource, which it does not create.
    if let Err(hr) = shared_handle_gate(
        this,
        &mtld3d_core::extended::CreateKind::VolumeTexture,
        pool,
        shared_handle,
        "CreateVolumeTexture",
    ) {
        null_out(texture);
        return hr;
    }
    let Some(pool) = create_pool(
        this,
        &mtld3d_core::extended::CreateKind::VolumeTexture,
        pool,
        "CreateVolumeTexture",
    ) else {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    };
    let Some(fmt) = crate::direct3d9::map_for_device(format, obj.inner().config().expand_packed16)
    else {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "reject CreateVolumeTexture(format={format}) → INVALIDCALL (no format mapping)");
        null_out(texture);
        return D3DERR_INVALIDCALL;
    };
    // DXT base extents must be block aligned in every pool. The GPU-backed
    // pools take what `is_volume_texture_format` advertises, DXT1 to DXT5
    // among the compressed formats; the other compressed and the packed-YUV
    // formats are creatable as SCRATCH volumes only.
    let block_w = fmt.block_width();
    let block_h = fmt.block_height();
    if is_dxt_format(format) && (!width.is_multiple_of(block_w) || !height.is_multiple_of(block_h))
    {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    if pool != D3DPOOL_SCRATCH && !mtld3d_core::format::is_volume_texture_format(format) {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // Volumes cannot be render targets or depth-stencils; D3DUSAGE_WRITEONLY is
    // a buffer-only flag and D3DUSAGE_AUTOGENMIPMAP is invalid on a volume
    // texture — all INVALIDCALL.
    if usage
        & (D3DUSAGE_RENDERTARGET
            | D3DUSAGE_DEPTHSTENCIL
            | D3DUSAGE_WRITEONLY
            | D3DUSAGE_AUTOGENMIPMAP)
        != 0
    {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    if mtld3d_core::pool::usage_conflicts_with_pool(usage, pool) {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: (u64::from(pool) << 32) | u64::from(format),
            "reject CreateVolumeTexture(format={format}, DYNAMIC, pool={pool}) → INVALIDCALL"
        );
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // A volume more than one slice deep is a 3D Metal texture, held to
    // `MaxVolumeExtent` on every axis; Metal aborts the process past its limit.
    if !mtld3d_core::pool::is_cpu_only(pool) && !caps::volume_extent_fits(width, height, depth) {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET,
            "reject CreateVolumeTexture({width}x{height}x{depth}, pool={pool}) → NOTAVAILABLE \
             (past the extent the device reports)");
        null_out(texture);
        return D3DERR_NOTAVAILABLE;
    }
    // A per-level 3D mip chain. Each level's box is the block-aware 2D slice
    // size (`compute_mip_size`, correct for DXT/ATI as well as plain formats)
    // times the level's depth; `LockBox` hands the game a pointer into the
    // level's box with the matching block-aware pitches. `levels == 0` requests
    // the full chain. Sizing the levels correctly is what lets `LockBox(level)`
    // resolve a real box instead of returning NULL (a NULL box would fault a
    // LockBox on a mip sub-level).
    // Zero stays zero: it marks a compressed layout, and the upload of a level
    // that is one slice deep counts block rows only while the marker survives.
    let bpp = fmt.bytes_per_pixel();
    let actual_levels = resolve_create_levels(
        "CreateVolumeTexture",
        levels,
        compute_volume_mip_count(width, height, depth),
    );
    let mut staging: Vec<PageBox> = Vec::with_capacity(actual_levels as usize);
    let mut mip_widths = Vec::with_capacity(actual_levels as usize);
    let mut mip_heights = Vec::with_capacity(actual_levels as usize);
    let mut mip_bytes_per_row = Vec::with_capacity(actual_levels as usize);
    for level in 0..actual_levels {
        let (mw, mh, slice_size, bpr) = compute_mip_size(width, height, level, &fmt);
        let md = (depth >> level).max(1);
        let box_bytes = (slice_size as usize).saturating_mul(md as usize);
        // The staging is the one allocation of a create that may fail.
        let Some(page) = PageBox::try_new_zeroed(box_bytes) else {
            break;
        };
        staging.push(page);
        mip_widths.push(mw);
        mip_heights.push(mh);
        mip_bytes_per_row.push(bpr);
    }
    if staging.len() != actual_levels as usize {
        return refuse_unallocatable_staging("CreateVolumeTexture", texture);
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let tex = crate::texture::Direct3DVolumeTexture9::new(TextureCreateInfo {
        texture_id: mtld3d_core::ids::TextureId::new_unique(),
        device_inner: obj.inner as u64,
        width,
        height,
        // A volume texture is never a render target, so it never scales.
        render_scale: mtld3d_core::render_scale::RenderScale::IDENTITY,
        depth,
        levels: actual_levels,
        d3d_format: format,
        metal_pixel_format: fmt.metal_pixel_format(),
        // Its sub-resources are `IDirect3DVolume9` levels, not surfaces, whatever
        // the depth; the container has to free the cached slots as the kind they
        // hold.
        flags: TextureFlags::VOLUME_TEXTURE,
        swizzle: fmt.swizzle(),
        d3d_usage: usage,
        d3d_pool: pool,
        bytes_per_pixel: bpp,
        block_w: fmt.block_width(),
        block_h: fmt.block_height(),
        block_bytes: fmt.block_bytes(),
        staging,
        mip_widths,
        mip_heights,
        mip_bytes_per_row,
    });
    let tex_ptr = Box::into_raw(Box::new(tex));
    // SAFETY: `tex_ptr` is a freshly created, live volume texture at refcount 1;
    // it shares `Direct3DTexture9`'s layout and refcount engine.
    unsafe {
        crate::com_ref::com_register_child(tex_ptr.cast::<crate::texture::Direct3DTexture9>());
    };
    // SAFETY: vtable out-param; `texture` is *mut *mut c_void per the ABI.
    unsafe { OutPtr::write_opt(texture, tex_ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_create_cube_texture(
    this: *mut c_void,
    edge_length: u32,
    levels: u32,
    usage: u32,
    format: u32,
    pool: u32,
    texture: *mut *mut c_void,
    shared_handle: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if edge_length == 0 || texture.is_null() {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // A plain device refuses a non-null pSharedHandle with E_NOTIMPL; an
    // extended one reads it as a shared resource, which it does not create.
    if let Err(hr) = shared_handle_gate(
        this,
        &mtld3d_core::extended::CreateKind::CubeTexture,
        pool,
        shared_handle,
        "CreateCubeTexture",
    ) {
        null_out(texture);
        return hr;
    }
    let Some(pool) = create_pool(
        this,
        &mtld3d_core::extended::CreateKind::CubeTexture,
        pool,
        "CreateCubeTexture",
    ) else {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    };
    // D3DUSAGE_WRITEONLY is a vertex/index-buffer-only flag; on a cube texture it
    // is INVALIDCALL.
    if usage & D3DUSAGE_WRITEONLY != 0 {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // Cube render targets must be DEFAULT-pool color resources. Depth cubes
    // remain unsupported, and DYNAMIC cannot combine with either target usage.
    let usage_rt = usage & D3DUSAGE_RENDERTARGET != 0;
    let usage_ds = usage & D3DUSAGE_DEPTHSTENCIL != 0;
    if usage_ds || (usage_rt && (pool != D3DPOOL_DEFAULT || usage & D3DUSAGE_DYNAMIC != 0)) {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // Format-vs-usage mismatch: a colour format
    // cannot carry D3DUSAGE_DEPTHSTENCIL and a depth format cannot carry
    // D3DUSAGE_RENDERTARGET. WoW pairs RT with colour formats and DS with depth
    // formats, so this never fires in-game.
    let is_depth_fmt = mtld3d_core::format::is_depth_format(format);
    if (usage_ds && !is_depth_fmt) || (usage_rt && is_depth_fmt) {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    };
    let expand_packed16 = obj.inner().config().expand_packed16;
    let Some(fmt) = crate::direct3d9::map_for_device(format, expand_packed16) else {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "reject CreateCubeTexture(format={format}) → INVALIDCALL (no format mapping)");
        null_out(texture);
        return D3DERR_INVALIDCALL;
    };
    if mtld3d_core::pool::usage_conflicts_with_pool(usage, pool) {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: (u64::from(pool) << 32) | u64::from(format),
            "reject CreateCubeTexture(format={format}, DYNAMIC, pool={pool}) → INVALIDCALL"
        );
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // DXTn cube faces must be block-aligned; edge_length
    // is both width and height of a face.
    if is_dxt_format(format) && !edge_length.is_multiple_of(fmt.block_width()) {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // ATI1 and packed YUV retain only their CPU SCRATCH resource form. Depth
    // formats remain unsupported, and render-target cubes require a Metal
    // renderable color format on THIS device (the packed 16-bit members drop
    // out where they are expansion-backed).
    if (matches!(format, D3DFMT_ATI1 | D3DFMT_YUY2 | D3DFMT_UYVY) && pool != D3DPOOL_SCRATCH)
        || is_depth_fmt
        || (usage_rt
            && !crate::direct3d9::is_render_target_format_on_device(format, expand_packed16))
    {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    if usage & D3DUSAGE_AUTOGENMIPMAP != 0 && levels > 1 {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    if usage & D3DUSAGE_AUTOGENMIPMAP != 0 && !matches!(pool, D3DPOOL_DEFAULT | D3DPOOL_MANAGED) {
        null_out(texture);
        return D3DERR_INVALIDCALL;
    }
    // Same extent rule as `CreateTexture`: a face past the reported extent is
    // refused in the pools Metal backs, before six faces of staging are made.
    if !mtld3d_core::pool::is_cpu_only(pool) && !caps::texture_extent_fits(edge_length, edge_length)
    {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET,
            "reject CreateCubeTexture(edge={edge_length}, pool={pool}) → NOTAVAILABLE (past the \
             {} texel extent the device reports)", caps::MAX_TEXTURE_DIM);
        null_out(texture);
        return D3DERR_NOTAVAILABLE;
    }
    // Zero stays zero: it marks a compressed layout, and a face upload counts
    // block rows and block offsets only while the marker survives creation.
    let bpp = fmt.bytes_per_pixel();
    // `levels == 0` means the full chain. Staging is face-major so the cube
    // sidecar can address `face * levels + level` without another allocation.
    let autogen_requested = usage & D3DUSAGE_AUTOGENMIPMAP != 0;
    let noautogen = autogen_requested && mtld3d_core::format::uses_noautogen_fallback(format);
    let autogen_mipmap = autogen_requested && !noautogen;
    let autogen_full_chain = autogen_mipmap && !fmt.is_compressed();
    let natural_levels = compute_mip_count(edge_length, edge_length);
    let actual_levels = if noautogen {
        1
    } else if autogen_full_chain {
        natural_levels
    } else {
        resolve_create_levels("CreateCubeTexture", levels, natural_levels)
    };
    let mut staging: Vec<PageBox> =
        Vec::with_capacity(actual_levels as usize * CUBE_FACE_COUNT as usize);
    let mut mip_widths = Vec::with_capacity(actual_levels as usize);
    let mut mip_heights = Vec::with_capacity(actual_levels as usize);
    let mut mip_bytes_per_row = Vec::with_capacity(actual_levels as usize);
    for level in 0..actual_levels {
        let (mw, mh, _size, bpr) = compute_mip_size(edge_length, edge_length, level, &fmt);
        mip_widths.push(mw);
        mip_heights.push(mh);
        mip_bytes_per_row.push(bpr);
    }
    // The staging is the one allocation of a create that may fail.
    'faces: for _face in 0..CUBE_FACE_COUNT {
        for level in 0..actual_levels {
            let (_, _, size, _) = compute_mip_size(edge_length, edge_length, level, &fmt);
            let Some(page) = PageBox::try_new_zeroed(size as usize) else {
                break 'faces;
            };
            staging.push(page);
        }
    }
    if staging.len() != actual_levels as usize * CUBE_FACE_COUNT as usize {
        return refuse_unallocatable_staging("CreateCubeTexture", texture);
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // A cube face is never the main view's colour or depth target, so it keeps
    // whatever edge length the game asked for.
    let mut flags = TextureFlags::CUBE;
    flags.set(TextureFlags::AUTOGEN_MIPMAP, autogen_mipmap);
    let tex = crate::texture::Direct3DCubeTexture9::new(TextureCreateInfo {
        texture_id: mtld3d_core::ids::TextureId::new_unique(),
        device_inner: obj.inner as u64,
        width: edge_length,
        height: edge_length,
        render_scale: mtld3d_core::render_scale::RenderScale::IDENTITY,
        depth: 1,
        levels: actual_levels,
        d3d_format: format,
        metal_pixel_format: fmt.metal_pixel_format(),
        flags,
        swizzle: fmt.swizzle(),
        d3d_usage: usage,
        d3d_pool: pool,
        bytes_per_pixel: bpp,
        block_w: fmt.block_width(),
        block_h: fmt.block_height(),
        block_bytes: fmt.block_bytes(),
        staging,
        mip_widths,
        mip_heights,
        mip_bytes_per_row,
    });
    let tex_ptr = Box::into_raw(Box::new(tex));
    // SAFETY: `tex_ptr` is a freshly created, live cube texture at refcount 1;
    // it shares `Direct3DTexture9`'s layout and refcount engine.
    unsafe {
        crate::com_ref::com_register_child(tex_ptr.cast::<crate::texture::Direct3DTexture9>());
    };
    // SAFETY: vtable out-param; `texture` is *mut *mut c_void per the ABI.
    unsafe { OutPtr::write_opt(texture, tex_ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_create_vertex_buffer(
    this: *mut c_void,
    length: u32,
    usage: u32,
    fvf: u32,
    pool: u32,
    vb: *mut *mut c_void,
    shared_handle: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if vb.is_null() || length == 0 {
        null_out(vb);
        return D3DERR_INVALIDCALL;
    }
    // A plain device refuses a non-null pSharedHandle with E_NOTIMPL; an
    // extended one reads it as a shared resource, which it does not create.
    if let Err(hr) = shared_handle_gate(
        this,
        &mtld3d_core::extended::CreateKind::VertexBuffer,
        pool,
        shared_handle,
        "CreateVertexBuffer",
    ) {
        null_out(vb);
        return hr;
    }
    let Some(pool) = create_pool(
        this,
        &mtld3d_core::extended::CreateKind::VertexBuffer,
        pool,
        "CreateVertexBuffer",
    ) else {
        null_out(vb);
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // D3DPOOL_SCRATCH is invalid for buffers (it is a CPU-only surface/texture
    // pool); D3D9 rejects CreateVertexBuffer(D3DPOOL_SCRATCH) with
    // INVALIDCALL.
    if pool == D3DPOOL_SCRATCH {
        warn!(target: LOG_TARGET, "reject CreateVertexBuffer(D3DPOOL_SCRATCH) → INVALIDCALL");
        null_out(vb);
        return D3DERR_INVALIDCALL;
    }
    // RENDERTARGET / DEPTHSTENCIL are surface-only usages, invalid on a buffer.
    // WoW buffers use WRITEONLY/DYNAMIC only.
    if usage & (D3DUSAGE_RENDERTARGET | D3DUSAGE_DEPTHSTENCIL) != 0 {
        null_out(vb);
        return D3DERR_INVALIDCALL;
    }
    let dev = obj.inner();
    trace!(
        target: LOG_TARGET,
        "CreateVertexBuffer(len={length}, usage={usage:#x}, fvf={fvf:#x}, pool={pool})"
    );
    warn_unused_usage_and_pool_once("VertexBuffer", usage, pool);
    let Some(buffer) = Direct3DVertexBuffer9::new(&VertexBufferCreateInfo {
        device_inner: std::ptr::from_mut::<DeviceInner>(dev),
        length,
        usage,
        fvf,
        pool,
    }) else {
        return refuse_unallocatable_staging("CreateVertexBuffer", vb);
    };
    // SAFETY: vtable out-param; `vb` is *mut *mut c_void per IDirect3DDevice9 ABI.
    let buffer_ptr = Box::into_raw(Box::new(buffer));
    // SAFETY: `buffer_ptr` is a freshly created, live vertex buffer at refcount 1.
    unsafe { crate::com_ref::com_register_child(buffer_ptr) };
    // SAFETY: vtable out-param; `vb` is *mut *mut c_void per IDirect3DDevice9 ABI.
    unsafe { OutPtr::write_opt(vb, buffer_ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_create_index_buffer(
    this: *mut c_void,
    length: u32,
    usage: u32,
    format: u32,
    pool: u32,
    ib: *mut *mut c_void,
    shared_handle: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if ib.is_null() || length == 0 {
        null_out(ib);
        return D3DERR_INVALIDCALL;
    }
    // A plain device refuses a non-null pSharedHandle with E_NOTIMPL; an
    // extended one reads it as a shared resource, which it does not create.
    if let Err(hr) = shared_handle_gate(
        this,
        &mtld3d_core::extended::CreateKind::IndexBuffer,
        pool,
        shared_handle,
        "CreateIndexBuffer",
    ) {
        null_out(ib);
        return hr;
    }
    let Some(pool) = create_pool(
        this,
        &mtld3d_core::extended::CreateKind::IndexBuffer,
        pool,
        "CreateIndexBuffer",
    ) else {
        null_out(ib);
        return D3DERR_INVALIDCALL;
    };
    // D3DFMT_INDEX16 = 101, D3DFMT_INDEX32 = 102 are the only legal index
    // formats; the draw path selects `MTLIndexType` from the stored format, so
    // both are fully supported here.
    if convert::index_format(format).is_none() {
        warn!(
            target: LOG_TARGET,
            "reject CreateIndexBuffer(format={format}) → INVALIDCALL (not an index format)"
        );
        null_out(ib);
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // D3DPOOL_SCRATCH is invalid for buffers (CPU-only surface/texture pool);
    // D3D9 rejects CreateIndexBuffer(D3DPOOL_SCRATCH) with INVALIDCALL.
    if pool == D3DPOOL_SCRATCH {
        warn!(target: LOG_TARGET, "reject CreateIndexBuffer(D3DPOOL_SCRATCH) → INVALIDCALL");
        null_out(ib);
        return D3DERR_INVALIDCALL;
    }
    // RENDERTARGET / DEPTHSTENCIL are surface-only usages, invalid on a buffer.
    if usage & (D3DUSAGE_RENDERTARGET | D3DUSAGE_DEPTHSTENCIL) != 0 {
        null_out(ib);
        return D3DERR_INVALIDCALL;
    }
    let dev = obj.inner();
    trace!(
        target: LOG_TARGET,
        "CreateIndexBuffer(len={length}, usage={usage:#x}, format={format}, pool={pool})"
    );
    warn_unused_usage_and_pool_once("IndexBuffer", usage, pool);
    let Some(buffer) = Direct3DIndexBuffer9::new(&IndexBufferCreateInfo {
        device_inner: std::ptr::from_mut::<DeviceInner>(dev),
        length,
        usage,
        format,
        pool,
    }) else {
        return refuse_unallocatable_staging("CreateIndexBuffer", ib);
    };
    // SAFETY: vtable out-param; `ib` is *mut *mut c_void per IDirect3DDevice9 ABI.
    let buffer_ptr = Box::into_raw(Box::new(buffer));
    // SAFETY: `buffer_ptr` is a freshly created, live index buffer at refcount 1.
    unsafe { crate::com_ref::com_register_child(buffer_ptr) };
    // SAFETY: vtable out-param; `ib` is *mut *mut c_void per IDirect3DDevice9 ABI.
    unsafe { OutPtr::write_opt(ib, buffer_ptr.cast::<c_void>()) };
    D3D_OK
}

/// Resolve a surface create's `(multi_sample, quality)` against the device.
///
/// The same predicate `CheckDeviceMultiSampleType` answers with, so a game
/// that asked first gets the same verdict at create time. A create says
/// `INVALIDCALL` for every rejection, malformed or merely unavailable: the
/// query is where D3D9 draws that distinction, and the create's contract is
/// the single "these parameters do not describe a surface I can make".
fn resolve_surface_multi_sample(
    multi_sample_type: u32,
    multi_sample_quality: u32,
    format: u32,
    site: &str,
) -> Result<SurfaceMultiSample, i32> {
    let Ok(sample_count) = mtld3d_core::multisample::resolve_sample_count(
        multi_sample_type,
        multi_sample_quality,
        format,
        crate::direct3d9::device_caps_flags(),
    ) else {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject {site}: multi_sample={multi_sample_type} quality={multi_sample_quality} on format {format} is not creatable on this device → INVALIDCALL"
        );
        return Err(D3DERR_INVALIDCALL);
    };
    Ok(SurfaceMultiSample {
        multi_sample_type,
        multi_sample_quality,
        sample_count: u8::try_from(sample_count).expect("sample count ≤ 16 fits u8"),
    })
}

/// The D3D9 description of one standalone colour surface to create.
///
/// `multi_sample` is the already-resolved `(type, quality)` pair: the sample
/// count plus the D3D9 type the surface reports from `GetDesc`. Above one
/// sample the create also produces the multisampled companion the passes
/// attach.
struct ColorTargetSpec {
    width: u32,
    height: u32,
    format: u32,
    usage: u32,
    multi_sample: SurfaceMultiSample,
}

/// Create a persistent render-target-capable color `MTLTexture` and wrap it as a surface.
///
/// The wrapper is a standalone `Direct3DSurface9`, mirroring the depth path.
/// `CreateRenderTarget` (usage = `D3DUSAGE_RENDERTARGET`) is its only caller
/// today; the usage-0 shape a `D3DPOOL_DEFAULT` offscreen plain would take is
/// answered too, because the two differ only in which format question they ask.
/// Returns the boxed wrapper pointer, or `None` (caller maps to `INVALIDCALL`)
/// for an unmappable or compressed color format, for a format whose usage the
/// device cannot honour, or if the Metal allocation fails.
fn create_color_target_surface(
    device_inner: *mut DeviceInner,
    spec: &ColorTargetSpec,
    expand_packed16: bool,
) -> Option<*mut Direct3DSurface9> {
    let &ColorTargetSpec {
        width,
        height,
        format,
        usage,
        multi_sample,
    } = spec;
    let mapping = crate::direct3d9::map_for_device(format, expand_packed16)?;
    if mapping.is_compressed() {
        return None;
    }
    if usage & D3DUSAGE_RENDERTARGET != 0 {
        if !crate::direct3d9::is_render_target_format_on_device(format, expand_packed16) {
            mtld3d_shared::log_once_warn_by!(
                target: crate::LOG_TARGET,
                key: u64::from(format),
                "reject CreateRenderTarget(format={format}) → INVALIDCALL (format is not colour-renderable on this device)"
            );
            return None;
        }
    } else if mtld3d_core::upload_pass::is_expanded_upload(format, mapping.metal_pixel_format()) {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: u64::from(format),
            "reject standalone surface(format={format}) → INVALIDCALL (a format widened on upload is sampling-only)"
        );
        return None;
    }
    if !mtld3d_core::caps::texture_extent_fits(width, height) {
        return None;
    }
    let id = mtld3d_core::ids::TextureId::new_unique();
    let srgb_twin = mapping.metal_pixel_format().srgb_twin();
    // SAFETY: `device_inner` is the live owning device, non-null for every caller.
    let device = unsafe { &mut *device_inner };
    let scale = device.scale_for_created_target(width, height, usage & D3DUSAGE_RENDERTARGET != 0);
    let (render_w, render_h) = scale.extent(width, height);
    device.core.texture_create(
        id,
        &crate::rt::device::CoreTextureDesc {
            pixel_format: mapping.metal_pixel_format() as u32,
            kind: crate::rt::device::TextureKind::Tex2d,
            width: render_w,
            height: render_h,
            depth: 1,
            levels: 1,
            flags: if srgb_twin.is_some() {
                crate::rt::device::CoreTextureFlags::RENDER_TARGET
                    .union(crate::rt::device::CoreTextureFlags::VIEW_TWINS)
            } else {
                crate::rt::device::CoreTextureFlags::RENDER_TARGET
            },
            srgb_format: srgb_twin.map_or(0, |twin| twin as u32),
            block: (1, 1, mapping.bytes_per_pixel()),
            samples: multi_sample.sample_count,
        },
    );
    if !scale.is_identity() {
        device.core.set_texture_logical_size(id, width, height);
    }
    let mut surf = Direct3DSurface9::new_color_target(&ColorTargetCreateInfo {
        device_inner,
        width,
        height,
        format,
        usage,
        render_scale: scale,
        multi_sample,
    });
    surf.set_core_texture(id);
    let surf_ptr = Box::into_raw(Box::new(surf));
    // SAFETY: `surf_ptr` is a freshly created, live standalone render-target surface at refcount 1.
    unsafe { crate::com_ref::com_register_child(surf_ptr) };
    Some(surf_ptr)
}

extern "system" fn device_create_render_target(
    this: *mut c_void,
    width: u32,
    height: u32,
    format: u32,
    multi_sample: u32,
    multi_sample_quality: u32,
    lockable: i32,
    surface: *mut *mut c_void,
    shared_handle: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if surface.is_null() || width == 0 || height == 0 {
        null_out(surface);
        return D3DERR_INVALIDCALL;
    }
    // A plain device refuses a non-null pSharedHandle with E_NOTIMPL; an
    // extended one reads it as a shared resource, which it does not create.
    if let Err(hr) = shared_handle_gate(
        this,
        &mtld3d_core::extended::CreateKind::RenderTarget,
        D3DPOOL_DEFAULT,
        shared_handle,
        "CreateRenderTarget",
    ) {
        null_out(surface);
        return hr;
    }
    let multi_sample = match resolve_surface_multi_sample(
        multi_sample,
        multi_sample_quality,
        format,
        "CreateRenderTarget",
    ) {
        Ok(ms) => ms,
        Err(hr) => {
            null_out(surface);
            return hr;
        }
    };
    // A lockable multisampled render target has no meaning: D3D9 defines the
    // lock against a single-sample surface, and the samples are the point of
    // the multisampled one.
    if lockable != 0 && multi_sample.sample_count > 1 {
        warn!(
            target: LOG_TARGET,
            "reject CreateRenderTarget({width}x{height}, ms={}) → INVALIDCALL (a multisampled render target cannot be lockable)",
            multi_sample.multi_sample_type
        );
        null_out(surface);
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        null_out(surface);
        return D3DERR_INVALIDCALL;
    };
    // A `Lockable == TRUE` render target keeps its standalone renderable colour
    // texture (so `GetContainer`/`GetDesc`/`StretchRect` are unchanged) but also
    // gets a CPU staging buffer: `LockRect` maps it, `UnlockRect` uploads it to
    // the colour texture. It is sized at the same row pitch every host-visible
    // surface store uses, so `LockRect`, the GPU read-back and the DIB a
    // `GetDC` wraps around it all step by the same stride. A format with no CPU
    // byte size sizes it at zero and is rejected by the create just below (a
    // lockable render target is an uncompressed colour format), so every
    // surface that survives the create carries the staging it asked for.
    let staging_bytes = if lockable == 0 {
        0
    } else {
        let bpp = map_d3d_format(format).map_or(0, |m| m.bytes_per_pixel());
        (linear_row_pitch(width, bpp) as usize).saturating_mul(height as usize)
    };
    // Zero-initialise the staging (defence-in-depth): a `LockRect` before any
    // render, or any path that skips the read-back fill, reads defined bytes
    // rather than allocator garbage. It is the one allocation of the create that
    // may fail, and it is made before the colour texture so a refusal leaves
    // nothing to tear down.
    let staging = if staging_bytes == 0 {
        None
    } else {
        let Some(staging) = PageBox::try_new_zeroed(staging_bytes) else {
            return refuse_unallocatable_staging("CreateRenderTarget", surface);
        };
        Some(staging)
    };
    let Some(surf_ptr) = create_color_target_surface(
        obj.inner_ptr(),
        &ColorTargetSpec {
            width,
            height,
            format,
            usage: D3DUSAGE_RENDERTARGET,
            multi_sample,
        },
        obj.inner().config().expand_packed16,
    ) else {
        warn!(
            target: LOG_TARGET,
            "reject CreateRenderTarget({width}x{height}, format={format}) → INVALIDCALL (no renderable Metal color mapping or allocation failed)"
        );
        null_out(surface);
        return D3DERR_INVALIDCALL;
    };
    if let Some(staging) = staging {
        // SAFETY: `surf_ptr` is the freshly created, live standalone RT
        // surface (refcount 1); no other reference exists yet, so the
        // exclusive borrow to attach the staging is sound.
        unsafe { &mut *surf_ptr }.set_lockable_staging(staging);
    }
    // SAFETY: vtable out-param; `surface` is *mut *mut c_void per IDirect3DDevice9 ABI.
    unsafe { OutPtr::write_opt(surface, surf_ptr.cast::<c_void>()) };
    D3D_OK
}

/// A standalone depth-stencil surface backed by a one-level depth texture.
///
/// # Errors
///
/// `D3DERR_INVALIDCALL` past the device's texture limit.
fn core_depth_surface(
    device_inner: *mut DeviceInner,
    (width, height): (u32, u32),
    format: u32,
    pixel_format: u32,
    multi_sample: SurfaceMultiSample,
) -> Result<*mut Direct3DSurface9, i32> {
    use crate::rt::device::{CoreTextureDesc, CoreTextureFlags, TextureKind};
    if !mtld3d_core::caps::texture_extent_fits(width, height) {
        return Err(D3DERR_INVALIDCALL);
    }
    let id = mtld3d_core::ids::TextureId::new_unique();
    // SAFETY: `device_inner` is the live owning device, non-null for the caller.
    let device = unsafe { &mut *device_inner };
    let scale = device.scale_for_created_target(width, height, true);
    let (render_w, render_h) = scale.extent(width, height);
    device.core.texture_create(
        id,
        &CoreTextureDesc {
            pixel_format,
            kind: TextureKind::Tex2d,
            width: render_w,
            height: render_h,
            depth: 1,
            levels: 1,
            flags: CoreTextureFlags::RENDER_TARGET,
            srgb_format: 0,
            // Never uploaded to or read back: the block is not read.
            block: (1, 1, 4),
            samples: multi_sample.sample_count,
        },
    );
    if !scale.is_identity() {
        device.core.set_texture_logical_size(id, width, height);
    }
    let mut surf = Direct3DSurface9::new_depth_stencil(
        device_inner,
        width,
        height,
        format,
        scale,
        multi_sample,
    );
    surf.set_core_texture(id);
    let surf_ptr = Box::into_raw(Box::new(surf));
    // SAFETY: `surf_ptr` is a freshly created, live standalone depth-stencil at refcount 1.
    unsafe { crate::com_ref::com_register_child(surf_ptr) };
    Ok(surf_ptr)
}

extern "system" fn device_create_depth_stencil_surface(
    this: *mut c_void,
    width: u32,
    height: u32,
    format: u32,
    multi_sample: u32,
    multi_sample_quality: u32,
    _discard: i32,
    surface: *mut *mut c_void,
    shared_handle: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if surface.is_null() || width == 0 || height == 0 {
        null_out(surface);
        return D3DERR_INVALIDCALL;
    }
    // A plain device refuses a non-null pSharedHandle with E_NOTIMPL; an
    // extended one reads it as a shared resource, which it does not create.
    if let Err(hr) = shared_handle_gate(
        this,
        &mtld3d_core::extended::CreateKind::DepthStencil,
        D3DPOOL_DEFAULT,
        shared_handle,
        "CreateDepthStencilSurface",
    ) {
        null_out(surface);
        return hr;
    }
    if !is_depth_stencil_format(format) {
        warn!(
            target: LOG_TARGET,
            "reject CreateDepthStencilSurface({width}x{height}, format={format}) → INVALIDCALL (not a depth format)"
        );
        null_out(surface);
        return D3DERR_INVALIDCALL;
    }
    let multi_sample = match resolve_surface_multi_sample(
        multi_sample,
        multi_sample_quality,
        format,
        "CreateDepthStencilSurface",
    ) {
        Ok(ms) => ms,
        Err(hr) => {
            null_out(surface);
            return hr;
        }
    };
    let Some(pixel_format) = mtld3d_core::format::map_d3d_depth_format(format) else {
        warn!(
            target: LOG_TARGET,
            "reject CreateDepthStencilSurface({width}x{height}, format={format}) → INVALIDCALL (no Metal depth mapping)"
        );
        null_out(surface);
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let made = core_depth_surface(
        obj.inner_ptr(),
        (width, height),
        format,
        pixel_format as u32,
        multi_sample,
    );
    match made {
        Ok(surf_ptr) => {
            // SAFETY: vtable out-param; `surface` is *mut *mut c_void per the ABI.
            unsafe { OutPtr::write_opt(surface, surf_ptr.cast::<c_void>()) };
            D3D_OK
        }
        Err(hr) => {
            null_out(surface);
            hr
        }
    }
}

/// Publish a staging write to `tex`: at the next draw's re-walk, or now for a render target.
///
/// Bind-time `flush_dirty_mips` only runs while the API thread rebuilds a
/// dirty snapshot, so a write that lands in a texture's staging between two
/// draws over otherwise-clean state has to dirty the snapshot itself or its
/// upload is never scheduled and the next draw samples the old texels. The
/// mark is deliberately coarse and does not ask whether the texture is bound:
/// a redundant snapshot re-emit dedups at the encoder. A render-target texture
/// uploads now instead, in application order
/// (`crate::texture::publish_render_target_write`).
fn publish_staging_write(tex: &mut crate::texture::Direct3DTexture9) {
    let device_inner_ptr = tex.inner().device_inner();
    if device_inner_ptr != 0 {
        // SAFETY: live `DeviceInner*` recorded at the texture's create; the
        // device outlives every texture it owns (textures hold a device
        // refcount via their COM ABI).
        let dev = unsafe { &mut *(device_inner_ptr as *mut DeviceInner) };
        dev.mark_snapshot_dirty_all();
        crate::texture::publish_render_target_write(tex.inner_mut(), dev);
    }
}

/// Shared system-memory → default-pool staging-copy tail for `UpdateSurface` / `UpdateTexture`.
///
/// `src_parent` and
/// `dst_parent` are distinct, live `Direct3DTexture9` pointers; `copy` runs the
/// per-mip staging copies once pool and format are validated. Validates source
/// `D3DPOOL_SYSTEMMEM`, destination `D3DPOOL_DEFAULT`, and a format pair that is
/// either identical or one the CPU converter covers, then publishes the write
/// ([`publish_staging_write`]).
fn copy_systemmem_to_default(
    dst_parent: *mut crate::texture::Direct3DTexture9,
    src_parent: *mut crate::texture::Direct3DTexture9,
    copy: impl FnOnce(&mut crate::texture::TextureInner, &crate::texture::TextureInner) -> i32,
) -> i32 {
    // SAFETY: `src_parent` is a live texture pointer, distinct from `dst_parent`
    // (the caller checks `ptr::eq`), so this immutable borrow does not alias the
    // mutable `dst_parent` borrow below.
    let src_tex = unsafe { &*src_parent };
    // SAFETY: `dst_parent` is a live texture pointer, distinct from `src_parent`.
    let dst_tex = unsafe { &mut *dst_parent };
    if src_tex.d3d_pool() != D3DPOOL_SYSTEMMEM || dst_tex.d3d_pool() != D3DPOOL_DEFAULT {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET, "reject Update*: pool mismatch → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    // D3D9 accepts a source and a destination of different formats and
    // converts. The CPU codec covers the simple uncompressed colour formats;
    // a pair outside it (block-compressed, depth, YUV destinations) has no
    // conversion to run, so it is the one mismatch still rejected.
    let (src_fmt, dst_fmt) = (src_tex.d3d_format(), dst_tex.d3d_format());
    if src_fmt != dst_fmt && !mtld3d_core::pixel_convert::can_convert_update(src_fmt, dst_fmt) {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject Update*: no conversion for src=0x{src_fmt:x} into dst=0x{dst_fmt:x} → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: re-borrow of the immutable src inner via a raw pointer; the
    // allocation is distinct from the dst inner (src_parent != dst_parent).
    let src_inner = unsafe { &*core::ptr::from_ref(src_tex.inner()) };
    let hr = copy(dst_tex.inner_mut(), src_inner);
    // A copy that failed partway has still written the levels before the one
    // that failed, and those owe their upload like any other.
    publish_staging_write(dst_tex);
    hr
}

extern "system" fn device_update_surface(
    this: *mut c_void,
    src: *mut c_void,
    src_rect: *const c_void,
    dst: *mut c_void,
    dst_point: *const c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // SAFETY: vtable args; live `Direct3DSurface9` pointers per the ABI.
    let Some(src_surf) = (unsafe { InPtr::<crate::surface::Direct3DSurface9>::opt(src) }) else {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "reject UpdateSurface: null source surface → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: as above.
    let Some(dst_surf) = (unsafe { InPtr::<crate::surface::Direct3DSurface9>::opt(dst) }) else {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "reject UpdateSurface: null destination surface → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    };
    // D3D9 rejects UpdateSurface when either endpoint is mapped, and a held
    // device context maps the surface exactly as a `LockRect` does.
    if src_surf.is_locked()
        || dst_surf.is_locked()
        || src_surf.has_open_dc()
        || dst_surf.has_open_dc()
    {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "reject UpdateSurface: a locked or DC-held source/destination surface → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    let src_parent = src_surf.parent_texture();
    let dst_parent = dst_surf.parent_texture();
    // A planar YUV surface is a `StretchRect` source and nothing else. Its
    // level is taller than its logical height, which none of the staging
    // copies below size for, so a planar endpoint is refused before any of
    // them can run, whatever the other endpoint is.
    for parent in [src_parent, dst_parent] {
        // SAFETY: a non-null parent is the live `Direct3DTexture9` the surface
        // holds a reference on.
        if let Some(tex) = unsafe { parent.as_ref() }
            && mtld3d_core::stretch_rect::is_planar_yuv(tex.d3d_format())
        {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject UpdateSurface: planar YUV endpoint ({}) → INVALIDCALL",
                mtld3d_core::format::format_name(tex.d3d_format())
            );
            return D3DERR_INVALIDCALL;
        }
    }
    // A standalone offscreen *source* surface (not texture-backed) updating a
    // texture destination: copy its CPU backing into the dst texture's mip
    // staging and mark it dirty so a subsequent bind / StretchRect uploads it.
    if src_parent.is_null()
        && !dst_parent.is_null()
        && let Some((src_ptr, src_len, src_pitch, src_w, src_h, src_fmt)) =
            src_surf.system_memory_source()
    {
        // An `UpdateSurface` source is a `D3DPOOL_SYSTEMMEM` surface. A
        // `D3DPOOL_SCRATCH` one carries the same CPU backing but is not a
        // device resource, so it is never a valid endpoint of a device copy.
        if src_surf.standalone_pool() != D3DPOOL_SYSTEMMEM {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject UpdateSurface: standalone source outside D3DPOOL_SYSTEMMEM → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        let dst_level = dst_surf.mip_level() as usize;
        if map_d3d_format(src_fmt).is_none() {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject UpdateSurface: unmapped source format {} (0x{src_fmt:x}) → INVALIDCALL",
                mtld3d_core::format::format_name(src_fmt)
            );
            return D3DERR_INVALIDCALL;
        }
        // SAFETY: optional *const RECT / *const POINT per the ABI; null → None.
        let rect = (unsafe { ValueIn::<mtld3d_types::D3DRECT>::read_opt(src_rect) })
            .map(|r| (r.x1, r.y1, r.x2, r.y2));
        // SAFETY: as above; POINT is two i32 (x, y).
        let point =
            (unsafe { ValueIn::<[i32; 2]>::read_opt(dst_point) }).map_or((0, 0), Into::into);
        // SAFETY: `src_ptr`/`src_len` describe the live system-memory backing of
        // the source surface (kept alive while the surface is alive).
        let src_bytes = unsafe { std::slice::from_raw_parts(src_ptr, src_len) };
        // SAFETY: `dst_parent` is a live `Direct3DTexture9` whose refcount keeps
        // it alive while the destination surface is alive.
        let tex = unsafe { &mut *dst_parent };
        // An `UpdateSurface` destination is a `D3DPOOL_DEFAULT` surface; a
        // managed or system-memory destination has its own upload path and
        // never takes this one.
        if tex.d3d_pool() != D3DPOOL_DEFAULT {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject UpdateSurface: destination pool is not D3DPOOL_DEFAULT → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        // D3D9 accepts a source and a destination of different formats and
        // converts. The copy below re-encodes such a pair through the CPU
        // codec, which covers the simple uncompressed colour formats; a pair
        // outside it has no conversion to run, so it is the one mismatch still
        // rejected.
        let dst_fmt = tex.d3d_format();
        if src_fmt != dst_fmt && !mtld3d_core::pixel_convert::can_convert_update(src_fmt, dst_fmt) {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject UpdateSurface: no conversion for src=0x{src_fmt:x} into dst=0x{dst_fmt:x} → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        // The same region rules as a texture-level source: the rect inside the
        // source, the copy inside the destination level, and block-aligned
        // origins and extents for a block-compressed pair.
        if !tex
            .inner()
            .update_region_valid(dst_level, (src_w, src_h), rect, point)
        {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject UpdateSurface: standalone source rect/destination point out of bounds → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        let image = SourceImage {
            bytes: src_bytes,
            pitch: src_pitch,
            width: src_w,
            height: src_h,
            format: src_fmt,
        };
        let copied = if let Some(face) = dst_surf.cube_face() {
            tex.inner_mut()
                .update_bytes_to_cube_staging_region(face, dst_level, &image, rect, point)
        } else {
            tex.inner_mut()
                .update_bytes_to_staging_region(dst_level, &image, rect, point)
        };
        if !copied {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject UpdateSurface: staging copy failed → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        publish_staging_write(tex);
        return D3D_OK;
    }
    // A render-target surface or the back buffer: a colour surface with no
    // texture behind it, whose core texture takes the region directly.
    if dst_parent.is_null() && dst_surf.core_target().is_some() {
        // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
        let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
            return D3DERR_INVALIDCALL;
        };
        // SAFETY: optional *const RECT / *const POINT per the ABI; null → None.
        let rect = (unsafe { ValueIn::<mtld3d_types::D3DRECT>::read_opt(src_rect) })
            .map(|r| (r.x1, r.y1, r.x2, r.y2));
        // SAFETY: as above; POINT is two i32 (x, y).
        let point =
            (unsafe { ValueIn::<[i32; 2]>::read_opt(dst_point) }).map_or((0, 0), Into::into);
        return update_surface_into_color_target(obj.inner(), &src_surf, &dst_surf, rect, point);
    }
    if src_parent.is_null() || dst_parent.is_null() || std::ptr::eq(src_parent, dst_parent) {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "reject UpdateSurface: endpoints are not two distinct textures → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    let src_level = src_surf.mip_level() as usize;
    let dst_level = dst_surf.mip_level() as usize;
    // SAFETY: optional *const RECT / *const POINT per the ABI; null → None.
    let rect = (unsafe { ValueIn::<mtld3d_types::D3DRECT>::read_opt(src_rect) })
        .map(|r| (r.x1, r.y1, r.x2, r.y2));
    // SAFETY: as above; POINT is two i32 (x, y).
    let point = (unsafe { ValueIn::<[i32; 2]>::read_opt(dst_point) }).map_or((0, 0), Into::into);
    copy_systemmem_to_default(dst_parent, src_parent, |dst, src| {
        let src_extent = (src.mip_width(src_level), src.mip_height(src_level));
        if !dst.update_region_valid(dst_level, src_extent, rect, point) {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject UpdateSurface: source rect/destination point out of bounds → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        let Some(image) = src.surface_source_image(src_surf.cube_face(), src_level) else {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject UpdateSurface: source surface has no staging → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        };
        let copied = if let Some(dst_face) = dst_surf.cube_face() {
            dst.update_bytes_to_cube_staging_region(dst_face, dst_level, &image, rect, point)
        } else {
            dst.update_bytes_to_staging_region(dst_level, &image, rect, point)
        };
        if !copied {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject UpdateSurface: staging copy failed → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        D3D_OK
    })
}

/// `UpdateSurface` into a render-target surface or the back buffer.
///
/// Both are `D3DPOOL_DEFAULT` colour surfaces with no texture behind them, so
/// the region goes to their Metal texture as tightly packed rows, ordered
/// among the application's passes on the encoder thread. The source is a
/// `D3DPOOL_SYSTEMMEM` surface, standalone or a texture level, its region is
/// held to the rules a texture destination applies, and a pair of two
/// formats converts where the update codec covers it, as it does into a
/// texture. A multisampled destination is refused, as D3D9 refuses one. A
/// core device runs the same rejects and builds the same rows, then hands them
/// to `core_arms::update_color_target` in place of the encoder operation.
fn update_surface_into_color_target(
    dev: &mut DeviceInner,
    src_surf: &crate::surface::Direct3DSurface9,
    dst_surf: &crate::surface::Direct3DSurface9,
    src_rect: Option<(i32, i32, i32, i32)>,
    dst_point: (i32, i32),
) -> i32 {
    // Keyed by the reason, so each one is warned once.
    let reject = |key: u64, why: &str| {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: key,
            "reject UpdateSurface into a render target or back buffer: {why} → INVALIDCALL"
        );
        D3DERR_INVALIDCALL
    };
    if dst_surf.multi_sample().sample_count > 1 {
        return reject(1, "the destination is multisampled");
    }
    let src_parent = src_surf.parent_texture();
    let image = if src_parent.is_null() {
        if src_surf.standalone_pool() != D3DPOOL_SYSTEMMEM {
            return reject(2, "the source is not D3DPOOL_SYSTEMMEM");
        }
        let Some((ptr, len, pitch, width, height, format)) = src_surf.system_memory_source() else {
            return reject(3, "the source has no system-memory backing");
        };
        SourceImage {
            // SAFETY: `ptr`/`len` describe the live system-memory backing of
            // the source surface, which outlives this call.
            bytes: unsafe { std::slice::from_raw_parts(ptr, len) },
            pitch,
            width,
            height,
            format,
        }
    } else {
        // SAFETY: a non-null parent is the live `Direct3DTexture9` the source
        // surface holds a reference on.
        let tex = unsafe { &*src_parent };
        if tex.d3d_pool() != D3DPOOL_SYSTEMMEM {
            return reject(4, "the source is not D3DPOOL_SYSTEMMEM");
        }
        let Some(image) = tex
            .inner()
            .surface_source_image(src_surf.cube_face(), src_surf.mip_level() as usize)
        else {
            return reject(5, "the source level has no staging");
        };
        image
    };
    let dst_format = dst_surf.standalone_format();
    let dst_extent = (dst_surf.standalone_width(), dst_surf.standalone_height());
    // The rows go to the Metal texture as they are, so the destination's
    // storage has to be its D3D format's own layout, which every format a
    // render target or back buffer is created in has.
    let Some(dst_mapping) =
        crate::direct3d9::map_for_device(dst_format, dev.config().expand_packed16).filter(|m| {
            !mtld3d_core::upload_pass::is_expanded_upload(dst_format, m.metal_pixel_format())
        })
    else {
        return reject(
            6,
            "the destination format has no storage that takes its rows",
        );
    };
    if image.format != dst_format
        && !mtld3d_core::pixel_convert::can_convert_update(image.format, dst_format)
    {
        return reject(7, "no conversion between the two formats");
    }
    let Some((region, origin)) = mtld3d_core::dirty_rect::update_surface_region(
        src_rect,
        dst_point,
        (image.width, image.height),
        dst_extent,
        (1, 1),
    ) else {
        return reject(8, "the source rect or destination point leaves a surface");
    };
    let bpp = dst_mapping.bytes_per_pixel() as usize;
    let row_bytes = region.w as usize * bpp;
    let mut rows = vec![0u8; row_bytes * region.h as usize];
    if image.format == dst_format {
        for (row, out) in (region.y as usize..).zip(rows.chunks_exact_mut(row_bytes)) {
            let start = row * image.pitch + region.x as usize * bpp;
            let Some(src_row) = image.bytes.get(start..start + row_bytes) else {
                return reject(9, "the source backing is shorter than its rows");
            };
            out.copy_from_slice(src_row);
        }
    } else {
        let convert = mtld3d_core::pixel_convert::ConvertRegion {
            src_x: region.x,
            src_y: region.y,
            dst_x: 0,
            dst_y: 0,
            width: region.w,
            height: region.h,
            src_pitch: image.pitch,
            dst_pitch: row_bytes,
            src_slice_pitch: image.pitch * image.height as usize,
            dst_slice_pitch: rows.len(),
            depth: 1,
        };
        if !mtld3d_core::pixel_convert::convert_region(
            &mut rows,
            dst_format,
            image.bytes,
            image.format,
            &convert,
        ) {
            return reject(10, "the conversion ran past the source backing");
        }
    }
    core_arms::update_color_target(
        dev,
        dst_surf,
        origin,
        (region.w, region.h),
        &rows,
        row_bytes,
    )
}

extern "system" fn device_update_texture(
    this: *mut c_void,
    src: *mut c_void,
    dst: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if src.is_null() || dst.is_null() {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "reject UpdateTexture: null source or destination texture → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // All texture interfaces share this wrapper layout. The type flag below
    // selects the cube path; volumes take the plain path, whose staging copy
    // walks every depth slice of a level.
    let src_parent = src.cast::<crate::texture::Direct3DTexture9>();
    let dst_parent = dst.cast::<crate::texture::Direct3DTexture9>();
    if std::ptr::eq(src_parent.cast_const(), dst_parent.cast_const()) {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "reject UpdateTexture: source and destination are one texture → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // D3D9 pairs the two resources by type: a 2D texture only updates a 2D
    // texture, a cube a cube, a volume a volume. The container reports the
    // type it was created as, which is not the kind of the backing Metal
    // texture: a single-slice volume texture is backed 2D and is still a
    // volume here.
    // SAFETY: both pointers are non-null live base-texture wrappers. All three
    // texture interfaces share the same wrapper layout.
    let src_type = unsafe { (*src_parent).d3d_resource_type() };
    // SAFETY: same invariant as the source pointer above.
    let dst_type = unsafe { (*dst_parent).d3d_resource_type() };
    if src_type != dst_type {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "reject UpdateTexture: source and destination resource types differ → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    let src_is_cube = src_type == D3DRTYPE_CUBETEXTURE;
    if src_is_cube {
        let hr = copy_systemmem_to_default(dst_parent, src_parent, |dst, src| {
            let mut s = src.mip_width(0);
            let d = dst.mip_width(0);
            let mut src_skip = 0usize;
            while s > d {
                s >>= 1;
                src_skip += 1;
            }
            let levels = (src.app_level_count() as usize)
                .saturating_sub(src_skip)
                .min(dst.app_level_count() as usize);
            for face in 0..6 {
                for level in 0..levels {
                    let src_level = level + src_skip;
                    let Some(dr) = src.cube_update_dirty_rect(face, src_level) else {
                        continue;
                    };
                    let sw = src.mip_width(src_level);
                    let sh = src.mip_height(src_level);
                    let Some(c) = dr.clamp(sw, sh) else { continue };
                    if !c.reaches(dst.mip_width(level), dst.mip_height(level)) {
                        continue;
                    }
                    let copied = if c.x == 0 && c.y == 0 && c.w >= sw && c.h >= sh {
                        dst.update_cube_sub_region_from(
                            (face, level),
                            src,
                            face,
                            src_level,
                            None,
                            (0, 0),
                        )
                    } else {
                        let rect = (
                            c.x.cast_signed(),
                            c.y.cast_signed(),
                            (c.x + c.w).cast_signed(),
                            (c.y + c.h).cast_signed(),
                        );
                        dst.update_cube_sub_region_from(
                            (face, level),
                            src,
                            face,
                            src_level,
                            Some(rect),
                            (c.x.cast_signed(), c.y.cast_signed()),
                        )
                    };
                    if !copied {
                        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                            "reject UpdateTexture: cube staging copy failed → INVALIDCALL");
                        return D3DERR_INVALIDCALL;
                    }
                }
            }
            D3D_OK
        });
        if hr == D3D_OK {
            // SAFETY: `src_parent` remains live and distinct from `dst_parent`.
            unsafe { (*src_parent).inner_mut() }.clear_all_cube_update_dirty();
        }
        return hr;
    }
    let hr = copy_systemmem_to_default(dst_parent, src_parent, |dst, src| {
        // D3D9 matches src/dst mips by aligning the SMALLEST levels: when the
        // source top-level is larger than the destination's, skip the extra
        // source mips so source level `src_skip` lines up with dst level 0,
        // per the D3D9 UpdateTexture smallest-mip alignment rule. For an
        // equal-or-smaller source `src_skip` stays 0 and this is identical to
        // a plain index-aligned copy.
        let mut s = src
            .mip_width(0)
            .max(src.mip_height(0))
            .max(src.mip_depth(0));
        let d = dst
            .mip_width(0)
            .max(dst.mip_height(0))
            .max(dst.mip_depth(0));
        let mut src_skip = 0usize;
        while s > d {
            s >>= 1;
            src_skip += 1;
        }
        let levels = (src.app_level_count() as usize)
            .saturating_sub(src_skip)
            .min(dst.app_level_count() as usize);
        // D3D9 copies ONLY the source's dirty region per mip; a clean mip is a
        // no-op, and AddDirtyRect tracks a partial rectangle. The source's
        // dirty state is cleared after the
        // copy succeeds (below, outside the closure).
        for level in 0..levels {
            let src_level = level + src_skip;
            let Some(dr) = src.update_dirty_rect(src_level) else {
                continue; // clean → ignored
            };
            let sw = src.mip_width(src_level);
            let sh = src.mip_height(src_level);
            let Some(c) = dr.clamp(sw, sh) else { continue };
            if !c.reaches(dst.mip_width(level), dst.mip_height(level)) {
                continue;
            }
            let copied = if c.x == 0 && c.y == 0 && c.w >= sw && c.h >= sh {
                // Whole mip.
                dst.update_sub_region_from(level, src, src_level, None, (0, 0))
            } else {
                let rect = (
                    c.x.cast_signed(),
                    c.y.cast_signed(),
                    (c.x + c.w).cast_signed(),
                    (c.y + c.h).cast_signed(),
                );
                dst.update_sub_region_from(
                    level,
                    src,
                    src_level,
                    Some(rect),
                    (c.x.cast_signed(), c.y.cast_signed()),
                )
            };
            if !copied {
                mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                    "reject UpdateTexture: staging copy failed → INVALIDCALL");
                return D3DERR_INVALIDCALL;
            }
        }
        D3D_OK
    });
    if hr == D3D_OK {
        // D3D9 clears the source's dirty state after a successful UpdateTexture,
        // so a second copy from the now-clean source does nothing.
        // SAFETY: `src_parent` is a live texture distinct from `dst_parent`
        // (checked via `ptr::eq` above).
        unsafe { (*src_parent).inner_mut() }.clear_all_update_dirty();
        // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
        if let Some(obj) = unsafe { InPtrMut::<Direct3DDevice9>::opt(this) } {
            obj.inner().invalidate_core_draw();
        }
    }
    hr
}

/// Reject a read-back whose destination cannot take the source; `None` accepts it.
///
/// One warn per distinct reason, naming the entry point that reached it first,
/// since an application that reads back every frame retries a rejected call
/// every frame.
fn reject_readback(
    entry_point: &str,
    src: &ReadbackSource,
    dst: &SystemMemoryDst,
) -> Option<ReadbackReject> {
    let reason = mtld3d_core::readback::reject_readback_dst(
        src,
        &ReadbackDestination {
            width: dst.width,
            height: dst.height,
            format: dst.format,
            bytes_per_row: dst.bytes_per_row,
            len: dst.len,
        },
    )?;
    warn_readback_rejected(entry_point, reason);
    Some(reason)
}

/// One `log_once_warn_by!` line per (entry point, reason) pair.
fn warn_readback_rejected(entry_point: &str, reason: ReadbackReject) {
    mtld3d_shared::log_once_warn_by!(
        target: crate::LOG_TARGET,
        key: reason.key(),
        "reject {entry_point}: {} → INVALIDCALL",
        reason.as_str()
    );
}

extern "system" fn device_get_render_target_data(
    this: *mut c_void,
    rt: *mut c_void,
    dst: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    core_arms::get_render_target_data(this, rt, dst)
}

extern "system" fn device_get_front_buffer_data(
    this: *mut c_void,
    swap_chain: u32,
    dst_surface: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // Only the implicit swapchain is addressed by this device entry point.
    if swap_chain != 0 {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET,
            "reject GetFrontBufferData swapchain={swap_chain} → INVALIDCALL (expected 0)");
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: `dst_surface` is a caller-owned IDirect3DSurface9* per the ABI.
    let Some(dst_surf) = (unsafe { InPtr::<Direct3DSurface9>::opt(dst_surface) }) else {
        return D3DERR_INVALIDCALL;
    };
    obj.inner().read_front_buffer(&dst_surf)
}

extern "system" fn device_stretch_rect(
    this: *mut c_void,
    src: *mut c_void,
    src_rect: *const c_void,
    dst: *mut c_void,
    dst_rect: *const c_void,
    filter: u32,
) -> i32 {
    use mtld3d_core::stretch_rect::RejectReason;

    use crate::surface::Direct3DSurface9;

    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if src.is_null() || dst.is_null() {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "reject StretchRect: null src or dst → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // StretchRect accepts only the NONE/POINT/LINEAR texture filters.
    if !matches!(filter, D3DTEXF_NONE | D3DTEXF_POINT | D3DTEXF_LINEAR) {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "reject StretchRect: invalid filter {filter} → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };

    let src_surf = src.cast::<Direct3DSurface9>();
    let dst_surf = dst.cast::<Direct3DSurface9>();

    let dev = obj.inner();

    let Some(src_info) = resolve_stretch_surface(src_surf) else {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: RejectReason::UnsupportedSource.key(),
            "reject StretchRect: {} → INVALIDCALL",
            RejectReason::UnsupportedSource.as_str()
        );
        return D3DERR_INVALIDCALL;
    };
    let Some(dst_info) = resolve_stretch_surface(dst_surf) else {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: RejectReason::UnsupportedDestination.key(),
            "reject StretchRect: {} → INVALIDCALL",
            RejectReason::UnsupportedDestination.as_str()
        );
        return D3DERR_INVALIDCALL;
    };

    // Depth-stencil StretchRect: if either surface is a
    // depth-stencil, BOTH must be — and they must share the Metal depth format
    // and dimensions, sit in D3DPOOL_DEFAULT, and be copied 1:1 over the whole
    // surface (no sub-rect, scale, or flip). Anything else is INVALIDCALL. The
    // copy is a same-format Private→Private depth blit, or a depth resolve
    // when the source carries samples the destination does not.
    if src_info
        .flags
        .contains(StretchSurfaceFlags::IS_DEPTH_STENCIL)
        || dst_info
            .flags
            .contains(StretchSurfaceFlags::IS_DEPTH_STENCIL)
    {
        let eligible = src_info
            .flags
            .contains(StretchSurfaceFlags::IS_DEPTH_STENCIL)
            && dst_info
                .flags
                .contains(StretchSurfaceFlags::IS_DEPTH_STENCIL)
            && src_info.format == dst_info.format
            && src_info.width == dst_info.width
            && src_info.height == dst_info.height
            && src_info.pool == D3DPOOL_DEFAULT
            && dst_info.pool == D3DPOOL_DEFAULT;
        if !eligible {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject StretchRect: depth-stencil pair must match format/size and be both depth → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        // Sample counts decide the transport. An equal pair is a copy. A
        // multisampled source into a single-sampled destination is the resolve
        // D3D9 defines for reading a multisampled surface. Neither remaining
        // pair has a Metal shape: the blit encoder cannot change the sample
        // count, and the resolve unit only ever reduces to one sample.
        let resolve = src_info.sample_count > 1 && dst_info.sample_count == 1;
        if src_info.sample_count != dst_info.sample_count && !resolve {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject StretchRect: depth-stencil sample counts {} → {} are neither equal nor a \
                 resolve → INVALIDCALL",
                src_info.sample_count,
                dst_info.sample_count
            );
            return D3DERR_INVALIDCALL;
        }
        let Some((src_region, dst_region)) =
            parse_stretch_regions(src_rect, dst_rect, &src_info, &dst_info)
        else {
            return D3DERR_INVALIDCALL;
        };
        // Only a full-surface 1:1 copy is supported for depth.
        if src_region.x != 0
            || src_region.y != 0
            || dst_region.x != 0
            || dst_region.y != 0
            || src_region.w != src_info.width
            || src_region.h != src_info.height
            || dst_region.w != dst_info.width
            || dst_region.h != dst_info.height
        {
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "reject StretchRect: depth-stencil copy must be full-surface 1:1 → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        return core_arms::stretch_depth(this, [src, dst], [&src_info, &dst_info]);
    }

    // D3D9 StretchRect eligibility: both surfaces must be D3DPOOL_DEFAULT, and
    // the surface classes must form a pair `class_reject` allows (a texture
    // source is valid into a render target, the CAN_STRETCHRECT_FROM_TEXTURES
    // cap we advertise).
    if src_info.pool != D3DPOOL_DEFAULT || dst_info.pool != D3DPOOL_DEFAULT {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "reject StretchRect: src/dst not D3DPOOL_DEFAULT → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    let rects_given = !src_rect.is_null() || !dst_rect.is_null();
    // An extended device also copies between two default-pool surfaces of
    // no eligible class, a texture level that is no render target included,
    // when the call is a whole-surface copy of one format and size.
    let extended_copy = mtld3d_core::stretch_rect::extended_whole_surface_copy(
        dev.is_extended(),
        rects_given,
        (src_info.format, src_info.width, src_info.height),
        (dst_info.format, dst_info.width, dst_info.height),
    );
    if let Some(reason) = mtld3d_core::stretch_rect::class_reject(src_info.flags, dst_info.flags)
        && !extended_copy
    {
        log_pair_reject(
            reason,
            (src, &src_info, src_rect),
            (dst, &dst_info, dst_rect),
            dev.is_extended(),
        );
        return D3DERR_INVALIDCALL;
    }

    let expand_packed16 = dev.config().expand_packed16;
    // The planar YUV formats have no entry in the format lookup, so the
    // generic format gate would refuse them as unmappable. A pair with a
    // planar endpoint is judged by its own route below, once the rects say
    // whether the copy scales.
    let planar_endpoint = mtld3d_core::stretch_rect::is_planar_yuv(src_info.format)
        || mtld3d_core::stretch_rect::is_planar_yuv(dst_info.format);
    if !planar_endpoint
        && let Err(hr) = check_stretch_rect_formats(&src_info, &dst_info, expand_packed16)
    {
        return hr;
    }
    let Some((src_region, dst_region)) =
        parse_stretch_regions(src_rect, dst_rect, &src_info, &dst_info)
    else {
        return D3DERR_INVALIDCALL;
    };

    let scaling = src_region.w != dst_region.w || src_region.h != dst_region.h;
    // D3D9 copies between two rects of one surface only 1:1. A surface is one
    // COM object per subresource, so two levels or two faces of one texture are
    // two surfaces and may stretch. So may an additional swap chain's back
    // buffer and the device's, which alias one texture here but are two
    // surfaces to the application. The back-buffer indices of one swap chain
    // are the exception: this layer hands every index the same surface object
    // over one texture, so a stretch between two of them, which D3D9 allows,
    // is refused here as a stretch inside one surface.
    if scaling && core::ptr::eq(src, dst) {
        log_pair_reject(
            RejectReason::SameSurfaceStretch,
            (src, &src_info, src_rect),
            (dst, &dst_info, dst_rect),
            dev.is_extended(),
        );
        return D3DERR_INVALIDCALL;
    }
    // A planar source decodes through the render quad into a render target and
    // through the CPU converter, 1:1, into an offscreen plain; both are the
    // cross-format branches below. Everything else with a planar endpoint is
    // refused here, ahead of the upload flush, so a rejected call schedules
    // nothing.
    if let mtld3d_core::stretch_rect::PlanarStretch::Reject(reason) =
        mtld3d_core::stretch_rect::planar_stretch_route(
            src_info.format,
            dst_info.format,
            dst_info
                .flags
                .contains(StretchSurfaceFlags::IS_RENDER_TARGET),
            scaling,
        )
    {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: reason.key(),
            "reject StretchRect: {} (src={} 0x{:x}, dst={} 0x{:x}) → INVALIDCALL",
            reason.as_str(),
            mtld3d_core::format::format_name(src_info.format),
            src_info.format,
            mtld3d_core::format::format_name(dst_info.format),
            dst_info.format
        );
        return D3DERR_INVALIDCALL;
    }
    // A scaling StretchRect needs a render pass that samples the source onto
    // the destination quad (Metal's blit encoder can't scale). That requires
    // the destination to be a render target — an offscreen-plain destination
    // can't be rendered into, so a scale into one stays INVALIDCALL
    // (offscreen→offscreen scaling is rejected). Same-size
    // blits take the 1:1 copy path below — unless they also convert format.
    if scaling
        && !dst_info
            .flags
            .contains(StretchSurfaceFlags::IS_RENDER_TARGET)
    {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: RejectReason::Scaling.key(),
            "reject StretchRect: {} into non-render-target dst (src={}x{}, dst={}x{}) → INVALIDCALL",
            RejectReason::Scaling.as_str(),
            src_region.w, src_region.h,
            dst_region.w, dst_region.h
        );
        return D3DERR_INVALIDCALL;
    }
    core_arms::stretch_colour(
        this,
        &obj,
        &core_arms::StretchCall {
            surfaces: [src, dst],
            infos: [&src_info, &dst_info],
            regions: [src_region, dst_region],
            rects: [src_rect, dst_rect],
            planar_endpoint,
            expand_packed16,
            filter,
        },
    )
}

/// Warn once per rule that a `StretchRect` pair broke a rule on its two surfaces.
///
/// The rules are the surface-class matrix and the refusal of a stretch inside
/// one surface. The line names the rule, then for each endpoint its class,
/// usage, format, level extent, mip level and the rect the call passed, and
/// whether the device is extended, so a game log alone says which pair the
/// application asked for and why an extended device's whole-surface copy did
/// not take it. The arguments are formatted only when the line fires, so each
/// rule names the first pair that broke it.
fn log_pair_reject(
    reason: mtld3d_core::stretch_rect::RejectReason,
    src: (*mut c_void, &StretchSurfaceInfo, *const c_void),
    dst: (*mut c_void, &StretchSurfaceInfo, *const c_void),
    extended: bool,
) {
    mtld3d_shared::log_once_warn_by!(
        target: crate::LOG_TARGET,
        key: reason.key(),
        "reject StretchRect: {} (src={}, dst={}, {} device) → INVALIDCALL",
        reason.as_str(),
        describe_stretch_endpoint(src.0, src.1, src.2),
        describe_stretch_endpoint(dst.0, dst.1, dst.2),
        if extended { "extended" } else { "plain" }
    );
}

/// One `StretchRect` endpoint for a rejection line.
///
/// Reads as `texture level usage=0x0 A8R8G8B8 0x15 256x256 level 0 rect
/// whole`, the rect being the `D3DRECT` the call passed or `whole` for a null
/// one.
fn describe_stretch_endpoint(
    surface: *mut c_void,
    info: &StretchSurfaceInfo,
    rect: *const c_void,
) -> String {
    use crate::surface::Direct3DSurface9;

    // SAFETY: `surface` is the live IDirect3DSurface9 the game passed to this
    // `StretchRect` call.
    let usage = unsafe { InPtr::<Direct3DSurface9>::opt(surface) }.map_or(0, |s| s.d3d_usage());
    // SAFETY: `rect` is the `*const D3DRECT` in-param of the same call.
    let rect = unsafe { ValueIn::<mtld3d_types::D3DRECT>::read_opt(rect) }.map_or_else(
        || String::from("whole"),
        |r| format!("({}, {})-({}, {})", r.x1, r.y1, r.x2, r.y2),
    );
    format!(
        "{} usage=0x{usage:x} {} 0x{:x} {}x{} level {} rect {rect}",
        info.class_name(),
        mtld3d_core::format::format_name(info.format),
        info.format,
        info.width,
        info.height,
        info.mip_level
    )
}

/// Claim a `StretchRect` or `ColorFill` destination subresource for the GPU.
///
/// A texture-backed destination carries CPU staging, and both the 1:1 blit and
/// the render quad write only its Metal texture, so a `LockRect` or a `GetDC`
/// of that subresource has to read it back instead of serving the staging the
/// write never reached. The claim is per (face, level): a cube face's five
/// siblings keep whatever their own staging holds. Every source kind lands
/// here, including one with no CPU staging of its own to copy from. Marking is
/// all this does; the read happens at the next map.
fn claim_dst_subresource_for_gpu(
    dst_surf: *mut crate::surface::Direct3DSurface9,
    dst_info: &StretchSurfaceInfo,
) {
    if dst_surf.is_null() {
        return;
    }
    // SAFETY: caller-supplied live `Direct3DSurface9*` from the StretchRect or
    // ColorFill thunk (non-null checked above).
    let dst_parent = unsafe { (*dst_surf).parent_texture() };
    if dst_parent.is_null() {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "StretchRect/ColorFill: texture-backed destination has no parent texture → \
             its staging keeps what it held"
        );
        return;
    }
    // SAFETY: `dst_parent` is non-null (checked) and a live `Direct3DTexture9`
    // kept alive by the destination surface's reference.
    unsafe { &mut *dst_parent }
        .inner_mut()
        .mark_subresource_gpu_authoritative(
            dst_info.slice.unwrap_or(0),
            dst_info.mip_level as usize,
        );
}

/// CPU-side cross-format `StretchRect` into an offscreen-plain destination.
///
/// The 1:1 blit cannot convert formats, and the render-quad conversion needs a
/// render-target destination. Decode each source pixel and re-encode it into the
/// destination texture's staging, then schedule the staging→texture upload
/// (`flush_dirty_mips`) so a later sample sees the converted pixels; a later
/// `LockRect` reads the same converted staging. Both surfaces are offscreen-
/// plain here, so both are texture-backed. Unsupported pairs are rejected
/// before this path schedules an upload or changes destination contents.
fn convert_stretch_dst_staging(
    _obj: &Direct3DDevice9,
    src_surf: *mut crate::surface::Direct3DSurface9,
    dst_surf: *mut crate::surface::Direct3DSurface9,
    src_info: &StretchSurfaceInfo,
    dst_info: &StretchSurfaceInfo,
    src_region: mtld3d_core::stretch_rect::StretchRegion,
    dst_region: mtld3d_core::stretch_rect::StretchRegion,
) -> i32 {
    if src_surf.is_null() || dst_surf.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if !mtld3d_core::pixel_convert::can_convert(src_info.format, dst_info.format) {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "StretchRect: cross-format offscreen pair (src=0x{:x}, dst=0x{:x}) not CPU-convertible → INVALIDCALL",
            src_info.format, dst_info.format);
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: caller-supplied live `Direct3DSurface9*` from the StretchRect
    // thunk (non-null checked above).
    let src_parent = unsafe { (*src_surf).parent_texture() };
    // SAFETY: as above.
    let dst_parent = unsafe { (*dst_surf).parent_texture() };
    if src_parent.is_null() || dst_parent.is_null() || src_parent == dst_parent {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "StretchRect: cross-format offscreen dst has no distinct texture backing → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: non-null (checked), distinct from `dst_parent`, and a live
    // `Direct3DTexture9` kept alive by the source surface's reference.
    let src_tex = unsafe { &mut *src_parent };
    if !src_tex.inner_mut().materialize_subresource_for_cpu_read(
        src_info.slice.unwrap_or(0),
        src_info.mip_level as usize,
    ) {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: non-null (checked), distinct from `src_parent`, and a live
    // `Direct3DTexture9` kept alive by the destination surface's reference.
    let dst_tex = unsafe { &mut *dst_parent };
    let src_rect = (
        src_region.x.cast_signed(),
        src_region.y.cast_signed(),
        (src_region.x + src_region.w).cast_signed(),
        (src_region.y + src_region.h).cast_signed(),
    );
    let converted = dst_tex.inner_mut().convert_sub_region_from(
        dst_info.mip_level as usize,
        src_tex.inner(),
        src_info.mip_level as usize,
        Some(src_rect),
        (dst_region.x.cast_signed(), dst_region.y.cast_signed()),
    );
    if !converted {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "StretchRect: cross-format offscreen pair (src=0x{:x}, dst=0x{:x}) staging conversion failed → INVALIDCALL",
            src_info.format,
            dst_info.format
        );
        return D3DERR_INVALIDCALL;
    }
    D3D_OK
}

/// Whether the 1:1 blit, a copy of the texture's bytes, is the copy D3D9 defines.
///
/// Compares the *Metal* pixel formats, not the D3D codes: distinct D3D
/// formats can share a single Metal format (e.g. A8R8G8B8 + X8R8G8B8 are both
/// `Bgra8Unorm`, and on a device without the packed 16-bit formats A1R5G5B5
/// and A4R4G4B4 are too). A shared storage does not make every pair a byte
/// copy, though. A source without alpha into a destination with alpha would
/// hand the source's padding to the destination as alpha where D3D9 writes
/// alpha one (`exposes_padding_as_alpha`, judged on the mappings this device
/// created), so that pair converts; the other direction stays a byte copy,
/// since every reader of a destination without alpha ignores the alpha it
/// carries. A packed YUV endpoint lays its bytes out in an order no other
/// format shares (`reinterprets_packed_yuv`), so such a pair converts or is
/// refused like a pair of two storages. A core device's `StretchRect` asks it of its two
/// surfaces' formats too (DXMTRS C4b-3d5).
fn copies_bytes(src_format: u32, dst_format: u32, expand_packed16: bool) -> bool {
    let src_map = crate::direct3d9::map_for_device(src_format, expand_packed16);
    let dst_map = crate::direct3d9::map_for_device(dst_format, expand_packed16);
    let exposes_padding = matches!(
        (&src_map, &dst_map),
        (Some(s), Some(d)) if mtld3d_core::stretch_rect::exposes_padding_as_alpha(s, d)
    );
    src_map.map(|m| m.metal_pixel_format()) == dst_map.map(|m| m.metal_pixel_format())
        && !exposes_padding
        && !mtld3d_core::stretch_rect::reinterprets_packed_yuv(src_format, dst_format)
}

/// Gate a colour `StretchRect` pair on a transport that copies or converts it.
fn check_stretch_rect_formats(
    src: &StretchSurfaceInfo,
    dst: &StretchSurfaceInfo,
    expand_packed16: bool,
) -> Result<(), i32> {
    // Device-aware mapping, so the comparison sees the Metal formats the
    // textures were actually created with on this device.
    let src_mtl = crate::direct3d9::map_for_device(src.format, expand_packed16)
        .map(|m| m.metal_pixel_format());
    let dst_mtl = crate::direct3d9::map_for_device(dst.format, expand_packed16)
        .map(|m| m.metal_pixel_format());
    // A pair `copies_bytes` accepts takes the 1:1 copy path. Any other pair
    // converts either via the render-quad path (sample src → write the dst
    // render target — needs a render-target destination) or, into an
    // offscreen-plain destination (which can't be rendered into), via the CPU
    // converter in `device_stretch_rect` (the offscreen→offscreen cross-format
    // path). An offscreen pair needs an available CPU codec before either
    // endpoint schedules an upload. Unmappable formats are always rejected.
    let convertible = src_mtl.is_some()
        && dst_mtl.is_some()
        && (copies_bytes(src.format, dst.format, expand_packed16)
            || dst.flags.contains(StretchSurfaceFlags::IS_RENDER_TARGET)
            || (dst
                .flags
                .contains(StretchSurfaceFlags::IS_OFFSCREEN_PLAIN_DEFAULT)
                && mtld3d_core::pixel_convert::can_convert(src.format, dst.format)));
    if !convertible {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: mtld3d_core::stretch_rect::RejectReason::FormatMismatch.key(),
            "reject StretchRect: {} (src={} 0x{:x}, dst={} 0x{:x}) → INVALIDCALL",
            mtld3d_core::stretch_rect::RejectReason::FormatMismatch.as_str(),
            mtld3d_core::format::format_name(src.format),
            src.format,
            mtld3d_core::format::format_name(dst.format),
            dst.format
        );
        return Err(D3DERR_INVALIDCALL);
    }
    Ok(())
}

/// Parse the source and destination rects against the surface dims.
///
/// Returns `Some((src_region, dst_region))` on success, `None` on any
/// rejection (an empty or inverted rect, or one with an edge outside its
/// surface), which callers map to `D3DERR_INVALIDCALL`. A rejection is
/// warned once per reason, naming both rects.
///
/// A size mismatch between the two regions is NOT rejected here: that's a
/// scaling request, which `device_stretch_rect` routes to the render-quad
/// path when the destination is a render target (and only rejects when the
/// destination can't be rendered into — e.g. an offscreen-plain surface).
fn parse_stretch_regions(
    src_rect: *const c_void,
    dst_rect: *const c_void,
    src_info: &StretchSurfaceInfo,
    dst_info: &StretchSurfaceInfo,
) -> Option<(
    mtld3d_core::stretch_rect::StretchRegion,
    mtld3d_core::stretch_rect::StretchRegion,
)> {
    use mtld3d_core::stretch_rect::parse_rect;
    use mtld3d_types::D3DRECT;

    // SAFETY: vtable in-params; `src_rect`/`dst_rect` are *const D3DRECT per ABI.
    let extracted_src =
        unsafe { ValueIn::<D3DRECT>::read_opt(src_rect) }.map(|r| (r.x1, r.y1, r.x2, r.y2));
    // SAFETY: see above.
    let extracted_dst =
        unsafe { ValueIn::<D3DRECT>::read_opt(dst_rect) }.map(|r| (r.x1, r.y1, r.x2, r.y2));
    let parsed = parse_rect(extracted_src, src_info.width, src_info.height).and_then(|src| {
        parse_rect(extracted_dst, dst_info.width, dst_info.height).map(|dst| (src, dst))
    });
    match parsed {
        Ok(regions) => Some(regions),
        Err(reason) => {
            mtld3d_shared::log_once_warn_by!(
                target: crate::LOG_TARGET,
                key: reason.key(),
                "reject StretchRect: {} (src rect {extracted_src:?} on {}x{}, dst rect \
                 {extracted_dst:?} on {}x{}) → INVALIDCALL",
                reason.as_str(),
                src_info.width,
                src_info.height,
                dst_info.width,
                dst_info.height
            );
            None
        }
    }
}

/// A `StretchRect` or `ColorFill` endpoint, `None` for a surface neither takes.
fn resolve_stretch_surface(
    surf: *mut crate::surface::Direct3DSurface9,
) -> Option<StretchSurfaceInfo> {
    if surf.is_null() {
        return None;
    }
    // SAFETY: `surf` is non-null (checked above) and is the
    // caller-supplied surface pointer from a `StretchRect` thunk; the
    // caller must pass live surface wrappers.
    let s = unsafe { &*surf };
    let parent = s.parent_texture();
    if !parent.is_null() {
        // SAFETY: `parent` is non-null (checked above) and points to a
        // live `Direct3DTexture9` whose refcount keeps it alive while
        // the surface is alive.
        let tex = unsafe { &*parent };
        let level = s.mip_level();
        let lvl_idx = level as usize;
        let mut flags = StretchSurfaceFlags::empty();
        flags.set(
            StretchSurfaceFlags::IS_RENDER_TARGET,
            (tex.d3d_usage() & D3DUSAGE_RENDERTARGET) != 0,
        );
        flags.set(
            StretchSurfaceFlags::IS_OFFSCREEN_PLAIN_DEFAULT,
            s.owns_parent_texture(),
        );
        let (width, height) = (
            tex.inner().mip_width(lvl_idx),
            tex.inner().mip_height(lvl_idx),
        );
        let texture_size = TargetExtent::mip_level(
            tex.inner().render_scale(),
            (width, height),
            tex.inner().render_extent(),
            tex.inner().metal_level(lvl_idx),
        )
        .texture();
        return Some(StretchSurfaceInfo {
            is_texture: true,
            width,
            height,
            texture_size,
            scale: tex.inner().render_scale(),
            format: tex.d3d_format(),
            mip_level: level,
            slice: s.cube_face(),
            pool: tex.d3d_pool(),
            flags,
            autogen_texture_id: (tex.inner().autogen_mipmap() && level == 0)
                .then(|| tex.texture_id()),
            sample_count: 1,
        });
    }
    if s.core_target().is_some() {
        return Some(standalone_stretch_info(
            s,
            StretchSurfaceFlags::IS_RENDER_TARGET,
        ));
    }
    if s.core_depth().is_some() {
        return Some(standalone_stretch_info(
            s,
            StretchSurfaceFlags::IS_DEPTH_STENCIL,
        ));
    }
    None
}

/// A standalone surface's `StretchRect` info: a back buffer, a render target or a depth-stencil.
fn standalone_stretch_info(
    s: &crate::surface::Direct3DSurface9,
    flags: StretchSurfaceFlags,
) -> StretchSurfaceInfo {
    StretchSurfaceInfo {
        is_texture: false,
        width: s.standalone_width(),
        height: s.standalone_height(),
        texture_size: TargetExtent::whole(
            s.render_scale(),
            (s.standalone_width(), s.standalone_height()),
        )
        .texture(),
        scale: s.render_scale(),
        format: s.standalone_format(),
        mip_level: 0,
        slice: None,
        pool: D3DPOOL_DEFAULT,
        flags,
        autogen_texture_id: None,
        sample_count: s.multi_sample().sample_count,
    }
}

/// Resolve a `ColorFill` rect against the destination mip extent.
///
/// `rect` is the caller's `D3DRECT`; `None` fills the whole mip. Edges are
/// clipped to the surface, so a rect that hangs over an edge fills the part
/// that lands on it, and one that misses entirely returns `None` for the
/// caller to treat as a no-op.
fn color_fill_region(rect: Option<mtld3d_types::D3DRECT>, extent: (u32, u32)) -> Option<DirtyRect> {
    let Some(r) = rect else {
        return DirtyRect::full(extent.0, extent.1).clamp(extent.0, extent.1);
    };
    let x = r.x1.max(0).cast_unsigned();
    let y = r.y1.max(0).cast_unsigned();
    let right = r.x2.max(0).cast_unsigned();
    let bottom = r.y2.max(0).cast_unsigned();
    DirtyRect {
        x,
        y,
        w: right.saturating_sub(x),
        h: bottom.saturating_sub(y),
    }
    .clamp(extent.0, extent.1)
}

/// True when a `ColorFill` region lands on the destination's block grid.
///
/// A block-compressed fill that splits a block is `INVALIDCALL`; an edge that
/// ends on the surface boundary is exempt, because the last block there is
/// partial anyway. Uncompressed formats have a 1x1 grid, so this is inert.
fn color_fill_block_aligned(region: DirtyRect, extent: (u32, u32), block: (u32, u32)) -> bool {
    let (bw, bh) = (block.0.max(1), block.1.max(1));
    if bw == 1 && bh == 1 {
        return true;
    }
    let right = region.x + region.w;
    let bottom = region.y + region.h;
    region.x.is_multiple_of(bw)
        && region.y.is_multiple_of(bh)
        && (right.is_multiple_of(bw) || right == extent.0)
        && (bottom.is_multiple_of(bh) || bottom == extent.1)
}

/// `ColorFill` a lockable `D3DPOOL_DEFAULT` offscreen-plain surface.
///
/// Marks the filled rect dirty so the core texture takes it when next flushed.
fn color_fill_offscreen_plain(
    dev: &mut DeviceInner,
    parent: *mut Direct3DTexture9,
    info: &StretchSurfaceInfo,
    region: DirtyRect,
    color: u32,
) -> i32 {
    // Encode the fill colour into the destination format. An unmapped format
    // still succeeds but leaves the surface unfilled (the colour check, not
    // the ColorFill return, is what would fail for those).
    let Some(pixel) = convert::d3dcolor_fill_pixel_bytes(color, info.format) else {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "ColorFill: no fill encoding for format {} → surface left unfilled", info.format);
        return D3D_OK;
    };
    if pixel.is_empty() {
        return D3D_OK;
    }
    // SAFETY: `parent` is non-null (the caller classified this surface as
    // texture-backed) and its refcount keeps it alive while the surface is
    // alive; access is exclusive: D3D9 objects are single-threaded, or
    // serialised by the device `ApiLock` under `D3DCREATE_MULTITHREADED`.
    let ti = unsafe { &mut *parent }.inner_mut();
    let level = info.mip_level;
    if !ti.fill_staging_region(
        level as usize,
        region.x,
        region.y,
        region.w,
        region.h,
        &pixel,
    ) {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "ColorFill: staging fill of level {level} fell outside the mip → surface left unfilled");
        return D3D_OK;
    }
    ti.mark_mip_dirty_rect(level as usize, region);
    dev.mark_snapshot_dirty_all();
    D3D_OK
}

extern "system" fn device_color_fill(
    this: *mut c_void,
    surface: *mut c_void,
    rect: *const c_void,
    color: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Frame);
    if surface.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable in-param; `rect` is *const D3DRECT per the D3D9 ABI.
    let rect = unsafe { ValueIn::<mtld3d_types::D3DRECT>::read_opt(rect) };
    let surf = surface.cast::<Direct3DSurface9>();
    let Some(info) = resolve_stretch_surface(surf) else {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "ColorFill on a surface with no DEFAULT-pool backing → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    };
    // D3D9: ColorFill takes a DEFAULT-pool render target (a texture level, a
    // `CreateRenderTarget` surface or the back buffer) or a DEFAULT-pool
    // offscreen-plain surface. Managed / sysmem / scratch, a depth-stencil
    // surface and an ordinary DEFAULT texture level are all INVALIDCALL.
    if info.pool != D3DPOOL_DEFAULT
        || !info.flags.intersects(
            StretchSurfaceFlags::IS_RENDER_TARGET | StretchSurfaceFlags::IS_OFFSCREEN_PLAIN_DEFAULT,
        )
    {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "ColorFill: surface is not a DEFAULT render target or offscreen-plain → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    // A planar YUV plain has no fill encoding: a D3DCOLOR names no Y, U, V
    // triple the layer could defend, the same position the packed YUV formats
    // are in. The call succeeds and the surface keeps its bytes. Answered
    // here because the format lookup below has no entry for the format and
    // would warn on every call.
    if mtld3d_core::stretch_rect::is_planar_yuv(info.format) {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "ColorFill: no fill encoding for planar YUV format {} → surface left unfilled",
            mtld3d_core::format::format_name(info.format));
        return D3D_OK;
    }
    let extent = (info.width, info.height);
    let Some(region) = color_fill_region(rect, extent) else {
        return D3D_OK;
    };
    if let Some(fmt) = map_d3d_format(info.format)
        && !color_fill_block_aligned(region, extent, (fmt.block_width(), fmt.block_height()))
    {
        return D3DERR_INVALIDCALL;
    }
    if info.flags.contains(StretchSurfaceFlags::IS_RENDER_TARGET) {
        return core_arms::color_fill(obj.inner(), surf, &info, region, color);
    }
    // SAFETY: an offscreen-plain surface is texture-backed, so
    // `resolve_stretch_surface` classified it off a non-null parent.
    let parent = unsafe { &*surf }.parent_texture();
    color_fill_offscreen_plain(obj.inner(), parent, &info, region, color)
}

extern "system" fn device_create_offscreen_plain_surface(
    this: *mut c_void,
    width: u32,
    height: u32,
    format: u32,
    pool: u32,
    surface: *mut *mut c_void,
    shared_handle: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if surface.is_null() || width == 0 || height == 0 {
        null_out(surface);
        return D3DERR_INVALIDCALL;
    }
    // `D3DPOOL_MANAGED_EX` is the managed pool, which no plain surface takes.
    let pool = mtld3d_core::pool::resolve(pool);
    // A plain device refuses a non-null pSharedHandle with E_NOTIMPL; an
    // extended one reads it as user memory for a system-memory surface and as
    // a shared resource, which it does not create, in the default pool.
    let user_memory = match shared_handle_gate(
        this,
        &mtld3d_core::extended::CreateKind::OffscreenPlain,
        pool,
        shared_handle,
        "CreateOffscreenPlainSurface",
    ) {
        Ok(memory) => memory,
        Err(hr) => {
            null_out(surface);
            return hr;
        }
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        null_out(surface);
        return D3DERR_INVALIDCALL;
    };
    // Raw `*mut DeviceInner` copied out so the DEFAULT branch can call
    // `device_create_texture` (which re-derives its own device borrow) without
    // holding a live borrow from `obj` across the call.
    let device_inner = obj.inner_ptr();
    // D3DPOOL_DEFAULT: a lockable, GPU-resident offscreen surface. Back it with
    // an internal color texture (CPU staging for LockRect + a Metal texture for
    // StretchRect) and hand back its owned level-0 surface: lock/unlock/upload
    // reuse the texture machinery, StretchRect resolves the texture handle. No
    // D3DUSAGE_* (offscreen plain), single mip. A core device takes the same arm:
    // the texture keeps its staging and records nothing; a lock, a device
    // context, `ColorFill` and `UpdateSurface` write that staging, and
    // `StretchRect` refuses it.
    if pool == D3DPOOL_DEFAULT {
        let mut tex_out: *mut c_void = core::ptr::null_mut();
        let hr = create_texture_path(&TextureCreateArgs {
            this,
            width,
            height,
            levels: 1,
            usage: 0,
            format,
            pool: D3DPOOL_DEFAULT,
            texture: &raw mut tex_out,
            shared_handle: core::ptr::null_mut(),
            offscreen_plain: true,
        });
        if hr != D3D_OK || tex_out.is_null() {
            warn!(target: LOG_TARGET,
                "reject CreateOffscreenPlainSurface({width}x{height}, format={format}, DEFAULT) → INVALIDCALL (internal texture create failed)");
            null_out(surface);
            return D3DERR_INVALIDCALL;
        }
        let tex_ptr = tex_out.cast::<crate::texture::Direct3DTexture9>();
        let surf = Direct3DSurface9::new_owned_texture_backed(device_inner, tex_ptr);
        // The internal texture (created just above) forwards the device
        // reference, so this owned surface is NOT registered (no double-count).
        // SAFETY: vtable out-param; `surface` is *mut *mut c_void per IDirect3DDevice9 ABI.
        unsafe { OutPtr::write_opt(surface, Box::into_raw(Box::new(surf)).cast::<c_void>()) };
        return D3D_OK;
    }
    // A planar YUV surface holds its chroma planes after its luma rows, so it
    // is half again as large as the pitch-times-height backing the CPU pools
    // below allocate. Only the DEFAULT path above sizes it from the planar
    // layout; a CPU-pool surface would let the application write its chroma
    // past the end of the allocation.
    if mtld3d_core::stretch_rect::is_planar_yuv(format) {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "reject CreateOffscreenPlainSurface(format={}, pool={pool}) → INVALIDCALL (planar \
             YUV surfaces exist in D3DPOOL_DEFAULT only)",
            mtld3d_core::format::format_name(format));
        null_out(surface);
        return D3DERR_INVALIDCALL;
    }
    // Otherwise a CPU/system-memory offscreen surface: D3DPOOL_SYSTEMMEM (the
    // destination of GetRenderTargetData / GetFrontBufferData) and D3DPOOL_SCRATCH
    // (e.g. a cursor bitmap) are both lockable
    // system-RAM surfaces, served by the same backing. D3DPOOL_MANAGED offscreen
    // surfaces are not supported.
    if pool != D3DPOOL_SYSTEMMEM && pool != D3DPOOL_SCRATCH {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "CreateOffscreenPlainSurface(pool={pool}) → INVALIDCALL (only D3DPOOL_SYSTEMMEM / D3DPOOL_SCRATCH / D3DPOOL_DEFAULT supported)");
        null_out(surface);
        return D3DERR_INVALIDCALL;
    }
    let Some(fmt) = map_d3d_format(format) else {
        warn!(target: LOG_TARGET,
            "reject CreateOffscreenPlainSurface({width}x{height}, format={format}) → INVALIDCALL (no format mapping)");
        null_out(surface);
        return D3DERR_INVALIDCALL;
    };
    // Block-compressed (DXTn) offscreen surfaces must be block-aligned in
    // width/height, like textures.
    if is_dxt_format(format)
        && (!width.is_multiple_of(fmt.block_width()) || !height.is_multiple_of(fmt.block_height()))
    {
        null_out(surface);
        return D3DERR_INVALIDCALL;
    }
    // Block-compressed formats (DXT*, ATI*) have no bytes-per-pixel — size the
    // backing by their block grid (`ceil(dim/block) * block_bytes`). Linear
    // formats size as `aligned_pitch * height`, where the pitch rounds the
    // `width * bpp` row stride up to a 4-byte boundary — matching the pitch
    // `systemmem_lock_rect` reports, so the last locked row stays in bounds.
    let bpp = fmt.bytes_per_pixel();
    let bytes = if bpp == 0 {
        let blocks_w = (width as usize).div_ceil(fmt.block_width().max(1) as usize);
        let blocks_h = (height as usize).div_ceil(fmt.block_height().max(1) as usize);
        blocks_w
            .saturating_mul(blocks_h)
            .saturating_mul(fmt.block_bytes() as usize)
    } else {
        (linear_row_pitch(width, bpp) as usize).saturating_mul(height as usize)
    };
    // The surface's bytes are the one allocation of the create that may fail.
    let Some(mut backing) = PageBox::try_new_zeroed(bytes) else {
        return refuse_unallocatable_staging("CreateOffscreenPlainSurface", surface);
    };
    // An extended device's system-memory surface starts with the
    // application's packed pixels, copied once at the lock pitch.
    if let Some(memory) = user_memory {
        let rows = mtld3d_core::extended::PackedRows::of_level(
            width,
            height,
            bpp,
            (fmt.block_width(), fmt.block_height(), fmt.block_bytes()),
        );
        let pitch = if bpp == 0 {
            rows.row_bytes
        } else {
            linear_row_pitch(width, bpp) as usize
        };
        copy_user_memory(memory, &rows, backing.as_mut_slice(), pitch);
    }
    let surf =
        Direct3DSurface9::new_system_memory(obj.inner_ptr(), width, height, format, pool, backing);
    let surf_ptr = Box::into_raw(Box::new(surf));
    // SAFETY: `surf_ptr` is a freshly created, live system-memory surface at
    // refcount 1.
    unsafe { crate::com_ref::com_register_child(surf_ptr) };
    // SAFETY: vtable out-param; `surface` is *mut *mut c_void per IDirect3DDevice9 ABI.
    unsafe { OutPtr::write_opt(surface, surf_ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_set_render_target(
    this: *mut c_void,
    index: u32,
    surface: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::RtDs);
    if index >= D3D_MAX_SIMULTANEOUS_RENDERTARGETS {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject SetRenderTarget(index={index}) → INVALIDCALL (four simultaneous render targets)"
        );
        return D3DERR_INVALIDCALL;
    }
    let slot = usize::try_from(index).expect("index < 4 fits usize");

    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();

    if surface.is_null() {
        if slot == 0 {
            // D3D9 spec: RT0 must remain non-null.
            mtld3d_shared::log_once_warn!(
                target: LOG_TARGET,
                "reject SetRenderTarget(index=0, null) → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        // Render targets 1..3 may be unbound; draws then leave the slot alone.
        dev.unbind_extra_render_target(slot);
        return D3D_OK;
    }

    // Pull width/height via GetDesc through the surface vtable so we cover
    // both standalone (CreateRenderTarget) and texture-backed (GetSurfaceLevel
    // on a render-target texture) cases uniformly.
    let surf = surface.cast::<Direct3DSurface9>();
    // A render target created by a DIFFERENT device is INVALIDCALL, and (because
    // the call fails) must leave the currently-bound RT0 untouched. Every
    // surface records its owning device
    // (used by GetDevice); compare it against this device before any mutation.
    // WoW uses a single device, so this never fires in-game.
    // SAFETY: `surf` is non-null (validated above) and points to a live surface.
    if unsafe { (*surf).device_inner() } != obj.inner_ptr() {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject SetRenderTarget: surface owned by a different device → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: `surf` is non-null (validated by `surface.is_null()` check
    // earlier in this fn) and points to a live `Direct3DSurface9` whose
    // refcount keeps it alive while bound on the device.
    let vtbl = unsafe { (*surf).vtbl() };
    let mut desc = mtld3d_types::D3DSURFACE_DESC {
        format: 0,
        resource_type: 0,
        usage: 0,
        pool: 0,
        multi_sample_type: 0,
        multi_sample_quality: 0,
        width: 0,
        height: 0,
    };
    // SAFETY: calling the just-loaded `get_desc` thunk through the
    // surface vtable with `surface` as `this` and `desc` as the writable
    // out-pointer.
    if unsafe { (vtbl.get_desc)(surface, &raw mut desc) } != 0 {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET, "reject SetRenderTarget: GetDesc failed");
        return D3DERR_INVALIDCALL;
    }
    // The destination must be render-target-capable.
    // GetDesc reports the surface's true usage — the parent texture's
    // D3DUSAGE_RENDERTARGET for a GetSurfaceLevel surface, or RENDERTARGET for
    // the implicit backbuffer / a CreateRenderTarget surface — so every
    // legitimate render-target bind passes.
    if desc.usage & D3DUSAGE_RENDERTARGET == 0 {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject SetRenderTarget: surface is not a render target (usage={:#x}) → INVALIDCALL",
            desc.usage
        );
        return D3DERR_INVALIDCALL;
    }
    let (target, parent, logical_size) = {
        // SAFETY: `surf` is the live surface validated above.
        let surf = unsafe { &*surf };
        let parent = surf.parent_texture();
        let logical_size = if surf.is_back_buffer() {
            None
        } else if !parent.is_null() {
            // SAFETY: `parent` is non-null (checked) and live while `surf` holds its
            // reference.
            let tex = unsafe { &*parent };
            let lvl = surf.mip_level() as usize;
            Some((tex.inner().mip_width(lvl), tex.inner().mip_height(lvl)))
        } else {
            Some((surf.standalone_width(), surf.standalone_height()))
        };
        (surf.core_target(), parent, logical_size)
    };
    let untargeted = "IDirect3DDevice9::SetRenderTarget (a surface the core does not target)";
    let Some(target) = target else {
        crate::device::refuse_under_core(untargeted);
        return mtld3d_types::D3DERR_NOTAVAILABLE;
    };
    // SAFETY: null, or the surface's parent texture, live while the surface is and distinct
    // from the device.
    if let Some(parent) = unsafe { parent.as_mut() } {
        crate::texture::rehydrate_for_device(parent, dev);
        // Its core texture exists from here, with every CPU write so far uploaded.
        let _ = crate::texture::core_flush_dirty(parent.inner_mut(), &mut dev.core);
    }
    let bound = if slot == 0 {
        let ok = dev.core.set_target(target);
        if ok {
            dev.core.set_target_logical_size(logical_size);
        }
        ok
    } else {
        dev.core.set_extra_target(slot, Some(target))
    };
    if !bound {
        crate::device::refuse_under_core(untargeted);
        return mtld3d_types::D3DERR_NOTAVAILABLE;
    }
    // SAFETY: as above.
    let new_autogen = unsafe { parent.as_ref() }
        .and_then(|pt| pt.inner().autogen_mipmap().then(|| pt.texture_id()));
    if let Some(old_id) = dev.cur_autogen_rt_ids[slot].take()
        && Some(old_id) != new_autogen
        && dev.latched_encoder_status().is_ok()
    {
        dev.core.texture_generate_mips(old_id);
    }
    dev.cur_autogen_rt_ids[slot] = new_autogen;
    // The surface the core device now draws to at the slot: a draw asks it its format.
    dev.bound_rt_mut()
        .replace_render_target(slot, surf, desc.width, desc.height);
    if slot != 0 {
        dev.invalidate_core_draw();
        return D3D_OK;
    }
    dev.set_viewport(mtld3d_types::D3DVIEWPORT9 {
        x: 0,
        y: 0,
        width: desc.width,
        height: desc.height,
        min_z: 0.0,
        max_z: 1.0,
    });
    dev.set_scissor_rect(mtld3d_core::render_state::full_target_scissor(
        desc.width,
        desc.height,
    ));
    dev.mark_snapshot_dirty(SnapshotDirty::RT_DS | SnapshotDirty::STAGES | SnapshotDirty::RS);
    D3D_OK
}

extern "system" fn device_get_render_target(
    this: *mut c_void,
    index: u32,
    surface: *mut *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::RtDs);
    if surface.is_null() || index >= D3D_MAX_SIMULTANEOUS_RENDERTARGETS {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject GetRenderTarget(index={index}) → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    let slot = usize::try_from(index).expect("index < 4 fits usize");
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let inner = obj.inner();

    // If a custom RT was set via SetRenderTarget, hand it back per D3D9 spec
    // (caller releases). Otherwise the default RT is the device-owned implicit
    // backbuffer surface — a single cached object returned by every call (the
    // `pRenderTarget == pBackBuffer` and refcount-0 identity the suite checks),
    // resolving its Metal handle live so StretchRect / LockRect readback see the
    // current backbuffer. Render targets 1..3 have no default: an unbound slot
    // reports `D3DERR_NOTFOUND` with a null out-pointer.
    let bound = inner.bound_rt().render_target(slot);
    let surf = if bound.is_null() {
        if slot != 0 {
            // SAFETY: `surface` is the caller's out-pointer per the D3D9 ABI; alignment is not
            // assumed.
            unsafe { OutPtr::write_opt(surface, core::ptr::null_mut()) };
            return crate::D3DERR_NOTFOUND;
        }
        inner.get_or_create_implicit_render_target()
    } else {
        bound
    };
    // SAFETY: `surf` is non-null — either the live bound RT (refcount keeps it
    // alive while bound) or the live cached implicit RT. Its AddRef thunk
    // forwards to the device on the implicit surface's 0→1 transition.
    let add_ref = unsafe { (*surf).vtbl().add_ref };
    // SAFETY: calling the surface's AddRef thunk; D3D9 mandates AddRef on return.
    unsafe { add_ref(surf.cast::<c_void>()) };
    // SAFETY: `surface` is the caller's out-pointer per the D3D9 ABI; alignment is not assumed.
    unsafe { OutPtr::write_opt(surface, surf.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_set_depth_stencil_surface(
    this: *mut c_void,
    surface: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::RtDs);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let surf = surface.cast::<Direct3DSurface9>();
    if !surf.is_null() {
        // SAFETY: `surf` is non-null (checked) and a live `Direct3DSurface9`.
        let vtbl = unsafe { (*surf).vtbl() };
        let mut desc = mtld3d_types::D3DSURFACE_DESC {
            format: 0,
            resource_type: 0,
            usage: 0,
            pool: 0,
            multi_sample_type: 0,
            multi_sample_quality: 0,
            width: 0,
            height: 0,
        };
        // SAFETY: the `get_desc` thunk with `surface` as `this` and `desc` as
        // the writable out-pointer.
        if unsafe { (vtbl.get_desc)(surface, &raw mut desc) } != 0
            || desc.usage & D3DUSAGE_DEPTHSTENCIL == 0
        {
            mtld3d_shared::log_once_warn!(
                target: LOG_TARGET,
                "reject SetDepthStencilSurface: surface is not a depth-stencil (usage={:#x}) → INVALIDCALL",
                desc.usage
            );
            return D3DERR_INVALIDCALL;
        }
    }
    let unattached =
        "IDirect3DDevice9::SetDepthStencilSurface (a surface the core does not attach)";
    // SAFETY: null, or the live surface validated above.
    let (depth, sized_depth) = if let Some(surf) = unsafe { surf.as_ref() } {
        let own = surf.device_inner() == obj.inner_ptr();
        let Some(depth) = surf.core_depth().filter(|_| own) else {
            crate::device::refuse_under_core(unattached);
            return mtld3d_types::D3DERR_NOTAVAILABLE;
        };
        // SAFETY: null, or the surface's parent texture, live while the surface is and
        // distinct from the device.
        let sized = if let Some(parent) = unsafe { surf.parent_texture().as_mut() } {
            crate::texture::rehydrate_for_device(parent, dev);
            let _ = crate::texture::core_flush_dirty(parent.inner_mut(), &mut dev.core);
            let lvl = surf.mip_level() as usize;
            Some((
                depth,
                parent.inner().mip_width(lvl),
                parent.inner().mip_height(lvl),
            ))
        } else {
            None
        };
        (Some(depth), sized)
    } else {
        (None, None)
    };
    if !dev.core.set_depth(depth) {
        crate::device::refuse_under_core(unattached);
        return mtld3d_types::D3DERR_NOTAVAILABLE;
    }
    if dev.config().depth_alias_same_size
        && let (Some((prev_depth, pw, ph)), Some((cur_depth, cw, ch))) =
            (dev.last_sized_depth, sized_depth)
        && prev_depth != cur_depth
        && (pw, ph) == (cw, ch)
        && dev.core.copy_depth(prev_depth, cur_depth)
    {
        dev.core.note_stretch_depth_copy(prev_depth, cur_depth);
    }
    dev.last_sized_depth = sized_depth;
    dev.bound_rt_mut().replace_depth_stencil(surf);
    dev.flags
        .set(DeviceFlags::DEPTH_EXPLICITLY_UNBOUND, surf.is_null());
    dev.invalidate_core_draw();
    D3D_OK
}

extern "system" fn device_get_depth_stencil_surface(
    this: *mut c_void,
    surface: *mut *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::RtDs);
    if surface.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // `GetDepthStencilSurface` reflects the currently *bound* depth-stencil, not
    // merely the device's auto depth-stencil. Three shapes, in order:
    // - a surface bound by `SetDepthStencilSurface` is handed back as itself, so
    //   an application comparing the pointer against its own surface matches and
    //   a `saved = Get; Set(other); ...; Set(saved)` sequence rebinds what was
    //   actually bound rather than the auto depth;
    // - an explicit `SetDepthStencilSurface(NULL)` unbinds depth, so report
    //   "none bound" (NOTFOUND) even though the auto depth texture still exists;
    // - otherwise the auto depth-stencil is in effect, and stands for the
    //   device-owned implicit depth-stencil surface (cached, refcount-0,
    //   container = the device). A single object across calls, depth-backed so
    //   the save/restore above restores the real Metal depth handle (resolved
    //   live). Null when the device has no auto depth-stencil.
    let dev = obj.inner();
    let bound = dev.bound_rt().depth_stencil();
    let surf = if bound.is_null() {
        if dev.flags.contains(DeviceFlags::DEPTH_EXPLICITLY_UNBOUND) {
            core::ptr::null_mut()
        } else {
            dev.get_or_create_implicit_depth_stencil()
        }
    } else {
        bound
    };
    if surf.is_null() {
        // SAFETY: `surface` is non-null (checked above) and per the D3D9 ABI
        // points to a writable `*mut c_void` slot owned by the caller; alignment is not assumed.
        unsafe { OutPtr::write_opt(surface, core::ptr::null_mut()) };
        return crate::D3DERR_NOTFOUND;
    }
    // SAFETY: `surf` is non-null: either the live bound depth-stencil (the
    // bind refcount keeps it alive) or the live cached implicit DS surface,
    // whose AddRef thunk forwards to the device on the 0→1 transition.
    let add_ref = unsafe { (*surf).vtbl().add_ref };
    // SAFETY: calling the surface's AddRef thunk; D3D9 mandates AddRef on return.
    unsafe { add_ref(surf.cast::<c_void>()) };
    // SAFETY: vtable out-param; `surface` is *mut *mut c_void per IDirect3DDevice9 ABI; alignment
    // is not assumed.
    unsafe { OutPtr::write_opt(surface, surf.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_begin_scene(this: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Frame);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // D3D9 forbids nested scenes: BeginScene while already in one fails.
    if dev.flags.contains(DeviceFlags::IN_SCENE) {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET, "BeginScene while already in a scene → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    dev.flags.insert(DeviceFlags::IN_SCENE);
    0 // S_OK
}

extern "system" fn device_end_scene(this: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Frame);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // EndScene without a matching BeginScene fails.
    if !dev.flags.contains(DeviceFlags::IN_SCENE) {
        mtld3d_shared::log_once_warn!(target: LOG_TARGET, "EndScene without BeginScene → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    dev.flags.remove(DeviceFlags::IN_SCENE);
    0 // S_OK
}

/// Read the `count` `D3DRECT`s an `IDirect3DDevice9::Clear` supplies at `rects`.
///
/// Each becomes an `(x1, y1, x2, y2)` tuple. `count == 0` or a null pointer means "no
/// rects" → empty (the caller then clears the whole viewport). Defensive
/// against a null pointer carried with a non-zero count.
fn clear_target_rects(count: u32, rects: *const c_void) -> Vec<(i32, i32, i32, i32)> {
    if count == 0 || rects.is_null() {
        return Vec::new();
    }
    // SAFETY: per the Clear ABI, `rects` points to `count` contiguous, caller-
    // owned `D3DRECT`s when non-null (checked above); read-only access here, and
    // alignment is not assumed.
    let slice = unsafe {
        mtld3d_shared::slice_from_caller(rects.cast::<mtld3d_types::D3DRECT>(), count as usize)
    };
    slice.iter().map(|r| (r.x1, r.y1, r.x2, r.y2)).collect()
}

extern "system" fn device_clear(
    this: *mut c_void,
    count: u32,
    rects: *const c_void,
    flags: u32,
    color: u32,
    z: f32,
    stencil: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Frame);
    core_arms::clear(this, count, rects, flags, color, z, stencil)
}

extern "system" fn device_set_transform(
    this: *mut c_void,
    state: u32,
    matrix: *const c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    // SAFETY: vtable in-param; `matrix` is *const D3DMATRIX per ABI.
    let Some(m) = (unsafe { ValueIn::<D3DMATRIX>::read_opt(matrix) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::Transform { state, matrix: m });
        return D3D_OK;
    }
    // Unknown D3DTS_* indices (vertex blending etc.) are silently accepted.
    dev.ff_state_mut().set_transform(state, &m);
    // A matrix feeds the FF VS constants only, never its key or row count.
    let mask = dev.ff_vs_write_mask(false) | dev.transform_write_side_dirty(state);
    dev.mark_snapshot_dirty(mask);
    0 // S_OK
}

extern "system" fn device_get_transform(this: *mut c_void, state: u32, matrix: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    if matrix.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let m = dev
        .ff_state()
        .transform(state)
        .copied()
        .unwrap_or(D3DMATRIX::IDENTITY);
    // SAFETY: `matrix` is non-null (checked above) and per the D3D9
    // ABI points to a writable `D3DMATRIX` slot owned by the caller; alignment is not assumed.
    unsafe {
        OutPtr::write_opt(matrix.cast::<D3DMATRIX>(), m);
    }
    0 // S_OK
}

extern "system" fn device_multiply_transform(
    this: *mut c_void,
    state: u32,
    matrix: *const c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    // SAFETY: vtable in-param; `matrix` is *const D3DMATRIX per ABI.
    let Some(rhs) = (unsafe { ValueIn::<D3DMATRIX>::read_opt(matrix) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // MultiplyTransform is NOT a recordable state-block operation in D3D9: even
    // inside a Begin/EndStateBlock it mutates the live device transform
    // immediately and is never captured into the block (GetTransform after
    // EndStateBlock returns the multiplied matrix, and a later Capture/Apply
    // does not restore it). So
    // always apply to live FF state, regardless of recording.
    dev.ff_state_mut().multiply_transform(state, &rhs);
    // A matrix feeds the FF VS constants only, never its key or row count.
    let mask = dev.ff_vs_write_mask(false) | dev.transform_write_side_dirty(state);
    dev.mark_snapshot_dirty(mask);
    0 // S_OK
}

extern "system" fn device_set_viewport(this: *mut c_void, viewport: *const c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::ViewScissor);
    // SAFETY: vtable in-param; `viewport` is *const D3DVIEWPORT9 per ABI.
    let Some(v) = (unsafe { ValueIn::<D3DVIEWPORT9>::read_opt(viewport) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::Viewport(v));
        return D3D_OK;
    }
    dev.set_viewport(v);
    D3D_OK
}

extern "system" fn device_get_viewport(this: *mut c_void, viewport: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::ViewScissor);
    if viewport.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // SAFETY: vtable out-param; `viewport` is *mut D3DVIEWPORT9 per IDirect3DDevice9 ABI.
    unsafe { OutPtr::write_opt(viewport.cast::<D3DVIEWPORT9>(), dev.viewport()) };
    D3D_OK
}

extern "system" fn device_set_material(this: *mut c_void, material: *const c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    // SAFETY: vtable in-param; `material` is *const D3DMATERIAL9 per ABI.
    let Some(m) = (unsafe { ValueIn::<D3DMATERIAL9>::read_opt(material) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::Material(m));
        return D3D_OK;
    }
    // The material feeds the constant sections alone, never the FF VS key.
    dev.ff_state_mut().set_material(&m);
    let mask = dev.ff_aware_mask(SnapshotDirty::VS_CONST);
    dev.mark_snapshot_dirty(mask);
    0 // S_OK
}

extern "system" fn device_get_material(this: *mut c_void, material: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    if material.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // SAFETY: vtable out-param; `material` is *mut D3DMATERIAL9 per IDirect3DDevice9 ABI.
    unsafe { OutPtr::write_opt(material.cast::<D3DMATERIAL9>(), *dev.ff_state().material()) };
    0 // S_OK
}

extern "system" fn device_set_light(this: *mut c_void, index: u32, light: *const c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    // SAFETY: vtable in-param; `light` is *const D3DLIGHT9 per ABI.
    let Some(l) = (unsafe { ValueIn::<D3DLIGHT9>::read_opt(light) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::Light { index, light: l });
        return D3D_OK;
    }
    let inputs = dev.ff_state().vs_source_light_inputs();
    dev.ff_state_mut().set_light_at(index, &l);
    let mask = dev.ff_vs_write_mask(dev.ff_state().vs_source_light_inputs() != inputs);
    dev.mark_snapshot_dirty(mask);
    0 // S_OK
}

extern "system" fn device_get_light(this: *mut c_void, index: u32, light: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    if light.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // D3D9: GetLight fails (leaving the caller's buffer untouched) for a slot
    // that was never defined via SetLight/LightEnable.
    let Some(l) = dev.ff_state().get_light_at(index) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: `light` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `D3DLIGHT9` slot owned by the caller; alignment is not assumed.
    unsafe {
        OutPtr::write_opt(light.cast::<D3DLIGHT9>(), l);
    }
    0 // S_OK
}

extern "system" fn device_light_enable(this: *mut c_void, index: u32, enable: i32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let on = enable != 0;
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record_light_enable(index, on);
        return D3D_OK;
    }
    let inputs = dev.ff_state().vs_source_light_inputs();
    dev.ff_state_mut().set_light_enabled_at(index, on);
    let mask = dev.ff_vs_write_mask(dev.ff_state().vs_source_light_inputs() != inputs);
    dev.mark_snapshot_dirty(mask);
    0 // S_OK
}

extern "system" fn device_get_light_enable(this: *mut c_void, index: u32, enable: *mut i32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    if enable.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // D3D9: GetLightEnable fails (leaving the caller's BOOL untouched) for a
    // slot that was never defined via SetLight/LightEnable.
    if !dev.ff_state().is_light_defined_at(index) {
        return D3DERR_INVALIDCALL;
    }
    // D3D9 reports the enabled flag as 128 (not 1).
    let enabled = if dev.ff_state().is_light_enabled_at(index) {
        128
    } else {
        0
    };
    // SAFETY: `enable` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `BOOL` (i32) slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(enable, enabled) };
    0 // S_OK
}

extern "system" fn device_set_clip_plane(this: *mut c_void, index: u32, plane: *const f32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    if plane.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: `plane` is non-null (checked) and per the D3D9 ABI points to the
    // 4 readable f32 plane-equation coefficients (A, B, C, D); alignment is not assumed.
    let coeffs = unsafe { plane.cast::<[f32; 4]>().read_unaligned() };
    let dev = obj.inner();
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::ClipPlane {
            index,
            plane: coeffs,
        });
        return D3D_OK;
    }
    dev.set_clip_plane(index, coeffs);
    0 // S_OK
}

extern "system" fn device_get_clip_plane(this: *mut c_void, index: u32, plane: *mut f32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::FfFixed);
    if plane.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let coeffs = obj.inner().clip_plane(index);
    // SAFETY: `plane` is non-null (checked) and per the D3D9 ABI points to 4
    // writable f32 slots owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(plane.cast::<[f32; 4]>(), coeffs) };
    0 // S_OK
}

extern "system" fn device_set_render_state(this: *mut c_void, state: u32, value: u32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::RenderState);
    // A state no D3D9 render state names (1 to 6, or past `D3DRS_BLENDOPALPHA`)
    // is accepted and ignored.
    if (1..mtld3d_types::D3DRS_ZENABLE).contains(&state) || (state as usize) >= RENDER_STATE_COUNT {
        return ignore_unnamed_render_state(state);
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // The 'RESZ' hack: this magic written to POINTSIZE asks the driver to
    // resolve the bound depth-stencil into the depth texture bound at
    // stage 0. Advertised via the RESZ fourcc in CheckDeviceFormat;
    // engines on the matching vendor path use it as their only way to
    // hand scene depth to a second depth consumer.
    if state == D3DRS_POINTSIZE
        && value == 0x7fa0_5000
        && let Err(hr) = core_arms::resz(dev)
    {
        return hr;
    }
    if let Some(rec) = dev.recording_state_block_mut() {
        if state == D3DRS_POINTSIZE {
            rec.record(StateOp::PointSize {
                raw: value,
                numeric: mtld3d_core::multisample::numeric_point_size(value),
                coverage: mtld3d_core::multisample::a2m_control(value),
            });
        } else {
            rec.record(StateOp::RenderState { state, value });
        }
        return D3D_OK;
    }
    // Skip the dirty mark only when raw and derived state are unchanged.
    let changed = dev.set_render_state(state as usize, value);
    if changed {
        let mut mask = dev.ff_aware_mask(rs_dirty_mask(state));
        // The user clip plane count is a key input of the programmable VS as
        // well, which `ff_aware_mask` would otherwise strip.
        if matches!(state, D3DRS_CLIPPLANEENABLE | D3DRS_CLIPPING) {
            mask |= SnapshotDirty::VS_SOURCE;
        }
        dev.mark_snapshot_dirty(mask);
    }
    dev.perf_mut()
        .record_keys_gate(KeysGate::SetRenderState, !changed);
    0 // S_OK
}

extern "system" fn device_get_render_state(this: *mut c_void, state: u32, value: *mut u32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::RenderState);
    if value.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // States 1 to 6 and past 255 are refused; state 0 and the unnamed states
    // up to 255 read as zero.
    if (1..mtld3d_types::D3DRS_ZENABLE).contains(&state) || state > 255 {
        return reject_unreadable_render_state(state);
    }
    if state == 0 || (state as usize) >= RENDER_STATE_COUNT {
        warn_unnamed_render_state_read(state);
        // SAFETY: `value` is non-null (checked above) and per the D3D9 ABI
        // points to a writable `u32` slot owned by the caller; alignment is not assumed.
        unsafe { OutPtr::write_opt(value, 0) };
        return D3D_OK;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // SAFETY: `value` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `u32` slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(value, dev.render_state(state as usize)) };
    0 // S_OK
}

/// The `D3D_OK` `SetRenderState` answers for a state no D3D9 render state names, which it ignores.
///
/// Out of line, like the other index rejects below, so the state and sampler
/// setters do not build the log arguments on their hot path.
#[cold]
#[inline(never)]
fn ignore_unnamed_render_state(state: u32) -> i32 {
    mtld3d_shared::log_once_warn!(
        target: LOG_TARGET,
        "SetRenderState({state}): no render state has that index → ignored, D3D_OK"
    );
    D3D_OK
}

/// The `INVALIDCALL` `GetRenderState` answers for states 1 to 6 and past 255.
#[cold]
#[inline(never)]
fn reject_unreadable_render_state(state: u32) -> i32 {
    mtld3d_shared::log_once_warn!(
        target: LOG_TARGET,
        "GetRenderState({state}): outside the readable render states → INVALIDCALL"
    );
    D3DERR_INVALIDCALL
}

/// Warn once that `GetRenderState` read zero for state 0 or an unnamed state up to 255.
#[cold]
#[inline(never)]
fn warn_unnamed_render_state_read(state: u32) {
    mtld3d_shared::log_once_warn!(
        target: LOG_TARGET,
        "GetRenderState({state}): no render state has that index → 0, D3D_OK"
    );
}

extern "system" fn device_create_state_block(
    this: *mut c_void,
    type_: u32,
    sb: *mut *mut c_void,
) -> i32 {
    use crate::state_block::Direct3DStateBlock9;
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::StateBlock);
    if sb.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // CreateStateBlock during an open BeginStateBlock recording is INVALIDCALL —
    // reject before creating/registering any block so the device refcount is
    // unchanged.
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    if let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) })
        && obj.inner().is_state_block_recording()
    {
        warn!(target: LOG_TARGET, "reject CreateStateBlock during BeginStateBlock recording → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    match Direct3DStateBlock9::capture(this.cast::<Direct3DDevice9>(), type_) {
        Ok(obj) => {
            // SAFETY: vtable out-param; `sb` is *mut *mut c_void per IDirect3DDevice9 ABI.
            let sb_ptr = Box::into_raw(Box::new(obj));
            // SAFETY: `sb_ptr` is a freshly created, live state block at refcount 1.
            unsafe { crate::com_ref::com_register_child(sb_ptr) };
            // SAFETY: vtable out-param; `sb` is *mut *mut c_void per the ABI.
            unsafe { OutPtr::write_opt(sb, sb_ptr.cast::<c_void>()) };
            D3D_OK
        }
        Err(e) => e,
    }
}

extern "system" fn device_begin_state_block(this: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::StateBlock);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if !dev.begin_state_block_recording() {
        // The rejection leaves the open recording alone, per the D3D9
        // contract, so an application that begins a block and never ends it
        // has every later `BeginStateBlock` rejected until the next `Reset`
        // clears the recording. One misstep, not one per call: warn once.
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "BeginStateBlock called while another recording is in progress → INVALIDCALL. \
             A recording left open by an earlier BeginStateBlock rejects every later one \
             until the next Reset."
        );
        return D3DERR_INVALIDCALL;
    }
    D3D_OK
}

extern "system" fn device_end_state_block(this: *mut c_void, sb: *mut *mut c_void) -> i32 {
    use crate::state_block::Direct3DStateBlock9;

    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::StateBlock);
    if sb.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let obj_ptr = this.cast::<Direct3DDevice9>();
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per
    // IDirect3DDevice9 ABI.
    let obj = unsafe { &mut *obj_ptr };
    let dev = obj.inner();
    let Some(recording) = dev.end_state_block_recording() else {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "EndStateBlock without matching BeginStateBlock → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    };
    let block = Direct3DStateBlock9::from_recording(obj_ptr, *recording);
    // SAFETY: vtable out-param; `sb` is *mut *mut c_void per IDirect3DDevice9 ABI.
    let sb_ptr = Box::into_raw(Box::new(block));
    // SAFETY: `sb_ptr` is a freshly created, live state block at refcount 1.
    unsafe { crate::com_ref::com_register_child(sb_ptr) };
    // SAFETY: vtable out-param; `sb` is *mut *mut c_void per the ABI.
    unsafe { OutPtr::write_opt(sb, sb_ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_set_clip_status(this: *mut c_void, _status: *const c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "stub IDirect3DDevice9::SetClipStatus → INVALIDCALL");
    D3DERR_INVALIDCALL
}

extern "system" fn device_get_clip_status(this: *mut c_void, _status: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "stub IDirect3DDevice9::GetClipStatus → INVALIDCALL");
    D3DERR_INVALIDCALL
}

extern "system" fn device_get_texture(
    this: *mut c_void,
    stage: u32,
    texture: *mut *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Texture);
    if texture.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let vertex_slot = vertex_sampler_slot(stage);
    if vertex_slot.is_none() && stage as usize >= STAGE_COUNT {
        warn_invalid_sampler("GetTexture", stage);
        // SAFETY: `texture` is non-null (checked above) and per the D3D9 ABI points to a writable
        // `*mut c_void` slot owned by the caller; alignment is not assumed.
        unsafe { OutPtr::write_opt(texture, core::ptr::null_mut()) };
        return D3D_OK;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let tex_ptr = vertex_slot.map_or_else(
        || dev.stage_bindings().texture(stage as usize),
        |slot| dev.vertex_textures[slot].raw(),
    );
    if !tex_ptr.is_null() {
        // SAFETY: `tex_ptr` is non-null (checked above) and points to a
        // live `Direct3DTexture9` whose refcount keeps it alive while
        // bound on the device.
        let add_ref = unsafe { (*tex_ptr).vtbl().add_ref };
        // SAFETY: calling the just-loaded `add_ref` thunk through the
        // texture vtable; D3D9 mandates AddRef on out-pointer returns.
        unsafe { add_ref(tex_ptr.cast::<c_void>()) };
    }
    // SAFETY: `texture` is non-null (checked above) and per the D3D9
    // ABI points to a writable `*mut c_void` slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(texture, tex_ptr.cast::<c_void>()) };
    0 // S_OK
}

/// Map a `D3DVERTEXTEXTURESAMPLER0..3` stage index (257..=260) to a slot.
///
/// `SetTexture` / `Set|GetSamplerState` accept these next to the sixteen
/// fragment stages; everything else in that range stays invalid
/// (`D3DDMAPSAMPLER` = 256 is displacement mapping, unimplemented).
pub const fn vertex_sampler_slot(stage: u32) -> Option<usize> {
    match stage {
        257..=260 => Some((stage - 257) as usize),
        _ => None,
    }
}

/// Warn once per entry point that a texture or sampler call named a stage no sampler has.
///
/// Such a call is accepted and ignored, and a getter reads zero or null: a
/// stage past the sixteen fragment samplers and outside the four vertex ones.
/// `D3DDMAPSAMPLER` is one of them, as in Wine: no displacement-map sampler
/// exists here.
#[cold]
#[inline(never)]
fn warn_invalid_sampler(call: &str, stage: u32) {
    mtld3d_shared::log_once_warn_by!(
        target: LOG_TARGET,
        key: caller_log_key(call),
        "{call}({stage}): no sampler has that stage → ignored, D3D_OK"
    );
}

/// Clamp a texture-stage-state stage and type into the stored table, as D3D9 runtimes do.
///
/// [`mtld3d_core::ff_state::clamp_texture_stage_state`] owns the rule; an
/// index outside the table is logged once per entry point.
fn clamp_texture_stage_state(call: &str, stage: u32, type_: u32) -> (u32, u32) {
    if mtld3d_core::ff_state::texture_stage_state_in_table(stage, type_) {
        return (stage, type_);
    }
    clamp_texture_stage_state_out_of_table(call, stage, type_)
}

/// The out-of-table half of [`clamp_texture_stage_state`]: log once and clamp.
///
/// Out of line so the texture-stage-state calls do not build the log
/// arguments on their hot path.
#[cold]
#[inline(never)]
fn clamp_texture_stage_state_out_of_table(call: &str, stage: u32, type_: u32) -> (u32, u32) {
    let (to_stage, to_type) = mtld3d_core::ff_state::clamp_texture_stage_state(stage, type_);
    mtld3d_shared::log_once_warn_by!(
        target: LOG_TARGET,
        key: caller_log_key(call),
        "{call}(stage {stage}, type {type_}): outside the table → clamped to stage {to_stage}, \
         type {to_type}"
    );
    (to_stage, to_type)
}

/// The `INVALIDCALL` a sampler-state call answers for a type past `D3DSAMP_DMAPOFFSET`.
#[cold]
#[inline(never)]
fn reject_sampler_state_type(call: &str, type_: u32) -> i32 {
    mtld3d_shared::log_once_warn_by!(
        target: LOG_TARGET,
        key: caller_log_key(call),
        "{call}: no sampler state has type {type_} → INVALIDCALL"
    );
    D3DERR_INVALIDCALL
}

extern "system" fn device_set_texture(this: *mut c_void, stage: u32, texture: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Texture);
    let vertex_slot = vertex_sampler_slot(stage);
    if vertex_slot.is_none() && stage as usize >= STAGE_COUNT {
        warn_invalid_sampler("SetTexture", stage);
        return D3D_OK;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();

    let new_tex = texture.cast::<Direct3DTexture9>();

    if let Some(rec) = dev.recording_state_block_mut() {
        // SAFETY: `new_tex` is null or a *mut Direct3DTexture9 supplied by
        // the calling game via SetTexture, which the reference taken here
        // keeps alive for the lifetime of the recording.
        let tex = unsafe { CachedComPtr::adopt(new_tex) };
        rec.record(StateOp::Texture { stage, tex });
        return D3D_OK;
    }

    if let Some(slot) = vertex_slot {
        dev.set_vertex_texture_slot(slot, new_tex);
        return D3D_OK;
    }
    if rebind_is_redundant(dev, stage as usize, new_tex) {
        dev.perf_mut().record_keys_gate(KeysGate::SetTexture, true);
        return D3D_OK;
    }
    let delta = dev
        .stage_bindings_mut()
        .replace_texture(stage as usize, new_tex);
    // STAGES on every swap: the encoder binds the new handle and
    // `snapshot_stage_bindings` re-runs flush_dirty_mips/rehydrate and
    // refreshes `cached_bound_texture_mask`. The FF VS/PS keys depend
    // only on the 8-bit occupancy mask (stages 0..7); the variant only
    // on `depth_sampler_mask` / `volume_sampler_mask` (any slot's
    // depth-format-ness / 3D-ness). A swap that flips none of those
    // rebuilds byte-identical keys, so gate those pieces on the actual
    // deltas. `ff_aware_mask` still strips VS/PS bits for programmable
    // shaders.
    let mut bits = SnapshotDirty::STAGES;
    if delta.intersects(
        TextureSwapDelta::DEPTH_CHANGED
            | TextureSwapDelta::VOLUME_CHANGED
            | TextureSwapDelta::CUBE_CHANGED
            | TextureSwapDelta::SAMPLING_CHANGED,
    ) {
        bits |= SnapshotDirty::VARIANT;
    }
    let ffkey_rebuilt = (stage as usize) < 8 && delta.contains(TextureSwapDelta::OCCUPANCY_CHANGED);
    if ffkey_rebuilt {
        bits |= SnapshotDirty::VS_SOURCE
            | SnapshotDirty::VS_CONST
            | SnapshotDirty::PS_SOURCE
            | SnapshotDirty::PS_CONST;
    }
    let mask = dev.ff_aware_mask(bits);
    dev.mark_snapshot_dirty(mask);
    dev.clear_core_draw_cache();
    dev.perf_mut()
        .record_keys_gate(KeysGate::SetTexture, !ffkey_rebuilt);
    0 // S_OK
}

/// Whether binding `tex` at pixel `stage` leaves every input of the stage walk as it is.
///
/// True when the stage already holds `tex` and `tex` is null or attached to
/// `dev`. The slot, its reference and the cached kind and Fetch4 masks are
/// functions of the pointer alone, so rewriting them changes nothing. What the
/// walk reads through the texture (its contents, pending uploads, LOD and
/// residency) is marked dirty on the texture's own device by whichever call
/// changes it, which is `dev` once the texture is attached here. A texture
/// attached to another device comes back through a rehydrate on this device,
/// which the stage walk performs for every bound stage, so that rebind keeps
/// its mark.
fn rebind_is_redundant(
    dev: &DeviceInner,
    stage: usize,
    tex: *mut crate::texture::Direct3DTexture9,
) -> bool {
    if dev.stage_bindings().texture(stage) != tex {
        return false;
    }
    // SAFETY: `tex` is the pointer the stage already holds, so it is null or
    // kept alive by the stage's reference.
    unsafe { tex.as_ref() }.is_none_or(|bound| {
        bound.inner().device_inner() == std::ptr::from_ref::<DeviceInner>(dev) as u64
    })
}

extern "system" fn device_get_texture_stage_state(
    this: *mut c_void,
    stage: u32,
    type_: u32,
    value: *mut u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::TexStageState);
    if value.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let (stage, type_) = clamp_texture_stage_state("GetTextureStageState", stage, type_);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // SAFETY: `value` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `u32` slot owned by the caller; alignment is not assumed.
    unsafe {
        OutPtr::write_opt(
            value,
            dev.ff_state()
                .texture_stage_state(stage as usize, type_ as usize),
        );
    }
    0 // S_OK
}

extern "system" fn device_set_texture_stage_state(
    this: *mut c_void,
    stage: u32,
    type_: u32,
    value: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::TexStageState);
    let (stage, type_) = clamp_texture_stage_state("SetTextureStageState", stage, type_);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::TextureStageState {
            stage,
            type_,
            value,
        });
        return D3D_OK;
    }
    let changed = dev
        .ff_state_mut()
        .set_texture_stage_state(stage as usize, type_ as usize, value);
    // Redundant-set elimination: a same-value TSS write leaves every FF
    // VS/PS key byte-identical, so skip the rebuild. Each slot dirties only
    // the piece that reads it. ff_aware strips the VS/PS bits for
    // programmable shaders; the bump uniform is not an FF piece and needs
    // no such filter.
    if changed {
        let mask = match tss_write_feeds(type_) {
            TssWriteFeeds::StageConstant => dev.ff_aware_mask(SnapshotDirty::PS_CONST),
            TssWriteFeeds::BumpEnv => SnapshotDirty::BUMP_ENV,
            TssWriteFeeds::FfPipeline => dev.ff_aware_mask(
                SnapshotDirty::STAGES
                    | SnapshotDirty::VARIANT
                    | SnapshotDirty::VS_SOURCE
                    | SnapshotDirty::VS_CONST
                    | SnapshotDirty::PS_SOURCE
                    | SnapshotDirty::PS_CONST,
            ),
        };
        dev.mark_snapshot_dirty(mask);
    }
    dev.perf_mut()
        .record_keys_gate(KeysGate::SetTextureStageState, !changed);
    0 // S_OK
}

extern "system" fn device_get_sampler_state(
    this: *mut c_void,
    sampler: u32,
    type_: u32,
    value: *mut u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::SamplerState);
    if value.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let vertex_slot = vertex_sampler_slot(sampler);
    if vertex_slot.is_none() && sampler as usize >= STAGE_COUNT {
        warn_invalid_sampler("GetSamplerState", sampler);
        // SAFETY: `value` is non-null (checked above) and per the D3D9 ABI
        // points to a writable `u32` slot owned by the caller; alignment is not assumed.
        unsafe { OutPtr::write_opt(value, 0) };
        return D3D_OK;
    }
    if type_ as usize >= SAMPLER_STATE_COUNT {
        return reject_sampler_state_type("GetSamplerState", type_);
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let read = vertex_slot.map_or_else(
        || {
            dev.stage_bindings()
                .sampler_state(sampler as usize, type_ as usize)
        },
        |slot| dev.vertex_sampler_states[slot][type_ as usize],
    );
    // SAFETY: `value` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `u32` slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(value, read) };
    0 // S_OK
}

extern "system" fn device_set_sampler_state(
    this: *mut c_void,
    sampler: u32,
    type_: u32,
    value: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::SamplerState);
    let vertex_slot = vertex_sampler_slot(sampler);
    if vertex_slot.is_none() && sampler as usize >= STAGE_COUNT {
        warn_invalid_sampler("SetSamplerState", sampler);
        return D3D_OK;
    }
    if type_ as usize >= SAMPLER_STATE_COUNT {
        return reject_sampler_state_type("SetSamplerState", type_);
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::SamplerState {
            sampler,
            type_,
            value,
            fetch4: if type_ == mtld3d_types::D3DSAMP_MIPMAPLODBIAS {
                mtld3d_core::fetch4::command(value)
            } else {
                None
            },
        });
        return D3D_OK;
    }
    if let Some(slot) = vertex_slot {
        dev.set_vertex_sampler_slot_state(slot, type_ as usize, value);
        return D3D_OK;
    }
    let stage = sampler as usize;
    let old_fetch4 = dev.stage_bindings().fetch4().masks();
    if dev
        .stage_bindings_mut()
        .set_sampler_state(stage, type_ as usize, value)
    {
        if old_fetch4 != dev.stage_bindings().fetch4().masks() {
            dev.mark_snapshot_dirty(SnapshotDirty::VARIANT);
        }
        // Sampler state lives inside StageBinding only.
        dev.mark_snapshot_dirty(SnapshotDirty::STAGES);
    } else if !rebind_is_redundant(dev, stage, dev.stage_bindings().texture(stage)) {
        // A write of the stored value changes no input of the stage walk, as
        // a rebind of the bound texture changes none, unless that texture is
        // attached to another device and needs the walk to come back.
        dev.mark_snapshot_dirty(SnapshotDirty::STAGES);
    }
    0 // S_OK
}

/// The `ValidateDevice` answer of the first sampler stage that rejects its filters.
///
/// Walks the sixteen fragment stages and then the four vertex-texture slots,
/// so the stage reported is the lowest-numbered one that fails. `None` means
/// every stage's filter setup runs, which is the only case that reports a
/// pass count.
fn rejected_filter_stage(dev: &DeviceInner) -> Option<i32> {
    let float32_filtering =
        crate::direct3d9::float32_filtering_supported(dev.config().deny_float32_filtering);
    let native_packed16 = crate::direct3d9::native_packed16_supported(dev.config().expand_packed16);
    let filterable = |texture: *mut Direct3DTexture9| {
        if texture.is_null() {
            return None;
        }
        // SAFETY: a non-null slot holds a live `Direct3DTexture9`; the binding
        // owns a reference for as long as the slot does.
        let format = unsafe { (*texture).d3d_format() };
        Some(mtld3d_core::format::supports_usage_query(
            format,
            D3DUSAGE_QUERY_FILTER,
            float32_filtering,
            native_packed16,
        ))
    };
    let fragment = (0..STAGE_COUNT).map(|stage| {
        (
            dev.stage_bindings().sampler_states(stage),
            dev.stage_bindings().texture(stage),
        )
    });
    let vertex = dev
        .vertex_sampler_states
        .iter()
        .zip(dev.vertex_textures.iter())
        .map(|(states, texture)| (*states, texture.raw()));
    for (sampler, (states, texture)) in fragment.chain(vertex).enumerate() {
        match stage_filter_verdict(&states, filterable(texture)) {
            StageFilterVerdict::Valid => {}
            StageFilterVerdict::FilterDisabled => {
                mtld3d_shared::log_once_warn!(
                    target: crate::LOG_TARGET,
                    "IDirect3DDevice9::ValidateDevice: sampler {sampler} disables its mag or min \
                     filter → UNSUPPORTEDTEXTUREFILTER"
                );
                return Some(D3DERR_UNSUPPORTEDTEXTUREFILTER);
            }
            StageFilterVerdict::TextureNotFilterable => {
                mtld3d_shared::log_once_warn!(
                    target: crate::LOG_TARGET,
                    "IDirect3DDevice9::ValidateDevice: sampler {sampler} filters a texture the \
                     device point-samples → E_FAIL"
                );
                return Some(E_FAIL);
            }
        }
    }
    None
}

extern "system" fn device_validate_device(this: *mut c_void, num_passes: *mut u32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // A sampler stage whose filters cannot run is the one thing D3D9 reports
    // here, and the pass count is left untouched when it does.
    if let Some(hr) = rejected_filter_stage(obj.inner()) {
        return hr;
    }
    // Metal validates the rest of the pipeline state at PSO-creation time, and
    // every fixed-function / shader state combination we accept renders in a
    // single pass, so the remaining state is always single-pass valid. Report
    // one pass and succeed. Returning INVALIDCALL would wrongly push games onto
    // a multi-pass / capability-fallback path.
    if !num_passes.is_null() {
        // SAFETY: caller-supplied writable `u32` out-param per the D3D9 ABI; alignment is not
        // assumed.
        unsafe { OutPtr::write_opt(num_passes, 1) };
    }
    mtld3d_shared::log_once_info!(target: crate::LOG_TARGET, "IDirect3DDevice9::ValidateDevice: single-pass valid under Metal → S_OK (1 pass)");
    D3D_OK
}

extern "system" fn device_set_palette_entries(
    this: *mut c_void,
    _palette: u32,
    entries: *const c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if entries.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // D3D9's palette API is non-functional on modern hardware: the setter
    // succeeds and the palette is simply ignored. One validation remains:
    // without D3DPTEXTURECAPS_ALPHAPALETTE — advertised only under
    // debug.capsAll — every PALETTEENTRY's peFlags must be 0xFF (fully
    // opaque); an alpha-bearing entry
    // is INVALIDCALL. The getters stay INVALIDCALL.
    if !obj.inner().config().caps_all {
        // PALETTEENTRY is { peRed, peGreen, peBlue, peFlags } (4 bytes, peFlags
        // last); the array holds 256 entries per the D3D9 ABI = 1024 bytes.
        // SAFETY: `entries` is non-null (checked) and per the D3D9 ABI points to
        // 256 PALETTEENTRYs.
        let bytes = unsafe { core::slice::from_raw_parts(entries.cast::<u8>(), 256 * 4) };
        // peFlags of each entry are bytes 3, 7, 11, … (every 4th, offset 3).
        if bytes.iter().skip(3).step_by(4).any(|&f| f != 0xFF) {
            return D3DERR_INVALIDCALL;
        }
    }
    mtld3d_shared::log_once_info!(target: crate::LOG_TARGET,
        "IDirect3DDevice9::SetPaletteEntries: palette API non-functional on modern hardware, palette ignored → S_OK");
    D3D_OK
}

extern "system" fn device_get_palette_entries(
    this: *mut c_void,
    _palette: u32,
    _entries: *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "stub IDirect3DDevice9::GetPaletteEntries → INVALIDCALL");
    D3DERR_INVALIDCALL
}

extern "system" fn device_set_current_texture_palette(this: *mut c_void, _palette: u32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // Non-functional palette API: the setter succeeds, the palette is ignored.
    // GetCurrentTexturePalette stays INVALIDCALL.
    mtld3d_shared::log_once_info!(target: crate::LOG_TARGET,
        "IDirect3DDevice9::SetCurrentTexturePalette: palette API non-functional on modern hardware, ignored → S_OK");
    D3D_OK
}

extern "system" fn device_get_current_texture_palette(
    this: *mut c_void,
    _palette_number: *mut u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "stub IDirect3DDevice9::GetCurrentTexturePalette → INVALIDCALL");
    D3DERR_INVALIDCALL
}

extern "system" fn device_set_scissor_rect(this: *mut c_void, rect: *const c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::ViewScissor);
    // SAFETY: vtable in-param; `rect` is *const D3DRECT per ABI.
    let Some(r) = (unsafe { ValueIn::<D3DRECT>::read_opt(rect) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::ScissorRect(r));
        return D3D_OK;
    }
    dev.set_scissor_rect(r);
    // scissor_rect is the only piece of RenderStateSnapshot affected.
    dev.mark_snapshot_dirty(SnapshotDirty::RS);
    D3D_OK
}

extern "system" fn device_get_scissor_rect(this: *mut c_void, rect: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::ViewScissor);
    if rect.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let scissor = dev.scissor_rect();
    // SAFETY: `rect` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `RECT` (alias for `D3DRECT`) owned by the
    // caller; alignment is not assumed.
    unsafe {
        OutPtr::write_opt(rect.cast::<D3DRECT>(), scissor);
    }
    D3D_OK
}

extern "system" fn device_set_software_vertex_processing(this: *mut c_void, software: i32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // SW vertex processing exists for old CPUs without HW T&L. Running
    // the same pipeline on HW VP produces the same visible result, so
    // accepting the call and using HW is transparent to the game — it
    // gets the draws it expects, just faster. Modern Windows drivers
    // behave the same way. Return S_OK; log once per distinct arg so
    // both the 0 and 1 cases surface.
    mtld3d_shared::log_once_info_by!(
        target: crate::LOG_TARGET,
        key: u64::from(software.cast_unsigned()),
        "IDirect3DDevice9::SetSoftwareVertexProcessing({software}): obsolete, hardware VP is always used"
    );
    D3D_OK
}

extern "system" fn device_get_software_vertex_processing(this: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    mtld3d_shared::log_once_info!(
        target: crate::LOG_TARGET,
        "IDirect3DDevice9::GetSoftwareVertexProcessing: obsolete, returning 0 (hardware VP)"
    );
    0
}

extern "system" fn device_set_npatch_mode(this: *mut c_void, segments: f32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // SetNPatchMode(0.0) and SetNPatchMode(1.0) both mean "N-patch
    // tessellation disabled" — i.e. default behavior. Games clear this
    // on startup without the intent to subdivide, so an INVALIDCALL was
    // wrong. Silently accept the disable. Warn once only for a real
    // subdivision request (> 1.0), which we can't honor.
    if segments <= 1.0 {
        return D3D_OK;
    }
    mtld3d_shared::log_once_warn!(
        target: crate::LOG_TARGET,
        "stub IDirect3DDevice9::SetNPatchMode({segments}) — N-patch tessellation not implemented, ignoring"
    );
    D3D_OK
}

extern "system" fn device_get_npatch_mode(this: *mut c_void) -> f32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // N-patches are obsolete fixed-function tessellation; every modern
    // driver returns 0.0 (disabled). No port candidate.
    mtld3d_shared::log_once_info!(
        target: crate::LOG_TARGET,
        "IDirect3DDevice9::GetNPatchMode: obsolete, returning 0.0"
    );
    0.0
}

/// Whether the device has a usable vertex-layout source for a draw.
///
/// A `Draw*` call needs either an explicitly bound vertex declaration or a
/// non-zero FVF; with neither, the runtime has no way to interpret the
/// vertex stream and the draw is invalid. A non-zero FVF binds its implicit
/// declaration, so a bound declaration is the whole test.
const fn has_vertex_layout_source(dev: &DeviceInner) -> bool {
    !dev.vertex_decl().is_null()
}

/// The `log_once_warn_by!` key of a log line shared by several entry points.
///
/// Keyed by the entry point's name, so each caller of a shared cold helper
/// logs its own first occurrence instead of the first caller hiding the rest.
fn caller_log_key(call: &str) -> u64 {
    use core::hash::BuildHasher as _;
    rustc_hash::FxBuildHasher.hash_one(call)
}

/// The `INVALIDCALL` every `Draw*` answers with no vertex declaration and no FVF bound.
#[cold]
#[inline(never)]
fn reject_missing_vertex_layout(call: &str) -> i32 {
    mtld3d_shared::log_once_warn_by!(
        target: LOG_TARGET,
        key: caller_log_key(call),
        "{call} with neither a vertex declaration nor an FVF bound → INVALIDCALL"
    );
    D3DERR_INVALIDCALL
}

/// The `INVALIDCALL` a triangle fan answers when its rewrite to a list cannot address it.
#[cold]
#[inline(never)]
fn reject_unaddressable_fan(call: &str, primitive_count: u32) -> i32 {
    mtld3d_shared::log_once_warn_by!(
        target: LOG_TARGET,
        key: caller_log_key(call),
        "{call}: a {primitive_count}-triangle fan has an index past u32 → INVALIDCALL"
    );
    D3DERR_INVALIDCALL
}

/// The `INVALIDCALL` an inline draw answers for a zero vertex stride.
#[cold]
#[inline(never)]
fn reject_zero_up_stride(call: &str) -> i32 {
    mtld3d_shared::log_once_warn_by!(
        target: LOG_TARGET,
        key: caller_log_key(call),
        "{call} with a zero vertex stride → INVALIDCALL"
    );
    D3DERR_INVALIDCALL
}

extern "system" fn device_draw_primitive(
    this: *mut c_void,
    primitive_type: u32,
    start_vertex: u32,
    primitive_count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Draws);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        std::hint::cold_path();
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if !has_vertex_layout_source(dev) {
        return reject_missing_vertex_layout("DrawPrimitive");
    }
    // A draw of no primitives is valid and draws nothing.
    if primitive_count == 0 {
        std::hint::cold_path();
        return D3D_OK;
    }
    core_draw::draw_primitive(dev, primitive_type, start_vertex, primitive_count)
}

extern "system" fn device_draw_indexed_primitive(
    this: *mut c_void,
    primitive_type: u32,
    base_vertex_index: i32,
    _min_vertex_index: u32,
    _num_vertices: u32,
    start_index: u32,
    primitive_count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Draws);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        std::hint::cold_path();
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if !has_vertex_layout_source(dev) {
        return reject_missing_vertex_layout("DrawIndexedPrimitive");
    }
    // A draw of no primitives is valid and draws nothing.
    if primitive_count == 0 {
        std::hint::cold_path();
        return D3D_OK;
    }
    core_draw::draw_indexed_primitive(
        dev,
        primitive_type,
        base_vertex_index,
        start_index,
        primitive_count,
    )
}

/// The vertex streams a draw reads: the bound declaration's, or stream 0 alone with none bound.
fn declared_stream_mask(dev: &DeviceInner) -> u16 {
    let decl_ptr = dev.vertex_decl();
    if decl_ptr.is_null() {
        1
    } else {
        // SAFETY: non-null; the device slot's refcount keeps the declaration
        // alive while bound.
        unsafe { &*decl_ptr }.inner().stream_mask()
    }
}

extern "system" fn device_draw_primitive_up(
    this: *mut c_void,
    primitive_type: u32,
    primitive_count: u32,
    vertex_data: *const c_void,
    vertex_stride: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Draws);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    if vertex_stride == 0 {
        return reject_zero_up_stride("DrawPrimitiveUP");
    }
    let dev = obj.inner();
    if !has_vertex_layout_source(dev) {
        return reject_missing_vertex_layout("DrawPrimitiveUP");
    }
    // A draw of no primitives is valid and draws nothing.
    if primitive_count == 0 {
        return D3D_OK;
    }
    let hr = core_draw::draw_primitive_up(
        dev,
        primitive_type,
        primitive_count,
        vertex_data,
        vertex_stride,
    );
    // D3D9 resets stream source 0 to (NULL, 0, 0) after DrawPrimitiveUP.
    if hr == D3D_OK
        || matches!(
            hr,
            mtld3d_types::D3DERR_DEVICELOST | mtld3d_types::E_OUTOFMEMORY
        )
    {
        dev.bound_buffers_mut().reset_stream0();
    }
    hr
}

/// Invalidates the core draw cache when vertex shader constants change.
pub const fn propagate_vs_const_delta(
    dev: &mut DeviceInner,
    _start_register: u32,
    _slice: &[[f32; 4]],
) {
    dev.invalidate_core_draw();
}

/// Invalidates the core draw cache when pixel shader constants change.
pub const fn propagate_ps_const_delta(
    dev: &mut DeviceInner,
    _start_register: u32,
    _slice: &[[f32; 4]],
) {
    dev.invalidate_core_draw();
}

extern "system" fn device_draw_indexed_primitive_up(
    this: *mut c_void,
    primitive_type: u32,
    min_vertex_index: u32,
    num_vertices: u32,
    primitive_count: u32,
    index_data: *const c_void,
    index_format: u32,
    vertex_data: *const c_void,
    vertex_stride: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Draws);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    if vertex_stride == 0 {
        return reject_zero_up_stride("DrawIndexedPrimitiveUP");
    }
    let dev = obj.inner();
    if !has_vertex_layout_source(dev) {
        return reject_missing_vertex_layout("DrawIndexedPrimitiveUP");
    }
    // A draw of no primitives is valid and draws nothing.
    if primitive_count == 0 {
        return D3D_OK;
    }
    if index_data.is_null() || vertex_data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if convert::index_format(index_format).is_none() {
        mtld3d_shared::log_once_warn_by!(
            target: LOG_TARGET,
            key: u64::from(index_format),
            "DrawIndexedPrimitiveUP: unsupported index format {index_format}"
        );
        return D3DERR_INVALIDCALL;
    }
    let hr = core_draw::draw_indexed_primitive_up(
        dev,
        &core_draw::IndexedUp {
            primitive: primitive_type,
            min_vertex_index,
            num_vertices,
            primitive_count,
            index_data,
            index_format,
            vertex_data,
            stride: vertex_stride,
        },
    );
    // D3D9 resets stream source 0 to (NULL, 0, 0) and the index buffer to NULL after a
    // DrawIndexedPrimitiveUP.
    if hr == D3D_OK
        || matches!(
            hr,
            mtld3d_types::D3DERR_DEVICELOST | mtld3d_types::E_OUTOFMEMORY
        )
    {
        let bound = dev.bound_buffers_mut();
        bound.reset_stream0();
        bound.replace_index_buffer(core::ptr::null_mut());
    }
    hr
}

extern "system" fn device_process_vertices(
    this: *mut c_void,
    src_start: u32,
    dst_index: u32,
    count: u32,
    dst_buffer: *mut c_void,
    decl: *mut c_void,
    _flags: u32,
) -> i32 {
    use crate::vertex_buffer::Direct3DVertexBuffer9;

    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Draws);
    if dst_buffer.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // A vertex declaration source (as opposed to the current FVF) is not
    // implemented; software vertex processing is a conformance-only path.
    if !decl.is_null() {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "ProcessVertices with an explicit vertex declaration is unimplemented → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();

    // The source is stream 0 read through the device's current FVF.
    let src_vb = dev.bound_buffers().stream_vertex_buffer(0);
    if src_vb.is_null() {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "ProcessVertices with no stream-0 vertex buffer bound → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    let src_fvf = dev.fvf();
    let src_stream_offset = dev.bound_buffers().stream_offset(0);
    let src_stride = dev.bound_buffers().stream_stride(0);
    let wvp = dev.ff_state().world_view_projection();
    let viewport = dev.viewport();

    // SAFETY: `src_vb` is a live bound `Direct3DVertexBuffer9` (non-null
    // checked); the bound-slot reference keeps it alive for this call.
    let src_inner = unsafe { (*src_vb).inner() };
    let first = || (src_stream_offset + src_start.saturating_mul(src_stride)) as usize;
    // A buffer a core device maps directly is read where the CPU wrote it.
    let mapped = match core_arms::process_vertices_source(dev, src_inner, first) {
        Ok(mapped) => mapped,
        Err(hr) => return hr,
    };
    let src_bytes = if let Some(bytes) = &mapped {
        bytes.as_slice()
    } else {
        if src_inner.backing_is_released() {
            // A default-pool D3DUSAGE_WRITEONLY buffer keeps its bytes on the GPU
            // alone. D3DUSAGE_SOFTWAREPROCESSING is the usage a title declares for
            // a buffer it feeds to ProcessVertices, and that declaration keeps the
            // CPU copy, so a source without it has nothing left to read.
            mtld3d_shared::log_once_warn!(
                target: crate::LOG_TARGET,
                "ProcessVertices from a D3DUSAGE_WRITEONLY source without D3DUSAGE_SOFTWAREPROCESSING: no CPU copy to read → INVALIDCALL"
            );
            return D3DERR_INVALIDCALL;
        }
        let backing = src_inner.backing();
        let first = first();
        if first > backing.len() {
            return D3DERR_INVALIDCALL;
        }
        &backing[first..]
    };

    // SAFETY: `dst_buffer` is a live `Direct3DVertexBuffer9*` per the D3D9 ABI;
    // it is distinct from `src_vb` (the source is a bound stream, the
    // destination a caller-owned buffer) so the two borrows do not alias.
    let dst_obj = unsafe { &mut *dst_buffer.cast::<Direct3DVertexBuffer9>() };
    let dst_fvf = dst_obj.inner().fvf();

    let processed = mtld3d_core::process_vertices::process_vertices(
        &mtld3d_core::process_vertices::ProcessVerticesRequest {
            src: src_bytes,
            src_stride,
            src_fvf,
            dst_fvf,
            count,
            wvp,
            viewport,
        },
    );
    let Some(processed) = processed else {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "ProcessVertices: destination FVF {dst_fvf:#x} has no transformed position or the \
             source is too short → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    };
    if mtld3d_core::process_vertices::dst_wants_shaded_output(dst_fvf)
        && dev.render_state(D3DRS_LIGHTING as usize) != 0
    {
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "ProcessVertices does not run software lighting; the destination colour is the \
             source colour passed through"
        );
    }
    let (_, dst_stride) = mtld3d_core::convert::fvf_to_elements(dst_fvf);
    let dst_offset = (dst_index.saturating_mul(dst_stride)) as usize;
    if let Err(hr) = dev.encoder_status() {
        return hr;
    }
    dst_obj
        .inner_mut()
        .core_write_processed(dst_offset, &processed, dev.core_mut());
    dev.encoder_status().map_or_else(|hr| hr, |()| D3D_OK)
}

extern "system" fn device_create_vertex_declaration(
    this: *mut c_void,
    elements: *const c_void,
    decl: *mut *mut c_void,
) -> i32 {
    const MAX_ELEMENTS: usize = 64; // generous; D3D9 limit is MAXD3DDECLLENGTH=64

    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    if elements.is_null() || decl.is_null() {
        null_out(decl);
        return D3DERR_INVALIDCALL;
    }
    // Read the element array up to D3DDECL_END (stream==0xFF). We don't
    // know the length up front, so walk in place until the terminator.
    let mut len = 0usize;
    loop {
        if len >= MAX_ELEMENTS {
            warn!(target: LOG_TARGET, "CreateVertexDeclaration: no terminator within {MAX_ELEMENTS} elements");
            null_out(decl);
            return D3DERR_INVALIDCALL;
        }
        // SAFETY: `elements + len * size_of::<D3DVERTEXELEMENT9>()` stays within
        // the caller-provided array; the `MAX_ELEMENTS` bound above guards `len`.
        let e_ptr = unsafe { elements.cast::<mtld3d_types::D3DVERTEXELEMENT9>().add(len) };
        // SAFETY: `e_ptr` addresses a readable `D3DVERTEXELEMENT9` of the caller's
        // array; alignment is not assumed.
        let e = unsafe { e_ptr.read_unaligned() };
        len += 1;
        if e.stream == mtld3d_types::D3DDECL_END_STREAM {
            break;
        }
        // Real-element validation. D3D9 rejects these with E_FAIL (distinct
        // from the INVALIDCALL used for structural problems): element offsets
        // must be DWORD-aligned, and D3DDECLTYPE_UNUSED is only legal in the
        // D3DDECL_END terminator.
        if e.offset % 4 != 0 || e.type_ == mtld3d_types::D3DDECLTYPE_UNUSED {
            null_out(decl);
            return E_FAIL;
        }
    }
    // SAFETY: `elements` is the caller-supplied decl array; the walk
    // above advanced `len` exactly to the `D3DDECL_END` terminator, so
    // `len` `D3DVERTEXELEMENT9` entries are readable.
    let slice = unsafe {
        mtld3d_shared::slice_from_caller(elements.cast::<mtld3d_types::D3DVERTEXELEMENT9>(), len)
    };
    // Trace-only probe — surface the decl shape under
    // `RUST_LOG=mtld3d::d3d9::state=trace` for bring-up enumeration. Not a
    // warn: FF VS consumes BLENDWEIGHT/BLENDINDICES correctly when
    // D3DRS_VERTEXBLEND opts in. WoW (and any game doing CPU skinning) just
    // leaves D3DRS_VERTEXBLEND at D3DVBF_DISABLE, in which case these decl
    // elements are declared but unused — neither a bug nor a warn-worthy
    // event.
    if mtld3d_core::state_trace::enabled() {
        for e in slice.iter().take(len.saturating_sub(1)) {
            if e.usage == mtld3d_types::D3DDECLUSAGE_BLENDWEIGHT
                || e.usage == mtld3d_types::D3DDECLUSAGE_BLENDINDICES
            {
                let usage = e.usage;
                let ty = e.type_;
                let stream = e.stream;
                let offset = e.offset;
                log::trace!(
                    target: mtld3d_core::state_trace::TARGET,
                    "vertex decl declares D3DDECLUSAGE_{usage} (type={ty}, stream={stream}, offset={offset})"
                );
            }
        }
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    Direct3DVertexDeclaration9::new(&VertexDeclCreateInfo {
        device_inner: obj.inner_ptr(),
        elements: &slice,
        fvf: 0,
    })
    .map_or_else(
        || {
            null_out(decl);
            D3DERR_INVALIDCALL
        },
        |obj| {
            // SAFETY: vtable out-param; `decl` is *mut *mut c_void per IDirect3DDevice9 ABI.
            let decl_ptr = Box::into_raw(Box::new(obj));
            // SAFETY: `decl_ptr` is a freshly created, live declaration at refcount 1.
            unsafe { crate::com_ref::com_register_child(decl_ptr) };
            // SAFETY: vtable out-param; `decl` is *mut *mut c_void per the ABI.
            unsafe { OutPtr::write_opt(decl, decl_ptr.cast::<c_void>()) };
            D3D_OK
        },
    )
}

extern "system" fn device_set_vertex_declaration(this: *mut c_void, decl: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Shader);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let new = decl.cast::<Direct3DVertexDeclaration9>();
    if let Some(rec) = dev.recording_state_block_mut() {
        // SAFETY: `new` is null or a *mut Direct3DVertexDeclaration9 supplied
        // by the calling game via SetVertexDeclaration.
        let adopted = unsafe { CachedComPtr::adopt(new) };
        rec.record(StateOp::VertexDeclaration(adopted));
        return D3D_OK;
    }
    // Redundant-set elimination: re-binding the same decl pointer
    // re-resolves to a byte-identical attrs slice (+ FfVsLayout), so
    // skip the expensive VDECL rebuild. VS_SOURCE/VS_CONST only matter
    // if FF VS bound (FF VS key reads ff_vs_layout).
    // The FVF is the declaration's (`DeviceInner::fvf`): the one `SetFVF`
    // built for an FVF keeps reporting that FVF when it is bound here, and a
    // declaration the game created reports 0, so the pointer decides both.
    let changed = dev.replace_vertex_decl(new);
    if changed {
        // VS_SOURCE is marked unconditionally (not via `ff_aware_mask`, which
        // drops it for a programmable VS): a declaration change alters which VS
        // input registers are provided (`provided_input_mask`), so the
        // programmable `VsSource` must rebuild to pick up the new mask.
        let mut mask = SnapshotDirty::VDECL
            | SnapshotDirty::VS_SOURCE
            | dev.ff_aware_mask(SnapshotDirty::VS_CONST);
        // Pre-transformed (POSITIONT) declarations bypass a bound VS, and
        // `cached_ff_vs_layout` (which `ff_aware_mask` consults) lags until
        // the next snapshot — when RHW-ness may flip, dirty the FF VS consts
        // (xyzrhw viewport row) and the variant (`fog_mode` keys on RHW)
        // unconditionally.
        let new_rhw = !new.is_null()
            // SAFETY: non-null checked; the slot adopted a ref above.
            && convert::vertex_decl_has_rhw(unsafe { (*new).inner().elements() });
        if new_rhw || dev.cached_ff_vs_layout.has_rhw() {
            mask |= SnapshotDirty::VS_CONST | SnapshotDirty::VARIANT;
        }
        dev.mark_snapshot_dirty(mask);
    }
    dev.perf_mut()
        .record_keys_gate(KeysGate::SetVertexDecl, !changed);
    D3D_OK
}

extern "system" fn device_get_vertex_declaration(this: *mut c_void, decl: *mut *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Shader);
    if decl.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let current = obj.inner().vertex_decl();
    if !current.is_null() {
        // SAFETY: `current` is non-null (checked above) and points to a
        // live `Direct3DVertexDeclaration9` whose refcount keeps it
        // alive while bound on the device.
        let wrapper = unsafe { &*current };
        let vtbl = wrapper.vtbl();
        // SAFETY: calling the just-loaded `add_ref` thunk through `vtbl`
        // with the wrapper pointer as `this`; D3D9 mandates AddRef on
        // out-pointer returns from getter thunks.
        unsafe { (vtbl.add_ref)(current.cast::<c_void>()) };
    }
    // SAFETY: `decl` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `*mut c_void` slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(decl, current.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_set_fvf(this: *mut c_void, fvf: u32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Shader);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if dev.is_state_block_recording() {
        // A recorded SetFVF is a change of declaration, to the one built for
        // the FVF, so a `Capture` of the block refreshes it from the bound
        // declaration like a recorded SetVertexDeclaration. SetFVF(0) changes
        // nothing and records nothing.
        if fvf != 0 {
            let decl = dev.get_or_create_fvf_decl(fvf);
            if decl.is_null() {
                mtld3d_shared::log_once_warn!(
                    target: LOG_TARGET,
                    "recorded SetFVF({fvf:#x}): no declaration for the FVF → DRIVERINTERNALERROR, nothing recorded"
                );
                return mtld3d_types::D3DERR_DRIVERINTERNALERROR;
            }
            if let Some(rec) = dev.recording_state_block_mut() {
                // SAFETY: `decl` is a live declaration the FVF cache holds
                // until the device drops.
                let adopted = unsafe { CachedComPtr::adopt(decl) };
                rec.record(StateOp::VertexDeclaration(adopted));
            }
        }
        return D3D_OK;
    }
    // SetFVF(0) is not a valid FVF; the driver treats it as a no-op, leaving
    // the current declaration (explicit, or a prior implicit one) bound.
    if fvf == 0 {
        dev.perf_mut().record_keys_gate(KeysGate::SetFvf, true);
        return 0;
    }
    // A non-zero FVF binds its implicit declaration so GetVertexDeclaration
    // returns it and the draw path resolves the same layout it would from the
    // FVF directly. Redundant-set elimination: re-binding the same cached decl
    // changes nothing, so skip the VDECL rebuild; `bind_fvf_decl` answers
    // that without the cache lookup when the FVF and its declaration are
    // already bound. An FVF whose declaration cannot be built leaves the
    // bound declaration as it is, as in Wine.
    let Some(changed) = dev.bind_fvf_decl(fvf) else {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "SetFVF({fvf:#x}): no declaration for the FVF → DRIVERINTERNALERROR, binding kept"
        );
        return mtld3d_types::D3DERR_DRIVERINTERNALERROR;
    };
    if changed {
        // VS_SOURCE is marked unconditionally (not via `ff_aware_mask`, which
        // drops it for a programmable VS): a declaration change alters which VS
        // input registers are provided (`provided_input_mask`), so the
        // programmable `VsSource` must rebuild to pick up the new mask.
        let mut mask = SnapshotDirty::VDECL
            | SnapshotDirty::VS_SOURCE
            | dev.ff_aware_mask(SnapshotDirty::VS_CONST);
        // XYZRHW bypasses a bound VS, and `cached_ff_vs_layout` (which
        // `ff_aware_mask` consults) lags until the next snapshot — when
        // RHW-ness may flip, dirty the FF VS consts (xyzrhw viewport row)
        // and the variant (`fog_mode` keys on RHW) unconditionally.
        let new_rhw = (fvf & mtld3d_types::D3DFVF_POSITION_MASK) == mtld3d_types::D3DFVF_XYZRHW;
        if new_rhw || dev.cached_ff_vs_layout.has_rhw() {
            mask |= SnapshotDirty::VS_CONST | SnapshotDirty::VARIANT;
        }
        dev.mark_snapshot_dirty(mask);
    }
    dev.perf_mut().record_keys_gate(KeysGate::SetFvf, !changed);
    0 // S_OK
}

extern "system" fn device_get_fvf(this: *mut c_void, fvf: *mut u32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Shader);
    if fvf.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: `fvf` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `u32` slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(fvf, obj.inner().fvf()) };
    0 // S_OK
}

/// Read a DXSO token stream from a caller-supplied pointer until the End opcode (0x0000FFFF).
///
/// Skips comment payloads and each instruction's operand run, so a payload or
/// immediate word that happens to equal 0xFFFF is not misread as End. The
/// operand run is counted the way the model requires, which for SM1 is not a
/// field but a scan: see [`operand_token_count`]. Getting that wrong here does
/// not merely mis-measure, it walks past the End token and keeps reading
/// whatever follows the shader in the caller's address space.
///
/// The version word is validated rather than skipped: the walk cannot pick its
/// counting rule without the model, and a stream whose first token is not a
/// VS/PS version word has no length to find.
fn read_shader_bytecode(ptr: *const u32) -> Option<Vec<u32>> {
    const MAX_TOKENS: usize = 65536;
    // SAFETY: both callers reject a null `pFunction` before reaching here, and
    // a non-null token stream is at least its version word; alignment is not assumed.
    let version = unsafe { ptr.read_unaligned() };
    match (version >> 16) & 0xFFFF {
        0xFFFE | 0xFFFF => {}
        _ => {
            mtld3d_shared::log_once_warn!(
                target: LOG_TARGET,
                "shader creation: first token 0x{version:08X} is not a vertex or pixel shader version word → INVALIDCALL"
            );
            return None;
        }
    }
    let major = ((version >> 8) & 0xFF) as u8;
    let mut len = 1; // version token
    loop {
        if len >= MAX_TOKENS {
            return None;
        }
        // SAFETY: `ptr + len` stays within the caller-provided token stream;
        // the `MAX_TOKENS` bound above guards `len`.
        let tok_ptr = unsafe { ptr.add(len) };
        // SAFETY: `tok_ptr` addresses a readable `u32` of the token stream;
        // alignment is not assumed.
        let tok = unsafe { tok_ptr.read_unaligned() };
        len += 1;
        let opcode = (tok & 0xFFFF) as u16;
        if opcode == 0xFFFF {
            break;
        }
        if opcode == 0xFFFE {
            let payload = ((tok >> 16) & 0x7FFF) as usize;
            len += payload;
        } else {
            len += operand_token_count(major, tok, |n| {
                if len + n >= MAX_TOKENS {
                    return None;
                }
                // SAFETY: same stream as the read above, under the same
                // `MAX_TOKENS` bound.
                let peek_ptr = unsafe { ptr.add(len + n) };
                // SAFETY: the bound above keeps `len + n` inside the same
                // stream the loop is already reading; alignment is not assumed.
                Some(unsafe { peek_ptr.read_unaligned() })
            });
        }
    }
    // SAFETY: `ptr` is the caller-supplied DXSO token stream; the
    // walk above advanced `len` exactly to the End opcode position,
    // so `len` u32 tokens are readable from `ptr`; alignment is not assumed.
    let bc = unsafe { mtld3d_shared::slice_from_caller(ptr, len) };
    Some(bc.into_owned())
}

/// Optional on-disk dump of DXSO bytecode.
///
/// Gated by the `debug.bytecodeDumpDir = <dir>` key in `mtld3d.conf`. Writes
/// `{dir}/{prefix}_{id:x}.dxso` as raw little-endian `u32` tokens (no
/// framing) the first time a shader with that id is seen; subsequent
/// calls with the same id are no-ops. Failures log once and don't
/// abort the caller — this is a forensic shader-capture probe, not a
/// correctness path.
fn maybe_dump_bytecode(dir: &str, prefix: &str, shader_id: ProgramId, bytecode: &[u32]) {
    if dir.is_empty() {
        return;
    }
    let path = std::path::PathBuf::from(dir).join(format!("{prefix}_{shader_id:x}.dxso"));
    if path.exists() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(dir) {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "debug.bytecodeDumpDir: create_dir_all({dir}) failed: {e}"
        );
        return;
    }
    let mut bytes = Vec::with_capacity(bytecode.len() * 4);
    for &t in bytecode {
        bytes.extend_from_slice(&t.to_le_bytes());
    }
    if let Err(e) = std::fs::write(&path, &bytes) {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "debug.bytecodeDumpDir: write({}) failed: {e}",
            path.display()
        );
    }
}

extern "system" fn device_create_vertex_shader(
    this: *mut c_void,
    function: *const u32,
    shader: *mut *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    null_out(shader);
    if function.is_null() || shader.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let Some(bytecode) = read_shader_bytecode(function) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; this is a live Direct3DDevice9 per the D3D9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if !dev.config().bytecode_dump_dir.is_empty() {
        maybe_dump_bytecode(
            &dev.config().bytecode_dump_dir,
            "vs",
            ProgramId::from_tokens(&bytecode),
            &bytecode,
        );
    }
    let (parsed, meta, code) = match core_draw::create_core_shader(
        dev,
        &bytecode,
        &mtld3d_shared::shader_create::ShaderStage::Vertex,
    ) {
        Ok(created) => created,
        Err(result) => return result,
    };
    let mut const_usage = crate::vertex_shader::VsConstUsage::empty();
    const_usage.set(
        crate::vertex_shader::VsConstUsage::USES_REL_CONST,
        parsed
            .usage
            .contains(mtld3d_shared::shader_create::ShaderUsage::RELATIVE_CONST),
    );
    const_usage.set(
        crate::vertex_shader::VsConstUsage::USES_INT_CONST,
        parsed
            .usage
            .contains(mtld3d_shared::shader_create::ShaderUsage::INT_CONST),
    );
    const_usage.set(
        crate::vertex_shader::VsConstUsage::USES_BOOL_CONST,
        parsed
            .usage
            .contains(mtld3d_shared::shader_create::ShaderUsage::BOOL_CONST),
    );
    let tokens = code.shared_tokens();
    let shader_obj = Direct3DVertexShader9::new(
        obj.inner_ptr(),
        parsed.max_const_used,
        const_usage,
        tokens,
        (code, meta),
    );
    let shader_ptr = Box::into_raw(Box::new(shader_obj));
    // SAFETY: shader_ptr is a freshly created live shader at refcount one.
    unsafe { crate::com_ref::com_register_child(shader_ptr) };
    // SAFETY: the D3D9 ABI supplies a writable shader out pointer, checked non-null above.
    unsafe { OutPtr::write_opt(shader, shader_ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_set_vertex_shader(this: *mut c_void, shader: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Shader);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let new = shader.cast::<Direct3DVertexShader9>();
    if let Some(rec) = dev.recording_state_block_mut() {
        // SAFETY: `new` is null or a *mut Direct3DVertexShader9 supplied by
        // the calling game via SetVertexShader.
        let adopted = unsafe { CachedComPtr::adopt(new) };
        rec.record(StateOp::VertexShader(adopted));
        return D3D_OK;
    }
    // Redundant-set elimination: re-binding the same VS pointer leaves
    // attr-resolution, shader id, uses_rel_const and max_const_used all
    // identical, so skip the rebuild. VDECL: attr-resolution branch
    // depends on whether prog VS is bound. VS_SOURCE/VS_CONST: shader
    // identity + uses_rel_const + max_const_used change.
    let changed = dev.shader_bindings_mut().replace_vertex_shader(new);
    if changed {
        dev.mark_snapshot_dirty(
            SnapshotDirty::VDECL | SnapshotDirty::VS_SOURCE | SnapshotDirty::VS_CONST,
        );
    }
    dev.perf_mut()
        .record_keys_gate(KeysGate::SetVertexShader, !changed);
    0
}

extern "system" fn device_get_vertex_shader(this: *mut c_void, shader: *mut *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Shader);
    if shader.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let vs = dev.shader_bindings().vertex_shader();
    if !vs.is_null() {
        // SAFETY: `vs` is non-null (checked) and points to a live
        // Direct3DVertexShader9 kept alive by the device binding.
        let add_ref = unsafe { (*vs).vtbl().add_ref };
        // SAFETY: calling the just-loaded `add_ref` thunk; D3D9 mandates
        // AddRef on out-pointer returns (the caller Releases the result).
        unsafe { add_ref(vs.cast::<c_void>()) };
    }
    // SAFETY: `shader` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `*mut c_void` slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(shader, vs.cast::<c_void>()) };
    0
}

extern "system" fn device_set_vertex_shader_constant_f(
    this: *mut c_void,
    start_register: u32,
    constant_data: *const f32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    if constant_data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if !window_in_range(start_register, count, CONSTANT_ROWS) {
        return reject_constant_window(
            "SetVertexShaderConstantF",
            start_register,
            count,
            CONSTANT_ROWS,
        );
    }
    // A zero count names an empty window, which D3D9 accepts and writes nothing into.
    if count == 0 {
        return D3D_OK;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // SAFETY: `constant_data` is non-null and `count` non-zero (both
    // settled above); per the D3D9 ABI the caller guarantees the bounded floats
    // are initialized and readable without mutation in one allocation for
    // this call. The byte size fits `isize::MAX`; all `f32` bits are valid.
    let constants = unsafe {
        mtld3d_shared::slice_from_caller(constant_data.cast::<[f32; 4]>(), count as usize)
    };
    let slice: &[[f32; 4]] = &constants;
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::VertexShaderConstantF {
            start: start_register,
            values: slice.to_vec(),
        });
        return D3D_OK;
    }
    // Redundant-set elimination: a write that leaves every mirror row
    // unchanged yields a byte-identical encoder delta, so skip the delta
    // push when nothing changed. WoW re-uploads identical
    // constant rows frequently; this is the ShaderConst analogue of the
    // RenderState / VDECL gates.
    let changed = dev
        .shader_bindings_mut()
        .write_vs_constants(start_register, slice);
    if changed {
        // Propagate the new rows to the encoder-side mirror via a delta op.
        // The encoder applies it before the next `Op::Draw` sees programmable
        // VS const state, so emit_snapshot_deltas does not need to bump
        // `vs_constants` into the API-thread arena (the encoder snapshots
        // from its own mirror at emit_draw time).
        // The delta invalidates the encoder's constant scratch cache directly.
        // Programmable float registers do not change the captured snapshot.
        propagate_vs_const_delta(dev, start_register, slice);
    }
    dev.perf_mut()
        .record_keys_gate(KeysGate::SetVsConst, !changed);
    0
}

/// Copy `mirror[start..]` into the caller's `count` values at `out`, clamped to the mirror.
///
/// Shared by every `Get*ShaderConstant*` thunk. Out-of-range tail rows are
/// left as the caller supplied them — the Set/Get pair clamps to the same
/// fixed register file, so an in-range round-trip is exact, which is all the
/// D3D9 ABI promises here. The copy runs over bytes: the caller's array
/// promises no alignment.
///
/// # Safety
/// `out` is non-null and writable for `count` `T`s; alignment is not required.
unsafe fn copy_constants_out<T: Copy>(mirror: &[T], start: u32, out: *mut T, count: usize) {
    let start = start as usize;
    let end = (start + count).min(mirror.len());
    if start >= end {
        return;
    }
    // SAFETY: the caller's contract covers `count` values, at least `end - start`; the
    // mirror is device memory, so the two do not overlap, and alignment is not assumed.
    unsafe { mtld3d_shared::slice_to_caller(out, &mirror[start..end]) };
}

/// How many registers an integer or boolean constant call moves, or the `HRESULT` refusing it.
///
/// D3D9 refuses a null array and a start at or past the `rows`-deep register
/// file whatever the count, accepts a zero count as a call that moves
/// nothing, and clamps a count that runs past the file to the registers that
/// remain, so the caller copy is bounded before an unaligned input can
/// allocate. Shared by the eight `Set`/`Get` integer and boolean thunks.
fn int_bool_constant_rows(
    call: &str,
    has_data: bool,
    start: u32,
    count: u32,
    rows: usize,
) -> Result<usize, i32> {
    if !has_data {
        return Err(D3DERR_INVALIDCALL);
    }
    int_bool_rows(start, count, rows).ok_or_else(|| reject_constant_start(call, start, rows))
}

/// The `INVALIDCALL` an integer or boolean constant call answers for a start past the file.
///
/// Out of line so the setters do not build the log arguments on their hot path.
#[cold]
#[inline(never)]
fn reject_constant_start(call: &str, start: u32, rows: usize) -> i32 {
    mtld3d_shared::log_once_warn_by!(
        target: LOG_TARGET,
        key: caller_log_key(call),
        "{call}: start register {start} past the {rows}-register file → INVALIDCALL"
    );
    D3DERR_INVALIDCALL
}

/// The `INVALIDCALL` a float constant setter or getter answers for a window past the file.
///
/// Out of line so the four calls do not build the log arguments on their hot path.
#[cold]
#[inline(never)]
fn reject_constant_window(call: &str, start: u32, count: u32, rows: usize) -> i32 {
    mtld3d_shared::log_once_warn_by!(
        target: LOG_TARGET,
        key: caller_log_key(call),
        "{call}: registers {start}+{count} run past the {rows}-register file → INVALIDCALL"
    );
    D3DERR_INVALIDCALL
}

extern "system" fn device_get_vertex_shader_constant_f(
    this: *mut c_void,
    start_register: u32,
    constant_data: *mut f32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    if constant_data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if !window_in_range(start_register, count, CONSTANT_ROWS) {
        return reject_constant_window(
            "GetVertexShaderConstantF",
            start_register,
            count,
            CONSTANT_ROWS,
        );
    }
    // A zero count reads nothing, which D3D9 accepts.
    if count == 0 {
        return D3D_OK;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let mirror = obj.inner().shader_bindings().vs_constants_copy();
    // SAFETY: `constant_data` is non-null and `count` non-zero (both settled above); per the D3D9
    // ABI the caller guarantees `count * 4` `f32`s are writable; alignment is not assumed.
    unsafe {
        copy_constants_out(
            &mirror,
            start_register,
            constant_data.cast::<[f32; 4]>(),
            count as usize,
        );
    }
    0
}

extern "system" fn device_set_vertex_shader_constant_i(
    this: *mut c_void,
    start_register: u32,
    constant_data: *const i32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    let count = match int_bool_constant_rows(
        "SetVertexShaderConstantI",
        !constant_data.is_null(),
        start_register,
        count,
        INT_CONSTANT_ROWS,
    ) {
        Ok(0) => return D3D_OK,
        Ok(rows) => rows,
        Err(hr) => return hr,
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // SAFETY: `constant_data` is non-null and `count` is bounded above;
    // per the D3D9 ABI, the bounded rows are initialized and readable
    // without mutation in one allocation for this call. The byte size
    // fits `isize::MAX`, and all `i32` bit patterns are valid.
    let constants =
        unsafe { mtld3d_shared::slice_from_caller(constant_data.cast::<[i32; 4]>(), count) };
    let slice: &[[i32; 4]] = &constants;
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::VertexShaderConstantI {
            start: start_register,
            values: slice.to_vec(),
        });
        return D3D_OK;
    }
    // A VS reading a dynamic integer constant (a `loop`/`rep` counter) consumes
    // these via the `vs_i` buffer (vertex slot 14), captured into the snapshot
    // on change. Boolean constants take the same route as a bitmask.
    let changed = dev
        .shader_bindings_mut()
        .write_vs_constants_i(start_register, slice);
    if changed {
        dev.mark_snapshot_dirty(SnapshotDirty::VS_CONST_I);
    }
    0
}

extern "system" fn device_get_vertex_shader_constant_i(
    this: *mut c_void,
    start_register: u32,
    constant_data: *mut i32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    let count = match int_bool_constant_rows(
        "GetVertexShaderConstantI",
        !constant_data.is_null(),
        start_register,
        count,
        INT_CONSTANT_ROWS,
    ) {
        Ok(0) => return D3D_OK,
        Ok(rows) => rows,
        Err(hr) => return hr,
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let mirror = obj.inner().shader_bindings().vs_constants_i_copy();
    // SAFETY: `constant_data` is non-null and `count` is non-zero and clamped to the file (above);
    // per the D3D9 ABI the caller guarantees `count * 4` `i32`s are writable; alignment is not
    // assumed.
    unsafe {
        copy_constants_out(
            &mirror,
            start_register,
            constant_data.cast::<[i32; 4]>(),
            count,
        );
    }
    0
}

extern "system" fn device_set_vertex_shader_constant_b(
    this: *mut c_void,
    start_register: u32,
    constant_data: *const i32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    let count = match int_bool_constant_rows(
        "SetVertexShaderConstantB",
        !constant_data.is_null(),
        start_register,
        count,
        BOOL_CONSTANT_COUNT,
    ) {
        Ok(0) => return D3D_OK,
        Ok(rows) => rows,
        Err(hr) => return hr,
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // SAFETY: `constant_data` is non-null and `count` is bounded above;
    // per the D3D9 ABI, the bounded BOOLs are initialized and readable
    // without mutation in one allocation for this call. The byte size
    // fits `isize::MAX`, and all `i32` bit patterns are valid.
    let constants = unsafe { mtld3d_shared::slice_from_caller(constant_data, count) };
    let slice = &*constants;
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::VertexShaderConstantB {
            start: start_register,
            values: slice.to_vec(),
        });
        return D3D_OK;
    }
    // A VS reading a dynamic boolean constant (a static `if b0`) consumes
    // these as the `vs_b` bitmask (vertex slot 26), captured into the
    // snapshot on change.
    let changed = dev
        .shader_bindings_mut()
        .write_vs_constants_b(start_register, slice);
    if changed {
        dev.mark_snapshot_dirty(SnapshotDirty::VS_CONST_B);
    }
    0
}

extern "system" fn device_get_vertex_shader_constant_b(
    this: *mut c_void,
    start_register: u32,
    constant_data: *mut i32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    let count = match int_bool_constant_rows(
        "GetVertexShaderConstantB",
        !constant_data.is_null(),
        start_register,
        count,
        BOOL_CONSTANT_COUNT,
    ) {
        Ok(0) => return D3D_OK,
        Ok(rows) => rows,
        Err(hr) => return hr,
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let mirror = obj.inner().shader_bindings().vs_constants_b_copy();
    // SAFETY: `constant_data` is non-null and `count` is non-zero and clamped to the file (above);
    // per the D3D9 ABI the caller guarantees `count` `BOOL`s are writable; alignment is not
    // assumed.
    unsafe { copy_constants_out(&mirror, start_register, constant_data, count) };
    0
}

extern "system" fn device_set_stream_source(
    this: *mut c_void,
    stream: u32,
    data: *mut c_void,
    offset: u32,
    stride: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Buffer);
    if stream >= mtld3d_types::MAX_STREAMS {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject SetStreamSource(stream={stream}) → INVALIDCALL (exceeds max_streams)"
        );
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let vb = data.cast::<Direct3DVertexBuffer9>();
    if let Some(rec) = dev.recording_state_block_mut() {
        // SAFETY: `vb` is null or a *mut Direct3DVertexBuffer9 supplied by
        // the calling game via SetStreamSource.
        let adopted = unsafe { CachedComPtr::adopt(vb) };
        rec.record(StateOp::StreamSource {
            stream,
            vb: adopted,
            offset,
            stride,
        });
        return D3D_OK;
    }
    dev.bound_buffers_mut()
        .set_stream(stream as usize, vb, offset, stride);
    D3D_OK
}

extern "system" fn device_get_stream_source(
    this: *mut c_void,
    stream: u32,
    stream_data: *mut *mut c_void,
    offset_in_bytes: *mut u32,
    stride: *mut u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Buffer);
    if stream_data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // Streams 0..MAX round-trip their binding; a caller that binds a stream
    // and reads it back — relying on the binding outliving its own Release —
    // sees the buffer. An out-of-range stream is unbound → NULL/0 per the
    // "nothing bound" contract (S_OK).
    let (vb_ptr, offset, vb_stride) = if stream < mtld3d_types::MAX_STREAMS {
        let b = dev.bound_buffers();
        (
            b.stream_vertex_buffer(stream as usize),
            b.stream_offset(stream as usize),
            b.stream_stride(stream as usize),
        )
    } else {
        (std::ptr::null_mut(), 0, 0)
    };
    if !vb_ptr.is_null() {
        // SAFETY: `vb_ptr` is non-null (checked) and points to a live
        // Direct3DVertexBuffer9 kept alive by the device binding.
        let add_ref = unsafe { (*vb_ptr).vtbl().add_ref };
        // SAFETY: calling the just-loaded `add_ref` thunk; D3D9 mandates
        // AddRef on out-pointer returns.
        unsafe { add_ref(vb_ptr.cast::<c_void>()) };
    }
    // SAFETY: `stream_data` is non-null (checked) and the D3D9 ABI guarantees a
    // writable `*mut c_void` slot; alignment is not assumed.
    unsafe { OutPtr::write_opt(stream_data, vb_ptr.cast::<c_void>()) };
    if !offset_in_bytes.is_null() {
        // SAFETY: caller-supplied writable `u32` slot per the D3D9 ABI; alignment is not assumed.
        unsafe { OutPtr::write_opt(offset_in_bytes, offset) };
    }
    if !stride.is_null() {
        // SAFETY: caller-supplied writable `u32` slot per the D3D9 ABI; alignment is not assumed.
        unsafe { OutPtr::write_opt(stride, vb_stride) };
    }
    0 // S_OK
}

extern "system" fn device_set_stream_source_freq(
    this: *mut c_void,
    stream: u32,
    setting: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Buffer);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    if let Err(reason) = validate_stream_freq(stream, setting) {
        // A rejected call leaves the stored frequency untouched.
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject SetStreamSourceFreq(stream={stream}, setting={setting:#x}) → INVALIDCALL ({reason:?})"
        );
        return D3DERR_INVALIDCALL;
    }
    let dev = obj.inner();
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::StreamSourceFreq { stream, setting });
        return D3D_OK;
    }
    dev.bound_buffers_mut()
        .set_stream_freq(stream as usize, setting);
    D3D_OK
}

extern "system" fn device_get_stream_source_freq(
    this: *mut c_void,
    stream: u32,
    setting: *mut u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Buffer);
    if setting.is_null() || stream >= mtld3d_types::MAX_STREAMS {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // The raw word round-trips, flags included.
    let value = obj.inner().bound_buffers().stream_freq(stream as usize);
    // SAFETY: `setting` is non-null (checked) and per the D3D9 ABI points to a
    // writable `u32` slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(setting, value) };
    D3D_OK
}

extern "system" fn device_set_indices(this: *mut c_void, index_data: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Buffer);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let ib = index_data.cast::<Direct3DIndexBuffer9>();
    if let Some(rec) = dev.recording_state_block_mut() {
        // SAFETY: `ib` is null or a *mut Direct3DIndexBuffer9 supplied by
        // the calling game via SetIndices.
        let adopted = unsafe { CachedComPtr::adopt(ib) };
        rec.record(StateOp::Indices(adopted));
        return D3D_OK;
    }
    dev.bound_buffers_mut().replace_index_buffer(ib);
    D3D_OK
}

extern "system" fn device_get_indices(this: *mut c_void, index_data: *mut *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Buffer);
    if index_data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let ib_ptr = dev.bound_buffers().index_buffer();
    if !ib_ptr.is_null() {
        // SAFETY: `ib_ptr` is non-null (checked) and points to a live
        // Direct3DIndexBuffer9 kept alive by the device binding.
        let add_ref = unsafe { (*ib_ptr).vtbl().add_ref };
        // SAFETY: calling the just-loaded `add_ref` thunk; D3D9 mandates
        // AddRef on out-pointer returns.
        unsafe { add_ref(ib_ptr.cast::<c_void>()) };
    }
    // SAFETY: `index_data` is non-null (checked) and the D3D9 ABI guarantees a
    // writable `*mut c_void` slot; alignment is not assumed.
    unsafe { OutPtr::write_opt(index_data, ib_ptr.cast::<c_void>()) };
    0 // S_OK
}

extern "system" fn device_create_pixel_shader(
    this: *mut c_void,
    function: *const u32,
    shader: *mut *mut c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    null_out(shader);
    if function.is_null() || shader.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let Some(bytecode) = read_shader_bytecode(function) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; this is a live Direct3DDevice9 per the D3D9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    if !dev.config().bytecode_dump_dir.is_empty() {
        maybe_dump_bytecode(
            &dev.config().bytecode_dump_dir,
            "ps",
            ProgramId::from_tokens(&bytecode),
            &bytecode,
        );
    }
    let (parsed, meta, code) = match core_draw::create_core_shader(
        dev,
        &bytecode,
        &mtld3d_shared::shader_create::ShaderStage::Pixel,
    ) {
        Ok(created) => created,
        Err(result) => return result,
    };
    let mut usage = crate::pixel_shader::PsUsage::empty();
    usage.set(
        crate::pixel_shader::PsUsage::AUTOMATIC_FOG,
        parsed
            .usage
            .contains(mtld3d_shared::shader_create::ShaderUsage::AUTOMATIC_FOG),
    );
    usage.set(
        crate::pixel_shader::PsUsage::USES_BUMP_ENV,
        parsed
            .usage
            .contains(mtld3d_shared::shader_create::ShaderUsage::BUMP_ENV),
    );
    usage.set(
        crate::pixel_shader::PsUsage::USES_INT_CONST,
        parsed
            .usage
            .contains(mtld3d_shared::shader_create::ShaderUsage::INT_CONST),
    );
    usage.set(
        crate::pixel_shader::PsUsage::USES_BOOL_CONST,
        parsed
            .usage
            .contains(mtld3d_shared::shader_create::ShaderUsage::BOOL_CONST),
    );
    usage.set(
        crate::pixel_shader::PsUsage::USES_REL_CONST,
        parsed
            .usage
            .contains(mtld3d_shared::shader_create::ShaderUsage::RELATIVE_CONST),
    );
    usage.set(
        crate::pixel_shader::PsUsage::READS_LINKED_INPUTS,
        parsed
            .usage
            .contains(mtld3d_shared::shader_create::ShaderUsage::LINKED_INPUTS),
    );
    let tokens = code.shared_tokens();
    let shader_obj = Direct3DPixelShader9::new(
        obj.inner_ptr(),
        parsed.max_const_used,
        usage,
        parsed.color_out_mask,
        tokens,
        (code, meta),
    );
    let shader_ptr = Box::into_raw(Box::new(shader_obj));
    // SAFETY: shader_ptr is a freshly created live shader at refcount one.
    unsafe { crate::com_ref::com_register_child(shader_ptr) };
    // SAFETY: the D3D9 ABI supplies a writable shader out pointer, checked non-null above.
    unsafe { OutPtr::write_opt(shader, shader_ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn device_set_pixel_shader(this: *mut c_void, shader: *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Shader);
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let new = shader.cast::<Direct3DPixelShader9>();
    if let Some(rec) = dev.recording_state_block_mut() {
        // SAFETY: `new` is null or a *mut Direct3DPixelShader9 supplied by
        // the calling game via SetPixelShader.
        let adopted = unsafe { CachedComPtr::adopt(new) };
        rec.record(StateOp::PixelShader(adopted));
        return D3D_OK;
    }
    // Redundant-set elimination: re-binding the same PS pointer leaves
    // the shader id / max_const_used identical, so skip the rebuild.
    let automatic_fog = dev.uses_automatic_fog();
    let changed = dev.shader_bindings_mut().replace_pixel_shader(new);
    if changed {
        dev.mark_snapshot_dirty(SnapshotDirty::PS_SOURCE | SnapshotDirty::PS_CONST);
        if automatic_fog != dev.uses_automatic_fog() {
            // SM3 owns its fog. Rebinding a legacy shader restores both its
            // fog specialization and the uniform bytes suppressed for SM3.
            dev.mark_snapshot_dirty(SnapshotDirty::VARIANT | SnapshotDirty::FOG_COLOR);
        }
    }
    dev.perf_mut()
        .record_keys_gate(KeysGate::SetPixelShader, !changed);
    0
}

extern "system" fn device_get_pixel_shader(this: *mut c_void, shader: *mut *mut c_void) -> i32 {
    let _api = device_api_lock(this);
    let _timer = bind_timer(this, BindSubCategory::Shader);
    if shader.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    let ps = dev.shader_bindings().pixel_shader();
    if !ps.is_null() {
        // SAFETY: `ps` is non-null (checked) and points to a live
        // Direct3DPixelShader9 kept alive by the device binding.
        let add_ref = unsafe { (*ps).vtbl().add_ref };
        // SAFETY: calling the just-loaded `add_ref` thunk; D3D9 mandates
        // AddRef on out-pointer returns (the caller Releases the result).
        unsafe { add_ref(ps.cast::<c_void>()) };
    }
    // SAFETY: `shader` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `*mut c_void` slot owned by the caller; alignment is not assumed.
    unsafe { OutPtr::write_opt(shader, ps.cast::<c_void>()) };
    0
}

extern "system" fn device_set_pixel_shader_constant_f(
    this: *mut c_void,
    start_register: u32,
    constant_data: *const f32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    if constant_data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if !window_in_range(start_register, count, PS_FLOAT_CONSTANT_LIMIT) {
        return reject_constant_window(
            "SetPixelShaderConstantF",
            start_register,
            count,
            PS_FLOAT_CONSTANT_LIMIT,
        );
    }
    // A zero count names an empty window, which D3D9 accepts and writes nothing into.
    if count == 0 {
        return D3D_OK;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // SAFETY: `constant_data` is non-null and `count` non-zero (both
    // settled above); per the D3D9 ABI the caller guarantees the bounded floats
    // are initialized and readable without mutation in one allocation for
    // this call. The byte size fits `isize::MAX`; all `f32` bits are valid.
    let constants = unsafe {
        mtld3d_shared::slice_from_caller(constant_data.cast::<[f32; 4]>(), count as usize)
    };
    let slice: &[[f32; 4]] = &constants;
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::PixelShaderConstantF {
            start: start_register,
            values: slice.to_vec(),
        });
        return D3D_OK;
    }
    // Redundant-set elimination: see `device_set_vertex_shader_constant_f`.
    let changed = dev
        .shader_bindings_mut()
        .write_ps_constants(start_register, slice);
    if changed {
        propagate_ps_const_delta(dev, start_register, slice);
    }
    dev.perf_mut()
        .record_keys_gate(KeysGate::SetPsConst, !changed);
    0
}

extern "system" fn device_get_pixel_shader_constant_f(
    this: *mut c_void,
    start_register: u32,
    constant_data: *mut f32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    if constant_data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    if !window_in_range(start_register, count, PS_FLOAT_CONSTANT_LIMIT) {
        return reject_constant_window(
            "GetPixelShaderConstantF",
            start_register,
            count,
            PS_FLOAT_CONSTANT_LIMIT,
        );
    }
    // A zero count reads nothing, which D3D9 accepts.
    if count == 0 {
        return D3D_OK;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let mirror = obj.inner().shader_bindings().ps_constants_copy();
    // SAFETY: `constant_data` is non-null and `count` non-zero (both settled above); per the D3D9
    // ABI the caller guarantees `count * 4` `f32`s are writable; alignment is not assumed.
    unsafe {
        copy_constants_out(
            &mirror,
            start_register,
            constant_data.cast::<[f32; 4]>(),
            count as usize,
        );
    }
    0
}

extern "system" fn device_set_pixel_shader_constant_i(
    this: *mut c_void,
    start_register: u32,
    constant_data: *const i32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    let count = match int_bool_constant_rows(
        "SetPixelShaderConstantI",
        !constant_data.is_null(),
        start_register,
        count,
        INT_CONSTANT_ROWS,
    ) {
        Ok(0) => return D3D_OK,
        Ok(rows) => rows,
        Err(hr) => return hr,
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // SAFETY: `constant_data` is non-null and `count` is bounded above;
    // per the D3D9 ABI, the bounded rows are initialized and readable
    // without mutation in one allocation for this call. The byte size
    // fits `isize::MAX`, and all `i32` bit patterns are valid.
    let constants =
        unsafe { mtld3d_shared::slice_from_caller(constant_data.cast::<[i32; 4]>(), count) };
    let slice: &[[i32; 4]] = &constants;
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::PixelShaderConstantI {
            start: start_register,
            values: slice.to_vec(),
        });
        return D3D_OK;
    }
    if dev
        .shader_bindings_mut()
        .write_ps_constants_i(start_register, slice)
    {
        dev.mark_snapshot_dirty(SnapshotDirty::PS_CONST_I);
    }
    0
}

extern "system" fn device_get_pixel_shader_constant_i(
    this: *mut c_void,
    start_register: u32,
    constant_data: *mut i32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    let count = match int_bool_constant_rows(
        "GetPixelShaderConstantI",
        !constant_data.is_null(),
        start_register,
        count,
        INT_CONSTANT_ROWS,
    ) {
        Ok(0) => return D3D_OK,
        Ok(rows) => rows,
        Err(hr) => return hr,
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let mirror = obj.inner().shader_bindings().ps_constants_i_copy();
    // SAFETY: `constant_data` is non-null and `count` is non-zero and clamped to the file (above);
    // per the D3D9 ABI the caller guarantees `count * 4` `i32`s are writable; alignment is not
    // assumed.
    unsafe {
        copy_constants_out(
            &mirror,
            start_register,
            constant_data.cast::<[i32; 4]>(),
            count,
        );
    }
    0
}

extern "system" fn device_set_pixel_shader_constant_b(
    this: *mut c_void,
    start_register: u32,
    constant_data: *const i32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    let count = match int_bool_constant_rows(
        "SetPixelShaderConstantB",
        !constant_data.is_null(),
        start_register,
        count,
        BOOL_CONSTANT_COUNT,
    ) {
        Ok(0) => return D3D_OK,
        Ok(rows) => rows,
        Err(hr) => return hr,
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtrMut::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let dev = obj.inner();
    // SAFETY: `constant_data` is non-null and `count` is bounded above;
    // per the D3D9 ABI, the bounded BOOLs are initialized and readable
    // without mutation in one allocation for this call. The byte size
    // fits `isize::MAX`, and all `i32` bit patterns are valid.
    let constants = unsafe { mtld3d_shared::slice_from_caller(constant_data, count) };
    let slice = &*constants;
    if let Some(rec) = dev.recording_state_block_mut() {
        rec.record(StateOp::PixelShaderConstantB {
            start: start_register,
            values: slice.to_vec(),
        });
        return D3D_OK;
    }
    if dev
        .shader_bindings_mut()
        .write_ps_constants_b(start_register, slice)
    {
        dev.mark_snapshot_dirty(SnapshotDirty::PS_CONST_B);
    }
    0
}

extern "system" fn device_get_pixel_shader_constant_b(
    this: *mut c_void,
    start_register: u32,
    constant_data: *mut i32,
    count: u32,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::ShaderConst);
    let count = match int_bool_constant_rows(
        "GetPixelShaderConstantB",
        !constant_data.is_null(),
        start_register,
        count,
        BOOL_CONSTANT_COUNT,
    ) {
        Ok(0) => return D3D_OK,
        Ok(rows) => rows,
        Err(hr) => return hr,
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let mirror = obj.inner().shader_bindings().ps_constants_b_copy();
    // SAFETY: `constant_data` is non-null and `count` is non-zero and clamped to the file (above);
    // per the D3D9 ABI the caller guarantees `count` `BOOL`s are writable; alignment is not
    // assumed.
    unsafe { copy_constants_out(&mirror, start_register, constant_data, count) };
    0
}

extern "system" fn device_draw_rect_patch(
    this: *mut c_void,
    _handle: u32,
    _num_segs: *const f32,
    _tri_patch_info: *const c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "stub IDirect3DDevice9::DrawRectPatch → INVALIDCALL");
    D3DERR_INVALIDCALL
}

extern "system" fn device_draw_tri_patch(
    this: *mut c_void,
    _handle: u32,
    _num_segs: *const f32,
    _tri_patch_info: *const c_void,
) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "stub IDirect3DDevice9::DrawTriPatch → INVALIDCALL");
    D3DERR_INVALIDCALL
}

extern "system" fn device_delete_patch(this: *mut c_void, _handle: u32) -> i32 {
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "stub IDirect3DDevice9::DeletePatch → INVALIDCALL");
    D3DERR_INVALIDCALL
}

extern "system" fn device_create_query(
    this: *mut c_void,
    type_: u32,
    query: *mut *mut c_void,
) -> i32 {
    use crate::query::{Direct3DQuery9, data_size_for};
    let _api = device_api_lock(this);
    let _timer = device_timer(this, DeviceSubCategory::Misc);
    // `query` being null is the D3D9 idiom for "is this query type supported?"
    // (returns S_OK without allocating). Unsupported types are logged so an
    // unhandled query type surfaces.
    let Some(data_size) = data_size_for(type_) else {
        warn!(
            target: LOG_TARGET,
            "reject CreateQuery(type={type_}) → NOTAVAILABLE (unsupported query type)"
        );
        return crate::D3DERR_NOTAVAILABLE;
    };
    if query.is_null() {
        return D3D_OK;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DDevice9 per IDirect3DDevice9 ABI.
    let Some(dev_obj) = (unsafe { InPtr::<Direct3DDevice9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let obj = Direct3DQuery9::new(dev_obj.inner_ptr(), type_, data_size);
    // SAFETY: vtable out-param; `query` is *mut *mut c_void per IDirect3DDevice9 ABI.
    let query_ptr = Box::into_raw(Box::new(obj));
    // SAFETY: `query_ptr` is a freshly created, live query at refcount 1.
    unsafe { crate::com_ref::com_register_child(query_ptr) };
    // SAFETY: vtable out-param; `query` is *mut *mut c_void per the ABI.
    unsafe { OutPtr::write_opt(query, query_ptr.cast::<c_void>()) };
    D3D_OK
}

// ── Silent-field audit: CreateTexture / CreateVertexBuffer / CreateIndexBuffer
// usage bits + pool value.
// Each unhandled usage bit and each non-default pool value gets one warn per
// process (log_once_warn is per-call-site; helper is called from all three
// Create* entry points so the first site to hit a bit wins).

fn warn_unused_usage_and_pool_once(kind: &str, usage: u32, pool: u32) {
    // Usage bits we actually honor at some level.

    // D3DUSAGE_DYNAMIC: no warn — every VB/IB uses a `PageBox` wrapped
    // as a Shared MTLBuffer on first draw (vertex_buffer.rs::vb_lock +
    // encoder.rs::ensure_{vb,ib}_mtl_buffer). Rename-on-DISCARD fires on
    // contention regardless of this flag, so we honour the dynamic
    // contract universally.
    // D3DUSAGE_WRITEONLY: no warn — Metal has no write-combined storage
    // tier. The PageBox backing is StorageModeShared; a Private+staging
    // variant would double uploads and defeat the zero-copy property we
    // care about. Architectural non-feature, not a stub.
    if usage & D3DUSAGE_SOFTWAREPROCESSING != 0 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "Create{kind}: D3DUSAGE_SOFTWAREPROCESSING set but software-vertex-processing not supported"
        );
    }
    if usage & D3DUSAGE_DONOTCLIP != 0 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "Create{kind}: D3DUSAGE_DONOTCLIP set but clip-bypass not honoured");
    }
    if usage & D3DUSAGE_POINTS != 0 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "Create{kind}: D3DUSAGE_POINTS set but point-sprites not implemented"
        );
    }
    if usage & D3DUSAGE_RTPATCHES != 0 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "Create{kind}: D3DUSAGE_RTPATCHES set but rectangular patches not implemented"
        );
    }
    if usage & D3DUSAGE_NPATCHES != 0 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "Create{kind}: D3DUSAGE_NPATCHES set but N-patches not implemented");
    }
    // D3DUSAGE_AUTOGENMIPMAP is honoured for textures (`Create{Texture,
    // CubeTexture,VolumeTexture}` — runtime owns the mip chain via
    // Metal's blit-encoder `generateMipmaps`. Compressed-format requests
    // are dropped silently in `device_create_texture` since Metal
    // refuses to regenerate BC/DXT.
    if usage & D3DUSAGE_NONSECURE != 0 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "Create{kind}: D3DUSAGE_NONSECURE set but non-secure hint ignored");
    }
    // D3DUSAGE_DMAP marks a displacement map for the N-patch tessellator,
    // which no modern driver runs either; the texture is an ordinary texture.
    if usage & D3DUSAGE_DMAP != 0 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "Create{kind}: D3DUSAGE_DMAP set but displacement mapping not implemented");
    }

    // Surface any remaining unknown bits (beyond the union of honored + warned).
    let known = D3DUSAGE_RENDERTARGET
        | D3DUSAGE_DEPTHSTENCIL
        | D3DUSAGE_WRITEONLY
        | D3DUSAGE_SOFTWAREPROCESSING
        | D3DUSAGE_DONOTCLIP
        | D3DUSAGE_POINTS
        | D3DUSAGE_RTPATCHES
        | D3DUSAGE_NPATCHES
        | D3DUSAGE_DYNAMIC
        | D3DUSAGE_AUTOGENMIPMAP
        | D3DUSAGE_NONSECURE
        | D3DUSAGE_DMAP;
    let unknown = usage & !known;
    if unknown != 0 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "Create{kind}: unknown usage bits {unknown:#x} — update warn_unused_usage_and_pool_once"
        );
    }

    // Pool. DEFAULT = 0.
    // D3DPOOL_MANAGED: behaviourally equivalent to DEFAULT in our
    // implementation. Metal's StorageModeShared/Managed textures are already
    // CPU+GPU accessible with OS-level paging, so "driver-managed residency"
    // is a non-concept. We also don't implement device loss (see
    // device_reset), so MANAGED's "survives reset" promise is vacuously true
    // for DEFAULT too. Accept and move on.
    // D3DPOOL_SYSTEMMEM / D3DPOOL_SCRATCH on a texture: a system-memory
    // resource, created with staging and no Metal texture
    // (`TextureFlags::CPU_ONLY`).
    // D3DPOOL_SYSTEMMEM on a vertex / index buffer: D3D9 draws from one
    // directly, so the buffer keeps its `MTLBuffer` and behaves like a
    // default-pool buffer. D3DPOOL_SCRATCH never reaches here for a buffer;
    // both Create* entry points reject it first.
    match pool {
        0 | D3DPOOL_MANAGED | D3DPOOL_MANAGED_EX | D3DPOOL_SYSTEMMEM | D3DPOOL_SCRATCH => {}
        other => {
            mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                "Create{kind}: unknown D3DPOOL={other} — update warn_unused_usage_and_pool_once"
            );
        }
    }
}

// ── Silent-field audit: D3DPRESENT_PARAMETERS ──
// `back_buffer_format` already warns at d3d9_create_device. Warn on
// every other non-default field so the next mismatched present-time
// expectation surfaces on first device creation / reset.

pub fn warn_present_params_fields_once(pp: &mtld3d_types::D3DPRESENT_PARAMETERS) {
    if pp.back_buffer_count > 1 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "CreateDevice: back_buffer_count={} requested but only single back-buffer supported",
            pp.back_buffer_count
        );
    }
    if pp.swap_effect != 0 && pp.swap_effect != 1 {
        // 0 is invalid per spec; 1 = D3DSWAPEFFECT_DISCARD. FLIP and COPY
        // keep the one back buffer's contents across Present
        // (`BackbufferContents::Preserved`); FLIP's buffer rotation would need
        // a real swap chain instead of a single drawable.
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "CreateDevice: swap_effect={} requested: the back buffer keeps its contents \
             across Present but is not rotated",
            pp.swap_effect
        );
    }
    // D3DPRESENTFLAG_LOCKABLE_BACKBUFFER (0x1) is honoured: WoW's portrait
    // path locks the backbuffer + reads it back through BlitTextureToBuffer.
    // Warn once per distinct bit-combo for anything else so D3DPRESENTFLAG_*
    // additions don't collapse into a single stale line.
    let unhandled_present_flags = pp.flags & !D3DPRESENTFLAG_LOCKABLE_BACKBUFFER;
    if unhandled_present_flags != 0 {
        mtld3d_shared::log_once_warn_by!(
            target: crate::LOG_TARGET,
            key: u64::from(unhandled_present_flags),
            "CreateDevice: flags={:#x} set but D3DPRESENTFLAG_* bits {:#x} not honoured",
            pp.flags, unhandled_present_flags
        );
    }
    if pp.full_screen_refresh_rate_in_hz != 0 {
        // The fullscreen mode set asks for the rate, then for the mode
        // without it (`mode_set_attempts`); vsync follows the panel.
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "CreateDevice: full_screen_refresh_rate_in_hz={} requested: the display mode set \
             asks for it; presentation intervals ONE and DEFAULT follow the panel's own rate",
            pp.full_screen_refresh_rate_in_hz
        );
    }
    // `presentation_interval` is honoured on the core present path via
    // `mtld3d_core::present::layer_pacing_for`.
}
