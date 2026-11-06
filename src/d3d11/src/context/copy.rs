//! Resource copy and tiled-resource operations (`CopyResource`, `CopySubresourceRegion*`).

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::{
    deptrack::Access,
    encoder::{NO_NEXT, stage},
    format::{
        MTL_DXGI_FORMAT_BC, MTL_DXGI_FORMAT_EMULATED_D24,
        MTL_DXGI_FORMAT_EMULATED_LINEAR_DEPTH_STENCIL, MtlDxgiFormatDesc, forget_srgb, query_dxgi,
    },
    resource::{Buffer, Texture, dims::mip_extent},
    staging::Staging,
};
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER, WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE,
    WMT_PIXEL_FORMAT_INVALID, WMTOrigin, WMTSize, WmtcmdBlitCopyFromBufferToBuffer,
    WmtcmdBlitCopyFromBufferToTexture, WmtcmdBlitCopyFromTextureToBuffer,
    WmtcmdBlitCopyFromTextureToTexture,
};

use super::{Context, Mode};
use crate::{
    abi::{
        records::{
            D3d11Box, D3d11BufferDesc, D3d11Texture1dDesc, D3d11Texture2dDesc, D3d11Texture2dDesc1,
            D3d11Texture3dDesc, D3d11Texture3dDesc1,
        },
        vtbl::{
            ID3D11BufferVtbl, ID3D11ResourceVtbl, ID3D11Texture1DVtbl, ID3D11Texture2DVtbl,
            ID3D11Texture3DVtbl,
        },
    },
    com::{E_NOTIMPL, Hresult},
};

/// `D3D11_RESOURCE_DIMENSION_UNKNOWN`.
pub(super) const D3D11_RESOURCE_DIMENSION_UNKNOWN: u32 = 0;
/// `D3D11_RESOURCE_DIMENSION_BUFFER`.
pub(super) const D3D11_RESOURCE_DIMENSION_BUFFER: u32 = 1;
/// `D3D11_RESOURCE_DIMENSION_TEXTURE1D`.
pub(super) const D3D11_RESOURCE_DIMENSION_TEXTURE1D: u32 = 2;
/// `D3D11_RESOURCE_DIMENSION_TEXTURE2D`.
pub(super) const D3D11_RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;
/// `D3D11_RESOURCE_DIMENSION_TEXTURE3D`.
pub(super) const D3D11_RESOURCE_DIMENSION_TEXTURE3D: u32 = 4;

/// Aligns `v` up to the next multiple of 4 (`align(v, 4u)`).
pub(super) const fn align4(v: u32) -> u32 {
    (v + 3) & !3
}

/// Converts a 3D `u32` origin `[x, y, z]` into a [`WMTOrigin`].
pub(super) const fn wmt_origin([x, y, z]: [u32; 3]) -> WMTOrigin {
    WMTOrigin {
        x: x as u64,
        y: y as u64,
        z: z as u64,
    }
}

/// Converts a 3D `u32` extent `[width, height, depth]` into a [`WMTSize`].
pub(super) const fn wmt_size([width, height, depth]: [u32; 3]) -> WMTSize {
    WMTSize {
        width: width as u64,
        height: height as u64,
        depth: depth as u64,
    }
}

/// Logs `<msg>: not implemented` and triggers the platform `UNIMPLEMENTED` trap.
pub(super) fn unimplemented_abort(msg: &str) {
    #[cfg(not(test))]
    crate::LOG.err(&format!("{msg}: not implemented"));
    #[cfg(test)]
    record_not_implemented(msg);
    #[cfg(windows)]
    std::process::abort();
    #[cfg(all(not(windows), not(test)))]
    {
        // SAFETY: `wmt_native_not_implemented` is a C diagnostic abort stub taking no arguments.
        unsafe { winemetal_sys::wmt_native_not_implemented() };
    }
}

/// Resource descriptor variant inside [`BlitObject`].
pub(super) enum BlitObjectDesc {
    Unknown,
    Buffer(D3d11BufferDesc),
    Texture1d(D3d11Texture1dDesc),
    Texture2d(D3d11Texture2dDesc1),
    Texture3d(D3d11Texture3dDesc1),
}

/// Inspected D3D11 resource dimension, descriptor, and DXGI format (`BlitObject`).
pub(super) struct BlitObject {
    pub p_resource: *mut c_void,
    pub dimension: u32,
    pub desc: BlitObjectDesc,
    pub format_desc: MtlDxgiFormatDesc,
}

impl BlitObject {
    /// Queries `p_resource`'s COM vtable for its resource dimension, descriptor, and format.
    pub(super) fn new(_device: *mut c_void, p_resource: *mut c_void) -> Option<Self> {
        if p_resource.is_null() {
            return None;
        }
        // SAFETY: `p_resource` is a live COM interface pointer whose first word is its vtable.
        let vtbl = unsafe { *p_resource.cast::<*const ID3D11ResourceVtbl>() };
        if vtbl.is_null() {
            return None;
        }
        let mut dimension = D3D11_RESOURCE_DIMENSION_UNKNOWN;
        // SAFETY: `vtbl` points to a valid `ID3D11ResourceVtbl`.
        let get_type = unsafe { (*vtbl).get_type };
        // SAFETY: `p_resource` is a valid `ID3D11Resource` and `dimension` is a stack local.
        unsafe { (get_type)(p_resource, &raw mut dimension) };

        let empty_fmt = query_dxgi(0).1;
        match dimension {
            D3D11_RESOURCE_DIMENSION_BUFFER => {
                let mut desc = D3d11BufferDesc::default();
                let buf_vtbl = vtbl.cast::<ID3D11BufferVtbl>();
                // SAFETY: `dimension == BUFFER` guarantees `buf_vtbl` is an `ID3D11BufferVtbl`.
                let get_desc = unsafe { (*buf_vtbl).get_desc };
                // SAFETY: `p_resource` implements `ID3D11Buffer` and `desc` is a stack local.
                unsafe { (get_desc)(p_resource, (&raw mut desc).cast::<c_void>()) };
                Some(Self {
                    p_resource,
                    dimension,
                    desc: BlitObjectDesc::Buffer(desc),
                    format_desc: empty_fmt,
                })
            }
            D3D11_RESOURCE_DIMENSION_TEXTURE1D => {
                let mut desc = D3d11Texture1dDesc::default();
                let tex_vtbl = vtbl.cast::<ID3D11Texture1DVtbl>();
                // SAFETY: `dimension == TEXTURE1D` guarantees `tex_vtbl` is `ID3D11Texture1DVtbl`.
                let get_desc = unsafe { (*tex_vtbl).get_desc };
                // SAFETY: `p_resource` implements `ID3D11Texture1D` and `desc` is a stack local.
                unsafe { (get_desc)(p_resource, (&raw mut desc).cast::<c_void>()) };
                let (_, format_desc) = query_dxgi(desc.format);
                Some(Self {
                    p_resource,
                    dimension,
                    desc: BlitObjectDesc::Texture1d(desc),
                    format_desc,
                })
            }
            D3D11_RESOURCE_DIMENSION_TEXTURE2D => {
                let mut base = D3d11Texture2dDesc::default();
                let tex_vtbl = vtbl.cast::<ID3D11Texture2DVtbl>();
                // SAFETY: `dimension == TEXTURE2D` guarantees `tex_vtbl` is `ID3D11Texture2DVtbl`.
                let get_desc = unsafe { (*tex_vtbl).get_desc };
                // SAFETY: `p_resource` implements `ID3D11Texture2D` and `base` is a stack local.
                unsafe { (get_desc)(p_resource, (&raw mut base).cast::<c_void>()) };
                let desc = D3d11Texture2dDesc1 {
                    width: base.width,
                    height: base.height,
                    mip_levels: base.mip_levels,
                    array_size: base.array_size,
                    format: base.format,
                    sample_desc: base.sample_desc,
                    usage: base.usage,
                    bind_flags: base.bind_flags,
                    cpu_access_flags: base.cpu_access_flags,
                    misc_flags: base.misc_flags,
                    texture_layout: 0,
                };
                let (_, format_desc) = query_dxgi(desc.format);
                Some(Self {
                    p_resource,
                    dimension,
                    desc: BlitObjectDesc::Texture2d(desc),
                    format_desc,
                })
            }
            D3D11_RESOURCE_DIMENSION_TEXTURE3D => {
                let mut base = D3d11Texture3dDesc::default();
                let tex_vtbl = vtbl.cast::<ID3D11Texture3DVtbl>();
                // SAFETY: `dimension == TEXTURE3D` guarantees `tex_vtbl` is `ID3D11Texture3DVtbl`.
                let get_desc = unsafe { (*tex_vtbl).get_desc };
                // SAFETY: `p_resource` implements `ID3D11Texture3D` and `base` is a stack local.
                unsafe { (get_desc)(p_resource, (&raw mut base).cast::<c_void>()) };
                let desc = D3d11Texture3dDesc1 {
                    width: base.width,
                    height: base.height,
                    depth: base.depth,
                    mip_levels: base.mip_levels,
                    format: base.format,
                    usage: base.usage,
                    bind_flags: base.bind_flags,
                    cpu_access_flags: base.cpu_access_flags,
                    misc_flags: base.misc_flags,
                    texture_layout: 0,
                };
                let (_, format_desc) = query_dxgi(desc.format);
                Some(Self {
                    p_resource,
                    dimension,
                    desc: BlitObjectDesc::Texture3d(desc),
                    format_desc,
                })
            }
            _ => Some(Self {
                p_resource,
                dimension: D3D11_RESOURCE_DIMENSION_UNKNOWN,
                desc: BlitObjectDesc::Unknown,
                format_desc: empty_fmt,
            }),
        }
    }

