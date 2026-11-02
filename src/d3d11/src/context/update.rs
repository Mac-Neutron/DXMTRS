//! Context subresource uploads (`UpdateSubresource`, `UpdateSubresource1`).

use core::{ffi::c_void, slice};
use std::sync::Arc;

use dxmt_core::{deptrack::Access, encoder::NO_NEXT, resource::Buffer};
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
    WmtcmdBlitCopyFromBufferToBuffer, WmtcmdBlitCopyFromBufferToTexture,
};

#[cfg(test)]
use super::copy::{
    has_test_dynamic_buffer, record_blit_b2b, record_blit_b2t, update_test_dynamic_buffer,
    write_test_staging_slice,
};
use super::{
    Context, Mode,
    copy::{
        BlitObject, BlitObjectDesc, D3D11_RESOURCE_DIMENSION_BUFFER, TextureUpdateCommand,
        get_buffer_ref, get_staging_ref, get_texture_ref, unimplemented_abort, wmt_origin,
        wmt_size,
    },
};
use crate::abi::records::D3d11Box;

/// `D3D11_COPY_NO_OVERWRITE`.
const D3D11_COPY_NO_OVERWRITE: u32 = 1;
/// `D3D11_COPY_DISCARD`.
const D3D11_COPY_DISCARD: u32 = 2;

/// Copies `data` into the Metal staging buffer `handle` at `offset`.
fn update_staging_contents(handle: u64, offset: u64, data: &[u8]) {
    #[cfg(test)]
    write_test_staging_slice(handle, offset, data);
    #[cfg(not(test))]
    if handle != 0 && !data.is_empty() {
        #[cfg(target_pointer_width = "64")]
        let mem = winemetal_sys::wire::MemPtr {
            ptr: data.as_ptr().expose_provenance() as u64,
        };
        #[cfg(target_pointer_width = "32")]
        let mem = winemetal_sys::wire::MemPtr {
            ptr: u32::try_from(data.as_ptr().expose_provenance()).unwrap_or(0),
            high: 0,
        };
        // SAFETY: `handle` is a live `MTLBuffer` from `M::staging` with room for `data.len()`.
        unsafe {
            winemetal_sys::MTLBuffer_updateContents(handle, offset, mem, data.len() as u64);
        }
    }
}

