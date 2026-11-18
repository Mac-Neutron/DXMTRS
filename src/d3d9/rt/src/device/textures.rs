//! D3D9 textures on the core (DXMTRS C4b-3b): a core texture per D3D9 texture, and its uploads.
//!
//! The store keys each core `Texture` by the D3D9 texture's process-unique, never-reused
//! [`TextureId`]. An upload copies a region's bytes into the queue's staging ring at record time
//! and records a blit into the texture in API order (the outline's upload model); consecutive
//! uploads share one blit pass, which the next render pass, Clear, read-back or commit closes.
//! The bytes staged in the chunk being recorded are counted: past the profile's budget the chunk
//! is committed, so a frame that uploads more than a 32-bit process can hold in ring blocks
//! retires them as it goes. The back buffer is written from CPU rows the same way (DXMTRS C4c-3b).

use std::sync::Arc;

use dxmt_core::{
    deptrack::Access,
    encoder::{NO_NEXT, stage},
    resource::{
        Texture, TextureAllocationFlags, TextureViewDescriptor, TextureViewKey, texture_info,
    },
};
use mtld3d_core::{dirty_rect::DirtyRect, ids::TextureId};
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
    WMT_BLIT_COMMAND_GENERATE_MIPMAPS, WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL,
    WMT_BLIT_OPTION_STENCIL_FROM_DEPTH_STENCIL, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
    WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
    WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE3D,
    WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW, WMT_TEXTURE_USAGE_RENDER_TARGET,
    WMT_TEXTURE_USAGE_SHADER_READ, WMTOrigin, WMTSize, WMTTextureInfo,
    WmtcmdBlitCopyFromBufferToTexture, WmtcmdBlitCopyFromBufferToTextureWithblitoption,
    WmtcmdBlitGenerateMipmaps,
};

use super::{
    CoreDevice, LOG_TARGET, TargetRef,
    draw::Stage,
    surface::rect_bytes,
    target::{Resolved, Shape},
};

/// A whole level of a depth texture as CPU planes, for [`CoreDevice::depth_upload`].
///
/// The planes `PlaneLayout::new(w, h, 4)` of `mtld3d_core::depth_texture` lays out fit: depth
/// rows of `w * 4` bytes, stencil rows of `w` rounded up to 4.
pub struct DepthPlanes<'a> {
    /// The depth plane: rows of `w` little-endian `f32`s, `depth_pitch` bytes apart.
    pub depth: &'a [u8],
    pub depth_pitch: usize,
    /// The stencil plane, rows of one byte a texel, and its pitch.
    ///
    /// Read for a format with a stencil plane only.
    pub stencil: Option<(&'a [u8], usize)>,
}

/// A D3D9 texture's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureKind {
    Tex2d,
    Cube,
    Volume,
}

bitflags::bitflags! {
    /// How a core texture is used beyond sampling.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CoreTextureFlags: u8 {
        /// Views in another format (an sRGB twin, a swizzle) will be taken: `PIXEL_FORMAT_VIEW`.
        const VIEW_TWINS = 1 << 0;
        /// A render target (C4b-3d1).
        ///
        /// Made with the render-target usage, tracked a subresource at a time, and zeroed.
        const RENDER_TARGET = 1 << 1;
    }
}

/// What a core texture is made of.
#[derive(Clone, Copy, Debug)]
pub struct CoreTextureDesc {
    /// The `WMTPixelFormat` it is stored in.
    pub pixel_format: u32,
    pub kind: TextureKind,
    pub width: u32,
    pub height: u32,
    /// The volume's depth; 1 otherwise.
    pub depth: u32,
    pub levels: u32,
    /// Its views in another format and its render-target use.
    pub flags: CoreTextureFlags,
    /// The sRGB twin's `WMTPixelFormat` a render target is attached through under sRGB write.
    ///
    /// 0 for none; read only with `VIEW_TWINS` (DXMTRS C4b-3d3).
    pub srgb_format: u32,
    /// The format's block: width and height in texels, size in bytes.
    ///
    /// (1, 1, texel size) for an uncompressed format.
    pub block: (u32, u32, u32),
    /// Samples a texel: 1, or 2, 4 or 8 for a multisampled render target (DXMTRS C4b-3d5).
    ///
    /// A multisampled texture is a one-level 2D render target of the multisample type, whose
    /// views are of that type too.
    pub samples: u8,
}

