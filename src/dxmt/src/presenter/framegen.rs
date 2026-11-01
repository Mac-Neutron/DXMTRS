//! Frame generation and pacing on `MetalFX` (`d3d12_command_queue.cpp:82-107, 2160-2476`, P11).
//!
//! Holds the process-wide slot of the most recent temporal upscale ([`record_upscale`]), the
//! adaptive decision, the three-frame ring, and the background pacing thread ([`FrameGen`]).

use core::ptr::{from_ref, null};
use std::{
    collections::VecDeque,
    io::Write as _,
    sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard, OnceLock, PoisonError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use dxmt_util::float_cast::{f64_to_f32, u32_to_f32};
pub use winemetal_sys::wire::{
    WMTFXTemporalScalerInfo, WMTFXTemporalScalerProps,
    WMTFXTemporalScalerProps as WmtFxTemporalScalerProps,
};
use winemetal_sys::{
    MTLBlitCommandEncoder_encodeCommands, MTLCommandBuffer_blitCommandEncoder,
    MTLCommandBuffer_commit, MTLCommandBuffer_encodeFrameInterpolate,
    MTLCommandBuffer_encodeSignalEvent, MTLCommandBuffer_encodeWaitForEvent,
    MTLCommandBuffer_presentDrawable, MTLCommandEncoder_endEncoding, MTLCommandQueue_commandBuffer,
    MTLDevice_newCommandQueue, MTLDevice_newEvent, MTLDevice_newFence,
    MTLDevice_newFrameInterpolator, MTLDevice_newTexture, MTLTexture_height,
    MTLTexture_pixelFormat, MTLTexture_width, Obj, Pool,
    wire::{
        WMT_BLIT_COMMAND_COPY_TEXTURE, WMT_BLIT_COMMAND_UPDATE_FENCE,
        WMT_BLIT_COMMAND_WAIT_FOR_FENCE, WMT_PIXEL_FORMAT_INVALID,
        WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_RENDER_TARGET, WMT_TEXTURE_USAGE_SHADER_READ,
        WMT_TEXTURE_USAGE_SHADER_WRITE, WMTFXFrameInterpolatorInfo, WMTFXFrameInterpolatorProps,
        WMTTextureInfo, WmtcmdBase, WmtcmdBlitCopyTexture, WmtcmdBlitFenceOp,
    },
};

pub use super::{DXMTPresentMetadata as PresentMetadata, Presenter};
use crate::scaler::Scaler;

const RING: usize = 3;
const STACK: usize = 1024 * 1024;
const PACER_THREAD: &str = "dxmt-fg-pacer";

#[cfg(not(feature = "perf-build"))]
macro_rules! scope {
    ($name:literal) => {
        let _dxmt_stat_scope = if crate::stats::on() {
            static ID: ::std::sync::LazyLock<crate::stats::StatId> =
                ::std::sync::LazyLock::new(|| crate::stats::id($name));
            Some(crate::stats::Scope::new(*ID))
        } else {
            None
        };
    };
}

#[cfg(feature = "perf-build")]
macro_rules! scope {
    ($name:literal) => {};
}

#[cfg(not(feature = "perf-build"))]
macro_rules! count {
    ($name:literal, $delta:expr) => {{
        let delta: u64 = $delta;
        if crate::stats::on() {
            static ID: ::std::sync::LazyLock<crate::stats::StatId> =
                ::std::sync::LazyLock::new(|| crate::stats::id($name));
            crate::stats::count(*ID, delta);
        }
    }};
}

#[cfg(feature = "perf-build")]
macro_rules! count {
    ($name:literal, $delta:expr) => {{
        let _: u64 = $delta;
    }};
}

fn retain(obj: &Obj) -> Obj {
    // SAFETY: `obj` is a live winemetal reference.
    unsafe { Obj::retain(obj.handle()) }.expect("retaining a live Obj answers Some")
}

/// A `MetalFX` temporal scaler paired with its [`WMTFXTemporalScalerInfo`] (`dxmt_scaler.hpp`).
pub struct TemporalScaler {
    pub scaler: Scaler,
    pub info: WMTFXTemporalScalerInfo,
}

impl TemporalScaler {
    /// Creates a temporal scaler and its fence on `device` for `info`.
    ///
    /// # Safety
    ///
    /// `device` must be a live `MTLDevice` handle.
    #[must_use]
    pub unsafe fn new(device: u64, info: WMTFXTemporalScalerInfo) -> Self {
        // SAFETY: `device` is a live `MTLDevice` and `info` is valid for the call.
        let scaler = unsafe { Scaler::temporal(device, &info) };
        Self { scaler, info }
    }

    /// The `MTLFXTemporalScaler` handle, or `None` when Metal made none.
    #[must_use]
    pub const fn scaler(&self) -> Option<&Obj> {
        self.scaler.scaler()
    }

    /// The scaler's `MTLFence` handle, or `None` when Metal made none.
    #[must_use]
    pub const fn fence(&self) -> Option<&Obj> {
        self.scaler.fence()
    }

    /// The description this scaler was created for.
    #[must_use]
    pub const fn info(&self) -> &WMTFXTemporalScalerInfo {
        &self.info
    }
}

/// Frame generation mode (`MACNEUTRON_FRAMEGEN`, `d3d12_command_queue.cpp:98-107`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Frame generation disabled (default).
    Off,
    /// Adaptive frame generation (`MACNEUTRON_FRAMEGEN=1`).
    Adaptive,
    /// Unconditional frame generation (`MACNEUTRON_FRAMEGEN=always`).
    Always,
}

