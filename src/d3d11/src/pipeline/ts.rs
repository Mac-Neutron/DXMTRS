//! Tessellation emulation via Metal mesh render pipeline (`d3d11_pipeline_ts.cpp`).

use winemetal_wire::WMTRenderPipelineBlendInfo;

use crate::{
    LOG,
    device::CONFIG,
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
    pub blend_info: WMTRenderPipelineBlendInfo,
    pub max_potential_tess_factor: u32,
    pub payload_memory_length: u16,
    pub immutable_object_buffers: u32,
    pub immutable_mesh_buffers: u32,
    pub immutable_fragment_buffers: u32,
    pub depth_pixel_format: u32,
    pub stencil_pixel_format: u32,
    pub num_output_element: u32,
    pub threads_per_patch: u32,
}

/// Builds the tessellation mesh pipeline spec from `desc` using default unit-test caps.
#[must_use]
pub fn build_tessellation_pipeline(desc: &GraphicsDesc) -> TessellationPipelineSpec {
    build_tessellation_pipeline_with_caps(desc, false, true)
}

/// Builds the tessellation mesh pipeline spec from `desc` with GPU family capability flags.
#[must_use]
pub fn build_tessellation_pipeline_with_caps(
    desc: &GraphicsDesc,
    supports_apple7: bool,
    supports_apple9: bool,
) -> TessellationPipelineSpec {
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

    let (num_output_element, threads_per_patch, mut expected_tess_factor) =
        desc.hull_shader.as_ref().map_or((0, 0, 0.0f32), |hs| {
            // SAFETY: `hs` is a hull shader so `u.tessellator` is initialized.
            let max_f = unsafe { hs.reflection.u.tessellator.max_factor };
            (
                hs.reflection.num_output_element,
                hs.reflection.threads_per_patch,
                max_f,
            )
        });

    let mut max_potential_tess_factor = desc
        .domain_shader
        .as_ref()
        .map_or(64, |ds| {
            // SAFETY: `ds` is a domain shader so `u.post_tessellator` is initialized.
            let f = unsafe { ds.reflection.u.post_tessellator.max_potential_tess_factor };
            if f == 0 { 64 } else { f }
        })
        .clamp(1, 64);

    if !supports_apple9 {
        max_potential_tess_factor = max_potential_tess_factor.min(8);
    }
    let tess_factor_config = CONFIG.get_i32("d3d11.maxTessFactor", 0).cast_unsigned();
    if (4..=64).contains(&tess_factor_config) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "tess_factor_config is bounded to 4..=64"
        )]
        {
            expected_tess_factor = tess_factor_config as f32;
        }
        max_potential_tess_factor = max_potential_tess_factor.min(tess_factor_config);
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "max_potential_tess_factor is bounded to 1..=64"
    )]
    let max_tess_f32 = max_potential_tess_factor as f32;
    if max_tess_f32 < expected_tess_factor {
        dxmt_util::warn!(
            LOG,
            "maxtessfactor({expected_tess_factor}) is too large for a mesh pipeline. Clamping to {max_potential_tess_factor}"
        );
    }

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

    let payload_memory_length = if supports_apple7 {
        0
    } else {
        MESH_PAYLOAD_MEMORY_LENGTH
    };

    TessellationPipelineSpec {
        vshs_variant,
        ds_variant,
        ps_variant,
        blend_info,
        max_potential_tess_factor,
        payload_memory_length,
        immutable_object_buffers: IMMUTABLE_TS_OBJECT_BUFFERS,
        immutable_mesh_buffers: IMMUTABLE_FRAGMENT_BUFFERS,
        immutable_fragment_buffers: IMMUTABLE_FRAGMENT_BUFFERS,
        depth_pixel_format,
        stencil_pixel_format,
        num_output_element,
        threads_per_patch,
    }
}
