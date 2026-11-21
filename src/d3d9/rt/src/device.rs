//! The core device under D3D9: its Metal device and its queue profile.
//!
//! The Metal device is winemetal's (`WMTCopyAllDevices`), the one whose registry ID the adapter
//! `GetDeviceInfo` reported has; the queue takes D3D9's memory profile ([`queue_profile`]).

use core::sync::atomic::Ordering;
#[cfg(test)]
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};

use dxmt_core::{
    device::Device, pipeline_cache::PsoMap, queue::QueueConfig, scheduler::CompileScheduler,
    shader_cache::Store, staging::Staging,
};
use dxmt_util::sync::lock;
use mtld3d_core::{
    async_compile::{ClearHistory, mark_kept_reads_generic},
    pipeline_key::{PipelineKey, PipelineSource},
    shader_prewarm::PrewarmHandle,
};
use winemetal_sys::{
    MTLDevice_registryID, NSArray_count, NSArray_object, NSObject_release, Obj, WMTCopyAllDevices,
};

/// The `log` target of the core runtime's lines.
pub const LOG_TARGET: &str = "mtld3d::d3d9::core";

bitflags::bitflags! {
    /// Per-device runtime options and one-shot diagnostic latches for [`CoreDevice`].
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CoreOptions: u8 {
        /// Open and read/write the shared DXMT SQLite shader cache (`shaderCache.enable`).
        const SHADER_CACHE = 1 << 0;
        /// Compile uncached shaders and pipelines asynchronously (`shader.asyncCompile`).
        const ASYNC_COMPILE = 1 << 1;
        /// Swap chain uses `D3DSWAPEFFECT_DISCARD`, rebuilding the back buffer every frame.
        const DISCARD_BACK_BUFFER = 1 << 2;
        /// One-shot log latch for the first skipped draw on this device.
        const LOGGED_SKIP = 1 << 3;
        /// Enable HDR/EDR output when the display supports it (`color.hdr.enable`).
        const HDR_ENABLE = 1 << 5;
    }
}

/// Target identity tracked in [`CoreDevice`]'s clear and persistent-feed history.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ClearTarget {
    BackBuffer,
    Texture(mtld3d_core::ids::TextureId),
}

const MAX_TRACKED_PASSES: usize = 4096;

struct TrackedPass {
    colors: [(ClearTarget, u32); 4],
    color_present: [bool; 4],
    depth: Option<(ClearTarget, u32)>,
    writes_stencil: bool,
}

#[derive(Default)]
pub(super) struct PassReadTracker {
    passes: Vec<TrackedPass>,
    reads: Vec<(usize, ClearTarget)>,
    active_pass: Option<usize>,
}

impl PassReadTracker {
    fn push_pass(
        &mut self,
        colors: [(ClearTarget, u32); 4],
        color_present: [bool; 4],
        depth: Option<(ClearTarget, u32)>,
    ) {
        if self.passes.len() >= MAX_TRACKED_PASSES {
            self.active_pass = Some(usize::MAX);
            return;
        }
        let idx = self.passes.len();
        self.passes.push(TrackedPass {
            colors,
            color_present,
            depth,
            writes_stencil: false,
        });
        self.active_pass = Some(idx);
    }

    fn mark_writes_stencil(&mut self) {
        if let Some(pass) = self.active_pass
            && let Some(record) = self.passes.get_mut(pass)
        {
            record.writes_stencil = true;
        }
    }

    fn push_read(&mut self, target: ClearTarget) {
        let Some(pass) = self.active_pass else {
            return;
        };
        let already = self
            .reads
            .iter()
            .rev()
            .take_while(|(p, _)| *p == pass)
            .any(|(_, t)| *t == target);
        if !already {
            self.reads.push((pass, target));
        }
    }

    fn clear(&mut self) {
        self.passes.clear();
        self.reads.clear();
        self.active_pass = None;
    }
}

/// Maps a colour [`TargetRef`] to its [`ClearTarget`] key and `slice | (level << 16)` subresource.
pub(super) const fn color_clear_key(target: TargetRef) -> (ClearTarget, u32) {
    match target {
        TargetRef::BackBuffer => (ClearTarget::BackBuffer, 0),
        TargetRef::Texture { id, level, face } => {
            let slice = match face {
                Some(f) => f,
                None => 0,
            };
            (ClearTarget::Texture(id), slice | (level << 16))
        }
    }
}

