//! Context compute dispatch commands (`Dispatch`, `DispatchIndirect`).
//!
//! Validates compute pipeline state via [`Context::pre_dispatch`] and emits direct and indirect
//! compute threadgroup dispatch records (`compute 1` and `compute 2`, `d3d11_context_impl.cpp`).

use core::ffi::c_void;

use dxmt_core::{
    deptrack::Access,
    encoder::{NO_NEXT, stage},
};
use winemetal_sys::wire::{
    WMT_COMPUTE_COMMAND_DISPATCH, WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT, WMTSize,
    WmtcmdComputeDispatch, WmtcmdComputeDispatchIndirect,
};

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    /// `ID3D11DeviceContext::Dispatch` (`d3d11_context_impl.cpp:2519-2538`).
    pub fn dispatch(
        &mut self,
        thread_group_count_x: u32,
        thread_group_count_y: u32,
        thread_group_count_z: u32,
    ) {
        if thread_group_count_x == 0 || thread_group_count_y == 0 || thread_group_count_z == 0 {
            return;
        }
        if !self.pre_dispatch() {
            return;
        }
        let is_live = self.is_live();
        // C++ closure #25 (`Dispatch`, d3d11_context_impl.cpp:2530):
        // emits `WmtcmdComputeDispatch` (`compute 1`).
        let cmd = WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH,
            reserved: [0; 3],
            next: NO_NEXT,
            size: WMTSize {
                width: u64::from(thread_group_count_x),
                height: u64::from(thread_group_count_y),
                depth: u64::from(thread_group_count_z),
            },
        };
        self.emit_compute_cmd(cmd, move |enc| {
            if is_live {
                enc.resolve_compute_pass_barrier();
            }
            enc.encode_compute_command(cmd);
        });
    }

    /// `ID3D11DeviceContext::DispatchIndirect` (`d3d11_context_impl.cpp:2540-2562`).
    pub fn dispatch_indirect(
        &mut self,
        buffer_for_args: *mut c_void,
        aligned_byte_offset_for_args: u32,
    ) {
        if buffer_for_args.is_null() {
            return;
        }
        if !self.pre_dispatch() {
            return;
        }
        if self.is_live() {
            let Some(arg_buf) =
                crate::resource::common(buffer_for_args).and_then(|c| c.buffer.clone())
            else {
                return;
            };
            let base = WmtcmdComputeDispatchIndirect {
                r#type: WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT,
                reserved: [0; 3],
                next: NO_NEXT,
                indirect_args_buffer: 0,
                indirect_args_offset: u64::from(aligned_byte_offset_for_args),
            };
            self.emit_compute_cmd(base, move |enc| {
                let Some((alloc, sub_off)) =
                    enc.access_buffer::<{ stage::COMPUTE }>(&arg_buf, Access::READ)
                else {
                    return;
                };
                let mut cmd = base;
                cmd.indirect_args_buffer = alloc.buffer();
                cmd.indirect_args_offset = u64::from(aligned_byte_offset_for_args) + sub_off;
                enc.resolve_compute_pass_barrier();
                enc.encode_compute_command(cmd);
            });
            return;
        }
        let Some((indirect_args_buffer, sub_off)) = Self::resolve_indirect_buffer(buffer_for_args)
        else {
            return;
        };
        let indirect_args_offset = u64::from(aligned_byte_offset_for_args) + sub_off;
        // C++ closure #26 (`DispatchIndirect`, d3d11_context_impl.cpp:2549):
        // emits `WmtcmdComputeDispatchIndirect` (`compute 2`).
        let cmd = WmtcmdComputeDispatchIndirect {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH_INDIRECT,
            reserved: [0; 3],
            next: NO_NEXT,
            indirect_args_buffer,
            indirect_args_offset,
        };
        self.emit_compute_cmd(cmd, move |enc| {
            enc.encode_compute_command(cmd);
        });
    }
}
