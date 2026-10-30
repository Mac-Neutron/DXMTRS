//! Queues, command buffers and encoders.

use winemetal_wire::{
    UnixcallGenericObjCmdNoret, UnixcallGenericObjNoret, UnixcallGenericObjObjDoubleNoret, UnixcallGenericObjObjNoret,
    UnixcallGenericObjObjRet, UnixcallGenericObjObjUint64Noret, UnixcallGenericObjUint64ObjRet,
    UnixcallGenericObjUint64Ret, UnixcallGenericObjUint64Uint64Ret,
    UnixcallMtlcommandbufferBlitcommandencoderwithsamplebuffers,
    UnixcallMtlcommandbufferComputecommandencoderwithsamplebuffers, WMTRenderPassInfo, WMTSampleBufferAttachmentInfo,
    WmtcmdBase, slot,
};

use crate::transport::{addr, call, mem_ptr};

exports! {
    fn MTLDevice_newCommandQueue(device: u64, max_command_buffer_count: u64) -> u64 {
        call(slot::MTL_DEVICE_NEW_COMMAND_QUEUE, |p: &mut UnixcallGenericObjUint64ObjRet| {
            p.handle = device;
            p.arg = max_command_buffer_count;
        })
        .ret
    }

    fn MTLCommandQueue_commandBuffer(queue: u64) -> u64 {
        call(slot::MTL_COMMAND_QUEUE_COMMAND_BUFFER, |p: &mut UnixcallGenericObjObjRet| p.handle = queue).ret
    }

    fn MTLCommandBuffer_commit(cmdbuf: u64) {
        call(slot::MTL_COMMAND_BUFFER_COMMIT, |p: &mut UnixcallGenericObjNoret| p.handle = cmdbuf);
    }

    fn MTLCommandBuffer_waitUntilCompleted(cmdbuf: u64) {
        call(slot::MTL_COMMAND_BUFFER_WAIT_UNTIL_COMPLETED, |p: &mut UnixcallGenericObjNoret| p.handle = cmdbuf);
    }

    fn MTLCommandBuffer_status(cmdbuf: u64) -> u64 {
        call(slot::MTL_COMMAND_BUFFER_STATUS, |p: &mut UnixcallGenericObjUint64Ret| p.handle = cmdbuf).ret
    }

    fn MTLCommandBuffer_encodeSignalEvent(cmdbuf: u64, event: u64, value: u64) {
        call(slot::MTL_COMMAND_BUFFER_ENCODE_SIGNAL_EVENT, |p: &mut UnixcallGenericObjObjUint64Noret| {
            p.handle = cmdbuf;
            p.arg0 = event;
            p.arg1 = value;
        });
    }

    fn MTLCommandBuffer_blitCommandEncoder(cmdbuf: u64) -> u64 {
        call(slot::MTL_COMMAND_BUFFER_BLIT_COMMAND_ENCODER, |p: &mut UnixcallGenericObjObjRet| p.handle = cmdbuf).ret
    }

    fn MTLCommandBuffer_computeCommandEncoder(cmdbuf: u64, concurrent: bool) -> u64 {
        call(slot::MTL_COMMAND_BUFFER_COMPUTE_COMMAND_ENCODER, |p: &mut UnixcallGenericObjUint64ObjRet| {
            p.handle = cmdbuf;
            p.arg = u64::from(concurrent);
        })
        .ret
    }

    fn MTLCommandBuffer_renderCommandEncoder(cmdbuf: u64, info: *mut WMTRenderPassInfo) -> u64 {
        call(slot::MTL_COMMAND_BUFFER_RENDER_COMMAND_ENCODER, |p: &mut UnixcallGenericObjUint64ObjRet| {
            p.handle = cmdbuf;
            p.arg = addr(info);
        })
        .ret
    }

    fn MTLCommandEncoder_endEncoding(encoder: u64) {
        call(slot::MTL_COMMAND_ENCODER_END_ENCODING, |p: &mut UnixcallGenericObjNoret| p.handle = encoder);
    }

    fn MTLBlitCommandEncoder_encodeCommands(encoder: u64, cmd_head: *const WmtcmdBase) {
        call(slot::MTL_BLIT_COMMAND_ENCODER_ENCODE_COMMANDS, |p: &mut UnixcallGenericObjCmdNoret| {
            p.encoder = encoder;
            p.cmd_head = mem_ptr(cmd_head);
        });
    }

    fn MTLComputeCommandEncoder_encodeCommands(encoder: u64, cmd_head: *const WmtcmdBase) {
        call(slot::MTL_COMPUTE_COMMAND_ENCODER_ENCODE_COMMANDS, |p: &mut UnixcallGenericObjCmdNoret| {
            p.encoder = encoder;
            p.cmd_head = mem_ptr(cmd_head);
        });
    }

    fn MTLRenderCommandEncoder_encodeCommands(encoder: u64, cmd_head: *const WmtcmdBase) {
        call(slot::MTL_RENDER_COMMAND_ENCODER_ENCODE_COMMANDS, |p: &mut UnixcallGenericObjCmdNoret| {
            p.encoder = encoder;
            p.cmd_head = mem_ptr(cmd_head);
        });
    }

    fn MTLCommandBuffer_presentDrawable(cmdbuf: u64, drawable: u64) {
        call(slot::MTL_COMMAND_BUFFER_PRESENT_DRAWABLE, |p: &mut UnixcallGenericObjObjNoret| {
            p.handle = cmdbuf;
            p.arg = drawable;
        });
    }

    fn MTLCommandBuffer_presentDrawableAfterMinimumDuration(cmdbuf: u64, drawable: u64, after: f64) {
        call(
            slot::MTL_COMMAND_BUFFER_PRESENT_DRAWABLE_AFTER_MINIMUM_DURATION,
            |p: &mut UnixcallGenericObjObjDoubleNoret| {
                p.handle = cmdbuf;
                p.arg0 = drawable;
                p.arg1 = after;
            },
        );
    }

    fn MTLCommandEncoder_setLabel(encoder: u64, label: u64) {
        call(slot::MTL_COMMAND_ENCODER_SET_LABEL, |p: &mut UnixcallGenericObjObjNoret| {
            p.handle = encoder;
            p.arg = label;
        });
    }

    fn MTLCommandBuffer_error(cmdbuf: u64) -> u64 {
        call(slot::MTL_COMMAND_BUFFER_ERROR, |p: &mut UnixcallGenericObjObjRet| p.handle = cmdbuf).ret
    }

    fn MTLCommandBuffer_logs(cmdbuf: u64) -> u64 {
        call(slot::MTL_COMMAND_BUFFER_LOGS, |p: &mut UnixcallGenericObjObjRet| p.handle = cmdbuf).ret
    }

    fn MTLCommandBuffer_encodeWaitForEvent(cmdbuf: u64, event: u64, value: u64) {
        call(slot::MTL_COMMAND_BUFFER_ENCODE_WAIT_FOR_EVENT, |p: &mut UnixcallGenericObjObjUint64Noret| {
            p.handle = cmdbuf;
            p.arg0 = event;
            p.arg1 = value;
        });
    }

    fn MTLCommandBuffer_blitCommandEncoderWithSampleBuffers(
        cmdbuf: u64,
        sample_buffer_attachments: *mut WMTSampleBufferAttachmentInfo,
        num_sample_buffer_attachments: u64,
    ) -> u64 {
        call(
            slot::MTL_COMMAND_BUFFER_BLIT_COMMAND_ENCODER_WITH_SAMPLE_BUFFERS,
            |p: &mut UnixcallMtlcommandbufferBlitcommandencoderwithsamplebuffers| {
                p.cmdbuf = cmdbuf;
                p.attachments = mem_ptr(sample_buffer_attachments);
                p.num_attachments = num_sample_buffer_attachments;
            },
        )
        .ret
    }

    fn MTLCommandBuffer_property(cmdbuf: u64, prop: u32) -> u64 {
        call(slot::MTL_COMMAND_BUFFER_PROPERTY, |p: &mut UnixcallGenericObjUint64Uint64Ret| {
            p.handle = cmdbuf;
            p.arg = u64::from(prop);
        })
        .ret
    }

    fn MTLCommandBuffer_computeCommandEncoderWithSampleBuffers(
        cmdbuf: u64,
        concurrent: bool,
        sample_buffer_attachments: *mut WMTSampleBufferAttachmentInfo,
        num_sample_buffer_attachments: u64,
    ) -> u64 {
        call(
            slot::MTL_COMMAND_BUFFER_COMPUTE_COMMAND_ENCODER_WITH_SAMPLE_BUFFERS,
            |p: &mut UnixcallMtlcommandbufferComputecommandencoderwithsamplebuffers| {
                p.cmdbuf = cmdbuf;
                p.attachments = mem_ptr(sample_buffer_attachments);
                p.num_attachments = num_sample_buffer_attachments;
                p.concurrent = u64::from(concurrent);
            },
        )
        .ret
    }
}
