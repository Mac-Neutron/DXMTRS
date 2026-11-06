//! Unit tests for `queue`: `order_as_cpp`, `plan_merge_as_cpp`, `serial_unless_overlap`,
//!
//! `timestamp_folding_as_cpp`, `cmdbuf_slots_clamped`, `execute_allocates_as_cpp`, and
//! `framegen_order_joins`.

use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};

use dxmt_core::presenter::framegen::Order as FrameGenOrderTrait;
use winemetal_sys::{
    Obj,
    wire::{
        MemPtr, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER, WMT_BLIT_COMMAND_FILL_BUFFER,
        WMT_BLIT_COMMAND_UPDATE_FENCE, WMT_BLIT_COMMAND_WAIT_FOR_FENCE, WMT_LOAD_ACTION_CLEAR,
        WMT_LOAD_ACTION_DONT_CARE, WMT_LOAD_ACTION_LOAD, WMT_RENDER_COMMAND_DRAW,
        WMT_RENDER_COMMAND_SET_PSO, WMT_STORE_ACTION_DONT_CARE, WMT_STORE_ACTION_STORE,
        WMTBufferInfo, WMTClearColor, WMTColorAttachmentInfo, WMTDepthAttachmentInfo,
        WMTRenderPassInfo, WMTRenderPassSampleBufferInfo, WMTSampleBufferAttachmentInfo,
        WMTStencilAttachmentInfo, WmtcmdBase, WmtcmdBlitCopyFromBufferToBuffer, WmtcmdBlitFenceOp,
        WmtcmdBlitFillbuffer, WmtcmdBlitNop, WmtcmdComputeNop, WmtcmdRenderDraw, WmtcmdRenderNop,
        WmtcmdRenderSetpso,
    },
};

use super::{
    Queue,
    merge::{MergePlan, apply_clear, clear_target, same_target},
    order::{FENCE_COUNT, FrameGenOrder, NO_FENCE, OrderState, draws, serial_from_env},
    slots_from_env,
    timestamp::{DONT_SAMPLE, LoneReason, RingSampling, TimestampState},
};
use crate::{
    device::Device,
    encoder::{
        BarrierOpacity, BlitEncoderData, ClearEncoderData, EncoderData, EncoderSample, EncoderType,
        RenderEncoderColorAttachmentData, RenderEncoderData, RenderEncoderDepthAttachmentData,
        RenderEncoderStencilAttachmentData, Submission,
    },
    list::List,
    tests::count_allocs,
};

thread_local! {
    static RECORD_BLIT: Cell<bool> = const { Cell::new(false) };
    static BLIT_OPS: RefCell<Vec<(u64, u16, u64)>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(export_name = "MTLDevice_newBuffer")]
extern "C-unwind" fn fake_new_buffer(_device: u64, _info: *mut WMTBufferInfo) -> u64 {
    0x9000
}

#[unsafe(export_name = "MTLCommandQueue_commandBuffer")]
extern "C-unwind" fn fake_command_buffer(_queue: u64) -> u64 {
    0x7000
}

#[unsafe(export_name = "MTLCommandBuffer_blitCommandEncoder")]
extern "C-unwind" fn fake_blit_encoder(_cmdbuf: u64) -> u64 {
    0x8000
}

#[unsafe(export_name = "MTLCommandBuffer_blitCommandEncoderWithSampleBuffers")]
extern "C-unwind" fn fake_blit_encoder_samples(
    _cmdbuf: u64,
    _attachments: *mut WMTSampleBufferAttachmentInfo,
    _count: u64,
) -> u64 {
    0x8001
}

#[unsafe(export_name = "MTLCommandEncoder_endEncoding")]
extern "C-unwind" fn fake_end_encoding(_encoder: u64) {}

#[unsafe(export_name = "MTLCommandEncoder_setLabel")]
extern "C-unwind" fn fake_set_label(_encoder: u64, _label: u64) {}

