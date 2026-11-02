use core::{ffi::c_void, mem::ManuallyDrop, ptr};
use std::sync::Arc;

use dxmt_core::{
    dynamic::DynamicBuffer,
    resource::{Buffer, BufferAllocationFlags},
};
use winemetal_sys::Obj;

use crate::{
    abi::ext::{D3D11Piece, D3D11UpscaleDesc, FEATURE_METALFX_TEMPORAL_SCALER},
    com::{E_FAIL, E_INVALIDARG, S_OK},
    context::{Context, immediate::encoder_trace},
    resource::Common,
};

fn fake_device() -> ManuallyDrop<Obj> {
    // SAFETY: Non-zero sentinel handle wrapped in `ManuallyDrop` so `Obj_release` never runs.
    ManuallyDrop::new(unsafe { Obj::adopt(1) }.expect("non-zero"))
}

const fn zero_piece() -> D3D11Piece {
    D3D11Piece {
        id: 0,
        cpu: ptr::null_mut(),
        row_pitch: 0,
        depth_pitch: 0,
    }
}

#[test]
fn pieces_refused_on_deferred() {
    let dev = fake_device();
    let buffer = Arc::new(Buffer::new(128, &dev));
    let flags = BufferAllocationFlags::CPU_PLACED;
    let first = buffer.allocate(flags);
    buffer.rename(first);
    let dyn_buf = Arc::new(DynamicBuffer::new(buffer, flags).expect("dynamic buffer"));
    let mut common = Common {
        dynamic_buffer: Some(Arc::clone(&dyn_buf)),
        bind_flags: 0x1,
        usage: 2,
        ..Common::default()
    };
    let res_ptr = (&raw mut common).cast::<c_void>();

    let mut def = Context::new_deferred(ptr::null_mut(), 0);
    let mut piece = zero_piece();
    assert_eq!(def.get_pieces(res_ptr, 0, 1, &raw mut piece), E_FAIL);

    // On immediate context, GetPieces lends pieces and UsePiece switches the active piece.
    let mut imm = Context::new_immediate(ptr::null_mut());
    let mut pieces = [zero_piece(), zero_piece()];
    assert_eq!(imm.get_pieces(res_ptr, 0, 2, pieces.as_mut_ptr()), S_OK);
    assert_ne!(pieces[0].id, pieces[1].id);
    assert!(!pieces[0].cpu.is_null());
    assert!(!pieces[1].cpu.is_null());

    imm.use_piece(res_ptr, 0, pieces[1].id);
    assert_eq!(dyn_buf.immediate_piece(), pieces[1].id);
    assert_eq!(
        dyn_buf.immediate_mapped_memory().cast::<c_void>(),
        pieces[1].cpu
    );
}

#[test]
fn upscale_desc_is_88() {
    assert_eq!(size_of::<D3D11UpscaleDesc>(), 88);

    let mut imm = Context::new_immediate(ptr::null_mut());
    imm.begin_uav_overlap();
    imm.end_uav_overlap();

    let desc = D3D11UpscaleDesc {
        input_content_width: 1920,
        input_content_height: 1080,
        auto_exposure: 0,
        in_reset: 0,
        depth_reversed: 1,
        motion_vector_in_display_res: 0,
        color: ptr::null_mut(),
        depth: ptr::null_mut(),
        motion_vector: ptr::null_mut(),
        output: ptr::null_mut(),
        motion_vector_scale: [1.0, -1.0],
        pre_exposure: 1.0,
        exposure_texture: ptr::null_mut(),
        jitter_offset: [0.0, 0.0],
    };
    imm.temporal_upscale(&raw const desc);

    let records = encoder_trace(imm.mode.encoder_mut());
    assert_eq!(
        records,
        [
            "BarrierControl(0)",
            "BarrierControl(1)",
            "ComputePass:start",
            "MvScale",
            "Pass:end",
            "UpscaleTemporal",
        ]
    );

    let mut supported: u32 = 0;
    assert_eq!(
        imm.check_ext_feature_support(
            FEATURE_METALFX_TEMPORAL_SCALER,
            (&raw mut supported).cast(),
            4,
        ),
        S_OK
    );
    assert_eq!(supported, 1);
    assert_eq!(
        imm.check_ext_feature_support(99, (&raw mut supported).cast(), 4),
        E_INVALIDARG
    );
}
