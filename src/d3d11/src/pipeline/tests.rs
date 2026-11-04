use core::mem::MaybeUninit;
use std::sync::Arc;

use winemetal_sys::sm50::MtlShaderReflection;

use crate::{
    pipeline::{
        GS_PAYLOAD_MEMORY_LENGTH, GraphicsDesc, GraphicsPipelineKind, IMMUTABLE_FRAGMENT_BUFFERS,
        IMMUTABLE_GS_OBJECT_BUFFERS, IMMUTABLE_TS_OBJECT_BUFFERS, IMMUTABLE_VERTEX_BUFFERS,
        MESH_PAYLOAD_MEMORY_LENGTH, PipelineCache, depth_stencil_planes, truncate_error_text,
    },
    shader::{ManagedShader, args::ShaderVariant},
};

fn dummy_reflection() -> MtlShaderReflection {
    let r = MaybeUninit::<MtlShaderReflection>::zeroed();
    // SAFETY: `MtlShaderReflection` is plain old data; zeroed bytes are a valid value.
    let mut init = unsafe { r.assume_init() };
    init.u.threadgroup_size = [8, 8, 1];
    init
}

#[test]
fn error_text_cut_at_256() {
    let short = "shader compile error";
    assert_eq!(truncate_error_text(short), short);

    let exact = "x".repeat(256);
    assert_eq!(truncate_error_text(&exact).len(), 256);

    let long = "a".repeat(400);
    let cut = truncate_error_text(&long);
    assert_eq!(cut.len(), 256);
    assert_eq!(cut, &long[..256]);
}

#[test]
fn pipeline_cache_dedupes_graphics_and_compute() {
    let cache = PipelineCache::default();
    let vs = ManagedShader::new_synthetic([1u8; 20], dummy_reflection());
    let ps = ManagedShader::new_synthetic([2u8; 20], dummy_reflection());

    let mut desc = GraphicsDesc {
        vertex_shader: Some(Arc::clone(&vs)),
        pixel_shader: Some(Arc::clone(&ps)),
        num_color_attachments: 1,
        rasterization_enabled: true,
        sample_count: 1,
        sample_mask: 0xffff_ffff,
        ..GraphicsDesc::default()
    };
    desc.color_attachment_formats[0] = 70; // RGBA8Unorm

    let pso1 = cache.graphics(&desc);
    let pso2 = cache.graphics(&desc);
    assert!(Arc::ptr_eq(&pso1, &pso2));
    assert!(pso1.ready().is_some());

    let GraphicsPipelineKind::Standard(spec) = &*pso1.kind else {
        panic!("expected standard graphics pipeline");
    };
    assert_eq!(spec.immutable_vertex_buffers, IMMUTABLE_VERTEX_BUFFERS);
    assert_eq!(spec.immutable_fragment_buffers, IMMUTABLE_FRAGMENT_BUFFERS);
    assert_eq!(
        spec.ps_variant,
        Some(ShaderVariant::Pixel {
            sample_mask: 0xffff_ffff,
            dual_source_blending: false,
            disable_depth_output: true,
            unorm_output_reg_mask: 1,
        })
    );

    let cs = Arc::new(ManagedShader::new_synthetic([9u8; 20], dummy_reflection()));
    let c1 = cache.compute(&cs);
    let c2 = cache.compute(&cs);
    assert!(Arc::ptr_eq(&c1, &c2));
    assert_eq!(c1.spec.threadgroup_size, [8, 8, 1]);
}

#[test]
fn gs_and_ts_dispatch_to_mesh_pipeline_specs() {
    let cache = PipelineCache::default();
    let vs = ManagedShader::new_synthetic([1u8; 20], dummy_reflection());
    let gs = ManagedShader::new_synthetic([3u8; 20], dummy_reflection());
    let hs = ManagedShader::new_synthetic([4u8; 20], dummy_reflection());
    let ds = ManagedShader::new_synthetic([5u8; 20], dummy_reflection());

    let gs_desc = GraphicsDesc {
        vertex_shader: Some(Arc::clone(&vs)),
        geometry_shader: Some(Arc::clone(&gs)),
        rasterization_enabled: true,
        sample_count: 1,
        gs_strip_topology: true,
        ..GraphicsDesc::default()
    };
    let gs_pso = cache.graphics(&gs_desc);
    let GraphicsPipelineKind::GeometryMesh(gs_spec) = &*gs_pso.kind else {
        panic!("expected geometry mesh pipeline");
    };
    assert_eq!(gs_spec.payload_memory_length, GS_PAYLOAD_MEMORY_LENGTH);
    assert_eq!(
        gs_spec.immutable_object_buffers,
        IMMUTABLE_GS_OBJECT_BUFFERS
    );
    assert_eq!(gs_spec.immutable_mesh_buffers, IMMUTABLE_FRAGMENT_BUFFERS);

    let ts_desc = GraphicsDesc {
        vertex_shader: Some(vs),
        hull_shader: Some(hs),
        domain_shader: Some(ds),
        rasterization_enabled: true,
        sample_count: 1,
        depth_stencil_format: 260,
        ..GraphicsDesc::default()
    };
    let ts_pso = cache.graphics(&ts_desc);
    let GraphicsPipelineKind::Tessellation(ts_spec) = &*ts_pso.kind else {
        panic!("expected tessellation mesh pipeline");
    };
    assert_eq!(ts_spec.payload_memory_length, MESH_PAYLOAD_MEMORY_LENGTH);
    assert_eq!(
        ts_spec.immutable_object_buffers,
        IMMUTABLE_TS_OBJECT_BUFFERS
    );
    assert_eq!(ts_spec.depth_pixel_format, 260);
    assert_eq!(ts_spec.stencil_pixel_format, 260);
    assert_eq!(depth_stencil_planes(252), (252, 0));
    assert_eq!(depth_stencil_planes(253), (0, 253));
}
