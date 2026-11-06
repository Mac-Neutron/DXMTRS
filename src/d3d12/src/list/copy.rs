//! Buffer/texture copies, MSAA resolves, and query recording (`d3d12_command_list.cpp`).

use core::{
    ffi::c_void,
    mem::{align_of, size_of},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use std::sync::Arc;

use dxmt_core::{
    format::{
        MTL_DXGI_FORMAT_BC, MtlDxgiFormatDesc, depth_stencil_planar_flags, query_dxgi,
        unsigned_integer,
    },
    resource::{Texture, TextureAllocationFlags, TextureViewDescriptor},
};
use dxmt_util::sync::lock;
use winemetal_sys::Obj;
use winemetal_wire::{
    MemPtr, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE, WMT_BLIT_COMMAND_COPY_TEXTURE,
    WMT_BLIT_COMMAND_FILL_BUFFER, WMT_BLIT_COMMAND_NOP, WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL,
    WMT_BLIT_OPTION_NONE, WMT_BLIT_OPTION_STENCIL_FROM_DEPTH_STENCIL, WMT_PIXEL_FORMAT_INVALID,
    WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
    WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE3D, WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW,
    WMT_TEXTURE_USAGE_RENDER_TARGET, WMT_TEXTURE_USAGE_SHADER_READ, WMTOrigin, WMTSize,
    WMTTextureInfo, WmtcmdBase, WmtcmdBlitCopyFromBufferToBuffer,
    WmtcmdBlitCopyFromBufferToTextureWithblitoption,
    WmtcmdBlitCopyFromTextureToBufferWithblitoption, WmtcmdBlitCopyFromTextureToTexture,
    WmtcmdBlitCopyTexture, WmtcmdBlitFillbuffer, WmtcmdBlitNop, original_format,
};

use crate::{
    abi::{
        barrier::{D3D12_HEAP_TYPE_CUSTOM, D3D12_HEAP_TYPE_READBACK},
        list_ops::{
            D3D12_QUERY_TYPE_BINARY_OCCLUSION, D3D12_QUERY_TYPE_OCCLUSION,
            D3D12_QUERY_TYPE_TIMESTAMP, D3D12_RESOLVE_MODE_AVERAGE,
            D3D12_RESOLVE_MODE_DECODE_SAMPLER_FEEDBACK, D3D12_RESOLVE_MODE_DECOMPRESS,
            D3D12_RESOLVE_MODE_ENCODE_SAMPLER_FEEDBACK, D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX, Rect, TextureCopyLocation,
        },
        resource::{
            D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE, D3D12_RESOURCE_DIMENSION_BUFFER,
            D3D12_RESOURCE_DIMENSION_TEXTURE3D, D3d12Box,
        },
    },
    allocator::Allocator,
    encoder::{
        BlitEncoderData, EncoderData, EncoderSample, EncoderType, ResolveEncoderData,
        TextureViewRef, TimestampResolve,
    },
    list::{DirtyFlags, List, PendingResolve},
    query_heap::{QueryHeap, TIMESTAMPS_PER_BUFFER},
    resource::Resource,
    resource_math::{decompose_subresource, resource_extent},
};

/// Returns a null [`MemPtr`] on both 32-bit and 64-bit targets.
#[cfg(target_pointer_width = "64")]
#[must_use]
pub const fn null_mem_ptr() -> MemPtr {
    MemPtr { ptr: 0 }
}

/// Returns a null [`MemPtr`] on both 32-bit and 64-bit targets.
#[cfg(target_pointer_width = "32")]
#[must_use]
pub const fn null_mem_ptr() -> MemPtr {
    MemPtr { ptr: 0, high: 0 }
}

/// Converts a host pointer into a [`MemPtr`].
#[cfg(target_pointer_width = "64")]
#[must_use]
pub fn ptr_to_mem_ptr(ptr: *mut u8) -> MemPtr {
    MemPtr {
        ptr: ptr.expose_provenance() as u64,
    }
}

/// Converts a host pointer into a [`MemPtr`].
#[cfg(target_pointer_width = "32")]
#[must_use]
pub fn ptr_to_mem_ptr(ptr: *mut u8) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(ptr.expose_provenance()).unwrap_or(0),
        high: 0,
    }
}

/// Resource identity for encoder dependency tracking (`ResourceKey`).
///
/// # Safety
///
/// If non-null, `resource` must point to a valid [`Resource`].
#[must_use]
pub unsafe fn resource_key(resource: *mut c_void) -> *const c_void {
    if resource.is_null() {
        return ptr::null();
    }
    // SAFETY: caller guarantees non-null `resource` is a valid `Resource`.
    let res = unsafe { &*resource.cast::<Resource>() };
    if let Some(tex) = &res.texture {
        return Arc::as_ptr(tex).cast::<c_void>();
    }
    if let Some(buf) = &res.buffer {
        return Arc::as_ptr(buf).cast::<c_void>();
    }
    ptr::null()
}

/// Opens or reuses a [`BlitEncoderData`] on `alloc` (`PreBlit`, `d3d12_command_list.cpp:823`).
pub fn pre_blit(
    alloc: &mut Allocator,
    dst_key: *const c_void,
    timestamp_resolve: bool,
) -> *mut BlitEncoderData {
    let need_new = if alloc.encoder_current.is_null() {
        true
    } else {
        // SAFETY: `encoder_current` is non-null and points to the active encoder header.
        let kind = unsafe { (*alloc.encoder_current).kind };
        kind != EncoderType::Blit
    };
    if need_new {
        alloc.invalidate_current_pass();
        let blit = alloc
            .allocate_cpu_heap(size_of::<BlitEncoderData>(), align_of::<BlitEncoderData>())
            .cast::<c_void>()
            .cast::<BlitEncoderData>();
        let mut base = EncoderData::new(EncoderType::Blit, 0);
        base.barriers = alloc.barriers;
        base.barriers_last = alloc.barriers;
        base.writes_unknown = false;
        base.query_resolve_only = timestamp_resolve;
        // SAFETY: `blit` points to aligned uninitialized storage from `allocate_cpu_heap`.
        unsafe {
            ptr::write(
                blit,
                BlitEncoderData {
                    base,
                    cmd_head: WmtcmdBlitNop {
                        r#type: WMT_BLIT_COMMAND_NOP,
                        reserved: [0; 3],
                        next: null_mem_ptr(),
                    },
                    cmd_tail: ptr::null_mut(),
                },
            );
        };
        // SAFETY: `blit` was initialized above.
        let head_ptr = unsafe { (&raw mut (*blit).cmd_head).cast::<WmtcmdBase>() };
        // SAFETY: `blit` was initialized above.
        unsafe { (*blit).cmd_tail = head_ptr };
        alloc.encoder_current = blit.cast::<EncoderData>();
    } else {
        // SAFETY: `encoder_current` is non-null and points to an active `BlitEncoderData`.
        unsafe { (*alloc.encoder_current).timestamp_only = false };
        // SAFETY: `encoder_current` is non-null and points to an active `BlitEncoderData`.
        unsafe { (*alloc.encoder_current).query_resolve_only &= timestamp_resolve };
    }
    if !dst_key.is_null() {
        alloc.writes(dst_key);
    }
    alloc.encoder_current.cast::<BlitEncoderData>()
}

