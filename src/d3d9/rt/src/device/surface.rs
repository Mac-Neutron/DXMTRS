//! Surfaces on the core: read-back, and the resolve of a multisampled target.
//!
//! A colour target read back into a system-memory surface's bytes (`GetRenderTargetData`), a
//! rect of the back buffer read as Present shows it (DXMTRS C4c-3a), and a multisampled target
//! resolved into a single-sample one (`StretchRect`'s resolve).

use std::sync::Arc;

use dxmt_core::{
    deptrack::Access,
    encoder::stage,
    format::{CAP_RESOLVE, device_capability_map},
    staging::Staging,
};
use mtld3d_core::dirty_rect::DirtyRect;
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER, WMTOrigin, WMTSize,
    WmtcmdBlitCopyFromTextureToBuffer,
};

use super::{
    CoreDevice, TargetRef,
    target::{Resolved, Shape},
};

impl CoreDevice {
    /// Copies the target `source` into `dst`, rows `dst_pitch` bytes apart (`GetRenderTargetData`).
    ///
    /// A blit of the source's one level and slice into the device's read-back buffer (kept while
    /// long enough, made anew only to grow), that subresource tracked as read so the blit waits
    /// for the passes that wrote it, then a wait for its chunk (C4-12). The rows are the source's
    /// width in its texels. False, and nothing recorded, when `source` does not resolve, is
    /// multisampled (Metal blits no multisampled texture; D3D9 refuses one, DXMTRS C4b-3d5) or
    /// `dst` is too small.
    pub fn read_back(&mut self, source: TargetRef, dst: &mut [u8], dst_pitch: usize) -> bool {
        let Some(target) = self.resolve(source) else {
            return false;
        };
        let shape = target.shape;
        if shape.samples > 1 {
            return false;
        }
        let whole = DirtyRect::full(shape.width, shape.height);
        let Some(sizes) = rect_bytes(shape, whole, dst.len(), dst_pitch) else {
            return false;
        };
        self.read_rect(target, whole, sizes, dst, dst_pitch)
    }

    /// Copies `rect` of the back buffer, as Present would show it now, into `dst`.
    ///
    /// `rect` is in texels of the BGRA8 back buffer; its rows land `dst_pitch` bytes apart. A
    /// multisampled back buffer is first resolved into the presented texture as Present resolves
    /// it, through the view its last pass or Clear wrote it in; a single-sample one is read as
    /// drawn. Then [`CoreDevice::read_back`]'s blit of the rect, wait and copy. False, and
    /// nothing recorded, without a back buffer, for an empty rect or one that leaves the back
    /// buffer, a pitch under the rect's row, or a `dst` too small (DXMTRS C4c-3a).
    #[must_use]
    pub fn read_presented(&mut self, rect: DirtyRect, dst: &mut [u8], dst_pitch: usize) -> bool {
        let (Some(shape), Some(back_buffer)) =
            (self.shape(TargetRef::BackBuffer), &self.back_buffer)
        else {
            return false;
        };
        let Some(sizes) = rect_bytes(shape, rect, dst.len(), dst_pitch) else {
            return false;
        };
        let (texture, key) = back_buffer.presented();
        let presented = Resolved {
            texture: Arc::clone(texture),
            key,
            shape: Shape {
                samples: 1,
                ..shape
            },
        };
        self.resolve_presented();
        self.read_rect(presented, rect, sizes, dst, dst_pitch)
    }

    /// Copies `rect` of `source`'s one subresource into `dst`, rows `dst_pitch` bytes apart.
    ///
    /// `bytes_per_row` and `bytes_per_image` are the rect's row and rows, which `rect_bytes`
    /// answered for this rect, source and `dst`. The blit packs the rows tightly from the start of
    /// the read-back buffer, and they are copied out by the rect's row. False when the buffer has
    /// no mapping this process reaches, after the wait.
    fn read_rect(
        &mut self,
        source: Resolved,
        rect: DirtyRect,
        (bytes_per_row, bytes_per_image): (u32, u32),
        dst: &mut [u8],
        dst_pitch: usize,
    ) -> bool {
        let length = u64::from(bytes_per_image);
        // Grown, never shrunk: a read of fewer bytes keeps the buffer there (DXMTRS C4c-3a).
        let staging = match &self.readback {
            Some(staging) if staging.length() >= length => Arc::clone(staging),
            _ => {
                let staging = Arc::new(Staging::new(
                    self.device.device(),
                    length,
                    bytes_per_row,
                    bytes_per_image,
                ));
                self.readback = Some(Arc::clone(&staging));
                staging
            }
        };
        let (texture, level, slice) = (source.texture, source.shape.level, source.shape.slice);
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
                    origin: WMTOrigin {
                        x: u64::from(rect.x),
                        y: u64::from(rect.y),
                        z: 0,
                    },
                    size: WMTSize {
                        width: u64::from(rect.w),
                        height: u64::from(rect.h),
                        depth: 1,
                    },
                    dst: allocation.buffer(),
                    offset,
                    bytes_per_row,
                    bytes_per_image,
                });
            }
            enc.end_pass();
        });
        self.sync_to(seq);
        let mapped = staging.mapped_immediate_memory();
        if mapped.is_null() {
            return false;
        }
        let row = bytes_per_row as usize;
        // SAFETY: the read-back buffer's mapping, at least `bytes_per_image` bytes (it is kept only
        // while that long), which the GPU finished writing (`sync_to`) and nothing else writes
        // until the next read-back.
        let packed = unsafe { core::slice::from_raw_parts(mapped, bytes_per_image as usize) };
        for (out, line) in dst.chunks_mut(dst_pitch).zip(packed.chunks(row)) {
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

/// The bytes of a row of `rect` and of all its rows, at `shape`'s texel size, when it is read.
///
/// None for an empty rect or one that leaves `shape`'s extent, rows `dst_pitch` apart that do not
/// hold a row, `dst_len` bytes that do not hold the rows, or 4 GiB or more (the blit names its
/// bytes in `u32`). Counted in `u64`, each converted once.
fn rect_bytes(
    shape: Shape,
    rect: DirtyRect,
    dst_len: usize,
    dst_pitch: usize,
) -> Option<(u32, u32)> {
    let inside = |start: u32, extent: u32, edge: u32| {
        start.checked_add(extent).is_some_and(|end| end <= edge)
    };
    if rect.w == 0
        || rect.h == 0
        || !inside(rect.x, rect.w, shape.width)
        || !inside(rect.y, rect.h, shape.height)
    {
        return None;
    }
    let row = u64::from(rect.w) * u64::from(shape.texel_bytes);
    let pitch = u64::try_from(dst_pitch).ok()?;
    let needed = pitch.checked_mul(u64::from(rect.h - 1))?.checked_add(row)?;
    if pitch < row || u64::try_from(dst_len).ok()? < needed {
        return None;
    }
    let image = row.checked_mul(u64::from(rect.h))?;
    Some((u32::try_from(row).ok()?, u32::try_from(image).ok()?))
}

#[cfg(test)]
mod tests;
