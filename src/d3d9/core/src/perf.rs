//! Performance-counter and API-timer plumbing.
//!
//! Every TSC timer and API-thread counter lives here, visibly separate from
//! the D3D9 / Metal runtime state. Nothing in this module touches COM,
//! `raw-dylib`, or `DeviceInner`. Enabled `ApiTimer`s retain the counter storage
//! independently of the device, including when a nested final Release destroys
//! `DeviceInner`.

use std::ops::DerefMut;
#[cfg(perf_tracking)]
use std::{cell::RefCell, rc::Rc};

/// Re-exports so existing `mtld3d_core::perf::*` paths in d3d9 keep working.
///
/// The gate + state-agnostic timers + latch fn moved to [`mtld3d_shared::perf`].
pub use mtld3d_shared::perf::{
    CycleAddTimer, CycleSetTimer, init_tracking_enabled, pair_stats_enabled, perf_enabled,
};
#[cfg(perf_tracking)]
use mtld3d_shared::tsc::rdtsc;
#[cfg(perf_tracking)]
use strum::EnumCount;

pub mod window_line;

/// Window length for the averaged `mtld3d::perf=debug` summary.
pub const SUMMARY_INTERVAL_SECS: u64 = 2;

#[cfg(not(perf_tracking))]
const _: () = {
    assert!(size_of::<ApiPerfStorage>() == 0);
    assert!(size_of::<ApiTimer>() == 0);
};

/// Fixed seed of the frame picks in [`ApiPerfState::new`], which unit tests use.
#[cfg(perf_tracking)]
const FIXED_FRAME_SEED: u32 = 0x2545_f491;

/// A seed for frame picks from the time-stamp counter, never 0.
#[cfg(perf_tracking)]
fn frame_seed_from_clock() -> u32 {
    let [b0, b1, b2, b3, b4, b5, b6, b7] = rdtsc().to_le_bytes();
    (u32::from_le_bytes([b0, b1, b2, b3]) ^ u32::from_le_bytes([b4, b5, b6, b7])) | 1
}

/// Which COM wrapper type owns the vtable fn wrapping an `ApiTimer`.
///
/// Arranged so `as usize` indexes directly into the accumulator arrays
/// on `ApiPerfState`.
#[derive(Clone, Copy, Debug, strum::EnumCount)]
#[repr(u32)]
pub enum ApiCategory {
    Device = 0,
    VertexBuffer,
    IndexBuffer,
    Texture,
    Surface,
    Query,
    StateBlock,
    VertexDecl,
    VertexShader,
    PixelShader,
}

/// Sub-bucket inside the `Device` `ApiCategory`.
///
/// Every `extern "system"` `IDirect3DDevice9` entry point tags its `ApiTimer`
/// with a `DeviceSubCategory`. `Misc` is the catch-all so the sub-buckets
/// sum to `api_cycles_by_category[Device]`.
#[derive(Clone, Copy, Debug, strum::EnumCount)]
#[repr(u32)]
pub enum DeviceSubCategory {
    Draws = 0,
    RenderState,
    TexStageState,
    SamplerState,
    ShaderConst,
    Bind,
    Frame,
    StateBlock,
    Misc,
}

/// Sub-bucket inside the `Bind` `DeviceSubCategory`.
///
/// Every `IDirect3DDevice9` entry point whose `device_timer` tag is `Bind`
/// instead uses `bind_timer` and supplies a `BindSubCategory`.
#[derive(Clone, Copy, Debug, strum::EnumCount)]
#[repr(u32)]
pub enum BindSubCategory {
    /// `Set/GetTexture`.
    Texture = 0,
    /// `Set/GetStreamSource`, `Set/GetStreamSourceFreq`, `Set/GetIndices`.
    Buffer,
    /// `Set/GetVertexDeclaration`, `Set/GetVertexShader`, `Set/GetPixelShader`, `Set/GetFVF`.
    Shader,
    /// `Set/GetRenderTarget`, `Set/GetDepthStencilSurface`.
    RtDs,
    /// Fixed-function state setters: transform, material, light, clip plane.
    FfFixed,
    /// `Set/GetViewport`, `Set/GetScissorRect`.
    ViewScissor,
}

