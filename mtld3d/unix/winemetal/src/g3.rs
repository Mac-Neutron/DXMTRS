//! Command queues, command buffers, the three command-list encoders and counter sample buffers.
//!
//! A result the C wrote straight from a send (a command buffer, an encoder, a command buffer's
//! error and logs, a log container's items) goes to the guest at +0 through
//! [`ret_passthrough`], from the local re-declarations here returning `*mut AnyObject`; a queue
//! and a counter sample buffer are +1. An object argument a guest sends as 0 goes out as nil:
//! where the binding takes `&T`, the send is re-declared with `Option<&T>`.

use core::ffi::c_void;
use core::ptr::{NonNull, null_mut};

use objc2::extern_methods;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_foundation::{NSFastEnumeration, NSFastEnumerationState, NSRange};
use objc2_metal::{
    MTLBlitPassDescriptor, MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLCommonCounterSetTimestamp, MTLComputePassDescriptor, MTLCounterDontSample,
    MTLCounterSampleBuffer, MTLCounterSampleBufferDescriptor, MTLCounterSet, MTLDevice,
    MTLDispatchType, MTLDrawable, MTLEvent, MTLLoadAction, MTLOrigin, MTLRenderPassDescriptor,
    MTLSize, MTLStorageMode, MTLStoreAction, MTLVisibilityResultType,
};
use winemetal_wire::{
    MemPtr, UnixcallEnumerate, UnixcallGenericObjNoret, UnixcallGenericObjObjDoubleNoret,
    UnixcallGenericObjObjNoret, UnixcallGenericObjObjRet, UnixcallGenericObjObjUint64Noret,
    UnixcallGenericObjUint64ObjRet, UnixcallGenericObjUint64Ret, UnixcallGenericObjUint64Uint64Ret,
    UnixcallMtlcommandbufferBlitcommandencoderwithsamplebuffers,
    UnixcallMtlcommandbufferComputecommandencoderwithsamplebuffers,
    UnixcallMtlcountersamplebufferNewtimestampbuffer,
    UnixcallMtlcountersamplebufferResolvecounterrange, WMT_COMMAND_BUFFER_PROPERTY_GPU_END_TIME,
    WMT_COMMAND_BUFFER_PROPERTY_GPU_START_TIME, WMT_COMMAND_BUFFER_PROPERTY_KERNEL_END_TIME,
    WMT_COMMAND_BUFFER_PROPERTY_KERNEL_START_TIME, WMTOrigin, WMTRenderPassInfo,
    WMTSampleBufferAttachmentInfo, WMTSize,
};

use crate::ffi::{AbortOnPanic, STATUS_SUCCESS, mem_ref, mem_slice, params};
use crate::float_cast::{f64_to_u64, u64_to_usize};
use crate::handle::{obj, ret_owned, ret_passthrough};

mod blit;
mod compute;
mod render;

type CommandBuffer = ProtocolObject<dyn MTLCommandBuffer>;

/// The native host's present hook, run before each present: `WMTSetPresentHook`'s, null for none.
///
/// Process-wide (§State lives on an object): set by the host before it makes a device, as the C's
/// static was.
#[cfg(feature = "native")]
static PRESENT_HOOK: core::sync::atomic::AtomicPtr<c_void> =
    core::sync::atomic::AtomicPtr::new(null_mut());

/// A native host's present hook: the command buffer's and the drawable's handles, as passed.
#[cfg(feature = "native")]
pub type PresentHook = unsafe extern "C-unwind" fn(*mut c_void, *mut c_void);

/// Slot 9: a command queue of at most `arg` command buffers, +1.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLDevice_newCommandQueue")]
pub unsafe extern "C-unwind" fn device_new_command_queue(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64ObjRet>(args) };
    // SAFETY: as above.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.handle) };
    let count = u64_to_usize(p.arg);
    p.ret = ret_owned(device.and_then(|d| d.newCommandQueueWithMaxCommandBufferCount(count)));
    STATUS_SUCCESS
}