/// Maps a [`DepthRef`] to its [`ClearTarget`] key and `level << 16` subresource.
pub(super) const fn depth_clear_key(depth: DepthRef) -> (ClearTarget, u32) {
    (ClearTarget::Texture(depth.id), depth.level << 16)
}

/// Shared immutable compilation context passed to worker threads and the prewarm thread.
pub(super) struct CompileContext {
    pub(super) mtl_device: Obj,
    pub(super) metal_version: u32,
    pub(super) functions: Arc<shader_cache::StageFunctions>,
    pub(super) recipes: Arc<Mutex<shader_cache::RecipeRecorder>>,
    pub(super) stats: Arc<shader_cache::CompileStats>,
}

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
    /// Cleared in `Drop` only after prewarm stop and queue drain.
    pipelines: Arc<PsoMap<PipelineKey, Option<PipelineSource>>>,
    /// Small MRU cache over resolved pipelines to avoid hashing and locking `pipelines`.
    pso_memo: draw::PsoMemo,
    /// Background compile scheduler, active when `CoreOptions::ASYNC_COMPILE` is set.
    scheduler: Option<Arc<OnceLock<CompileScheduler>>>,
    /// Shared compilation context (Metal device, SQLite-backed stage functions, recipes, stats).
    compile_ctx: Arc<CompileContext>,
    /// Shared SQLite shader cache store when `CoreOptions::SHADER_CACHE` is enabled.
    cache: Option<Arc<Store>>,
    /// Warm-start pipeline prewarm worker handle, stopped first in `Drop`.
    prewarm: Option<PrewarmHandle>,
    #[cfg(test)]
    prewarm_done: Option<mpsc::Receiver<Option<()>>>,
    /// Per-device runtime options and one-shot latches.
    options: CoreOptions,
    /// Two-frame whole-clear and persistent-feed history across targets and back buffer.
    clear_history: ClearHistory<ClearTarget>,
    /// Per-frame pass attachment and sampled-texture tracker for persistent read propagation.
    pass_reads: PassReadTracker,
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
    /// Monotonic generation bumped whenever colour targets, depth-stencil, or back buffer change.
    target_gen: u64,
    render_open: bool,
    /// The depth-stencil the open render pass has attached; none for a pass without one.
    open_depth: Option<DepthRef>,
    /// The `WMTPixelFormat` each colour slot of the open render pass attached; 0 for none.
    ///
    /// A target's own or its sRGB twin's; all 0 while no render pass is open.
    open_colors: [u32; 4],
    /// Per-pass bound state for deduplicating redundant Metal render commands.
    pass_bound: pass_bound::PassBound,
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
    /// Inline 256-entry `ushort4` gamma lookup table applied by the presenter, or none.
    gamma: Option<Arc<[u16; 1024]>>,
    /// Layer drawable size in pixels, preserved across `resize` and `retarget`.
    drawable_size: (u32, u32),
}

impl CoreDevice {
    /// A core device on `mtl` with `CoreOptions::empty()` (synchronous, no SQLite cache).
    ///
    /// # Panics
    ///
    /// When the queue's threads cannot be spawned.
    #[must_use]
    pub fn new(mtl: Obj, profile: QueueConfig) -> Self {
        Self::with_options(mtl, profile, CoreOptions::empty())
    }