#[unsafe(export_name = "MTLCommandBuffer_commit")]
extern "C-unwind" fn fake_commit(_cmdbuf: u64) {}

#[unsafe(export_name = "MTLBlitCommandEncoder_encodeCommands")]
extern "C-unwind" fn fake_blit_encode_commands(encoder: u64, cmd_head: *const WmtcmdBase) {
    let record = RECORD_BLIT.try_with(Cell::get).unwrap_or(false);
    if !record {
        return;
    }
    let mut cur = cmd_head;
    while !cur.is_null() {
        // SAFETY: `cur` points to a valid `WmtcmdBase` node in the caller's command chain.
        let base = unsafe { *cur };
        let arg = match base.r#type {
            WMT_BLIT_COMMAND_WAIT_FOR_FENCE | WMT_BLIT_COMMAND_UPDATE_FENCE => {
                // SAFETY: `cur` has type `WAIT_FOR_FENCE` or `UPDATE_FENCE` (`WmtcmdBlitFenceOp`).
                unsafe { (*cur.cast::<WmtcmdBlitFenceOp>()).fence }
            }
            WMT_BLIT_COMMAND_FILL_BUFFER => {
                // SAFETY: `cur` has type `FILL_BUFFER` (`WmtcmdBlitFillbuffer`).
                unsafe { (*cur.cast::<WmtcmdBlitFillbuffer>()).buffer }
            }
            _ => 0,
        };
        BLIT_OPS.with(|ops| ops.borrow_mut().push((encoder, base.r#type, arg)));
        cur = base.next.ptr as *const WmtcmdBase;
    }
}

fn empty_render_pass_info() -> WMTRenderPassInfo {
    WMTRenderPassInfo {
        colors: [WMTColorAttachmentInfo {
            texture: 0,
            load_action: WMT_LOAD_ACTION_DONT_CARE,
            store_action: WMT_STORE_ACTION_STORE,
            level: 0,
            slice: 0,
            depth_plane: 0,
            clear_color: WMTClearColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            resolve_texture: 0,
            resolve_level: 0,
            resolve_slice: 0,
            resolve_depth_plane: 0,
        }; 8],
        depth: WMTDepthAttachmentInfo {
            texture: 0,
            load_action: WMT_LOAD_ACTION_CLEAR,
            store_action: WMT_STORE_ACTION_DONT_CARE,
            level: 0,
            slice: 0,
            depth_plane: 0,
            clear_depth: 1.0,
        },
        stencil: WMTStencilAttachmentInfo {
            texture: 0,
            load_action: WMT_LOAD_ACTION_CLEAR,
            store_action: WMT_STORE_ACTION_DONT_CARE,
            level: 0,
            slice: 0,
            depth_plane: 0,
            clear_stencil: 0,
        },
        default_raster_sample_count: 1,
        render_target_array_length: 0,
        tile_width: 0,
        tile_height: 0,
        render_target_height: 1080,
        render_target_width: 1920,
        visibility_accumulate: 0,
        visibility_buffer: 0,
        sample_buffers: [WMTRenderPassSampleBufferInfo {
            sample_buffer: 0,
            end_of_fragment_sample_index: DONT_SAMPLE,
            start_of_vertex_sample_index: DONT_SAMPLE,
        }; 4],
    }
}

fn make_clear_encoder(position: u32) -> ClearEncoderData {
    let mut base = EncoderData::new(EncoderType::Clear, u64::from(position));
    base.position = position;
    base.writes_unknown = false;
    ClearEncoderData {
        base,
        color: WMTClearColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
        depth_stencil: (1.0, 0),
        attachment: None,
        clear_dsv: 0,
        array_length: 0,
        width: 1920,
        height: 1080,
        depth_plane: 0,
    }
}

fn make_render_encoder(position: u32, join: bool) -> RenderEncoderData {
    let mut base = EncoderData::new(EncoderType::Render, u64::from(position));
    base.position = position;
    base.join = join;
    base.writes_unknown = false;
    RenderEncoderData {
        base,
        colors: core::array::from_fn(|_| RenderEncoderColorAttachmentData::default()),
        depth: RenderEncoderDepthAttachmentData::default(),
        stencil: RenderEncoderStencilAttachmentData::default(),
        default_raster_sample_count: 1,
        render_target_array_length: 0,
        render_target_height: 1080,
        render_target_width: 1920,
        cmd_head: WmtcmdRenderNop {
            r#type: 0,
            reserved: [0; 3],
            next: MemPtr::NULL,
        },
        cmd_tail: ptr::null_mut(),
        dsv_planar_flags: 0,
        dsv_readonly_flags: 0,
        render_target_count: 0,
        use_visibility_result: false,
        visibility_buffer: 0,
        use_tessellation: false,
        use_geometry: false,
        uav_writes: false,
        pre_head: WmtcmdComputeNop {
            r#type: 0,
            reserved: [0; 3],
            next: MemPtr::NULL,
        },
        pre_tail: ptr::null_mut(),
    }
}

fn make_blit_encoder(position: u32) -> BlitEncoderData {
    let mut base = EncoderData::new(EncoderType::Blit, u64::from(position));
    base.position = position;
    base.writes_unknown = false;
    BlitEncoderData {
        base,
        cmd_head: WmtcmdBlitNop {
            r#type: 0,
            reserved: [0; 3],
            next: MemPtr::NULL,
        },
        cmd_tail: ptr::null_mut(),
    }
}

#[test]
fn order_as_cpp() {
    let mut state = OrderState::new(false);
    for i in 0..FENCE_COUNT {
        state.fence_handles[i] = 1000 + i as u64;
    }

    // Encoder 0: a drawing render pass with `join = true` in overlap mode (`!serial`) allocates
    // both its own fence (0) and an early fence (1), setting `group_wait = early = 1`.
    let mut draw_cmd = WmtcmdRenderDraw {
        r#type: WMT_RENDER_COMMAND_DRAW,
        reserved: [0; 3],
        next: MemPtr::NULL,
        primitive_type: 3,
        vertex_start: 0,
        vertex_count: 3,
        instance_count: 1,
        base_instance: 0,
    };
    let mut non_draw_cmd = WmtcmdRenderSetpso {
        r#type: WMT_RENDER_COMMAND_SET_PSO,
        reserved: [0; 3],
        next: MemPtr {
            ptr: (&raw mut draw_cmd) as _,
        },
        pso: 1,
    };
    let mut r0 = make_render_encoder(0, true);
    r0.cmd_head.next.ptr = (&raw mut non_draw_cmd) as _;
    assert!(draws(&r0));

    let f0 = state.order(Some(&r0.base), false);
    assert_eq!(f0, 0);
    assert_eq!(state.early, 1);
    assert_eq!(state.group_wait, 1);
    assert_eq!(state.next_fence, 2);
    assert_eq!(state.waits, [] as [u16; 0]);
    assert_eq!(state.frontier, vec![0]);

    // Encoder 1: non-join blit encoder at position 1 with no extra deps waits only on `group_wait` (1).
    let mut b1 = make_blit_encoder(1);
    b1.base.join = false;
    let f1 = state.order(Some(&b1.base), false);
    assert_eq!(f1, 2);
    assert_eq!(state.early, NO_FENCE);
    assert_eq!(state.waits, vec![1]);
    assert_eq!(state.frontier, vec![0, 2]);

    // Encoder 2: non-join blit encoder at position 2 depending on position 1 waits on `[group_wait, list_fences[1]]` = `[1, 2]`.
    let deps = [1u32];
    let mut b2 = make_blit_encoder(2);
    b2.base.join = false;
    b2.base.deps = deps.as_ptr();
    b2.base.dep_count = 1;
    let f2 = state.order(Some(&b2.base), false);
    assert_eq!(f2, 3);
    assert_eq!(state.waits, vec![1, 2]);
    assert_eq!(state.frontier, vec![0, 2, 3]);

    // Encoder 3: a join encoder waits on the entire frontier `[0, 2, 3]` and resets the frontier to `[f3]`.
    let mut b3 = make_blit_encoder(3);
    b3.base.join = true;
    let f3 = state.order(Some(&b3.base), false);
    assert_eq!(f3, 4);
    assert_eq!(state.waits, vec![0, 2, 3]);
    assert_eq!(state.frontier, vec![4]);
    assert_eq!(state.group_wait, 4);

    // Queue's own work (`None`) joins the frontier `[4]`, sets `list_join = u32::MAX`, and a
    // subsequent non-join encoder at position 4 adopts `list_join = 4` and ignores earlier deps `< 4`.
    let fq = state.order(None, true);
    assert_eq!(fq, 5);
    assert_eq!(state.waits, vec![4]);
    assert_eq!(state.list_join, u32::MAX);

    let old_deps = [1u32];
    let mut b4 = make_blit_encoder(4);
    b4.base.join = false;
    b4.base.deps = old_deps.as_ptr();
    b4.base.dep_count = 1;
    let f4 = state.order(Some(&b4.base), false);
    assert_eq!(f4, 6);
    assert_eq!(state.list_join, 4);
    assert_eq!(state.waits, vec![5]);
}

#[test]
fn plan_merge_as_cpp() {
    let tex_key = ptr::without_provenance::<c_void>(0xA000);
    let other_key = ptr::without_provenance::<c_void>(0xB000);

    // Build a submission chain:
    // Clear (0) -> Render (1) -> Timestamp-only Blit (2) -> Render (3, across crossable barrier) -> Render (4, across opaque barrier).
    let mut clear = make_clear_encoder(0);
    clear.base.write_count = 1;
    clear.base.writes[0] = tex_key;
    clear.clear_dsv = 1;
    clear.depth_stencil = (0.5, 0);

    let mut r1 = make_render_encoder(1, true);
    r1.base.write_count = 1;
    r1.base.writes[0] = tex_key;
    r1.dsv_planar_flags = 1;
    r1.depth.load_action = WMT_LOAD_ACTION_LOAD;
    r1.depth.store_action = WMT_STORE_ACTION_STORE;

    let mut ts_blit = make_blit_encoder(2);
    ts_blit.base.timestamp_only = true;
    ts_blit.base.num_samples = 1;
    ts_blit.base.samples[0] = EncoderSample {
        buffer: 0x55,
        index: 7,
        value: ptr::null_mut(),
    };

    let mut r2 = make_render_encoder(3, false);
    r2.base.write_count = 1;
    r2.base.writes[0] = tex_key;
    r2.dsv_planar_flags = 1;
    r2.depth.load_action = WMT_LOAD_ACTION_LOAD;
    r2.depth.store_action = WMT_STORE_ACTION_STORE;
    r2.base.barriers = 1;
    r2.base.barriers_last = 1;

    let mut r3 = make_render_encoder(4, false);
    r3.base.write_count = 1;
    r3.base.writes[0] = tex_key;
    r3.dsv_planar_flags = 1;
    r3.depth.load_action = WMT_LOAD_ACTION_LOAD;
    r3.depth.store_action = WMT_STORE_ACTION_STORE;
    r3.base.barriers = 2;
    r3.base.barriers_last = 2;

    clear.base.next = &raw mut r1.base;
    r1.base.next = &raw mut ts_blit.base;
    ts_blit.base.next = &raw mut r2.base;
    r2.base.next = &raw mut r3.base;

    let mut sub = Submission::new();
    sub.entry = &raw mut clear.base;
    sub.encoder_count = 5;
    sub.barrier_count = 2;
    // Barrier 1 names `tex_key` (the render target attachment written by the passes -> crossable!).
    sub.barrier_names.push((1, tex_key));
    // Barrier 2 is an always-opaque barrier (e.g. `SplitEnd`) -> refuses crossing!
    sub.barrier_opaque.push((2, BarrierOpacity::SplitEnd));
    sub.barrier_names.push((2, other_key));

    assert!(same_target(&r1, &r2));
    assert!(same_target(&r2, &r3));

    let subs = [&sub];
    // SAFETY: `clear` and `subs` point to stack-allocated encoders valid for this scope.
    let target = unsafe { clear_target(&raw const clear, 0, &subs) };
    assert_eq!(target, Some((0, &raw mut r1)));

    let mut info = empty_render_pass_info();
    apply_clear(&clear, &r1, &mut info);
    assert_eq!(info.depth.load_action, WMT_LOAD_ACTION_CLEAR);
    assert_eq!(info.depth.clear_depth, 0.5);

    let mut plan = MergePlan::new(true, true);
    // SAFETY: `r1` and `subs` point to stack-allocated encoders valid for this scope.
    unsafe { plan.plan_merge(&raw mut r1, 0, &subs, &mut info, true, false) };

    // `plan.merged` contains `ts_blit` and `r2`, stopping before `r3` because barrier 2 is opaque.
    assert_eq!(plan.merged, vec![&raw mut ts_blit.base, &raw mut r2.base]);
    assert_eq!(info.sample_buffers[0].sample_buffer, 0x55);
    assert_eq!(info.sample_buffers[0].end_of_fragment_sample_index, 7);
}

#[test]
fn serial_unless_overlap() {
    assert!(serial_from_env(None, None));
    assert!(serial_from_env(Some(""), None));
    assert!(serial_from_env(Some("0"), None));
    assert!(!serial_from_env(Some("1"), None));
    assert!(!serial_from_env(Some("1"), Some("0")));
    assert!(serial_from_env(Some("1"), Some("1")));
    assert!(serial_from_env(None, Some("1")));
}

#[test]
fn timestamp_folding_as_cpp() {
    let mut ts = TimestampState::new();

    // Queue a pending sample on buffer `0xAAA`.
    ts.pending_samples.push(EncoderSample {
        buffer: 0xAAA,
        index: 3,
        value: ptr::null_mut(),
    });

    // When the next encoder uses the same counter sample buffer `0xAAA`, `take_pending` folds it
    // into the encoder's start without flushing a lone blit.
    let mut flushed = Vec::new();
    let (start_buf, start_idx) = ts.take_pending(0xAAA, |buf, idx, why| {
        flushed.push((buf, idx, why as u8));
    });
    assert_eq!(flushed, [] as [(u64, u64, u8); 0]);
    assert_eq!((start_buf, start_idx), (0xAAA, 3));
    assert_eq!(ts.pending_samples.len(), 0);

    // Multiple pending samples: all but the last are flushed as `LoneReason::Several`, and if the
    // next encoder uses a different buffer, the last is also flushed as `LoneReason::CounterBuffer`.
    ts.pending_samples.push(EncoderSample {
        buffer: 0xAAA,
        index: 5,
        value: ptr::null_mut(),
    });
    ts.pending_samples.push(EncoderSample {
        buffer: 0xAAA,
        index: 6,
        value: ptr::null_mut(),
    });
    let (start_buf2, start_idx2) = ts.take_pending(0xCCC, |buf, idx, why| {
        flushed.push((buf, idx, why as u8));
    });
    assert_eq!((start_buf2, start_idx2), (0, DONT_SAMPLE));
    assert_eq!(
        flushed,
        vec![
            (0xAAA, 5, LoneReason::Several as u8),
            (0xAAA, 6, LoneReason::CounterBuffer as u8)
        ]
    );

    // Shared counter ring (`Point`, `TakeStart`, `Trailing`, `Attach`):
    // Pending start sample + encoder's own sample + trailing timestamp-only blit before a Clear
    // fold into a single attachment `(ring_samples, start_slot = 0, end_slot = 1)`.
    let mut val_start = 0u64;
    let mut val_own = 0u64;
    let mut val_trail = 0u64;
    ts.pending_samples.push(EncoderSample {
        buffer: 0xAAA,
        index: 0,
        value: &raw mut val_start,
    });

    let mut blit = make_blit_encoder(0);
    blit.base.num_samples = 1;
    blit.base.samples[0] = EncoderSample {
        buffer: 0xAAA,
        index: 1,
        value: &raw mut val_own,
    };

    let mut trailing_ts = make_blit_encoder(1);
    trailing_ts.base.timestamp_only = true;
    trailing_ts.base.num_samples = 1;
    trailing_ts.base.samples[0] = EncoderSample {
        buffer: 0xAAA,
        index: 2,
        value: &raw mut val_trail,
    };

    let mut clear_after = make_clear_encoder(2);
    trailing_ts.base.next = &raw mut clear_after.base;
    blit.base.next = &raw mut trailing_ts.base;

    let mut sub = Submission::new();
    sub.entry = &raw mut blit.base;
    let subs = [&sub];

    let mut sample_first = 0u64;
    let mut sample_count = 0u32;
    let mut sample_values = Vec::new();
    let mut ring = RingSampling {
        samples: 0xF000,
        ring_done: 0,
        sample_first: &mut sample_first,
        sample_count: &mut sample_count,
        sample_values: &mut sample_values,
    };

    // SAFETY: `blit` and `subs` point to stack-allocated encoders valid for this scope.
    let (att, num_att, owed) = unsafe { ts.attach(&raw mut blit.base, 0, &subs, true, &mut ring) };
    assert_eq!(num_att, 1);
    assert!(owed);
    assert_eq!(att.sample_buffer, 0xF000);
    assert_eq!(att.start_of_encoder_sample_index, 0);
    assert_eq!(att.end_of_encoder_sample_index, 1);
    assert_eq!(sample_count, 2);
    assert_eq!(sample_values.len(), 3);
    assert!(ts.taken);
    assert_eq!(ts.taken_stop, &raw mut clear_after.base);
}

#[test]
fn cmdbuf_slots_clamped() {
    assert_eq!(slots_from_env(None), 64);
    assert_eq!(slots_from_env(Some("")), 64);
    assert_eq!(slots_from_env(Some("abc")), 64);
    assert_eq!(slots_from_env(Some("0")), 1);
    assert_eq!(slots_from_env(Some("-8")), 64);
    assert_eq!(slots_from_env(Some("1")), 1);
    assert_eq!(slots_from_env(Some("16")), 16);
    assert_eq!(slots_from_env(Some("64")), 64);
    assert_eq!(slots_from_env(Some("65")), 64);
    assert_eq!(slots_from_env(Some("999")), 64);
}

fn make_fake_list(
    dev: *mut Device,
    blits: &mut [BlitEncoderData],
    cmds: &mut [WmtcmdBlitCopyFromBufferToBuffer],
) -> Box<List> {
    if !dev.is_null() {
        // SAFETY: `dev` is a live `Device` COM object; `List::drop` releases one reference.
        unsafe { crate::com::add_ref_com(dev) };
    }
    let mut list = List::new_closed(dev);
    let n = blits.len();
    for i in 0..n {
        cmds[i] = WmtcmdBlitCopyFromBufferToBuffer {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
            reserved: [0; 3],
            next: MemPtr::NULL,
            src: 0x1000,
            src_offset: 0,
            dst: 0x2000,
            dst_offset: 0,
            copy_length: 64,
        };
        blits[i] = make_blit_encoder(u32::try_from(i).unwrap_or(0));
        blits[i].cmd_head = WmtcmdBlitNop {
            r#type: 0,
            reserved: [0; 3],
            next: MemPtr {
                ptr: (&raw mut cmds[i]) as _,
            },
        };
        blits[i].cmd_tail = (&raw mut cmds[i]).cast::<WmtcmdBase>();
    }
    for i in 0..n.saturating_sub(1) {
        blits[i].base.next = &raw mut blits[i + 1].base;
    }
    list.submission.entry = &raw mut blits[0].base;
    list.submission.encoder_count = n;
    list
}

#[test]
fn execute_allocates_as_cpp() {
    let dev = Box::into_raw(Device::new_dummy());

    let mut blits_1 = (0..1).map(make_blit_encoder).collect::<Vec<_>>();
    let mut cmds_1 = vec![
        WmtcmdBlitCopyFromBufferToBuffer {
            r#type: 0,
            reserved: [0; 3],
            next: MemPtr::NULL,
            src: 0,
            src_offset: 0,
            dst: 0,
            dst_offset: 0,
            copy_length: 0,
        };
        1
    ];
    let mut blits_64 = (0..64).map(make_blit_encoder).collect::<Vec<_>>();
    let mut cmds_64 = vec![
        WmtcmdBlitCopyFromBufferToBuffer {
            r#type: 0,
            reserved: [0; 3],
            next: MemPtr::NULL,
            src: 0,
            src_offset: 0,
            dst: 0,
            dst_offset: 0,
            copy_length: 0,
        };
        64
    ];

    let list_1 = make_fake_list(dev, &mut blits_1, &mut cmds_1);
    let list_64 = make_fake_list(dev, &mut blits_64, &mut cmds_64);

    let queue = Queue::new_for_test(dev);

    // Warm up once so any one-time initialization happens before comparing 1 vs 64 encoders.
    queue.execute(&[&list_64]);

    let allocs_1 = count_allocs(|| {
        queue.execute(&[&list_1]);
    });
    let allocs_64 = count_allocs(|| {
        queue.execute(&[&list_64]);
    });

    assert_eq!(allocs_1, allocs_64);
    assert_eq!(allocs_64, 0);

    drop(list_64);
    drop(list_1);
    // SAFETY: `dev` was allocated by `Box::into_raw(Device::new_dummy())`.
    drop(unsafe { Box::from_raw(dev) });
}

#[test]
fn framegen_order_joins() {
    let mut state = OrderState::new(false);
    for i in 0..FENCE_COUNT {
        state.fence_handles[i] = 2000 + i as u64;
    }

    // Populate the frontier with two overlapping encoders (fences 0 and 1, handles 2000 and 2001).
    let mut b0 = make_blit_encoder(0);
    b0.base.join = true;
    assert_eq!(state.order(Some(&b0.base), false), 0);

    let mut b1 = make_blit_encoder(1);
    b1.base.join = false;
    assert_eq!(state.order(Some(&b1.base), false), 1);
    assert_eq!(state.frontier, vec![0, 1]);

    // SAFETY: fake test handles `0xB001` and `0xB002` with `NSObject_release` faked in `lib.rs`.
    let begin = unsafe { Obj::adopt(0xB001) }.expect("non-zero handle");
    // SAFETY: fake test handle `0xB002`.
    let end = unsafe { Obj::adopt(0xB002) }.expect("non-zero handle");

    BLIT_OPS.with(|ops| ops.borrow_mut().clear());
    RECORD_BLIT.with(|r| r.set(true));

    let mut fg_order = FrameGenOrder::new(&mut state, 0xD000);
    fg_order.wait(&begin);
    fg_order.update(&end);

    RECORD_BLIT.with(|r| r.set(false));
    let recorded = BLIT_OPS.with(|ops| ops.borrow().clone());

    assert_eq!(fg_order.done, 2);
    // `wait(&begin)` joins the frontier `[0, 1]` (handles 2000, 2001), emits `ScratchFill`
    // (`0x9000`), and `update(&end)` updates `fences_[done]` (fence 2 -> handle 2002).
    assert_eq!(
        recorded,
        vec![
            (0xB001, WMT_BLIT_COMMAND_WAIT_FOR_FENCE, 2000),
            (0xB001, WMT_BLIT_COMMAND_WAIT_FOR_FENCE, 2001),
            (0xB001, WMT_BLIT_COMMAND_FILL_BUFFER, 0x9000),
            (0xB002, WMT_BLIT_COMMAND_UPDATE_FENCE, 2002),
        ]
    );
}