/// A region of one subresource to upload.
///
/// A level of a slice (a cube face, else 0), a volume's depth range in `z`/`depth`. Extents past
/// the level's edge are clamped to it; the origin is rounded down to the format's block.
#[derive(Clone, Copy, Debug)]
pub struct UploadRegion {
    pub level: u32,
    pub slice: u32,
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

/// A texture and sampler a draw binds at one slot of a stage.
pub struct TextureBind {
    pub(super) stage: Stage,
    pub(super) slot: u8,
    pub(super) texture: Arc<Texture>,
    pub(super) key: TextureViewKey,
    pub(super) sampler: Option<Arc<dxmt_core::sampler::Sampler>>,
}

/// A stored texture: the core texture and the description it was made from.
pub(super) struct Stored {
    texture: Arc<Texture>,
    desc: CoreTextureDesc,
    /// The `WMTPixelFormat` of the view the last pass or Clear wrote it through.
    ///
    /// Its stored format or its sRGB twin's; the stored one until a pass or Clear writes it. A
    /// resolve of a multisampled texture reads it through that view (DXMTRS C4b-3d5).
    last_written: u32,
}

/// The view format of `pixel_format` with `swizzle` in its high bits.
///
/// `swizzle` is Metal's `MTLTextureSwizzle` per channel, red green blue alpha; no swizzle is the
/// format itself.
#[must_use]
pub const fn view_format(pixel_format: u32, swizzle: Option<[u8; 4]>) -> u32 {
    match swizzle {
        Some([r, g, b, a]) => winemetal_sys::wire::encode_format_swizzle(pixel_format, r, g, b, a),
        None => pixel_format,
    }
}

impl CoreDevice {
    /// Makes the core texture of `id`.
    ///
    /// Its storage is uninitialised, as D3D9's contents are, unless it is a render target, which
    /// starts transparent black as mtld3d's do: the core's initializer zeroes each subresource
    /// ahead of the next chunk, so nothing enters the chunk being recorded and an open pass stays
    /// open. A second call for one id replaces the first.
    pub fn texture_create(&mut self, id: TextureId, desc: &CoreTextureDesc) {
        let render_target = desc.flags.contains(CoreTextureFlags::RENDER_TARGET);
        debug_assert!(
            desc.samples == 1
                || (desc.kind == TextureKind::Tex2d && desc.levels == 1 && render_target),
            "a multisampled texture is a one-level 2D render target"
        );
        let (texture_type, array_length) = match desc.kind {
            TextureKind::Tex2d => (two_d(desc.samples), 1),
            TextureKind::Cube => (WMT_TEXTURE_TYPE_CUBE, 1),
            TextureKind::Volume => (WMT_TEXTURE_TYPE3D, 1),
        };
        let mut usage = WMT_TEXTURE_USAGE_SHADER_READ;
        if desc.flags.contains(CoreTextureFlags::VIEW_TWINS) {
            usage |= WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW;
        }
        if render_target {
            usage |= WMT_TEXTURE_USAGE_RENDER_TARGET;
        }
        let info = WMTTextureInfo {
            pixel_format: desc.pixel_format,
            width: desc.width,
            height: desc.height,
            depth: if desc.kind == TextureKind::Volume {
                desc.depth
            } else {
                1
            },
            array_length,
            bits: texture_info::info_bits(
                texture_type,
                desc.levels,
                u32::from(desc.samples),
                usage,
            ),
            options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
            flags: 0,
            mach_port: 0,
            gpu_resource_id: 0,
        };
        let texture = Arc::new(Texture::new(info, self.device.device()));
        if render_target {
            // No `SHADER_READONLY`: a pass that writes one level or face and a draw that samples
            // another are tracked apart.
            let allocation = texture.allocate(TextureAllocationFlags::GPU_PRIVATE);
            texture.rename(Arc::clone(&allocation));
            let slices = if desc.kind == TextureKind::Cube { 6 } else { 1 };
            let initializer = self.device.queue().initializer();
            for slice in 0..slices {
                for level in 0..desc.levels {
                    let _ = initializer.init_texture_zero(&texture, &allocation, slice, level);
                }
            }
            // The next commit carries the initializer's work, even with nothing else recorded.
            self.dirty = true;
        } else {
            texture.rename(texture.allocate(
                TextureAllocationFlags::GPU_PRIVATE | TextureAllocationFlags::SHADER_READONLY,
            ));
        }
        self.textures.insert(
            id,
            Stored {
                texture,
                desc: *desc,
                last_written: desc.pixel_format,
            },
        );
    }

