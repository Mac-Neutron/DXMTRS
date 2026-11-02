use core::ptr;

use super::*;
use crate::context::immediate::encoder_trace;

#[test]
fn event_and_occlusion_queries_track_completion_and_flush() {
    let mut ctx = Context::new_immediate(ptr::null_mut());
    let mut ev_q = ContextQuery::new(D3D11_QUERY_EVENT, 0);
    let mut occ_q = ContextQuery::new(D3D11_QUERY_OCCLUSION, 0);

    // GetData before End is invalid.
    let mut flag: i32 = 0;
    assert_eq!(
        ctx.get_data(ev_q.as_async_ptr(), (&raw mut flag).cast(), 4, 0),
        DXGI_ERROR_INVALID_CALL
    );

    ctx.begin(occ_q.as_async_ptr());
    ctx.end(occ_q.as_async_ptr());
    ctx.end(ev_q.as_async_ptr());

    // Without DONOTFLUSH, GetData flushes (commits chunk 1) and returns S_FALSE while in flight.
    assert_eq!(
        ctx.get_data(ev_q.as_async_ptr(), (&raw mut flag).cast(), 4, 0),
        S_FALSE
    );
    assert_eq!(ctx.mode.commit_count, 1);

    // Once GPU completes seq 1, GetData returns S_OK and writes query results.
    ctx.wait_for_seq(1);
    assert_eq!(
        ctx.get_data(ev_q.as_async_ptr(), (&raw mut flag).cast(), 4, 0),
        S_OK
    );
    assert_eq!(flag, 1);

    let mut samples: u64 = u64::MAX;
    assert_eq!(
        ctx.get_data(occ_q.as_async_ptr(), (&raw mut samples).cast(), 8, 0),
        S_OK
    );
    assert_eq!(samples, 0);
}

#[test]
fn fence_signal_and_wait_and_context_type_queries() {
    let mut imm = Context::new_immediate(ptr::null_mut());
    let mut def = Context::new_deferred(ptr::null_mut(), 0x8);

    assert_eq!(imm.get_type(), D3D11_DEVICE_CONTEXT_IMMEDIATE);
    assert_eq!(imm.get_context_flags(), 0);
    assert_eq!(def.get_type(), D3D11_DEVICE_CONTEXT_DEFERRED);
    assert_eq!(def.get_context_flags(), 0x8);

    let fake_fence: u64 = 0;
    let fence_ptr = (&raw const fake_fence).cast_mut().cast::<c_void>();
    assert_eq!(def.signal(fence_ptr, 42), E_INVALIDARG);
    assert_eq!(def.wait(fence_ptr, 42), E_INVALIDARG);

    assert_eq!(imm.signal(fence_ptr, 10), S_OK);
    assert_eq!(imm.wait(fence_ptr, 20), S_OK);
    assert_eq!(
        encoder_trace(imm.mode.encoder_mut()),
        ["SignalEvent(10)", "WaitEvent(20)"]
    );
}