fn parse_mode(val: Option<&str>) -> Mode {
    match val {
        Some("1") => Mode::Adaptive,
        Some("always") => Mode::Always,
        _ => Mode::Off,
    }
}

/// Reads the process-wide [`Mode`] from `MACNEUTRON_FRAMEGEN`.
#[must_use]
pub fn mode() -> Mode {
    static MODE: LazyLock<Mode> = LazyLock::new(|| {
        let val = std::env::var("MACNEUTRON_FRAMEGEN").ok();
        parse_mode(val.as_deref())
    });
    *MODE
}

struct RecordedScaler {
    handle: Obj,
    info: WMTFXTemporalScalerInfo,
}

impl Clone for RecordedScaler {
    fn clone(&self) -> Self {
        Self {
            handle: retain(&self.handle),
            info: self.info,
        }
    }
}

const ZERO_PROPS: WmtFxTemporalScalerProps = WmtFxTemporalScalerProps {
    input_content_width: 0,
    input_content_height: 0,
    reset: 0,
    depth_reversed: 0,
    motion_vector_scale_x: 0.0,
    motion_vector_scale_y: 0.0,
    jitter_offset_x: 0.0,
    jitter_offset_y: 0.0,
    pre_exposure: 0.0,
    color_content_offset_x: 0,
    color_content_offset_y: 0,
    depth_content_offset_x: 0,
    depth_content_offset_y: 0,
    motion_content_offset_x: 0,
    motion_content_offset_y: 0,
    reactive_mask_content_offset_x: 0,
    reactive_mask_content_offset_y: 0,
    output_offset_x: 0,
    output_offset_y: 0,
};

struct Slot {
    scaler: Option<RecordedScaler>,
    depth: Option<Obj>,
    motion: Option<Obj>,
    props: WmtFxTemporalScalerProps,
    fresh: bool,
}

static SLOT: Mutex<Slot> = Mutex::new(Slot {
    scaler: None,
    depth: None,
    motion: None,
    props: ZERO_PROPS,
    fresh: false,
});

/// Records the most recent temporal upscale into the process-wide frame-generation slot
///
/// (`d3d12_command_queue.cpp:2001-2008`).
pub fn record_upscale(
    scaler: &TemporalScaler,
    depth: &Obj,
    motion: &Obj,
    props: &WmtFxTemporalScalerProps,
) {
    let mut slot = SLOT.lock().unwrap_or_else(PoisonError::into_inner);
    slot.scaler = scaler.scaler().map(|s| RecordedScaler {
        handle: retain(s),
        info: *scaler.info(),
    });
    slot.depth = Some(retain(depth));
    slot.motion = Some(retain(motion));
    slot.props = *props;
    slot.fresh = true;
}

fn take_slot() -> (
    bool,
    Option<RecordedScaler>,
    Option<Obj>,
    Option<Obj>,
    WmtFxTemporalScalerProps,
) {
    let mut slot = SLOT.lock().unwrap_or_else(PoisonError::into_inner);
    let fresh = slot.fresh;
    slot.fresh = false;
    (
        fresh,
        slot.scaler.clone(),
        slot.depth.as_ref().map(retain),
        slot.motion.as_ref().map(retain),
        slot.props,
    )
}

/// Fence-ring ordering callbacks supplied by the command queue
///
/// (`d3d12_command_queue.cpp:2430-2466`).
pub trait Order {
    /// Encodes the queue's frontier join (`Order(nullptr, true)`, `waits_`, `ScratchFill`) into
    ///
    /// the `begin` blit command encoder before `updateFence(fg.fence)`.
    fn wait(&mut self, begin: &Obj);

    /// Encodes `updateFence(fences_[done])` into the `end` copy blit command encoder.
    fn update(&mut self, end: &Obj);
}

struct FramePair {
    index: u64,
    generated: Option<Obj>,
    real: Obj,
    presenter: Arc<Presenter>,
    metadata: PresentMetadata,
}

struct PacerState {
    pending: VecDeque<FramePair>,
    enqueued: u64,
    committed: u64,
    started: bool,
    stop: bool,
    pair: f64,
}

struct Shared {
    mutex: Mutex<PacerState>,
    cv: Condvar,
}