    /// The `WMTPixelFormat` of the view the last pass or Clear wrote `target` through.
    ///
    /// A texture's, or a multisampled back buffer's companion's (DXMTRS C4b-3d5); `None` for a
    /// single-sample back buffer, which nothing resolves, or a texture the store lacks. What a
    /// resolve and Present read a multisampled target through, as mtld3d resolves.
    pub(super) fn last_written(&self, target: TargetRef) -> Option<u32> {
        match target {
            TargetRef::BackBuffer => self
                .back_buffer
                .as_ref()
                .and_then(|back_buffer| back_buffer.companion.as_ref())
                .map(|companion| companion.last_written),
            TargetRef::Texture { id, .. } => self.textures.get(&id).map(|s| s.last_written),
        }
    }

    /// Records that a pass or Clear writes `target` through its view in `format`.
    pub(super) fn note_written(&mut self, target: TargetRef, format: u32) {
        let written = match target {
            TargetRef::BackBuffer => self
                .back_buffer
                .as_mut()
                .and_then(|back_buffer| back_buffer.companion.as_mut())
                .map(|companion| &mut companion.last_written),
            TargetRef::Texture { id, .. } => {
                self.textures.get_mut(&id).map(|s| &mut s.last_written)
            }
        };
        if let Some(written) = written {
            *written = format;
        }
    }

    /// The `WMTPixelFormat` `id`'s texture is stored in, if the store holds it.
    #[must_use]
    pub fn texture_format(&self, id: TextureId) -> Option<u32> {
        self.textures.get(&id).map(|s| s.desc.pixel_format)
    }

    /// A draw's bind of `id`'s texture in `view_format` with `sampler` at `stage`'s `slot`.
    #[must_use]
    pub fn texture_bind(
        &self,
        stage: Stage,
        slot: u8,
        id: TextureId,
        view_format: u32,
        sampler: Option<Arc<dxmt_core::sampler::Sampler>>,
    ) -> Option<TextureBind> {
        let (texture, key) = self.texture_view(id, view_format)?;
        Some(TextureBind {
            stage,
            slot,
            texture,
            key,
            sampler,
        })
    }

    /// A draw's bind of the opaque black at `stage`'s `slot`, with `sampler`.
    pub fn black_bind(
        &mut self,
        stage: Stage,
        slot: u8,
        sampler: Option<Arc<dxmt_core::sampler::Sampler>>,
    ) -> TextureBind {
        let (texture, key) = self.black();
        TextureBind {
            stage,
            slot,
            texture,
            key,
            sampler,
        }
    }

    /// Whether the store holds `id`'s texture.
    #[must_use]
    pub fn has_texture(&self, id: TextureId) -> bool {
        self.textures.contains_key(&id)
    }

