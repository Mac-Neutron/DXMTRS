//! Slot 37: a PE compute command list on a compute command encoder.

use core::ffi::c_void;

use objc2::{extern_methods, runtime::ProtocolObject};
use objc2_foundation::NSRange;
use objc2_metal::{
    MTLBarrierScope, MTLBuffer, MTLComputeCommandEncoder, MTLComputePipelineState, MTLFence,
    MTLIndirectCommandBuffer, MTLResource, MTLResourceUsage, MTLSize,
};
use winemetal_wire::{
    MemPtr, UnixcallGenericObjCmdNoret, WMT_COMPUTE_COMMAND_DISPATCH,
    WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT, WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
    WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER, WMT_COMPUTE_COMMAND_MEMORY_BARRIER,
    WMT_COMPUTE_COMMAND_SET_BUFFER, WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET,
    WMT_COMPUTE_COMMAND_SET_BYTES, WMT_COMPUTE_COMMAND_SET_PSO, WMT_COMPUTE_COMMAND_SET_TEXTURE,
    WMT_COMPUTE_COMMAND_UPDATE_FENCE, WMT_COMPUTE_COMMAND_USE_RESOURCE,
    WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE, WmtcmdBase, WmtcmdComputeDispatch,
    WmtcmdComputeDispatchIndirect, WmtcmdComputeExecutecommands, WmtcmdComputeFenceOp,
    WmtcmdComputeMemoryBarrier, WmtcmdComputeSetbuffer, WmtcmdComputeSetbufferoffset,
    WmtcmdComputeSetbytes, WmtcmdComputeSetpso, WmtcmdComputeSettexture, WmtcmdComputeUseresource,
};

use super::{body, range, raw, size};
use crate::{
    ffi::{AbortOnPanic, STATUS_SUCCESS, mem_ref, params},
    float_cast::u64_to_usize,
    handle::obj,
};

type Encoder = ProtocolObject<dyn MTLComputeCommandEncoder>;
type Buffer = ProtocolObject<dyn MTLBuffer>;
type Fence = ProtocolObject<dyn MTLFence>;

