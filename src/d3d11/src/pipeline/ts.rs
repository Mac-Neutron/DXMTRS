//! Tessellation emulation via Metal mesh render pipeline (`d3d11_pipeline_ts.cpp`).

use crate::{
    pipeline::{
        GraphicsDesc, IMMUTABLE_FRAGMENT_BUFFERS, IMMUTABLE_TS_OBJECT_BUFFERS,
        MESH_PAYLOAD_MEMORY_LENGTH, depth_stencil_planes, is_unorm8_render_target_format,
    },
    shader::args::ShaderVariant,
};

/// Resolved tessellation mesh render pipeline spec.
pub struct TessellationPipelineSpec {
    pub vshs_variant: ShaderVariant,
    pub ds_variant: ShaderVariant,
    pub ps_variant: Option<ShaderVariant>,
    pub max_potential_tess_factor: u32,
    pub payload_memory_length: u16,
    pub immutable_object_buffers: u32,
    pub immutable_mesh_buffers: u32,
    pub immutable_fragment_buffers: u32,
    pub depth_pixel_format: u32,
    pub stencil_pixel_format: u32,
}

/// Builds the tessellation mesh pipeline spec from `desc`.
#[must_use]
pub fn build_tessellation_pipeline(desc: &GraphicsDesc) -> TessellationPipelineSpec {
    let input_layout_sha1 = desc.input_layout.and_then(|il| {
        if il.is_null() {
            None
        } else {
            // SAFETY: `il` points to a valid `InputLayout`.
            Some(unsafe { *(*il).sha1() })
        }
    });
    let vs_sha1 = desc.vertex_shader.as_ref().map_or([0u8; 20], |s| s.sha1);
    let hs_sha1 = desc.hull_shader.as_ref().map_or([0u8; 20], |s| s.sha1);

    let max_potential_tess_factor = desc
        .domain_shader
        .as_ref()
        .map_or(64, |ds| {
            // SAFETY: `ds` is a domain shader so `u.post_tessellator` is initialized.
            let f = unsafe { ds.reflection.u.post_tessellator.max_potential_tess_factor };
            if f == 0 { 64 } else { f }
        })
        .clamp(1, 64);

    let vshs_variant = ShaderVariant::TessellationVertexHull {
        input_layout_sha1,
        vs_sha1,
        index_buffer_format: desc.index_buffer_format,
        max_potential_tess_factor,
    };

    let ds_variant = ShaderVariant::TessellationDomain {
        hs_sha1,
        gs_passthrough: desc.gs_passthrough,
        max_potential_tess_factor,
        rasterization_disabled: !desc.rasterization_enabled,
    };

    let num_rtvs = if desc.rasterization_enabled && desc.pixel_shader.is_none() {
        0
    } else {
        (desc.num_color_attachments as usize).min(8)
    };
    let mut unorm_output_reg_mask = 0u32;
    for (i, &fmt) in desc.color_attachment_formats[..num_rtvs].iter().enumerate() {
        if is_unorm8_render_target_format(fmt) {
            unorm_output_reg_mask |= 1u32 << i;
        }
    }

    let dual_source_blending = desc.blend_state.is_some_and(|bs| {
        !bs.is_null() && {
            // SAFETY: `bs` points to a valid `BlendState`.
            unsafe { (*bs).is_dual_source_blending() }
        }
    });

    let ps_variant = desc.pixel_shader.as_ref().map(|_| ShaderVariant::Pixel {
        sample_mask: desc.sample_mask,
        dual_source_blending,
        disable_depth_output: desc.depth_stencil_format == 0,
        unorm_output_reg_mask,
    });

    let (depth_pixel_format, stencil_pixel_format) =
        depth_stencil_planes(desc.depth_stencil_format);

    TessellationPipelineSpec {
        vshs_variant,
        ds_variant,
        ps_variant,
        max_potential_tess_factor,
        payload_memory_length: MESH_PAYLOAD_MEMORY_LENGTH,
        immutable_object_buffers: IMMUTABLE_TS_OBJECT_BUFFERS,
        immutable_mesh_buffers: IMMUTABLE_FRAGMENT_BUFFERS,
        immutable_fragment_buffers: IMMUTABLE_FRAGMENT_BUFFERS,
        depth_pixel_format,
        stencil_pixel_format,
    }
}