/// Sub-bucket inside the `Surface` `ApiCategory`.
///
/// Every `IDirect3DSurface9` vtable thunk tags its `ApiTimer` with a
/// `SurfaceSubCategory` (via `surf_timer`). `Misc` is the catch-all so the
/// sub-buckets sum to `api_cycles_by_category[Surface]`.
#[derive(Clone, Copy, Debug, strum::EnumCount)]
#[repr(u32)]
pub enum SurfaceSubCategory {
    /// `LockRect`: map, staging rename, or synchronous backbuffer readback.
    LockRect = 0,
    /// `UnlockRect`: unmap plus dirty-range upload queueing.
    UnlockRect,
    /// `GetDC`: DIB section + memory DC build.
    GetDc,
    /// `ReleaseDC`: DC teardown and write-back.
    ReleaseDc,
    /// Everything else: getters, `IUnknown`, container walks.
    Misc,
}

/// Per-draw dirty-mark gate whose redundant-skip rate perf telemetry tracks.
///
/// Each variant indexes the `keys_gate_calls` / `keys_gate_skips` arrays.
/// Defined unconditionally so COM thunks can name a variant regardless of
/// `cfg(perf_tracking)`.
#[derive(Clone, Copy, Debug, strum::EnumCount)]
#[repr(u32)]
pub enum KeysGate {
    /// `SetTexture`.
    SetTexture = 0,
    /// `SetRenderState`.
    SetRenderState,
    /// `SetTextureStageState`.
    SetTextureStageState,
    /// `SetFVF`.
    SetFvf,
    /// `SetVertexDeclaration`.
    SetVertexDecl,
    /// `SetVertexShader`.
    SetVertexShader,
    /// `SetPixelShader`.
    SetPixelShader,
    /// `SetVertexShaderConstantF`.
    SetVsConst,
    /// `SetPixelShaderConstantF`.
    SetPsConst,
}

/// Device-owned holder of [`ApiPerfState`].
///
/// Shared with enabled [`ApiTimer`] guards via `Rc<RefCell<...>>` so an outer
/// timer keeps the counter block alive if a nested final `Release` drops the
/// device while the timer is on the stack. Under `cfg(not(perf_tracking))`
/// this is a zero-sized wrapper around the zero-sized `ApiPerfState`.
pub struct ApiPerfStorage {
    #[cfg(perf_tracking)]
    state: Rc<RefCell<ApiPerfState>>,
    #[cfg(not(perf_tracking))]
    state: ApiPerfState,
}

impl Default for ApiPerfStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiPerfStorage {
    #[cfg(perf_tracking)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(ApiPerfState::with_frame_seed(
                frame_seed_from_clock(),
            ))),
        }
    }

    #[cfg(not(perf_tracking))]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: ApiPerfState::new(),
        }
    }

    /// Borrow counters only for the update, never across a nested API call.
    pub fn state_mut(&mut self) -> impl DerefMut<Target = ApiPerfState> + '_ {
        #[cfg(perf_tracking)]
        {
            self.state.borrow_mut()
        }
        #[cfg(not(perf_tracking))]
        {
            &mut self.state
        }
    }

    /// Stable backing for subtimers nested inside an owning API timer, or null in an untimed frame.
    #[cfg(perf_tracking)]
    pub fn as_ptr(&mut self) -> *mut ApiPerfState {
        if self.frame_timed() {
            self.state.as_ptr()
        } else {
            core::ptr::null_mut()
        }
    }

    /// Whether the frame being recorded runs its timers.
    #[cfg(perf_tracking)]
    fn frame_timed(&self) -> bool {
        self.state.borrow().untimed_frames == 0
    }

    #[cfg(not(perf_tracking))]
    pub const fn as_ptr(&mut self) -> *mut ApiPerfState {
        core::ptr::null_mut()
    }
}

