use mtld3d_core::{
    ids::{BufferId, TextureId},
    pipeline_key::{MAX_STREAMS, PipelineKey},
    streams::STREAM_FREQ_DEFAULT,
};
use mtld3d_types::{D3DSTREAMSOURCE_INDEXEDDATA, D3DSTREAMSOURCE_INSTANCEDATA};
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_UPDATE_FENCE, WMT_BLIT_COMMAND_WAIT_FOR_FENCE, WMT_PIXEL_FORMAT_BGRA8_UNORM,
    WMT_RENDER_COMMAND_UPDATE_FENCE, WMT_RENDER_COMMAND_WAIT_FOR_FENCE,
    WMT_RENDER_STAGE_PRE_RASTER,
};

use crate::device::{
    CoreDevice, DrawData, DrawRefused, IndexData, Indices, StreamData, TargetRef, TextureKind,
    fixture::{
        at, bound, core_on_fakes, draw, ff_binds, key, passes, target_desc, triangle,
        two_stream_key,
    },
    queue_profile_for,
};

/// The key of a default-state `XYZRHW | DIFFUSE` draw from a buffer bound 20 bytes apart.
fn bound_key() -> PipelineKey {
    let mut streams = [mtld3d_core::pipeline_key::StreamKey::Unused; MAX_STREAMS as usize];
    streams[0] = bound(20, STREAM_FREQ_DEFAULT);
    PipelineKey {
        streams,
        ..key(0xf)
    }
}

/// A draw's streams: the stored buffers `buffers` (an id and a byte offset) from stream 0 on.
fn stored(buffers: &[(BufferId, u32)]) -> [StreamData<'static>; MAX_STREAMS as usize] {
    let mut streams = [StreamData::None; MAX_STREAMS as usize];
    for (data, &(id, offset)) in streams.iter_mut().zip(buffers) {
        *data = StreamData::Buffer { id, offset };
    }
    streams
}

/// A stored buffer of `length` bytes, every byte uploaded.
fn filled(core: &mut CoreDevice, id: u64, length: usize) -> BufferId {
    let id = BufferId::from_raw(id);
    assert!(core.buffer_create(id, u64::try_from(length).expect("a length")));
    assert!(core.buffer_upload(id, 0, &vec![0x5a; length]));
    id
}

/// Each encoder of `calls` that uploads a buffer or draws, in order: its lines to before its end.
///
/// Encoders the core's initializer records (the back buffer's zeroing) are left out.
fn work(calls: &[String]) -> Vec<&[String]> {
    let mut found = Vec::new();
    let mut start = None;
    for (at, call) in calls.iter().enumerate() {
        if call.starts_with("MTLCommandBuffer_") && call.ends_with("CommandEncoder") {
            start = Some(at);
        } else if call == "MTLCommandEncoder_endEncoding"
            && let Some(from) = start.take()
        {
            let encoder = &calls[from..at];
            if encoder.iter().any(|c| {
                c.starts_with("  blit copy_to_buffer_from_buffer ")
                    || c.starts_with("  render 19 ")
                    || c.starts_with("  render 20 ")
            }) {
                found.push(encoder);
            }
        }
    }
    found
}

/// The fences `pass`'s render fence records of `command` name at its vertex stage.
fn vertex_fences(pass: &[String], command: u16) -> Vec<&str> {
    let prefix = format!("  render {command} fence=#");
    pass.iter()
        .filter_map(|c| c.strip_prefix(&prefix))
        .filter_map(|c| c.split_once(" stages="))
        .filter(|(_, stages)| stages.parse() == Ok(WMT_RENDER_STAGE_PRE_RASTER))
        .map(|(fence, _)| fence)
        .collect()
}

/// The fences `blit` names in its fence records of `command`.
fn blit_fences(blit: &[String], command: u16) -> Vec<&str> {
    let prefix = format!("  blit {command} fence=#");
    blit.iter()
        .filter_map(|c| c.strip_prefix(&prefix))
        .collect()
}

/// Whether `encoder` waits on any of `fences`, at any stage.
fn waits_on(encoder: &[String], fences: &[&str]) -> bool {
    let render = format!("  render {WMT_RENDER_COMMAND_WAIT_FOR_FENCE} fence=#");
    encoder
        .iter()
        .filter_map(|c| c.strip_prefix(&render).and_then(|c| c.split(' ').next()))
        .chain(blit_fences(encoder, WMT_BLIT_COMMAND_WAIT_FOR_FENCE))
        .any(|fence| fences.contains(&fence))
}

