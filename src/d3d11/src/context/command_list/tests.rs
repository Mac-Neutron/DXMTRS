use core::{ffi::c_void, fmt::Write as _, mem::ManuallyDrop, ptr, sync::atomic::Ordering};
use std::sync::Arc;

use dxmt_core::{
    dynamic::DynamicBuffer,
    encoder::{NO_NEXT, Pass},
    resource::{Buffer, BufferAllocationFlags},
    staging::{MapResult, Staging},
};
use winemetal_sys::{
    Obj,
    wire::{
        WMT_PRIMITIVE_TYPE_TRIANGLE, WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_NOP,
        WMT_RENDER_COMMAND_SET_VISIBILITY_MODE, WmtcmdBase, WmtcmdRenderDraw,
        WmtcmdRenderSetvisibilitymode,
    },
};

use super::*;
use crate::{
    com::{S_FALSE, S_OK},
    context::{
        Deferred,
        sync::{ContextQuery, D3D11_QUERY_OCCLUSION, QueryState},
    },
};

fn format_render_pass(pass: &Pass) -> String {
    let mut out = String::new();
    let mut cur = Some(pass.records());
    while let Some(node) = cur {
        // SAFETY: `node` is a valid `WmtcmdBase` record in the pass's arena.
        let base = unsafe { node.as_ref() };
        match base.r#type {
            WMT_RENDER_COMMAND_NOP => {
                out.push_str("render 0\n");
            }
            WMT_RENDER_COMMAND_SET_VISIBILITY_MODE => {
                // SAFETY: `base.r#type` matched `WMT_RENDER_COMMAND_SET_VISIBILITY_MODE`.
                let v = unsafe { node.cast::<WmtcmdRenderSetvisibilitymode>().as_ref() };
                let mode = v.mode;
                let offset = v.offset;
                let _ = writeln!(out, "render 18 offset={offset} mode={mode}");
            }
            WMT_RENDER_COMMAND_DRAW => {
                // SAFETY: `base.r#type` matched `WMT_RENDER_COMMAND_DRAW`.
                let d = unsafe { node.cast::<WmtcmdRenderDraw>().as_ref() };
                let pt = d.primitive_type;
                let vs = d.vertex_start;
                let vc = d.vertex_count;
                let ic = d.instance_count;
                let bi = d.base_instance;
                let _ = writeln!(
                    out,
                    "render 19 primitive_type={pt} vertex_start={vs} vertex_count={vc} instance_count={ic} base_instance={bi}"
                );
            }
            other => {
                let _ = writeln!(out, "render {other}");
            }
        }
        let next_addr = usize::try_from(base.next.ptr).unwrap_or(0);
        cur = core::ptr::NonNull::new(core::ptr::with_exposed_provenance_mut::<WmtcmdBase>(
            next_addr,
        ));
    }
    out
}

fn fake_device() -> ManuallyDrop<Obj> {
    // SAFETY: Non-zero sentinel handle wrapped in `ManuallyDrop` so `Obj_release` never runs.
    ManuallyDrop::new(unsafe { Obj::adopt(1) }.expect("non-zero"))
}

fn make_dynamic_buffer(size: u64) -> Arc<DynamicBuffer> {
    let dev = fake_device();
    let buffer = Arc::new(Buffer::new(size, &dev));
    let flags = BufferAllocationFlags::CPU_PLACED;
    let first = buffer.allocate(flags);
    buffer.rename(first);
    Arc::new(DynamicBuffer::new(buffer, flags).expect("dynamic buffer"))
}

fn release_cmd_list(raw: *mut c_void) {
    if raw.is_null() {
        return;
    }
    // SAFETY: `raw` is a valid `ID3D11CommandList*` interface pointer.
    let vtbl_ptr = unsafe { *raw.cast::<*const ID3D11CommandListVtbl>() };
    // SAFETY: `vtbl_ptr` points to the static command list vtable.
    let release_fn = unsafe { (*vtbl_ptr).release };
    // SAFETY: Calling `IUnknown::Release` on `raw`.
    unsafe {
        release_fn(raw);
    }
}