/// RAII guard that measures TSC cycles spent inside a D3D9 COM vtable entry point.
///
/// Adds the cycles to the owning device's `ApiPerfState`.
///
/// Under `cfg(not(perf_tracking))` this collapses to a unit struct with an
/// empty `Drop`. The entire timer disappears at compile time via LLVM DCE on
/// every `let _timer = device_timer(...)` call site.
#[cfg(perf_tracking)]
pub struct ApiTimer {
    start: u64,
    state: Option<Rc<RefCell<ApiPerfState>>>,
    /// Whether the timer read the clock; one in an untimed frame only counts its call at `Drop`.
    clock: bool,
    category: ApiCategory,
    device_sub: Option<DeviceSubCategory>,
    bind_sub: Option<BindSubCategory>,
    surface_sub: Option<SurfaceSubCategory>,
    saved_child_cycles: u64,
}

#[cfg(not(perf_tracking))]
pub struct ApiTimer;

impl ApiTimer {
    /// Enter a child timing scope.
    ///
    /// Save the parent's child-cycle accumulator, zero it so this timer
    /// measures only *its own* children, and bump the nesting depth.
    #[cfg(perf_tracking)]
    #[inline]
    fn enter_scope(state: Option<&Rc<RefCell<ApiPerfState>>>) -> u64 {
        let Some(state) = state else {
            return 0;
        };
        let mut perf = state.borrow_mut();
        let saved = perf.active_child_cycles;
        perf.active_child_cycles = 0;
        perf.timer_depth = perf.timer_depth.saturating_add(1);
        saved
    }

    #[cfg(perf_tracking)]
    #[must_use]
    pub fn start(storage: Option<&ApiPerfStorage>, category: ApiCategory) -> Self {
        Self::begin(storage, category, false)
    }

    /// Start a timer that reads the clock in timed frames, or in every frame with `every_frame`.
    #[cfg(perf_tracking)]
    fn begin(storage: Option<&ApiPerfStorage>, category: ApiCategory, every_frame: bool) -> Self {
        Self::frame_gated(storage.filter(|_| perf_enabled()), category, every_frame)
    }

    /// The frame gate of [`Self::begin`], past the runtime perf switch.
    #[cfg(perf_tracking)]
    fn frame_gated(
        storage: Option<&ApiPerfStorage>,
        category: ApiCategory,
        every_frame: bool,
    ) -> Self {
        let clock = storage.is_some_and(|storage| every_frame || storage.frame_timed());
        Self::new(storage, category, clock)
    }

    #[cfg(perf_tracking)]
    fn new(storage: Option<&ApiPerfStorage>, category: ApiCategory, clock: bool) -> Self {
        let state = storage.map(|s| Rc::clone(&s.state));
        let saved_child_cycles = if clock {
            Self::enter_scope(state.as_ref())
        } else {
            0
        };
        let start = if clock && state.is_some() { rdtsc() } else { 0 };
        Self {
            start,
            state,
            clock,
            category,
            device_sub: None,
            bind_sub: None,
            surface_sub: None,
            saved_child_cycles,
        }
    }

    #[cfg(not(perf_tracking))]
    #[inline]
    #[must_use]
    pub const fn start(_storage: Option<&ApiPerfStorage>, _category: ApiCategory) -> Self {
        Self
    }

    /// Like `start`, but tags the timer with a `DeviceSubCategory`.
    #[cfg(perf_tracking)]
    #[must_use]
    pub fn start_device(storage: Option<&ApiPerfStorage>, sub: DeviceSubCategory) -> Self {
        let every_frame = matches!(sub, DeviceSubCategory::Frame);
        let mut timer = Self::begin(storage, ApiCategory::Device, every_frame);
        timer.device_sub = Some(sub);
        timer
    }

    #[cfg(not(perf_tracking))]
    #[inline]
    #[must_use]
    pub const fn start_device(_storage: Option<&ApiPerfStorage>, _sub: DeviceSubCategory) -> Self {
        Self
    }

