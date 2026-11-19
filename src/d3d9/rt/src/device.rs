//! The core device under D3D9: its Metal device and its queue profile.
//!
//! The Metal device is winemetal's (`WMTCopyAllDevices`), the one whose registry ID the adapter
//! `GetDeviceInfo` reported has; the queue takes D3D9's memory profile ([`queue_profile`]).

use std::sync::{Arc, OnceLock};

use dxmt_core::{device::Device, pipeline_cache::PsoMap, queue::QueueConfig, staging::Staging};
use mtld3d_core::pipeline_key::{PipelineKey, PipelineSource};
use winemetal_sys::{
    MTLDevice_registryID, NSArray_count, NSArray_object, NSObject_release, Obj, WMTCopyAllDevices,
};

/// The `log` target of the core runtime's lines.
pub const LOG_TARGET: &str = "mtld3d::d3d9::core";

/// A D3D9 device's core: the Metal device, the core's command queue and what it records.
///
/// The recording state is the API thread's view of the chunk being recorded: whether the last
/// closure left a render pass open, and whether the chunk holds any work. [`Drop`] drains the
/// queue first; then fields drop in order: the queue (its threads, and the chunks holding the
/// closures) before the back buffer and the window the closures name, and the pipelines, the
/// stage functions, the depth-stencil states, the `StretchRect` quad's functions and pipelines
/// and the depth-stencil resolve's, whose raw handles the closures carry.
pub struct CoreDevice {
    device: Device,
    back_buffer: Option<swapchain::BackBuffer>,
    window: Option<swapchain::Window>,
    /// The read-back destination, kept while it is long enough and remade only to grow.
    ///
    /// Reads of different sizes alternate (whole targets, rects of the back buffer); its ceiling
    /// is the largest read's bytes, held for the device's life (DXMTRS C4c-3a).
    readback: Option<Arc<Staging>>,
    /// The pipelines by key, each with its source (none when a stage does not parse).
    ///
    /// Handle 0 for one that did not build or was not built (its layout crosses the stride).
    /// Never cleared: a recorded closure names its pipeline by handle until the drain.
    pipelines: PsoMap<PipelineKey, Option<PipelineSource>>,
    /// The compiled stage functions the pipelines are made of.
    functions: draw::StageFunctions,
    /// The D3D9 textures' core textures, by id (C4b-3b).
    textures: rustc_hash::FxHashMap<mtld3d_core::ids::TextureId, textures::Stored>,
    /// The D3D9 vertex and index buffers' core buffers, by id (DXMTRS C4c-1).
    ///
    /// A dynamic one's with its names (DXMTRS C4c-2b).
    buffers: buffers::BufferStore,
    /// The samplers by their key's bits (C4b-3b).
    samplers: rustc_hash::FxHashMap<u64, Arc<dxmt_core::sampler::Sampler>>,
    /// The depth-stencil states by their key's bits (C4b-3d2).
    ///
    /// Never cleared, as the pipelines: a recorded closure names its state by handle.
    depth_states: rustc_hash::FxHashMap<u64, Obj>,
    /// The `StretchRect` quad's two functions, made at the first sampled copy (DXMTRS C4c-4c).
    ///
    /// Of the library compiled from mtld3d's source; none inside once Metal made no library or
    /// function, which the device does not try again.
    quad_functions: OnceLock<Option<[Obj; 2]>>,
    /// The quad's pipelines by the destination's stored format and sample count.
    ///
    /// None for one Metal does not make, never made again. Never cleared, as the pipelines: a
    /// recorded closure names its pipeline by handle.
    quad_pipelines: rustc_hash::FxHashMap<(u32, u8), Option<Obj>>,
    /// The depth-stencil resolve's three functions, made at the first resolve (DXMTRS C4c-4d).
    ///
    /// The vertex function, then the fragment functions of a depth plane alone and of depth and
    /// stencil; none inside once Metal made no library or function, which the device does not try
    /// again.
    resolve_functions: OnceLock<Option<[Obj; 3]>>,
    /// The resolve's pipelines by the destination's `WMTPixelFormat`.
    ///
    /// None for one Metal does not make, never made again. Never cleared, as the pipelines: a
    /// recorded closure names its pipeline by handle.
    resolve_pipelines: rustc_hash::FxHashMap<u32, Option<Obj>>,
    /// The colour target draws and clears go to (C4b-3d1).
    target: TargetRef,
    /// The colour targets bound at index 1-3: `extras[i]` is slot `i + 1`'s (DXMTRS C4b-3d4).
    ///
    /// A draw's pass attaches those of the pass set ([`CoreDevice::target_desc`]).
    extras: [Option<TargetRef>; 3],
    /// The depth-stencil draws test against and clears go to (C4b-3d2).
    depth: Option<DepthRef>,
    render_open: bool,
    /// The depth-stencil the open render pass has attached; none for a pass without one.
    open_depth: Option<DepthRef>,
    /// The `WMTPixelFormat` each colour slot of the open render pass attached; 0 for none.
    ///
    /// A target's own or its sRGB twin's; all 0 while no render pass is open.
    open_colors: [u32; 4],
    /// The scissor the open render pass last recorded; none since it began (DXMTRS C4b-3d3).
    pass_scissor: Option<[u32; 4]>,
    /// The last closure left a blit pass open (consecutive uploads share it).
    blit_open: bool,
    dirty: bool,
    /// Bytes staged for uploads in the chunk being recorded, and the budget past which it commits.
    staged_bytes: u64,
    staged_budget: u64,
    /// The vertex and index bytes the GPU side still holds, by chunk (DXMTRS C4c-2b).
    retained: retained::Retained,
    /// The cap on those bytes, `memory.vbibRetentionCapMB`; 0 for none.
    retention_cap: u64,
    /// The OCCLUSION brackets begun and not ended; none before the device's first BEGIN.
    ///
    /// One up a recorded begin, one down a recorded end (DXMTRS C4c-5). From the first BEGIN on,
    /// each draw pass and each triangle pass of the device's own starts with counting off; any
    /// bracket left open at the device's end is logged.
    open_brackets: Option<u32>,
}