/// Slot 11: a fresh command buffer, autoreleased (+0).
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandQueue_commandBuffer")]
pub unsafe extern "C-unwind" fn queue_command_buffer(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: as above.
    let queue = unsafe { obj::<ProtocolObject<dyn MTLCommandQueue>>(p.handle) };
    p.ret = ret_passthrough(queue.map_or(null_mut(), RawQueue::command_buffer));
    STATUS_SUCCESS
}

/// Slot 12: commits a command buffer.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_commit")]
pub unsafe extern "C-unwind" fn command_buffer_commit(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjNoret>(args) };
    // SAFETY: as above.
    if let Some(cb) = unsafe { obj::<CommandBuffer>(p.handle) } {
        cb.commit();
    }
    STATUS_SUCCESS
}

/// Slot 13: blocks until a command buffer has completed.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_waitUntilCompleted")]
pub unsafe extern "C-unwind" fn command_buffer_wait_until_completed(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjNoret>(args) };
    // SAFETY: as above.
    if let Some(cb) = unsafe { obj::<CommandBuffer>(p.handle) } {
        cb.waitUntilCompleted();
    }
    STATUS_SUCCESS
}

/// Slot 14: a command buffer's status.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_status")]
pub unsafe extern "C-unwind" fn command_buffer_status(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: as above.
    let cb = unsafe { obj::<CommandBuffer>(p.handle) };
    p.ret = cb.map_or(0, |cb| cb.status().0 as u64);
    STATUS_SUCCESS
}

/// Slot 17: encodes signalling an event with a value.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_encodeSignalEvent")]
pub unsafe extern "C-unwind" fn command_buffer_encode_signal_event(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjUint64Noret>(args) };
    // SAFETY: as above.
    if let Some(cb) = unsafe { obj::<CommandBuffer>(p.handle) } {
        // SAFETY: as above.
        cb.encode_signal_event(unsafe { obj(p.arg0) }, p.arg1);
    }
    STATUS_SUCCESS
}

/// Slot 30: a blit command encoder, autoreleased (+0).
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_blitCommandEncoder")]
pub unsafe extern "C-unwind" fn command_buffer_blit_command_encoder(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: as above.
    let cb = unsafe { obj::<CommandBuffer>(p.handle) };
    p.ret = ret_passthrough(cb.map_or(null_mut(), RawCommandBuffer::blit_command_encoder));
    STATUS_SUCCESS
}

/// Slot 31: a compute command encoder, serial for an `arg` of 0, else concurrent (+0).
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_computeCommandEncoder")]
pub unsafe extern "C-unwind" fn command_buffer_compute_command_encoder(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64ObjRet>(args) };
    // SAFETY: as above.
    let cb = unsafe { obj::<CommandBuffer>(p.handle) };
    let dispatch = if p.arg == 0 { MTLDispatchType::Serial } else { MTLDispatchType::Concurrent };
    p.ret = ret_passthrough(cb.map_or(null_mut(), |cb| cb.compute_command_encoder(dispatch)));
    STATUS_SUCCESS
}

/// Slot 32: a render command encoder for the `WMTRenderPassInfo` at `arg` (+0).
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds;
/// `arg` is the address of a `WMTRenderPassInfo` whose handles are too.
#[unsafe(export_name = "_MTLCommandBuffer_renderCommandEncoder")]
pub unsafe extern "C-unwind" fn command_buffer_render_command_encoder(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64ObjRet>(args) };
    // SAFETY: as above.
    let cb = unsafe { obj::<CommandBuffer>(p.handle) };
    // SAFETY: as above. A null `arg`, where the C read through null, writes nothing.
    let Some(info) = (unsafe { mem_ref::<WMTRenderPassInfo>(MemPtr { ptr: p.arg }) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: as above.
    let descriptor = unsafe { render_pass_descriptor(info) };
    p.ret = ret_passthrough(cb.map_or(null_mut(), |cb| cb.render_command_encoder(&descriptor)));
    STATUS_SUCCESS
}

/// Slot 33: ends an encoder's encoding.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandEncoder_endEncoding")]
pub unsafe extern "C-unwind" fn command_encoder_end_encoding(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjNoret>(args) };
    // SAFETY: as above.
    if let Some(encoder) = unsafe { obj::<ProtocolObject<dyn MTLCommandEncoder>>(p.handle) } {
        encoder.endEncoding();
    }
    STATUS_SUCCESS
}

