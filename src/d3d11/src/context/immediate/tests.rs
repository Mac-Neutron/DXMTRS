use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::sync::Arc;

use dxmt_core::encoder::NO_NEXT;
use winemetal_sys::wire::{
    MemPtr, WMT_PRIMITIVE_TYPE_TRIANGLE, WMT_RENDER_COMMAND_DRAW, WMTBufferInfo, WMTSamplerInfo,
    WmtcmdRenderDraw,
};

use super::*;

#[unsafe(no_mangle)]
extern "C-unwind" fn cx_test_new_buffer(_device: u64, info: *mut WMTBufferInfo) -> u64 {
    static NEXT_GPU_ADDR: AtomicU64 = AtomicU64::new(0x1000_0000);
    let addr = NEXT_GPU_ADDR.fetch_add(0x10000, Ordering::Relaxed);
    if !info.is_null() {
        // SAFETY: `info` is a valid buffer descriptor provided by the caller.
        let desc = unsafe { &mut *info };
        desc.gpu_address = addr;
        if desc.memory.ptr == 0 && desc.length > 0 {
            let len = usize::try_from(desc.length).unwrap_or(64).max(64);
            let mem = Box::leak(vec![0u8; len].into_boxed_slice());
            desc.memory.ptr = mem.as_mut_ptr() as usize as u64;
        }
    }
    0
}

#[unsafe(no_mangle)]
extern "C-unwind" fn cx_test_update_contents(
    _buffer: u64,
    _offset: u64,
    _data: MemPtr,
    _length: u64,
) {
}

#[unsafe(no_mangle)]
extern "C-unwind" fn cx_test_new_sampler(_device: u64, _info: *mut WMTSamplerInfo) -> u64 {
    0
}

#[unsafe(no_mangle)]
extern "C-unwind" fn cx_test_zero_ret(_device: u64) -> u64 {
    0
}

#[unsafe(no_mangle)]
extern "C-unwind" fn cx_test_noop_obj(_obj: u64) {}

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".globl _MTLDevice_newBuffer",
    ".weak_definition _MTLDevice_newBuffer",
    "_MTLDevice_newBuffer:",
    "b _cx_test_new_buffer",
    ".globl _MTLBuffer_updateContents",
    ".weak_definition _MTLBuffer_updateContents",
    "_MTLBuffer_updateContents:",
    "b _cx_test_update_contents",
    ".globl _MTLDevice_newSamplerState",
    ".weak_definition _MTLDevice_newSamplerState",
    "_MTLDevice_newSamplerState:",
    "b _cx_test_new_sampler",
    ".globl _MTLDevice_newFence",
    ".weak_definition _MTLDevice_newFence",
    "_MTLDevice_newFence:",
    "b _cx_test_zero_ret",
    ".globl _MTLDevice_newEvent",
    ".weak_definition _MTLDevice_newEvent",
    "_MTLDevice_newEvent:",
    "b _cx_test_zero_ret",
    ".globl _MTLBuffer_newTexture",
    ".weak_definition _MTLBuffer_newTexture",
    "_MTLBuffer_newTexture:",
    "b _cx_test_zero_ret",
    ".globl _MTLDevice_newSharedTexture",
    ".weak_definition _MTLDevice_newSharedTexture",
    "_MTLDevice_newSharedTexture:",
    "b _cx_test_zero_ret",
    ".globl _MTLDevice_newTexture",
    ".weak_definition _MTLDevice_newTexture",
    "_MTLDevice_newTexture:",
    "b _cx_test_zero_ret",
    ".globl _MTLTexture_newTextureView",
    ".weak_definition _MTLTexture_newTextureView",
    "_MTLTexture_newTextureView:",
    "b _cx_test_zero_ret",
    ".globl _Obj_retain",
    ".weak_definition _Obj_retain",
    "_Obj_retain:",
    "b _cx_test_noop_obj",
    ".globl _Obj_release",
    ".weak_definition _Obj_release",
    "_Obj_release:",
    "b _cx_test_noop_obj",
    ".globl _NSObject_retain",
    ".weak_definition _NSObject_retain",
    "_NSObject_retain:",
    "b _cx_test_noop_obj",
    ".globl _NSObject_release",
    ".weak_definition _NSObject_release",
    "_NSObject_release:",
    "b _cx_test_noop_obj",
);

#[test]
fn flush1_sets_event_after_commit() {
    let mut ctx = Context::new_immediate(ptr::null_mut());
    ctx.begin_render_pass();
    Immediate::emit_op(&mut ctx, |enc| {
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
        push_encoder_trace(enc, "Render:Draw(3,0)");
    });

    let fake_event = 0xDEAD_BEEF_usize as *mut c_void;
    ctx.flush1(0, fake_event);

    assert_eq!(ctx.mode.commit_count, 1);
    assert_eq!(ctx.mode.cmdbuf_seq, 2);
    assert_eq!(ctx.mode.event_log, ["commit", "set_event"]);
    assert_eq!(ctx.mode.signaled_win32_events, [(1, 0xDEAD_BEEF)]);
}

#[test]
fn commit_and_wait_until_gpu_idle_retires_callbacks() {
    let mut ctx = Context::new_immediate(ptr::null_mut());
    let retired = Arc::new(AtomicBool::new(false));
    let retired_clone = Arc::clone(&retired);

    Immediate::emit_op(&mut ctx, |enc| {
        enc.start_blit_pass();
        enc.end_pass();
    });
    let seq = ctx.mode.current_seq();
    ctx.register_in_flight(seq, move |_completed| {
        retired_clone.store(true, Ordering::SeqCst);
    });

    assert!(!retired.load(Ordering::SeqCst));
    ctx.wait_until_gpu_idle();
    assert!(retired.load(Ordering::SeqCst));
    assert_eq!(ctx.mode.coherent_seq(), seq);
}

#[test]
fn so_get_targets_with_offsets_zeros_outputs() {
    let ctx = Context::new_immediate(ptr::null_mut());
    let mut targets = [0x1234_usize as *mut c_void; 2];
    let mut offsets = [99_u32; 2];
    ctx.so_get_targets_with_offsets(2, targets.as_mut_ptr(), offsets.as_mut_ptr());
    assert!(targets[0].is_null());
    assert!(targets[1].is_null());
    assert_eq!(offsets, [0, 0]);
}
