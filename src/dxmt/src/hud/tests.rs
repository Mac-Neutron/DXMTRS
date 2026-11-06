use super::statistics_lines;
use crate::frame_stats::{CompatibilityFlags, Container, ScalerType};

fn lines(statistics: &Container, frame_id: u64) -> Vec<String> {
    let mut out = Vec::new();
    statistics_lines(statistics, frame_id, |line| out.push(line.to_owned()));
    out
}

#[test]
fn statistics_text_as_cpp() {
    let mut statistics = Container::default();
    let frame = statistics.at_mut(4);
    frame.compatibility_flags = CompatibilityFlags::UNSUPPORTED_GEOMETRY_DRAW
        | CompatibilityFlags::UNSUPPORTED_DRAW_AUTO
        | CompatibilityFlags::UNSUPPORTED_MULTIPLE_STREAM_OUTPUT;
    frame.command_buffer_count = 7;
    frame.sync_count = 3;
    frame.latency = 2;
    frame.render_pass_count = 10;
    frame.render_pass_optimized = 4;
    frame.clear_pass_count = 3;
    frame.clear_pass_optimized = 1;
    frame.last_scaler_info.kind = ScalerType::Temporal;
    frame.last_scaler_info.auto_exposure = true;
    frame.last_scaler_info.input_width = 640;
    frame.last_scaler_info.input_height = 360;
    frame.last_scaler_info.output_width = 1280;
    frame.last_scaler_info.output_height = 720;
    assert_eq!(
        lines(&statistics, 5),
        [
            "---G----------A------MS----",
            "Commit:  7 - 0.0 - 0.0",
            "Sync:    3  0.0   0  0.0  2",
            "Encode:  0.0+ 0.0+ 0.0= 0.0",
            "Render:  6+4   Clear:  2+1 ",
            "MetalFX: Temporal AEXP ",
            "Scale:  640x 360->1280x 720",
        ]
    );
    // Frame 0's statistics are slot 15's; no scaler ran there, so no MetalFX lines.
    assert_eq!(lines(&statistics, 0).len(), 5);
}