    /// Like `start_device(Bind)`, but additionally tags the timer with a `BindSubCategory`.
    #[cfg(perf_tracking)]
    #[must_use]
    pub fn start_bind(storage: Option<&ApiPerfStorage>, sub: BindSubCategory) -> Self {
        let mut timer = Self::start_device(storage, DeviceSubCategory::Bind);
        timer.bind_sub = Some(sub);
        timer
    }

    #[cfg(not(perf_tracking))]
    #[inline]
    #[must_use]
    pub const fn start_bind(_storage: Option<&ApiPerfStorage>, _sub: BindSubCategory) -> Self {
        Self
    }

    /// Like `start(Surface)`, but tags the timer with a `SurfaceSubCategory`.
    #[cfg(perf_tracking)]
    #[must_use]
    pub fn start_surface(storage: Option<&ApiPerfStorage>, sub: SurfaceSubCategory) -> Self {
        let mut timer = Self::start(storage, ApiCategory::Surface);
        timer.surface_sub = Some(sub);
        timer
    }

    #[cfg(not(perf_tracking))]
    #[inline]
    #[must_use]
    pub const fn start_surface(
        _storage: Option<&ApiPerfStorage>,
        _sub: SurfaceSubCategory,
    ) -> Self {
        Self
    }
}

/// Pure arithmetic of [`ApiTimer`]'s exclusive-time `Drop`.
///
/// Invariant across a balanced nest: the sum of every timer's `self_time`
/// equals the outermost timer's `elapsed` (no double-count).
#[cfg(perf_tracking)]
#[inline]
const fn exclusive_exit(elapsed: u64, children: u64, saved: u64, depth: u32) -> (u64, u64) {
    let self_time = elapsed.saturating_sub(children);
    let restored = if depth == 0 {
        0
    } else {
        saved.saturating_add(elapsed)
    };
    (self_time, restored)
}

#[cfg(perf_tracking)]
impl Drop for ApiTimer {
    fn drop(&mut self) {
        let Some(state) = &self.state else {
            return;
        };
        if !self.clock {
            // An untimed frame counts the call and books no time.
            self.book(&mut state.borrow_mut(), 0);
            return;
        }
        let elapsed = rdtsc() - self.start;
        let mut perf = state.borrow_mut();
        let my_children = perf.active_child_cycles;
        perf.timer_depth = perf.timer_depth.saturating_sub(1);
        let (self_time, restored) = exclusive_exit(
            elapsed,
            my_children,
            self.saved_child_cycles,
            perf.timer_depth,
        );
        perf.active_child_cycles = restored;
        self.book(&mut perf, self_time);
    }
}

#[cfg(perf_tracking)]
impl ApiTimer {
    /// Book one call and `cycles` into this timer's bucket and the buckets above it.
    const fn book(&self, perf: &mut ApiPerfState, cycles: u64) {
        if let Some(bind) = self.bind_sub {
            perf.add_bind_cycles(bind, cycles);
        } else if let Some(sub) = self.device_sub {
            perf.add_device_cycles(sub, cycles);
        } else if let Some(sub) = self.surface_sub {
            perf.add_surface_cycles(sub, cycles);
        } else {
            perf.add_api_cycles(self.category, cycles);
        }
    }
}

#[cfg(not(perf_tracking))]
impl Drop for ApiTimer {
    fn drop(&mut self) {}
}

#[cfg(perf_tracking)]
struct ApiCounters {
    api_cycles_by_category: [u64; ApiCategory::COUNT],
    api_call_counts_by_category: [u64; ApiCategory::COUNT],
    device_sub_cycles: [u64; DeviceSubCategory::COUNT],
    device_sub_calls: [u64; DeviceSubCategory::COUNT],
    bind_sub_cycles: [u64; BindSubCategory::COUNT],
    bind_sub_calls: [u64; BindSubCategory::COUNT],
    surface_sub_cycles: [u64; SurfaceSubCategory::COUNT],
    surface_sub_calls: [u64; SurfaceSubCategory::COUNT],
    keys_gate_calls: [u32; KeysGate::COUNT],
    keys_gate_skips: [u32; KeysGate::COUNT],
    texture_renames: u32,
    texture_discards: u32,
    texture_preserve_cpu: u32,
    texture_write_in_place_unseen: u32,
    texture_add_dirty_calls: u32,
    texture_add_dirty_partial: u32,
    texture_add_dirty_area_bp: u32,
}

