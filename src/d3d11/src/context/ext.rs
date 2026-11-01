//! `IMTLD3D11ContextExt` / `Ext1` / `Ext2` implementation (`d3d11_context_impl.cpp:5195-5429`).

use core::ffi::c_void;

use super::{Context, Mode};
use crate::{
    abi::ext::{D3D11Piece, D3D11UpscaleDesc},
    com::{E_NOTIMPL, Hresult},
};

impl<M: Mode> Context<M> {
    /// `IMTLD3D11ContextExt::TemporalUpscale`.
    pub const fn temporal_upscale(&mut self, desc: *const D3D11UpscaleDesc) {
        let _ = desc;
    }

    /// `IMTLD3D11ContextExt::BeginUAVOverlap`.
    pub const fn begin_uav_overlap(&mut self) {}

    /// `IMTLD3D11ContextExt::EndUAVOverlap`.
    pub const fn end_uav_overlap(&mut self) {}

    /// `IMTLD3D11ContextExt1::CheckFeatureSupport`.
    pub const fn check_ext_feature_support(
        &mut self,
        feature: u32,
        feature_data: *mut c_void,
        feature_data_size: u32,
    ) -> Hresult {
        let _ = (feature, feature_data, feature_data_size);
        E_NOTIMPL
    }

    /// `IMTLD3D11ContextExt2::GetPieces`.
    pub const fn get_pieces(
        &mut self,
        resource: *mut c_void,
        subresource: u32,
        count: u32,
        pieces: *mut D3D11Piece,
    ) -> Hresult {
        let _ = (resource, subresource, count, pieces);
        E_NOTIMPL
    }

    /// `IMTLD3D11ContextExt2::UsePiece`.
    pub const fn use_piece(&mut self, resource: *mut c_void, subresource: u32, piece_id: u64) {
        let _ = (resource, subresource, piece_id);
    }
}