/// The first `prefix` line of `calls`, without the prefix.
fn first<'a>(calls: &'a [String], prefix: &str) -> &'a str {
    calls
        .iter()
        .find_map(|c| c.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("no {prefix:?} in {calls:#?}"))
}

/// The handle (`#<n>`) a buffer-to-buffer copy wrote, the `index`th of `calls`.
fn copied_into(calls: &[String], index: usize) -> String {
    let copies = calls
        .iter()
        .filter_map(|c| c.strip_prefix("  blit copy_to_buffer_from_buffer "))
        .collect::<Vec<_>>();
    let copy = copies.get(index).unwrap_or_else(|| panic!("{calls:#?}"));
    copy.split(' ').nth(2).expect("the destination").to_owned()
}

/// An upload is one copy at its offset and length, in a blit pass that ends the render pass.
#[test]
fn an_upload_blits_into_the_buffer_and_ends_the_render_pass() {
    const DEVICE: u64 = 0x3c4;
    let mut core = core_on_fakes(DEVICE);
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    let id = BufferId::from_raw(1);
    assert!(core.buffer_create(id, 64));
    assert!(core.has_buffer(id));
    assert!(core.buffer_upload(id, 8, &[1; 16]));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
    let end = at(&calls, "MTLCommandEncoder_endEncoding");
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    assert!(render < end && end < blit, "{calls:#?}");
    let copy = first(&calls, "  blit copy_to_buffer_from_buffer ");
    assert!(copy.ends_with(" dst_offset=8 length=16"), "{copy}");
}

/// An upload the store cannot copy is refused and records nothing.
///
/// An unknown or forgotten id, no bytes, a range past the buffer's end or past `u64`.
#[test]
fn an_upload_it_cannot_copy_records_nothing() {
    const DEVICE: u64 = 0x3c5;
    let mut core = core_on_fakes(DEVICE);
    let id = BufferId::from_raw(1);
    assert!(!core.buffer_upload(id, 0, &[1; 4]));
    assert!(core.buffer_create(id, 64));
    assert!(!core.buffer_upload(id, 0, &[]));
    assert!(!core.buffer_upload(id, 60, &[1; 8]));
    assert!(!core.buffer_upload(id, u64::MAX, &[1; 8]));
    core.forget_buffer(id);
    assert!(!core.has_buffer(id));
    assert!(!core.buffer_upload(id, 0, &[1; 4]));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        !calls
            .iter()
            .any(|c| c == "MTLCommandBuffer_blitCommandEncoder"),
        "{calls:#?}"
    );
}

/// Past the staged-bytes budget the chunk is committed.
///
/// Three 16 MiB uploads in a 32-bit profile (32 MiB budget) commit once, at the third.
#[test]
fn uploads_past_the_budget_commit() {
    const DEVICE: u64 = 0x3c6;
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { winemetal_sys::Obj::adopt(DEVICE) }.expect("a non-zero handle");
    let mut core = CoreDevice::new(mtl, queue_profile_for(32));
    let id = BufferId::from_raw(1);
    assert!(core.buffer_create(id, 16 << 20));
    let bytes = vec![0_u8; 16 << 20];
    let before = core.device().queue().current_seq();
    for _ in 0..3 {
        assert!(core.buffer_upload(id, 0, &bytes));
    }
    assert_eq!(core.device().queue().current_seq() - before, 1);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
}

/// A draw binds each stream's stored buffer at its slot and offset, and starts at its vertex.
#[test]
fn a_draw_binds_its_stored_buffers() {
    const DEVICE: u64 = 0x3c7;
    let mut core = core_on_fakes(DEVICE);
    let (vertices, colors) = (filled(&mut core, 1, 160), filled(&mut core, 2, 64));
    let mut streams = [mtld3d_core::pipeline_key::StreamKey::Unused; MAX_STREAMS as usize];
    streams[0] = bound(20, STREAM_FREQ_DEFAULT);
    streams[1] = bound(4, STREAM_FREQ_DEFAULT);
    let binds = ff_binds();
    let drawn = DrawData {
        streams: stored(&[(vertices, 20), (colors, 8)]),
        vertex_start: 2,
        ..draw(3, 3, None, &binds)
    };
    assert_eq!(core.draw(&two_stream_key(0, streams), &drawn), Ok(()));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let (vertex_buffer, color_buffer) = (copied_into(&calls, 0), copied_into(&calls, 1));
    let pass = passes(&calls)[0];
    for line in [
        format!("  render 2 buffer={vertex_buffer} offset=20 index=0"),
        format!("  render 2 buffer={color_buffer} offset=8 index=1"),
        "  render 19 primitive_type=3 vertex_start=2 vertex_count=3 instance_count=1 \
         base_instance=0"
            .to_owned(),
    ] {
        assert!(pass.contains(&line), "no {line:?} in {pass:#?}");
    }
}