impl Shared {
    const fn new() -> Self {
        Self {
            mutex: Mutex::new(PacerState {
                pending: VecDeque::new(),
                enqueued: 0,
                committed: 0,
                started: false,
                stop: false,
                pair: 0.0,
            }),
            cv: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, PacerState> {
        self.mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct InterpolateArgs<'a> {
    cmdbuf: &'a Obj,
    interpolator: &'a Obj,
    color: &'a Obj,
    prev: &'a Obj,
    depth: &'a Obj,
    motion: &'a Obj,
    output: &'a Obj,
    fence: &'a Obj,
    props: &'a WMTFXFrameInterpolatorProps,
}

struct PresentArgs<'a> {
    now: Instant,
    cmdbuf: &'a Obj,
    presenter: Option<&'a Arc<Presenter>>,
    device: u64,
    frame: &'a Obj,
    metadata: &'a PresentMetadata,
    refresh: f64,
}

struct EnqueueArgs<'a> {
    n: u64,
    real: &'a Obj,
    gen_tex: Option<&'a Obj>,
    presenter: Option<&'a Arc<Presenter>>,
    metadata: &'a PresentMetadata,
}

trait Backend {
    fn texture_size_format(&mut self, tex: &Obj) -> (u64, u64, u32);
    fn new_texture(
        &mut self,
        device: u64,
        info: &mut WMTTextureInfo,
        generated: bool,
        slot: usize,
    ) -> Obj;
    fn new_command_queue(&mut self, device: u64, max_count: u64) -> Obj;
    fn new_event(&mut self, device: u64, is_written: bool) -> Obj;
    fn new_fence(&mut self, device: u64) -> Obj;
    fn new_frame_interpolator(
        &mut self,
        device: u64,
        info: &WMTFXFrameInterpolatorInfo,
    ) -> Option<Obj>;
    fn spawn_pacer(&mut self, fg: &mut FrameGen);
    fn wait_for_pacer(&mut self, fg: &FrameGen, n: u64) -> f64;
    fn blit_encoder(&mut self, cmdbuf: &Obj, is_begin: bool) -> Obj;
    fn blit_wait_fence(&mut self, enc: &Obj, fence: &Obj);
    fn blit_update_fence(&mut self, enc: &Obj, fence: &Obj);
    fn blit_copy_texture(&mut self, enc: &Obj, src: &Obj, dst: &Obj);
    fn end_encoding(&mut self, enc: &Obj);
    fn wait_event(&mut self, cmdbuf: &Obj, event: &Obj, value: u64);
    fn signal_event(&mut self, cmdbuf: &Obj, event: &Obj, value: u64);
    fn encode_interpolate(&mut self, args: &InterpolateArgs<'_>);
    fn enqueue(&mut self, fg: &FrameGen, args: &EnqueueArgs<'_>);
}

struct ProdBackend;

impl Backend for ProdBackend {
    fn texture_size_format(&mut self, tex: &Obj) -> (u64, u64, u32) {
        let h = tex.handle();
        // SAFETY: `tex` is a live `MTLTexture`.
        let width = unsafe { MTLTexture_width(h) };
        // SAFETY: `tex` is a live `MTLTexture`.
        let height = unsafe { MTLTexture_height(h) };
        // SAFETY: `tex` is a live `MTLTexture`.
        let format = unsafe { MTLTexture_pixelFormat(h) };
        (width, height, format)
    }

    fn new_texture(
        &mut self,
        device: u64,
        info: &mut WMTTextureInfo,
        _generated: bool,
        _slot: usize,
    ) -> Obj {
        // SAFETY: `device` is a live `MTLDevice` and `info` is initialized.
        let handle = unsafe { MTLDevice_newTexture(device, info) };
        // SAFETY: `newTexture` returns a +1 owned handle.
        unsafe { Obj::adopt(handle) }.expect("MTLDevice_newTexture failed")
    }

    fn new_command_queue(&mut self, device: u64, max_count: u64) -> Obj {
        // SAFETY: `device` is a live `MTLDevice`.
        let handle = unsafe { MTLDevice_newCommandQueue(device, max_count) };
        // SAFETY: `newCommandQueue` returns a +1 owned handle.
        unsafe { Obj::adopt(handle) }.expect("MTLDevice_newCommandQueue failed")
    }

    fn new_event(&mut self, device: u64, _is_written: bool) -> Obj {
        // SAFETY: `device` is a live `MTLDevice`.
        let handle = unsafe { MTLDevice_newEvent(device) };
        // SAFETY: `newEvent` returns a +1 owned handle.
        unsafe { Obj::adopt(handle) }.expect("MTLDevice_newEvent failed")
    }

    fn new_fence(&mut self, device: u64) -> Obj {
        // SAFETY: `device` is a live `MTLDevice`.
        let handle = unsafe { MTLDevice_newFence(device) };
        // SAFETY: `newFence` returns a +1 owned handle.
        unsafe { Obj::adopt(handle) }.expect("MTLDevice_newFence failed")
    }

    fn new_frame_interpolator(
        &mut self,
        device: u64,
        info: &WMTFXFrameInterpolatorInfo,
    ) -> Option<Obj> {
        // SAFETY: `device` is a live `MTLDevice` and `info` points to a valid descriptor.
        let handle = unsafe { MTLDevice_newFrameInterpolator(device, info) };
        // SAFETY: `newFrameInterpolator` returns a +1 owned handle or 0 on failure.
        unsafe { Obj::adopt(handle) }
    }

    fn spawn_pacer(&mut self, fg: &mut FrameGen) {
        let queue = retain(fg.queue.as_ref().expect("queue created before pacer"));
        let written = retain(fg.written.as_ref().expect("written event created"));
        let read = retain(fg.read.as_ref().expect("read event created"));
        let shared = Arc::clone(fg.shared());
        shared.lock().started = true;
        let handle = thread::Builder::new()
            .name(PACER_THREAD.to_owned())
            .stack_size(STACK)
            .spawn(move || {
                let _guard = crate::ffi::AbortOnPanic;
                pacing_thread(&shared, &queue, &written, &read);
            })
            .expect("failed to spawn dxmt-fg-pacer thread");
        fg.thread = Some(handle);
    }

    fn wait_for_pacer(&mut self, fg: &FrameGen, n: u64) -> f64 {
        let start = Instant::now();
        let shared = fg.shared();
        let mut state = shared.lock();
        let ring_u64 = RING as u64;
        while state.committed + ring_u64 - 1 <= n {
            state = shared
                .cv
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        drop(state);
        start.elapsed().as_secs_f64()
    }

    fn blit_encoder(&mut self, cmdbuf: &Obj, _is_begin: bool) -> Obj {
        // SAFETY: `cmdbuf` is a live `MTLCommandBuffer`.
        let handle = unsafe { MTLCommandBuffer_blitCommandEncoder(cmdbuf.handle()) };
        // SAFETY: `blitCommandEncoder` returns an autoreleased encoder; retaining gives `Obj` its
        // own reference so dropping it does not consume the pool's reference early.
        unsafe { Obj::retain(handle) }.expect("MTLCommandBuffer_blitCommandEncoder failed")
    }

    fn blit_wait_fence(&mut self, enc: &Obj, fence: &Obj) {
        let cmd = WmtcmdBlitFenceOp {
            r#type: WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
            reserved: [0; 3],
            next: super::mem(null::<u8>()),
            fence: fence.handle(),
        };
        // SAFETY: `enc` is a live `MTLBlitCommandEncoder` and `cmd` is a single-node command list.
        unsafe {
            MTLBlitCommandEncoder_encodeCommands(enc.handle(), from_ref(&cmd).cast::<WmtcmdBase>());
        }
    }

    fn blit_update_fence(&mut self, enc: &Obj, fence: &Obj) {
        let cmd = WmtcmdBlitFenceOp {
            r#type: WMT_BLIT_COMMAND_UPDATE_FENCE,
            reserved: [0; 3],
            next: super::mem(null::<u8>()),
            fence: fence.handle(),
        };
        // SAFETY: `enc` is a live `MTLBlitCommandEncoder` and `cmd` is a single-node command list.
        unsafe {
            MTLBlitCommandEncoder_encodeCommands(enc.handle(), from_ref(&cmd).cast::<WmtcmdBase>());
        }
    }

    fn blit_copy_texture(&mut self, enc: &Obj, src: &Obj, dst: &Obj) {
        let copy = WmtcmdBlitCopyTexture {
            r#type: WMT_BLIT_COMMAND_COPY_TEXTURE,
            reserved: [0; 3],
            next: super::mem(null::<u8>()),
            src: src.handle(),
            dst: dst.handle(),
        };
        // SAFETY: `enc` is a live `MTLBlitCommandEncoder` and `copy` is a single-node command list.
        unsafe {
            MTLBlitCommandEncoder_encodeCommands(
                enc.handle(),
                from_ref(&copy).cast::<WmtcmdBase>(),
            );
        }
    }

    fn end_encoding(&mut self, enc: &Obj) {
        // SAFETY: `enc` is an active `MTLBlitCommandEncoder`.
        unsafe { MTLCommandEncoder_endEncoding(enc.handle()) };
    }

    fn wait_event(&mut self, cmdbuf: &Obj, event: &Obj, value: u64) {
        // SAFETY: `cmdbuf` is a live `MTLCommandBuffer` and `event` is a live `MTLEvent`.
        unsafe { MTLCommandBuffer_encodeWaitForEvent(cmdbuf.handle(), event.handle(), value) };
    }

    fn signal_event(&mut self, cmdbuf: &Obj, event: &Obj, value: u64) {
        // SAFETY: `cmdbuf` is a live `MTLCommandBuffer` and `event` is a live `MTLEvent`.
        unsafe { MTLCommandBuffer_encodeSignalEvent(cmdbuf.handle(), event.handle(), value) };
    }

    fn encode_interpolate(&mut self, args: &InterpolateArgs<'_>) {
        // SAFETY: all object handles in `args` are live Metal/MetalFX objects and `args.props` is a
        // valid `WMTFXFrameInterpolatorProps`.
        unsafe {
            MTLCommandBuffer_encodeFrameInterpolate(
                args.cmdbuf.handle(),
                args.interpolator.handle(),
                args.color.handle(),
                args.prev.handle(),
                args.depth.handle(),
                args.motion.handle(),
                args.output.handle(),
                args.fence.handle(),
                args.props,
            );
        }
    }

    fn enqueue(&mut self, fg: &FrameGen, args: &EnqueueArgs<'_>) {
        let presenter = args.presenter.expect("presenter required in production");
        let mut state = fg.shared().lock();
        state.pending.push_back(FramePair {
            index: args.n,
            generated: args.gen_tex.map(retain),
            real: retain(args.real),
            presenter: Arc::clone(presenter),
            metadata: PresentMetadata {
                edr_scale: args.metadata.edr_scale,
                max_content_luminance: args.metadata.max_content_luminance,
                max_display_luminance: args.metadata.max_display_luminance,
            },
        });
        state.enqueued = args.n + 1;
    }
}

fn present_drawable(cmdbuf: u64, drawable: Option<&Obj>) {
    let drawable_handle = drawable.map_or(0, Obj::handle);
    // SAFETY: `cmdbuf` is a live `MTLCommandBuffer` and `drawable_handle` is a drawable or null.
    unsafe { MTLCommandBuffer_presentDrawable(cmdbuf, drawable_handle) };
}

fn pacing_thread(shared: &Shared, queue: &Obj, written: &Obj, read: &Obj) {
    loop {
        let pair = {
            let mut state = shared.lock();
            while !state.stop && state.pending.is_empty() {
                state = shared
                    .cv
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            let Some(pair) = state.pending.pop_front() else {
                return;
            };
            drop(state);
            pair
        };
        let _pool = Pool::new();
        let start = Instant::now();
        // SAFETY: `queue` is a live `MTLCommandQueue`.
        let cmdbuf = unsafe { MTLCommandQueue_commandBuffer(queue.handle()) };
        // SAFETY: `cmdbuf` and `written` are live Metal objects.
        unsafe { MTLCommandBuffer_encodeWaitForEvent(cmdbuf, written.handle(), pair.index + 1) };
        let had_generated = if let Some(ref gen_tex) = pair.generated {
            scope!("pacer.(generated frame: drawable and blit)");
            // SAFETY: `cmdbuf` and `gen_tex` are live.
            let drawable = unsafe {
                pair.presenter
                    .encode(cmdbuf, gen_tex.handle(), &pair.metadata, |_| {}, |_| {})
            };
            present_drawable(cmdbuf, drawable.as_ref());
            true
        } else {
            false
        };
        let drawable = {
            scope!("pacer.(real frame: drawable and blit)");
            // SAFETY: `cmdbuf` and `pair.real` are live.
            unsafe {
                pair.presenter
                    .encode(cmdbuf, pair.real.handle(), &pair.metadata, |_| {}, |_| {})
            }
        };
        present_drawable(cmdbuf, drawable.as_ref());
        // SAFETY: `cmdbuf` and `read` are live Metal objects.
        unsafe { MTLCommandBuffer_encodeSignalEvent(cmdbuf, read.handle(), pair.index + 1) };
        // SAFETY: `cmdbuf` has been encoded and is ready to commit.
        unsafe { MTLCommandBuffer_commit(cmdbuf) };
        {
            let mut state = shared.lock();
            state.committed = pair.index + 1;
            if had_generated {
                state.pair = start.elapsed().as_secs_f64().mul_add(0.1, state.pair * 0.9);
            }
        }
        shared.cv.notify_all();
    }
}

const ZERO_INTERP_INFO: WMTFXFrameInterpolatorInfo = WMTFXFrameInterpolatorInfo {
    color_format: 0,
    output_format: 0,
    depth_format: 0,
    motion_format: 0,
    input_width: 0,
    input_height: 0,
    output_width: 0,
    output_height: 0,
    scaler: 0,
};

const fn same_interpolator_info(
    a: &WMTFXFrameInterpolatorInfo,
    b: &WMTFXFrameInterpolatorInfo,
) -> bool {
    a.color_format == b.color_format
        && a.output_format == b.output_format
        && a.depth_format == b.depth_format
        && a.motion_format == b.motion_format
        && a.input_width == b.input_width
        && a.input_height == b.input_height
        && a.output_width == b.output_width
        && a.output_height == b.output_height
        && a.scaler == b.scaler
}

fn ring_texture_info(width: u64, height: u64, pixel_format: u32) -> WMTTextureInfo {
    let usage = WMT_TEXTURE_USAGE_SHADER_READ
        | WMT_TEXTURE_USAGE_SHADER_WRITE
        | WMT_TEXTURE_USAGE_RENDER_TARGET;
    let bits = WMT_TEXTURE_TYPE2D | (1 << 8) | (1 << 16) | (usage << 24);
    let options = WMT_RESOURCE_STORAGE_MODE_PRIVATE | WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED;
    WMTTextureInfo {
        width: u32::try_from(width).unwrap_or(u32::MAX),
        height: u32::try_from(height).unwrap_or(u32::MAX),
        depth: 1,
        array_length: 1,
        pixel_format,
        bits,
        options,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    }
}

struct Adaptive {
    on: bool,
    since: Option<Instant>,
    holding_since: Option<Instant>,
    learned_at: Option<Instant>,
    wait: f64,
    learned_cap: f64,
}

impl Adaptive {
    const fn new() -> Self {
        Self {
            on: false,
            since: None,
            holding_since: None,
            learned_at: None,
            wait: 0.0,
            learned_cap: 0.0,
        }
    }
}

/// Per-queue frame-generation ring, interpolator, adaptive state, and pacing thread
///
/// (`MTLD3D12CommandQueueImpl::FrameGen`, `d3d12_command_queue.cpp:2176-2202`).
pub struct FrameGen {
    interpolator: Option<Obj>,
    fence: Option<Obj>,
    real: [Option<Obj>; RING],
    generated: [Option<Obj>; RING],
    info: WMTFXFrameInterpolatorInfo,
    format: u32,
    width: u64,
    height: u64,
    failed: bool,
    interval: f64,
    last: Option<Instant>,
    enqueued: u64,
    has_prev: bool,
    queue: Option<Obj>,
    written: Option<Obj>,
    read: Option<Obj>,
    shared: OnceLock<Arc<Shared>>,
    thread: Option<JoinHandle<()>>,
    adapt: Adaptive,
    always: bool,
}

impl FrameGen {
    /// Creates an idle [`FrameGen`] instance.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            interpolator: None,
            fence: None,
            real: [None, None, None],
            generated: [None, None, None],
            info: ZERO_INTERP_INFO,
            format: WMT_PIXEL_FORMAT_INVALID,
            width: 0,
            height: 0,
            failed: false,
            interval: 0.0,
            last: None,
            enqueued: 0,
            has_prev: false,
            queue: None,
            written: None,
            read: None,
            shared: OnceLock::new(),
            thread: None,
            adapt: Adaptive::new(),
            always: false,
        }
    }

    fn shared(&self) -> &Arc<Shared> {
        self.shared.get_or_init(|| Arc::new(Shared::new()))
    }

    fn is_always(&self) -> bool {
        self.always || mode() == Mode::Always
    }

    /// Attempts to present `frame` through the frame-generation ring and pacing thread
    ///
    /// (`PresentFrameGen`, `d3d12_command_queue.cpp:2322-2477`).
    ///
    /// Returns `true` when the pacing thread took the frame; `false` when the caller should
    /// present the frame directly as rendered.
    pub fn present(
        &mut self,
        cmdbuf: &Obj,
        presenter: &Arc<Presenter>,
        frame: &Obj,
        metadata: &PresentMetadata,
        refresh: f64,
        order: &mut dyn Order,
    ) -> bool {
        let mut backend = ProdBackend;
        self.present_with(
            &mut backend,
            &PresentArgs {
                now: Instant::now(),
                cmdbuf,
                presenter: Some(presenter),
                device: presenter.device,
                frame,
                metadata,
                refresh,
            },
            order,
        )
    }

    fn present_with(
        &mut self,
        backend: &mut dyn Backend,
        args: &PresentArgs<'_>,
        order: &mut dyn Order,
    ) -> bool {
        let since = self.last.map_or(1.0, |last| {
            args.now.saturating_duration_since(last).as_secs_f64()
        });
        self.last = Some(args.now);
        if since < 0.05 {
            self.interval = if self.interval > 0.0 {
                since.mul_add(0.1, self.interval * 0.9)
            } else {
                since
            };
        }
        let (fresh, scaler, depth, motion, props) = take_slot();
        if self.failed {
            return false;
        }
        if !self.is_always() {
            let refresh = if args.refresh > 0.0 {
                args.refresh
            } else {
                60.0
            };
            if self.interval <= 0.0 || !self.adapt_frame_gen(self.interval, refresh, args.now) {
                self.has_prev = false;
                return false;
            }
        }
        let (frame_w, frame_h, frame_fmt) = backend.texture_size_format(args.frame);
        if frame_w != self.width || frame_h != self.height || frame_fmt != self.format {
            let Some(ref sc) = scaler else {
                return false;
            };
            if !fresh
                || u64::from(sc.info.output_width) != frame_w
                || u64::from(sc.info.output_height) != frame_h
            {
                return false;
            }
            self.drain();
            let mut t = ring_texture_info(frame_w, frame_h, frame_fmt);
            for slot in 0..RING {
                self.real[slot] = Some(backend.new_texture(args.device, &mut t, false, slot));
                self.generated[slot] = Some(backend.new_texture(args.device, &mut t, true, slot));
            }
            self.width = frame_w;
            self.height = frame_h;
            self.format = frame_fmt;
            self.has_prev = false;
            if self.queue.is_none() {
                self.queue = Some(backend.new_command_queue(args.device, (RING as u64) + 1));
                self.written = Some(backend.new_event(args.device, true));
                self.read = Some(backend.new_event(args.device, false));
                self.fence = Some(backend.new_fence(args.device));
                backend.spawn_pacer(self);
            }
        }
        let generate = fresh
            && self.has_prev
            && props.reset == 0
            && since < 0.05
            && self.interval > 0.0
            && depth.is_some()
            && motion.is_some()
            && scaler.as_ref().is_some_and(|sc| {
                u64::from(sc.info.output_width) == self.width
                    && u64::from(sc.info.output_height) == self.height
            });
        if generate {
            let sc = scaler.as_ref().expect("checked in generate");
            let depth_tex = depth.as_ref().expect("checked in generate");
            let motion_tex = motion.as_ref().expect("checked in generate");
            let (_, _, depth_fmt) = backend.texture_size_format(depth_tex);
            let (_, _, motion_fmt) = backend.texture_size_format(motion_tex);
            let info = WMTFXFrameInterpolatorInfo {
                color_format: self.format,
                output_format: self.format,
                depth_format: depth_fmt,
                motion_format: motion_fmt,
                input_width: sc.info.input_width,
                input_height: sc.info.input_height,
                output_width: sc.info.output_width,
                output_height: sc.info.output_height,
                scaler: sc.handle.handle(),
            };
            if self.interpolator.is_none() || !same_interpolator_info(&info, &self.info) {
                scope!("queue.(frame generation: make the interpolator)");
                self.interpolator = backend.new_frame_interpolator(args.device, &info);
                self.info = info;
                if self.interpolator.is_none() {
                    log_stderr(&format!(
                        "macneutron-framegen: MetalFX made no frame interpolator for {}x{} to {}x{} \
                         (colour {}, depth {}, motion {}): frames are presented as rendered",
                        info.input_width,
                        info.input_height,
                        info.output_width,
                        info.output_height,
                        info.color_format,
                        info.depth_format,
                        info.motion_format,
                    ));
                    self.failed = true;
                    self.drain();
                    return false;
                }
                log_stderr(&format!(
                    "macneutron-framegen: generating a frame between each two, {}x{} \
                     (paired with the {}x{} upscaler)",
                    info.output_width, info.output_height, info.input_width, info.input_height,
                ));
            }
        }
        let n = self.enqueued;
        {
            scope!("queue.(frame generation: wait for the pacing thread)");
            let w = backend.wait_for_pacer(self, n);
            self.adapt.wait = w.mul_add(0.1, self.adapt.wait * 0.9);
        }
        let ring_u64 = RING as u64;
        let slot = usize::try_from(n % ring_u64).unwrap_or(0);
        let prev = usize::try_from((n + ring_u64 - 1) % ring_u64).unwrap_or(0);
        let fence = retain(self.fence.as_ref().expect("fence initialized with ring"));
        let begin = backend.blit_encoder(args.cmdbuf, true);
        order.wait(&begin);
        backend.blit_update_fence(&begin, &fence);
        backend.end_encoding(&begin);
        if n >= ring_u64 {
            let read = self
                .read
                .as_ref()
                .expect("read event initialized with ring");
            backend.wait_event(args.cmdbuf, read, n - ring_u64 + 1);
        }
        if generate {
            scope!("queue.(encode frame interpolation)");
            count!("#frames generated", 1);
            let aspect_w = u32_to_f32(u32::try_from(self.width).unwrap_or(u32::MAX));
            let aspect_h = u32_to_f32(u32::try_from(self.height).unwrap_or(u32::MAX));
            let interp_props = WMTFXFrameInterpolatorProps {
                motion_vector_scale_x: props.motion_vector_scale_x,
                motion_vector_scale_y: props.motion_vector_scale_y,
                delta_time: f64_to_f32(self.interval),
                near_plane: 0.1,
                far_plane: 10000.0,
                field_of_view: 70.0,
                aspect_ratio: aspect_w / aspect_h,
                jitter_offset_x: props.jitter_offset_x,
                jitter_offset_y: props.jitter_offset_y,
                reset: 0,
                depth_reversed: props.depth_reversed,
                ui_composited: 0,
            };
            let interpolator = self
                .interpolator
                .as_ref()
                .expect("interpolator created above");
            let prev_tex = self.real[prev].as_ref().expect("prev slot initialized");
            let depth_tex = depth.as_ref().expect("depth checked in generate");
            let motion_tex = motion.as_ref().expect("motion checked in generate");
            let out_tex = self.generated[slot]
                .as_ref()
                .expect("generated slot initialized");
            backend.encode_interpolate(&InterpolateArgs {
                cmdbuf: args.cmdbuf,
                interpolator,
                color: args.frame,
                prev: prev_tex,
                depth: depth_tex,
                motion: motion_tex,
                output: out_tex,
                fence: &fence,
                props: &interp_props,
            });
        }
        let real_slot = retain(self.real[slot].as_ref().expect("real slot initialized"));
        let gen_slot = self.generated[slot].as_ref().map(retain);
        let end = backend.blit_encoder(args.cmdbuf, false);
        backend.blit_wait_fence(&end, &fence);
        backend.blit_copy_texture(&end, args.frame, &real_slot);
        order.update(&end);
        backend.end_encoding(&end);
        let written = self
            .written
            .as_ref()
            .expect("written event initialized with ring");
        backend.signal_event(args.cmdbuf, written, n + 1);
        backend.enqueue(
            self,
            &EnqueueArgs {
                n,
                real: &real_slot,
                gen_tex: if generate { gen_slot.as_ref() } else { None },
                presenter: args.presenter,
                metadata: args.metadata,
            },
        );
        self.enqueued = n + 1;
        self.has_prev = true;
        true
    }

    /// Wakes the pacing thread after the game's command buffer has been committed
    ///
    /// (`d3d12_command_queue.cpp:2474-2475, 2504`).
    pub fn notify(&self) {
        if let Some(shared) = self.shared.get() {
            shared.cv.notify_all();
        }
    }

    /// Waits until the pacing thread has committed every enqueued frame (`DrainPacing`,
    ///
    /// `d3d12_command_queue.cpp:2310-2315`).
    pub fn drain(&self) {
        let Some(shared) = self.shared.get() else {
            return;
        };
        let mut state = shared.lock();
        while state.committed != self.enqueued {
            state = shared
                .cv
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        drop(state);
    }

    /// Returns `true` when the pacing thread has been started (`fg.thread.joinable()`).
    #[must_use]
    pub fn running(&self) -> bool {
        self.thread.is_some() || self.shared.get().is_some_and(|s| s.lock().started)
    }

    /// Adaptive frame-generation decision (`AdaptFrameGen`, `d3d12_command_queue.cpp:2212-2250`).
    pub fn adapt_frame_gen(&mut self, interval: f64, refresh: f64, now: Instant) -> bool {
        if self.adapt.learned_cap > 0.0
            && self
                .adapt
                .learned_at
                .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_secs(60))
        {
            self.adapt.learned_cap = 0.0;
        }
        if !self.adapt.on {
            let pair = self.shared().lock().pair;
            let cap = (2.0 / refresh).max(pair).max(self.adapt.learned_cap);
            if self.held(interval > 1.1 * cap, Duration::from_millis(500), now) {
                self.transition(
                    true,
                    interval,
                    refresh,
                    now,
                    "a frame generated between each two",
                );
            }
        } else if self.held(
            self.adapt.wait > 0.1 * interval,
            Duration::from_millis(500),
            now,
        ) {
            self.adapt.learned_cap = self.adapt.learned_cap.max(interval);
            self.adapt.learned_at = Some(now);
            self.transition(
                false,
                interval,
                refresh,
                now,
                "generating held the game back, frames presented as rendered",
            );
        }
        self.adapt.on
    }

    fn held(&mut self, cond: bool, duration: Duration, now: Instant) -> bool {
        if !cond {
            self.adapt.holding_since = None;
            return false;
        }
        let since = *self.adapt.holding_since.get_or_insert(now);
        now.saturating_duration_since(since) >= duration
    }

    fn transition(&mut self, on: bool, interval: f64, refresh: f64, now: Instant, why: &str) {
        self.adapt.on = on;
        self.adapt.since = Some(now);
        self.adapt.holding_since = None;
        self.adapt.wait = 0.0;
        log_stderr(&format!(
            "macneutron-framegen: {} at {:.0} fps (display {:.0} Hz): {why}",
            if on { "on" } else { "off" },
            1.0 / interval,
            refresh,
        ));
    }

    #[cfg(test)]
    fn set_pair(&self, pair: f64) {
        self.shared().lock().pair = pair;
    }

    #[cfg(test)]
    const fn set_wait(&mut self, wait: f64) {
        self.adapt.wait = wait;
    }

    #[cfg(test)]
    const fn wait(&self) -> f64 {
        self.adapt.wait
    }

    #[cfg(test)]
    const fn learned_cap(&self) -> f64 {
        self.adapt.learned_cap
    }

    #[cfg(test)]
    const fn learned_at(&self) -> Option<Instant> {
        self.adapt.learned_at
    }

    #[cfg(test)]
    const fn holding_since(&self) -> Option<Instant> {
        self.adapt.holding_since
    }

    #[cfg(test)]
    const fn set_always(&mut self, always: bool) {
        self.always = always;
    }

    #[cfg(test)]
    fn mark_pacer_started(&self) {
        self.shared().lock().started = true;
    }

    #[cfg(test)]
    fn set_committed(&self, committed: u64) {
        let shared = self.shared();
        shared.lock().committed = committed;
        shared.cv.notify_all();
    }

    #[cfg(test)]
    fn committed(&self) -> u64 {
        self.shared().lock().committed
    }

    #[cfg(test)]
    const fn enqueued(&self) -> u64 {
        self.enqueued
    }

    #[cfg(test)]
    fn present_or_drain(
        &mut self,
        backend: &mut dyn Backend,
        after: f64,
        args: &PresentArgs<'_>,
        order: &mut dyn Order,
    ) -> bool {
        let handed = if after <= 0.0 {
            self.present_with(backend, args, order)
        } else {
            false
        };
        if !handed && self.running() {
            self.drain();
        }
        handed
    }
}

impl Default for FrameGen {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FrameGen {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            if let Some(shared) = self.shared.get() {
                shared.lock().stop = true;
                shared.cv.notify_all();
            }
            // Poll `is_finished` rather than `JoinHandle::join`, which deadlocks under the loader
            // lock if `FrameGen` is dropped during `DllMain(DLL_PROCESS_DETACH)`.
            while !thread.is_finished() {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

fn log_stderr(msg: &str) {
    #[cfg(test)]
    tests::capture_stderr(msg);
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{msg}");
}

#[cfg(test)]
mod tests;