/// Appends a blit command `cmd` to `alloc`'s active [`BlitEncoderData`].
///
/// Mirrors `EncodeBlitCommand` (`d3d12_command_allocator.hpp:436`).
pub fn encode_blit_cmd<T>(alloc: &mut Allocator, cmd: T) -> *mut T {
    let blit = alloc.encoder_current.cast::<BlitEncoderData>();
    let storage = alloc
        .allocate_cpu_heap(size_of::<T>(), align_of::<T>().max(16))
        .cast::<c_void>()
        .cast::<T>();
    // SAFETY: `storage` points to aligned uninitialized storage from `allocate_cpu_heap`.
    unsafe { ptr::write(storage, cmd) };
    let base_ptr = storage.cast::<WmtcmdBase>();
    // SAFETY: every blit command struct begins with `WmtcmdBase` (`type` + `next`).
    unsafe { (*base_ptr).next = null_mem_ptr() };
    // SAFETY: `blit` has a valid `cmd_tail` pointing into the CPU arena.
    let tail = unsafe { (*blit).cmd_tail };
    // SAFETY: `tail` points to the previous command's `WmtcmdBase` header.
    unsafe { (*tail).next = ptr_to_mem_ptr(storage.cast::<u8>()) };
    // SAFETY: `blit` is the active `BlitEncoderData`.
    unsafe { (*blit).cmd_tail = base_ptr };
    // SAFETY: `blit` is the active `BlitEncoderData`.
    unsafe { (*blit).base.barriers_last = alloc.barriers };
    storage
}

/// Source or destination endpoint for [`copy_reinterpret`].
pub struct CopyReinterpretSide<'a> {
    pub texture: &'a Arc<Texture>,
    pub format: &'a MtlDxgiFormatDesc,
    pub level: u32,
    pub slice: u32,
    pub origin: WMTOrigin,
}

/// Reinterpreting copy between formats with matching bytes-per-texel/block.
///
/// Mirrors `CopyReinterpret` (`d3d12_command_list.cpp:846`).
pub fn copy_reinterpret(
    alloc: &mut Allocator,
    src: &CopyReinterpretSide<'_>,
    src_size: WMTSize,
    dst: &CopyReinterpretSide<'_>,
) {
    let src_bc = (src.format.flag & MTL_DXGI_FORMAT_BC) != 0;
    let dst_bc = (dst.format.flag & MTL_DXGI_FORMAT_BC) != 0;
    if src.format.bytes_per_texel != dst.format.bytes_per_texel {
        crate::warn!(
            "CopyReinterpret: formats {} and {} differ in size, skipped",
            src.format.pixel_format,
            dst.format.pixel_format
        );
        return;
    }
    let src_pf = src.texture.info().pixel_format;
    let dst_pf = dst.texture.info().pixel_format;
    let src_depth = depth_stencil_planar_flags(src_pf);
    let dst_depth = depth_stencil_planar_flags(dst_pf);
    let src_layout = unsigned_integer(src_pf);
    let one_layout = src_pf == dst.format.pixel_format
        || (src_layout != WMT_PIXEL_FORMAT_INVALID
            && src_layout == unsigned_integer(dst.format.pixel_format));
    if !src_bc && !dst_bc && src_depth == 0 && dst_depth == 0 && one_layout {
        let key = TextureViewDescriptor {
            format: original_format(dst.format.pixel_format),
            texture_type: src.texture.info().r#type(),
            first_mip: 0,
            mip_count: src.texture.info().mipmap_level_count(),
            first_slice: 0,
            array_size: src.texture.array_length(),
        }
        .key();
        let src_handle = src.texture.current().map_or(0, |a| a.view(key).texture);
        let dst_handle = dst.texture.current().map_or(0, |a| a.texture());
        encode_blit_cmd(
            alloc,
            WmtcmdBlitCopyFromTextureToTexture {
                r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE,
                reserved: [0; 3],
                next: null_mem_ptr(),
                src: src_handle,
                src_level: src.level,
                src_slice: src.slice,
                src_origin: src.origin,
                src_size,
                dst: dst_handle,
                dst_level: dst.level,
                dst_slice: dst.slice,
                dst_origin: dst.origin,
            },
        );
        return;
    }
    let src_scale = if src_bc { 4_u32 } else { 1_u32 };
    let dst_scale = if dst_bc { 4_u32 } else { 1_u32 };
    let units_w = u32::try_from(src_size.width)
        .unwrap_or(u32::MAX)
        .div_ceil(src_scale);
    let units_h = u32::try_from(src_size.height)
        .unwrap_or(u32::MAX)
        .div_ceil(src_scale);
    let bytes_per_row = (units_w * src.format.bytes_per_texel).next_multiple_of(256);
    let bytes_per_image = bytes_per_row * units_h;
    let temp_len = usize::try_from(u64::from(bytes_per_image) * src_size.depth).unwrap_or(0);
    let (temp, temp_offset) = alloc.allocate_temp_buffer(temp_len, 256);
    let src_handle = src.texture.current().map_or(0, |a| a.texture());
    let dst_handle = dst.texture.current().map_or(0, |a| a.texture());
    encode_blit_cmd(
        alloc,
        WmtcmdBlitCopyFromTextureToBufferWithblitoption {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
            reserved: [0; 3],
            next: null_mem_ptr(),
            src: src_handle,
            slice: src.slice,
            level: u16::try_from(src.level).unwrap_or(0),
            origin: src.origin,
            size: src_size,
            dst: temp,
            offset: temp_offset,
            bytes_per_row,
            bytes_per_image,
            options: if src_depth == 3 {
                WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL
            } else {
                WMT_BLIT_OPTION_NONE
            },
        },
    );
    let dst_width = u64::from((dst.texture.info().width >> dst.level).max(1));
    let dst_height = u64::from((dst.texture.info().height >> dst.level).max(1));
    let copy_size = WMTSize {
        width: (u64::from(units_w) * u64::from(dst_scale))
            .min(dst_width.saturating_sub(dst.origin.x)),
        height: (u64::from(units_h) * u64::from(dst_scale))
            .min(dst_height.saturating_sub(dst.origin.y)),
        depth: src_size.depth,
    };
    encode_blit_cmd(
        alloc,
        WmtcmdBlitCopyFromBufferToTextureWithblitoption {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
            reserved: [0; 3],
            next: null_mem_ptr(),
            src: temp,
            src_offset: temp_offset,
            bytes_per_row,
            bytes_per_image,
            size: copy_size,
            dst: dst_handle,
            slice: dst.slice,
            level: u16::try_from(dst.level).unwrap_or(0),
            origin: dst.origin,
            options: if dst_depth == 3 {
                WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL
            } else {
                WMT_BLIT_OPTION_NONE
            },
        },
    );
}

/// `ID3D12GraphicsCommandList::CopyBufferRegion` (`d3d12_command_list.cpp:918`).
///
/// # Safety
///
/// `this`, `dst_buffer`, and `src_buffer` are caller-supplied pointers.
pub unsafe fn copy_buffer_region(
    this: *mut c_void,
    dst_buffer: *mut c_void,
    dst_offset: u64,
    src_buffer: *mut c_void,
    src_offset: u64,
    num_bytes: u64,
) {
    crate::scope!("list.CopyBufferRegion");
    if this.is_null() || dst_buffer.is_null() || src_buffer.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `List` COM pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.allocator.is_null() {
        return;
    }
    // SAFETY: `list.allocator` is non-null during recording.
    let alloc = unsafe { &mut *list.allocator };
    // SAFETY: `dst_buffer` is a valid `Resource` COM pointer.
    let dst_res = unsafe { &*dst_buffer.cast::<Resource>() };
    // SAFETY: `src_buffer` is a valid `Resource` COM pointer.
    let src_res = unsafe { &*src_buffer.cast::<Resource>() };
    // SAFETY: `dst_buffer` is a valid `Resource` COM pointer.
    let dst_key = unsafe { resource_key(dst_buffer) };
    pre_blit(alloc, dst_key, false);
    let src_handle = src_res
        .buffer
        .as_ref()
        .and_then(|b| b.current())
        .map_or(0, |a| a.buffer());
    let dst_handle = dst_res
        .buffer
        .as_ref()
        .and_then(|b| b.current())
        .map_or(0, |a| a.buffer());
    encode_blit_cmd(
        alloc,
        WmtcmdBlitCopyFromBufferToBuffer {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
            reserved: [0; 3],
            next: null_mem_ptr(),
            src: src_handle,
            src_offset,
            dst: dst_handle,
            dst_offset,
            copy_length: num_bytes,
        },
    );
}