/// Slot 47: presents a drawable, after the native host's present hook.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_presentDrawable")]
pub unsafe extern "C-unwind" fn command_buffer_present_drawable(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjNoret>(args) };
    present_hook(p.handle, p.arg);
    // SAFETY: as above.
    if let Some(cb) = unsafe { obj::<CommandBuffer>(p.handle) } {
        // SAFETY: as above.
        cb.present_drawable(unsafe { obj(p.arg) });
    }
    STATUS_SUCCESS
}

/// Slot 48: presents a drawable no sooner than a duration after the last, after the hook.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_presentDrawableAfterMinimumDuration")]
pub unsafe extern "C-unwind" fn command_buffer_present_drawable_after_minimum_duration(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjDoubleNoret>(args) };
    present_hook(p.handle, p.arg0);
    // SAFETY: as above.
    if let Some(cb) = unsafe { obj::<CommandBuffer>(p.handle) } {
        // SAFETY: as above.
        cb.present_drawable_after_minimum_duration(unsafe { obj(p.arg0) }, p.arg1);
    }
    STATUS_SUCCESS
}

/// Slot 89: a command buffer's error, as Metal holds it (+0).
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_error")]
pub unsafe extern "C-unwind" fn command_buffer_error(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: as above.
    let cb = unsafe { obj::<CommandBuffer>(p.handle) };
    p.ret = ret_passthrough(cb.map_or(null_mut(), RawCommandBuffer::raw_error));
    STATUS_SUCCESS
}

/// Slot 90: a command buffer's log container, as Metal holds it (+0).
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_logs")]
pub unsafe extern "C-unwind" fn command_buffer_logs(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: as above.
    let cb = unsafe { obj::<CommandBuffer>(p.handle) };
    p.ret = ret_passthrough(cb.map_or(null_mut(), RawCommandBuffer::raw_logs));
    STATUS_SUCCESS
}

/// Slot 91: the items `start` on of a log container, up to `buffer_size` of them (+0 each).
///
/// The container is walked with fast enumeration, as the C's `for … in` did, and the walk stops
/// at the first item past the window. A null buffer, where the C wrote through null, gets nothing
/// written; the count read is still the window's.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds;
/// a non-null `buffer` holds `buffer_size` handles.
#[unsafe(export_name = "_MTLLogContainer_enumerate")]
pub unsafe extern "C-unwind" fn log_container_enumerate(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallEnumerate>(args) };
    // SAFETY: as above.
    let container = unsafe { obj::<ProtocolObject<dyn NSFastEnumeration>>(p.enumerable) };
    let buffer = raw::<u64>(p.buffer);
    let start = p.start;
    // The C's `start + buffer_size`, wrapping as it did.
    let end = start.wrapping_add(p.buffer_size);
    let mut read = 0;
    if let Some(container) = container {
        let mut state = NSFastEnumerationState {
            state: 0,
            itemsPtr: null_mut(),
            mutationsPtr: null_mut(),
            extra: [0; 5],
        };
        let mut batch = [null_mut::<AnyObject>(); 16];
        let mut count: u64 = 0;
        'walk: loop {
            // SAFETY: the state and the batch are this walk's, the batch holds `batch.len()`.
            let n = unsafe {
                container.countByEnumeratingWithState_objects_count(
                    NonNull::from(&mut state),
                    NonNull::from(&mut batch).cast(),
                    batch.len(),
                )
            };
            if n == 0 {
                break;
            }
            // SAFETY: the container's batch: `n` items at `itemsPtr`, alive while it is.
            let items = unsafe { core::slice::from_raw_parts(state.itemsPtr, n) };
            for &item in items {
                if count >= start {
                    if count >= end {
                        break 'walk;
                    }
                    if !buffer.is_null() {
                        let at = buffer.wrapping_add(u64_to_usize(count - start));
                        // SAFETY: `count - start` is below `buffer_size`, the guest's length.
                        unsafe { at.write(ret_passthrough(item)) };
                    }
                    read += 1;
                }
                count += 1;
            }
        }
    }
    p.ret_read = read;
    STATUS_SUCCESS
}