    /// A core device on `mtl` whose queue has `profile`'s sizes and `options` enabled.
    ///
    /// # Panics
    ///
    /// When the queue's threads cannot be spawned.
    #[must_use]
    pub fn with_options(mtl: Obj, profile: QueueConfig, options: CoreOptions) -> Self {
        let chunks = profile.chunks;
        let staging_block = profile.staging_block;
        let mtl_handle = mtl.handle();
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
        let metal_version = device.metal_version();
        let cache = options
            .contains(CoreOptions::SHADER_CACHE)
            .then(|| shader_cache::open_store(metal_version));
        let loaded_recipes = cache.as_deref().map_or_else(Vec::new, |store| {
            shader_cache::load_recipes(store, metal_version)
        });
        let stats = Arc::new(shader_cache::CompileStats::default());
        let functions = Arc::new(shader_cache::StageFunctions::with_cache(
            cache.clone(),
            Arc::clone(&stats),
        ));
        let prewarm_recipes = if options.contains(CoreOptions::SHADER_CACHE)
            && !loaded_recipes.is_empty()
            && cache.as_deref().is_some_and(Store::has_reader)
            && (mtl_handle > 0x1000 || cfg!(test))
        {
            loaded_recipes.clone()
        } else {
            Vec::new()
        };
        let recipes = Arc::new(Mutex::new(shader_cache::RecipeRecorder::seed(
            loaded_recipes,
        )));
        // SAFETY: `device.device()` holds a live Metal device for the lifetime of `device`.
        let retained_mtl = unsafe { Obj::retain(device.device().handle()) }
            .expect("device holds a non-null Metal device handle");
        let compile_ctx = Arc::new(CompileContext {
            mtl_device: retained_mtl,
            metal_version,
            functions,
            recipes,
            stats,
        });
        let pipelines = Arc::new(PsoMap::default());
        let scheduler = options
            .contains(CoreOptions::ASYNC_COMPILE)
            .then(|| Arc::new(OnceLock::new()));

        let mut core = Self {
            device,
            back_buffer: None,
            window: None,
            readback: None,
            pipelines: Arc::clone(&pipelines),
            pso_memo: draw::PsoMemo::default(),
            scheduler,
            compile_ctx: Arc::clone(&compile_ctx),
            cache,
            prewarm: None,
            #[cfg(test)]
            prewarm_done: None,
            options,
            clear_history: ClearHistory::new(),
            pass_reads: PassReadTracker::default(),
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
            target_gen: 0,
            render_open: false,
            open_depth: None,
            open_colors: [0; 4],
            pass_bound: pass_bound::PassBound::new(),
            blit_open: false,
            dirty: false,
            staged_bytes: 0,
            staged_budget: staged_budget(staging_block),
            retained: retained::Retained::default(),
            retention_cap: 0,
            open_brackets: None,
            gamma: None,
            drawable_size: (0, 0),
        };

        if !prewarm_recipes.is_empty() {
            let (handle, rx) = PrewarmHandle::spawn(move |stop| {
                for key in prewarm_recipes {
                    if stop.load(Ordering::Acquire) {
                        return Some(());
                    }
                    if pipelines.get(&key).is_some() {
                        continue;
                    }
                    let _pool = winemetal_sys::Pool::new();
                    let Ok(source) = mtld3d_core::pipeline_key::pipeline_source(&key) else {
                        continue;
                    };
                    if !source.fetchable() {
                        continue;
                    }
                    let Some(pso) = draw::build_pipeline(&compile_ctx, &source) else {
                        continue;
                    };
                    let pso_handle = pso.handle();
                    if pso_handle == 0 {
                        continue;
                    }
                    let entry = dxmt_core::pipeline_cache::Compiled::ready_with(
                        Arc::new(Some(source)),
                        pso_handle,
                        Some(pso),
                    );
                    let kept = pipelines.insert_ready(key, Arc::clone(&entry));
                    if Arc::ptr_eq(&kept, &entry) {
                        compile_ctx
                            .stats
                            .prewarmed_psos
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
                Some(())
            });
            core.prewarm = Some(handle);
            #[cfg(test)]
            {
                core.prewarm_done = Some(rx);
            }
            #[cfg(not(test))]
            drop(rx);
        }

        core
    }

    /// Records whether the swap chain discards its back buffer (`D3DSWAPEFFECT_DISCARD`).
    pub fn set_discard_back_buffer(&mut self, discard: bool) {
        self.options.set(CoreOptions::DISCARD_BACK_BUFFER, discard);
    }

    /// Clears the frame clear and pass-read histories on `Reset` / `ResetEx`.
    pub fn reset_clear_history(&mut self) {
        self.clear_history.clear();
        self.pass_reads.clear();
    }

    /// Accounts an application-level colour `StretchRect` or resolve for async draw skipping.
    pub fn note_stretch_copy(
        &mut self,
        src: TargetRef,
        dst: TargetRef,
        planes: ClearPlanes,
        whole_dst: bool,
    ) {
        if !self.options.contains(CoreOptions::ASYNC_COMPILE) {
            return;
        }
        let (src_key, _) = color_clear_key(src);
        let (dst_key, dst_sub) = color_clear_key(dst);
        self.note_copy_keys(src_key, dst_key, dst_sub, planes, whole_dst);
    }

    /// Accounts an application depth-stencil `StretchRect` or RESZ copy for async draw skipping.
    pub fn note_stretch_depth_copy(&mut self, src: DepthRef, dst: DepthRef) {
        if !self.options.contains(CoreOptions::ASYNC_COMPILE) {
            return;
        }
        let Some((_, _, pixel_format, _)) = self.depth_level(dst.id, dst.level) else {
            return;
        };
        let mask = depth::planes(pixel_format);
        let mut planes = ClearPlanes::empty();
        if mask & 1 != 0 {
            planes.insert(ClearPlanes::DEPTH);
        }
        if mask & 2 != 0 {
            planes.insert(ClearPlanes::STENCIL);
        }
        if planes.is_empty() {
            return;
        }
        let (src_key, _) = depth_clear_key(src);
        let (dst_key, dst_sub) = depth_clear_key(dst);
        self.note_copy_keys(src_key, dst_key, dst_sub, planes, true);
    }

    fn note_copy_keys(
        &mut self,
        src_key: ClearTarget,
        dst_key: ClearTarget,
        dst_sub: u32,
        planes: ClearPlanes,
        whole_dst: bool,
    ) {
        if whole_dst {
            self.clear_history.record_key(dst_key, dst_sub, planes);
        }
        let discard_bb = self.options.contains(CoreOptions::DISCARD_BACK_BUFFER);
        let dst_rebuilt = !self.clear_history.feeds_persistent(dst_key)
            && planes.iter().all(|plane| {
                if plane == ClearPlanes::COLOR {
                    (dst_key == ClearTarget::BackBuffer && discard_bb)
                        || self.clear_history.regenerated(dst_key, dst_sub, plane)
                } else {
                    self.clear_history.regenerated(dst_key, dst_sub, plane)
                }
            });
        if !dst_rebuilt {
            let _ = self.clear_history.mark_feeds_persistent_key(src_key);
        }
    }

    /// Evaluates kept-pass texture reads at the end of a frame and advances clear history.
    pub(super) fn end_frame_clear_tracking(&mut self) {
        if self.options.contains(CoreOptions::ASYNC_COMPILE) {
            let discard_bb = self.options.contains(CoreOptions::DISCARD_BACK_BUFFER);
            let passes = &self.pass_reads.passes;
            let reads = &self.pass_reads.reads;
            mark_kept_reads_generic(reads, &mut self.clear_history, |history, pass_idx| {
                let Some(pass) = passes.get(pass_idx) else {
                    return true;
                };
                let color_rebuilt = |key: ClearTarget, sub: u32| {
                    ((key == ClearTarget::BackBuffer && discard_bb)
                        || history.regenerated(key, sub, ClearPlanes::COLOR))
                        && !history.feeds_persistent(key)
                };
                for slot in 0..4 {
                    if pass.color_present[slot]
                        && !color_rebuilt(pass.colors[slot].0, pass.colors[slot].1)
                    {
                        return true;
                    }
                }
                if let Some((depth_key, depth_sub)) = pass.depth {
                    let depth_rebuilt =
                        history.regenerated(depth_key, depth_sub, ClearPlanes::DEPTH)
                            && !history.feeds_persistent(depth_key);
                    let stencil_rebuilt = !pass.writes_stencil
                        || (history.regenerated(depth_key, depth_sub, ClearPlanes::STENCIL)
                            && !history.feeds_persistent(depth_key));
                    if !depth_rebuilt || !stencil_rebuilt {
                        return true;
                    }
                }
                false
            });
        }
        self.pass_reads.clear();
        self.clear_history.begin_frame();
    }

    /// Returns whether every attached color target and active depth/stencil plane was rebuilt.
    pub(super) fn targets_rebuilt(&self, colors: [u32; 4], planes: ClearPlanes) -> bool {
        let discard_bb = self.options.contains(CoreOptions::DISCARD_BACK_BUFFER);
        let set = self.pass_set();
        for slot in 0..4 {
            if colors[slot] == 0 {
                continue;
            }
            if set[slot].is_none() {
                return false;
            }
            let Some(target) = self.slot_target(slot) else {
                return false;
            };
            let (key, sub) = color_clear_key(target);
            let rebuilt = ((key == ClearTarget::BackBuffer && discard_bb)
                || self.clear_history.regenerated(key, sub, ClearPlanes::COLOR))
                && !self.clear_history.feeds_persistent(key);
            if !rebuilt {
                return false;
            }
        }
        if planes.intersects(ClearPlanes::DEPTH | ClearPlanes::STENCIL) {
            let Some(depth_ref) = self.depth else {
                return false;
            };
            let (depth_key, depth_sub) = depth_clear_key(depth_ref);
            if self.clear_history.feeds_persistent(depth_key) {
                return false;
            }
            if planes.contains(ClearPlanes::DEPTH)
                && !self
                    .clear_history
                    .regenerated(depth_key, depth_sub, ClearPlanes::DEPTH)
            {
                return false;
            }
            if planes.contains(ClearPlanes::STENCIL)
                && !self
                    .clear_history
                    .regenerated(depth_key, depth_sub, ClearPlanes::STENCIL)
            {
                return false;
            }
        }
        true
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

    /// Monotonic generation bumped on every colour-target, depth-stencil, or back-buffer change.
    #[must_use]
    pub const fn target_gen(&self) -> u64 {
        self.target_gen
    }

    /// Whether any finished Metal command buffer on this device's queue failed.
    #[must_use]
    pub fn has_error(&self) -> bool {
        self.device.queue().has_error()
    }

    /// Waits for the warm-start prewarm thread to complete in unit tests.
    #[cfg(test)]
    pub fn wait_prewarm(&mut self) {
        if let Some(rx) = self.prewarm_done.take() {
            let _ = rx.recv();
        }
        if let Some(mut prewarm) = self.prewarm.take() {
            prewarm.cancel_and_join();
        }
    }
}

impl Drop for CoreDevice {
    fn drop(&mut self) {
        if let Some(mut prewarm) = self.prewarm.take() {
            prewarm.cancel_and_join();
        }
        self.drain();
        // A bracket the front end never ended counted on every pass since its BEGIN.
        if let Some(open @ 1..) = self.open_brackets {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: {open} occlusion queries' END was not recorded"
            );
        }
        self.pso_memo.clear();
        self.pipelines.clear();
        if let Some(sched_arc) = self.scheduler.take() {
            if let Some(s) = sched_arc.get() {
                s.cancel_unstarted();
            }
            drop(sched_arc);
        }
        let (recipes_loaded, recipes_saved) = {
            let mut recipes = lock(&self.compile_ctx.recipes);
            let loaded = recipes.recipes_loaded();
            let saved = self.cache.as_deref().map_or(0, |store| {
                recipes.flush(store, self.compile_ctx.metal_version)
            });
            drop(recipes);
            (loaded, saved)
        };
        if self
            .options
            .intersects(CoreOptions::SHADER_CACHE | CoreOptions::ASYNC_COMPILE)
        {
            let stats = &self.compile_ctx.stats;
            log::log!(
                target: LOG_TARGET,
                gate_level(),
                "d3d9 core: shader cache summary: hits={} misses={} compiles={} async={} skipped={} \
                 recipes_loaded={} recipes_saved={} prewarmed={}",
                stats.cache_hits.load(Ordering::Relaxed),
                stats.cache_misses.load(Ordering::Relaxed),
                stats.stage_compiles.load(Ordering::Relaxed),
                stats.async_compiled.load(Ordering::Relaxed),
                stats.draws_skipped.load(Ordering::Relaxed),
                recipes_loaded,
                recipes_saved,
                stats.prewarmed_psos.load(Ordering::Relaxed),
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
mod pass_bound;
mod query;
mod recorder;
mod retained;
mod samplers;
mod shader_cache;
mod stretch;
mod surface;
mod swapchain;
mod sync;
mod target;
mod textures;

pub use depth::DepthRef;
pub use draw::{Bind, DrawData, DrawRefused, IndexData, Indices, Stage, StreamData};
pub use mtld3d_core::async_compile::ClearPlanes;
pub use query::{Occlusion, OcclusionAnswer};
pub use shader_cache::close_shader_cache_on_detach;
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
