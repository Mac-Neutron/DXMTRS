//! Context resource mapping (`Map`, `Unmap`).

use core::ffi::c_void;

use dxmt_core::staging::MapResult;

use super::{
    Context, Mode,
    encode::CommandBufferState,
    immediate::{push_encoder_trace, resource_common},
    state::DirtyFlags,
};
use crate::{
    abi::records::D3d11MappedSubresource,
    com::{E_INVALIDARG, E_NOTIMPL, Hresult, S_OK},
};

/// `D3D11_MAP_READ`.
pub const D3D11_MAP_READ: u32 = 1;
/// `D3D11_MAP_WRITE`.
pub const D3D11_MAP_WRITE: u32 = 2;
/// `D3D11_MAP_READ_WRITE`.
pub const D3D11_MAP_READ_WRITE: u32 = 3;
/// `D3D11_MAP_WRITE_DISCARD`.
pub const D3D11_MAP_WRITE_DISCARD: u32 = 4;
/// `D3D11_MAP_WRITE_NO_OVERWRITE`.
pub const D3D11_MAP_WRITE_NO_OVERWRITE: u32 = 5;

/// `D3D11_MAP_FLAG_DO_NOT_WAIT`.
pub const D3D11_MAP_FLAG_DO_NOT_WAIT: u32 = 0x0010_0000;

/// `DXGI_ERROR_WAS_STILL_DRAWING`.
pub const DXGI_ERROR_WAS_STILL_DRAWING: Hresult = 0x887A_000A_u32.cast_signed();

const D3D11_BIND_VERTEX_BUFFER: u32 = 0x1;
const D3D11_BIND_INDEX_BUFFER: u32 = 0x2;