/// Slot 102: encodes waiting for an event to reach a value.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_encodeWaitForEvent")]
pub unsafe extern "C-unwind" fn command_buffer_encode_wait_for_event(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjUint64Noret>(args) };
    // SAFETY: as above.
    if let Some(cb) = unsafe { obj::<CommandBuffer>(p.handle) } {
        // SAFETY: as above.
        cb.encode_wait_for_event(unsafe { obj(p.arg0) }, p.arg1);
    }
    STATUS_SUCCESS
}

/// Slot 127: a timestamp counter sample buffer of `sample_count` samples, +1.
///
/// The device's first counter set named `MTLCommonCounterSetTimestamp`, nil when it has none;
/// shared storage for a non-zero `shared`, else private. The error Metal reports is dropped.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCounterSampleBuffer_newTimestampBuffer")]
pub unsafe extern "C-unwind" fn counter_sample_buffer_new_timestamp_buffer(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlcountersamplebufferNewtimestampbuffer>(args) };
    // SAFETY: as above.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    let descriptor = MTLCounterSampleBufferDescriptor::new();
    // SAFETY: Metal's constant string, alive for the process.
    let timestamp = unsafe { MTLCommonCounterSetTimestamp };
    let set = device
        .and_then(MTLDevice::counterSets)
        .and_then(|sets| sets.iter().find(|s| s.name().isEqualToString(timestamp)));
    descriptor.setCounterSet(set.as_deref());
    // SAFETY: the C's send of the guest's count: Metal checks it when it makes the buffer.
    unsafe { descriptor.setSampleCount(p.sample_count as usize) };
    descriptor.setStorageMode(if p.shared == 0 { MTLStorageMode::Private } else { MTLStorageMode::Shared });
    p.ret = ret_owned(device.and_then(|d| d.newCounterSampleBufferWithDescriptor_error(&descriptor).ok()));
    STATUS_SUCCESS
}

/// Slot 128: `len` samples from `start` into `data_out`, at most `data_length` bytes of them.
///
/// In a pool of its own: the resolved data is autoreleased, and a queue's completion thread,
/// which calls this, has no pool.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds;
/// a non-null `data_out` holds `data_length` bytes.
#[unsafe(export_name = "_MTLCounterSampleBuffer_resolveCounterRange")]
pub unsafe extern "C-unwind" fn counter_sample_buffer_resolve_counter_range(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlcountersamplebufferResolvecounterrange>(args) };
    // SAFETY: as above.
    let samples = unsafe { obj::<ProtocolObject<dyn MTLCounterSampleBuffer>>(p.sample_buffer) };
    let range = NSRange::new(p.start as usize, p.len as usize);
    autoreleasepool(|_| {
        // SAFETY: the C's send of the guest's range: Metal checks it.
        let data = samples.and_then(|s| unsafe { s.resolveCounterRange(range) });
        if let (Some(data), Some(out)) = (data, NonNull::new(raw::<c_void>(p.data_out))) {
            // SAFETY: the guest's buffer holds `data_length` bytes.
            unsafe { data.getBytes_length(out, u64_to_usize(p.data_length)) };
        }
    });
    STATUS_SUCCESS
}

/// Slot 129: a blit command encoder sampling into the given counter sample buffers (+0).
///
/// The attachments go to the descriptor's array unclamped, as the C's did. A null attachment
/// array with a count, where the C read through null, writes nothing.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds;
/// a non-null `attachments` holds `num_attachments` of them, whose handles are too.
#[unsafe(export_name = "_MTLCommandBuffer_blitCommandEncoderWithSampleBuffers")]
pub unsafe extern "C-unwind" fn command_buffer_blit_command_encoder_with_sample_buffers(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlcommandbufferBlitcommandencoderwithsamplebuffers>(args) };
    // SAFETY: as above.
    let cb = unsafe { obj::<CommandBuffer>(p.cmdbuf) };
    // SAFETY: as above.
    let attachments = unsafe {
        mem_slice::<WMTSampleBufferAttachmentInfo>(p.attachments, p.num_attachments)
    };
    let Some(attachments) = attachments else {
        return STATUS_SUCCESS;
    };
    let descriptor = MTLBlitPassDescriptor::new();
    let array = descriptor.sampleBufferAttachments();
    for (i, a) in attachments.iter().enumerate() {
        // SAFETY: the C's index, unclamped: Metal checks it.
        let d = unsafe { array.objectAtIndexedSubscript(i) };
        // SAFETY: as above.
        d.setSampleBuffer(unsafe { obj(a.sample_buffer) });
        // SAFETY: the C's send of the guest's index: Metal checks it when it makes the encoder.
        unsafe { d.setStartOfEncoderSampleIndex(u64_to_usize(a.start_of_encoder_sample_index)) };
        // SAFETY: as above.
        unsafe { d.setEndOfEncoderSampleIndex(u64_to_usize(a.end_of_encoder_sample_index)) };
    }
    p.ret = ret_passthrough(cb.map_or(null_mut(), |cb| cb.blit_command_encoder_with(&descriptor)));
    STATUS_SUCCESS
}