    /// Drops the store's texture of `id`; the closures already recorded hold their own.
    ///
    /// When it is the current colour target the back buffer becomes the target. Every slot at
    /// index 1-3 that holds it is emptied, whichever target 0 is (DXMTRS C4b-3d4). Either change
    /// ends the open pass, which draws to it.
    pub fn forget_texture(&mut self, id: TextureId) {
        let holds = |target: &TargetRef| match *target {
            TargetRef::Texture { id: held, .. } => held == id,
            TargetRef::BackBuffer => false,
        };
        if holds(&self.target) {
            if self.render_open {
                self.close_pass();
            }
            self.target = TargetRef::BackBuffer;
        }
        if self.extras.iter().flatten().any(holds) {
            if self.render_open {
                self.close_pass();
            }
            for extra in &mut self.extras {
                if extra.as_ref().is_some_and(holds) {
                    *extra = None;
                }
            }
        }
        if self.depth.is_some_and(|depth| depth.id == id) {
            if self.render_open {
                self.close_pass();
            }
            self.depth = None;
        }
        self.textures.remove(&id);
    }

    /// The size, `WMTPixelFormat` and sample count of level `level` of `id`'s depth texture.
    ///
    /// `None` unless the store holds `id` as a 2D depth texture (a render-target description in
    /// a format with depth-stencil planes) with that level.
    pub(super) fn depth_level(&self, id: TextureId, level: u32) -> Option<(u32, u32, u32, u8)> {
        let desc = &self.textures.get(&id)?.desc;
        if desc.kind != TextureKind::Tex2d
            || !desc.flags.contains(CoreTextureFlags::RENDER_TARGET)
            || level >= desc.levels
            || super::depth::planes(desc.pixel_format) == 0
        {
            return None;
        }
        let extent = |size: u32| size.checked_shr(level).unwrap_or(0).max(1);
        Some((
            extent(desc.width),
            extent(desc.height),
            desc.pixel_format,
            desc.samples,
        ))
    }

    /// `id`'s core texture, if the store holds it.
    pub(super) fn stored_texture(&self, id: TextureId) -> Option<Arc<Texture>> {
        self.textures.get(&id).map(|s| Arc::clone(&s.texture))
    }

    /// The shape of level `level` (and `face`, for a cube) of `id`'s texture as a target.
    ///
    /// `None` unless the store holds `id` as a colour render target with that level (a depth
    /// texture is none), and `face` is a cube's face exactly when it is a cube.
    pub(super) fn texture_shape(
        &self,
        id: TextureId,
        level: u32,
        face: Option<u32>,
    ) -> Option<Shape> {
        let desc = &self.textures.get(&id)?.desc;
        let slice = match (desc.kind, face) {
            (TextureKind::Tex2d, None) => 0,
            (TextureKind::Cube, Some(face)) if face < 6 => face,
            _ => return None,
        };
        if !desc.flags.contains(CoreTextureFlags::RENDER_TARGET)
            || level >= desc.levels
            || super::depth::planes(desc.pixel_format) != 0
        {
            return None;
        }
        let extent = |size: u32| size.checked_shr(level).unwrap_or(0).max(1);
        Some(Shape {
            width: extent(desc.width),
            height: extent(desc.height),
            pixel_format: desc.pixel_format,
            srgb_format: if desc.flags.contains(CoreTextureFlags::VIEW_TWINS) {
                desc.srgb_format
            } else {
                0
            },
            texel_bytes: desc.block.2,
            level,
            slice,
            samples: desc.samples,
        })
    }

    /// `id`'s texture as a pass attaches the subresource `shape` names.
    ///
    /// The view is of that one subresource, a 2D view whose slice is the face (C4b3d1-3): built
    /// here field by field, since the core's own 2D view helper has no slice. A multisampled
    /// texture's is of the multisample type (DXMTRS C4b-3d5).
    pub(super) fn texture_target(&self, id: TextureId, shape: Shape) -> Option<Resolved> {
        let stored = self.textures.get(&id)?;
        let key = TextureViewDescriptor {
            format: shape.pixel_format,
            texture_type: two_d(shape.samples),
            first_mip: shape.level,
            mip_count: 1,
            first_slice: shape.slice,
            array_size: 1,
        }
        .key();
        Some(Resolved {
            texture: Arc::clone(&stored.texture),
            key,
            shape,
        })
    }