impl<M: Mode> Context<M> {
    /// `ID3D11DeviceContext::Map`.
    pub fn map(
        &mut self,
        resource: *mut c_void,
        _subresource: u32,
        map_type: u32,
        map_flags: u32,
        mapped_resource: *mut c_void,
    ) -> Hresult {
        if resource.is_null() || mapped_resource.is_null() {
            return E_INVALIDARG;
        }
        let Some(common) = resource_common(resource) else {
            return E_INVALIDARG;
        };

        if let Some(dyn_buf) = common.dynamic_buffer.as_ref() {
            if !matches!(
                map_type,
                D3D11_MAP_WRITE_DISCARD | D3D11_MAP_WRITE_NO_OVERWRITE
            ) {
                return E_INVALIDARG;
            }
            let bind_flags = common.bind_flags;
            let len = u32::try_from(dyn_buf.buffer().length()).unwrap_or(0);
            if let Some(imm) = self.as_immediate_mut() {
                if map_type == D3D11_MAP_WRITE_DISCARD {
                    let next_sub = dyn_buf.next_suballocation();
                    if next_sub != 0 {
                        dyn_buf.use_immediate_suballocation(next_sub);
                    } else {
                        let coherent = imm.mode.coherent_seq();
                        let seq = imm.mode.current_seq();
                        let alloc = dyn_buf.allocate(coherent);
                        dyn_buf.update_immediate_name(seq, alloc, 0, false);
                    }
                    if (bind_flags & D3D11_BIND_VERTEX_BUFFER) != 0 {
                        imm.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
                        if imm.cmdbuf_state == CommandBufferState::RenderEncoderActive {
                            push_encoder_trace(
                                imm.mode.encoder_mut(),
                                "Render:SetVertexBuffers(slot=0)",
                            );
                        }
                    }
                    if (bind_flags & D3D11_BIND_INDEX_BUFFER) != 0 {
                        imm.dirty.insert(DirtyFlags::INDEX_BUFFER);
                    }
                }
                let out = D3d11MappedSubresource {
                    p_data: dyn_buf.immediate_mapped_memory().cast(),
                    row_pitch: len,
                    depth_pitch: len,
                };
                // SAFETY: `mapped_resource` was checked for null above.
                unsafe {
                    mapped_resource
                        .cast::<D3d11MappedSubresource>()
                        .write_unaligned(out);
                }
                return S_OK;
            } else if let Some(def) = self.as_deferred_mut() {
                let alloc = if map_type == D3D11_MAP_WRITE_DISCARD {
                    if (bind_flags & D3D11_BIND_VERTEX_BUFFER) != 0 {
                        def.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
                        if def.cmdbuf_state == CommandBufferState::RenderEncoderActive {
                            def.mode.commands_mut().emit_fn(|enc| {
                                push_encoder_trace(enc, "Render:SetVertexBuffers(slot=0)");
                            });
                        }
                    }
                    if (bind_flags & D3D11_BIND_INDEX_BUFFER) != 0 {
                        def.dirty.insert(DirtyFlags::INDEX_BUFFER);
                    }
                    def.mode.rotate_dynamic_buffer(dyn_buf)
                } else {
                    def.mode.get_or_alloc_dynamic_buffer(dyn_buf)
                };
                let out = D3d11MappedSubresource {
                    p_data: alloc.mapped_memory(0).cast(),
                    row_pitch: len,
                    depth_pitch: len,
                };
                // SAFETY: `mapped_resource` was checked for null above.
                unsafe {
                    mapped_resource
                        .cast::<D3d11MappedSubresource>()
                        .write_unaligned(out);
                }
                return S_OK;
            }
        }

        if let Some(dyn_tex) = common.dynamic_texture.as_ref() {
            if !matches!(
                map_type,
                D3D11_MAP_WRITE_DISCARD | D3D11_MAP_WRITE_NO_OVERWRITE
            ) {
                return E_INVALIDARG;
            }
            let info = dyn_tex.texture().info();
            let row_pitch = info.width.saturating_mul(4);
            let depth_pitch = row_pitch.saturating_mul(info.height);
            if let Some(imm) = self.as_immediate_mut() {
                if map_type == D3D11_MAP_WRITE_DISCARD {
                    let coherent = imm.mode.coherent_seq();
                    let seq = imm.mode.current_seq();
                    let alloc = dyn_tex.allocate(coherent);
                    dyn_tex.update_immediate_name(seq, alloc, false);
                }
                let out = D3d11MappedSubresource {
                    p_data: dyn_tex.mapped_memory().cast(),
                    row_pitch,
                    depth_pitch,
                };
                // SAFETY: `mapped_resource` was checked for null above.
                unsafe {
                    mapped_resource
                        .cast::<D3d11MappedSubresource>()
                        .write_unaligned(out);
                }
                return S_OK;
            } else if let Some(def) = self.as_deferred_mut() {
                let alloc = if map_type == D3D11_MAP_WRITE_DISCARD {
                    def.mode.rotate_dynamic_texture(dyn_tex)
                } else {
                    def.mode.get_or_alloc_dynamic_texture(dyn_tex)
                };
                let out = D3d11MappedSubresource {
                    p_data: alloc.mapped_memory().cast(),
                    row_pitch,
                    depth_pitch,
                };
                // SAFETY: `mapped_resource` was checked for null above.
                unsafe {
                    mapped_resource
                        .cast::<D3d11MappedSubresource>()
                        .write_unaligned(out);
                }
                return S_OK;
            }
        }

        if let Some(staging) = common.staging.as_ref() {
            let Some(imm) = self.as_immediate_mut() else {
                return E_INVALIDARG;
            };
            let read = matches!(map_type, D3D11_MAP_READ | D3D11_MAP_READ_WRITE);
            let write = matches!(map_type, D3D11_MAP_WRITE | D3D11_MAP_READ_WRITE);
            if !read && !write {
                return E_INVALIDARG;
            }
            loop {
                let coherent = imm.mode.coherent_seq();
                match staging.try_map(coherent, read, write) {
                    MapResult::Mappable => {
                        let out = D3d11MappedSubresource {
                            p_data: staging.mapped_immediate_memory().cast(),
                            row_pitch: staging.bytes_per_row(),
                            depth_pitch: staging.bytes_per_image(),
                        };
                        // SAFETY: `mapped_resource` was checked for null above.
                        unsafe {
                            mapped_resource
                                .cast::<D3d11MappedSubresource>()
                                .write_unaligned(out);
                        }
                        return S_OK;
                    }
                    MapResult::Renamable => {
                        let seq = imm.mode.current_seq();
                        let id = staging.allocate(coherent);
                        staging.update_immediate_name(seq, id);
                        let out = D3d11MappedSubresource {
                            p_data: staging.mapped_immediate_memory().cast(),
                            row_pitch: staging.bytes_per_row(),
                            depth_pitch: staging.bytes_per_image(),
                        };
                        // SAFETY: `mapped_resource` was checked for null above.
                        unsafe {
                            mapped_resource
                                .cast::<D3d11MappedSubresource>()
                                .write_unaligned(out);
                        }
                        return S_OK;
                    }
                    MapResult::WaitFor(wait_seq) => {
                        let no_wait = (map_flags & D3D11_MAP_FLAG_DO_NOT_WAIT) != 0;
                        if no_wait && !imm.mode.ignore_no_wait {
                            if wait_seq == imm.mode.current_seq_peek() && imm.mode.has_dirty_op {
                                imm.commit_internal(None);
                            }
                            return DXGI_ERROR_WAS_STILL_DRAWING;
                        }
                        imm.wait_for_seq(wait_seq);
                    }
                }
            }
        }

        E_NOTIMPL
    }

    /// `ID3D11DeviceContext::Unmap`.
    pub const fn unmap(&mut self, resource: *mut c_void, subresource: u32) {
        let _ = (resource, subresource);
    }
}

#[cfg(test)]
mod tests;
