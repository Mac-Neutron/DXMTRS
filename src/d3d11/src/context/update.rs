//! Context subresource uploads (`UpdateSubresource`, `UpdateSubresource1`).

use core::ffi::c_void;

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    pub const fn update_subresource(
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
    pub const fn update_subresource1(
        &mut self,
        dst_resource: *mut c_void,
        dst_subresource: u32,
        dst_box: *const c_void,
        src_data: *const c_void,
        src_row_pitch: u32,
        src_depth_pitch: u32,
        copy_flags: u32,
    ) {
        let _ = (
            dst_resource,
            dst_subresource,
            dst_box,
            src_data,
            src_row_pitch,
            src_depth_pitch,
            copy_flags,
        );
    }
}
