//! Slot 36: a PE blit command list on a blit command encoder.

use core::ffi::c_void;

use objc2::{extern_methods, runtime::ProtocolObject};
use objc2_foundation::NSRange;
use objc2_metal::{
    MTLBlitCommandEncoder, MTLBlitOption, MTLBuffer, MTLCounterSampleBuffer, MTLFence,
    MTLIndirectCommandBuffer, MTLOrigin, MTLSize, MTLTexture,
};
use winemetal_wire::{
    MemPtr, UnixcallGenericObjCmdNoret, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE, WMT_BLIT_COMMAND_COPY_TEXTURE,
    WMT_BLIT_COMMAND_FILL_BUFFER, WMT_BLIT_COMMAND_GENERATE_MIPMAPS,
    WMT_BLIT_COMMAND_RESET_COMMANDS_IN_BUFFER, WMT_BLIT_COMMAND_RESOLVE_COUNTERS,
    WMT_BLIT_COMMAND_UPDATE_FENCE, WMT_BLIT_COMMAND_WAIT_FOR_FENCE, WmtcmdBase,
    WmtcmdBlitCopyFromBufferToBuffer, WmtcmdBlitCopyFromBufferToTexture,
    WmtcmdBlitCopyFromBufferToTextureWithblitoption, WmtcmdBlitCopyFromTextureToBuffer,
    WmtcmdBlitCopyFromTextureToBufferWithblitoption, WmtcmdBlitCopyFromTextureToTexture,
    WmtcmdBlitCopyTexture, WmtcmdBlitFenceOp, WmtcmdBlitFillbuffer, WmtcmdBlitGenerateMipmaps,
    WmtcmdBlitResetcommands, WmtcmdBlitResolvecounters,
};

use super::{body, origin, range, size};
use crate::{
    ffi::{AbortOnPanic, STATUS_SUCCESS, mem_ref, params},
    float_cast::u64_to_usize,
    handle::obj,
};

type Encoder = ProtocolObject<dyn MTLBlitCommandEncoder>;
type Buffer = ProtocolObject<dyn MTLBuffer>;
type Texture = ProtocolObject<dyn MTLTexture>;
type Fence = ProtocolObject<dyn MTLFence>;