    /// The view of `id`'s texture in `format` (its own, an sRGB twin, swizzle bits included).
    #[must_use]
    pub fn texture_view(
        &self,
        id: TextureId,
        format: u32,
    ) -> Option<(Arc<Texture>, TextureViewKey)> {
        let stored = self.textures.get(&id)?;
        let texture_type = match stored.desc.kind {
            TextureKind::Tex2d => WMT_TEXTURE_TYPE2D,
            TextureKind::Cube => WMT_TEXTURE_TYPE_CUBE,
            TextureKind::Volume => WMT_TEXTURE_TYPE3D,
        };
        let key = TextureViewDescriptor {
            format,
            texture_type,
            first_mip: 0,
            mip_count: stored.desc.levels,
            first_slice: 0,
            array_size: if stored.desc.kind == TextureKind::Cube {
                6
            } else {
                1
            },
        }
        .key();
        Some((Arc::clone(&stored.texture), key))
    }

    /// Uploads `region` of `id`'s texture from `bytes`.
    ///
    /// The rows are `bytes_per_row` apart and images (a volume's slices) `bytes_per_image` apart,
    /// starting at the region's origin.
    ///
    /// The rows are repacked to the format's tight row in a staging-ring block now; a blit copies
    /// them at the chunk's encode, after every pass recorded before it. False, and nothing
    /// recorded, when the store has no `id`, the region is empty or `bytes` is short.
    pub fn texture_upload(
        &mut self,
        id: TextureId,
        region: UploadRegion,
        bytes: &[u8],
        bytes_per_row: u32,
        bytes_per_image: u32,
    ) -> bool {
        let Some(stored) = self.textures.get(&id) else {
            return false;
        };
        let texture = Arc::clone(&stored.texture);
        let Some(copy) = Blit::of(&stored.desc, region) else {
            return false;
        };
        let pitches = (bytes_per_row as usize, bytes_per_image as usize);
        self.upload(texture, region.level, &copy, bytes, pitches)
    }

    /// Writes the whole back buffer from CPU rows of its BGRA8 texels, `pitch` bytes apart.
    ///
    /// In application order, as [`CoreDevice::texture_upload`] records an upload: the rows are
    /// packed in a staging-ring block now, and a blit copies them at the chunk's encode, after
    /// every pass recorded before it. A Clear recorded before stays under them, and the next pass
    /// on the back buffer loads them (DXMTRS C4c-3b). False, and nothing recorded, without a back
    /// buffer, for a multisampled one (no blit writes its companion), for an empty extent, a pitch
    /// under the row or `bytes` short of the rows.
    #[must_use]
    pub fn write_presented(&mut self, bytes: &[u8], pitch: usize) -> bool {
        let (Some(shape), Some(back_buffer)) =
            (self.shape(TargetRef::BackBuffer), &self.back_buffer)
        else {
            return false;
        };
        if shape.samples > 1 {
            return false;
        }
        let whole = DirtyRect::full(shape.width, shape.height);
        let Some((row_bytes, _)) = rect_bytes(shape, whole, bytes.len(), pitch) else {
            return false;
        };
        let texture = Arc::clone(back_buffer.presented().0);
        let copy = Blit {
            slice: 0,
            origin: WMTOrigin { x: 0, y: 0, z: 0 },
            size: WMTSize {
                width: u64::from(shape.width),
                height: u64::from(shape.height),
                depth: 1,
            },
            row_bytes,
            rows: shape.height,
            images: 1,
        };
        // One image: its pitch is never read.
        self.upload(texture, 0, &copy, bytes, (pitch, 0))
    }