/// `ID3D12GraphicsCommandList::CopyTextureRegion` (`d3d12_command_list.cpp:937`).
///
/// # Safety
///
/// `this`, `dst`, `src`, and `src_box` are caller-supplied pointers.
pub unsafe fn copy_texture_region(
    this: *mut c_void,
    dst: *const c_void,
    dst_x: u32,
    dst_y: u32,
    dst_z: u32,
    src: *const c_void,
    src_box: *const c_void,
) {
    crate::scope!("list.CopyTextureRegion");
    if this.is_null() || dst.is_null() || src.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `List` COM pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.allocator.is_null() {
        return;
    }
    // SAFETY: `list.allocator` is non-null during recording.
    let alloc = unsafe { &mut *list.allocator };
    // SAFETY: `dst` points to a `D3D12_TEXTURE_COPY_LOCATION`.
    let dst_loc = unsafe { ptr::read_unaligned(dst.cast::<TextureCopyLocation>()) };
    // SAFETY: `src` points to a `D3D12_TEXTURE_COPY_LOCATION`.
    let src_loc = unsafe { ptr::read_unaligned(src.cast::<TextureCopyLocation>()) };
    if dst_loc.resource.is_null() || src_loc.resource.is_null() {
        return;
    }
    // SAFETY: `dst_loc.resource` is a valid `Resource` COM pointer.
    let dst_res = unsafe { &*dst_loc.resource.cast::<Resource>() };
    // SAFETY: `src_loc.resource` is a valid `Resource` COM pointer.
    let src_res = unsafe { &*src_loc.resource.cast::<Resource>() };
    // SAFETY: `dst_loc.resource` is a valid `Resource` COM pointer.
    let dst_key = unsafe { resource_key(dst_loc.resource) };
    pre_blit(alloc, dst_key, false);

    let src_desc = src_res.desc_scoped();
    let dst_desc = dst_res.desc_scoped();
    let mut src_level = 0_u32;
    let mut src_slice = 0_u32;
    let mut src_planar = 0_u32;
    let full_src_box = if src_loc.r#type == D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT {
        // SAFETY: `src_loc.r#type == PLACED_FOOTPRINT` selects the `placed_footprint` arm.
        let pf = unsafe { &src_loc.location.placed_footprint };
        D3d12Box {
            left: 0,
            top: 0,
            front: 0,
            right: pf.footprint.width,
            bottom: pf.footprint.height,
            back: pf.footprint.depth,
        }
    } else {
        // SAFETY: `src_loc.r#type == SUBRESOURCE_INDEX` selects the `subresource_index` arm.
        let sub_idx = unsafe { src_loc.location.subresource_index };
        let dec = decompose_subresource(src_desc, sub_idx);
        src_level = dec.mip_slice;
        src_slice = dec.array_slice;
        src_planar = dec.plane_slice;
        resource_extent(src_desc, src_level)
    };
    let sbox = if src_box.is_null() {
        full_src_box
    } else {
        // SAFETY: `src_box` points to a readable `D3D12_BOX`.
        unsafe { ptr::read_unaligned(src_box.cast::<D3d12Box>()) }
    };
    if sbox.left >= sbox.right || sbox.front >= sbox.back || sbox.top >= sbox.bottom {
        return;
    }

    if dst_loc.r#type == D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX {
        let Some(dst_tex) = dst_res.texture.as_ref() else {
            return;
        };
        // SAFETY: `dst_loc.r#type == SUBRESOURCE_INDEX` selects the `subresource_index` arm.
        let dst_sub_idx = unsafe { dst_loc.location.subresource_index };
        let dec = decompose_subresource(dst_desc, dst_sub_idx);
        let dst_level = dec.mip_slice;
        let dst_slice = dec.array_slice;
        let dst_planar = dec.plane_slice;
        let (hr, dst_format) = query_dxgi(dst_desc.format);
        if hr < 0 {
            crate::warn!("CopyTextureRegion: unsupported format {}", dst_desc.format);
            return;
        }
        let dst_planar_count = dst_format.planar_count;

        if src_loc.r#type == D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT {
            let Some(src_buf) = src_res.buffer.as_ref() else {
                return;
            };
            // SAFETY: `src_loc.r#type == PLACED_FOOTPRINT` selects the `placed_footprint` arm.
            let pf = unsafe { &src_loc.location.placed_footprint };
            let (src_hr, src_format) = query_dxgi(pf.footprint.format);
            if src_hr < 0 {
                crate::warn!(
                    "CopyTextureRegion: unsupported format {}",
                    pf.footprint.format
                );
                return;
            }
            let block_width = if (src_format.flag & MTL_DXGI_FORMAT_BC) != 0 {
                4_u32
            } else {
                1_u32
            };
            let src_depth_pitch = if dst_tex.info().r#type() == WMT_TEXTURE_TYPE3D {
                (pf.footprint.height / block_width) * pf.footprint.row_pitch
            } else {
                0
            };
            let src_offset = pf.offset
                + u64::from(sbox.left / block_width) * u64::from(src_format.bytes_per_texel)
                + u64::from(sbox.top / block_width) * u64::from(pf.footprint.row_pitch)
                + u64::from(sbox.front) * u64::from(src_depth_pitch);
            let dst_width = (dst_tex.info().width >> dst_level).max(1);
            let dst_height = (dst_tex.info().height >> dst_level).max(1);
            let size = WMTSize {
                width: u64::from((sbox.right - sbox.left).min(dst_width.saturating_sub(dst_x))),
                height: u64::from((sbox.bottom - sbox.top).min(dst_height.saturating_sub(dst_y))),
                depth: u64::from(sbox.back - sbox.front),
            };
            let options = if dst_planar_count > 1 {
                if dst_planar != 0 {
                    WMT_BLIT_OPTION_STENCIL_FROM_DEPTH_STENCIL
                } else {
                    WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL
                }
            } else {
                WMT_BLIT_OPTION_NONE
            };
            encode_blit_cmd(
                alloc,
                WmtcmdBlitCopyFromBufferToTextureWithblitoption {
                    r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
                    reserved: [0; 3],
                    next: null_mem_ptr(),
                    src: src_buf.current().map_or(0, |a| a.buffer()),
                    src_offset,
                    bytes_per_row: pf.footprint.row_pitch,
                    bytes_per_image: src_depth_pitch,
                    size,
                    dst: dst_tex.current().map_or(0, |a| a.texture()),
                    slice: dst_slice,
                    level: u16::try_from(dst_level).unwrap_or(0),
                    origin: WMTOrigin {
                        x: u64::from(dst_x),
                        y: u64::from(dst_y),
                        z: u64::from(dst_z),
                    },
                    options,
                },
            );
        } else {
            let Some(src_tex) = src_res.texture.as_ref() else {
                return;
            };
            let (src_hr, src_format) = query_dxgi(src_desc.format);
            if src_hr < 0 {
                crate::warn!("CopyTextureRegion: unsupported format {}", src_desc.format);
                return;
            }
            let src_planar_count = src_format.planar_count;
            let src_origin = WMTOrigin {
                x: u64::from(sbox.left),
                y: u64::from(sbox.top),
                z: u64::from(sbox.front),
            };
            let src_size = WMTSize {
                width: u64::from(sbox.right - sbox.left),
                height: u64::from(sbox.bottom - sbox.top),
                depth: u64::from(sbox.back - sbox.front),
            };
            let dst_origin = WMTOrigin {
                x: u64::from(dst_x),
                y: u64::from(dst_y),
                z: u64::from(dst_z),
            };
            if src_tex.info().pixel_format != dst_tex.info().pixel_format
                && src_planar_count == 1
                && dst_planar_count == 1
            {
                copy_reinterpret(
                    alloc,
                    &CopyReinterpretSide {
                        texture: src_tex,
                        format: &src_format,
                        level: src_level,
                        slice: src_slice,
                        origin: src_origin,
                    },
                    src_size,
                    &CopyReinterpretSide {
                        texture: dst_tex,
                        format: &dst_format,
                        level: dst_level,
                        slice: dst_slice,
                        origin: dst_origin,
                    },
                );
                return;
            }
            if dst_planar_count > 1 || src_planar_count > 1 {
                if dst_planar_count > 1 && src_planar_count > 1 && dst_planar != src_planar {
                    crate::warn!("CopyTextureRegion: unmatched planar");
                    return;
                }
                let texel_size = if dst_planar == 1 || src_planar == 1 {
                    1_u32
                } else {
                    4_u32
                };
                let bytes_per_row = ((sbox.right - sbox.left) * texel_size).next_multiple_of(256);
                let bytes_per_image = bytes_per_row * (sbox.bottom - sbox.top);
                let (temp_buffer, temp_buffer_offset) =
                    alloc.allocate_temp_buffer(usize::try_from(bytes_per_image).unwrap_or(0), 256);
                let src_options = if src_planar_count > 1 {
                    if src_planar != 0 {
                        WMT_BLIT_OPTION_STENCIL_FROM_DEPTH_STENCIL
                    } else {
                        WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL
                    }
                } else {
                    WMT_BLIT_OPTION_NONE
                };
                encode_blit_cmd(
                    alloc,
                    WmtcmdBlitCopyFromTextureToBufferWithblitoption {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
                        reserved: [0; 3],
                        next: null_mem_ptr(),
                        src: src_tex.current().map_or(0, |a| a.texture()),
                        slice: src_slice,
                        level: u16::try_from(src_level).unwrap_or(0),
                        origin: src_origin,
                        size: src_size,
                        dst: temp_buffer,
                        offset: temp_buffer_offset,
                        bytes_per_row,
                        bytes_per_image: 0,
                        options: src_options,
                    },
                );
                let dst_options = if dst_planar_count > 1 {
                    if dst_planar != 0 {
                        WMT_BLIT_OPTION_STENCIL_FROM_DEPTH_STENCIL
                    } else {
                        WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL
                    }
                } else {
                    WMT_BLIT_OPTION_NONE
                };
                encode_blit_cmd(
                    alloc,
                    WmtcmdBlitCopyFromBufferToTextureWithblitoption {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE_WITH_BLIT_OPTION,
                        reserved: [0; 3],
                        next: null_mem_ptr(),
                        src: temp_buffer,
                        src_offset: temp_buffer_offset,
                        bytes_per_row,
                        bytes_per_image: 0,
                        size: src_size,
                        dst: dst_tex.current().map_or(0, |a| a.texture()),
                        slice: dst_slice,
                        level: u16::try_from(dst_level).unwrap_or(0),
                        origin: dst_origin,
                        options: dst_options,
                    },
                );
                return;
            }
            encode_blit_cmd(
                alloc,
                WmtcmdBlitCopyFromTextureToTexture {
                    r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_TEXTURE,
                    reserved: [0; 3],
                    next: null_mem_ptr(),
                    src: src_tex.current().map_or(0, |a| a.texture()),
                    src_level,
                    src_slice,
                    src_origin,
                    src_size,
                    dst: dst_tex.current().map_or(0, |a| a.texture()),
                    dst_level,
                    dst_slice,
                    dst_origin,
                },
            );
        }
    } else if dst_loc.r#type == D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT {
        let Some(dst_buf) = dst_res.buffer.as_ref() else {
            return;
        };
        // SAFETY: `dst_loc.r#type == PLACED_FOOTPRINT` selects the `placed_footprint` arm.
        let pf = unsafe { &dst_loc.location.placed_footprint };
        let (dst_hr, dst_format) = query_dxgi(pf.footprint.format);
        if dst_hr < 0 {
            crate::warn!(
                "CopyTextureRegion: unsupported format {}",
                pf.footprint.format
            );
            return;
        }
        if src_loc.r#type == D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX {
            let Some(src_tex) = src_res.texture.as_ref() else {
                return;
            };
            let (src_hr, src_format) = query_dxgi(src_desc.format);
            if src_hr < 0 {
                crate::warn!("CopyTextureRegion: unsupported format {}", src_desc.format);
                return;
            }
            let src_planar_count = src_format.planar_count;
            let block_width = if (dst_format.flag & MTL_DXGI_FORMAT_BC) != 0 {
                4_u32
            } else {
                1_u32
            };
            let dst_depth_pitch = if src_tex.info().r#type() == WMT_TEXTURE_TYPE3D {
                (pf.footprint.height / block_width) * pf.footprint.row_pitch
            } else {
                0
            };
            let offset = pf.offset
                + u64::from(dst_x / block_width) * u64::from(dst_format.bytes_per_texel)
                + u64::from(dst_y / block_width) * u64::from(pf.footprint.row_pitch)
                + u64::from(dst_z) * u64::from(dst_depth_pitch);
            let options = if src_planar_count > 1 {
                if src_planar != 0 {
                    WMT_BLIT_OPTION_STENCIL_FROM_DEPTH_STENCIL
                } else {
                    WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL
                }
            } else {
                WMT_BLIT_OPTION_NONE
            };
            encode_blit_cmd(
                alloc,
                WmtcmdBlitCopyFromTextureToBufferWithblitoption {
                    r#type: WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER_WITH_BLIT_OPTION,
                    reserved: [0; 3],
                    next: null_mem_ptr(),
                    src: src_tex.current().map_or(0, |a| a.texture()),
                    slice: src_slice,
                    level: u16::try_from(src_level).unwrap_or(0),
                    origin: WMTOrigin {
                        x: u64::from(sbox.left),
                        y: u64::from(sbox.top),
                        z: u64::from(sbox.front),
                    },
                    size: WMTSize {
                        width: u64::from(sbox.right - sbox.left),
                        height: u64::from(sbox.bottom - sbox.top),
                        depth: u64::from(sbox.back - sbox.front),
                    },
                    dst: dst_buf.current().map_or(0, |a| a.buffer()),
                    offset,
                    bytes_per_row: pf.footprint.row_pitch,
                    bytes_per_image: dst_depth_pitch,
                    options,
                },
            );
        } else {
            let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::CopyTextureRegion");
        }
    }
}