#[cfg(perf_tracking)]
impl ApiCounters {
    const fn new() -> Self {
        Self {
            api_cycles_by_category: [0; ApiCategory::COUNT],
            api_call_counts_by_category: [0; ApiCategory::COUNT],
            device_sub_cycles: [0; DeviceSubCategory::COUNT],
            device_sub_calls: [0; DeviceSubCategory::COUNT],
            bind_sub_cycles: [0; BindSubCategory::COUNT],
            bind_sub_calls: [0; BindSubCategory::COUNT],
            surface_sub_cycles: [0; SurfaceSubCategory::COUNT],
            surface_sub_calls: [0; SurfaceSubCategory::COUNT],
            keys_gate_calls: [0; KeysGate::COUNT],
            keys_gate_skips: [0; KeysGate::COUNT],
            texture_renames: 0,
            texture_discards: 0,
            texture_preserve_cpu: 0,
            texture_write_in_place_unseen: 0,
            texture_add_dirty_calls: 0,
            texture_add_dirty_partial: 0,
            texture_add_dirty_area_bp: 0,
        }
    }
}

/// Per-frame API-thread counters owned by [`ApiPerfStorage`].
///
/// Under `cfg(not(perf_tracking))` this collapses to a unit struct; all
/// `bump_*` / `add_*` methods become `const fn` no-ops.
#[cfg(perf_tracking)]
pub struct ApiPerfState {
    counters: ApiCounters,
    reset_epoch: u64,
    reset_epoch_saturated: bool,
    active_child_cycles: u64,
    timer_depth: u32,
    untimed_frames: u8,
}

#[cfg(perf_tracking)]
impl Default for ApiPerfState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(perf_tracking)]
impl ApiPerfState {
    /// Counters whose frame picks start from a fixed seed, as the unit tests want.
    #[must_use]
    pub const fn new() -> Self {
        Self::with_frame_seed(FIXED_FRAME_SEED)
    }

    /// Counters whose frame picks start from `seed`.
    #[must_use]
    pub const fn with_frame_seed(_seed: u32) -> Self {
        Self {
            counters: ApiCounters::new(),
            reset_epoch: 0,
            reset_epoch_saturated: false,
            active_child_cycles: 0,
            timer_depth: 0,
            untimed_frames: 0,
        }
    }

    /// Start a new cache lifetime after Reset drained the previous frame.
    pub const fn advance_reset_epoch(&mut self) {
        self.reset_epoch_saturated |= self.reset_epoch == u64::MAX;
        self.reset_epoch = self.reset_epoch.saturating_add(1);
    }

    /// Add cycles + one call to the per-category accumulator.
    pub const fn add_api_cycles(&mut self, category: ApiCategory, cycles: u64) {
        let idx = category as usize;
        let cyc = &mut self.counters.api_cycles_by_category[idx];
        *cyc = cyc.saturating_add(cycles);
        let cnt = &mut self.counters.api_call_counts_by_category[idx];
        *cnt = cnt.saturating_add(1);
    }

    /// Add cycles + one call to the top-level `Device` bucket and the per-sub-category breakdown.
    pub const fn add_device_cycles(&mut self, sub: DeviceSubCategory, cycles: u64) {
        let top = &mut self.counters.api_cycles_by_category[ApiCategory::Device as usize];
        *top = top.saturating_add(cycles);
        let top_cnt = &mut self.counters.api_call_counts_by_category[ApiCategory::Device as usize];
        *top_cnt = top_cnt.saturating_add(1);

        let idx = sub as usize;
        let cyc = &mut self.counters.device_sub_cycles[idx];
        *cyc = cyc.saturating_add(cycles);
        let cnt = &mut self.counters.device_sub_calls[idx];
        *cnt = cnt.saturating_add(1);
    }