/// Slot 130: a command buffer's time `arg` (`WMTCommandBufferProperty`) in nanoseconds.
///
/// 0 for any other `arg`.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds.
#[unsafe(export_name = "_MTLCommandBuffer_property")]
pub unsafe extern "C-unwind" fn command_buffer_property(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Uint64Ret>(args) };
    // SAFETY: as above.
    let cb = unsafe { obj::<CommandBuffer>(p.handle) };
    let seconds = match u32::try_from(p.arg) {
        Ok(WMT_COMMAND_BUFFER_PROPERTY_KERNEL_START_TIME) => cb.map_or(0.0, MTLCommandBuffer::kernelStartTime),
        Ok(WMT_COMMAND_BUFFER_PROPERTY_KERNEL_END_TIME) => cb.map_or(0.0, MTLCommandBuffer::kernelEndTime),
        Ok(WMT_COMMAND_BUFFER_PROPERTY_GPU_START_TIME) => cb.map_or(0.0, MTLCommandBuffer::GPUStartTime),
        Ok(WMT_COMMAND_BUFFER_PROPERTY_GPU_END_TIME) => cb.map_or(0.0, MTLCommandBuffer::GPUEndTime),
        _ => 0.0,
    };
    p.ret = f64_to_u64(seconds * 1e9);
    STATUS_SUCCESS
}

/// Slot 147: a compute command encoder sampling into the given counter sample buffers (+0).
///
/// Concurrent for a non-zero `concurrent`, else serial; the attachments as slot 129's.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds;
/// a non-null `attachments` holds `num_attachments` of them, whose handles are too.
#[unsafe(export_name = "_MTLCommandBuffer_computeCommandEncoderWithSampleBuffers")]
pub unsafe extern "C-unwind" fn command_buffer_compute_command_encoder_with_sample_buffers(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p =
        unsafe { params::<UnixcallMtlcommandbufferComputecommandencoderwithsamplebuffers>(args) };
    // SAFETY: as above.
    let cb = unsafe { obj::<CommandBuffer>(p.cmdbuf) };
    // SAFETY: as above.
    let attachments = unsafe {
        mem_slice::<WMTSampleBufferAttachmentInfo>(p.attachments, p.num_attachments)
    };
    let Some(attachments) = attachments else {
        return STATUS_SUCCESS;
    };
    let descriptor = MTLComputePassDescriptor::new();
    descriptor.setDispatchType(if p.concurrent == 0 {
        MTLDispatchType::Serial
    } else {
        MTLDispatchType::Concurrent
    });
    let array = descriptor.sampleBufferAttachments();
    for (i, a) in attachments.iter().enumerate() {
        // SAFETY: the C's index, unclamped: Metal checks it.
        let d = unsafe { array.objectAtIndexedSubscript(i) };
        // SAFETY: as above.
        d.setSampleBuffer(unsafe { obj(a.sample_buffer) });
        // SAFETY: the C's send of the guest's index: Metal checks it when it makes the encoder.
        unsafe { d.setStartOfEncoderSampleIndex(u64_to_usize(a.start_of_encoder_sample_index)) };
        // SAFETY: as above.
        unsafe { d.setEndOfEncoderSampleIndex(u64_to_usize(a.end_of_encoder_sample_index)) };
    }
    p.ret =
        ret_passthrough(cb.map_or(null_mut(), |cb| cb.compute_command_encoder_with(&descriptor)));
    STATUS_SUCCESS
}