/// An indexed draw reads its stored index buffer at its offset, from its base vertex.
///
/// A negative base too.
#[test]
fn an_indexed_draw_reads_its_stored_index_buffer() {
    const DEVICE: u64 = 0x3c8;
    let mut core = core_on_fakes(DEVICE);
    let (vertices, indices) = (filled(&mut core, 1, 160), filled(&mut core, 2, 32));
    let binds = ff_binds();
    for base_vertex in [5, -2] {
        let drawn = DrawData {
            streams: stored(&[(vertices, 0)]),
            base_vertex,
            ..draw(
                3,
                3,
                Some(Indices {
                    data: IndexData::Buffer {
                        id: indices,
                        offset: 6,
                    },
                    index_type: 0,
                    count: 3,
                }),
                &binds,
            )
        };
        assert_eq!(core.draw(&bound_key(), &drawn), Ok(()));
    }
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let index_buffer = copied_into(&calls, 1);
    let pass = passes(&calls)[0];
    for base_vertex in [5, -2] {
        let line = format!(
            "  render 20 primitive_type=3 index_type=0 index_count=3 index_buffer={index_buffer} \
             index_buffer_offset=6 instance_count=1 base_vertex={base_vertex} base_instance=0"
        );
        assert!(pass.contains(&line), "no {line:?} in {pass:#?}");
    }
}

/// An indexed draw takes stream 0's count only when a stream the stage reads is per instance.
///
/// Stream 1 (the colour, read) instanced: three instances; stream 2 (declared nowhere, so not
/// read) instanced alone: one.
#[test]
fn instancing_counts_only_a_stream_the_stage_reads() {
    const DEVICE: u64 = 0x3c9;
    let mut core = core_on_fakes(DEVICE);
    let (vertices, colors) = (filled(&mut core, 1, 160), filled(&mut core, 2, 64));
    let index_bytes = [0_u8, 0, 1, 0, 2, 0];
    let instanced = D3DSTREAMSOURCE_INSTANCEDATA | 1;
    let mut streams = [mtld3d_core::pipeline_key::StreamKey::Unused; MAX_STREAMS as usize];
    streams[0] = bound(20, D3DSTREAMSOURCE_INDEXEDDATA | 3);
    streams[1] = bound(4, instanced);
    let key = two_stream_key(0, streams);
    let binds = ff_binds();
    for mask in [0b010, 0b100] {
        let drawn = DrawData {
            streams: stored(&[(vertices, 0), (colors, 0)]),
            instancing: Some((D3DSTREAMSOURCE_INDEXEDDATA | 3, mask)),
            ..draw(
                3,
                3,
                Some(Indices {
                    data: IndexData::Bytes(&index_bytes),
                    index_type: 0,
                    count: 3,
                }),
                &binds,
            )
        };
        assert_eq!(core.draw(&key, &drawn), Ok(()));
    }
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let counts = calls
        .iter()
        .filter(|c| c.starts_with("  render 20 "))
        .filter_map(|c| c.split(" instance_count=").nth(1))
        .filter_map(|c| c.split(' ').next())
        .collect::<Vec<_>>();
    assert_eq!(counts, ["3", "1"], "{calls:#?}");
}

/// A stream the stage reads at an offset off four bytes refuses the draw before any record.
///
/// One the stage does not read draws at such an offset.
#[test]
fn an_odd_offset_refuses_only_a_stream_the_stage_reads() {
    const DEVICE: u64 = 0x3ca;
    let mut core = core_on_fakes(DEVICE);
    let vertices = filled(&mut core, 1, 160);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
    let binds = ff_binds();
    let odd = DrawData {
        streams: stored(&[(vertices, 2)]),
        ..draw(3, 3, None, &binds)
    };
    assert_eq!(core.draw(&bound_key(), &odd), Err(DrawRefused::Offset));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(passes(&calls).is_empty(), "{calls:#?}");
    let mut streams = stored(&[(vertices, 0)]);
    streams[2] = StreamData::Buffer {
        id: vertices,
        offset: 2,
    };
    let unread = DrawData {
        streams,
        ..draw(3, 3, None, &binds)
    };
    assert_eq!(core.draw(&bound_key(), &unread), Ok(()));
}

