//! The core device under D3D9: its Metal device and its queue profile.
//!
//! The Metal device is winemetal's (`WMTCopyAllDevices`), the one whose registry ID the adapter
//! `GetDeviceInfo` reported has; the queue takes D3D9's memory profile ([`queue_profile`]).

use std::sync::Arc;

use dxmt_core::{
    device::Device,
    pipeline_cache::PsoMap,
    queue::QueueConfig,
    resource::{Texture, TextureViewKey},
    staging::Staging,
};
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
/// stage functions and the depth-stencil states, whose raw handles the closures carry.
pub struct CoreDevice {
    device: Device,
    back_buffer: Option<swapchain::BackBuffer>,
    window: Option<swapchain::Window>,
    /// The read-back destination and its row's bytes, kept until a read-back needs another.
    readback: Option<(Arc<Staging>, u32)>,
    /// The pipelines by key, each with its source (none when a stage does not parse).
    ///
    /// Handle 0 for one that did not build or was not built (its layout crosses the stride).
    /// Never cleared: a recorded closure names its pipeline by handle until the drain.
    pipelines: PsoMap<PipelineKey, Option<PipelineSource>>,
    /// The compiled stage functions the pipelines are made of.
    functions: draw::StageFunctions,
    /// The D3D9 textures' core textures, by id (C4b-3b).
    textures: rustc_hash::FxHashMap<mtld3d_core::ids::TextureId, textures::Stored>,
    /// The samplers by their key's bits (C4b-3b).
    samplers: rustc_hash::FxHashMap<u64, Arc<dxmt_core::sampler::Sampler>>,
    /// The depth-stencil states by their key's bits (C4b-3d2).
    ///
    /// Never cleared, as the pipelines: a recorded closure names its state by handle.
    depth_states: rustc_hash::FxHashMap<u64, Obj>,
    /// The colour target draws and clears go to (C4b-3d1).
    target: TargetRef,
    /// The depth-stencil draws test against and clears go to (C4b-3d2).
    depth: Option<DepthRef>,
    render_open: bool,
    /// The depth-stencil the open render pass has attached; none for a pass without one.
    open_depth: Option<DepthRef>,
    /// The scissor the open render pass last recorded; none since it began (DXMTRS C4b-3d3).
    pass_scissor: Option<[u32; 4]>,
    /// The last closure left a blit pass open (consecutive uploads share it).
    blit_open: bool,
    dirty: bool,
    /// Bytes staged for uploads in the chunk being recorded, and the budget past which it commits.
    staged_bytes: u64,
    staged_budget: u64,
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
        // A perf build compiles out what is below a warning; the gates read this line there too.
        let level = if cfg!(feature = "perf-build") {
            log::Level::Warn
        } else {
            log::Level::Info
        };
        log::log!(target: LOG_TARGET, level, "d3d9 core: device on dxmt-core ({chunks} chunks)");
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
            samplers: rustc_hash::FxHashMap::default(),
            depth_states: rustc_hash::FxHashMap::default(),
            target: TargetRef::BackBuffer,
            depth: None,
            render_open: false,
            open_depth: None,
            pass_scissor: None,
            blit_open: false,
            dirty: false,
            staged_bytes: 0,
            staged_budget: staged_budget(staging_block),
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
    }
}

/// The back buffer's texture and the view a pass draws to or clears.
fn back_buffer_view(back_buffer: &swapchain::BackBuffer) -> (Arc<Texture>, TextureViewKey) {
    (Arc::clone(&back_buffer.texture), back_buffer.key)
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

mod depth;
mod draw;
mod recorder;
mod samplers;
mod surface;
mod swapchain;
mod sync;
mod target;
mod textures;

pub use depth::DepthRef;
pub use draw::{Bind, DrawData, DrawRefused, Indices, Stage};
pub use swapchain::present_after;
pub use target::TargetRef;
pub use textures::{CoreTextureDesc, TextureBind, TextureKind, UploadRegion, view_format};

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;
