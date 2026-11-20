//! The fakes as a front end's tests link them: from the rlib.

use dxmt_core::{device::Device, encoder::NO_NEXT, library, queue::QueueConfig};
use winemetal_sys::{
    MTLDevice_newRenderPipelineStateWithVertexDescriptor,
    wire::{
        WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER, WMT_BLIT_COMMAND_FILL_BUFFER,
        WMT_RENDER_COMMAND_DRAW, WMTVertexAttributeInfo, WMTVertexBufferLayoutInfo,
        WMTVertexDescriptorInfo, WmtcmdBlitCopyFromBufferToBuffer, WmtcmdBlitFillbuffer,
        WmtcmdRenderDraw,
    },
};

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

/// A blit pass reaches Metal as a blit encoder, its record and the end.
#[test]
fn commit_captures_a_blit_pass() {
    const DEVICE: u64 = 0xf2;
    let mut device = Device::headless(DEVICE, QueueConfig::default());
    let queue = device.queue_mut();
    queue.chunk().emit(|encoder| {
        encoder.start_blit_pass();
        encoder.encode_blit_command(WmtcmdBlitFillbuffer {
            r#type: WMT_BLIT_COMMAND_FILL_BUFFER,
            reserved: [0; 3],
            next: NO_NEXT,
            buffer: 0,
            offset: 0,
            length: 4,
            value: 0,
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
    let encoder = at("MTLCommandBuffer_blitCommandEncoder");
    let fill = at("  blit 8");
    let end = at("MTLCommandEncoder_endEncoding");
    assert!(encoder < fill && fill < end, "{calls:#?}");
}

/// A buffer-to-buffer copy prints its source, its destination, their offsets and the length.
#[test]
fn a_buffer_copy_prints_its_ranges() {
    const DEVICE: u64 = 0xf3;
    let mut device = Device::headless(DEVICE, QueueConfig::default());
    let queue = device.queue_mut();
    queue.chunk().emit(|encoder| {
        encoder.start_blit_pass();
        encoder.encode_blit_command(WmtcmdBlitCopyFromBufferToBuffer {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
            reserved: [0; 3],
            next: NO_NEXT,
            src: 7,
            src_offset: 32,
            dst: 9,
            dst_offset: 48,
            copy_length: 64,
        });
        encoder.end_pass();
    });
    let seq = queue.current_seq();
    queue.commit();
    queue.wait_cpu(seq);
    let calls = dxmt_fakes::take_calls(DEVICE);
    let copy = "  blit copy_to_buffer_from_buffer #7 src_offset=32 #9 dst_offset=48 length=64";
    assert!(calls.iter().any(|c| c == copy), "{calls:#?}");
}

/// A pipeline's vertex descriptor prints its used attributes, then its used layouts, by index.
#[test]
fn a_vertex_descriptor_prints_its_used_attributes_and_layouts() {
    const DEVICE: u64 = 0xf4;
    let info = library::render_pipeline_info();
    let unused = WMTVertexAttributeInfo {
        format: 0,
        offset: 0,
        buffer_index: 0,
    };
    let mut vertex = WMTVertexDescriptorInfo {
        attributes: [unused; 31],
        layouts: [WMTVertexBufferLayoutInfo {
            stride: 0,
            step_function: 0,
            step_rate: 0,
        }; 31],
    };
    vertex.attributes[3] = WMTVertexAttributeInfo {
        format: 29,
        offset: 16,
        buffer_index: 1,
    };
    vertex.attributes[0] = WMTVertexAttributeInfo {
        format: 31,
        offset: 0,
        buffer_index: 0,
    };
    vertex.layouts[0] = WMTVertexBufferLayoutInfo {
        stride: 20,
        step_function: 1,
        step_rate: 1,
    };
    vertex.layouts[1] = WMTVertexBufferLayoutInfo {
        stride: 8,
        step_function: 2,
        step_rate: 3,
    };
    let mut error = 0;
    // SAFETY: the fakes' pipeline call on a made-up device; the info, the descriptor and the
    // error slot are alive for the call.
    let pso = unsafe {
        MTLDevice_newRenderPipelineStateWithVertexDescriptor(
            DEVICE,
            &raw const info,
            &raw const vertex,
            &raw mut error,
        )
    };
    assert_ne!(pso, 0);
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert_eq!(
        calls,
        [
            "MTLDevice_newRenderPipelineStateWithVertexDescriptor",
            "  pso color0=0",
            "  attr 0 format=31 offset=0 buffer=0",
            "  attr 3 format=29 offset=16 buffer=1",
            "  layout 0 stride=20 step=1 rate=1",
            "  layout 1 stride=8 step=2 rate=3",
        ],
    );
}

/// Arming `fail_next_command_buffer` makes the next finished buffer latch `Queue::has_error`.
#[test]
fn fail_next_command_buffer_latches_queue_has_error() {
    const DEVICE: u64 = 0xf5;
    let mut device = Device::headless(DEVICE, QueueConfig::default());
    let queue = device.queue_mut();
    assert!(!queue.has_error());

    let seq1 = queue.current_seq();
    queue.commit();
    queue.wait_cpu(seq1);
    assert!(!queue.has_error());

    dxmt_fakes::fail_next_command_buffer(DEVICE);
    let seq2 = queue.current_seq();
    queue.commit();
    queue.wait_cpu(seq2);
    assert!(queue.has_error());
}