/// `WMTSetPresentHook`: the hook slots 47 and 48 run before they present, `None` for none.
#[cfg(feature = "native")]
#[unsafe(export_name = "WMTSetPresentHook")]
pub extern "C-unwind" fn set_present_hook(hook: Option<PresentHook>) {
    let _guard = AbortOnPanic;
    let hook = hook.map_or(null_mut(), |hook| hook as *mut c_void);
    PRESENT_HOOK.store(hook, core::sync::atomic::Ordering::Relaxed);
}

/// `MTLCommandQueue`'s send whose result the C wrote at +0.
trait RawQueue {
    fn command_buffer(&self) -> *mut AnyObject;
}

impl RawQueue for ProtocolObject<dyn MTLCommandQueue> {
    extern_methods!(
        /// `-commandBuffer`, autoreleased.
        #[unsafe(method(commandBuffer))]
        #[unsafe(method_family = none)]
        fn command_buffer(&self) -> *mut AnyObject;
    );
}

/// `MTLCommandBuffer`'s sends whose result the C wrote at +0, or whose object may be nil.
trait RawCommandBuffer {
    fn blit_command_encoder(&self) -> *mut AnyObject;
    fn compute_command_encoder(&self, dispatch_type: MTLDispatchType) -> *mut AnyObject;
    fn render_command_encoder(&self, descriptor: &MTLRenderPassDescriptor) -> *mut AnyObject;
    fn blit_command_encoder_with(&self, descriptor: &MTLBlitPassDescriptor) -> *mut AnyObject;
    fn compute_command_encoder_with(&self, descriptor: &MTLComputePassDescriptor) -> *mut AnyObject;
    fn raw_error(&self) -> *mut AnyObject;
    fn raw_logs(&self) -> *mut AnyObject;
    fn encode_signal_event(&self, event: Option<&ProtocolObject<dyn MTLEvent>>, value: u64);
    fn encode_wait_for_event(&self, event: Option<&ProtocolObject<dyn MTLEvent>>, value: u64);
    fn present_drawable(&self, drawable: Option<&ProtocolObject<dyn MTLDrawable>>);
    fn present_drawable_after_minimum_duration(
        &self,
        drawable: Option<&ProtocolObject<dyn MTLDrawable>>,
        duration: f64,
    );
}

impl RawCommandBuffer for CommandBuffer {
    extern_methods!(
        /// `-blitCommandEncoder`, autoreleased.
        #[unsafe(method(blitCommandEncoder))]
        #[unsafe(method_family = none)]
        fn blit_command_encoder(&self) -> *mut AnyObject;

        /// `-computeCommandEncoderWithDispatchType:`, autoreleased.
        #[unsafe(method(computeCommandEncoderWithDispatchType:))]
        #[unsafe(method_family = none)]
        fn compute_command_encoder(&self, dispatch_type: MTLDispatchType) -> *mut AnyObject;

        /// `-renderCommandEncoderWithDescriptor:`, autoreleased.
        #[unsafe(method(renderCommandEncoderWithDescriptor:))]
        #[unsafe(method_family = none)]
        fn render_command_encoder(&self, descriptor: &MTLRenderPassDescriptor) -> *mut AnyObject;

        /// `-blitCommandEncoderWithDescriptor:`, autoreleased.
        #[unsafe(method(blitCommandEncoderWithDescriptor:))]
        #[unsafe(method_family = none)]
        fn blit_command_encoder_with(&self, descriptor: &MTLBlitPassDescriptor) -> *mut AnyObject;

        /// `-computeCommandEncoderWithDescriptor:`, autoreleased.
        #[unsafe(method(computeCommandEncoderWithDescriptor:))]
        #[unsafe(method_family = none)]
        fn compute_command_encoder_with(
            &self,
            descriptor: &MTLComputePassDescriptor,
        ) -> *mut AnyObject;

        /// `-error`, the command buffer's.
        #[unsafe(method(error))]
        #[unsafe(method_family = none)]
        fn raw_error(&self) -> *mut AnyObject;

        /// `-logs`, the command buffer's.
        #[unsafe(method(logs))]
        #[unsafe(method_family = none)]
        fn raw_logs(&self) -> *mut AnyObject;

        /// `-encodeSignalEvent:value:`.
        #[unsafe(method(encodeSignalEvent:value:))]
        #[unsafe(method_family = none)]
        fn encode_signal_event(&self, event: Option<&ProtocolObject<dyn MTLEvent>>, value: u64);

        /// `-encodeWaitForEvent:value:`.
        #[unsafe(method(encodeWaitForEvent:value:))]
        #[unsafe(method_family = none)]
        fn encode_wait_for_event(&self, event: Option<&ProtocolObject<dyn MTLEvent>>, value: u64);

        /// `-presentDrawable:`.
        #[unsafe(method(presentDrawable:))]
        #[unsafe(method_family = none)]
        fn present_drawable(&self, drawable: Option<&ProtocolObject<dyn MTLDrawable>>);

        /// `-presentDrawable:afterMinimumDuration:`.
        #[unsafe(method(presentDrawable:afterMinimumDuration:))]
        #[unsafe(method_family = none)]
        fn present_drawable_after_minimum_duration(
            &self,
            drawable: Option<&ProtocolObject<dyn MTLDrawable>>,
            duration: f64,
        );
    );
}

