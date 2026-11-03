//! The winemetal functions more than one module's tests reach, faked once for the test binary.
//!
//! `NSObject_retain` and `NSObject_release` come from every `Obj`, whichever module's test made
//! it. Each call goes to the hook its thread set with [`log_object_calls`], and nowhere when the
//! thread set none, so a test sees the calls of its own threads only.

use core::cell::Cell;

/// What a hook is handed: the call's name and the object.
pub type Hook = fn(&'static str, u64);

thread_local! {
    /// The calling thread's hook.
    ///
    /// A thread-local, because winemetal's C functions reach no object of the test's.
    static HOOK: Cell<Option<Hook>> = const { Cell::new(None) };
}

/// Sends this thread's `NSObject_retain` and `NSObject_release` calls to `hook`.
pub fn log_object_calls(hook: Hook) {
    HOOK.set(Some(hook));
}

fn call(name: &'static str, object: u64) {
    if let Some(hook) = HOOK.get() {
        hook(name, object);
    }
}

#[unsafe(export_name = "NSObject_retain")]
extern "C-unwind" fn fake_retain(object: u64) {
    call("NSObject_retain", object);
}

#[unsafe(export_name = "NSObject_release")]
extern "C-unwind" fn fake_release(object: u64) {
    call("NSObject_release", object);
}

#[unsafe(export_name = "DispatchData_alloc_init")]
extern "C-unwind" fn fake_dispatch_data_alloc_init(_ptr: u64, _len: u64) -> u64 {
    0x7301
}

#[unsafe(export_name = "MTLDevice_newLibrary")]
extern "C-unwind" fn fake_new_library(_dev: u64, _data: u64, _err: *mut u64) -> u64 {
    0x7302
}

#[unsafe(export_name = "NSObject_description")]
extern "C-unwind" fn fake_description(_obj: u64) -> u64 {
    0
}

#[unsafe(export_name = "NSString_lengthOfBytesUsingEncoding")]
extern "C-unwind" fn fake_nsstring_len(_str: u64, _enc: u64) -> u64 {
    0
}

#[unsafe(export_name = "NSString_getCString")]
extern "C-unwind" fn fake_nsstring_get_cstring(
    _str: u64,
    _buf: *mut core::ffi::c_char,
    _max: u64,
    _enc: u64,
) -> u32 {
    0
}

#[unsafe(export_name = "NSString_string")]
extern "C-unwind" fn fake_nsstring_string(_data: *const core::ffi::c_char, _enc: u64) -> u64 {
    0x7303
}

#[unsafe(export_name = "MTLDevice_newComputePipelineState")]
extern "C-unwind" fn fake_new_compute_pso(
    _dev: u64,
    _info: *const winemetal_sys::wire::WMTComputePipelineInfo,
    _err: *mut u64,
) -> u64 {
    0x7304
}

#[unsafe(export_name = "MTLDevice_newDepthStencilState")]
extern "C-unwind" fn fake_new_dss(
    _dev: u64,
    _info: *const winemetal_sys::wire::WMTDepthStencilInfo,
) -> u64 {
    0x7305
}

#[unsafe(export_name = "MTLDevice_newTileRenderPipelineState")]
extern "C-unwind" fn fake_new_tile_pso(
    _dev: u64,
    _info: *const winemetal_sys::wire::WMTTileRenderPipelineInfo,
    _err: *mut u64,
) -> u64 {
    0x7306
}

#[unsafe(export_name = "MTLDevice_newSharedTexture")]
extern "C-unwind" fn fake_new_shared_tex(
    _dev: u64,
    _info: *mut winemetal_sys::wire::WMTTextureInfo,
) -> u64 {
    0x7307
}

#[unsafe(export_name = "MTLBuffer_newTexture")]
extern "C-unwind" fn fake_buf_new_tex(
    _buf: u64,
    _info: *mut winemetal_sys::wire::WMTTextureInfo,
    _off: u64,
    _bpr: u64,
) -> u64 {
    0x7308
}

#[unsafe(export_name = "MTLTexture_newTextureView")]
extern "C-unwind" fn fake_tex_new_view(
    _tex: u64,
    _fmt: u32,
    _ty: u32,
    _ls: u16,
    _lc: u16,
    _ss: u16,
    _sc: u16,
    _swz: winemetal_sys::wire::WMTTextureSwizzleChannels,
    _out_id: *mut u64,
) -> u64 {
    0x7309
}

#[unsafe(export_name = "MTLCaptureManager_sharedCaptureManager")]
extern "C-unwind" fn fake_capture_mgr() -> u64 {
    0x730a
}

#[unsafe(export_name = "MTLCaptureManager_startCapture")]
extern "C-unwind" fn fake_start_capture(
    _mgr: u64,
    _info: *mut winemetal_sys::wire::WMTCaptureInfo,
) -> bool {
    true
}

#[unsafe(export_name = "MTLCaptureManager_stopCapture")]
extern "C-unwind" fn fake_stop_capture(_mgr: u64) {}

#[unsafe(export_name = "MTLCommandBuffer_blitCommandEncoder")]
extern "C-unwind" fn fake_blit_enc(_cmdbuf: u64) -> u64 {
    0x730b
}

#[unsafe(export_name = "MTLCommandBuffer_blitCommandEncoderWithSampleBuffers")]
extern "C-unwind" fn fake_blit_enc_samples(
    _cmdbuf: u64,
    _attachments: *mut winemetal_sys::wire::WMTSampleBufferAttachmentInfo,
    _num: u64,
) -> u64 {
    0x730c
}

#[unsafe(export_name = "MTLCommandBuffer_computeCommandEncoder")]
extern "C-unwind" fn fake_compute_enc(_cmdbuf: u64, _concurrent: bool) -> u64 {
    0x730d
}

#[unsafe(export_name = "MTLCommandEncoder_setLabel")]
extern "C-unwind" fn fake_enc_set_label(_enc: u64, _label: u64) {}

#[unsafe(export_name = "MTLBlitCommandEncoder_encodeCommands")]
extern "C-unwind" fn fake_blit_encode_cmds(
    _enc: u64,
    _head: *const winemetal_sys::wire::WmtcmdBase,
) {
}

#[unsafe(export_name = "MTLComputeCommandEncoder_encodeCommands")]
extern "C-unwind" fn fake_compute_encode_cmds(
    _enc: u64,
    _head: *const winemetal_sys::wire::WmtcmdBase,
) {
}

#[unsafe(export_name = "MTLCommandBuffer_encodeSpatialScale")]
extern "C-unwind" fn fake_spatial_scale(
    _cmdbuf: u64,
    _scaler: u64,
    _color: u64,
    _out: u64,
    _fence: u64,
) {
}

#[unsafe(export_name = "MTLCommandBuffer_encodeTemporalScale")]
extern "C-unwind" fn fake_temporal_scale(
    _cmdbuf: u64,
    _scaler: u64,
    _color: u64,
    _out: u64,
    _depth: u64,
    _motion: u64,
    _exp: u64,
    _fence: u64,
    _props: *const winemetal_sys::wire::WMTFXTemporalScalerProps,
    _mask: u64,
) {
}

#[unsafe(export_name = "MTLCommandBuffer_presentDrawable")]
extern "C-unwind" fn fake_present_drawable(_cmdbuf: u64, _drawable: u64) {}

#[unsafe(export_name = "MTLCommandBuffer_presentDrawableAfterMinimumDuration")]
extern "C-unwind" fn fake_present_drawable_after(_cmdbuf: u64, _drawable: u64, _after: f64) {}

#[unsafe(export_name = "MTLCommandBuffer_property")]
extern "C-unwind" fn fake_cmdbuf_property(_cmdbuf: u64, _prop: u32) -> u64 {
    0
}
