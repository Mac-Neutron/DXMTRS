//! Mipmap generation (`GenerateMips`).

use core::ffi::c_void;

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    pub const fn generate_mips(&mut self, shader_resource_view: *mut c_void) {
        let _ = shader_resource_view;
    }
}