/// `ID3D12GraphicsCommandList::CopyResource` (`d3d12_command_list.cpp:1150`).
///
/// # Safety
///
/// `this`, `dst_resource`, and `src_resource` are caller-supplied pointers.
pub unsafe fn copy_resource(
    this: *mut c_void,
    dst_resource: *mut c_void,
    src_resource: *mut c_void,
) {
    crate::scope!("list.CopyResource");
    if this.is_null()
        || dst_resource.is_null()
        || src_resource.is_null()
        || ptr::eq(dst_resource, src_resource)
    {
        return;
    }
    // SAFETY: `this` is a valid `List` COM pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.allocator.is_null() {
        return;
    }
    // SAFETY: `list.allocator` is non-null during recording.
    let alloc = unsafe { &mut *list.allocator };
    // SAFETY: `dst_resource` is a valid `Resource` COM pointer.
    let dst = unsafe { &*dst_resource.cast::<Resource>() };
    // SAFETY: `src_resource` is a valid `Resource` COM pointer.
    let src = unsafe { &*src_resource.cast::<Resource>() };
    let dst_desc = dst.desc_scoped();
    let src_desc = src.desc_scoped();
    if dst_desc.dimension != src_desc.dimension {
        return;
    }
    // SAFETY: `dst_resource` is a valid `Resource` COM pointer.
    let dst_key = unsafe { resource_key(dst_resource) };
    pre_blit(alloc, dst_key, false);

    if dst.desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
        let src_handle = src
            .buffer
            .as_ref()
            .and_then(|b| b.current())
            .map_or(0, |a| a.buffer());
        let dst_handle = dst
            .buffer
            .as_ref()
            .and_then(|b| b.current())
            .map_or(0, |a| a.buffer());
        encode_blit_cmd(
            alloc,
            WmtcmdBlitCopyFromBufferToBuffer {
                r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
                reserved: [0; 3],
                next: null_mem_ptr(),
                src: src_handle,
                src_offset: 0,
                dst: dst_handle,
                dst_offset: 0,
                copy_length: src.desc.width,
            },
        );
        return;
    }

    let (Some(dst_tex), Some(src_tex)) = (dst.texture.as_ref(), src.texture.as_ref()) else {
        return;
    };
    if dst_tex.info().pixel_format != src_tex.info().pixel_format {
        let (src_hr, src_format) = query_dxgi(src.desc.format);
        let (dst_hr, dst_format) = query_dxgi(dst.desc.format);
        if src_hr < 0 || dst_hr < 0 {
            return;
        }
        let volume = src.desc.dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D;
        let levels = u32::from(src.desc.mip_levels);
        let slices = if volume {
            1
        } else {
            u32::from(src.desc.depth_or_array_size)
        };
        for slice in 0..slices {
            for level in 0..levels {
                let w = u32::try_from(src.desc.width >> level)
                    .unwrap_or(u32::MAX)
                    .max(1);
                let h = (src.desc.height >> level).max(1);
                let d = if volume {
                    (u32::from(src.desc.depth_or_array_size) >> level).max(1)
                } else {
                    1
                };
                copy_reinterpret(
                    alloc,
                    &CopyReinterpretSide {
                        texture: src_tex,
                        format: &src_format,
                        level,
                        slice,
                        origin: WMTOrigin { x: 0, y: 0, z: 0 },
                    },
                    WMTSize {
                        width: u64::from(w),
                        height: u64::from(h),
                        depth: u64::from(d),
                    },
                    &CopyReinterpretSide {
                        texture: dst_tex,
                        format: &dst_format,
                        level,
                        slice,
                        origin: WMTOrigin { x: 0, y: 0, z: 0 },
                    },
                );
            }
        }
        return;
    }

    encode_blit_cmd(
        alloc,
        WmtcmdBlitCopyTexture {
            r#type: WMT_BLIT_COMMAND_COPY_TEXTURE,
            reserved: [0; 3],
            next: null_mem_ptr(),
            src: src_tex.current().map_or(0, |a| a.texture()),
            dst: dst_tex.current().map_or(0, |a| a.texture()),
        },
    );
}