    /// Uploads the whole of level `level` of `id`'s depth texture from CPU planes.
    ///
    /// In application order, as [`CoreDevice::texture_upload`] records an upload: each plane's
    /// rows are packed in a staging-ring block now, and a blit copies them at the chunk's encode,
    /// after every pass recorded before it (DXMTRS C4c-3b). A `Depth32Float` level takes its
    /// depth plane in one plain copy, as mtld3d's encoder does, and a stencil plane given is not
    /// read; a `Depth32FloatStencil8` level takes a copy a plane, depth first. The copies' rows
    /// are `w * 4` bytes of depth and `w` rounded up to 4 of stencil, as dxmt-core's initializer
    /// copies. False, and nothing recorded, unless the store holds `id` as a single-sample depth
    /// texture in one of those two formats with that level, which fits the blit's `u16`; for a
    /// plane's pitch under its row or its bytes short of its rows; for a missing stencil plane.
    #[must_use]
    pub fn depth_upload(
        &mut self,
        id: TextureId,
        level: u32,
        DepthPlanes {
            depth,
            depth_pitch,
            stencil,
        }: DepthPlanes<'_>,
    ) -> bool {
        let Some((width, height, pixel_format, samples)) = self.depth_level(id, level) else {
            return false;
        };
        let Ok(blit_level) = u16::try_from(level) else {
            return false;
        };
        if samples > 1 {
            return false;
        }
        let shape = Shape {
            width,
            height,
            pixel_format,
            srgb_format: 0,
            texel_bytes: 4,
            level,
            slice: 0,
            samples,
        };
        let whole = DirtyRect::full(width, height);
        let Some((depth_row, _)) = rect_bytes(shape, whole, depth.len(), depth_pitch) else {
            return false;
        };
        let Some(texture) = self.stored_texture(id) else {
            return false;
        };
        match pixel_format {
            WMT_PIXEL_FORMAT_DEPTH32_FLOAT => {
                let copy = Blit {
                    slice: 0,
                    origin: WMTOrigin { x: 0, y: 0, z: 0 },
                    size: WMTSize {
                        width: u64::from(width),
                        height: u64::from(height),
                        depth: 1,
                    },
                    row_bytes: depth_row,
                    rows: height,
                    images: 1,
                };
                self.upload(texture, level, &copy, depth, (depth_pitch, 0))
            }
            WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 => {
                let Some((stencil, stencil_pitch)) = stencil else {
                    return false;
                };
                let stencil_shape = Shape {
                    texel_bytes: 1,
                    ..shape
                };
                let (Some((stencil_row, _)), Some(mut copies)) = (
                    rect_bytes(stencil_shape, whole, stencil.len(), stencil_pitch),
                    plane_copies(blit_level, (width, height)),
                ) else {
                    return false;
                };
                let sources = [
                    (depth, depth_pitch, depth_row),
                    (stencil, stencil_pitch, stencil_row),
                ];
                let mut total = 0;
                for (copy, (plane, pitch, row)) in copies.iter_mut().zip(sources) {
                    let stride = copy.bytes_per_row;
                    let Some(at) = self.stage_rows(plane, (pitch, 0), (row, height, 1), stride)
                    else {
                        return false;
                    };
                    (copy.src, copy.src_offset) = at;
                    total += u64::from(stride) * u64::from(height);
                }
                self.open_blit_pass();
                self.emit(move |enc| {
                    let dst =
                        enc.access_texture::<{ stage::COMPUTE }>(&texture, level, 0, Access::WRITE);
                    for copy in copies {
                        enc.encode_blit_command(WmtcmdBlitCopyFromBufferToTextureWithblitoption {
                            dst,
                            ..copy
                        });
                    }
                });
                self.staged_bytes += total;
                if self.staged_bytes > self.staged_budget {
                    self.commit();
                }
                true
            }
            _ => false,
        }
    }

