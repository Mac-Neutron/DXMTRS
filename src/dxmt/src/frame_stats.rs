//! Per-frame statistics and feature-compatibility flags (`dxmt_statistics.hpp`).

use core::time::Duration;

bitflags::bitflags! {
    /// Feature-compatibility warnings recorded during a frame (`dxmt::FeatureCompatibility`).
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    pub struct CompatibilityFlags: u32 {
        const UNSUPPORTED_GEOMETRY_DRAW = 1 << 0;
        const UNSUPPORTED_TESSELLATION_OUTPUT_PRIMITIVE = 1 << 1;
        const UNSUPPORTED_INDIRECT_TESSELLATION_DRAW = 1 << 2;
        const UNSUPPORTED_GEOMETRY_TESSELLATION_DRAW = 1 << 3;
        const UNSUPPORTED_DRAW_AUTO = 1 << 4;
        const UNSUPPORTED_PREDICATION = 1 << 5;
        const UNSUPPORTED_STREAM_OUTPUT_APPENDING = 1 << 6;
        const UNSUPPORTED_MULTIPLE_STREAM_OUTPUT = 1 << 7;
    }
}

/// Which `MetalFX` scaler was used in the frame (`dxmt::ScalerType`).
#[derive(Default, PartialEq, Eq)]
pub enum ScalerType {
    #[default]
    None,
    Spatial,
    Temporal,
}

/// Parameters of the last scaler pass in the frame (`dxmt::ScalerInfo`).
#[derive(Default, PartialEq, Eq)]
pub struct ScalerInfo {
    pub kind: ScalerType,
    pub input_width: u32,
    pub input_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub auto_exposure: bool,
    pub motion_vector_highres: bool,
}

/// Counters and intervals for one frame (`dxmt::FrameStatistics`).
#[derive(Default)]
pub struct FrameStatistics {
    pub compatibility_flags: CompatibilityFlags,
    pub command_buffer_count: u32,
    pub sync_count: u32,
    pub sync_interval: Duration,
    pub commit_interval: Duration,
    pub render_pass_count: u32,
    pub render_pass_optimized: u32,
    pub clear_pass_count: u32,
    pub clear_pass_optimized: u32,
    pub resolve_pass_optimized: u32,
    pub compute_pass_count: u32,
    pub blit_pass_count: u32,
    pub event_stall: u32,
    pub latency: u32,
    pub encode_prepare_interval: Duration,
    pub encode_flush_interval: Duration,
    pub drawable_blocking_interval: Duration,
    pub present_latency_interval: Duration,
    pub last_scaler_info: ScalerInfo,
}

impl FrameStatistics {
    /// Zeroes all counters and intervals for a new frame (`FrameStatistics::reset`).
    pub const fn reset(&mut self) {
        self.compatibility_flags = CompatibilityFlags::empty();
        self.command_buffer_count = 0;
        self.sync_count = 0;
        self.sync_interval = Duration::ZERO;
        self.commit_interval = Duration::ZERO;
        self.render_pass_count = 0;
        self.render_pass_optimized = 0;
        self.clear_pass_count = 0;
        self.clear_pass_optimized = 0;
        self.resolve_pass_optimized = 0;
        self.compute_pass_count = 0;
        self.blit_pass_count = 0;
        self.event_stall = 0;
        self.latency = 0;
        self.encode_prepare_interval = Duration::ZERO;
        self.encode_flush_interval = Duration::ZERO;
        self.drawable_blocking_interval = Duration::ZERO;
        self.present_latency_interval = Duration::ZERO;
        self.last_scaler_info.kind = ScalerType::None;
    }

    /// Accumulates a CPU synchronization stall (`FrameStatistics::addSyncInterval`).
    pub fn add_sync_interval(&mut self, interval: Duration) {
        if !cfg!(feature = "perf-build") {
            self.sync_interval += interval;
            self.sync_count += 1;
        }
    }

    /// Accumulates a drawable acquisition stall (`FrameStatistics::addDrawableBlockingInterval`).
    pub fn add_drawable_blocking_interval(&mut self, interval: Duration) {
        if !cfg!(feature = "perf-build") {
            self.drawable_blocking_interval += interval;
        }
    }

