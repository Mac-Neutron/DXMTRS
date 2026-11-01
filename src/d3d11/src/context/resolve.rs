//! Multisample resolve (`ResolveSubresource`).

use core::ffi::c_void;

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    pub const fn resolve_subresource(
        &mut self,
        dst_resource: *mut c_void,
        dst_subresource: u32,
        src_resource: *mut c_void,
        src_subresource: u32,
        format: u32,
    ) {
        let _ = (
            dst_resource,
            dst_subresource,
            src_resource,
            src_subresource,
            format,
        );
    }
}