    /// Records `copy` of `bytes` into level `level` of `texture`, after every pass recorded before.
    ///
    /// `bytes`' rows are `src_row` bytes apart and its images `src_image`. The rows are packed in
    /// a staging-ring block now; the blit runs in the blit pass consecutive uploads share, and
    /// past the staged budget the chunk is committed. False, and nothing recorded, for a row
    /// pitch under the copy's row or `bytes` short of its rows.
    fn upload(
        &mut self,
        texture: Arc<Texture>,
        level: u32,
        copy: &Blit,
        bytes: &[u8],
        (src_row, src_image): (usize, usize),
    ) -> bool {
        let needed = src_image * (copy.images as usize - 1)
            + src_row * (copy.rows as usize - 1)
            + copy.row_bytes as usize;
        if src_row < copy.row_bytes as usize || bytes.len() < needed {
            return false;
        }
        let total = u64::from(copy.row_bytes) * u64::from(copy.rows) * u64::from(copy.images);
        let layout = (copy.row_bytes, copy.rows, copy.images);
        let pitches = (src_row, src_image);
        let Some((buffer, offset)) = self.stage_rows(bytes, pitches, layout, copy.row_bytes) else {
            return false;
        };
        self.open_blit_pass();
        let (slice, origin, size, bytes_per_row) =
            (copy.slice, copy.origin, copy.size, copy.row_bytes);
        let bytes_per_image = copy.row_bytes * copy.rows;
        self.emit(move |enc| {
            let dst =
                enc.access_texture::<{ stage::COMPUTE }>(&texture, level, slice, Access::WRITE);
            enc.encode_blit_command(WmtcmdBlitCopyFromBufferToTexture {
                r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
                reserved: [0; 3],
                next: NO_NEXT,
                src: buffer,
                src_offset: offset,
                bytes_per_row,
                bytes_per_image,
                size,
                dst,
                slice,
                level,
                origin,
            });
        });
        self.staged_bytes += total;
        if self.staged_bytes > self.staged_budget {
            self.commit();
        }
        true
    }

    /// Copies rows of `bytes` into a new staging-ring block, `stride` apart: its buffer and offset.
    ///
    /// `images` images of `rows` rows of `row_bytes` bytes: row `r` of image `i` is read at
    /// `i * src_image + r * src_row` of `bytes`, which the caller checked holds every row, and
    /// lands `stride` bytes after the row before it: `row_bytes` packs them, a wider stride
    /// leaves the ring's bytes between them as they are (a stencil plane's rows, rounded up to 4
    /// bytes, DXMTRS C4c-3b). None, with an error, when the block has no mapping this process
    /// reaches; None for a stride under the row.
    fn stage_rows(
        &mut self,
        bytes: &[u8],
        (src_row, src_image): (usize, usize),
        (row_bytes, rows, images): (u32, u32, u32),
        stride: u32,
    ) -> Option<(u64, u64)> {
        if stride < row_bytes {
            return None;
        }
        let total = u64::from(stride) * u64::from(rows) * u64::from(images);
        let (block, offset) = self.device.queue_mut().allocate_staging_buffer(total, 16);
        let start = usize::try_from(offset).ok()?;
        if block.mapped.is_null() {
            log::error!(target: LOG_TARGET, "d3d9 core: a staging block this process cannot reach");
            return None;
        }
        let tight = row_bytes as usize;
        for i in 0..images as usize {
            for r in 0..rows as usize {
                let from = i * src_image + r * src_row;
                let to = (i * rows as usize + r) * stride as usize;
                let src = &bytes[from..from + tight];
                // SAFETY: the block's mapping holds `total` bytes from `start`; `to + tight` is
                // within them, `tight` being at most `stride`.
                let dst = unsafe { block.mapped.add(start + to) };
                // SAFETY: `dst` holds `tight` writable bytes (above), which `src` (the caller's
                // slice) does not overlap.
                unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst, tight) };
            }
        }
        Some((block.buffer, offset))
    }

    /// Regenerates `id`'s levels below 0 from level 0 (`D3DUSAGE_AUTOGENMIPMAP`), in API order.
    pub fn texture_generate_mips(&mut self, id: TextureId) {
        let Some(stored) = self.textures.get(&id) else {
            return;
        };
        if stored.desc.levels < 2 {
            return;
        }
        let texture = Arc::clone(&stored.texture);
        let key = texture.full_view();
        self.open_blit_pass();
        self.emit(move |enc| {
            let Some(view) = enc.access_texture_view::<{ stage::COMPUTE }>(
                &texture,
                key,
                Access::READ | Access::WRITE,
            ) else {
                return;
            };
            enc.encode_blit_command(WmtcmdBlitGenerateMipmaps {
                r#type: WMT_BLIT_COMMAND_GENERATE_MIPMAPS,
                reserved: [0; 3],
                next: NO_NEXT,
                texture: view.texture,
            });
        });
    }
}