/// `ID3D12GraphicsCommandList::CopyTiles` (`d3d12_command_list.cpp:1199`).
///
/// # Safety
///
/// `this` and the copy arguments are caller-supplied pointers.
pub unsafe fn copy_tiles(
    _this: *mut c_void,
    _tiled_resource: *mut c_void,
    _tile_region_start_coord: *const c_void,
    _tile_region_size: *const c_void,
    _buffer: *mut c_void,
    _buffer_start_offset_in_bytes: u64,
    _flags: u32,
) {
    crate::scope!("list.CopyTiles");
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::CopyTiles");
}

/// Records a full-attachment MSAA resolve pass (`EncodeResolve`, `d3d12_command_list.cpp:1234`).
pub fn encode_resolve(
    alloc: &mut Allocator,
    src: &Arc<Texture>,
    src_slice: u32,
    dst: &Arc<Texture>,
    dst_level: u32,
    dst_slice: u32,
    format: u32,
) {
    alloc.invalidate_current_pass();
    let resolve = alloc
        .allocate_cpu_heap(
            size_of::<ResolveEncoderData>(),
            align_of::<ResolveEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<ResolveEncoderData>();
    let mut base = EncoderData::new(EncoderType::Resolve, 0);
    base.barriers = alloc.barriers;
    base.barriers_last = alloc.barriers;
    base.writes_unknown = false;
    let src_key = TextureViewDescriptor {
        format: original_format(format),
        texture_type: src.info().r#type(),
        first_mip: 0,
        mip_count: 1,
        first_slice: src_slice,
        array_size: 1,
    }
    .key();
    let dst_key = TextureViewDescriptor {
        format: original_format(format),
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: dst_level,
        mip_count: 1,
        first_slice: dst_slice,
        array_size: 1,
    }
    .key();
    let src_view = TextureViewRef::from_texture(src, src_key);
    let dst_view = TextureViewRef::from_texture(dst, dst_key);
    // SAFETY: `resolve` points to aligned uninitialized storage from `allocate_cpu_heap`.
    unsafe {
        ptr::write(
            resolve,
            ResolveEncoderData {
                base,
                src: src_view,
                dst: dst_view,
            },
        );
    };
    alloc.encoder_current = resolve.cast::<EncoderData>();
    alloc.writes(Arc::as_ptr(dst).cast::<c_void>());
    alloc.writes(Arc::as_ptr(src).cast::<c_void>());
    alloc.invalidate_current_pass();
}

/// `ID3D12GraphicsCommandList::ResolveSubresource` (`d3d12_command_list.cpp:1208`).
///
/// # Safety
///
/// `this`, `dst_resource`, and `src_resource` are caller-supplied pointers.
pub unsafe fn resolve_subresource(
    this: *mut c_void,
    dst_resource: *mut c_void,
    dst_subresource: u32,
    src_resource: *mut c_void,
    src_subresource: u32,
    format: u32,
) {
    crate::scope!("list.ResolveSubresource");
    if this.is_null() || dst_resource.is_null() || src_resource.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `List` COM pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.allocator.is_null() {
        return;
    }
    // SAFETY: `list.allocator` is non-null during recording.
    let alloc = unsafe { &mut *list.allocator };
    // SAFETY: `dst_resource` is a valid `Resource` COM pointer.
    let dst = unsafe { &*dst_resource.cast::<Resource>() };
    // SAFETY: `src_resource` is a valid `Resource` COM pointer.
    let src = unsafe { &*src_resource.cast::<Resource>() };
    let (Some(dst_tex), Some(src_tex)) = (dst.texture.as_ref(), src.texture.as_ref()) else {
        return;
    };
    let (hr, format_desc) = query_dxgi(format);
    if hr < 0 {
        crate::err!("ResolveSubresource: invalid format {format}");
        return;
    }
    let dst_mips = dst_tex.info().mipmap_level_count().max(1);
    encode_resolve(
        alloc,
        src_tex,
        src_subresource,
        dst_tex,
        dst_subresource % dst_mips,
        dst_subresource / dst_mips,
        format_desc.pixel_format,
    );
}

/// `ID3D12GraphicsCommandList::BeginQuery` (`d3d12_command_list.cpp:1848`).
///
/// # Safety
///
/// `this` and `query_heap` are caller-supplied pointers.
pub unsafe fn begin_query(this: *mut c_void, query_heap: *mut c_void, query_type: u32, index: u32) {
    crate::scope!("list.BeginQuery");
    if this.is_null()
        || query_heap.is_null()
        || (query_type != D3D12_QUERY_TYPE_OCCLUSION
            && query_type != D3D12_QUERY_TYPE_BINARY_OCCLUSION)
    {
        return;
    }
    // SAFETY: `this` is a valid `List` COM pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    // SAFETY: `query_heap` is a valid `QueryHeap` COM pointer.
    let heap = unsafe { &*query_heap.cast::<QueryHeap>() };
    list.render_pass_visibility_buffer = heap.results.as_ref().map_or(0, Obj::handle);
    let binary = u64::from(query_type == D3D12_QUERY_TYPE_BINARY_OCCLUSION);
    list.active_visibility_offset = (u64::from(index) * 8) | binary;
    list.render_pass.use_visibility_result = query_type == D3D12_QUERY_TYPE_BINARY_OCCLUSION;
    list.dirty_state.insert(DirtyFlags::VISIBILITY_BUFFER);
}

/// Records a timestamp sample at an encoder boundary (`EndTimestamp`).
///
/// Mirrors `EndTimestamp` (`d3d12_command_list.cpp:1864`).
fn end_timestamp(list: &mut List, heap: &QueryHeap, index: u32) {
    let chunk = (index / TIMESTAMPS_PER_BUFFER) as usize;
    if heap.counters.is_empty() || chunk >= heap.counters.len() || list.allocator.is_null() {
        return;
    }
    let buffer = heap.counters[chunk].handle();
    // SAFETY: `list.allocator` is non-null during recording.
    let alloc = unsafe { &mut *list.allocator };
    let mut current = alloc.encoder_current;
    let mut render = if current.is_null() {
        false
    } else {
        // SAFETY: `current` is non-null and points to an active encoder header.
        unsafe { (*current).kind == EncoderType::Render }
    };
    {
        let mut aliases = lock(&heap.aliases);
        if let Some(slot) = aliases.get_mut(index as usize) {
            *slot = u32::MAX;
        }
        if render {
            // SAFETY: `current` is non-null and points to an active `RenderEncoderData`.
            let num_samples = unsafe { usize::from((*current).num_samples) };
            for i in 0..num_samples {
                // SAFETY: `i < num_samples <= 4`.
                let sample = unsafe { &(*current).samples[i] };
                if sample.buffer == buffer {
                    if let Some(slot) = aliases.get_mut(index as usize) {
                        *slot =
                            (index / TIMESTAMPS_PER_BUFFER) * TIMESTAMPS_PER_BUFFER + sample.index;
                    }
                    drop(aliases);
                    return;
                }
            }
        }
    }
    let needs_dummy_blit = if render {
        false
    } else if current.is_null() {
        true
    } else {
        // SAFETY: `current` is non-null and points to an active encoder header.
        let kind = unsafe { (*current).kind };
        kind != EncoderType::Blit && kind != EncoderType::Compute
    };
    if needs_dummy_blit {
        pre_blit(alloc, ptr::null(), false);
        current = alloc.encoder_current;
        encode_blit_cmd(
            alloc,
            WmtcmdBlitFillbuffer {
                r#type: WMT_BLIT_COMMAND_FILL_BUFFER,
                reserved: [0; 3],
                next: null_mem_ptr(),
                buffer: heap.results.as_ref().map_or(0, Obj::handle),
                offset: u64::from(index) * 8,
                length: 4,
                value: 0,
            },
        );
        // SAFETY: `current` is the newly opened `BlitEncoderData`.
        unsafe { (*current).timestamp_only = true };
    }
    // SAFETY: `current` is non-null after `needs_dummy_blit` handling.
    let cur_ref = unsafe { &*current };
    let full = usize::from(cur_ref.num_samples) == cur_ref.samples.len();
    if full {
        alloc.invalidate_current_pass();
        pre_blit(alloc, ptr::null(), false);
        current = alloc.encoder_current;
        render = false;
    }
    let value_ptr = heap
        .values
        .get(index as usize)
        .map_or(ptr::null_mut(), |v| ptr::from_ref(v).cast_mut());
    // SAFETY: `current` has `num_samples < 4`.
    let sample_idx = unsafe { usize::from((*current).num_samples) };
    // SAFETY: `current` has `sample_idx < 4`.
    unsafe {
        (*current).samples[sample_idx] = EncoderSample {
            buffer,
            index: index % TIMESTAMPS_PER_BUFFER,
            value: value_ptr,
        }
    };
    // SAFETY: `current` is non-null.
    unsafe { (*current).num_samples += 1 };
    list.submission.timestamps = true;
    if !render {
        alloc.invalidate_current_pass();
    }
}

/// `ID3D12GraphicsCommandList::EndQuery` (`d3d12_command_list.cpp:1904`).
///
/// # Safety
///
/// `this` and `query_heap` are caller-supplied pointers.
pub unsafe fn end_query(this: *mut c_void, query_heap: *mut c_void, query_type: u32, index: u32) {
    crate::scope!("list.EndQuery");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `List` COM pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if query_type == D3D12_QUERY_TYPE_TIMESTAMP && !query_heap.is_null() {
        // SAFETY: `query_heap` is a valid `QueryHeap` COM pointer.
        let heap = unsafe { &*query_heap.cast::<QueryHeap>() };
        end_timestamp(list, heap, index);
        return;
    }
    if query_type != D3D12_QUERY_TYPE_OCCLUSION && query_type != D3D12_QUERY_TYPE_BINARY_OCCLUSION {
        return;
    }
    list.active_visibility_offset = u64::MAX;
    list.render_pass_visibility_buffer = 0;
    list.dirty_state.insert(DirtyFlags::VISIBILITY_BUFFER);
}

/// `ID3D12GraphicsCommandList::ResolveQueryData` (`d3d12_command_list.cpp:1915`).
///
/// # Safety
///
/// `this`, `query_heap`, and `destination_buffer` are caller-supplied pointers.
pub unsafe fn resolve_query_data(
    this: *mut c_void,
    query_heap: *mut c_void,
    query_type: u32,
    start_index: u32,
    num_queries: u32,
    destination_buffer: *mut c_void,
    aligned_destination_buffer_offset: u64,
) {
    crate::scope!("list.ResolveQueryData");
    if this.is_null() || query_heap.is_null() || destination_buffer.is_null() || num_queries == 0 {
        return;
    }
    // SAFETY: `this` is a valid `List` COM pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.allocator.is_null() {
        return;
    }
    // SAFETY: `query_heap` is a valid `QueryHeap` COM pointer.
    let heap = unsafe { &*query_heap.cast::<QueryHeap>() };
    // SAFETY: `destination_buffer` is a valid `Resource` COM pointer.
    let dst_res = unsafe { &*destination_buffer.cast::<Resource>() };
    let dst_heap = &dst_res.heap_props;
    let dst_readable = dst_heap.r#type == D3D12_HEAP_TYPE_READBACK
        || (dst_heap.r#type == D3D12_HEAP_TYPE_CUSTOM
            && dst_heap.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE);
    let dst_memory = dst_res
        .buffer
        .as_ref()
        .and_then(|b| b.current())
        .map_or(ptr::null_mut(), |a| a.mapped_memory(0));
    if query_type == D3D12_QUERY_TYPE_TIMESTAMP
        && !heap.counters.is_empty()
        && !dst_memory.is_null()
        && dst_readable
    {
        let aliases_guard = lock(&heap.aliases);
        let mut i = start_index;
        let end = start_index.saturating_add(num_queries);
        while i < end {
            let chunk = i / TIMESTAMPS_PER_BUFFER;
            let first = i % TIMESTAMPS_PER_BUFFER;
            let n = (TIMESTAMPS_PER_BUFFER - first).min(end - i);
            let Some(counter) = heap.counters.get(chunk as usize) else {
                break;
            };
            let byte_off = aligned_destination_buffer_offset + u64::from(i - start_index) * 8;
            // SAFETY: `dst_memory` points to the mapped destination buffer and `byte_off` is within range.
            let dst_ptr = unsafe { dst_memory.add(usize::try_from(byte_off).unwrap_or(0)) };
            let mut job_aliases = Vec::new();
            for q in i..(i + n) {
                if let Some(&alias) = aliases_guard.get(q as usize)
                    && alias != u32::MAX
                {
                    job_aliases.push((q - i, alias % TIMESTAMPS_PER_BUFFER));
                }
            }
            let values = heap
                .values
                .get((chunk * TIMESTAMPS_PER_BUFFER) as usize)
                .map_or(ptr::null(), ptr::from_ref);
            list.submission.timestamp_resolves.push(TimestampResolve {
                samples: counter.handle(),
                start: first,
                count: n,
                dst: dst_ptr.cast::<c_void>(),
                aliases: job_aliases,
                values,
            });
            list.submission.custom_resolves |= dst_heap.r#type == D3D12_HEAP_TYPE_CUSTOM;
            i += n;
        }
        return;
    }

    // SAFETY: `list.allocator` is non-null during recording.
    let alloc = unsafe { &mut *list.allocator };
    alloc.barrier(true, &[]);
    // SAFETY: `destination_buffer` is a valid `Resource` COM pointer.
    let dst_key = unsafe { resource_key(destination_buffer) };
    pre_blit(alloc, dst_key, query_type == D3D12_QUERY_TYPE_TIMESTAMP);
    let results_handle = heap.results.as_ref().map_or(0, Obj::handle);
    alloc.writes(ptr::with_exposed_provenance(
        usize::try_from(results_handle).unwrap_or(0),
    ));
    let src_offset = u64::from(start_index) * u64::from(heap.stride);
    let copy_length = u64::from(num_queries) * u64::from(heap.stride);
    let dst_handle = dst_res
        .buffer
        .as_ref()
        .and_then(|b| b.current())
        .map_or(0, |a| a.buffer());
    encode_blit_cmd(
        alloc,
        WmtcmdBlitCopyFromBufferToBuffer {
            r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
            reserved: [0; 3],
            next: null_mem_ptr(),
            src: results_handle,
            src_offset,
            dst: dst_handle,
            dst_offset: aligned_destination_buffer_offset,
            copy_length,
        },
    );
    if query_type != D3D12_QUERY_TYPE_OCCLUSION && query_type != D3D12_QUERY_TYPE_BINARY_OCCLUSION {
        return;
    }
    encode_blit_cmd(
        alloc,
        WmtcmdBlitFillbuffer {
            r#type: WMT_BLIT_COMMAND_FILL_BUFFER,
            reserved: [0; 3],
            next: null_mem_ptr(),
            buffer: results_handle,
            offset: src_offset,
            length: copy_length,
            value: 0,
        },
    );
}

