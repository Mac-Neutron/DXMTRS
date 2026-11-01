use core::time::Duration;

use super::{
    CompatibilityFlags, Container, FRAME_STATISTICS_COUNT, FrameStatistics, ScalerInfo, ScalerType,
};

#[test]
fn average_over_the_window() {
    let mut c = Container::default();
    for i in 0..FRAME_STATISTICS_COUNT {
        let k = u32::try_from(i + 1).expect("fits in u32");
        let slot = c.at_mut(i as u64);
        slot.command_buffer_count = k * 15;
        slot.sync_count = k * 30;
        slot.event_stall = k * 45;
        slot.commit_interval = Duration::from_micros(u64::from(k) * 150);
        slot.sync_interval = Duration::from_micros(u64::from(k) * 300);
        slot.encode_prepare_interval = Duration::from_micros(u64::from(k) * 450);
        slot.encode_flush_interval = Duration::from_micros(u64::from(k) * 600);
        slot.drawable_blocking_interval = Duration::from_micros(u64::from(k) * 750);
        slot.present_latency_interval = Duration::from_micros(u64::from(k) * 900);
    }

    // Exclude slot 15 (`k = 16`): remaining `k = 1..=15` have sum 120, average `120 / 15 = 8`,
    // and max `k = 15`.
    c.compute(15);

    let avg = c.average();
    assert_eq!(avg.command_buffer_count, 8 * 15);
    assert_eq!(avg.sync_count, 8 * 30);
    assert_eq!(avg.event_stall, 8 * 45);
    assert_eq!(avg.commit_interval, Duration::from_micros(8 * 150));
    assert_eq!(avg.sync_interval, Duration::from_micros(8 * 300));
    assert_eq!(avg.encode_prepare_interval, Duration::from_micros(8 * 450));
    assert_eq!(avg.encode_flush_interval, Duration::from_micros(8 * 600));
    assert_eq!(avg.drawable_blocking_interval, Duration::from_millis(6));
    assert_eq!(avg.present_latency_interval, Duration::from_micros(8 * 900));

    let max = c.max();
    assert_eq!(max.command_buffer_count, 15 * 15);
    assert_eq!(max.sync_count, 15 * 30);
    assert_eq!(max.event_stall, 15 * 45);
    assert_eq!(max.commit_interval, Duration::from_micros(15 * 150));
    assert_eq!(max.sync_interval, Duration::from_micros(15 * 300));
    assert_eq!(max.encode_prepare_interval, Duration::from_micros(15 * 450));
    assert_eq!(max.encode_flush_interval, Duration::from_millis(9));
    assert_eq!(
        max.drawable_blocking_interval,
        Duration::from_micros(15 * 750)
    );
    assert_eq!(
        max.present_latency_interval,
        Duration::from_micros(15 * 900)
    );
}

#[test]
fn scaler_info_kept() {
    let mut c = Container::default();
    {
        let f0 = c.at_mut(0);
        f0.command_buffer_count = 30;
        f0.compatibility_flags = CompatibilityFlags::UNSUPPORTED_GEOMETRY_DRAW;
        f0.last_scaler_info = ScalerInfo {
            kind: ScalerType::Temporal,
            input_width: 1280,
            input_height: 720,
            output_width: 2560,
            output_height: 1440,
            auto_exposure: true,
            motion_vector_highres: true,
        };
    }

    // Recomputing at frame 1 aggregates frame 0 into `max`/`average` while preserving frame 0's
    // `last_scaler_info`, and `reset()` on a frame clears `kind` to `None` while keeping the last
    // scaler dimensions and flags intact as C++'s `FrameStatistics::reset` does.
    c.compute(1);
    assert_eq!(c.max().command_buffer_count, 30);
    assert_eq!(c.average().command_buffer_count, 2);
    assert!(
        c.at(0).last_scaler_info
            == ScalerInfo {
                kind: ScalerType::Temporal,
                input_width: 1280,
                input_height: 720,
                output_width: 2560,
                output_height: 1440,
                auto_exposure: true,
                motion_vector_highres: true,
            }
    );

    let mut f: FrameStatistics = FrameStatistics {
        compatibility_flags: CompatibilityFlags::UNSUPPORTED_DRAW_AUTO,
        command_buffer_count: 5,
        last_scaler_info: ScalerInfo {
            kind: ScalerType::Spatial,
            input_width: 1920,
            input_height: 1080,
            output_width: 3840,
            output_height: 2160,
            auto_exposure: false,
            motion_vector_highres: true,
        },
        ..FrameStatistics::default()
    };
    f.reset();
    assert!(f.compatibility_flags.is_empty());
    assert_eq!(f.command_buffer_count, 0);
    assert!(
        f.last_scaler_info
            == ScalerInfo {
                kind: ScalerType::None,
                input_width: 1920,
                input_height: 1080,
                output_width: 3840,
                output_height: 2160,
                auto_exposure: false,
                motion_vector_highres: true,
            }
    );
}
