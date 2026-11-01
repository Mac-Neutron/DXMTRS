use core::ptr::{self, NonNull};
use std::sync::Arc;

use winemetal_sys::{
    Obj,
    wire::{
        WMT_COMPUTE_COMMAND_DISPATCH_THREADS, WMT_COMPUTE_COMMAND_SET_BYTES,
        WMT_COMPUTE_COMMAND_SET_PSO, WMT_COMPUTE_COMMAND_SET_TEXTURE, WMT_LOAD_ACTION_LOAD,
        WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, WMT_PIXEL_FORMAT_RG16_FLOAT,
        WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE, WMT_RENDER_COMMAND_DRAW,
        WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_VISIBILITY_MODE,
        WMT_RENDER_COMMAND_USE_RESOURCE, WMT_RENDER_STAGE_FRAGMENT, WMT_RENDER_STAGE_VERTEX,
        WMT_RESOURCE_USAGE_READ, WMT_STORE_ACTION_STORE, WMT_TEXTURE_TYPE2D,
        WMT_VISIBILITY_RESULT_MODE_COUNTING, WMT_VISIBILITY_RESULT_MODE_DISABLED, WMTTextureInfo,
        WmtcmdBase, WmtcmdRenderDispatchThreadsPerTile, WmtcmdRenderSetpso,
        WmtcmdRenderSetvisibilitymode, WmtcmdRenderUseresource,
    },
};

use crate::{
    counter::Counter,
    deptrack::Access,
    encoder::{
        ArgumentBuffer, Encoder, NO_NEXT, RenderUses, kind,
        pass::{Pass, PassKind},
        stage,
    },
    frame_stats::CompatibilityFlags,
    query::{TimestampQuery, VisibilityQuery},
    resource::{Buffer, BufferAllocationFlags, Texture, TextureAllocationFlags},
    staging::Staging,
};

fn fake_device() -> Obj {
    // SAFETY: fake non-zero handle for the test binary's `NSObject_retain`/`NSObject_release`.
    unsafe { Obj::adopt(0xD9) }.expect("non-zero handle")
}

fn allocated_buffer(length: u64) -> Arc<Buffer> {
    let dev = fake_device();
    let buf = Arc::new(Buffer::new(length, &dev));
    let alloc = buf.allocate(BufferAllocationFlags::empty());
    buf.rename(alloc);
    buf
}

fn allocated_texture(pixel_format: u32, width: u32, height: u32) -> Arc<Texture> {
    let dev = fake_device();
    let bits = (WMT_TEXTURE_TYPE2D << WMTTextureInfo::TYPE_SHIFT)
        | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
        | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT);
    let info = WMTTextureInfo {
        pixel_format,
        width,
        height,
        depth: 1,
        array_length: 1,
        bits,
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let tex = Arc::new(Texture::new(info, &dev));
    let alloc = tex.allocate(TextureAllocationFlags::empty());
    tex.rename(alloc);
    tex
}