/// `ID3D12GraphicsCommandList1::AtomicCopyBufferUINT` (`d3d12_command_list.cpp:2096`).
///
/// # Safety
///
/// `this` and the copy arguments are caller-supplied pointers.
pub unsafe extern "system-unwind" fn atomic_copy_buffer_uint(
    _this: *mut c_void,
    _dst_buffer: *mut c_void,
    _dst_offset: u64,
    _src_buffer: *mut c_void,
    _src_offset: u64,
    _dependencies: u32,
    _dependent_resources: *const *mut c_void,
    _dependent_subresource_ranges: *const c_void,
) {
    crate::scope!("list.AtomicCopyBufferUINT");
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::AtomicCopyBufferUINT");
}

/// `ID3D12GraphicsCommandList1::AtomicCopyBufferUINT64` (`d3d12_command_list.cpp:2105`).
///
/// # Safety
///
/// `this` and the copy arguments are caller-supplied pointers.
pub unsafe extern "system-unwind" fn atomic_copy_buffer_uint64(
    _this: *mut c_void,
    _dst_buffer: *mut c_void,
    _dst_offset: u64,
    _src_buffer: *mut c_void,
    _src_offset: u64,
    _dependencies: u32,
    _dependent_resources: *const *mut c_void,
    _dependent_subresource_ranges: *const c_void,
) {
    crate::scope!("list.AtomicCopyBufferUINT64");
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::AtomicCopyBufferUINT64");
}