    /// Total subresource count (`MipLevels * ArraySize`, or `MipLevels` for 3D).
    pub(super) const fn subresource_count(&self) -> u32 {
        match &self.desc {
            BlitObjectDesc::Texture1d(d) => d.mip_levels.saturating_mul(d.array_size),
            BlitObjectDesc::Texture2d(d) => d.mip_levels.saturating_mul(d.array_size),
            BlitObjectDesc::Texture3d(d) => d.mip_levels,
            BlitObjectDesc::Buffer(_) | BlitObjectDesc::Unknown => 1,
        }
    }
}

/// Decomposed subresource coordinates and mip dimensions (`Subresource`).
#[derive(Default)]
pub(super) struct Subresource {
    pub mip_level: u32,
    pub array_slice: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

impl Subresource {
    pub(super) const fn from_blit(obj: &BlitObject, subresource: u32) -> Option<Self> {
        match &obj.desc {
            BlitObjectDesc::Texture1d(d) => {
                let total = d.mip_levels.saturating_mul(d.array_size);
                if d.mip_levels == 0 || subresource >= total {
                    return None;
                }
                let mip_level = subresource % d.mip_levels;
                let array_slice = subresource / d.mip_levels;
                let width = mip_extent(d.width, mip_level);
                Some(Self {
                    mip_level,
                    array_slice,
                    width,
                    height: 1,
                    depth: 1,
                })
            }
            BlitObjectDesc::Texture2d(d) => {
                let total = d.mip_levels.saturating_mul(d.array_size);
                if d.mip_levels == 0 || subresource >= total {
                    return None;
                }
                let mip_level = subresource % d.mip_levels;
                let array_slice = subresource / d.mip_levels;
                let width = mip_extent(d.width, mip_level);
                let height = mip_extent(d.height, mip_level);
                Some(Self {
                    mip_level,
                    array_slice,
                    width,
                    height,
                    depth: 1,
                })
            }
            BlitObjectDesc::Texture3d(d) => {
                if subresource >= d.mip_levels {
                    return None;
                }
                let mip_level = subresource;
                let width = mip_extent(d.width, mip_level);
                let height = mip_extent(d.height, mip_level);
                let depth = mip_extent(d.depth, mip_level);
                Some(Self {
                    mip_level,
                    array_slice: 0,
                    width,
                    height,
                    depth,
                })
            }
            BlitObjectDesc::Buffer(_) | BlitObjectDesc::Unknown => None,
        }
    }
}

/// Validated texture-to-texture / staging copy parameters (`TextureCopyCommand`).
pub(super) struct TextureCopyCommand {
    pub p_src: *mut c_void,
    pub p_dst: *mut c_void,
    pub src_subresource: u32,
    pub dst_subresource: u32,
    pub src: Subresource,
    pub dst: Subresource,
    pub src_origin: [u32; 3],
    pub src_size: [u32; 3],
    pub dst_origin: [u32; 3],
    pub src_format_flag: u16,
    pub src_bytes_per_texel: u32,
    pub dst_format_flag: u16,
    pub dst_bytes_per_texel: u32,
    pub dimension_incompatible: bool,
}

impl TextureCopyCommand {
    pub(super) fn new(
        dst_obj: &BlitObject,
        dst_subresource: u32,
        dst_offset: [u32; 3],
        src_obj: &BlitObject,
        src_subresource: u32,
        p_src_box: *const D3d11Box,
    ) -> Option<Self> {
        let [dst_x, dst_y, dst_z] = dst_offset;
        if src_obj.format_desc.pixel_format == WMT_PIXEL_FORMAT_INVALID
            || dst_obj.format_desc.pixel_format == WMT_PIXEL_FORMAT_INVALID
        {
            return None;
        }
        let dst = Subresource::from_blit(dst_obj, dst_subresource)?;
        let src = Subresource::from_blit(src_obj, src_subresource)?;

        let mut src_box = if p_src_box.is_null() {
            D3d11Box {
                left: 0,
                top: 0,
                front: 0,
                right: src.width,
                bottom: src.height,
                back: src.depth,
            }
        } else {
            // SAFETY: caller supplied a non-null `D3D11_BOX` pointer.
            let b = unsafe { *p_src_box };
            if b.left >= b.right || b.top >= b.bottom || b.front >= b.back {
                return None;
            }
            b
        };

        if src_box.left >= src.width || src_box.top >= src.height || src_box.front >= src.depth {
            return None;
        }
        if dst_x >= dst.width || dst_y >= dst.height || dst_z >= dst.depth {
            return None;
        }

        src_box.right = src_box.right.min(src.width);
        src_box.bottom = src_box.bottom.min(src.height);
        src_box.back = src_box.back.min(src.depth);

        let src_bc = (src_obj.format_desc.flag & MTL_DXGI_FORMAT_BC) != 0;
        let dst_bc = (dst_obj.format_desc.flag & MTL_DXGI_FORMAT_BC) != 0;
        if src_bc == dst_bc {
            if dst_x.saturating_add(src_box.right - src_box.left) > dst.width {
                return None;
            }
            if dst_y.saturating_add(src_box.bottom - src_box.top) > dst.height {
                return None;
            }
            if dst_z.saturating_add(src_box.back - src_box.front) > dst.depth {
                return None;
            }
        }

        Some(Self {
            p_src: src_obj.p_resource,
            p_dst: dst_obj.p_resource,
            src_subresource,
            dst_subresource,
            src,
            dst,
            src_origin: [src_box.left, src_box.top, src_box.front],
            src_size: [
                src_box.right - src_box.left,
                src_box.bottom - src_box.top,
                src_box.back - src_box.front,
            ],
            dst_origin: [dst_x, dst_y, dst_z],
            src_format_flag: src_obj.format_desc.flag,
            src_bytes_per_texel: src_obj.format_desc.bytes_per_texel,
            dst_format_flag: dst_obj.format_desc.flag,
            dst_bytes_per_texel: dst_obj.format_desc.bytes_per_texel,
            dimension_incompatible: dst_obj.dimension != src_obj.dimension,
        })
    }
}

/// Validated texture upload parameters (`TextureUpdateCommand`).
pub(super) struct TextureUpdateCommand {
    pub p_dst: *mut c_void,
    pub dst_subresource: u32,
    pub dst: Subresource,
    pub dst_origin: [u32; 3],
    pub dst_size: [u32; 3],
    pub effective_bytes_per_row: u32,
    pub effective_rows: u32,
}

impl TextureUpdateCommand {
    pub(super) fn new(
        dst_obj: &BlitObject,
        dst_subresource: u32,
        p_dst_box: *const D3d11Box,
    ) -> Option<Self> {
        if dst_obj.format_desc.pixel_format == WMT_PIXEL_FORMAT_INVALID
            || dst_obj.format_desc.bytes_per_texel == 0
        {
            return None;
        }
        let dst = Subresource::from_blit(dst_obj, dst_subresource)?;

        let mut dst_box = if p_dst_box.is_null() {
            D3d11Box {
                left: 0,
                top: 0,
                front: 0,
                right: dst.width,
                bottom: dst.height,
                back: dst.depth,
            }
        } else {
            // SAFETY: caller supplied a non-null `D3D11_BOX` pointer.
            let b = unsafe { *p_dst_box };
            if b.left >= b.right || b.top >= b.bottom || b.front >= b.back {
                return None;
            }
            b
        };

        if dst_box.left >= dst.width || dst_box.top >= dst.height || dst_box.front >= dst.depth {
            return None;
        }

        dst_box.right = dst_box.right.min(dst.width);
        dst_box.bottom = dst_box.bottom.min(dst.height);
        dst_box.back = dst_box.back.min(dst.depth);

        let dst_origin = [dst_box.left, dst_box.top, dst_box.front];
        let dst_size = [
            dst_box.right - dst_box.left,
            dst_box.bottom - dst_box.top,
            dst_box.back - dst_box.front,
        ];

        let (effective_bytes_per_row, effective_rows) =
            if (dst_obj.format_desc.flag & MTL_DXGI_FORMAT_BC) != 0 {
                (
                    (align4(dst_size[0]) >> 2) * dst_obj.format_desc.bytes_per_texel,
                    align4(dst_size[1]) >> 2,
                )
            } else {
                (
                    dst_size[0] * dst_obj.format_desc.bytes_per_texel,
                    dst_size[1],
                )
            };

        Some(Self {
            p_dst: dst_obj.p_resource,
            dst_subresource,
            dst,
            dst_origin,
            dst_size,
            effective_bytes_per_row,
            effective_rows,
        })
    }
}

/// Backing default buffer reference (live `Arc<Buffer>` or unit-test fixture).
pub(super) enum BufferRef {
    Live(Arc<Buffer>),
    #[cfg(test)]
    Test {
        handle: u64,
        offset: u64,
    },
}

impl BufferRef {
    #[cfg(test)]
    pub(super) const fn test_handle(&self) -> Option<(u64, u64)> {
        match self {
            Self::Live(_) => None,
            Self::Test { handle, offset } => Some((*handle, *offset)),
        }
    }

