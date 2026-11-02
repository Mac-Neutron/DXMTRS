use core::ptr;

use dxmt_core::encoder::NO_NEXT;
use winemetal_sys::wire::{WMT_PRIMITIVE_TYPE_TRIANGLE, WMT_RENDER_COMMAND_DRAW, WmtcmdRenderDraw};

use super::*;
use crate::{
    com::E_INVALIDARG,
    context::immediate::{encoder_trace, push_encoder_trace},
};

#[test]
fn create_deferred_validates_out_and_records_commands() {
    let hr_null = create_deferred(ptr::null_mut(), 0, ptr::null_mut());
    assert_eq!(hr_null, E_INVALIDARG);

    let mut raw_ctx: *mut c_void = ptr::null_mut();
    let hr = create_deferred(ptr::null_mut(), 0x4, &raw mut raw_ctx);
    assert_eq!(hr, S_OK);
    assert!(!raw_ctx.is_null());

    // SAFETY: `create_deferred` returned a valid `Context<Deferred>` interface pointer.
    let ctx = unsafe { &mut *raw_ctx.cast::<Context<Deferred>>() };
    assert_eq!(ctx.mode.context_flags, 0x4);
    assert!(ctx.as_deferred().is_some());
    assert!(ctx.as_immediate().is_none());

    ctx.begin_render_pass();
    Deferred::emit_op(ctx, |enc| {
        enc.encode_render_command(WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: NO_NEXT,
            primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
            vertex_start: 0,
            vertex_count: 6,
            instance_count: 1,
            base_instance: 0,
        });
        push_encoder_trace(enc, "Render:Draw(6,0)");
    });
    ctx.finish_pass(false);

    let mut imm = Context::new_immediate(ptr::null_mut());
    ctx.mode.commands_mut().run(imm.mode.encoder_mut());
    assert_eq!(
        encoder_trace(imm.mode.encoder_mut()),
        ["RenderPass:start", "Render:Draw(6,0)", "Pass:end"]
    );

    // SAFETY: Reconstruct the `Box<Context<Deferred>>` created by `create_deferred`.
    let _ = unsafe { Box::from_raw(raw_ctx.cast::<Context<Deferred>>()) };
}