    /// Add cycles + one call to the Device top, `Bind`, and per-`BindSubCategory` buckets.
    pub const fn add_bind_cycles(&mut self, sub: BindSubCategory, cycles: u64) {
        let top = &mut self.counters.api_cycles_by_category[ApiCategory::Device as usize];
        *top = top.saturating_add(cycles);
        let top_cnt = &mut self.counters.api_call_counts_by_category[ApiCategory::Device as usize];
        *top_cnt = top_cnt.saturating_add(1);

        let dev_idx = DeviceSubCategory::Bind as usize;
        let dev_cyc = &mut self.counters.device_sub_cycles[dev_idx];
        *dev_cyc = dev_cyc.saturating_add(cycles);
        let dev_cnt = &mut self.counters.device_sub_calls[dev_idx];
        *dev_cnt = dev_cnt.saturating_add(1);

        let idx = sub as usize;
        let cyc = &mut self.counters.bind_sub_cycles[idx];
        *cyc = cyc.saturating_add(cycles);
        let cnt = &mut self.counters.bind_sub_calls[idx];
        *cnt = cnt.saturating_add(1);
    }

    /// Add cycles + one call to the `Surface` top and per-`SurfaceSubCategory` buckets.
    pub const fn add_surface_cycles(&mut self, sub: SurfaceSubCategory, cycles: u64) {
        let top = &mut self.counters.api_cycles_by_category[ApiCategory::Surface as usize];
        *top = top.saturating_add(cycles);
        let top_cnt = &mut self.counters.api_call_counts_by_category[ApiCategory::Surface as usize];
        *top_cnt = top_cnt.saturating_add(1);

        let idx = sub as usize;
        let cyc = &mut self.counters.surface_sub_cycles[idx];
        *cyc = cyc.saturating_add(cycles);
        let cnt = &mut self.counters.surface_sub_calls[idx];
        *cnt = cnt.saturating_add(1);
    }

    /// Record one live call to a [`KeysGate`] setter, plus whether it skipped the key rebuild.
    pub const fn record_keys_gate(&mut self, gate: KeysGate, skipped: bool) {
        let idx = gate as usize;
        let calls = &mut self.counters.keys_gate_calls[idx];
        *calls = calls.saturating_add(1);
        if skipped {
            let skips = &mut self.counters.keys_gate_skips[idx];
            *skips = skips.saturating_add(1);
        }
    }

    /// Bump the CPU-memcpy preserve counter.
    pub const fn bump_texture_preserve_cpu(&mut self) {
        self.counters.texture_preserve_cpu = self.counters.texture_preserve_cpu.saturating_add(1);
    }

    /// Partial Lock of a texture level handed back in place over an unseen upload of this frame.
    pub const fn bump_texture_write_in_place_unseen(&mut self) {
        self.counters.texture_write_in_place_unseen = self
            .counters
            .texture_write_in_place_unseen
            .saturating_add(1);
    }

    /// Feed the `AddDirtyRect` probe: one call this frame.
    pub const fn bump_texture_add_dirty_rect(&mut self, partial: bool, area_bp: u32) {
        self.counters.texture_add_dirty_calls =
            self.counters.texture_add_dirty_calls.saturating_add(1);
        if partial {
            self.counters.texture_add_dirty_partial =
                self.counters.texture_add_dirty_partial.saturating_add(1);
        }
        self.counters.texture_add_dirty_area_bp = self
            .counters
            .texture_add_dirty_area_bp
            .saturating_add(area_bp);
    }

    pub const fn bump_texture_rename(&mut self) {
        self.counters.texture_renames = self.counters.texture_renames.saturating_add(1);
    }

    /// Subset of `bump_texture_rename` that took the no-preserve branch.
    pub const fn bump_texture_discard(&mut self) {
        self.counters.texture_discards = self.counters.texture_discards.saturating_add(1);
    }

