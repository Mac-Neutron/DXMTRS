//! The fakes as a front end's tests link them: from the rlib.

use dxmt_core::{device::Device, encoder::NO_NEXT, queue::QueueConfig};
use winemetal_sys::wire::{WMT_RENDER_COMMAND_DRAW, WmtcmdRenderDraw};

/// A render pass with one draw, committed, reaches Metal as an encoder, its records, the end.
#[test]
fn commit_captures_a_render_pass() {
    const DEVICE: u64 = 0xf1;
    let mut device = Device::headless(DEVICE, QueueConfig::default());
    let queue = device.queue_mut();
    queue.chunk().emit(|encoder| {
        encoder.start_render_pass(0, 0, 0, None);
        encoder.encode_render_command(WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type: 3,
            vertex_start: 0,
            vertex_count: 3,
            instance_count: 1,
            base_instance: 0,
        });
        encoder.end_pass();
    });
    let seq = queue.current_seq();
    queue.commit();
    queue.wait_cpu(seq);
    let calls = dxmt_fakes::take_calls(DEVICE);
    let at = |line: &str| {
        calls
            .iter()
            .position(|c| c == line)
            .unwrap_or_else(|| panic!("no {line:?} in {calls:#?}"))
    };
    let encoder = at("MTLCommandBuffer_renderCommandEncoder");
    let draw = at(
        "  render 19 primitive_type=3 vertex_start=0 vertex_count=3 instance_count=1 \
         base_instance=0",
    );
    let end = at("MTLCommandEncoder_endEncoding");
    let commit = at("MTLCommandBuffer_commit");
    assert!(encoder < draw && draw < end && end < commit, "{calls:#?}");
}
