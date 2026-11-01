//! Frame generation and pacing on `MetalFX` (`d3d12_command_queue.cpp:82-107, 2160-2476`, P11).
//!
//! Holds the process-wide slot of the most recent temporal upscale ([`record_upscale`]), the
//! adaptive decision, the three-frame ring, and the background pacing thread ([`FrameGen`]).

use std::{sync::Arc, time::Instant};

use winemetal_sys::Obj;
pub use winemetal_sys::wire::{
    WMTFXTemporalScalerInfo, WMTFXTemporalScalerProps,
    WMTFXTemporalScalerProps as WmtFxTemporalScalerProps,
};

pub use super::{DXMTPresentMetadata as PresentMetadata, Presenter};
use crate::scaler::Scaler;

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

/// Reads the process-wide [`Mode`] from `MACNEUTRON_FRAMEGEN`.
#[must_use]
pub fn mode() -> Mode {
    crate::log::err("framegen::mode is not implemented.");
    Mode::Off
}

/// Records the most recent temporal upscale into the process-wide frame-generation slot
///
/// (`d3d12_command_queue.cpp:2001-2008`).
pub fn record_upscale(
    _scaler: &TemporalScaler,
    _depth: &Obj,
    _motion: &Obj,
    _props: &WmtFxTemporalScalerProps,
) {
    crate::log::err("framegen::record_upscale is not implemented.");
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

/// Per-queue frame-generation ring, interpolator, adaptive state, and pacing thread
///
/// (`MTLD3D12CommandQueueImpl::FrameGen`, `d3d12_command_queue.cpp:2176-2202`).
pub struct FrameGen {
    _private: (),
}

impl FrameGen {
    /// Creates an idle [`FrameGen`] instance.
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }

    /// Attempts to present `frame` through the frame-generation ring and pacing thread
    ///
    /// (`PresentFrameGen`, `d3d12_command_queue.cpp:2322-2477`).
    ///
    /// Returns `true` when the pacing thread took the frame; `false` when the caller should
    /// present the frame directly as rendered.
    pub fn present(
        &mut self,
        _cmdbuf: &Obj,
        _presenter: &Arc<Presenter>,
        _frame: &Obj,
        _metadata: &PresentMetadata,
        _refresh: f64,
        _order: &mut dyn Order,
    ) -> bool {
        false
    }

    /// Wakes the pacing thread after the game's command buffer has been committed
    ///
    /// (`d3d12_command_queue.cpp:2474-2475, 2504`).
    pub fn notify(&self) {
        crate::log::err("framegen::FrameGen::notify is not implemented.");
    }

    /// Waits until the pacing thread has committed every enqueued frame (`DrainPacing`,
    ///
    /// `d3d12_command_queue.cpp:2310-2315`).
    pub fn drain(&self) {
        crate::log::err("framegen::FrameGen::drain is not implemented.");
    }

    /// Returns `true` when the pacing thread has been started (`fg.thread.joinable()`).
    #[must_use]
    pub fn running(&self) -> bool {
        crate::log::err("framegen::FrameGen::running is not implemented.");
        false
    }

    /// Adaptive frame-generation decision (`AdaptFrameGen`, `d3d12_command_queue.cpp:2212-2250`).
    pub fn adapt_frame_gen(&mut self, _interval: f64, _refresh: f64, _now: Instant) -> bool {
        crate::log::err("framegen::FrameGen::adapt_frame_gen is not implemented.");
        false
    }
}

impl Default for FrameGen {
    fn default() -> Self {
        Self::new()
    }
}