impl CoreDevice {
    /// A core device on `mtl` whose queue has `profile`'s sizes; logs the device line once.
    ///
    /// # Panics
    ///
    /// When the queue's threads cannot be spawned.
    #[must_use]
    pub fn new(mtl: Obj, profile: QueueConfig) -> Self {
        let chunks = profile.chunks;
        let staging_block = profile.staging_block;
        let device = Device::with_config(mtl, 0, profile);
        log::log!(
            target: LOG_TARGET,
            gate_level(),
            "d3d9 core: device on dxmt-core ({chunks} chunks)"
        );
        if device.cmd_library_handle() == 0 && device.device().handle() > 0x1000 {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: no command library (dxmt_command.metallib), so nothing presents"
            );
        }
        Self {
            device,
            back_buffer: None,
            window: None,
            readback: None,
            pipelines: PsoMap::default(),
            functions: draw::StageFunctions::default(),
            textures: rustc_hash::FxHashMap::default(),
            buffers: buffers::BufferStore::default(),
            samplers: rustc_hash::FxHashMap::default(),
            depth_states: rustc_hash::FxHashMap::default(),
            quad_functions: OnceLock::new(),
            quad_pipelines: rustc_hash::FxHashMap::default(),
            resolve_functions: OnceLock::new(),
            resolve_pipelines: rustc_hash::FxHashMap::default(),
            target: TargetRef::BackBuffer,
            extras: [None; 3],
            depth: None,
            render_open: false,
            open_depth: None,
            open_colors: [0; 4],
            pass_scissor: None,
            blit_open: false,
            dirty: false,
            staged_bytes: 0,
            staged_budget: staged_budget(staging_block),
            retained: retained::Retained::default(),
            retention_cap: 0,
            open_brackets: None,
        }
    }

    /// The core device.
    #[must_use]
    pub const fn device(&self) -> &Device {
        &self.device
    }

    /// The core device, mutably: its queue records.
    pub const fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }
}