    pub(super) fn access(
        &self,
        enc: &mut dxmt_core::encoder::Encoder,
        _offset: u32,
        _length: u32,
        access: Access,
    ) -> (u64, u64) {
        match self {
            Self::Live(buf) => enc
                .access_buffer::<{ stage::COMPUTE }>(buf, access)
                .map_or((0, 0), |(alloc, sub_off)| (alloc.buffer(), sub_off)),
            #[cfg(test)]
            Self::Test { handle, offset } => (*handle, *offset),
        }
    }
}

/// Backing default texture reference (live `Arc<Texture>` or unit-test fixture).
pub(super) enum TextureRef {
    Live(Arc<Texture>),
    #[cfg(test)]
    Test {
        handle: u64,
        pixel_format: u32,
        width: u32,
        height: u32,
    },
}

impl TextureRef {
    pub(super) fn access_subresource(
        &self,
        enc: &mut dxmt_core::encoder::Encoder,
        level: u32,
        slice: u32,
        access: Access,
    ) {
        match self {
            Self::Live(tex) => {
                let _ =
                    enc.access_texture_subresource::<{ stage::COMPUTE }>(tex, level, slice, access);
            }
            #[cfg(test)]
            Self::Test { .. } => {}
        }
    }

    #[cfg(test)]
    pub(super) const fn test_handle(&self) -> Option<u64> {
        match self {
            Self::Live(_) => None,
            Self::Test { handle, .. } => Some(*handle),
        }
    }

    pub(super) fn pixel_format(&self) -> u32 {
        match self {
            Self::Live(tex) => tex.info().pixel_format,
            #[cfg(test)]
            Self::Test { pixel_format, .. } => *pixel_format,
        }
    }

    pub(super) fn width(&self) -> u32 {
        match self {
            Self::Live(tex) => tex.info().width,
            #[cfg(test)]
            Self::Test { width, .. } => *width,
        }
    }

    pub(super) fn height(&self) -> u32 {
        match self {
            Self::Live(tex) => tex.info().height,
            #[cfg(test)]
            Self::Test { height, .. } => *height,
        }
    }

    pub(super) fn access(
        &self,
        enc: &mut dxmt_core::encoder::Encoder,
        level: u32,
        slice: u32,
        access: Access,
    ) -> u64 {
        match self {
            Self::Live(tex) => enc.access_texture::<{ stage::COMPUTE }>(tex, level, slice, access),
            #[cfg(test)]
            Self::Test { handle, .. } => *handle,
        }
    }
}

/// Backing staging subresource reference (live `Arc<Staging>` or unit-test fixture).
pub(super) enum StagingRef {
    Live(Arc<Staging>),
    #[cfg(test)]
    Test {
        handle: u64,
        bytes_per_row: u32,
        bytes_per_image: u32,
        length: u64,
    },
}

impl StagingRef {
    pub(super) fn access_buffer(&self, enc: &mut dxmt_core::encoder::Encoder, access: Access) {
        match self {
            Self::Live(s) => {
                let _ = enc.access_buffer::<{ stage::COMPUTE }>(s.buffer(), access);
            }
            #[cfg(test)]
            Self::Test { .. } => {}
        }
    }

    pub(super) fn depth_stencil_copy_to(
        &self,
        enc: &mut dxmt_core::encoder::Encoder,
        dst: &TextureRef,
        dst_mip: u32,
        dst_slice: u32,
        from_d24s8: bool,
    ) {
        match (self, dst) {
            (Self::Live(src), TextureRef::Live(dst_tex)) => {
                enc.ds_copy_from_buffer(
                    (
                        src.buffer(),
                        0,
                        src.length(),
                        src.bytes_per_row(),
                        src.bytes_per_image(),
                    ),
                    (dst_tex, dst_mip, dst_slice),
                    from_d24s8,
                );
            }
            #[cfg(test)]
            _ => {}
        }
    }

    pub(super) fn depth_stencil_copy_from(
        &self,
        enc: &mut dxmt_core::encoder::Encoder,
        src: &TextureRef,
        src_mip: u32,
        src_slice: u32,
        to_d24s8: bool,
    ) {
        match (self, src) {
            (Self::Live(dst), TextureRef::Live(src_tex)) => {
                enc.ds_copy_from_texture(
                    (src_tex, src_mip, src_slice),
                    (
                        dst.buffer(),
                        0,
                        dst.length(),
                        dst.bytes_per_row(),
                        dst.bytes_per_image(),
                    ),
                    to_d24s8,
                );
            }
            #[cfg(test)]
            _ => {}
        }
    }

    #[cfg(test)]
    pub(super) const fn test_handle(&self) -> Option<u64> {
        match self {
            Self::Live(_) => None,
            Self::Test { handle, .. } => Some(*handle),
        }
    }

    pub(super) fn bytes_per_row(&self) -> u32 {
        match self {
            Self::Live(s) => s.bytes_per_row(),
            #[cfg(test)]
            Self::Test { bytes_per_row, .. } => *bytes_per_row,
        }
    }

    pub(super) fn bytes_per_image(&self) -> u32 {
        match self {
            Self::Live(s) => s.bytes_per_image(),
            #[cfg(test)]
            Self::Test {
                bytes_per_image, ..
            } => *bytes_per_image,
        }
    }

    pub(super) fn length(&self) -> u64 {
        match self {
            Self::Live(s) => s.length(),
            #[cfg(test)]
            Self::Test { length, .. } => *length,
        }
    }

    pub(super) fn use_copy_destination<M: Mode>(&self, ctx: &mut Context<M>) {
        match self {
            Self::Live(s) => {
                if let Some(def) = ctx.as_deferred_mut() {
                    def.mode.record_staging_write(s);
                    def.mode.has_promoted_flush = true;
                } else {
                    let seq = ctx.active_seq_any();
                    s.use_copy_destination(seq);
                }
            }
            #[cfg(test)]
            Self::Test { .. } => {}
        }
    }

    pub(super) fn use_copy_source<M: Mode>(&self, ctx: &mut Context<M>) {
        match self {
            Self::Live(s) => {
                if let Some(def) = ctx.as_deferred_mut() {
                    def.mode.record_staging_read(s);
                } else {
                    let seq = ctx.active_seq_any();
                    s.use_copy_source(seq);
                }
            }
            #[cfg(test)]
            Self::Test { .. } => {}
        }
    }

