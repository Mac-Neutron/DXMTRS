//! `IMTLD3D11ContextExt` / `Ext1` / `Ext2` implementation (`d3d11_context_impl.cpp:5195-5429`).

use core::ffi::c_void;
use std::sync::Arc;

use super::{
    Context, Mode,
    immediate::{push_encoder_trace, resource_common},
    state::DirtyFlags,
};
use crate::{
    abi::ext::{D3D11Piece, D3D11UpscaleDesc, FEATURE_METALFX_TEMPORAL_SCALER},
    com::{E_FAIL, E_INVALIDARG, Hresult, S_OK},
};

const D3D11_BIND_VERTEX_BUFFER: u32 = 0x1;
const D3D11_BIND_INDEX_BUFFER: u32 = 0x2;

const fn read_upscale_desc(desc: *const D3D11UpscaleDesc) -> D3D11UpscaleDesc {
    // SAFETY: `desc` was verified non-null by `temporal_upscale`.
    unsafe { desc.read_unaligned() }
}

const fn write_piece(pieces: *mut D3D11Piece, index: usize, piece: D3D11Piece) {
    // SAFETY: `pieces` has at least `index + 1` elements per `get_pieces` contract.
    let slot = unsafe { pieces.add(index) };
    // SAFETY: `slot` points to a valid element within `pieces`.
    unsafe {
        slot.write_unaligned(piece);
    }
}

impl<M: Mode> Context<M> {
    /// `IMTLD3D11ContextExt::TemporalUpscale`.
    pub fn temporal_upscale(&mut self, desc: *const D3D11UpscaleDesc) {
        if desc.is_null() {
            return;
        }
        let d = read_upscale_desc(desc);
        let [scale_x, scale_y] = d.motion_vector_scale;
        let need_mv_scale = (scale_x - 1.0).abs() > f32::EPSILON
            || (scale_y - 1.0).abs() > f32::EPSILON
            || d.motion_vector_in_display_res != 0;
        if need_mv_scale {
            self.begin_compute_pass();
            M::emit_op(self, move |enc| {
                push_encoder_trace(enc, "MvScale");
            });
        }
        self.finish_pass(false);
        M::emit_op(self, move |enc| {
            push_encoder_trace(enc, "UpscaleTemporal");
        });
    }

    /// `IMTLD3D11ContextExt::BeginUAVOverlap`.
    pub fn begin_uav_overlap(&mut self) {
        M::emit_st(self, |enc| {
            enc.set_intrapass_barrier_control(0);
            push_encoder_trace(enc, "BarrierControl(0)");
        });
    }

    /// `IMTLD3D11ContextExt::EndUAVOverlap`.
    pub fn end_uav_overlap(&mut self) {
        M::emit_st(self, |enc| {
            enc.set_intrapass_barrier_control(1);
            push_encoder_trace(enc, "BarrierControl(1)");
        });
    }

    /// `IMTLD3D11ContextExt1::CheckFeatureSupport`.
    pub const fn check_ext_feature_support(
        &mut self,
        feature: u32,
        feature_data: *mut c_void,
        feature_data_size: u32,
    ) -> Hresult {
        if feature != FEATURE_METALFX_TEMPORAL_SCALER
            || feature_data.is_null()
            || (feature_data_size as usize) < size_of::<u32>()
        {
            return E_INVALIDARG;
        }
        // SAFETY: `feature_data` was checked for null and `feature_data_size >= 4`.
        unsafe {
            feature_data.cast::<u32>().write_unaligned(1);
        }
        S_OK
    }

    /// `IMTLD3D11ContextExt2::GetPieces`.
    pub fn get_pieces(
        &mut self,
        resource: *mut c_void,
        subresource: u32,
        count: u32,
        pieces: *mut D3D11Piece,
    ) -> Hresult {
        if self.as_deferred().is_some() {
            return E_FAIL;
        }
        if resource.is_null() || pieces.is_null() || subresource != 0 {
            return E_INVALIDARG;
        }
        let Some(common) = resource_common(resource) else {
            return E_INVALIDARG;
        };
        let Some(dyn_buf) = common.dynamic_buffer.as_ref() else {
            return E_INVALIDARG;
        };
        let Some(imm) = self.as_immediate() else {
            return E_FAIL;
        };
        let coherent = imm.mode.coherent_seq();
        let len = u32::try_from(dyn_buf.buffer().length()).unwrap_or(0);
        for i in 0..(count as usize) {
            let (id, alloc) = dyn_buf.lend(coherent);
            let piece = D3D11Piece {
                id,
                cpu: alloc.mapped_memory(0).cast(),
                row_pitch: len,
                depth_pitch: len,
            };
            write_piece(pieces, i, piece);
        }
        S_OK
    }

    /// `IMTLD3D11ContextExt2::UsePiece`.
    pub fn use_piece(&mut self, resource: *mut c_void, subresource: u32, piece_id: u64) {
        if resource.is_null() || subresource != 0 {
            return;
        }
        let Some(common) = resource_common(resource) else {
            return;
        };
        let Some(dyn_buf) = common.dynamic_buffer.as_ref() else {
            return;
        };
        let bind_flags = common.bind_flags;
        let Some(imm) = self.as_immediate_mut() else {
            return;
        };
        if dyn_buf.immediate_piece() == piece_id {
            return;
        }
        if let Some(alloc) = dyn_buf.take_lent(piece_id) {
            let seq = imm.mode.current_seq();
            dyn_buf.update_immediate_name(seq, Arc::clone(&alloc), 0, false);
            dyn_buf.set_immediate_piece(piece_id);
            if (bind_flags & D3D11_BIND_VERTEX_BUFFER) != 0 {
                imm.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
            }
            if (bind_flags & D3D11_BIND_INDEX_BUFFER) != 0 {
                imm.dirty.insert(DirtyFlags::INDEX_BUFFER);
            }
        }
    }
}

#[cfg(test)]
mod tests;
