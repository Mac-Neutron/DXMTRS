//! Geometry-shader emulation via Metal mesh render pipeline (`d3d11_pipeline_gs.cpp`).

use winemetal_wire::WMTRenderPipelineBlendInfo;

use crate::{
    pipeline::{
        GS_PAYLOAD_MEMORY_LENGTH, GraphicsDesc, IMMUTABLE_FRAGMENT_BUFFERS,
        IMMUTABLE_GS_OBJECT_BUFFERS, depth_stencil_planes, is_unorm8_render_target_format,
    },
    shader::args::ShaderVariant,
};

/// Resolved geometry-shader mesh render pipeline spec.
pub struct GeometryPipelineSpec {
    pub vs_variant: ShaderVariant,
    pub gs_variant: ShaderVariant,
    pub ps_variant: Option<ShaderVariant>,
    pub blend_info: WMTRenderPipelineBlendInfo,
    pub payload_memory_length: u16,
    pub immutable_object_buffers: u32,
    pub immutable_mesh_buffers: u32,
    pub immutable_fragment_buffers: u32,
    pub depth_pixel_format: u32,
    pub stencil_pixel_format: u32,
}

/// Builds the geometry-shader mesh pipeline spec from `desc`.
#[must_use]
pub fn build_geometry_pipeline(desc: &GraphicsDesc) -> GeometryPipelineSpec {
    let input_layout_sha1 = desc.input_layout.and_then(|il| {
        if il.is_null() {
            None
        } else {
            // SAFETY: `il` points to a valid `InputLayout`.
            Some(unsafe { *(*il).sha1() })
        }
    });
    let vs_sha1 = desc.vertex_shader.as_ref().map_or([0u8; 20], |s| s.sha1);
    let gs_sha1 = desc.geometry_shader.as_ref().map_or([0u8; 20], |s| s.sha1);

    let vs_variant = ShaderVariant::GeometryVertex {
        input_layout_sha1,
        gs_sha1,
        index_buffer_format: desc.index_buffer_format,
        strip_topology: desc.gs_strip_topology,
    };

    let gs_variant = ShaderVariant::Geometry {
        vs_sha1,
        strip_topology: desc.gs_strip_topology,
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

    let valid_rt_mask = desc.pixel_shader.as_ref().map_or(0, |ps| {
        // SAFETY: `ps` is a pixel shader so `u.ps_valid_render_targets` is initialized.
        unsafe { ps.reflection.u.ps_valid_render_targets }
    });

    let mut blend_info: WMTRenderPipelineBlendInfo = super::default_blend_info();
    for (i, &fmt) in desc.color_attachment_formats[..num_rtvs].iter().enumerate() {
        blend_info.colors[i].pixel_format = fmt;
    }
    if let Some(bs) = desc.blend_state
        && !bs.is_null()
    {
        // SAFETY: `bs` points to a valid `BlendState`.
        unsafe {
            (*bs).setup_metal_pipeline_descriptor(
                &mut blend_info,
                u32::try_from(num_rtvs).unwrap_or(0),
                valid_rt_mask,
            );
        }
    }

    let (depth_pixel_format, stencil_pixel_format) =
        depth_stencil_planes(desc.depth_stencil_format);

    GeometryPipelineSpec {
        vs_variant,
        gs_variant,
        ps_variant,
        blend_info,
        payload_memory_length: GS_PAYLOAD_MEMORY_LENGTH,
        immutable_object_buffers: IMMUTABLE_GS_OBJECT_BUFFERS,
        immutable_mesh_buffers: IMMUTABLE_FRAGMENT_BUFFERS,
        immutable_fragment_buffers: IMMUTABLE_FRAGMENT_BUFFERS,
        depth_pixel_format,
        stencil_pixel_format,
    }
}