/// Slot 36: each command of the list on the encoder, in order.
///
/// Nop and unknown types are skipped and the walk goes on. A nil encoder sends nothing, as the
/// C's sends to nil did.
///
/// # Safety
///
/// `args` points to the slot's params block, whose handles are 0 or live objects of their kinds;
/// `cmd_head` is null or the first node of a list the PE built: each node live and of the
/// structure its type names, its handles 0 or live objects, its `next` null or the next node.
#[unsafe(export_name = "_MTLBlitCommandEncoder_encodeCommands")]
pub unsafe extern "C-unwind" fn encode_commands(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjCmdNoret>(args) };
    // SAFETY: as above.
    let Some(encoder) = (unsafe { obj::<Encoder>(p.encoder) }) else {
        return STATUS_SUCCESS;
    };
    let mut at = p.cmd_head;
    // SAFETY: as above.
    while let Some(cmd) = unsafe { mem_ref::<WmtcmdBase>(at) } {
        // SAFETY: as above.
        unsafe { encode(encoder, cmd.r#type, at) };
        at = cmd.next;
    }
    STATUS_SUCCESS
}

/// The blit sends a list's commands make, each object parameter nullable: 0 goes out as nil.
trait RawBlit {
    // allow: the class of CONVENTIONS' exception 4: Metal's selector takes these arguments one by
    // one, and a re-declaration must take them as the selector does.
    #[allow(clippy::too_many_arguments)]
    unsafe fn copy_buffer_to_texture(
        &self,
        source: Option<&Buffer>,
        source_offset: usize,
        source_bytes_per_row: usize,
        source_bytes_per_image: usize,
        source_size: MTLSize,
        destination: Option<&Texture>,
        destination_slice: usize,
        destination_level: usize,
        destination_origin: MTLOrigin,
    );
    // allow: as above.
    #[allow(clippy::too_many_arguments)]
    unsafe fn copy_buffer_to_texture_options(
        &self,
        source: Option<&Buffer>,
        source_offset: usize,
        source_bytes_per_row: usize,
        source_bytes_per_image: usize,
        source_size: MTLSize,
        destination: Option<&Texture>,
        destination_slice: usize,
        destination_level: usize,
        destination_origin: MTLOrigin,
        options: MTLBlitOption,
    );
    // allow: as above.
    #[allow(clippy::too_many_arguments)]
    unsafe fn copy_texture_to_buffer(
        &self,
        source: Option<&Texture>,
        source_slice: usize,
        source_level: usize,
        source_origin: MTLOrigin,
        source_size: MTLSize,
        destination: Option<&Buffer>,
        destination_offset: usize,
        destination_bytes_per_row: usize,
        destination_bytes_per_image: usize,
    );
    // allow: as above.
    #[allow(clippy::too_many_arguments)]
    unsafe fn copy_texture_to_buffer_options(
        &self,
        source: Option<&Texture>,
        source_slice: usize,
        source_level: usize,
        source_origin: MTLOrigin,
        source_size: MTLSize,
        destination: Option<&Buffer>,
        destination_offset: usize,
        destination_bytes_per_row: usize,
        destination_bytes_per_image: usize,
        options: MTLBlitOption,
    );
    // allow: as above.
    #[allow(clippy::too_many_arguments)]
    unsafe fn copy_texture_to_texture(
        &self,
        source: Option<&Texture>,
        source_slice: usize,
        source_level: usize,
        source_origin: MTLOrigin,
        source_size: MTLSize,
        destination: Option<&Texture>,
        destination_slice: usize,
        destination_level: usize,
        destination_origin: MTLOrigin,
    );
    unsafe fn copy_buffer_to_buffer(
        &self,
        source: Option<&Buffer>,
        source_offset: usize,
        destination: Option<&Buffer>,
        destination_offset: usize,
        size: usize,
    );
    unsafe fn copy_texture(&self, source: Option<&Texture>, destination: Option<&Texture>);
    fn generate_mipmaps(&self, texture: Option<&Texture>);
    fn update_fence(&self, fence: Option<&Fence>);
    fn wait_for_fence(&self, fence: Option<&Fence>);
    fn fill_buffer(&self, buffer: Option<&Buffer>, range: NSRange, value: u8);
    unsafe fn resolve_counters(
        &self,
        sample_buffer: Option<&ProtocolObject<dyn MTLCounterSampleBuffer>>,
        range: NSRange,
        destination: Option<&Buffer>,
        destination_offset: usize,
    );
    unsafe fn reset_commands(
        &self,
        buffer: Option<&ProtocolObject<dyn MTLIndirectCommandBuffer>>,
        range: NSRange,
    );
}

impl RawBlit for Encoder {
    extern_methods!(
        /// `-copyFromBuffer:…:destinationOrigin:`.
        #[unsafe(method(copyFromBuffer:sourceOffset:sourceBytesPerRow:sourceBytesPerImage:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:))]
        #[unsafe(method_family = none)]
        unsafe fn copy_buffer_to_texture(
            &self,
            source: Option<&Buffer>,
            source_offset: usize,
            source_bytes_per_row: usize,
            source_bytes_per_image: usize,
            source_size: MTLSize,
            destination: Option<&Texture>,
            destination_slice: usize,
            destination_level: usize,
            destination_origin: MTLOrigin,
        );

        /// `-copyFromBuffer:…:destinationOrigin:options:`.
        #[unsafe(method(copyFromBuffer:sourceOffset:sourceBytesPerRow:sourceBytesPerImage:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:options:))]
        #[unsafe(method_family = none)]
        unsafe fn copy_buffer_to_texture_options(
            &self,
            source: Option<&Buffer>,
            source_offset: usize,
            source_bytes_per_row: usize,
            source_bytes_per_image: usize,
            source_size: MTLSize,
            destination: Option<&Texture>,
            destination_slice: usize,
            destination_level: usize,
            destination_origin: MTLOrigin,
            options: MTLBlitOption,
        );

        /// `-copyFromTexture:…:destinationBytesPerImage:`.
        #[unsafe(method(copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:))]
        #[unsafe(method_family = none)]
        unsafe fn copy_texture_to_buffer(
            &self,
            source: Option<&Texture>,
            source_slice: usize,
            source_level: usize,
            source_origin: MTLOrigin,
            source_size: MTLSize,
            destination: Option<&Buffer>,
            destination_offset: usize,
            destination_bytes_per_row: usize,
            destination_bytes_per_image: usize,
        );

        /// `-copyFromTexture:…:destinationBytesPerImage:options:`.
        #[unsafe(method(copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:options:))]
        #[unsafe(method_family = none)]
        unsafe fn copy_texture_to_buffer_options(
            &self,
            source: Option<&Texture>,
            source_slice: usize,
            source_level: usize,
            source_origin: MTLOrigin,
            source_size: MTLSize,
            destination: Option<&Buffer>,
            destination_offset: usize,
            destination_bytes_per_row: usize,
            destination_bytes_per_image: usize,
            options: MTLBlitOption,
        );

        /// `-copyFromTexture:…:toTexture:…:destinationOrigin:`.
        #[unsafe(method(copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:))]
        #[unsafe(method_family = none)]
        unsafe fn copy_texture_to_texture(
            &self,
            source: Option<&Texture>,
            source_slice: usize,
            source_level: usize,
            source_origin: MTLOrigin,
            source_size: MTLSize,
            destination: Option<&Texture>,
            destination_slice: usize,
            destination_level: usize,
            destination_origin: MTLOrigin,
        );

        /// `-copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:`.
        #[unsafe(method(copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:))]
        #[unsafe(method_family = none)]
        unsafe fn copy_buffer_to_buffer(
            &self,
            source: Option<&Buffer>,
            source_offset: usize,
            destination: Option<&Buffer>,
            destination_offset: usize,
            size: usize,
        );

        /// `-copyFromTexture:toTexture:`.
        #[unsafe(method(copyFromTexture:toTexture:))]
        #[unsafe(method_family = none)]
        unsafe fn copy_texture(&self, source: Option<&Texture>, destination: Option<&Texture>);

        /// `-generateMipmapsForTexture:`.
        #[unsafe(method(generateMipmapsForTexture:))]
        #[unsafe(method_family = none)]
        fn generate_mipmaps(&self, texture: Option<&Texture>);

        /// `-updateFence:`.
        #[unsafe(method(updateFence:))]
        #[unsafe(method_family = none)]
        fn update_fence(&self, fence: Option<&Fence>);

        /// `-waitForFence:`.
        #[unsafe(method(waitForFence:))]
        #[unsafe(method_family = none)]
        fn wait_for_fence(&self, fence: Option<&Fence>);

        /// `-fillBuffer:range:value:`.
        #[unsafe(method(fillBuffer:range:value:))]
        #[unsafe(method_family = none)]
        fn fill_buffer(&self, buffer: Option<&Buffer>, range: NSRange, value: u8);

        /// `-resolveCounters:inRange:destinationBuffer:destinationOffset:`.
        #[unsafe(method(resolveCounters:inRange:destinationBuffer:destinationOffset:))]
        #[unsafe(method_family = none)]
        unsafe fn resolve_counters(
            &self,
            sample_buffer: Option<&ProtocolObject<dyn MTLCounterSampleBuffer>>,
            range: NSRange,
            destination: Option<&Buffer>,
            destination_offset: usize,
        );

        /// `-resetCommandsInBuffer:withRange:`.
        #[unsafe(method(resetCommandsInBuffer:withRange:))]
        #[unsafe(method_family = none)]
        unsafe fn reset_commands(
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
unsafe fn encode(encoder: &Encoder, ty: u16, at: MemPtr) {
    match ty {
        WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitCopyFromBufferToBuffer>(at) };
            // SAFETY: as above.
            let source = unsafe { obj(c.src) };
            // SAFETY: as above.
            let destination = unsafe { obj(c.dst) };
            // SAFETY: the C's send of the guest's values: Metal takes them as the C's.
            unsafe {
                encoder.copy_buffer_to_buffer(
                    source,
                    u64_to_usize(c.src_offset),
                    destination,
                    u64_to_usize(c.dst_offset),
                    u64_to_usize(c.copy_length),
                );
            }
        }
        WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitCopyFromBufferToTexture>(at) };
            // SAFETY: as above.
            let source = unsafe { obj(c.src) };
            // SAFETY: as above.
            let destination = unsafe { obj(c.dst) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                encoder.copy_buffer_to_texture(
                    source,
                    u64_to_usize(c.src_offset),
                    c.bytes_per_row as usize,
                    c.bytes_per_image as usize,
                    size(&c.size),
                    destination,
                    c.slice as usize,
                    c.level as usize,
                    origin(&c.origin),
                );
            }
        }
        WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitCopyFromBufferToTextureWithblitoption>(at) };
            // SAFETY: as above.
            let source = unsafe { obj(c.src) };
            // SAFETY: as above.
            let destination = unsafe { obj(c.dst) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                encoder.copy_buffer_to_texture_options(
                    source,
                    u64_to_usize(c.src_offset),
                    c.bytes_per_row as usize,
                    c.bytes_per_image as usize,
                    size(&c.size),
                    destination,
                    c.slice as usize,
                    usize::from(c.level),
                    origin(&c.origin),
                    MTLBlitOption(usize::from(c.options)),
                );
            }
        }
        WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitCopyFromTextureToBuffer>(at) };
            // SAFETY: as above.
            let source = unsafe { obj(c.src) };
            // SAFETY: as above.
            let destination = unsafe { obj(c.dst) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                encoder.copy_texture_to_buffer(
                    source,
                    c.slice as usize,
                    c.level as usize,
                    origin(&c.origin),
                    size(&c.size),
                    destination,
                    u64_to_usize(c.offset),
                    c.bytes_per_row as usize,
                    c.bytes_per_image as usize,
                );
            }
        }
        WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitCopyFromTextureToBufferWithblitoption>(at) };
            // SAFETY: as above.
            let source = unsafe { obj(c.src) };
            // SAFETY: as above.
            let destination = unsafe { obj(c.dst) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                encoder.copy_texture_to_buffer_options(
                    source,
                    c.slice as usize,
                    usize::from(c.level),
                    origin(&c.origin),
                    size(&c.size),
                    destination,
                    u64_to_usize(c.offset),
                    c.bytes_per_row as usize,
                    c.bytes_per_image as usize,
                    MTLBlitOption(usize::from(c.options)),
                );
            }
        }
        WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitCopyFromTextureToTexture>(at) };
            // SAFETY: as above.
            let source = unsafe { obj(c.src) };
            // SAFETY: as above.
            let destination = unsafe { obj(c.dst) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                encoder.copy_texture_to_texture(
                    source,
                    c.src_slice as usize,
                    c.src_level as usize,
                    origin(&c.src_origin),
                    size(&c.src_size),
                    destination,
                    c.dst_slice as usize,
                    c.dst_level as usize,
                    origin(&c.dst_origin),
                );
            }
        }
        WMT_BLIT_COMMAND_GENERATE_MIPMAPS => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitGenerateMipmaps>(at) };
            // SAFETY: as above.
            encoder.generate_mipmaps(unsafe { obj(c.texture) });
        }
        WMT_BLIT_COMMAND_UPDATE_FENCE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitFenceOp>(at) };
            // SAFETY: as above.
            encoder.update_fence(unsafe { obj(c.fence) });
        }
        WMT_BLIT_COMMAND_WAIT_FOR_FENCE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitFenceOp>(at) };
            // SAFETY: as above.
            encoder.wait_for_fence(unsafe { obj(c.fence) });
        }
        WMT_BLIT_COMMAND_FILL_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitFillbuffer>(at) };
            let filled = range(u64_to_usize(c.offset), u64_to_usize(c.length));
            // SAFETY: as above.
            encoder.fill_buffer(unsafe { obj(c.buffer) }, filled, c.value);
        }
        WMT_BLIT_COMMAND_RESOLVE_COUNTERS => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitResolvecounters>(at) };
            // SAFETY: as above.
            let sample_buffer = unsafe { obj(c.sample_buffer) };
            // SAFETY: as above.
            let destination = unsafe { obj(c.dst_buffer) };
            let resolved = range(c.start as usize, c.len as usize);
            // SAFETY: the C's send of the guest's values, as above.
            unsafe {
                encoder.resolve_counters(
                    sample_buffer,
                    resolved,
                    destination,
                    u64_to_usize(c.dst_offset),
                );
            }
        }
        WMT_BLIT_COMMAND_RESET_COMMANDS_IN_BUFFER => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitResetcommands>(at) };
            // SAFETY: as above.
            let buffer = unsafe { obj(c.indirect_command_buffer) };
            let reset = range(c.location as usize, c.length as usize);
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.reset_commands(buffer, reset) };
        }
        WMT_BLIT_COMMAND_COPY_TEXTURE => {
            // SAFETY: the caller's contract.
            let c = unsafe { body::<WmtcmdBlitCopyTexture>(at) };
            // SAFETY: as above.
            let source = unsafe { obj(c.src) };
            // SAFETY: as above.
            let destination = unsafe { obj(c.dst) };
            // SAFETY: the C's send of the guest's values, as above.
            unsafe { encoder.copy_texture(source, destination) };
        }
        _ => {}
    }
}
