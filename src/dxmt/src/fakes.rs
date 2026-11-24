//! The winemetal functions more than one module's tests reach, faked once for the test binary.
//!
//! `NSObject_retain` and `NSObject_release` come from every `Obj`, whichever module's test made
//! it. Each call goes to the hook its thread set with [`log_object_calls`], and nowhere when the
//! thread set none, so a test sees the calls of its own threads only.

use core::{
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
};
use std::sync::{Mutex, PoisonError};

/// What a hook is handed: the call's name and the object.
pub type Hook = fn(&'static str, u64);

/// Thread-local hook for `NSAutoreleasePool_alloc_init`.
pub type PoolHook = fn() -> u64;

/// Thread-local hook for `MTLCommandQueue_commandBuffer`.
pub type CommandBufferHook = fn(u64) -> u64;

/// Global handler for default `MTLCommandQueue_commandBuffer` allocations.
pub type QueueCommandBufferHandler = fn(u64, u64) -> u64;

/// Thread-local hook for `NSObject_description` and `NSString` readback.
pub type DescriptionHook = fn(u64) -> Option<&'static str>;

/// Thread-local hook for `MTLSharedEvent_waitUntilSignaledValue`.
pub type WaitUntilSignaledHook = fn(u64, u64, u64);

static NEXT_CMDBUF: AtomicU64 = AtomicU64::new(0x8000_0001);
static QUEUE_CMDBUF_HANDLER: Mutex<Option<QueueCommandBufferHandler>> = Mutex::new(None);

thread_local! {
    /// The calling thread's hook.
    ///
    /// A thread-local, because winemetal's C functions reach no object of the test's.
    static HOOK: Cell<Option<Hook>> = const { Cell::new(None) };
    static POOL_HOOK: Cell<Option<PoolHook>> = const { Cell::new(None) };
    static COMMAND_BUFFER_HOOK: Cell<Option<CommandBufferHook>> = const { Cell::new(None) };
    static DESCRIPTION_HOOK: Cell<Option<DescriptionHook>> = const { Cell::new(None) };
    static WAIT_UNTIL_SIGNALED_HOOK: Cell<Option<WaitUntilSignaledHook>> = const { Cell::new(None) };
}

/// Sends this thread's `NSObject_retain` and `NSObject_release` calls to `hook`.
pub fn log_object_calls(hook: Hook) {
    HOOK.set(Some(hook));
}

/// Clears this thread's `NSObject_retain` and `NSObject_release` hook.
pub fn clear_object_calls_hook() {
    HOOK.set(None);
}

/// Routes this thread's `NSAutoreleasePool_alloc_init` calls through `hook`.
pub fn on_autorelease_pool(hook: Option<PoolHook>) {
    POOL_HOOK.set(hook);
}

/// Routes this thread's `MTLCommandQueue_commandBuffer` calls through `hook`.
pub fn on_command_buffer(hook: Option<CommandBufferHook>) {
    COMMAND_BUFFER_HOOK.set(hook);
}

/// Registers the process-wide handler notified for default `MTLCommandQueue_commandBuffer` calls.
pub fn register_command_buffer_handler(handler: QueueCommandBufferHandler) {
    *QUEUE_CMDBUF_HANDLER
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(handler);
}

/// Routes this thread's `NSObject_description` and `NSString` queries through `hook`.
pub fn on_description(hook: Option<DescriptionHook>) {
    DESCRIPTION_HOOK.set(hook);
}

/// Routes this thread's `MTLSharedEvent_waitUntilSignaledValue` calls through `hook`.
pub fn on_wait_until_signaled(hook: Option<WaitUntilSignaledHook>) {
    WAIT_UNTIL_SIGNALED_HOOK.set(hook);
}

/// Notifies the thread-local hook installed via [`on_wait_until_signaled`].
pub fn notify_wait_until_signaled(event: u64, value: u64, timeout: u64) {
    if let Some(hook) = WAIT_UNTIL_SIGNALED_HOOK.get() {
        hook(event, value, timeout);
    }
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

#[unsafe(export_name = "NSAutoreleasePool_alloc_init")]
extern "C-unwind" fn fake_autorelease_pool() -> u64 {
    POOL_HOOK.get().map_or(0x730e, |hook| hook())
}

#[unsafe(export_name = "MTLCommandQueue_commandBuffer")]
extern "C-unwind" fn fake_command_buffer(queue: u64) -> u64 {
    if let Some(hook) = COMMAND_BUFFER_HOOK.get() {
        return hook(queue);
    }
    let cmdbuf = NEXT_CMDBUF.fetch_add(1, Ordering::Relaxed);
    let handler = *QUEUE_CMDBUF_HANDLER
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    handler.map_or(cmdbuf, |h| h(queue, cmdbuf))
}

#[unsafe(export_name = "NSObject_description")]
extern "C-unwind" fn fake_description(object: u64) -> u64 {
    DESCRIPTION_HOOK
        .get()
        .and_then(|hook| hook(object))
        .map_or(0, |_| object)
}

#[unsafe(export_name = "NSString_lengthOfBytesUsingEncoding")]
extern "C-unwind" fn fake_nsstring_len(string: u64, _encoding: u64) -> u64 {
    DESCRIPTION_HOOK
        .get()
        .and_then(|hook| hook(string))
        .map_or(0, |text| u64::try_from(text.len()).unwrap_or(0))
}

#[unsafe(export_name = "NSString_getCString")]
extern "C-unwind" fn fake_nsstring_get_cstring(
    string: u64,
    buffer: *mut core::ffi::c_char,
    max: u64,
    _encoding: u64,
) -> u32 {
    let Some(text) = DESCRIPTION_HOOK.get().and_then(|hook| hook(string)) else {
        return 0;
    };
    let max_len = usize::try_from(max).unwrap_or(usize::MAX);
    if buffer.is_null() || max_len == 0 {
        return 0;
    }
    let bytes = text.as_bytes();
    let copy_len = bytes.len().min(max_len - 1);
    // SAFETY: `buffer` is non-null with room for `max_len` bytes (`copy_len + 1 <= max_len`).
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast::<u8>(), copy_len) };
    // SAFETY: `copy_len < max_len`, so offset `copy_len` is within the caller's buffer.
    let end = unsafe { buffer.add(copy_len) };
    // SAFETY: `end` points to a writable byte within the caller's buffer.
    unsafe { end.write(0) };
    1
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

#[unsafe(export_name = "MTLCommandBuffer_encodeTemporalDenoisedScale")]
extern "C-unwind" fn fake_temporal_denoised_scale(
    _cmdbuf: u64,
    _scaler: u64,
    _color: u64,
    _out: u64,
    _depth: u64,
    _motion: u64,
    _diffuse_albedo: u64,
    _specular_albedo: u64,
    _normal: u64,
    _roughness: u64,
    _specular_hit_distance: u64,
    _denoise_strength_mask: u64,
    _transparency_overlay: u64,
    _exposure: u64,
    _reactive_mask: u64,
    _fence: u64,
    _props: *const winemetal_sys::wire::WMTFXTemporalDenoisedScalerProps,
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

#[unsafe(export_name = "WMTDeclareLatencyCriticalActivity")]
const extern "C-unwind" fn fake_declare_latency_critical_activity() -> u64 {
    1
}

#[unsafe(export_name = "WMTAttachMetalView")]
const extern "C-unwind" fn fake_attach_metal_view(
    _hwnd: isize,
    _device: u64,
    _desc: *const winemetal_sys::wire::WMTViewAttachDesc,
    out_poll: *mut winemetal_sys::wire::WMTViewPollOut,
    ret_layer: *mut u64,
) -> u64 {
    if !ret_layer.is_null() {
        // SAFETY: caller provides writable `ret_layer`.
        unsafe { ret_layer.write(0) };
    }
    if !out_poll.is_null() {
        // SAFETY: caller provides writable `out_poll`; all-zero bytes are valid.
        unsafe { out_poll.write_bytes(0, 1) };
    }
    0
}

#[unsafe(export_name = "WMTRetireMetalView")]
const extern "C-unwind" fn fake_retire_metal_view(_view: u64, _flags: u32) {}

#[unsafe(export_name = "WMTViewSyncAndPoll")]
const extern "C-unwind" fn fake_view_sync_and_poll(
    _view: u64,
    _layer: u64,
    _flags: u32,
    out_poll: *mut winemetal_sys::wire::WMTViewPollOut,
) {
    if !out_poll.is_null() {
        // SAFETY: caller provides writable `out_poll`; all-zero bytes are valid.
        unsafe { out_poll.write_bytes(0, 1) };
    }
}

#[unsafe(export_name = "WMTPresentClientSurface")]
const extern "C-unwind" fn fake_present_client_surface(_view: u64, _reasons: u32) {}

#[unsafe(export_name = "MTLDrawable_setPresentedEvent")]
const extern "C-unwind" fn fake_drawable_set_presented_event(
    _drawable: u64,
    _view: u64,
    _shared_event: u64,
    _signal_value: u64,
    _flags: u32,
) {
}

#[unsafe(export_name = "WMTSetCursorOverlay")]
const extern "C-unwind" fn fake_set_cursor_overlay(
    _view: u64,
    _params: *const winemetal_sys::wire::WMTCursorOverlayParams,
) {
}