impl<M: Mode> Context<M> {
    pub fn update_subresource(
        &mut self,
        dst_resource: *mut c_void,
        dst_subresource: u32,
        dst_box: *const c_void,
        src_data: *const c_void,
        src_row_pitch: u32,
        src_depth_pitch: u32,
    ) {
        self.update_subresource1(
            dst_resource,
            dst_subresource,
            dst_box,
            src_data,
            src_row_pitch,
            src_depth_pitch,
            0,
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "matches ID3D11DeviceContext1::UpdateSubresource1"
    )]
    pub fn update_subresource1(
        &mut self,
        dst_resource: *mut c_void,
        dst_subresource: u32,
        dst_box: *const c_void,
        src_data: *const c_void,
        src_row_pitch: u32,
        src_depth_pitch: u32,
        copy_flags: u32,
    ) {
        if dst_resource.is_null() || src_data.is_null() {
            return;
        }
        let Some(dst_obj) = BlitObject::new(self.device, dst_resource) else {
            return;
        };

        if dst_obj.dimension == D3D11_RESOURCE_DIMENSION_BUFFER {
            let BlitObjectDesc::Buffer(desc) = &dst_obj.desc else {
                return;
            };
            let (copy_offset, copy_len) = if dst_box.is_null() {
                (0_u32, desc.byte_width)
            } else {
                // SAFETY: caller supplied a non-null `D3D11_BOX` pointer.
                let b = unsafe { *dst_box.cast::<D3d11Box>() };
                if b.right <= b.left {
                    return;
                }
                (b.left, b.right - b.left)
            };
            if copy_len == 0 {
                return;
            }
            // SAFETY: `src_data` is valid for `copy_len` bytes during `UpdateSubresource1`.
            let src_slice =
                unsafe { slice::from_raw_parts(src_data.cast::<u8>(), copy_len as usize) };

            let is_discard = (copy_flags & D3D11_COPY_DISCARD) != 0
                || (copy_len == desc.byte_width && copy_offset == 0);
            let is_no_overwrite = (copy_flags & D3D11_COPY_NO_OVERWRITE) != 0;

            #[cfg(test)]
            if has_test_dynamic_buffer(dst_resource) && (is_discard || is_no_overwrite) {
                update_test_dynamic_buffer(
                    dst_resource,
                    is_discard,
                    copy_offset as usize,
                    src_slice,
                );
                return;
            }

            if let Some(dynamic) = crate::resource::dynamic_buffer(dst_resource) {
                if is_discard {
                    if dynamic.next_suballocation() == 0 {
                        let new_alloc = dynamic.allocate(0);
                        dynamic.update_immediate_name(0, new_alloc, 0, false);
                    }
                    let alloc = M::dynamic_name(self, dynamic);
                    let sub = alloc.current_suballocation();
                    alloc.update_contents(u64::from(copy_offset), src_slice, sub);
                    return;
                }
                if is_no_overwrite {
                    let alloc = M::dynamic_name(self, dynamic);
                    let sub = alloc.current_suballocation();
                    alloc.update_contents(u64::from(copy_offset), src_slice, sub);
                    return;
                }
            }

            if get_staging_ref(dst_resource, dst_subresource).is_some() {
                unimplemented_abort("update buffer: staging");
                return;
            }

            if let Some(dst_buf) = get_buffer_ref(dst_resource) {
                let (staging_buffer, offset) = M::staging(self, u64::from(copy_len), 16);
                update_staging_contents(staging_buffer, offset, src_slice);
                self.switch_to_blit_encoder();
                #[cfg(test)]
                if let Some((dst_h, dst_off)) = dst_buf.test_handle() {
                    record_blit_b2b(
                        staging_buffer,
                        offset,
                        dst_h,
                        u64::from(copy_offset) + dst_off,
                        u64::from(copy_len),
                    );
                }
                // C++ closure #15 (`UpdateSubresource1` buffer, d3d11_context_impl.cpp:1255).
                M::emit_op(self, move |enc| {
                    let (dst_h, dst_off) =
                        dst_buf.access(enc, copy_offset, copy_len, Access::WRITE);
                    let cmd = WmtcmdBlitCopyFromBufferToBuffer {
                        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        src: staging_buffer,
                        src_offset: offset,
                        dst: dst_h,
                        dst_offset: u64::from(copy_offset) + dst_off,
                        copy_length: u64::from(copy_len),
                    };
                    enc.encode_blit_command(cmd);
                });
            }
            return;
        }

        let Some(cmd) =
            TextureUpdateCommand::new(&dst_obj, dst_subresource, dst_box.cast::<D3d11Box>())
        else {
            return;
        };
        self.update_texture(&cmd, src_data, src_row_pitch, src_depth_pitch, copy_flags);
    }