/// A forgotten buffer's recorded draw still binds it; a later draw naming it is refused.
#[test]
fn a_forgotten_buffers_recorded_draw_still_encodes() {
    const DEVICE: u64 = 0x3cb;
    let mut core = core_on_fakes(DEVICE);
    let vertices = filled(&mut core, 1, 160);
    let binds = ff_binds();
    let drawn = DrawData {
        streams: stored(&[(vertices, 0)]),
        ..draw(3, 3, None, &binds)
    };
    assert_eq!(core.draw(&bound_key(), &drawn), Ok(()));
    core.forget_buffer(vertices);
    assert!(!core.has_buffer(vertices));
    assert_eq!(core.draw(&bound_key(), &drawn), Err(DrawRefused::Buffer));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let vertex_buffer = copied_into(&calls, 0);
    let bind = format!("  render 2 buffer={vertex_buffer} offset=0 index=0");
    assert_eq!(
        calls.iter().filter(|c| **c == bind).count(),
        1,
        "{calls:#?}"
    );
}

/// A stored buffer the store does not hold refuses the draw, a vertex or an index buffer.
#[test]
fn an_unknown_buffer_refuses_the_draw() {
    const DEVICE: u64 = 0x3cc;
    let mut core = core_on_fakes(DEVICE);
    let vertices = filled(&mut core, 1, 160);
    let binds = ff_binds();
    let unknown = DrawData {
        streams: stored(&[(BufferId::from_raw(9), 0)]),
        ..draw(3, 3, None, &binds)
    };
    assert_eq!(core.draw(&bound_key(), &unknown), Err(DrawRefused::Buffer));
    let unknown_indices = DrawData {
        streams: stored(&[(vertices, 0)]),
        ..draw(
            3,
            3,
            Some(Indices {
                data: IndexData::Buffer {
                    id: BufferId::from_raw(9),
                    offset: 0,
                },
                index_type: 0,
                count: 3,
            }),
            &binds,
        )
    };
    assert_eq!(
        core.draw(&bound_key(), &unknown_indices),
        Err(DrawRefused::Buffer)
    );
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(passes(&calls).is_empty(), "{calls:#?}");
}

/// A draw of three vertices from the stored buffer `vertices`, indexed from `indices` if any.
fn draw_from(core: &mut CoreDevice, vertices: BufferId, indices: Option<BufferId>) {
    let binds = ff_binds();
    let indices = indices.map(|id| Indices {
        data: IndexData::Buffer { id, offset: 0 },
        index_type: 0,
        count: 3,
    });
    let drawn = DrawData {
        streams: stored(&[(vertices, 0)]),
        ..draw(3, 3, indices, &binds)
    };
    assert_eq!(core.draw(&bound_key(), &drawn), Ok(()));
}

/// Draw, upload, draw of one buffer, ordered by fences: the two draws' passes and the blit's.
///
/// The blit waits on a fence the first render pass's vertex half updates, and the second render
/// pass waits before its vertex stage on a fence the blit updates.
fn assert_ordered(first: &[String], blit: &[String], second: &[String]) {
    let read = vertex_fences(first, WMT_RENDER_COMMAND_UPDATE_FENCE);
    assert!(
        !read.is_empty(),
        "the first pass's vertex half updates: {first:#?}"
    );
    assert!(
        waits_on(blit, &read),
        "the upload waits for the draw before it: {first:#?} {blit:#?}"
    );
    let written = blit_fences(blit, WMT_BLIT_COMMAND_UPDATE_FENCE);
    let awaited = vertex_fences(second, WMT_RENDER_COMMAND_WAIT_FOR_FENCE);
    assert!(
        awaited.iter().any(|fence| written.contains(fence)),
        "the draw after waits for the upload before its vertex stage: {blit:#?} {second:#?}"
    );
}

/// An upload between two draws of its vertex buffer is ordered after the first, before the second.
#[test]
fn an_upload_is_ordered_between_the_draws_of_its_buffer() {
    const DEVICE: u64 = 0x3cd;
    let mut core = core_on_fakes(DEVICE);
    let vertices = filled(&mut core, 1, 160);
    draw_from(&mut core, vertices, None);
    assert!(core.buffer_upload(vertices, 0, &[0xa5; 60]));
    draw_from(&mut core, vertices, None);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let all = work(&calls);
    // The first upload's blit, the first draw's pass, the second upload's blit, the second's.
    assert_eq!(all.len(), 4, "{calls:#?}");
    assert_ordered(all[1], all[2], all[3]);
}