/// The render pass descriptor slot 32 makes from a `WMTRenderPassInfo`, the C's sends in order.
///
/// # Safety
///
/// `info`'s handles are 0 or live objects of their kinds.
unsafe fn render_pass_descriptor(info: &WMTRenderPassInfo) -> Retained<MTLRenderPassDescriptor> {
    let descriptor = MTLRenderPassDescriptor::new();
    let colors = descriptor.colorAttachments();
    for (i, c) in info.colors.iter().enumerate() {
        // SAFETY: one of the descriptor's 8 colour attachments.
        let a = unsafe { colors.objectAtIndexedSubscript(i) };
        a.setClearColor(MTLClearColor {
            red: c.clear_color.r,
            green: c.clear_color.g,
            blue: c.clear_color.b,
            alpha: c.clear_color.a,
        });
        a.setLevel(usize::from(c.level));
        a.setSlice(usize::from(c.slice));
        a.setDepthPlane(c.depth_plane as usize);
        // SAFETY: the caller's contract.
        a.setTexture(unsafe { obj(c.texture) });
        a.setLoadAction(MTLLoadAction(c.load_action as usize));
        a.setStoreAction(MTLStoreAction(c.store_action as usize));
        // SAFETY: as above.
        a.setResolveTexture(unsafe { obj(c.resolve_texture) });
        a.setResolveLevel(usize::from(c.resolve_level));
        a.setResolveSlice(usize::from(c.resolve_slice));
        a.setResolveDepthPlane(c.resolve_depth_plane as usize);
    }
    let depth = &info.depth;
    if depth.texture != 0 {
        let a = descriptor.depthAttachment();
        a.setClearDepth(f64::from(depth.clear_depth));
        a.setDepthPlane(depth.depth_plane as usize);
        a.setLevel(usize::from(depth.level));
        a.setSlice(usize::from(depth.slice));
        // SAFETY: as above.
        a.setTexture(unsafe { obj(depth.texture) });
        a.setLoadAction(MTLLoadAction(depth.load_action as usize));
        a.setStoreAction(MTLStoreAction(depth.store_action as usize));
    }
    let stencil = &info.stencil;
    if stencil.texture != 0 {
        let a = descriptor.stencilAttachment();
        a.setClearStencil(u32::from(stencil.clear_stencil));
        a.setDepthPlane(stencil.depth_plane as usize);
        a.setLevel(usize::from(stencil.level));
        a.setSlice(usize::from(stencil.slice));
        // SAFETY: as above.
        a.setTexture(unsafe { obj(stencil.texture) });
        a.setLoadAction(MTLLoadAction(stencil.load_action as usize));
        a.setStoreAction(MTLStoreAction(stencil.store_action as usize));
    }
    descriptor.setDefaultRasterSampleCount(usize::from(info.default_raster_sample_count));
    descriptor.setRenderTargetArrayLength(usize::from(info.render_target_array_length));
    descriptor.setRenderTargetHeight(info.render_target_height as usize);
    descriptor.setRenderTargetWidth(info.render_target_width as usize);
    // SAFETY: as above.
    descriptor.setVisibilityResultBuffer(unsafe { obj(info.visibility_buffer) });
    // Timestamps at the pass's end.
    let samples = descriptor.sampleBufferAttachments();
    for (i, s) in info.sample_buffers.iter().enumerate() {
        if s.sample_buffer == 0 {
            continue;
        }
        // SAFETY: one of the descriptor's 4 sample buffer attachments.
        let a = unsafe { samples.objectAtIndexedSubscript(i) };
        // SAFETY: the caller's contract.
        a.setSampleBuffer(unsafe { obj(s.sample_buffer) });
        // SAFETY: the C's send of the guest's index: Metal checks it when it makes the encoder.
        unsafe { a.setStartOfVertexSampleIndex(u64_to_usize(s.start_of_vertex_sample_index)) };
        // SAFETY: Metal's value for no sample.
        unsafe { a.setEndOfVertexSampleIndex(MTLCounterDontSample) };
        // SAFETY: as above.
        unsafe { a.setStartOfFragmentSampleIndex(MTLCounterDontSample) };
        // SAFETY: the C's send of the guest's index, as above.
        unsafe { a.setEndOfFragmentSampleIndex(u64_to_usize(s.end_of_fragment_sample_index)) };
    }
    if info.visibility_accumulate != 0 {
        descriptor.setVisibilityResultType(MTLVisibilityResultType::Accumulate);
    }
    if info.tile_height != 0 && info.tile_width != 0 {
        descriptor.setTileWidth(usize::from(info.tile_width));
        descriptor.setTileHeight(usize::from(info.tile_height));
    }
    descriptor
}