    pub(super) fn access(
        &self,
        enc: &mut dxmt_core::encoder::Encoder,
        _offset: u32,
        _length: u32,
        access: Access,
    ) -> (u64, u64) {
        match self {
            Self::Live(s) => enc
                .access_buffer::<{ stage::COMPUTE }>(s.buffer(), access)
                .map_or((0, 0), |(alloc, sub_off)| (alloc.buffer(), sub_off)),
            #[cfg(test)]
            Self::Test { handle, .. } => (*handle, 0),
        }
    }
}

/// Resolves `res` to a [`BufferRef`] if it is a default/dynamic buffer.
pub(super) fn get_buffer_ref(res: *mut c_void) -> Option<BufferRef> {
    if let Some(common) = crate::resource::common(res)
        && let Some(buf) = common.buffer.as_ref()
    {
        return Some(BufferRef::Live(Arc::clone(buf)));
    }
    #[cfg(test)]
    if let Some((handle, offset, _)) = lookup_test_buffer(res) {
        return Some(BufferRef::Test { handle, offset });
    }
    None
}

/// Resolves `res` to a [`TextureRef`] if it is a default texture.
pub(super) fn get_texture_ref(res: *mut c_void) -> Option<TextureRef> {
    if let Some(common) = crate::resource::common(res)
        && let Some(tex) = common.texture.as_ref()
    {
        return Some(TextureRef::Live(Arc::clone(tex)));
    }
    #[cfg(test)]
    if let Some(tex) = lookup_test_texture(res) {
        return Some(tex);
    }
    None
}

/// Resolves `(res, subresource)` to a [`StagingRef`] if it is a staging resource.
pub(super) fn get_staging_ref(res: *mut c_void, subresource: u32) -> Option<StagingRef> {
    if let Some(stg) = crate::resource::staging(res, subresource) {
        return Some(StagingRef::Live(Arc::clone(stg)));
    }
    #[cfg(test)]
    if let Some(stg) = lookup_test_staging(res, subresource) {
        return Some(stg);
    }
    None
}

impl<M: Mode> Context<M> {
    #[expect(
        clippy::too_many_arguments,
        reason = "matches ID3D11DeviceContext::CopySubresourceRegion"
    )]
    pub fn copy_subresource_region(
        &mut self,
        dst_resource: *mut c_void,
        dst_subresource: u32,
        dst_x: u32,
        dst_y: u32,
        dst_z: u32,
        src_resource: *mut c_void,
        src_subresource: u32,
        src_box: *const c_void,
    ) {
        self.copy_subresource_region1(
            dst_resource,
            dst_subresource,
            dst_x,
            dst_y,
            dst_z,
            src_resource,
            src_subresource,
            src_box,
            0,
        );
    }

    pub fn copy_resource(&mut self, dst_resource: *mut c_void, src_resource: *mut c_void) {
        if dst_resource.is_null() || src_resource.is_null() || dst_resource == src_resource {
            return;
        }
        let (Some(dst_obj), Some(src_obj)) = (
            BlitObject::new(self.device, dst_resource),
            BlitObject::new(self.device, src_resource),
        ) else {
            return;
        };

        match dst_obj.dimension {
            D3D11_RESOURCE_DIMENSION_BUFFER => {
                if src_obj.dimension != D3D11_RESOURCE_DIMENSION_BUFFER {
                    return;
                }
                self.copy_buffer(&dst_obj, 0, 0, &src_obj, 0, ptr::null());
            }
            D3D11_RESOURCE_DIMENSION_TEXTURE1D
            | D3D11_RESOURCE_DIMENSION_TEXTURE2D
            | D3D11_RESOURCE_DIMENSION_TEXTURE3D => {
                let count = src_obj.subresource_count();
                for sub in 0..count {
                    if let Some(cmd) = TextureCopyCommand::new(
                        &dst_obj,
                        sub,
                        [0, 0, 0],
                        &src_obj,
                        sub,
                        ptr::null(),
                    ) {
                        self.copy_texture(&cmd);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn copy_structure_count(
        &mut self,
        dst_buffer: *mut c_void,
        dst_aligned_byte_offset: u32,
        src_view: *mut c_void,
    ) {
        let Some(uav) = crate::view::Uav::from_iface(src_view) else {
            return;
        };
        let Some(counter) = uav.counter.clone() else {
            return;
        };
        let uav_buf = uav.buffer.clone();

        if let Some(staging_dst) = get_staging_ref(dst_buffer, 0) {
            self.switch_to_blit_encoder();
            staging_dst.use_copy_destination(self);
            // C++ closure #1 (`CopyStructureCount` staging destination, d3d11_context_impl.cpp:1162).
            M::emit_op(self, move |enc| {
                let (dst_h, dst_off) =
                    staging_dst.access(enc, dst_aligned_byte_offset, 4, Access::WRITE);
                let (counter_h, counter_off) = enc
                    .access_counter::<{ stage::COMPUTE }>(&counter, uav_buf.as_ref())
                    .map_or((0, 0), |(alloc, off)| (alloc.buffer(), off));
                let cmd = WmtcmdBlitCopyFromBufferToBuffer {
                    r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    src: counter_h,
                    src_offset: counter_off,
                    dst: dst_h,
                    dst_offset: u64::from(dst_aligned_byte_offset) + dst_off,
                    copy_length: 4,
                };
                enc.encode_blit_command(cmd);
            });
        } else if let Some(dst_buf) = get_buffer_ref(dst_buffer) {
            self.switch_to_blit_encoder();
            // C++ closure #2 (`CopyStructureCount` default destination, d3d11_context_impl.cpp:1176).
            M::emit_op(self, move |enc| {
                let (dst_h, dst_off) =
                    dst_buf.access(enc, dst_aligned_byte_offset, 4, Access::WRITE);
                let (counter_h, counter_off) = enc
                    .access_counter::<{ stage::COMPUTE }>(&counter, uav_buf.as_ref())
                    .map_or((0, 0), |(alloc, off)| (alloc.buffer(), off));
                let cmd = WmtcmdBlitCopyFromBufferToBuffer {
                    r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    src: counter_h,
                    src_offset: counter_off,
                    dst: dst_h,
                    dst_offset: u64::from(dst_aligned_byte_offset) + dst_off,
                    copy_length: 4,
                };
                enc.encode_blit_command(cmd);
            });
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "matches ID3D11DeviceContext1::CopySubresourceRegion1"
    )]
    pub fn copy_subresource_region1(
        &mut self,
        dst_resource: *mut c_void,
        dst_subresource: u32,
        dst_x: u32,
        dst_y: u32,
        dst_z: u32,
        src_resource: *mut c_void,
        src_subresource: u32,
        src_box: *const c_void,
        _copy_flags: u32,
    ) {
        if dst_resource.is_null() || src_resource.is_null() {
            return;
        }
        let (Some(dst_obj), Some(src_obj)) = (
            BlitObject::new(self.device, dst_resource),
            BlitObject::new(self.device, src_resource),
        ) else {
            return;
        };

        match dst_obj.dimension {
            D3D11_RESOURCE_DIMENSION_UNKNOWN => {}
            D3D11_RESOURCE_DIMENSION_BUFFER => {
                if src_obj.dimension != D3D11_RESOURCE_DIMENSION_BUFFER {
                    return;
                }
                self.copy_buffer(
                    &dst_obj,
                    dst_subresource,
                    dst_x,
                    &src_obj,
                    src_subresource,
                    src_box.cast::<D3d11Box>(),
                );
            }
            _ => {
                if src_obj.dimension == D3D11_RESOURCE_DIMENSION_BUFFER {
                    return;
                }
                if let Some(cmd) = TextureCopyCommand::new(
                    &dst_obj,
                    dst_subresource,
                    [dst_x, dst_y, dst_z],
                    &src_obj,
                    src_subresource,
                    src_box.cast::<D3d11Box>(),
                ) {
                    self.copy_texture(&cmd);
                }
            }
        }
    }

    fn copy_buffer(
        &mut self,
        dst_obj: &BlitObject,
        dst_subresource: u32,
        dst_x: u32,
        src_obj: &BlitObject,
        src_subresource: u32,
        p_src_box: *const D3d11Box,
    ) {
        let BlitObjectDesc::Buffer(src_desc) = &src_obj.desc else {
            return;
        };
        let (src_left, src_right) = if p_src_box.is_null() {
            (0_u32, src_desc.byte_width)
        } else {
            // SAFETY: caller supplied a non-null `D3D11_BOX` pointer.
            let b = unsafe { *p_src_box };
            (b.left, b.right)
        };
        if src_right <= src_left {
            return;
        }
        let size = src_right - src_left;

        if let Some(staging_dst) = get_staging_ref(dst_obj.p_resource, dst_subresource) {
            if let Some(staging_src) = get_staging_ref(src_obj.p_resource, src_subresource) {
                self.switch_to_blit_encoder();
                staging_dst.use_copy_destination(self);
                staging_src.use_copy_source(self);
                #[cfg(test)]
                if let (Some(src_h), Some(dst_h)) =
                    (staging_src.test_handle(), staging_dst.test_handle())
                {
                    record_blit_b2b(
                        src_h,
                        u64::from(src_left),
                        dst_h,
                        u64::from(dst_x),
                        u64::from(size),
                    );
                }
                // C++ closure #3 (`CopyBuffer` staging -> staging, d3d11_context_impl.cpp:3842).
                M::emit_op(self, move |enc| {
                    let (src_h, src_off) = staging_src.access(enc, src_left, size, Access::READ);
                    let (dst_h, dst_off) = staging_dst.access(enc, dst_x, size, Access::WRITE);
                    let cmd = WmtcmdBlitCopyFromBufferToBuffer {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        src_offset: u64::from(src_left) + src_off,
                        dst: dst_h,
                        dst_offset: u64::from(dst_x) + dst_off,
                        copy_length: u64::from(size),
                    };
                    enc.encode_blit_command(cmd);
                });
            } else if let Some(src_buf) = get_buffer_ref(src_obj.p_resource) {
                self.switch_to_blit_encoder();
                staging_dst.use_copy_destination(self);
                #[cfg(test)]
                if let (Some((src_h, src_off)), Some(dst_h)) =
                    (src_buf.test_handle(), staging_dst.test_handle())
                {
                    record_blit_b2b(
                        src_h,
                        u64::from(src_left) + src_off,
                        dst_h,
                        u64::from(dst_x),
                        u64::from(size),
                    );
                }
                // C++ closure #4 (`CopyBuffer` default -> staging, d3d11_context_impl.cpp:3859).
                M::emit_op(self, move |enc| {
                    let (src_h, src_off) = src_buf.access(enc, src_left, size, Access::READ);
                    let (dst_h, dst_off) = staging_dst.access(enc, dst_x, size, Access::WRITE);
                    let cmd = WmtcmdBlitCopyFromBufferToBuffer {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        src_offset: u64::from(src_left) + src_off,
                        dst: dst_h,
                        dst_offset: u64::from(dst_x) + dst_off,
                        copy_length: u64::from(size),
                    };
                    enc.encode_blit_command(cmd);
                });
            }
        } else if let Some(dst_buf) = get_buffer_ref(dst_obj.p_resource) {
            if let Some(staging_src) = get_staging_ref(src_obj.p_resource, src_subresource) {
                self.switch_to_blit_encoder();
                staging_src.use_copy_source(self);
                #[cfg(test)]
                if let (Some(src_h), Some((dst_h, dst_off))) =
                    (staging_src.test_handle(), dst_buf.test_handle())
                {
                    record_blit_b2b(
                        src_h,
                        u64::from(src_left),
                        dst_h,
                        u64::from(dst_x) + dst_off,
                        u64::from(size),
                    );
                }
                // C++ closure #5 (`CopyBuffer` staging -> default, d3d11_context_impl.cpp:3878).
                M::emit_op(self, move |enc| {
                    let (src_h, src_off) = staging_src.access(enc, src_left, size, Access::READ);
                    let (dst_h, dst_off) = dst_buf.access(enc, dst_x, size, Access::WRITE);
                    let cmd = WmtcmdBlitCopyFromBufferToBuffer {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        src_offset: u64::from(src_left) + src_off,
                        dst: dst_h,
                        dst_offset: u64::from(dst_x) + dst_off,
                        copy_length: u64::from(size),
                    };
                    enc.encode_blit_command(cmd);
                });
            } else if let Some(src_buf) = get_buffer_ref(src_obj.p_resource) {
                self.switch_to_blit_encoder();
                #[cfg(test)]
                if let (Some((src_h, src_off)), Some((dst_h, dst_off))) =
                    (src_buf.test_handle(), dst_buf.test_handle())
                {
                    record_blit_b2b(
                        src_h,
                        u64::from(src_left) + src_off,
                        dst_h,
                        u64::from(dst_x) + dst_off,
                        u64::from(size),
                    );
                }
                // C++ closure #6 (`CopyBuffer` default -> default, d3d11_context_impl.cpp:3892).
                M::emit_op(self, move |enc| {
                    let (src_h, src_off) = src_buf.access(enc, src_left, size, Access::READ);
                    let (dst_h, dst_off) = dst_buf.access(enc, dst_x, size, Access::WRITE);
                    let cmd = WmtcmdBlitCopyFromBufferToBuffer {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        src_offset: u64::from(src_left) + src_off,
                        dst: dst_h,
                        dst_offset: u64::from(dst_x) + dst_off,
                        copy_length: u64::from(size),
                    };
                    enc.encode_blit_command(cmd);
                });
            }
        }
    }

    fn copy_texture(&mut self, cmd: &TextureCopyCommand) {
        let src_bc = (cmd.src_format_flag & MTL_DXGI_FORMAT_BC) != 0;
        let dst_bc = (cmd.dst_format_flag & MTL_DXGI_FORMAT_BC) != 0;
        if src_bc != dst_bc {
            if src_bc {
                self.copy_texture_from_compressed(cmd);
            } else {
                self.copy_texture_to_compressed(cmd);
            }
            return;
        }
        self.copy_texture_bitcast(cmd);
    }

    fn copy_texture_bitcast(&mut self, cmd: &TextureCopyCommand) {
        let src_mip = cmd.src.mip_level;
        let src_slice = cmd.src.array_slice;
        let dst_mip = cmd.dst.mip_level;
        let dst_slice = cmd.dst.array_slice;
        let src_origin = cmd.src_origin;
        let src_size = cmd.src_size;
        let dst_origin = cmd.dst_origin;
        let src_format_flag = cmd.src_format_flag;
        let dst_format_flag = cmd.dst_format_flag;
        let src_bpt = cmd.src_bytes_per_texel;
        let dst_bpt = cmd.dst_bytes_per_texel;
        let dim_incompat = cmd.dimension_incompatible;

        let is_live = self.is_live();
        if let Some(staging_dst) = get_staging_ref(cmd.p_dst, cmd.dst_subresource) {
            if let Some(staging_src) = get_staging_ref(cmd.p_src, cmd.src_subresource) {
                self.switch_to_blit_encoder();
                staging_dst.use_copy_destination(self);
                staging_src.use_copy_source(self);
                let src_len = u32::try_from(staging_src.length()).unwrap_or(u32::MAX);
                let dst_len = u32::try_from(staging_dst.length()).unwrap_or(u32::MAX);
                // C++ closure #7 (`CopyTextureBitcast` staging -> staging, d3d11_context_impl.cpp:3933).
                M::emit_op(self, move |enc| {
                    let (src_h, src_sub_off) = staging_src.access(enc, 0, src_len, Access::READ);
                    let (dst_h, dst_sub_off) = staging_dst.access(enc, 0, dst_len, Access::WRITE);
                    if (src_format_flag & MTL_DXGI_FORMAT_BC) != 0 {
                        crate::LOG.err("copy between staging BC texture");
                        return;
                    }
                    let src_image_pitch = staging_src.bytes_per_image();
                    let src_row_pitch = staging_src.bytes_per_row();
                    let dst_image_pitch = staging_dst.bytes_per_image();
                    let dst_row_pitch = staging_dst.bytes_per_row();
                    for offset_z in 0..src_size[2] {
                        for offset_y in 0..src_size[1] {
                            let src_offset = (src_origin[2] + offset_z) * src_image_pitch
                                + (src_origin[1] + offset_y) * src_row_pitch
                                + src_origin[0] * src_bpt;
                            let dst_offset = (dst_origin[2] + offset_z) * dst_image_pitch
                                + (dst_origin[1] + offset_y) * dst_row_pitch
                                + dst_origin[0] * dst_bpt;
                            let cmdcp = WmtcmdBlitCopyFromBufferToBuffer {
                                r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
                                reserved: [0; 3],
                                next: NO_NEXT,
                                src: src_h,
                                src_offset: src_sub_off + u64::from(src_offset),
                                dst: dst_h,
                                dst_offset: dst_sub_off + u64::from(dst_offset),
                                copy_length: u64::from(src_size[0] * dst_bpt),
                            };
                            enc.encode_blit_command(cmdcp);
                        }
                    }
                });
            } else if let Some(src_tex) = get_texture_ref(cmd.p_src) {
                if (dst_format_flag & MTL_DXGI_FORMAT_EMULATED_LINEAR_DEPTH_STENCIL) != 0 {
                    let to_d24s8 = (dst_format_flag & MTL_DXGI_FORMAT_EMULATED_D24) != 0;
                    self.invalidate_current_pass(true);
                    staging_dst.use_copy_destination(self);
                    // C++ closure #8 (`CopyTextureBitcast` depth-stencil to staging, d3d11_context_impl.cpp:3964).
                    M::emit_op(self, move |enc| {
                        if is_live {
                            staging_dst.depth_stencil_copy_from(
                                enc, &src_tex, src_mip, src_slice, to_d24s8,
                            );
                        } else {
                            src_tex.access_subresource(enc, src_mip, src_slice, Access::READ);
                            staging_dst.access_buffer(enc, Access::WRITE);
                        }
                    });
                    return;
                }
                self.switch_to_blit_encoder();
                staging_dst.use_copy_destination(self);
                let dst_row_pitch = staging_dst.bytes_per_row();
                let dst_image_pitch = staging_dst.bytes_per_image();
                let dst_len = u32::try_from(staging_dst.length()).unwrap_or(u32::MAX);
                let offset = dst_origin[2] * dst_image_pitch
                    + dst_origin[1] * dst_row_pitch
                    + dst_origin[0] * dst_bpt;
                #[cfg(test)]
                if let (Some(src_h), Some(dst_h)) =
                    (src_tex.test_handle(), staging_dst.test_handle())
                {
                    record_blit_t2b(&WmtcmdBlitCopyFromTextureToBuffer {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        slice: src_slice,
                        level: src_mip,
                        origin: wmt_origin(src_origin),
                        size: wmt_size(src_size),
                        dst: dst_h,
                        offset: u64::from(offset),
                        bytes_per_row: dst_row_pitch,
                        bytes_per_image: dst_image_pitch,
                    });
                }
                // C++ closure #9 (`CopyTextureBitcast` default -> staging, d3d11_context_impl.cpp:3976).
                M::emit_op(self, move |enc| {
                    let src_h = src_tex.access(enc, src_mip, src_slice, Access::READ);
                    let (dst_h, dst_off) = staging_dst.access(enc, 0, dst_len, Access::WRITE);
                    let cmd_cpbuf = WmtcmdBlitCopyFromTextureToBuffer {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        slice: src_slice,
                        level: src_mip,
                        origin: wmt_origin(src_origin),
                        size: wmt_size(src_size),
                        dst: dst_h,
                        offset: u64::from(offset) + dst_off,
                        bytes_per_row: dst_row_pitch,
                        bytes_per_image: dst_image_pitch,
                    };
                    enc.encode_blit_command(cmd_cpbuf);
                });
            }
        } else if let Some(dst_tex) = get_texture_ref(cmd.p_dst) {
            if let Some(staging_src) = get_staging_ref(cmd.p_src, cmd.src_subresource) {
                if (src_format_flag & MTL_DXGI_FORMAT_EMULATED_LINEAR_DEPTH_STENCIL) != 0 {
                    let from_d24s8 = (src_format_flag & MTL_DXGI_FORMAT_EMULATED_D24) != 0;
                    self.invalidate_current_pass(true);
                    staging_src.use_copy_source(self);
                    // C++ closure #10 (`CopyTextureBitcast` staging -> depth-stencil, d3d11_context_impl.cpp:4003).
                    M::emit_op(self, move |enc| {
                        staging_src
                            .depth_stencil_copy_to(enc, &dst_tex, dst_mip, dst_slice, from_d24s8);
                    });
                    return;
                }
                self.switch_to_blit_encoder();
                staging_src.use_copy_source(self);
                let src_row_pitch = staging_src.bytes_per_row();
                let src_image_pitch = staging_src.bytes_per_image();
                let src_len = u32::try_from(staging_src.length()).unwrap_or(u32::MAX);
                let offset = if (src_format_flag & MTL_DXGI_FORMAT_BC) != 0 {
                    src_origin[2] * src_image_pitch
                        + (src_origin[1] >> 2) * src_row_pitch
                        + (src_origin[0] >> 2) * src_bpt
                } else {
                    src_origin[2] * src_image_pitch
                        + src_origin[1] * src_row_pitch
                        + src_origin[0] * src_bpt
                };
                #[cfg(test)]
                if let (Some(src_h), Some(dst_h)) =
                    (staging_src.test_handle(), dst_tex.test_handle())
                {
                    record_blit_b2t(&WmtcmdBlitCopyFromBufferToTexture {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        src_offset: u64::from(offset),
                        bytes_per_row: src_row_pitch,
                        bytes_per_image: src_image_pitch,
                        size: wmt_size(src_size),
                        dst: dst_h,
                        slice: dst_slice,
                        level: dst_mip,
                        origin: wmt_origin(dst_origin),
                    });
                }
                // C++ closure #11 (`CopyTextureBitcast` staging -> default, d3d11_context_impl.cpp:4015).
                M::emit_op(self, move |enc| {
                    let (src_h, src_off) = staging_src.access(enc, 0, src_len, Access::READ);
                    let dst_h = dst_tex.access(enc, dst_mip, dst_slice, Access::WRITE);
                    let cmd_cptex = WmtcmdBlitCopyFromBufferToTexture {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        src_offset: u64::from(offset) + src_off,
                        bytes_per_row: src_row_pitch,
                        bytes_per_image: src_image_pitch,
                        size: wmt_size(src_size),
                        dst: dst_h,
                        slice: dst_slice,
                        level: dst_mip,
                        origin: wmt_origin(dst_origin),
                    };
                    enc.encode_blit_command(cmd_cptex);
                });
            } else if let Some(src_tex) = get_texture_ref(cmd.p_src) {
                self.switch_to_blit_encoder();
                #[cfg(test)]
                if let (Some(src_h), Some(dst_h)) = (src_tex.test_handle(), dst_tex.test_handle())
                    && forget_srgb(dst_tex.pixel_format()) == forget_srgb(src_tex.pixel_format())
                    && !dim_incompat
                {
                    record_blit_t2t(&WmtcmdBlitCopyFromTextureToTexture {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        src_slice,
                        src_level: src_mip,
                        src_origin: wmt_origin(src_origin),
                        src_size: wmt_size(src_size),
                        dst: dst_h,
                        dst_slice,
                        dst_level: dst_mip,
                        dst_origin: wmt_origin(dst_origin),
                    });
                }
                // C++ closure #12 (`CopyTextureBitcast` default -> default, d3d11_context_impl.cpp:4042).
                M::emit_op(self, move |enc| {
                    let src_format = src_tex.pixel_format();
                    let dst_format = dst_tex.pixel_format();
                    let src_h = src_tex.access(enc, src_mip, src_slice, Access::READ);
                    let dst_h = dst_tex.access(enc, dst_mip, dst_slice, Access::WRITE);
                    if forget_srgb(dst_format) != forget_srgb(src_format) || dim_incompat {
                        let (bytes_per_row, bytes_per_image) =
                            if (src_format_flag & MTL_DXGI_FORMAT_BC) != 0 {
                                let bpr = (align4(src_size[0]) >> 2) * src_bpt;
                                let bpi = bpr * (align4(src_size[1]) >> 2);
                                (bpr, bpi)
                            } else {
                                let bpr = src_size[0] * src_bpt;
                                let bpi = bpr * src_size[1];
                                (bpr, bpi)
                            };
                        let bytes_total = u64::from(bytes_per_image) * u64::from(src_size[2]);
                        let (buffer, offset) = enc.allocate_temp_buffer(bytes_total, 256);
                        let cmd_cpbuf = WmtcmdBlitCopyFromTextureToBuffer {
                            r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
                            reserved: [0; 3],
                            next: NO_NEXT,
                            src: src_h,
                            slice: src_slice,
                            level: src_mip,
                            origin: wmt_origin(src_origin),
                            size: wmt_size(src_size),
                            dst: buffer,
                            offset,
                            bytes_per_row,
                            bytes_per_image,
                        };
                        enc.encode_blit_command(cmd_cpbuf);
                        let cmd_cptex = WmtcmdBlitCopyFromBufferToTexture {
                            r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
                            reserved: [0; 3],
                            next: NO_NEXT,
                            src: buffer,
                            src_offset: offset,
                            bytes_per_row,
                            bytes_per_image,
                            size: wmt_size(src_size),
                            dst: dst_h,
                            slice: dst_slice,
                            level: dst_mip,
                            origin: wmt_origin(dst_origin),
                        };
                        enc.encode_blit_command(cmd_cptex);
                        return;
                    }
                    let cmd_cptex = WmtcmdBlitCopyFromTextureToTexture {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        src_slice,
                        src_level: src_mip,
                        src_origin: wmt_origin(src_origin),
                        src_size: wmt_size(src_size),
                        dst: dst_h,
                        dst_slice,
                        dst_level: dst_mip,
                        dst_origin: wmt_origin(dst_origin),
                    };
                    enc.encode_blit_command(cmd_cptex);
                });
            }
        }
    }

    fn copy_texture_from_compressed(&mut self, cmd: &TextureCopyCommand) {
        let src_mip = cmd.src.mip_level;
        let src_slice = cmd.src.array_slice;
        let dst_mip = cmd.dst.mip_level;
        let dst_slice = cmd.dst.array_slice;
        let src_origin = cmd.src_origin;
        let src_size = cmd.src_size;
        let dst_origin = cmd.dst_origin;
        let src_bpt = cmd.src_bytes_per_texel;

        if get_staging_ref(cmd.p_dst, cmd.dst_subresource).is_some() {
            unimplemented_abort("copy texture: from compressed to staging");
        } else if let Some(dst_tex) = get_texture_ref(cmd.p_dst) {
            if get_staging_ref(cmd.p_src, cmd.src_subresource).is_some() {
                unimplemented_abort("copy texture: from compressed staging to default");
            } else if let Some(src_tex) = get_texture_ref(cmd.p_src) {
                self.switch_to_blit_encoder();
                // C++ closure #13 (`CopyTextureFromCompressed` default -> default, d3d11_context_impl.cpp:4117).
                M::emit_op(self, move |enc| {
                    let src_h = src_tex.access(enc, src_mip, src_slice, Access::READ);
                    let dst_h = dst_tex.access(enc, dst_mip, dst_slice, Access::WRITE);
                    let block_w = align4(src_size[0]) >> 2;
                    let block_h = align4(src_size[1]) >> 2;
                    let bytes_per_row = block_w * src_bpt;
                    let bytes_per_image = bytes_per_row * block_h;
                    let (buffer, offset) = enc.allocate_temp_buffer(
                        u64::from(bytes_per_image) * u64::from(src_size[2]),
                        256,
                    );
                    let cmd_cpbuf = WmtcmdBlitCopyFromTextureToBuffer {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        slice: src_slice,
                        level: src_mip,
                        origin: wmt_origin(src_origin),
                        size: wmt_size(src_size),
                        dst: buffer,
                        offset,
                        bytes_per_row,
                        bytes_per_image,
                    };
                    enc.encode_blit_command(cmd_cpbuf);
                    let cmd_cptex = WmtcmdBlitCopyFromBufferToTexture {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: buffer,
                        src_offset: offset,
                        bytes_per_row,
                        bytes_per_image,
                        size: wmt_size([block_w, block_h, src_size[2]]),
                        dst: dst_h,
                        slice: dst_slice,
                        level: dst_mip,
                        origin: wmt_origin(dst_origin),
                    };
                    enc.encode_blit_command(cmd_cptex);
                });
            }
        }
    }

    fn copy_texture_to_compressed(&mut self, cmd: &TextureCopyCommand) {
        let src_mip = cmd.src.mip_level;
        let src_slice = cmd.src.array_slice;
        let dst_mip = cmd.dst.mip_level;
        let dst_slice = cmd.dst.array_slice;
        let src_origin = cmd.src_origin;
        let src_size = cmd.src_size;
        let dst_origin = cmd.dst_origin;
        let src_bpt = cmd.src_bytes_per_texel;

        if get_staging_ref(cmd.p_dst, cmd.dst_subresource).is_some() {
            unimplemented_abort("copy texture: copy to compressed staging");
        } else if let Some(dst_tex) = get_texture_ref(cmd.p_dst) {
            if get_staging_ref(cmd.p_src, cmd.src_subresource).is_some() {
                unimplemented_abort("copy texture: from staging to compressed default");
            } else if let Some(src_tex) = get_texture_ref(cmd.p_src) {
                self.switch_to_blit_encoder();
                // C++ closure #14 (`CopyTextureToCompressed` default -> default, d3d11_context_impl.cpp:4168).
                M::emit_op(self, move |enc| {
                    let src_h = src_tex.access(enc, src_mip, src_slice, Access::READ);
                    let dst_h = dst_tex.access(enc, dst_mip, dst_slice, Access::WRITE);
                    let bytes_per_row = src_size[0] * src_bpt;
                    let bytes_per_image = bytes_per_row * src_size[1];
                    let (buffer, offset) = enc.allocate_temp_buffer(
                        u64::from(bytes_per_image) * u64::from(src_size[2]),
                        256,
                    );
                    let dst_mip_w = mip_extent(dst_tex.width(), dst_mip);
                    let dst_mip_h = mip_extent(dst_tex.height(), dst_mip);
                    let clamped_src_width =
                        (src_size[0] << 2).min(dst_mip_w.saturating_sub(dst_origin[0]));
                    let clamped_src_height =
                        (src_size[1] << 2).min(dst_mip_h.saturating_sub(dst_origin[1]));
                    let cmd_cpbuf = WmtcmdBlitCopyFromTextureToBuffer {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: src_h,
                        slice: src_slice,
                        level: src_mip,
                        origin: wmt_origin(src_origin),
                        size: wmt_size(src_size),
                        dst: buffer,
                        offset,
                        bytes_per_row,
                        bytes_per_image,
                    };
                    enc.encode_blit_command(cmd_cpbuf);
                    let cmd_cptex = WmtcmdBlitCopyFromBufferToTexture {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: buffer,
                        src_offset: offset,
                        bytes_per_row,
                        bytes_per_image,
                        size: wmt_size([clamped_src_width, clamped_src_height, src_size[2]]),
                        dst: dst_h,
                        slice: dst_slice,
                        level: dst_mip,
                        origin: wmt_origin(dst_origin),
                    };
                    enc.encode_blit_command(cmd_cptex);
                });
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "matches ID3D11DeviceContext2::UpdateTileMappings"
    )]
    pub fn update_tile_mappings(
        &mut self,
        tiled_resource: *mut c_void,
        num_tiled_resource_regions: u32,
        tiled_resource_region_start_coordinates: *const c_void,
        tiled_resource_region_sizes: *const c_void,
        tile_pool: *mut c_void,
        num_ranges: u32,
        range_flags: *const u32,
        tile_pool_start_offsets: *const u32,
        range_tile_counts: *const u32,
        flags: u32,
    ) -> Hresult {
        let _ = (
            tiled_resource,
            num_tiled_resource_regions,
            tiled_resource_region_start_coordinates,
            tiled_resource_region_sizes,
            tile_pool,
            num_ranges,
            range_flags,
            tile_pool_start_offsets,
            range_tile_counts,
            flags,
        );
        unimplemented_abort("tiled resource: update mapping");
        E_NOTIMPL
    }

    pub fn copy_tile_mappings(
        &mut self,
        dst_tiled_resource: *mut c_void,
        dst_region_start_coordinate: *const c_void,
        src_tiled_resource: *mut c_void,
        src_region_start_coordinate: *const c_void,
        region_size: *const c_void,
        flags: u32,
    ) -> Hresult {
        let _ = (
            dst_tiled_resource,
            dst_region_start_coordinate,
            src_tiled_resource,
            src_region_start_coordinate,
            region_size,
            flags,
        );
        unimplemented_abort("tiled resource: copy mapping");
        E_NOTIMPL
    }

    pub fn copy_tiles(
        &mut self,
        tiled_resource: *mut c_void,
        tile_region_start_coordinate: *const c_void,
        tile_region_size: *const c_void,
        buffer: *mut c_void,
        buffer_start_offset_in_bytes: u64,
        flags: u32,
    ) {
        let _ = (
            tiled_resource,
            tile_region_start_coordinate,
            tile_region_size,
            buffer,
            buffer_start_offset_in_bytes,
            flags,
        );
        unimplemented_abort("tiled resource: copy tiles");
    }

    pub fn update_tiles(
        &mut self,
        dst_tiled_resource: *mut c_void,
        dst_tile_region_start_coordinate: *const c_void,
        dst_tile_region_size: *const c_void,
        src_tile_data: *const c_void,
        flags: u32,
    ) {
        let _ = (
            dst_tiled_resource,
            dst_tile_region_start_coordinate,
            dst_tile_region_size,
            src_tile_data,
            flags,
        );
        unimplemented_abort("tiled resource: update tiles");
    }

    pub fn resize_tile_pool(&mut self, tile_pool: *mut c_void, new_size_in_bytes: u64) -> Hresult {
        let _ = (tile_pool, new_size_in_bytes);
        unimplemented_abort("tiled resource: resize");
        E_NOTIMPL
    }

    pub fn tiled_resource_barrier(
        &mut self,
        tiled_resource_or_view_access_before_barrier: *mut c_void,
        tiled_resource_or_view_access_after_barrier: *mut c_void,
    ) {
        let _ = (
            tiled_resource_or_view_access_before_barrier,
            tiled_resource_or_view_access_after_barrier,
        );
        unimplemented_abort("tiled resource: barrier");
    }
}

#[cfg(test)]
pub(super) struct DynamicStats {
    pub current_suballocation: u32,
    pub suballocation_rotations: u32,
    pub allocation_renames: u32,
    pub current_bytes: Vec<u8>,
}

#[cfg(test)]
struct TestDynamicEntry {
    length: usize,
    suballocation_count: u32,
    current_suballocation: u32,
    suballocation_rotations: u32,
    allocation_renames: u32,
    storage: Vec<u8>,
}

#[cfg(test)]
#[derive(Default)]
struct TestTransferState {
    buffers: rustc_hash::FxHashMap<usize, (u64, u64, u64)>,
    textures: rustc_hash::FxHashMap<usize, (u64, u32, u32, u32)>,
    stagings: rustc_hash::FxHashMap<(usize, u32), (u64, u32, u32, u64)>,
    dynamics: rustc_hash::FxHashMap<usize, TestDynamicEntry>,
    staging_mem: rustc_hash::FxHashMap<u64, Vec<u8>>,
    recorded_blit_cmds: Vec<String>,
    not_implemented_msgs: Vec<String>,
}

#[cfg(test)]
thread_local! {
    static TEST_STATE: std::cell::RefCell<TestTransferState> =
        std::cell::RefCell::new(TestTransferState::default());
}

#[cfg(test)]
pub(super) fn clear_test_state() {
    TEST_STATE.with(|s| {
        *s.borrow_mut() = TestTransferState::default();
    });
}

#[cfg(test)]
pub(super) fn register_test_buffer(res: *mut c_void, handle: u64, offset: u64, length: u64) {
    TEST_STATE.with(|s| {
        s.borrow_mut()
            .buffers
            .insert(res.addr(), (handle, offset, length));
    });
}

#[cfg(test)]
pub(super) fn register_test_texture(
    res: *mut c_void,
    handle: u64,
    dxgi_format: u32,
    width: u32,
    height: u32,
) {
    let (_, fmt_desc) = query_dxgi(dxgi_format);
    TEST_STATE.with(|s| {
        s.borrow_mut()
            .textures
            .insert(res.addr(), (handle, fmt_desc.pixel_format, width, height));
    });
}

#[cfg(test)]
pub(super) fn register_test_staging(
    res: *mut c_void,
    subresource: u32,
    handle: u64,
    bytes_per_row: u32,
    bytes_per_image: u32,
    length: u64,
) {
    TEST_STATE.with(|s| {
        s.borrow_mut().stagings.insert(
            (res.addr(), subresource),
            (handle, bytes_per_row, bytes_per_image, length),
        );
    });
}

#[cfg(test)]
pub(super) fn register_test_dynamic_buffer(
    res: *mut c_void,
    _handle: u64,
    length: u32,
    suballocation_count: u32,
    _flags: dxmt_core::resource::BufferAllocationFlags,
) {
    let len = length as usize;
    let total = len.saturating_mul(suballocation_count.max(1) as usize);
    TEST_STATE.with(|s| {
        s.borrow_mut().dynamics.insert(
            res.addr(),
            TestDynamicEntry {
                length: len,
                suballocation_count: suballocation_count.max(1),
                current_suballocation: 0,
                suballocation_rotations: 0,
                allocation_renames: 0,
                storage: vec![0; total],
            },
        );
    });
}

#[cfg(test)]
pub(super) fn has_test_dynamic_buffer(res: *mut c_void) -> bool {
    TEST_STATE.with(|s| s.borrow().dynamics.contains_key(&res.addr()))
}

#[cfg(test)]
pub(super) fn update_test_dynamic_buffer(
    res: *mut c_void,
    discard: bool,
    copy_offset: usize,
    src_slice: &[u8],
) {
    TEST_STATE.with(|s| {
        let mut st = s.borrow_mut();
        let Some(entry) = st.dynamics.get_mut(&res.addr()) else {
            return;
        };
        if discard {
            entry.suballocation_rotations += 1;
            if entry.current_suballocation + 1 < entry.suballocation_count {
                entry.current_suballocation += 1;
            } else {
                entry.current_suballocation = 0;
                entry.allocation_renames += 1;
                entry.storage.fill(0);
            }
        }
        let base = (entry.current_suballocation as usize) * entry.length + copy_offset;
        let end = (base + src_slice.len()).min(entry.storage.len());
        if base < end {
            entry.storage[base..end].copy_from_slice(&src_slice[..(end - base)]);
        }
    });
}

#[cfg(test)]
pub(super) fn dynamic_buffer_stats(res: *mut c_void) -> Option<DynamicStats> {
    TEST_STATE.with(|s| {
        let st = s.borrow();
        let entry = st.dynamics.get(&res.addr())?;
        let start = (entry.current_suballocation as usize) * entry.length;
        let end = start + entry.length;
        Some(DynamicStats {
            current_suballocation: entry.current_suballocation,
            suballocation_rotations: entry.suballocation_rotations,
            allocation_renames: entry.allocation_renames,
            current_bytes: entry.storage[start..end].to_vec(),
        })
    })
}

#[cfg(test)]
pub(super) fn write_test_staging_slice(handle: u64, offset: u64, data: &[u8]) {
    TEST_STATE.with(|s| {
        let mut st = s.borrow_mut();
        let vec = st.staging_mem.entry(handle).or_default();
        let start = usize::try_from(offset).unwrap_or(0);
        let end = start.saturating_add(data.len());
        if vec.len() < end {
            vec.resize(end, 0);
        }
        vec[start..end].copy_from_slice(data);
    });
}

#[cfg(test)]
pub(super) fn staging_slice_bytes(handle: u64, offset: u64, len: usize) -> Vec<u8> {
    TEST_STATE.with(|s| {
        let st = s.borrow();
        let Some(vec) = st.staging_mem.get(&handle) else {
            return Vec::new();
        };
        let start = usize::try_from(offset).unwrap_or(0);
        let end = start.saturating_add(len).min(vec.len());
        if start >= end {
            return Vec::new();
        }
        vec[start..end].to_vec()
    })
}

#[cfg(test)]
fn lookup_test_buffer(res: *mut c_void) -> Option<(u64, u64, u64)> {
    TEST_STATE.with(|s| s.borrow().buffers.get(&res.addr()).copied())
}

#[cfg(test)]
fn lookup_test_texture(res: *mut c_void) -> Option<TextureRef> {
    TEST_STATE.with(|s| {
        let (handle, pixel_format, width, height) = *s.borrow().textures.get(&res.addr())?;
        Some(TextureRef::Test {
            handle,
            pixel_format,
            width,
            height,
        })
    })
}

#[cfg(test)]
fn lookup_test_staging(res: *mut c_void, subresource: u32) -> Option<StagingRef> {
    TEST_STATE.with(|s| {
        let (handle, bytes_per_row, bytes_per_image, length) =
            *s.borrow().stagings.get(&(res.addr(), subresource))?;
        Some(StagingRef::Test {
            handle,
            bytes_per_row,
            bytes_per_image,
            length,
        })
    })
}

#[cfg(test)]
pub(super) fn record_blit_b2b(
    src: u64,
    src_offset: u64,
    dst: u64,
    dst_offset: u64,
    copy_length: u64,
) {
    TEST_STATE.with(|s| {
        s.borrow_mut().recorded_blit_cmds.push(format!(
            "blit 1 src=#{src} src_offset={src_offset} dst=#{dst} dst_offset={dst_offset} copy_length={copy_length}"
        ));
    });
}

#[cfg(test)]
pub(super) fn record_blit_b2t(cmd: &WmtcmdBlitCopyFromBufferToTexture) {
    TEST_STATE.with(|s| {
        s.borrow_mut().recorded_blit_cmds.push(format!(
            "blit 2 src=#{} src_offset={} bytes_per_row={} bytes_per_image={} size.width={} size.height={} size.depth={} dst=#{} slice={} level={} origin.x={} origin.y={} origin.z={}",
            cmd.src,
            cmd.src_offset,
            cmd.bytes_per_row,
            cmd.bytes_per_image,
            cmd.size.width,
            cmd.size.height,
            cmd.size.depth,
            cmd.dst,
            cmd.slice,
            cmd.level,
            cmd.origin.x,
            cmd.origin.y,
            cmd.origin.z,
        ));
    });
}

#[cfg(test)]
pub(super) fn record_blit_t2b(cmd: &WmtcmdBlitCopyFromTextureToBuffer) {
    TEST_STATE.with(|s| {
        s.borrow_mut().recorded_blit_cmds.push(format!(
            "blit 3 src=#{} slice={} level={} origin.x={} origin.y={} origin.z={} size.width={} size.height={} size.depth={} dst=#{} offset={} bytes_per_row={} bytes_per_image={}",
            cmd.src,
            cmd.slice,
            cmd.level,
            cmd.origin.x,
            cmd.origin.y,
            cmd.origin.z,
            cmd.size.width,
            cmd.size.height,
            cmd.size.depth,
            cmd.dst,
            cmd.offset,
            cmd.bytes_per_row,
            cmd.bytes_per_image,
        ));
    });
}

#[cfg(test)]
pub(super) fn record_blit_t2t(cmd: &WmtcmdBlitCopyFromTextureToTexture) {
    TEST_STATE.with(|s| {
        s.borrow_mut().recorded_blit_cmds.push(format!(
            "blit 4 src=#{} src_slice={} src_level={} src_origin.x={} src_origin.y={} src_origin.z={} src_size.width={} src_size.height={} src_size.depth={} dst=#{} dst_slice={} dst_level={} dst_origin.x={} dst_origin.y={} dst_origin.z={}",
            cmd.src,
            cmd.src_slice,
            cmd.src_level,
            cmd.src_origin.x,
            cmd.src_origin.y,
            cmd.src_origin.z,
            cmd.src_size.width,
            cmd.src_size.height,
            cmd.src_size.depth,
            cmd.dst,
            cmd.dst_slice,
            cmd.dst_level,
            cmd.dst_origin.x,
            cmd.dst_origin.y,
            cmd.dst_origin.z,
        ));
    });
}

#[cfg(test)]
fn record_not_implemented(msg: &str) {
    TEST_STATE.with(|s| {
        s.borrow_mut().not_implemented_msgs.push(msg.to_owned());
    });
}

#[cfg(test)]
pub(super) fn take_recorded_blit_cmds() -> Vec<String> {
    TEST_STATE.with(|s| core::mem::take(&mut s.borrow_mut().recorded_blit_cmds))
}

#[cfg(test)]
pub(super) fn take_not_implemented_msgs() -> Vec<String> {
    TEST_STATE.with(|s| core::mem::take(&mut s.borrow_mut().not_implemented_msgs))
}

#[cfg(test)]
pub mod tests;
