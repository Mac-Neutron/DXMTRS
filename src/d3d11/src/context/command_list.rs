//! `ID3D11CommandList`, pool, `FinishCommandList` and `ExecuteCommandList`.

use core::ffi::c_void;

use super::{Context, Mode};
use crate::com::{E_NOTIMPL, Hresult};

/// Device-owned pool of reusable [`CommandList`] instances (`MTLD3D11CommandListPool`).
#[derive(Default)]
pub struct CommandListPool;

/// Recorded D3D11 command list (`MTLD3D11CommandList`).
pub struct CommandList {
    pub context_flags: u32,
}

impl<M: Mode> Context<M> {
    /// `ID3D11DeviceContext::FinishCommandList`.
    pub const fn finish_command_list(
        &mut self,
        restore_deferred_context_state: i32,
        command_list: *mut *mut c_void,
    ) -> Hresult {
        let _ = (restore_deferred_context_state, command_list);
        E_NOTIMPL
    }

    /// `ID3D11DeviceContext::ExecuteCommandList`.
    pub const fn execute_command_list(
        &mut self,
        command_list: *mut c_void,
        restore_context_state: i32,
    ) {
        let _ = (command_list, restore_context_state);
    }
}
