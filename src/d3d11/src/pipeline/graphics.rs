//! Standard vertex + pixel graphics pipeline compilation (`d3d11_pipeline.cpp`).

use winemetal_wire::WMTRenderPipelineBlendInfo;

use crate::{
    pipeline::{
        GraphicsDesc, IMMUTABLE_FRAGMENT_BUFFERS, IMMUTABLE_VERTEX_BUFFERS, depth_stencil_planes,
        is_unorm8_render_target_format,
    },
    shader::args::ShaderVariant,
};

/// Resolved standard vertex + pixel graphics pipeline descriptor fields.
pub struct GraphicsPipelineSpec {
    pub vs_variant: ShaderVariant,
    pub ps_variant: Option<ShaderVariant>,
    pub blend_info: WMTRenderPipelineBlendInfo,
    pub depth_pixel_format: u32,
    pub stencil_pixel_format: u32,
    pub immutable_vertex_buffers: u32,
    pub immutable_fragment_buffers: u32,
}

/// Builds the standard vertex + pixel graphics pipeline spec from `desc`.
#[must_use]
pub fn build_graphics_pipeline(desc: &GraphicsDesc) -> GraphicsPipelineSpec {
    let input_layout_sha1 = desc.input_layout.and_then(|il| {
        if il.is_null() {
            None
        } else {
            // SAFETY: `il` points to a valid `InputLayout`.
            Some(unsafe { *(*il).sha1() })
        }
    });

    let so_layout_sha1 = desc.so_layout.and_then(|so| {
        if so.is_null() {
            None
        } else {
            // SAFETY: `so` points to a valid `StreamOutputLayout`.
            Some(unsafe { (*so).sha1 })
        }
    });

    let vs_variant = so_layout_sha1.map_or(
        ShaderVariant::Vertex {
            input_layout_sha1,
            gs_passthrough: desc.gs_passthrough,
            rasterization_disabled: !desc.rasterization_enabled,
        },
        |so_sha1| ShaderVariant::VertexStreamOutput {
            input_layout_sha1,
            so_layout_sha1: so_sha1,
        },
    );

    let (depth_pixel_format, stencil_pixel_format) =
        depth_stencil_planes(desc.depth_stencil_format);

    let mut unorm_output_reg_mask = 0u32;
    let num_rt = (desc.num_color_attachments as usize).min(8);
    for (i, &fmt) in desc.color_attachment_formats[..num_rt].iter().enumerate() {
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

    let valid_rt_mask = desc.pixel_shader.as_ref().map_or(0xff, |ps| {
        // SAFETY: `ps` is a pixel shader so `u.ps_valid_render_targets` is initialized.
        unsafe { ps.reflection.u.ps_valid_render_targets }
    });

    // SAFETY: `WMTRenderPipelineBlendInfo` is a C ABI struct of plain integers.
    let mut blend_info: WMTRenderPipelineBlendInfo = unsafe { core::mem::zeroed() };
    for (i, &fmt) in desc.color_attachment_formats[..num_rt].iter().enumerate() {
        blend_info.colors[i].pixel_format = fmt;
    }
    if let Some(bs) = desc.blend_state
        && !bs.is_null()
    {
        // SAFETY: `bs` points to a valid `BlendState`.
        unsafe {
            (*bs).setup_metal_pipeline_descriptor(
                &mut blend_info,
                desc.num_color_attachments,
                valid_rt_mask,
            );
        }
    }

    GraphicsPipelineSpec {
        vs_variant,
        ps_variant,
        blend_info,
        depth_pixel_format,
        stencil_pixel_format,
        immutable_vertex_buffers: IMMUTABLE_VERTEX_BUFFERS,
        immutable_fragment_buffers: IMMUTABLE_FRAGMENT_BUFFERS,
    }
}