/// An upload between two draws of its index buffer is ordered after the first, before the second.
#[test]
fn an_upload_is_ordered_between_the_draws_of_its_index_buffer() {
    const DEVICE: u64 = 0x3ce;
    let mut core = core_on_fakes(DEVICE);
    let (vertices, indices) = (filled(&mut core, 1, 160), filled(&mut core, 2, 32));
    draw_from(&mut core, vertices, Some(indices));
    assert!(core.buffer_upload(indices, 0, &[0; 6]));
    draw_from(&mut core, vertices, Some(indices));
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let all = work(&calls);
    // The two first uploads share a blit pass.
    assert_eq!(all.len(), 4, "{calls:#?}");
    assert_ordered(all[1], all[2], all[3]);
}

/// An upload that commits its chunk is ordered too: the draw after it waits in the next one.
///
/// A 17 MiB buffer in a 32-bit profile (32 MiB budget): its second whole upload commits.
#[test]
fn an_upload_that_commits_is_ordered_across_the_commit() {
    const DEVICE: u64 = 0x3cf;
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { winemetal_sys::Obj::adopt(DEVICE) }.expect("a non-zero handle");
    let mut core = CoreDevice::new(mtl, queue_profile_for(32));
    core.attach(1, 64, 32, 1);
    let vertices = filled(&mut core, 1, 17 << 20);
    draw_from(&mut core, vertices, None);
    let before = core.device().queue().current_seq();
    assert!(core.buffer_upload(vertices, 0, &vec![0xa5; 17 << 20]));
    assert_eq!(
        core.device().queue().current_seq() - before,
        1,
        "the upload commits"
    );
    draw_from(&mut core, vertices, None);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let all = work(&calls);
    assert_eq!(all.len(), 4, "{calls:#?}");
    assert_ordered(all[1], all[2], all[3]);
    // The second draw's pass is in the command buffer after the blit's.
    let blit = calls
        .iter()
        .rposition(|c| c == "MTLCommandBuffer_blitCommandEncoder")
        .expect("a blit");
    let commit = blit + at(&calls[blit..], "MTLCommandBuffer_commit");
    let render = calls
        .iter()
        .rposition(|c| c == "MTLCommandBuffer_renderCommandEncoder")
        .expect("a render pass");
    assert!(blit < commit && commit < render, "{calls:#?}");
}

/// An upload 33 render passes (66 encoder ids) after a draw of its buffer is still ordered.
///
/// Past 64 ids the read has no wait of its own: the blit, or a pass between, waits on the first
/// draw's vertex fence through the window of passes 64 to 127 ids back. The passes between draw
/// to 33 targets of their own, so none merges into another.
#[test]
fn an_upload_far_after_a_draw_of_its_buffer_is_ordered() {
    const DEVICE: u64 = 0x3d0;
    let mut core = core_on_fakes(DEVICE);
    let targets = (0..33)
        .map(|n| TextureId::from_raw(0x100 + n))
        .collect::<Vec<_>>();
    for &id in &targets {
        core.texture_create(
            id,
            &target_desc(WMT_PIXEL_FORMAT_BGRA8_UNORM, TextureKind::Tex2d, 64, 1, 4),
        );
    }
    let vertices = filled(&mut core, 1, 160);
    draw_from(&mut core, vertices, None);
    let binds = ff_binds();
    for &id in &targets {
        assert!(core.set_target(TargetRef::Texture {
            id,
            level: 0,
            face: None,
        }));
        assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    }
    assert!(core.set_target(TargetRef::BackBuffer));
    assert!(core.buffer_upload(vertices, 0, &[0xa5; 60]));
    draw_from(&mut core, vertices, None);
    core.drain();
    let calls = dxmt_fakes::take_calls(DEVICE);
    let all = work(&calls);
    // The first upload, the first draw, 33 passes of their own, the upload, the last draw.
    assert_eq!(all.len(), 37, "{calls:#?}");
    assert!(
        all[1..35]
            .iter()
            .all(|encoder| encoder[0] == "MTLCommandBuffer_renderCommandEncoder"),
        "{calls:#?}"
    );
    let read = vertex_fences(all[1], WMT_RENDER_COMMAND_UPDATE_FENCE);
    assert!(!read.is_empty(), "{:#?}", all[1]);
    assert!(
        all[2..=35].iter().any(|encoder| waits_on(encoder, &read)),
        "the upload, or a pass before it, waits for the first draw: {calls:#?}"
    );
    let written = blit_fences(all[35], WMT_BLIT_COMMAND_UPDATE_FENCE);
    let awaited = vertex_fences(all[36], WMT_RENDER_COMMAND_WAIT_FOR_FENCE);
    assert!(
        awaited.iter().any(|fence| written.contains(fence)),
        "{calls:#?}"
    );
}
