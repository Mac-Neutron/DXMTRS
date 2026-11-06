//! Context resource mapping (`Map`, `Unmap`).

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::staging::MapResult;
use dxmt_util::{
    com::hresult::DXGI_ERROR_WAS_STILL_DRAWING,
    dxgi_consts::{
        D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_INDEX_BUFFER, D3D11_BIND_SHADER_RESOURCE,
        D3D11_BIND_VERTEX_BUFFER,
    },
};

use super::{
    Context, Mode,
    encode::CommandBufferState,
    immediate::{push_encoder_trace, resource_common},
    state::DirtyFlags,
};
use crate::{
    abi::records::D3d11MappedSubresource,
    com::{E_FAIL, E_INVALIDARG, E_NOTIMPL, Hresult, S_OK},
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

impl<M: Mode> Context<M> {
    /// `ID3D11DeviceContext::Map`.
    pub fn map(
        &mut self,
        resource: *mut c_void,
        subresource: u32,
        map_type: u32,
        map_flags: u32,
        mapped_resource: *mut c_void,
    ) -> Hresult {
        if resource.is_null() {
            return E_INVALIDARG;
        }
        let Some(common) = resource_common(resource) else {
            return E_INVALIDARG;
        };

        let is_dyn_tex_sub =
            crate::resource::dynamic_texture_subresource(resource, subresource).is_some();
        if let Some((dyn_buf, row_pitch, depth_pitch)) =
            crate::resource::dynamic_texture_subresource(resource, subresource).or_else(|| {
                let b = common.dynamic_buffer.as_ref()?;
                let len = u32::try_from(b.buffer().length()).unwrap_or(0);
                Some((b, len, len))
            })
        {
            if mapped_resource.is_null()
                || !matches!(
                    map_type,
                    D3D11_MAP_WRITE_DISCARD | D3D11_MAP_WRITE_NO_OVERWRITE
                )
            {
                return E_INVALIDARG;
            }
            let bind_flags = common.bind_flags;
            if map_type == D3D11_MAP_WRITE_DISCARD {
                if is_dyn_tex_sub {
                    for stage in &mut self.state.shader_stages {
                        stage.srvs.set_all_dirty();
                    }
                } else {
                    if (bind_flags & D3D11_BIND_VERTEX_BUFFER) != 0 {
                        self.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
                        self.state.input_assembler.vertex_buffers.set_all_dirty();
                    }
                    if (bind_flags & D3D11_BIND_INDEX_BUFFER) != 0 {
                        self.dirty.insert(DirtyFlags::INDEX_BUFFER);
                    }
                    if (bind_flags & D3D11_BIND_CONSTANT_BUFFER) != 0 {
                        for stage in &mut self.state.shader_stages {
                            stage.constant_buffers.set_all_dirty();
                        }
                    }
                    if (bind_flags & D3D11_BIND_SHADER_RESOURCE) != 0 {
                        for stage in &mut self.state.shader_stages {
                            stage.srvs.set_all_dirty();
                        }
                    }
                }
            }
            if let Some(imm) = self.as_immediate_mut() {
                if map_type == D3D11_MAP_WRITE_DISCARD {
                    let next_sub = dyn_buf.next_suballocation();
                    if next_sub != 0 {
                        dyn_buf.use_immediate_suballocation(next_sub);
                        let alloc = dyn_buf.immediate_name();
                        super::Immediate::emit_st(imm, move |_enc| {
                            alloc.use_suballocation(next_sub);
                        });
                    } else {
                        let coherent = imm.active_coherent_seq();
                        let seq = imm.active_seq();
                        let alloc = dyn_buf.allocate(coherent);
                        dyn_buf.update_immediate_name(seq, Arc::clone(&alloc), 0, false);
                        let buf = Arc::clone(dyn_buf.buffer());
                        super::Immediate::emit_st(imm, move |_enc| {
                            alloc.use_suballocation(0);
                            let _ = buf.rename(Arc::clone(&alloc));
                        });
                    }
                    if !is_dyn_tex_sub
                        && (bind_flags & D3D11_BIND_VERTEX_BUFFER) != 0
                        && imm.cmdbuf_state == CommandBufferState::RenderEncoderActive
                    {
                        push_encoder_trace(
                            imm.mode.encoder_mut(),
                            "Render:SetVertexBuffers(slot=0)",
                        );
                    }
                }
                let out = D3d11MappedSubresource {
                    p_data: dyn_buf.immediate_mapped_memory().cast(),
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
                    if !is_dyn_tex_sub
                        && (bind_flags & D3D11_BIND_VERTEX_BUFFER) != 0
                        && def.cmdbuf_state == CommandBufferState::RenderEncoderActive
                    {
                        def.mode.commands_mut().emit_fn(|enc| {
                            push_encoder_trace(enc, "Render:SetVertexBuffers(slot=0)");
                        });
                    }
                    let a = def.mode.rotate_dynamic_buffer(dyn_buf);
                    let a_emit = Arc::clone(&a);
                    let buf = Arc::clone(dyn_buf.buffer());
                    super::Deferred::emit_st(def, move |_enc| {
                        a_emit.use_suballocation(0);
                        let _ = buf.rename(Arc::clone(&a_emit));
                    });
                    a
                } else {
                    def.mode.get_or_alloc_dynamic_buffer(dyn_buf)
                };
                let out = D3d11MappedSubresource {
                    p_data: alloc.mapped_memory(0).cast(),
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

        if let Some(dyn_tex) = common.dynamic_texture.as_ref() {
            if mapped_resource.is_null()
                || !matches!(
                    map_type,
                    D3D11_MAP_WRITE_DISCARD | D3D11_MAP_WRITE_NO_OVERWRITE
                )
            {
                return E_INVALIDARG;
            }
            if map_type == D3D11_MAP_WRITE_DISCARD {
                for stage in &mut self.state.shader_stages {
                    stage.srvs.set_all_dirty();
                }
            }
            let (row_pitch, depth_pitch) = crate::resource::dynamic_linear_texture_layout(resource)
                .unwrap_or_else(|| {
                    let info = dyn_tex.texture().info();
                    let rp = info.width.saturating_mul(4);
                    (rp, rp.saturating_mul(info.height))
                });
            if let Some(imm) = self.as_immediate_mut() {
                if map_type == D3D11_MAP_WRITE_DISCARD {
                    let coherent = imm.active_coherent_seq();
                    let seq = imm.active_seq();
                    let alloc = dyn_tex.allocate(coherent);
                    dyn_tex.update_immediate_name(seq, Arc::clone(&alloc), false);
                    let tex = Arc::clone(dyn_tex.texture());
                    super::Immediate::emit_st(imm, move |_enc| {
                        let _ = tex.rename(Arc::clone(&alloc));
                    });
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
                    let a = def.mode.rotate_dynamic_texture(dyn_tex);
                    let a_emit = Arc::clone(&a);
                    let tex = Arc::clone(dyn_tex.texture());
                    super::Deferred::emit_st(def, move |_enc| {
                        let _ = tex.rename(Arc::clone(&a_emit));
                    });
                    a
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

        if let Some(staging) =
            crate::resource::staging(resource, subresource).or(common.staging.as_ref())
        {
            let Some(imm) = self.as_immediate_mut() else {
                return E_FAIL;
            };
            if mapped_resource.is_null() {
                return E_INVALIDARG;
            }
            let read = matches!(map_type, D3D11_MAP_READ | D3D11_MAP_READ_WRITE);
            let write = matches!(map_type, D3D11_MAP_WRITE | D3D11_MAP_READ_WRITE);
            if !read && !write {
                return E_INVALIDARG;
            }
            loop {
                let coherent = imm.active_coherent_seq();
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
                        let seq = imm.active_seq();
                        let id = staging.allocate(coherent);
                        let dst_mem = staging.mapped_memory(id);
                        let src_mem = staging.mapped_immediate_memory();
                        let len = usize::try_from(staging.length()).unwrap_or(0);
                        if !dst_mem.is_null() && !src_mem.is_null() && len > 0 {
                            // SAFETY: `dst_mem` and `src_mem` are distinct allocations of `len` bytes.
                            unsafe {
                                core::ptr::copy_nonoverlapping(src_mem, dst_mem, len);
                            }
                        }
                        staging.update_immediate_name(seq, id);
                        if let Some(new_alloc) = staging.allocation(id) {
                            let buf = Arc::clone(staging.buffer());
                            super::Immediate::emit_st(imm, move |_enc| {
                                let _ = buf.rename(Arc::clone(&new_alloc));
                            });
                        }
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
                            if wait_seq == imm.active_seq() && imm.mode.has_dirty_op {
                                imm.commit_internal(None);
                            }
                            return DXGI_ERROR_WAS_STILL_DRAWING;
                        }
                        imm.wait_for_seq(wait_seq);
                    }
                }
            }
        }

        crate::LOG.err("Mapped resource is neither dynamic nor staging");
        #[cfg(all(not(windows), not(test)))]
        {
            // SAFETY: Recording not-implemented call for native probe diagnostic.
            unsafe {
                winemetal_sys::wmt_native_not_implemented();
            }
        }
        E_NOTIMPL
    }

    /// `ID3D11DeviceContext::Unmap`.
    pub fn unmap(&mut self, resource: *mut c_void, subresource: u32) {
        if resource.is_null() {
            return;
        }
        if let Some((dyn_buf, row_pitch, depth_pitch)) =
            crate::resource::dynamic_texture_subresource(resource, subresource)
        {
            self.update_texture_from_buffer(
                resource,
                subresource,
                ptr::null(),
                Arc::clone(dyn_buf.buffer()),
                row_pitch,
                depth_pitch,
            );
        }
    }
}

#[cfg(test)]
mod tests;
