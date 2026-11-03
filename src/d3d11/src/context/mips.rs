//! Mipmap generation (`GenerateMips`).

use core::ffi::c_void;

use dxmt_core::{
    deptrack::Access,
    encoder::{NO_NEXT, stage},
};
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_GENERATE_MIPMAPS, WMT_PIXEL_FORMAT_A8_UNORM, WMT_PIXEL_FORMAT_R8_UNORM,
    WmtcmdBlitGenerateMipmaps,
};

use super::{Context, Mode};
use crate::view::Srv;

impl<M: Mode> Context<M> {
    pub fn generate_mips(&mut self, shader_resource_view: *mut c_void) {
        if shader_resource_view.is_null() {
            return;
        }
        let Some(srv) = Srv::from_iface(shader_resource_view) else {
            return;
        };
        if srv.buffer.is_some() {
            return;
        }
        let (Some(texture), Some(view_key)) = (srv.texture.clone(), srv.texture_view) else {
            return;
        };

        self.switch_to_blit_encoder();
        // C++ closure (`GenerateMips`, d3d11_context_impl.cpp:977).
        M::emit_op(self, move |enc| {
            let fixed_view = if view_key.format() == WMT_PIXEL_FORMAT_A8_UNORM {
                view_key.with_format(WMT_PIXEL_FORMAT_R8_UNORM)
            } else {
                view_key
            };
            let view_tex = enc
                .access_texture_view::<{ stage::COMPUTE }>(
                    &texture,
                    fixed_view,
                    Access::READ | Access::WRITE,
                )
                .map_or(0, |v| v.texture);
            if fixed_view.mip_count() > 1 {
                let cmd = WmtcmdBlitGenerateMipmaps {
                    r#type: WMT_BLIT_COMMAND_GENERATE_MIPMAPS,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    texture: view_tex,
                };
                enc.encode_blit_command(cmd);
            }
        });
    }
}
