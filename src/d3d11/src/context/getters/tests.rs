use core::{ffi::c_void, ptr};

use crate::{
    abi::records::{D3d11Viewport, Rect},
    context::{
        CommandBufferState, Context, Immediate,
        state::{ContextState, DirtyFlags, REQ_CONSTANT_BUFFER_ELEMENTS},
    },
    state::Defaults,
};

fn new_ctx() -> Context<Immediate> {
    Context {
        vtbl: ptr::null(),
        ext_vtbl: ptr::null(),
        annotation_vtbl: ptr::null(),
        refs: crate::com::Managed::default(),
        private_data: dxmt_util::private_data::PrivateData::default(),
        device: ptr::null_mut(),
        state: ContextState::default(),
        cmdbuf_state: CommandBufferState::Idle,
        encode: crate::context::encode::EncodeSession::default(),
        dirty: DirtyFlags::empty(),
        defaults: Defaults::default(),
        mode: Immediate::default(),
    }
}

#[test]
fn unbound_cb1_getters_return_default_4096_constants() {
    let ctx = new_ctx();
    let mut buf: *mut c_void = ptr::dangling_mut();
    let mut fc = 99_u32;
    let mut nc = 0_u32;
    ctx.ps_get_constant_buffers1(0, 1, &raw mut buf, &raw mut fc, &raw mut nc);
    assert!(buf.is_null());
    assert_eq!(fc, 0);
    assert_eq!(nc, REQ_CONSTANT_BUFFER_ELEMENTS);
}

#[test]
fn viewport_and_scissor_getters_zero_pad_extra_slots() {
    let mut ctx = new_ctx();
    let vp = D3d11Viewport {
        top_left_x: 2.0,
        top_left_y: 4.0,
        width: 64.0,
        height: 32.0,
        min_depth: 0.1,
        max_depth: 0.9,
    };
    ctx.rs_set_viewports(1, (&raw const vp).cast());

    let mut count = 2_u32;
    let mut out_vps = [
        D3d11Viewport::default(),
        D3d11Viewport {
            top_left_x: 9.0,
            top_left_y: 9.0,
            width: 9.0,
            height: 9.0,
            min_depth: 9.0,
            max_depth: 9.0,
        },
    ];
    ctx.rs_get_viewports(&raw mut count, out_vps.as_mut_ptr().cast());
    assert_eq!(count, 1);
    assert_eq!(out_vps[0].width.to_bits(), 64.0_f32.to_bits());
    assert_eq!(out_vps[0].height.to_bits(), 32.0_f32.to_bits());
    assert_eq!(out_vps[1].width.to_bits(), 0.0_f32.to_bits());
    assert_eq!(out_vps[1].height.to_bits(), 0.0_f32.to_bits());

    let sc = Rect {
        left: 10,
        top: 20,
        right: 30,
        bottom: 40,
    };
    ctx.rs_set_scissor_rects(1, (&raw const sc).cast());
    let mut sc_count = 2_u32;
    let mut out_scs = [
        Rect::default(),
        Rect {
            left: 9,
            top: 9,
            right: 9,
            bottom: 9,
        },
    ];
    ctx.rs_get_scissor_rects(&raw mut sc_count, out_scs.as_mut_ptr().cast());
    assert_eq!(sc_count, 1);
    assert_eq!(
        (
            out_scs[0].left,
            out_scs[0].top,
            out_scs[0].right,
            out_scs[0].bottom
        ),
        (10, 20, 30, 40)
    );
    assert_eq!(
        (
            out_scs[1].left,
            out_scs[1].top,
            out_scs[1].right,
            out_scs[1].bottom
        ),
        (0, 0, 0, 0)
    );
}
