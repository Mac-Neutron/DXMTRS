//! D3D9 textures on the core (DXMTRS C4b-3b): a core texture per D3D9 texture, and its uploads.
//!
//! The store keys each core `Texture` by the D3D9 texture's process-unique, never-reused
//! [`TextureId`]. An upload copies a region's bytes into the queue's staging ring at record time
//! and records a blit into the texture in API order (the outline's upload model); consecutive
//! uploads share one blit pass, which the next render pass, Clear, read-back or commit closes.
//! The bytes staged in the chunk being recorded are counted: past the profile's budget the chunk
//! is committed, so a frame that uploads more than a 32-bit process can hold in ring blocks
//! retires them as it goes.

use std::sync::Arc;

use dxmt_core::{
    deptrack::Access,
    encoder::{NO_NEXT, stage},
    resource::{
        Texture, TextureAllocationFlags, TextureViewDescriptor, TextureViewKey, texture_info,
    },
};
use mtld3d_core::ids::TextureId;
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE, WMT_BLIT_COMMAND_GENERATE_MIPMAPS,
    WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE2D,
    WMT_TEXTURE_TYPE3D, WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW, WMT_TEXTURE_USAGE_SHADER_READ,
    WMTOrigin, WMTSize, WMTTextureInfo, WmtcmdBlitCopyFromBufferToTexture,
    WmtcmdBlitGenerateMipmaps,
};

use super::{CoreDevice, LOG_TARGET};

/// A D3D9 texture's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureKind {
    Tex2d,
    Cube,
    Volume,
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
    /// Views in another format (an sRGB twin, a swizzle) will be taken: `PIXEL_FORMAT_VIEW` usage.
    pub view_twins: bool,
    /// The format's block: width and height in texels, size in bytes.
    ///
    /// (1, 1, texel size) for an uncompressed format.
    pub block: (u32, u32, u32),
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

/// A texture and sampler a draw binds at one pixel slot.
pub struct TextureBind {
    pub(super) slot: u8,
    pub(super) texture: Arc<Texture>,
    pub(super) key: TextureViewKey,
    pub(super) sampler: Option<Arc<dxmt_core::sampler::Sampler>>,
}

/// A stored texture: the core texture and the description it was made from.
pub(super) struct Stored {
    texture: Arc<Texture>,
    desc: CoreTextureDesc,
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
    /// Its storage is uninitialised, as D3D9's contents are; a second call for one id replaces the
    /// first.
    pub fn texture_create(&mut self, id: TextureId, desc: &CoreTextureDesc) {
        let (texture_type, array_length) = match desc.kind {
            TextureKind::Tex2d => (WMT_TEXTURE_TYPE2D, 1),
            TextureKind::Cube => (WMT_TEXTURE_TYPE_CUBE, 1),
            TextureKind::Volume => (WMT_TEXTURE_TYPE3D, 1),
        };
        let mut usage = WMT_TEXTURE_USAGE_SHADER_READ;
        if desc.view_twins {
            usage |= WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW;
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
            bits: texture_info::info_bits(texture_type, desc.levels, 1, usage),
            options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
            flags: 0,
            mach_port: 0,
            gpu_resource_id: 0,
        };
        let texture = Arc::new(Texture::new(info, self.device.device()));
        texture.rename(texture.allocate(
            TextureAllocationFlags::GPU_PRIVATE | TextureAllocationFlags::SHADER_READONLY,
        ));
        self.textures.insert(
            id,
            Stored {
                texture,
                desc: *desc,
            },
        );
    }

    /// The `WMTPixelFormat` `id`'s texture is stored in, if the store holds it.
    #[must_use]
    pub fn texture_format(&self, id: TextureId) -> Option<u32> {
        self.textures.get(&id).map(|s| s.desc.pixel_format)
    }

    /// A draw's bind of `id`'s texture in `view_format` with `sampler` at pixel `slot`.
    #[must_use]
    pub fn texture_bind(
        &self,
        slot: u8,
        id: TextureId,
        view_format: u32,
        sampler: Option<Arc<dxmt_core::sampler::Sampler>>,
    ) -> Option<TextureBind> {
        let (texture, key) = self.texture_view(id, view_format)?;
        Some(TextureBind {
            slot,
            texture,
            key,
            sampler,
        })
    }

    /// A draw's bind of the opaque black at pixel `slot`, with `sampler`.
    pub fn black_bind(
        &mut self,
        slot: u8,
        sampler: Option<Arc<dxmt_core::sampler::Sampler>>,
    ) -> TextureBind {
        let (texture, key) = self.black();
        TextureBind {
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
    pub fn forget_texture(&mut self, id: TextureId) {
        self.textures.remove(&id);
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
        let desc = stored.desc;
        let texture = Arc::clone(&stored.texture);
        let Some(copy) = Blit::of(&desc, region) else {
            return false;
        };
        let (src_row, src_image) = (bytes_per_row as usize, bytes_per_image as usize);
        let needed = src_image * (copy.images as usize - 1)
            + src_row * (copy.rows as usize - 1)
            + copy.row_bytes as usize;
        if src_row < copy.row_bytes as usize || bytes.len() < needed {
            return false;
        }
        let total = u64::from(copy.row_bytes) * u64::from(copy.rows) * u64::from(copy.images);
        let (block, offset) = self.device.queue_mut().allocate_staging_buffer(total, 16);
        let Ok(start) = usize::try_from(offset) else {
            return false;
        };
        if block.mapped.is_null() {
            log::error!(target: LOG_TARGET, "d3d9 core: a staging block this process cannot reach");
            return false;
        }
        let tight = copy.row_bytes as usize;
        for image in 0..copy.images as usize {
            for row in 0..copy.rows as usize {
                let from = image * src_image + row * src_row;
                let to = (image * copy.rows as usize + row) * tight;
                let src = &bytes[from..from + tight];
                // SAFETY: the block's mapping holds `total` bytes from `start`; `to + tight` is
                // within them.
                let dst = unsafe { block.mapped.add(start + to) };
                // SAFETY: `dst` holds `tight` writable bytes (above), which `src` (the caller's
                // slice) does not overlap.
                unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst, tight) };
            }
        }
        self.open_blit_pass();
        let (buffer, level, slice) = (block.buffer, region.level, copy.slice);
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
                bytes_per_row: copy.row_bytes,
                bytes_per_image,
                size: copy.size,
                dst,
                slice,
                level,
                origin: copy.origin,
            });
        });
        self.staged_bytes += total;
        if self.staged_bytes > self.staged_budget {
            self.commit();
        }
        true
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

#[cfg(test)]
mod tests;