/// Runs the native host's present hook, when it set one, with the two handles.
#[cfg(feature = "native")]
fn present_hook(cmdbuf: u64, drawable: u64) {
    let hook = PRESENT_HOOK.load(core::sync::atomic::Ordering::Relaxed);
    if hook.is_null() {
        return;
    }
    // SAFETY: a `PresentHook` that `set_present_hook` stored.
    let hook = unsafe { core::mem::transmute::<*mut c_void, PresentHook>(hook) };
    let cmdbuf = core::ptr::with_exposed_provenance_mut(u64_to_usize(cmdbuf));
    let drawable = core::ptr::with_exposed_provenance_mut(u64_to_usize(drawable));
    // SAFETY: the host's hook, which takes the two handles as the C passed them.
    unsafe { hook(cmdbuf, drawable) };
}

/// No present hook outside the native build.
#[cfg(not(feature = "native"))]
const fn present_hook(_cmdbuf: u64, _drawable: u64) {}

/// The command at `at`, as its type's structure.
///
/// # Safety
///
/// `at` points to a live, aligned `T` that nothing writes during `'a`.
const unsafe fn body<'a, T>(at: MemPtr) -> &'a T {
    // SAFETY: the caller's contract.
    unsafe { &*raw::<T>(at) }
}

/// The address a `WMTMemoryPointer` holds, as a pointer.
const fn raw<T>(p: MemPtr) -> *mut T {
    core::ptr::with_exposed_provenance_mut(u64_to_usize(p.ptr))
}

/// A `WMTSize` as an `MTLSize`.
const fn size(s: &WMTSize) -> MTLSize {
    MTLSize {
        width: u64_to_usize(s.width),
        height: u64_to_usize(s.height),
        depth: u64_to_usize(s.depth),
    }
}

/// A `WMTOrigin` as an `MTLOrigin`.
const fn origin(o: &WMTOrigin) -> MTLOrigin {
    MTLOrigin { x: u64_to_usize(o.x), y: u64_to_usize(o.y), z: u64_to_usize(o.z) }
}

/// A command's `location` and `length` as an `NSRange`.
const fn range(location: usize, length: usize) -> NSRange {
    NSRange { location, length }
}

#[cfg(test)]
mod tests;
