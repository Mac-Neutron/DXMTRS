//! Surfaces on the core: read-back, and the resolve of a multisampled target.
//!
//! A colour target read back into a system-memory surface's bytes (`GetRenderTargetData`), and a
//! multisampled target resolved into a single-sample one (`StretchRect`'s resolve).

use std::sync::Arc;

use dxmt_core::{
    deptrack::Access,
    encoder::stage,
    format::{CAP_RESOLVE, device_capability_map},
    staging::Staging,
};
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
    /// recorded, when `source` does not resolve, is multisampled (Metal blits no multisampled
    /// texture; D3D9 refuses one, DXMTRS C4b-3d5) or `dst` is too small.
    pub fn read_back(&mut self, source: TargetRef, dst: &mut [u8], dst_pitch: usize) -> bool {
        let Some(target) = self.resolve(source) else {
            return false;
        };
        let shape = target.shape;
        if shape.samples > 1 {
            return false;
        }
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

    /// Resolves the multisampled target `source` into the single-sample target `destination`.
    ///
    /// False, and nothing recorded, unless `source` has more than one sample, `destination` one,
    /// both resolve with equal sizes and stored formats, and Metal resolves that format on this
    /// device (`CAP_RESOLVE`; the 32-bit floats only from Apple9). Otherwise the open render pass
    /// ends, and the core's resolve of the source's view into the destination's is recorded, both
    /// in the format a pass or Clear last wrote the source through (its stored one or its sRGB
    /// twin's, as mtld3d resolves; equal stored formats have the same twin). The optimizer folds
    /// it into the source's render pass when that pass comes right before it (DXMTRS C4b-3d5).
    #[must_use]
    pub fn resolve_into(&mut self, source: TargetRef, destination: TargetRef) -> bool {
        let (Some(from), Some(to)) = (self.resolve(source), self.resolve(destination)) else {
            return false;
        };
        let (shape, into) = (from.shape, to.shape);
        if shape.samples <= 1
            || into.samples != 1
            || (shape.width, shape.height) != (into.width, into.height)
            || shape.pixel_format != into.pixel_format
            || !self.resolves(shape.pixel_format)
        {
            return false;
        }
        let format = self.last_written(source).unwrap_or(shape.pixel_format);
        debug_assert!(
            format == into.pixel_format || format == into.srgb_format,
            "the destination has a view in the format the source was written in"
        );
        self.close_pass();
        let (texture, key) = (from.texture, from.key.with_format(format));
        let (resolved, view) = (to.texture, to.key.with_format(format));
        self.emit(move |enc| enc.resolve_texture(&texture, key, &resolved, view));
        true
    }

    /// Whether Metal resolves a multisampled texture of `pixel_format` on this device.
    ///
    /// dxmt-core's capability table of the device, built at each call: a `StretchRect` resolve's
    /// cost, which Present's own resolve of the BGRA8 back buffer does not pay.
    fn resolves(&self, pixel_format: u32) -> bool {
        // SAFETY: the device's handle, live while the core device is.
        let caps = unsafe { device_capability_map(self.device.device().handle()) };
        caps.get(&pixel_format)
            .is_some_and(|caps| caps & CAP_RESOLVE != 0)
    }
}

#[cfg(test)]
mod tests;