/// Arguments for [`prepare_pending_resolve`].
pub struct ResolveRegionDesc {
    pub dst_resource: *mut c_void,
    pub dst_subresource: u32,
    pub dst_x: u32,
    pub dst_y: u32,
    pub src_resource: *mut c_void,
    pub src_subresource: u32,
    pub src_rect: *const Rect,
    pub format: u32,
    pub resolve_mode: u32,
}

/// Validates and builds a [`PendingResolve`] for a subresource region resolve.
///
/// Mirrors `ResolveSubresourceRegion` validation (`d3d12_command_list.cpp:2126-2177`).
///
/// # Safety
///
/// `desc.dst_resource`, `desc.src_resource`, and `desc.src_rect` must be null or valid pointers.
#[must_use]
pub unsafe fn prepare_pending_resolve(desc: &ResolveRegionDesc) -> Option<PendingResolve> {
    let ResolveRegionDesc {
        dst_resource,
        dst_subresource,
        dst_x,
        dst_y,
        src_resource,
        src_subresource,
        src_rect,
        format,
        resolve_mode,
    } = *desc;
    if resolve_mode == D3D12_RESOLVE_MODE_DECOMPRESS
        || resolve_mode == D3D12_RESOLVE_MODE_ENCODE_SAMPLER_FEEDBACK
        || resolve_mode == D3D12_RESOLVE_MODE_DECODE_SAMPLER_FEEDBACK
    {
        return None;
    }
    if dst_resource.is_null() || src_resource.is_null() {
        return None;
    }
    // SAFETY: `dst_resource` is a valid `Resource` COM pointer.
    let dst_res = unsafe { &*dst_resource.cast::<Resource>() };
    // SAFETY: `src_resource` is a valid `Resource` COM pointer.
    let src_res = unsafe { &*src_resource.cast::<Resource>() };
    let (Some(dst_tex), Some(src_tex)) = (dst_res.texture.as_ref(), src_res.texture.as_ref())
    else {
        return None;
    };
    let (hr, format_desc) = query_dxgi(format);
    if hr < 0 {
        crate::err!("ResolveSubresourceRegion: invalid format {format}");
        return None;
    }
    if resolve_mode != D3D12_RESOLVE_MODE_AVERAGE
        || depth_stencil_planar_flags(format_desc.pixel_format) != 0
    {
        static WARNED: AtomicBool = AtomicBool::new(false);
        if !WARNED.swap(true, Ordering::Relaxed) {
            crate::warn!(
                "ResolveSubresourceRegion: mode {resolve_mode} of format {format} not supported, skipped"
            );
        }
        return None;
    }
    let width = i32::try_from(src_tex.info().width).unwrap_or(i32::MAX);
    let height = i32::try_from(src_tex.info().height).unwrap_or(i32::MAX);
    let dst_mips = dst_tex.info().mipmap_level_count().max(1);
    let dst_level = dst_subresource % dst_mips;
    let dst_slice = dst_subresource / dst_mips;
    let dst_width = i32::try_from((dst_tex.info().width >> dst_level).max(1)).unwrap_or(i32::MAX);
    let dst_height = i32::try_from((dst_tex.info().height >> dst_level).max(1)).unwrap_or(i32::MAX);
    let offset_col = i32::try_from(dst_x).unwrap_or(i32::MAX);
    let offset_row = i32::try_from(dst_y).unwrap_or(i32::MAX);
    if offset_col >= dst_width || offset_row >= dst_height {
        return None;
    }
    let mut rect = if src_rect.is_null() {
        Rect {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }
    } else {
        // SAFETY: `src_rect` is non-null and points to a readable `Rect`.
        unsafe { ptr::read_unaligned(src_rect) }
    };
    rect.left = rect.left.max(0);
    rect.top = rect.top.max(0);
    rect.right = rect
        .right
        .min(width)
        .min(rect.left.saturating_add(dst_width - offset_col));
    rect.bottom = rect
        .bottom
        .min(height)
        .min(rect.top.saturating_add(dst_height - offset_row));
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return None;
    }
    let same_size = dst_x == 0
        && dst_y == 0
        && rect.left == 0
        && rect.top == 0
        && rect.right == width
        && rect.bottom == height
        && dst_width == width
        && dst_height == height;
    if !same_size {
        let (dst_hr, _) = query_dxgi(dst_res.desc_scoped().format);
        if dst_hr < 0 {
            crate::err!(
                "ResolveSubresourceRegion: invalid destination format {}",
                dst_res.desc_scoped().format
            );
            return None;
        }
    }
    let src_alloc = src_tex.current()?;
    let dst_alloc = dst_tex.current()?;
    let src_key = TextureViewDescriptor {
        format,
        texture_type: src_tex.info().r#type(),
        first_mip: 0,
        mip_count: 1,
        first_slice: src_subresource,
        array_size: 1,
    }
    .key();
    let dst_key = TextureViewDescriptor {
        format: dst_res.desc.format,
        texture_type: WMT_TEXTURE_TYPE2D,
        first_mip: dst_level,
        mip_count: 1,
        first_slice: dst_slice,
        array_size: 1,
    }
    .key();
    Some(PendingResolve {
        src_view: Some(TextureViewRef {
            descriptor: Arc::clone(src_tex),
            allocation: src_alloc,
            key: src_key,
            texture: 0,
            gpu_resource_id: 0,
        }),
        src_x: u32::try_from(rect.left).unwrap_or(0),
        src_y: u32::try_from(rect.top).unwrap_or(0),
        src_w: u32::try_from(rect.right - rect.left).unwrap_or(0),
        src_h: u32::try_from(rect.bottom - rect.top).unwrap_or(0),
        dst_view: Some(TextureViewRef {
            descriptor: Arc::clone(dst_tex),
            allocation: dst_alloc,
            key: dst_key,
            texture: 0,
            gpu_resource_id: 0,
        }),
        dst_x,
        dst_y,
        same_size,
    })
}

