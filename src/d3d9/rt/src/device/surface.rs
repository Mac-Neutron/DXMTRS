//! Surfaces on the core: the back buffer read back into a system-memory surface's bytes.

use std::sync::Arc;

use dxmt_core::{deptrack::Access, encoder::stage, staging::Staging};
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER, WMTOrigin, WMTSize,
    WmtcmdBlitCopyFromTextureToBuffer,
};

use super::{CoreDevice, back_buffer_view};

impl CoreDevice {
    /// Copies the back buffer into `dst`, rows `dst_pitch` bytes apart (`GetRenderTargetData`).
    ///
    /// A blit into the device's read-back buffer, the back buffer tracked as read so the blit
    /// waits for the passes that wrote it, then a wait for its chunk (C4-12). False, and nothing
    /// recorded, when there is no back buffer or `dst` is too small.
    pub fn read_back(&mut self, dst: &mut [u8], dst_pitch: usize) -> bool {
        let Some(back_buffer) = &self.back_buffer else {
            return false;
        };
        let (width, height) = (back_buffer.width, back_buffer.height);
        let row = width as usize * 4;
        let needed = dst_pitch * (height as usize).saturating_sub(1) + row;
        if height == 0 || dst_pitch < row || dst.len() < needed {
            return false;
        }
        let (texture, _) = back_buffer_view(back_buffer);
        let bytes_per_row = width * 4;
        let length = u64::from(bytes_per_row) * u64::from(height);
        let staging = Arc::clone(self.readback.get_or_insert_with(|| {
            Arc::new(Staging::new(
                self.device.device(),
                length,
                bytes_per_row,
                bytes_per_row * height,
            ))
        }));
        if staging.length() != length {
            return false;
        }
        self.close_pass();
        let seq = self.device.queue().current_seq();
        staging.use_copy_destination(seq);
        let destination = Arc::clone(&staging);
        self.emit(move |enc| {
            enc.start_blit_pass();
            let src = enc.access_texture::<{ stage::COMPUTE }>(&texture, 0, 0, Access::READ);
            if let Some((allocation, offset)) =
                enc.access_buffer::<{ stage::COMPUTE }>(destination.buffer(), Access::WRITE)
            {
                enc.encode_blit_command(WmtcmdBlitCopyFromTextureToBuffer {
                    r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
                    reserved: [0; 3],
                    next: dxmt_core::encoder::NO_NEXT,
                    src,
                    slice: 0,
                    level: 0,
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