    /// Current device reset epoch and saturation flag.
    #[must_use]
    pub const fn reset_epoch(&self) -> (u64, bool) {
        (self.reset_epoch, self.reset_epoch_saturated)
    }

    /// Accumulated cycles and calls for one [`ApiCategory`].
    #[must_use]
    pub const fn api_category_totals(&self, category: ApiCategory) -> (u64, u64) {
        let idx = category as usize;
        (
            self.counters.api_cycles_by_category[idx],
            self.counters.api_call_counts_by_category[idx],
        )
    }

    /// Accumulated cycles and calls for one [`DeviceSubCategory`].
    #[must_use]
    pub const fn device_sub_totals(&self, sub: DeviceSubCategory) -> (u64, u64) {
        let idx = sub as usize;
        (
            self.counters.device_sub_cycles[idx],
            self.counters.device_sub_calls[idx],
        )
    }

    /// Accumulated cycles and calls for one [`BindSubCategory`].
    #[must_use]
    pub const fn bind_sub_totals(&self, sub: BindSubCategory) -> (u64, u64) {
        let idx = sub as usize;
        (
            self.counters.bind_sub_cycles[idx],
            self.counters.bind_sub_calls[idx],
        )
    }

    /// Accumulated cycles and calls for one [`SurfaceSubCategory`].
    #[must_use]
    pub const fn surface_sub_totals(&self, sub: SurfaceSubCategory) -> (u64, u64) {
        let idx = sub as usize;
        (
            self.counters.surface_sub_cycles[idx],
            self.counters.surface_sub_calls[idx],
        )
    }

    /// Total calls and skipped calls recorded for one [`KeysGate`].
    #[must_use]
    pub const fn keys_gate_totals(&self, gate: KeysGate) -> (u32, u32) {
        let idx = gate as usize;
        (
            self.counters.keys_gate_calls[idx],
            self.counters.keys_gate_skips[idx],
        )
    }

    /// Texture rename, discard, preserve, in-place write, and dirty-rect counts.
    #[must_use]
    pub const fn texture_totals(&self) -> (u32, u32, u32, u32, u32, u32, u32) {
        (
            self.counters.texture_renames,
            self.counters.texture_discards,
            self.counters.texture_preserve_cpu,
            self.counters.texture_write_in_place_unseen,
            self.counters.texture_add_dirty_calls,
            self.counters.texture_add_dirty_partial,
            self.counters.texture_add_dirty_area_bp,
        )
    }
}

/// ZST twin of [`ApiPerfState`] for `cfg(not(perf_tracking))`.
#[cfg(not(perf_tracking))]
#[derive(Default)]
pub struct ApiPerfState;

#[cfg(not(perf_tracking))]
impl ApiPerfState {
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self
    }

    #[inline]
    pub const fn advance_reset_epoch(&mut self) {}
    #[inline]
    pub const fn add_api_cycles(&mut self, _category: ApiCategory, _cycles: u64) {}
    #[inline]
    pub const fn add_device_cycles(&mut self, _sub: DeviceSubCategory, _cycles: u64) {}
    #[inline]
    pub const fn add_bind_cycles(&mut self, _sub: BindSubCategory, _cycles: u64) {}
    #[inline]
    pub const fn add_surface_cycles(&mut self, _sub: SurfaceSubCategory, _cycles: u64) {}
    #[inline]
    pub const fn record_keys_gate(&mut self, _gate: KeysGate, _skipped: bool) {}
    #[inline]
    pub const fn bump_texture_preserve_cpu(&mut self) {}
    #[inline]
    pub const fn bump_texture_write_in_place_unseen(&mut self) {}
    #[inline]
    pub const fn bump_texture_add_dirty_rect(&mut self, _partial: bool, _area_bp: u32) {}
    #[inline]
    pub const fn bump_texture_rename(&mut self) {}
    #[inline]
    pub const fn bump_texture_discard(&mut self) {}
}

#[cfg(all(test, perf_tracking))]
mod tests;