#[test]
fn runs_twice_with_the_same_records() {
    let mut def = Context::new_deferred(ptr::null_mut(), 0);
    let mut q1 = ContextQuery::new(D3D11_QUERY_OCCLUSION, 0);
    let mut q2 = ContextQuery::new(D3D11_QUERY_OCCLUSION, 0);

    Deferred::emit_st(&mut def, |enc| {
        enc.start_render_pass(0, 0, 0, None);
    });
    def.begin(q1.as_async_ptr());
    Deferred::emit_op(&mut def, |enc| {
        enc.bump_visibility_result_offset();
        enc.encode_render_command(WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
            vertex_start: 0,
            vertex_count: 3,
            instance_count: 1,
            base_instance: 0,
        });
    });
    def.end(q1.as_async_ptr());

    def.begin(q2.as_async_ptr());
    Deferred::emit_op(&mut def, |enc| {
        enc.bump_visibility_result_offset();
        enc.encode_render_command(WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
            vertex_start: 0,
            vertex_count: 3,
            instance_count: 1,
            base_instance: 0,
        });
    });
    def.end(q2.as_async_ptr());

    let mut raw_list: *mut c_void = ptr::null_mut();
    assert_eq!(def.finish_command_list(0, &raw mut raw_list), S_OK);
    assert!(!raw_list.is_null());

    let expected_golden = include_str!("golden/d3d11_deferred.txt");

    let mut imm1 = Context::new_immediate(ptr::null_mut());
    imm1.execute_command_list(raw_list, 0);
    let pass1 = imm1.mode.encoder_mut().pass().expect("render pass active");
    let formatted1 = format_render_pass(pass1);
    assert_eq!(formatted1, expected_golden);

    let mut imm2 = Context::new_immediate(ptr::null_mut());
    imm2.execute_command_list(raw_list, 0);
    let pass2 = imm2.mode.encoder_mut().pass().expect("render pass active");
    let formatted2 = format_render_pass(pass2);
    assert_eq!(formatted1, formatted2);

    release_cmd_list(raw_list);
}

#[test]
fn dynamic_names_per_list() {
    let dyn_buf = make_dynamic_buffer(256);
    let initial_imm_name = dyn_buf.immediate_name();

    let mut def_a = Context::new_deferred(ptr::null_mut(), 0);
    let mut def_b = Context::new_deferred(ptr::null_mut(), 0);

    let alloc_a1 = def_a.mode.get_or_alloc_dynamic_buffer(&dyn_buf);
    let alloc_a2 = Deferred::dynamic_name(&mut def_a, &dyn_buf);
    assert!(Arc::ptr_eq(&alloc_a1, &alloc_a2));

    let alloc_b = def_b.mode.get_or_alloc_dynamic_buffer(&dyn_buf);
    assert!(!Arc::ptr_eq(&alloc_a1, &alloc_b));
    assert!(!Arc::ptr_eq(&alloc_a1, &initial_imm_name));
    assert!(!Arc::ptr_eq(&alloc_b, &initial_imm_name));

    // Recording on deferred contexts does not mutate the immediate context's current name.
    assert!(Arc::ptr_eq(&dyn_buf.immediate_name(), &initial_imm_name));

    let mut list_a: *mut c_void = ptr::null_mut();
    let mut list_b: *mut c_void = ptr::null_mut();
    assert_eq!(def_a.finish_command_list(0, &raw mut list_a), S_OK);
    assert_eq!(def_b.finish_command_list(0, &raw mut list_b), S_OK);

    let mut imm = Context::new_immediate(ptr::null_mut());
    imm.execute_command_list(list_a, 0);
    assert!(Arc::ptr_eq(&dyn_buf.immediate_name(), &alloc_a1));

    imm.execute_command_list(list_b, 0);
    assert!(Arc::ptr_eq(&dyn_buf.immediate_name(), &alloc_b));

    release_cmd_list(list_a);
    release_cmd_list(list_b);
}

#[test]
fn restore_state_vs_clear() {
    let mut def = Context::new_deferred(ptr::null_mut(), 0);
    def.state.sample_mask = 0x1234_5678;
    def.state.stencil_ref = 7;
    def.state.topology = 4;

    // FinishCommandList with restore == 1 preserves state.
    let mut list1: *mut c_void = ptr::null_mut();
    assert_eq!(def.finish_command_list(1, &raw mut list1), S_OK);
    assert_eq!(def.state.sample_mask, 0x1234_5678);
    assert_eq!(def.state.stencil_ref, 7);
    assert_eq!(def.state.topology, 4);

    // FinishCommandList with restore == 0 clears state.
    let mut list2: *mut c_void = ptr::null_mut();
    assert_eq!(def.finish_command_list(0, &raw mut list2), S_OK);
    assert_eq!(def.state.sample_mask, 0xffff_ffff);
    assert_eq!(def.state.stencil_ref, 0);
    assert_eq!(def.state.topology, 0);

    // ExecuteCommandList with restore == 1 vs restore == 0 on Immediate context.
    let mut imm = Context::new_immediate(ptr::null_mut());
    imm.state.sample_mask = 0x00ff_00ff;
    imm.state.stencil_ref = 9;
    imm.execute_command_list(list1, 1);
    assert_eq!(imm.state.sample_mask, 0x00ff_00ff);
    assert_eq!(imm.state.stencil_ref, 9);

    imm.execute_command_list(list1, 0);
    assert_eq!(imm.state.sample_mask, 0xffff_ffff);
    assert_eq!(imm.state.stencil_ref, 0);

    release_cmd_list(list1);
    release_cmd_list(list2);
}