impl Drop for CoreDevice {
    fn drop(&mut self) {
        self.drain();
        // A bracket the front end never ended counted on every pass since its BEGIN.
        if let Some(open @ 1..) = self.open_brackets {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: {open} occlusion queries' END was not recorded"
            );
        }
    }
}

/// The level of the lines the gates read: a perf build compiles out what is below a warning.
const fn gate_level() -> log::Level {
    if cfg!(feature = "perf-build") {
        log::Level::Warn
    } else {
        log::Level::Info
    }
}

/// The bytes a chunk may stage for uploads before it is committed.
///
/// 32 MiB in a 32-bit profile (its 1 MiB ring blocks), 256 MiB otherwise.
const fn staged_budget(staging_block: u64) -> u64 {
    if staging_block <= 0x10_0000 {
        32 << 20
    } else {
        256 << 20
    }
}

/// D3D9's queue sizes in this process: [`queue_profile_for`] its pointer width.
#[must_use]
pub fn queue_profile() -> QueueConfig {
    queue_profile_for(usize::BITS)
}

/// D3D9's queue sizes for a process of `pointer_bits`.
///
/// D3D11's, whose 32 chunks also cover D3D9's frame latency of up to 30; a 32-bit process's
/// staging and argument rings take 1 MiB blocks, which it places in its own address space.
#[must_use]
pub fn queue_profile_for(pointer_bits: u32) -> QueueConfig {
    let mut profile = QueueConfig::default();
    if pointer_bits <= 32 {
        profile.staging_block = 0x10_0000;
        profile.gpu_heap = 0x10_0000;
    }
    profile
}

/// winemetal's device whose registry ID is `registry_id`; the first device when none is.
///
/// `None` when winemetal lists no device. A fallback logs a warning: the adapter `GetDeviceInfo`
/// described is then not the one the core renders on.
#[must_use]
pub fn select_device(registry_id: u64) -> Option<Obj> {
    // SAFETY: winemetal's call; the array it answers is this function's to release.
    let devices = unsafe { WMTCopyAllDevices() };
    if devices == 0 {
        return None;
    }
    // SAFETY: a live array of devices.
    let count = unsafe { NSArray_count(devices) };
    // SAFETY: (each) an index below the array's count.
    let at = |index: u64| unsafe { NSArray_object(devices, index) };
    let found = (0..count)
        .map(at)
        // SAFETY: a device of the live array.
        .find(|&device| unsafe { MTLDevice_registryID(device) } == registry_id);
    let picked = found.or_else(|| {
        (count > 0).then(|| {
            log::warn!(
                target: LOG_TARGET,
                "d3d9 core: no Metal device has registry ID {registry_id:#x}; the first one renders"
            );
            at(0)
        })
    });
    // SAFETY: the array's element, retained before the array (which holds it) is released.
    let device = picked.and_then(|handle| unsafe { Obj::retain(handle) });
    // SAFETY: the reference `WMTCopyAllDevices` gave.
    unsafe { NSObject_release(devices) };
    device
}

mod buffers;
mod depth;
mod draw;
mod query;
mod recorder;
mod retained;
mod samplers;
mod stretch;
mod surface;
mod swapchain;
mod sync;
mod target;
mod textures;

pub use depth::DepthRef;
pub use draw::{Bind, DrawData, DrawRefused, IndexData, Indices, Stage, StreamData};
pub use query::{Occlusion, OcclusionAnswer};
pub use stretch::StretchQuad;
pub use swapchain::present_after;
pub use target::TargetRef;
pub use textures::{
    CoreTextureDesc, CoreTextureFlags, DepthPlanes, TextureBind, TextureKind, UploadRegion,
    view_format,
};

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;
