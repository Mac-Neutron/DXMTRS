//! Context compute dispatch commands (`Dispatch`, `DispatchIndirect`).

use core::ffi::c_void;

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    pub const fn dispatch(
        &mut self,
        thread_group_count_x: u32,
        thread_group_count_y: u32,
        thread_group_count_z: u32,
    ) {
        let _ = (
            thread_group_count_x,
            thread_group_count_y,
            thread_group_count_z,
        );
    }

    pub const fn dispatch_indirect(
        &mut self,
        buffer_for_args: *mut c_void,
        aligned_byte_offset_for_args: u32,
    ) {
        let _ = (buffer_for_args, aligned_byte_offset_for_args);
    }
}
