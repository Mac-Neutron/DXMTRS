//! Surfaces on the core: a colour target read back into a system-memory surface's bytes.

use std::sync::Arc;

use dxmt_core::{deptrack::Access, encoder::stage, staging::Staging};
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER, WMTOrigin, WMTSize,
    WmtcmdBlitCopyFromTextureToBuffer,
};

use super::{CoreDevice, TargetRef};

impl CoreDevice {
    /// Copies the target `source` into `dst`, rows `dst_pitch` bytes apart (`GetRenderTargetData`).
    ///
    /// A blit of the source's one level and slice into the device's read-back buffer, that
    /// subresource tracked as read so the blit waits for the passes that wrote it, then a wait
    /// for its chunk (C4-12). The rows are the source's width in its texels. False, and nothing
    /// recorded, when `source` does not resolve or `dst` is too small.
    pub fn read_back(&mut self, source: TargetRef, dst: &mut [u8], dst_pitch: usize) -> bool {
        let Some(target) = self.resolve(source) else {
            return false;
        };
        let shape = target.shape;
        let (width, height) = (shape.width, shape.height);
        let bytes_per_row = width * shape.texel_bytes;
        let row = bytes_per_row as usize;
        let needed = dst_pitch * (height as usize).saturating_sub(1) + row;
        if height == 0 || dst_pitch < row || dst.len() < needed {
            return false;
        }
        let (level, slice) = (shape.level, shape.slice);
        let texture = target.texture;
        let length = u64::from(bytes_per_row) * u64::from(height);
        // One buffer, remade when a source of another size or row is read.
        let staging = match &self.readback {
            Some((staging, kept_row))
                if staging.length() == length && *kept_row == bytes_per_row =>
            {
                Arc::clone(staging)
            }
            _ => {
                let staging = Arc::new(Staging::new(
                    self.device.device(),
                    length,
                    bytes_per_row,
                    bytes_per_row * height,
                ));
                self.readback = Some((Arc::clone(&staging), bytes_per_row));
                staging
            }
        };
        self.close_pass();
        let seq = self.device.queue().current_seq();
        staging.use_copy_destination(seq);
        let destination = Arc::clone(&staging);
        self.emit(move |enc| {
            enc.start_blit_pass();
            let src =
                enc.access_texture::<{ stage::COMPUTE }>(&texture, level, slice, Access::READ);
            if let Some((allocation, offset)) =
                enc.access_buffer::<{ stage::COMPUTE }>(destination.buffer(), Access::WRITE)
            {
                enc.encode_blit_command(WmtcmdBlitCopyFromTextureToBuffer {
                    r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
                    reserved: [0; 3],
                    next: dxmt_core::encoder::NO_NEXT,
                    src,
                    slice,
                    level,
                    origin: WMTOrigin { x: 0, y: 0, z: 0 },
                    size: WMTSize {
                        width: u64::from(width),
                        height: u64::from(height),
                        depth: 1,
                    },
                    dst: allocation.buffer(),
                    offset,
                    bytes_per_row,
                    bytes_per_image: bytes_per_row * height,
                });
            }
            enc.end_pass();
        });
        self.sync_to(seq);
        let source = staging.mapped_immediate_memory();
        if source.is_null() {
            return false;
        }
        // SAFETY: the read-back buffer's mapping, `length` (row times height) bytes, which the GPU finished writing
        // (`sync_to`) and nothing else writes until the next read-back.
        let source = unsafe { core::slice::from_raw_parts(source, row * height as usize) };
        for (out, line) in dst.chunks_mut(dst_pitch).zip(source.chunks(row)) {
            out[..row].copy_from_slice(line);
        }
        true
    }
}

#[cfg(test)]
mod tests;