    /// Records a feature-compatibility flag (`FrameStatistics::setCompatibilityFlag`).
    pub fn set_compatibility_flag(&mut self, flag: CompatibilityFlags) {
        if !cfg!(feature = "perf-build") {
            self.compatibility_flags |= flag;
        }
    }
}

/// Ring window of frame statistics (`kFrameStatisticsCount`).
pub const FRAME_STATISTICS_COUNT: usize = 16;

/// Rolling 16-frame statistics window (`dxmt::FrameStatisticsContainer`).
#[derive(Default)]
pub struct Container {
    frames: [FrameStatistics; FRAME_STATISTICS_COUNT],
    max: FrameStatistics,
    average: FrameStatistics,
}

impl Container {
    /// Slot for `frame` (`frame % 16`).
    #[must_use]
    pub fn at(&self, frame: u64) -> &FrameStatistics {
        let index = usize::try_from(frame % (FRAME_STATISTICS_COUNT as u64)).unwrap_or(0);
        &self.frames[index]
    }

    /// Mutable slot for `frame` (`frame % 16`).
    pub fn at_mut(&mut self, frame: u64) -> &mut FrameStatistics {
        let index = usize::try_from(frame % (FRAME_STATISTICS_COUNT as u64)).unwrap_or(0);
        &mut self.frames[index]
    }

    /// Maximum values across the completed window (`FrameStatisticsContainer::max`).
    #[must_use]
    pub const fn max(&self) -> &FrameStatistics {
        &self.max
    }

    /// Average values across the completed window (`FrameStatisticsContainer::average`).
    #[must_use]
    pub const fn average(&self) -> &FrameStatistics {
        &self.average
    }

    /// Recomputes `max` and `average` excluding `current_frame`.
    pub fn compute(&mut self, current_frame: u64) {
        if cfg!(feature = "perf-build") {
            return;
        }
        self.max.reset();
        self.average.reset();
        let current = usize::try_from(current_frame % (FRAME_STATISTICS_COUNT as u64)).unwrap_or(0);
        for (i, frame) in self.frames.iter().enumerate() {
            if i == current {
                continue;
            }
            self.average.command_buffer_count += frame.command_buffer_count;
            self.max.command_buffer_count = self
                .max
                .command_buffer_count
                .max(frame.command_buffer_count);

            self.average.sync_count += frame.sync_count;
            self.max.sync_count = self.max.sync_count.max(frame.sync_count);

            self.average.event_stall += frame.event_stall;
            self.max.event_stall = self.max.event_stall.max(frame.event_stall);

            self.average.commit_interval += frame.commit_interval;
            self.max.commit_interval = self.max.commit_interval.max(frame.commit_interval);

            self.average.sync_interval += frame.sync_interval;
            self.max.sync_interval = self.max.sync_interval.max(frame.sync_interval);

            self.average.encode_prepare_interval += frame.encode_prepare_interval;
            self.max.encode_prepare_interval = self
                .max
                .encode_prepare_interval
                .max(frame.encode_prepare_interval);

            self.average.encode_flush_interval += frame.encode_flush_interval;
            self.max.encode_flush_interval = self
                .max
                .encode_flush_interval
                .max(frame.encode_flush_interval);

            self.average.drawable_blocking_interval += frame.drawable_blocking_interval;
            self.max.drawable_blocking_interval = self
                .max
                .drawable_blocking_interval
                .max(frame.drawable_blocking_interval);

            self.average.present_latency_interval += frame.present_latency_interval;
            self.max.present_latency_interval = self
                .max
                .present_latency_interval
                .max(frame.present_latency_interval);
        }
        let window = u32::try_from(FRAME_STATISTICS_COUNT - 1).unwrap_or(15);
        self.average.command_buffer_count /= window;
        self.average.sync_count /= window;
        self.average.event_stall /= window;
        self.average.commit_interval /= window;
        self.average.sync_interval /= window;
        self.average.encode_prepare_interval /= window;
        self.average.encode_flush_interval /= window;
        self.average.drawable_blocking_interval /= window;
        self.average.present_latency_interval /= window;
    }
}

#[cfg(test)]
mod tests;