/// Executes a [`PendingResolve`] on `list` (`d3d12_command_list.cpp:2168-2204`).
pub fn execute_pending_resolve(list: &mut List, pending: &PendingResolve) {
    if list.allocator.is_null() {
        return;
    }
    let (Some(src_view), Some(dst_view)) = (pending.src_view.as_ref(), pending.dst_view.as_ref())
    else {
        return;
    };
    let (hr, format_desc) = query_dxgi(src_view.key.format());
    if hr < 0 {
        return;
    }
    let src_slice = src_view.key.first_slice();
    let dst_level = dst_view.key.first_mip();
    let dst_slice = dst_view.key.first_slice();
    // SAFETY: `list.allocator` is non-null during recording.
    let alloc = unsafe { &mut *list.allocator };
    if pending.same_size {
        encode_resolve(
            alloc,
            &src_view.descriptor,
            src_slice,
            &dst_view.descriptor,
            dst_level,
            dst_slice,
            format_desc.pixel_format,
        );
        return;
    }
    let (dst_hr, dst_format) = query_dxgi(dst_view.key.format());
    if dst_hr < 0 || list.device.is_null() {
        return;
    }
    // SAFETY: `list.device` is non-null and points to the owning `Device`.
    let device = unsafe { &*list.device };
    let Some(mtl_device) = device.mtl.as_ref() else {
        return;
    };
    let usage = WMT_TEXTURE_USAGE_RENDER_TARGET
        | WMT_TEXTURE_USAGE_SHADER_READ
        | WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW;
    let info = WMTTextureInfo {
        pixel_format: original_format(format_desc.pixel_format),
        width: src_view.descriptor.info().width,
        height: src_view.descriptor.info().height,
        depth: 1,
        array_length: 1,
        bits: (WMT_TEXTURE_TYPE2D << WMTTextureInfo::TYPE_SHIFT)
            | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
            | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
            | (usage << WMTTextureInfo::USAGE_SHIFT),
        options: WMT_RESOURCE_STORAGE_MODE_PRIVATE | WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let scratch = Arc::new(Texture::new(info, mtl_device));
    scratch.rename(scratch.allocate(TextureAllocationFlags::GPU_PRIVATE));
    encode_resolve(
        alloc,
        &src_view.descriptor,
        src_slice,
        &scratch,
        0,
        0,
        format_desc.pixel_format,
    );
    pre_blit(
        alloc,
        Arc::as_ptr(&dst_view.descriptor).cast::<c_void>(),
        false,
    );
    alloc.writes(Arc::as_ptr(&scratch).cast::<c_void>());
    copy_reinterpret(
        alloc,
        &CopyReinterpretSide {
            texture: &scratch,
            format: &format_desc,
            level: 0,
            slice: 0,
            origin: WMTOrigin {
                x: u64::from(pending.src_x),
                y: u64::from(pending.src_y),
                z: 0,
            },
        },
        WMTSize {
            width: u64::from(pending.src_w),
            height: u64::from(pending.src_h),
            depth: 1,
        },
        &CopyReinterpretSide {
            texture: &dst_view.descriptor,
            format: &dst_format,
            level: dst_level,
            slice: dst_slice,
            origin: WMTOrigin {
                x: u64::from(pending.dst_x),
                y: u64::from(pending.dst_y),
                z: 0,
            },
        },
    );
    alloc.keep_texture(scratch);
}

/// `ID3D12GraphicsCommandList1::ResolveSubresourceRegion` (`d3d12_command_list.cpp:2126`).
///
/// # Safety
///
/// `this`, `dst_resource`, `src_resource`, and `src_rect` are caller-supplied pointers.
pub unsafe extern "system-unwind" fn resolve_subresource_region(
    this: *mut c_void,
    dst_resource: *mut c_void,
    dst_subresource: u32,
    dst_x: u32,
    dst_y: u32,
    src_resource: *mut c_void,
    src_subresource: u32,
    src_rect: *mut c_void,
    format: u32,
    resolve_mode: u32,
) {
    crate::scope!("list.ResolveSubresourceRegion");
    if this.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `List` COM pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    let desc = ResolveRegionDesc {
        dst_resource,
        dst_subresource,
        dst_x,
        dst_y,
        src_resource,
        src_subresource,
        src_rect: src_rect.cast::<Rect>(),
        format,
        resolve_mode,
    };
    // SAFETY: caller forwards `ResolveSubresourceRegion` pointers.
    let pending = unsafe { prepare_pending_resolve(&desc) };
    if let Some(pending) = pending {
        execute_pending_resolve(list, &pending);
    }
}

/// `ID3D12GraphicsCommandList2::WriteBufferImmediate` (`d3d12_command_list.cpp:2521`).
///
/// # Safety
///
/// `this`, `params`, and `modes` are caller-supplied pointers.
pub unsafe fn write_buffer_immediate(
    _this: *mut c_void,
    _count: u32,
    _params: *const c_void,
    _modes: *const u32,
) {
    crate::scope!("list.WriteBufferImmediate");
    let (): () = crate::not_implemented!("ID3D12GraphicsCommandList::WriteBufferImmediate");
}

#[cfg(test)]
mod tests;