    fn update_texture(
        &mut self,
        cmd: &TextureUpdateCommand,
        src_data: *const c_void,
        src_row_pitch: u32,
        src_depth_pitch: u32,
        _copy_flags: u32,
    ) {
        let dst_mip = cmd.dst.mip_level;
        let dst_slice = cmd.dst.array_slice;
        let dst_origin = cmd.dst_origin;
        let dst_size = cmd.dst_size;
        let effective_bpr = cmd.effective_bytes_per_row;
        let effective_rows = cmd.effective_rows;

        if let Some(dst_tex) = get_texture_ref(cmd.p_dst) {
            let bytes_per_depth_slice =
                u64::from(effective_rows).saturating_mul(u64::from(effective_bpr));
            let total_bytes = bytes_per_depth_slice.saturating_mul(u64::from(dst_size[2]));
            let (staging_buffer, offset) = M::staging(self, total_bytes, 16);
            let src_base = src_data.cast::<u8>();

            if effective_bpr == src_row_pitch {
                let slice_len = usize::try_from(bytes_per_depth_slice).unwrap_or(0);
                for depth_slice in 0..dst_size[2] {
                    let dst_offset = offset + u64::from(depth_slice) * bytes_per_depth_slice;
                    let src_byte_off =
                        (depth_slice as usize).saturating_mul(src_depth_pitch as usize);
                    // SAFETY: `src_base` is valid for `depth_slice * src_depth_pitch + slice_len`.
                    let src_ptr = unsafe { src_base.add(src_byte_off) };
                    // SAFETY: `src_ptr` points to `slice_len` readable bytes from `src_data`.
                    let data = unsafe { slice::from_raw_parts(src_ptr, slice_len) };
                    update_staging_contents(staging_buffer, dst_offset, data);
                }
            } else {
                let row_len = effective_bpr as usize;
                for depth_slice in 0..dst_size[2] {
                    for row in 0..effective_rows {
                        let dst_offset = offset
                            + u64::from(row) * u64::from(effective_bpr)
                            + u64::from(depth_slice) * bytes_per_depth_slice;
                        let src_byte_off = (row as usize)
                            .saturating_mul(src_row_pitch as usize)
                            .saturating_add(
                                (depth_slice as usize).saturating_mul(src_depth_pitch as usize),
                            );
                        // SAFETY: `src_base` is valid for the pitched row inside `src_data`.
                        let src_ptr = unsafe { src_base.add(src_byte_off) };
                        // SAFETY: `src_ptr` points to `row_len` readable bytes.
                        let data = unsafe { slice::from_raw_parts(src_ptr, row_len) };
                        update_staging_contents(staging_buffer, dst_offset, data);
                    }
                }
            }

            let bytes_per_depth_slice_u32 = effective_rows.saturating_mul(effective_bpr);
            self.switch_to_blit_encoder();
            #[cfg(test)]
            if let Some(dst_h) = dst_tex.test_handle() {
                record_blit_b2t(&WmtcmdBlitCopyFromBufferToTexture {
                    r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    src: staging_buffer,
                    src_offset: offset,
                    bytes_per_row: effective_bpr,
                    bytes_per_image: bytes_per_depth_slice_u32,
                    size: wmt_size(dst_size),
                    dst: dst_h,
                    slice: dst_slice,
                    level: dst_mip,
                    origin: wmt_origin(dst_origin),
                });
            }
            // C++ closure #16 (`UpdateTexture` from CPU memory, d3d11_context_impl.cpp:4240).
            M::emit_op(self, move |enc| {
                let texture = dst_tex.access(enc, dst_mip, dst_slice, Access::WRITE);
                let cmd_cptex = WmtcmdBlitCopyFromBufferToTexture {
                    r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    src: staging_buffer,
                    src_offset: offset,
                    bytes_per_row: effective_bpr,
                    bytes_per_image: bytes_per_depth_slice_u32,
                    size: wmt_size(dst_size),
                    dst: texture,
                    slice: dst_slice,
                    level: dst_mip,
                    origin: wmt_origin(dst_origin),
                };
                enc.encode_blit_command(cmd_cptex);
            });
        } else if get_staging_ref(cmd.p_dst, cmd.dst_subresource).is_some() {
            unimplemented_abort("update texture: staging");
        }
    }

    /// Uploads a texture subresource from an existing `Buffer` (`UpdateTexture` overload).
    pub fn update_texture_from_buffer(
        &mut self,
        dst_resource: *mut c_void,
        dst_subresource: u32,
        dst_box: *const D3d11Box,
        src: Arc<Buffer>,
        src_row_pitch: u32,
        src_depth_pitch: u32,
    ) {
        let Some(dst_obj) = BlitObject::new(self.device, dst_resource) else {
            return;
        };
        let Some(cmd) = TextureUpdateCommand::new(&dst_obj, dst_subresource, dst_box) else {
            return;
        };
        let Some(dst_tex) = get_texture_ref(cmd.p_dst) else {
            return;
        };
        let dst_mip = cmd.dst.mip_level;
        let dst_slice = cmd.dst.array_slice;
        let dst_origin = cmd.dst_origin;
        let dst_size = cmd.dst_size;
        self.switch_to_blit_encoder();
        let src_len = u32::try_from(src.length()).unwrap_or(u32::MAX);
        // C++ closure #17 (`UpdateTexture` from Buffer, d3d11_context_impl.cpp:4275).
        M::emit_op(self, move |enc| {
            let (src_h, src_offset) = enc
                .access_buffer_in::<0, 0>(&src, 0, src_len, Access::READ)
                .map_or((0, 0), |(alloc, off)| (alloc.buffer(), off));
            let texture = dst_tex.access(enc, dst_mip, dst_slice, Access::WRITE);
            let cmd_cptex = WmtcmdBlitCopyFromBufferToTexture {
                r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
                reserved: [0; 3],
                next: NO_NEXT,
                src: src_h,
                src_offset,
                bytes_per_row: src_row_pitch,
                bytes_per_image: src_depth_pitch,
                size: wmt_size(dst_size),
                dst: texture,
                slice: dst_slice,
                level: dst_mip,
                origin: wmt_origin(dst_origin),
            };
            enc.encode_blit_command(cmd_cptex);
        });
    }
}