fn record_types(head: NonNull<WmtcmdBase>) -> Vec<u16> {
    let mut out = Vec::new();
    let mut at = head;
    loop {
        // SAFETY: each node in the pass's record chain starts with `WmtcmdBase`.
        let next = unsafe { at.as_ref() }.next.ptr;
        let Some(record) = NonNull::new(ptr::with_exposed_provenance_mut::<WmtcmdBase>(
            usize::try_from(next).expect("pointer width"),
        )) else {
            return out;
        };
        // SAFETY: as above.
        out.push(unsafe { record.as_ref() }.r#type);
        at = record;
    }
}

#[test]
fn current_seq_and_staging_and_subresource_access() {
    let dev = fake_device();
    let mut enc = Encoder::headless();
    enc.begin_chunk(42);
    assert_eq!(enc.current_seq(), 42);

    let staging = Staging::new(&dev, 64, 64, 64);
    let staging_alloc = enc
        .access_staging(&staging, Access::READ)
        .expect("staging allocation");
    assert_eq!(staging_alloc.last_retained_seq(), 42);

    let tex = allocated_texture(WMT_PIXEL_FORMAT_RG16_FLOAT, 16, 16);
    enc.start_compute_pass(None);
    let tex_alloc = enc
        .access_texture_subresource::<{ stage::COMPUTE }>(&tex, 0, 0, Access::READ)
        .expect("texture allocation");
    assert_eq!(tex_alloc.last_retained_seq(), 42);
    enc.end_pass();
}

#[test]
fn access_counter_allocates_tracks_and_updates() {
    let mut enc = Encoder::headless();
    enc.begin_chunk(7);
    enc.start_compute_pass(None);

    let counter = Arc::new(Counter::new());
    let uav_buf = allocated_buffer(256);
    enc.update_counter(&counter, 99);
    let (counter_alloc, offset) = enc
        .access_counter::<{ stage::COMPUTE }>(&counter, Some(&uav_buf))
        .expect("counter allocation");
    assert_ne!(counter_alloc.buffer(), 0);
    assert_eq!(offset % 4, 0);
    assert_eq!(counter_alloc.last_retained_seq(), 7);
    assert_eq!(uav_buf.current().expect("uav alloc").last_retained_seq(), 7);
    enc.end_pass();
}

#[test]
fn access_buffer_in_ordinary_marks_resident() {
    let mut enc = Encoder::headless();
    enc.begin_chunk(3);
    enc.start_render_pass(0, 0, 0, None);

    let buf = allocated_buffer(128);
    let (alloc, _) = enc
        .access_buffer_in::<{ stage::VERTEX }, { kind::ORDINARY }>(&buf, 0, 128, Access::READ)
        .expect("buffer allocation");

    let head = enc.pass().expect("active pass").records();
    // SAFETY: the first record after `head` is the `WmtcmdRenderUseresource` emitted by residency.
    let next = unsafe { head.as_ref() }.next.ptr;
    assert_ne!(next, 0, "ordinary access_buffer_in must emit useResource");
    let rec = ptr::with_exposed_provenance::<WmtcmdRenderUseresource>(
        usize::try_from(next).expect("pointer"),
    );
    // SAFETY: verified non-null `WmtcmdRenderUseresource` in the active pass.
    let u = unsafe { &*rec };
    assert_eq!(u.r#type, WMT_RENDER_COMMAND_USE_RESOURCE);
    assert_eq!(u.resource, alloc.buffer());
    assert_eq!(u.usage, WMT_RESOURCE_USAGE_READ);
    assert_eq!(u.stages, WMT_RENDER_STAGE_VERTEX);
    enc.end_pass();
}

#[test]
fn use_render_resource_encodes_command() {
    let mut enc = Encoder::headless();
    enc.start_render_pass(0, 0, 0, None);
    enc.use_render_resource(0xBEEF, WMT_RESOURCE_USAGE_READ, WMT_RENDER_STAGE_FRAGMENT);

    let head = enc.pass().expect("active pass").records();
    // SAFETY: `head` is the pass's sentinel record.
    let next = unsafe { head.as_ref() }.next.ptr;
    assert_ne!(next, 0);
    let rec = ptr::with_exposed_provenance::<WmtcmdRenderUseresource>(
        usize::try_from(next).expect("pointer"),
    );
    // SAFETY: `use_render_resource` just appended a `WmtcmdRenderUseresource`.
    let u = unsafe { &*rec };
    assert_eq!(u.r#type, WMT_RENDER_COMMAND_USE_RESOURCE);
    assert_eq!(u.resource, 0xBEEF);
    assert_eq!(u.usage, WMT_RESOURCE_USAGE_READ);
    assert_eq!(u.stages, WMT_RENDER_STAGE_FRAGMENT);
    enc.end_pass();
}

#[test]
fn allocate_temp_buffer_and_slice_from_ring() {
    let mut enc = Encoder::headless();
    enc.begin_chunk(1);
    let (buf, off0) = enc.allocate_temp_buffer(128, 16);
    assert_ne!(buf, 0);
    assert_eq!(off0, 0);

    let slice = enc.allocate_temp_buffer_slice(256, 64);
    assert_eq!(slice.buffer, buf);
    assert_eq!(slice.offset, 128);
}

#[test]
fn mapped_and_final_argument_buffer_in_pass() {
    let mut backing = [0_u64; 16];
    let mut enc = Encoder::headless();
    let len = size_of_val(&backing);
    // SAFETY: `backing` outlives `enc`.
    let argbuf =
        unsafe { ArgumentBuffer::new(NonNull::from(&mut backing).cast(), len, 0x88, 0x2000) };
    enc.start_render_pass(0, 0, 0, Some(argbuf));

    assert_eq!(enc.final_argument_buffer(), (0x88, 0x2000));
    assert_eq!(enc.final_argument_buffer_offset(24), 0x2000 + 24);

    let ptr = enc.mapped_argument_buffer(16, 8, 8).cast::<u64>();
    // SAFETY: offset 16 is `backing[2]`, aligned and in bounds.
    unsafe { ptr.write(0x1122_3344_5566_7788) };
    assert_eq!(backing[2], 0x1122_3344_5566_7788);
    enc.end_pass();
}

#[test]
fn index_buffer_offset_binding_and_reset() {
    let mut enc = Encoder::headless();
    enc.begin_chunk(1);
    enc.start_render_pass(0, 0, 0, None);

    let ib = allocated_buffer(256);
    let expected_handle = ib.current().expect("ib alloc").buffer();
    enc.bind_index_buffer(Some(Arc::clone(&ib)));
    enc.bind_index_buffer_offset(64);

    let (handle, alloc_off, sub_off) = enc
        .current_index_buffer_with_offset()
        .expect("bound index buffer");
    assert_eq!(handle, expected_handle);
    assert_eq!(alloc_off, 0);
    assert_eq!(sub_off, 64);

    enc.clear_state();
    enc.bind_index_buffer(Some(ib));
    let (_, _, sub_off_after_clear) = enc
        .current_index_buffer_with_offset()
        .expect("re-bound index buffer");
    assert_eq!(sub_off_after_clear, 0);
    enc.end_pass();
}

#[test]
fn visibility_queries_begin_bump_and_end() {
    let mut enc = Encoder::headless();
    enc.begin_chunk(5);
    enc.start_render_pass(0, 0, 0, None);

    let q = Arc::new(VisibilityQuery::new());
    enc.begin_visibility_query(Arc::clone(&q));
    enc.bump_visibility_result_offset();
    enc.end_visibility_query(q);
    enc.bump_visibility_result_offset();

    let uses = enc.render_targets().expect("render pass").uses;
    assert!(uses.contains(RenderUses::VISIBILITY_RESULT));

    let head = enc.pass().expect("active pass").records();
    // SAFETY: first record after `head` is the counting visibility mode command.
    let first_ptr = unsafe { head.as_ref() }.next.ptr;
    assert_ne!(first_ptr, 0);
    let first = ptr::with_exposed_provenance::<WmtcmdRenderSetvisibilitymode>(
        usize::try_from(first_ptr).expect("pointer"),
    );
    // SAFETY: valid `WmtcmdRenderSetvisibilitymode` record.
    let first = unsafe { &*first };
    assert_eq!(first.r#type, WMT_RENDER_COMMAND_SET_VISIBILITY_MODE);
    assert_eq!(first.mode, WMT_VISIBILITY_RESULT_MODE_COUNTING);
    assert_eq!(first.offset, 0);

    let second = ptr::with_exposed_provenance::<WmtcmdRenderSetvisibilitymode>(
        usize::try_from(first.next.ptr).expect("pointer"),
    );
    // SAFETY: second record is the disabled visibility mode command.
    let second = unsafe { &*second };
    assert_eq!(second.r#type, WMT_RENDER_COMMAND_SET_VISIBILITY_MODE);
    assert_eq!(second.mode, WMT_VISIBILITY_RESULT_MODE_DISABLED);
    assert_eq!(second.offset, 0);
    enc.end_pass();
}

#[test]
fn deferred_visibility_queries_stack() {
    let mut enc = Encoder::headless();
    let q0 = Arc::new(VisibilityQuery::new());
    let q1 = Arc::new(VisibilityQuery::new());
    let q2 = Arc::new(VisibilityQuery::new());
    let outer = [Arc::clone(&q0), Arc::clone(&q1)];
    let inner = [Arc::clone(&q2)];

    enc.push_deferred_visibility_queries(outer.as_ptr());
    assert!(Arc::ptr_eq(
        &enc.current_deferred_visibility_query(0).expect("q0"),
        &q0
    ));
    assert!(Arc::ptr_eq(
        &enc.current_deferred_visibility_query(1).expect("q1"),
        &q1
    ));

    enc.push_deferred_visibility_queries(inner.as_ptr());
    assert!(Arc::ptr_eq(
        &enc.current_deferred_visibility_query(0).expect("q2"),
        &q2
    ));

    enc.pop_deferred_visibility_queries();
    assert!(Arc::ptr_eq(
        &enc.current_deferred_visibility_query(1).expect("q1 again"),
        &q1
    ));

    enc.pop_deferred_visibility_queries();
    assert!(enc.current_deferred_visibility_query(0).is_none());
}

#[test]
fn sample_timestamp_query_adds_sample_pass() {
    let mut enc = Encoder::headless();
    enc.begin_chunk(1);
    assert!(!enc.last_samples_timestamp());

    enc.sample_timestamp_query(Arc::new(TimestampQuery::new()));
    assert!(enc.last_samples_timestamp());
    assert_eq!(enc.passes.len(), 1);

    // A second timestamp query with no intervening pass shares the sample pass.
    enc.sample_timestamp_query(Arc::new(TimestampQuery::new()));
    assert_eq!(enc.passes.len(), 1);
}

#[test]
fn gs_and_ts_dispatch_arguments_marshal_recorded() {
    let mut enc = Encoder::headless();
    enc.start_render_pass(0, 0, 0, None);
    enc.encode_gs_dispatch_arguments_marshal(0x10, 0x100, 32, 0x20, 0x200);
    enc.encode_ts_dispatch_arguments_marshal(0x30, 0x300, 4, 8, 0x40, 0x400);
    enc.end_pass();

    let Some(Pass {
        kind: PassKind::Render(targets),
        ..
    }) = enc.passes.first()
    else {
        panic!("expected a render pass");
    };
    assert_eq!(targets.gs_marshal_tasks.len(), 1);
    assert_eq!(targets.ts_marshal_tasks.len(), 1);
}

#[test]
fn depth_stencil_copy_from_buffer_encodes_render_pass() {
    let mut enc = Encoder::headless();
    enc.begin_chunk(1);
    let src = allocated_buffer(4096);
    let dst = allocated_texture(WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, 64, 32);

    enc.depth_stencil_copy_from_buffer(
        (&src, 0, 256, 8192),
        (&dst, 0, 0, 1),
        [0, 0, 0],
        [64, 32, 1],
    );

    let Some(Pass {
        kind: PassKind::Render(targets),
        records,
        ..
    }) = enc.passes.first()
    else {
        panic!("expected a render pass from depth_stencil_copy_from_buffer");
    };
    assert_eq!(targets.depth.target.load_action, WMT_LOAD_ACTION_LOAD);
    assert_eq!(targets.depth.target.store_action, WMT_STORE_ACTION_STORE);
    assert_eq!(targets.stencil.target.load_action, WMT_LOAD_ACTION_LOAD);
    assert_eq!(targets.stencil.target.store_action, WMT_STORE_ACTION_STORE);
    assert_eq!(targets.render_target_width, 64);
    assert_eq!(targets.render_target_height, 32);
    assert!(record_types(records.head).contains(&WMT_RENDER_COMMAND_DRAW));
}

#[test]
fn depth_stencil_copy_from_texture_encodes_compute_pass() {
    let mut enc = Encoder::headless();
    enc.begin_chunk(1);
    let src = allocated_texture(WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, 64, 32);
    let dst = allocated_texture(WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, 64, 32);

    enc.depth_stencil_copy_from_texture(
        (&src, 0, 0, [0, 0, 0], [64, 32, 1]),
        (&dst, 0, 0, 1, [0, 0, 0]),
    );

    let Some(Pass {
        kind: PassKind::Compute,
        records,
        ..
    }) = enc.passes.first()
    else {
        panic!("expected a compute pass from depth_stencil_copy_from_texture");
    };
    let types = record_types(records.head);
    assert!(types.contains(&WMT_COMPUTE_COMMAND_SET_PSO));
    assert!(types.contains(&WMT_COMPUTE_COMMAND_SET_TEXTURE));
    assert!(types.contains(&WMT_COMPUTE_COMMAND_SET_BYTES));
    assert!(types.contains(&WMT_COMPUTE_COMMAND_DISPATCH_THREADS));
}

#[test]
fn mv_scale_dispatch_encodes_compute_pass() {
    let mut enc = Encoder::headless();
    enc.begin_chunk(1);
    let src = allocated_texture(WMT_PIXEL_FORMAT_RG16_FLOAT, 128, 64);
    let dst = allocated_texture(WMT_PIXEL_FORMAT_RG16_FLOAT, 64, 32);

    enc.mv_scale_dispatch(&src, &dst, 0.5, -0.5);

    let Some(Pass {
        kind: PassKind::Compute,
        records,
        ..
    }) = enc.passes.first()
    else {
        panic!("expected a compute pass from mv_scale_dispatch");
    };
    let types = record_types(records.head);
    assert!(types.contains(&WMT_COMPUTE_COMMAND_SET_PSO));
    assert!(types.contains(&WMT_COMPUTE_COMMAND_SET_TEXTURE));
    assert!(types.contains(&WMT_COMPUTE_COMMAND_SET_BYTES));
    assert!(types.contains(&WMT_COMPUTE_COMMAND_DISPATCH_THREADS));
}

#[test]
fn dispatch_tile_barrier_encodes_and_recovers_pso() {
    let mut enc = Encoder::headless();
    enc.start_render_pass(0, 0, 1, None);
    enc.encode_render_command(WmtcmdRenderSetpso {
        r#type: WMT_RENDER_COMMAND_SET_PSO,
        reserved: [0; 3],
        next: NO_NEXT,
        pso: 0x4242,
    });
    enc.dispatch_tile_barrier();

    let head = enc.pass().expect("active pass").records();
    let types = record_types(head);
    assert_eq!(
        types,
        [
            WMT_RENDER_COMMAND_SET_PSO,
            WMT_RENDER_COMMAND_SET_PSO,
            WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE,
            WMT_RENDER_COMMAND_SET_PSO,
        ]
    );

    // SAFETY: `head` points to the render pass's dummy header node in `enc.heap`.
    let r1_addr = usize::try_from(unsafe { head.as_ref() }.next.ptr).expect("pointer");
    let r1 = ptr::with_exposed_provenance::<WmtcmdRenderSetpso>(r1_addr);
    // SAFETY: `r1` is the first `WmtcmdRenderSetpso` record in `enc.heap`.
    let r1_ref = unsafe { &*r1 };
    let r2_addr = usize::try_from(r1_ref.next.ptr).expect("pointer");
    let r2 = ptr::with_exposed_provenance::<WmtcmdRenderSetpso>(r2_addr);
    // SAFETY: `r2` is the second `WmtcmdRenderSetpso` record in `enc.heap`.
    let r2_ref = unsafe { &*r2 };
    let r3_addr = usize::try_from(r2_ref.next.ptr).expect("pointer");
    let r3 = ptr::with_exposed_provenance::<WmtcmdRenderDispatchThreadsPerTile>(r3_addr);
    // SAFETY: `r3` is the `WmtcmdRenderDispatchThreadsPerTile` record in `enc.heap`.
    let r3_ref = unsafe { &*r3 };
    let r4_addr = usize::try_from(r3_ref.next.ptr).expect("pointer");
    let r4 = ptr::with_exposed_provenance::<WmtcmdRenderSetpso>(r4_addr);
    // SAFETY: `r4` is the third `WmtcmdRenderSetpso` record in `enc.heap`.
    let r4_ref = unsafe { &*r4 };

    assert_eq!(r1_ref.pso, 0x4242);
    assert_ne!(r2_ref.pso, 0);
    assert_ne!(r2_ref.pso, 0x4242);
    assert_eq!(r3_ref.width, 16);
    assert_eq!(r3_ref.height, 16);
    assert_eq!(r4_ref.pso, 0x4242);
    enc.end_pass();
}

#[test]
fn frame_id_and_statistics_recorded() {
    let mut enc = Encoder::headless();
    enc.set_current_frame(7, 3);
    assert_eq!(enc.current_frame_id(), 7);
    enc.set_compatibility_flag(CompatibilityFlags::UNSUPPORTED_PREDICATION);
    let stats = enc.current_frame_statistics().expect("frame statistics");
    assert!(
        stats
            .compatibility_flags
            .contains(CompatibilityFlags::UNSUPPORTED_PREDICATION)
    );
}