/// Slot 37: each command of the list on the encoder, in order.
///
/// The threadgroup size of a dispatch is the last `SetPSO`'s of the same call, zero before one.
/// Nop and unknown types are skipped and the walk goes on. A nil encoder sends nothing, as the
/// C's sends to nil did.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds;
/// `cmd_head` is null or the first node of a list the PE built: each node live and of the
/// structure its type names, its handles 0 or live objects, its `next` null or the next node.
#[unsafe(export_name = "_MTLComputeCommandEncoder_encodeCommands")]
pub unsafe extern "C-unwind" fn encode_commands(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjCmdNoret>(args) };
    // SAFETY: as above.
    let Some(encoder) = (unsafe { obj::<Encoder>(p.encoder) }) else {
        return STATUS_SUCCESS;
    };
    let mut threadgroup = MTLSize {
        width: 0,
        height: 0,
        depth: 0,
    };
    let mut at = p.cmd_head;
    // SAFETY: as above.
    while let Some(cmd) = unsafe { mem_ref::<WmtcmdBase>(at) } {
        // SAFETY: as above.
        unsafe { encode(encoder, cmd.r#type, at, &mut threadgroup) };
        at = cmd.next;
    }
    STATUS_SUCCESS
}

/// The compute sends a list's commands make, each object parameter nullable: 0 goes out as nil.
trait RawCompute {
    unsafe fn dispatch_threadgroups_indirect(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        threads_per_threadgroup: MTLSize,
    );
    fn set_pipeline_state(&self, state: Option<&ProtocolObject<dyn MTLComputePipelineState>>);
    fn use_resource(
        &self,
        resource: Option<&ProtocolObject<dyn MTLResource>>,
        usage: MTLResourceUsage,
    );
    unsafe fn set_bytes(&self, bytes: *const c_void, length: usize, index: usize);
    fn update_fence(&self, fence: Option<&Fence>);
    fn wait_for_fence(&self, fence: Option<&Fence>);
    unsafe fn execute_commands(
        &self,
        buffer: Option<&ProtocolObject<dyn MTLIndirectCommandBuffer>>,
        range: NSRange,
    );
}

impl RawCompute for Encoder {
    extern_methods!(
        /// `-dispatchThreadgroupsWithIndirectBuffer:indirectBufferOffset:threadsPerThreadgroup:`.
        #[unsafe(method(dispatchThreadgroupsWithIndirectBuffer:indirectBufferOffset:threadsPerThreadgroup:))]
        #[unsafe(method_family = none)]
        unsafe fn dispatch_threadgroups_indirect(
            &self,
            buffer: Option<&Buffer>,
            offset: usize,
            threads_per_threadgroup: MTLSize,
        );

        /// `-setComputePipelineState:`.
        #[unsafe(method(setComputePipelineState:))]
        #[unsafe(method_family = none)]
        fn set_pipeline_state(&self, state: Option<&ProtocolObject<dyn MTLComputePipelineState>>);

        /// `-useResource:usage:`.
        #[unsafe(method(useResource:usage:))]
        #[unsafe(method_family = none)]
        fn use_resource(
            &self,
            resource: Option<&ProtocolObject<dyn MTLResource>>,
            usage: MTLResourceUsage,
        );

        /// `-setBytes:length:atIndex:`, the guest's address as it is.
        #[unsafe(method(setBytes:length:atIndex:))]
        #[unsafe(method_family = none)]
        unsafe fn set_bytes(&self, bytes: *const c_void, length: usize, index: usize);

        /// `-updateFence:`.
        #[unsafe(method(updateFence:))]
        #[unsafe(method_family = none)]
        fn update_fence(&self, fence: Option<&Fence>);

        /// `-waitForFence:`.
        #[unsafe(method(waitForFence:))]
        #[unsafe(method_family = none)]
        fn wait_for_fence(&self, fence: Option<&Fence>);

        /// `-executeCommandsInBuffer:withRange:`.
        #[unsafe(method(executeCommandsInBuffer:withRange:))]
        #[unsafe(method_family = none)]
        unsafe fn execute_commands(
            &self,
            buffer: Option<&ProtocolObject<dyn MTLIndirectCommandBuffer>>,
            range: NSRange,
        );
    );
}

/// One command on the encoder; Nop and unknown types send nothing.
///
/// # Safety
///
/// `at` points to a live node of type `ty`, of the structure the type names, whose handles are 0
/// or live objects of their kinds.
unsafe fn encode(encoder: &Encoder, ty: u16, at: MemPtr, threadgroup: &mut MTLSize) {
    match ty {
        WMT_COMPUTE_COMMAND_DISPATCH => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeDispatch>(at) };
            encoder.dispatchThreadgroups_threadsPerThreadgroup(size(&c.size), *threadgroup);
        }
        WMT_COMPUTE_COMMAND_DISPATCH_THREADS => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeDispatch>(at) };
            encoder.dispatchThreads_threadsPerThreadgroup(size(&c.size), *threadgroup);
        }
        WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeDispatchIndirect>(at) };
            // SAFETY: as above.
            let buffer = unsafe { obj(c.indirect_args_buffer) };
            let offset = u64_to_usize(c.indirect_args_offset);
            // SAFETY: the C's send of the guest's values: Metal takes them as the C's.
            unsafe { encoder.dispatch_threadgroups_indirect(buffer, offset, *threadgroup) };
        }
        WMT_COMPUTE_COMMAND_SET_PSO => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeSetpso>(at) };
            // SAFETY: as above.
            encoder.set_pipeline_state(unsafe { obj(c.pso) });
            *threadgroup = size(&c.threadgroup_size);
        }
        WMT_COMPUTE_COMMAND_SET_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeSetbuffer>(at) };
            // SAFETY: as above.
            let buffer = unsafe { obj(c.buffer) };
            let (offset, index) = (u64_to_usize(c.offset), usize::from(c.index));
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setBuffer_offset_atIndex(buffer, offset, index) };
        }
        WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeSetbufferoffset>(at) };
            let (offset, index) = (u64_to_usize(c.offset), usize::from(c.index));
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setBufferOffset_atIndex(offset, index) };
        }
        WMT_COMPUTE_COMMAND_USE_RESOURCE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeUseresource>(at) };
            let usage = MTLResourceUsage(c.usage as usize);
            // SAFETY: as above.
            encoder.use_resource(unsafe { obj(c.resource) }, usage);
        }
        WMT_COMPUTE_COMMAND_SET_BYTES => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeSetbytes>(at) };
            let (length, index) = (u64_to_usize(c.length), usize::from(c.index));
            // SAFETY: the guest's `length` bytes at `bytes`, as the C passed them.
            unsafe { encoder.set_bytes(raw(c.bytes), length, index) };
        }
        WMT_COMPUTE_COMMAND_SET_TEXTURE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeSettexture>(at) };
            // SAFETY: as above.
            let texture = unsafe { obj(c.texture) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.setTexture_atIndex(texture, usize::from(c.index)) };
        }
        WMT_COMPUTE_COMMAND_UPDATE_FENCE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeFenceOp>(at) };
            // SAFETY: as above.
            encoder.update_fence(unsafe { obj(c.fence) });
        }
        WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeFenceOp>(at) };
            // SAFETY: as above.
            encoder.wait_for_fence(unsafe { obj(c.fence) });
        }
        WMT_COMPUTE_COMMAND_MEMORY_BARRIER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeMemoryBarrier>(at) };
            encoder.memoryBarrierWithScope(MTLBarrierScope(usize::from(c.scope)));
        }
        WMT_COMPUTE_COMMAND_EXECUTE_COMMANDS_IN_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdComputeExecutecommands>(at) };
            // SAFETY: as above.
            let buffer = unsafe { obj(c.indirect_command_buffer) };
            let executed = range(c.location as usize, c.length as usize);
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.execute_commands(buffer, executed) };
        }
        _ => {}
    }
}