#[test]
fn executed_list_recycles_after_completion() {
    let dyn_buf = make_dynamic_buffer(256);
    let mut def = Context::new_deferred(ptr::null_mut(), 0);
    let deferred_alloc = def.mode.get_or_alloc_dynamic_buffer(&dyn_buf);
    let deferred_gpu_addr = deferred_alloc.gpu_address();

    let mut raw_list: *mut c_void = ptr::null_mut();
    assert_eq!(def.finish_command_list(0, &raw mut raw_list), S_OK);
    let list_ref = CommandList::from_iface(raw_list).expect("valid command list");
    let recycled_counter = Arc::clone(&list_ref.recycled_allocations);

    let mut imm = Context::new_immediate(ptr::null_mut());
    imm.execute_command_list(raw_list, 0);
    assert_eq!(recycled_counter.load(Ordering::Relaxed), 0);

    // Rotate the immediate name while the command list still owns `deferred_alloc`.
    let fresh = dyn_buf.allocate(imm.mode.coherent_seq());
    dyn_buf.update_immediate_name(imm.mode.current_seq(), fresh, 0, false);

    // Completing GPU work retires the command list's in-flight callback and recycles allocations.
    imm.wait_until_gpu_idle();
    assert_eq!(recycled_counter.load(Ordering::Relaxed), 1);
    drop(deferred_alloc);

    // First pop returns the initial allocation retired when ExecuteCommandList renamed `dyn_buf`;
    // second pop returns `deferred_alloc` recycled by the command list's completion callback.
    let _initial_reused = dyn_buf.allocate(imm.mode.coherent_seq());
    let reused = dyn_buf.allocate(imm.mode.coherent_seq());
    assert_eq!(reused.gpu_address(), deferred_gpu_addr);

    // Also verify CommandListPool reuse.
    let pool = CommandListPool::default();
    let pooled = pool.acquire(ptr::null_mut(), 0x2);
    assert_eq!(pooled.context_flags, 0x2);
    pool.recycle(pooled);
    assert_eq!(pool.free_count(), 1);
    assert_eq!(pool.recycled_total(), 1);

    release_cmd_list(raw_list);
}

#[test]
fn execute_marks_staging_and_reissues_queries() {
    let dev = fake_device();
    let staging = Arc::new(Staging::new(&dev, 64, 64, 64));
    let mut query = ContextQuery::new(D3D11_QUERY_OCCLUSION, 0);

    let mut def = Context::new_deferred(ptr::null_mut(), 0);
    def.record_staging_copy_destination(&staging);
    def.begin(query.as_async_ptr());
    def.begin_render_pass();
    Deferred::emit_op(&mut def, |enc| {
        enc.encode_render_command(WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
            vertex_start: 0,
            vertex_count: 3,
            instance_count: 1,
            base_instance: 0,
        });
    });
    def.end(query.as_async_ptr());

    let mut raw_list: *mut c_void = ptr::null_mut();
    assert_eq!(def.finish_command_list(0, &raw mut raw_list), S_OK);

    // Before ExecuteCommandList, the staging buffer has not been marked busy on the GPU.
    assert!(staging.try_map(0, true, false) == MapResult::Mappable);

    let mut imm = Context::new_immediate(ptr::null_mut());
    let exec_seq = imm.mode.current_seq();
    imm.execute_command_list(raw_list, 0);

    // ExecuteCommandList MUST mark the staging buffer as a copy destination at `exec_seq`.
    assert!(staging.try_map(0, true, false) == MapResult::WaitFor(exec_seq));

    // And the occlusion query MUST be re-issued on `imm` at `exec_seq`.
    assert_eq!(query.seq, exec_seq);
    assert_eq!(query.state, QueryState::Issued);
    let mut samples: u64 = u64::MAX;
    assert_eq!(
        imm.get_data(query.as_async_ptr(), (&raw mut samples).cast(), 8, 0),
        S_FALSE
    );

    imm.wait_for_seq(exec_seq);
    assert!(staging.try_map(exec_seq, true, false) == MapResult::Mappable);
    assert_eq!(
        imm.get_data(query.as_async_ptr(), (&raw mut samples).cast(), 8, 0),
        S_OK
    );
    assert_eq!(samples, 0);

    release_cmd_list(raw_list);
}

#[test]
fn finish_ends_open_queries() {
    let mut query = ContextQuery::new(D3D11_QUERY_OCCLUSION, 0);
    let mut def = Context::new_deferred(ptr::null_mut(), 0);

    // Begin query without calling End before FinishCommandList.
    def.begin(query.as_async_ptr());
    assert_eq!(def.mode.open_queries.len(), 1);
    assert_eq!(query.state, QueryState::Building);

    let mut raw_list: *mut c_void = ptr::null_mut();
    assert_eq!(def.finish_command_list(0, &raw mut raw_list), S_OK);

    // FinishCommandList must have ended the open query and queued it for reissue.
    assert_eq!(def.mode.open_queries.len(), 0);
    assert_eq!(query.state, QueryState::Issued);

    let mut imm = Context::new_immediate(ptr::null_mut());
    imm.execute_command_list(raw_list, 0);
    imm.wait_until_gpu_idle();

    let mut samples: u64 = u64::MAX;
    assert_eq!(
        imm.get_data(query.as_async_ptr(), (&raw mut samples).cast(), 8, 0),
        S_OK
    );
    assert_eq!(samples, 0);

    release_cmd_list(raw_list);
}
