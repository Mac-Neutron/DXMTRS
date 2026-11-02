//! Multisample resolve (`ResolveSubresource`).

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::{
    format::{MTL_DXGI_FORMAT_BC, MTL_DXGI_FORMAT_TYPELESS, query_dxgi},
    resource::TextureViewDescriptor,
};
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_INVALID, WMT_TEXTURE_TYPE2D_ARRAY, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
};

use super::{
    Context, Mode,
    copy::{BlitObject, BlitObjectDesc, D3D11_RESOURCE_DIMENSION_TEXTURE2D, Subresource},
};

impl<M: Mode> Context<M> {
    pub fn resolve_subresource(
        &mut self,
        dst_resource: *mut c_void,
        dst_subresource: u32,
        src_resource: *mut c_void,
        src_subresource: u32,
        format: u32,
    ) {
        if dst_resource.is_null() || src_resource.is_null() {
            return;
        }
        let (Some(dst_obj), Some(src_obj)) = (
            BlitObject::new(self.device, dst_resource),
            BlitObject::new(self.device, src_resource),
        ) else {
            return;
        };

        if src_obj.dimension != D3D11_RESOURCE_DIMENSION_TEXTURE2D
            || dst_obj.dimension != D3D11_RESOURCE_DIMENSION_TEXTURE2D
        {
            return;
        }

        let (BlitObjectDesc::Texture2d(src_desc), BlitObjectDesc::Texture2d(dst_desc)) =
            (&src_obj.desc, &dst_obj.desc)
        else {
            return;
        };
        if src_desc.sample_desc.count <= 1 || dst_desc.sample_desc.count != 1 {
            return;
        }

        let (Some(dst_sub), Some(src_sub)) = (
            Subresource::from_blit(&dst_obj, dst_subresource),
            Subresource::from_blit(&src_obj, src_subresource),
        ) else {
            return;
        };

        let (_, format_desc) = query_dxgi(format);
        if format_desc.pixel_format == WMT_PIXEL_FORMAT_INVALID {
            return;
        }
        if (format_desc.flag & (MTL_DXGI_FORMAT_TYPELESS | MTL_DXGI_FORMAT_BC)) != 0 {
            return;
        }
        if src_sub.width != dst_sub.width || src_sub.height != dst_sub.height {
            return;
        }

        let (Some(src_common), Some(dst_common)) = (
            crate::resource::common(src_resource),
            crate::resource::common(dst_resource),
        ) else {
            return;
        };
        let (Some(src_tex), Some(dst_tex)) = (
            src_common.texture.as_ref().map(Arc::clone),
            dst_common.texture.as_ref().map(Arc::clone),
        ) else {
            return;
        };

        let src_view_key = TextureViewDescriptor {
            format: format_desc.pixel_format,
            texture_type: WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
            first_mip: src_sub.mip_level,
            mip_count: 1,
            first_slice: src_sub.array_slice,
            array_size: 1,
        }
        .key();
        let dst_view_key = TextureViewDescriptor {
            format: format_desc.pixel_format,
            texture_type: WMT_TEXTURE_TYPE2D_ARRAY,
            first_mip: dst_sub.mip_level,
            mip_count: 1,
            first_slice: dst_sub.array_slice,
            array_size: 1,
        }
        .key();

        self.invalidate_current_pass(false);
        // C++ closure #28 (`ResolveSubresource`, d3d11_context_impl.cpp:1661).
        M::emit_op(self, move |enc| {
            enc.resolve_texture(&src_tex, src_view_key, &dst_tex, dst_view_key);
        });
    }
}