/// One upload's blit: its origin, size and tight layout.
struct Blit {
    slice: u32,
    origin: WMTOrigin,
    size: WMTSize,
    /// Bytes of one row of blocks, rows of blocks per image, images (a volume's slices).
    row_bytes: u32,
    rows: u32,
    images: u32,
}

impl Blit {
    /// The copy of `region` into a texture of `desc`; `None` when it is empty or past the texture.
    fn of(desc: &CoreTextureDesc, region: UploadRegion) -> Option<Self> {
        let (bw, bh, block_bytes) = desc.block;
        if region.level >= desc.levels || bw == 0 || bh == 0 {
            return None;
        }
        let mip = |extent: u32| (extent >> region.level).max(1);
        let (mw, mh) = (mip(desc.width), mip(desc.height));
        let md = if desc.kind == TextureKind::Volume {
            mip(desc.depth)
        } else {
            1
        };
        let (x, y) = (region.x / bw * bw, region.y / bh * bh);
        let z = if desc.kind == TextureKind::Volume {
            region.z
        } else {
            0
        };
        if x >= mw || y >= mh || z >= md {
            return None;
        }
        let width = region.width.min(mw - x);
        let height = region.height.min(mh - y);
        let depth = if desc.kind == TextureKind::Volume {
            region.depth.min(md - z)
        } else {
            1
        };
        if width == 0 || height == 0 || depth == 0 {
            return None;
        }
        let slice = if desc.kind == TextureKind::Cube {
            region.slice
        } else {
            0
        };
        Some(Self {
            slice,
            origin: WMTOrigin {
                x: u64::from(x),
                y: u64::from(y),
                z: u64::from(z),
            },
            size: WMTSize {
                width: u64::from(width),
                height: u64::from(height),
                depth: u64::from(depth),
            },
            row_bytes: width.div_ceil(bw) * block_bytes,
            rows: height.div_ceil(bh),
            images: depth,
        })
    }
}

/// The copies of a `Depth32FloatStencil8` level's two planes, depth first, as dxmt-core's.
///
/// A copy of level `level`, `width` x `height`, at slice 0 and origin 0 a plane: rows of
/// `width * 4` bytes of depth, of `width` rounded up to 4 of stencil. The source and the
/// destination are 0, for the caller to fill; none when a row overflows `u32`.
fn plane_copies(
    level: u16,
    (width, height): (u32, u32),
) -> Option<[WmtcmdBlitCopyFromBufferToTextureWithblitoption; 2]> {
    let copy = |bytes_per_row, options| WmtcmdBlitCopyFromBufferToTextureWithblitoption {
        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
        reserved: [0; 3],
        next: NO_NEXT,
        src: 0,
        src_offset: 0,
        bytes_per_row,
        bytes_per_image: 0,
        size: WMTSize {
            width: u64::from(width),
            height: u64::from(height),
            depth: 1,
        },
        dst: 0,
        slice: 0,
        level,
        options,
        origin: WMTOrigin { x: 0, y: 0, z: 0 },
    };
    Some([
        copy(
            width.checked_mul(4)?,
            WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL,
        ),
        copy(
            width.checked_next_multiple_of(4)?,
            WMT_BLIT_OPTION_STENCIL_FROM_DEPTH_STENCIL,
        ),
    ])
}

/// The Metal type of a 2D texture, or of a view of one subresource of it, of `samples` a texel.
pub(super) const fn two_d(samples: u8) -> u32 {
    if samples > 1 {
        WMT_TEXTURE_TYPE2D_MULTISAMPLE
    } else {
        WMT_TEXTURE_TYPE2D
    }
}

#[cfg(test)]
mod tests;
