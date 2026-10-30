use crate::{WMTIndirectCommandBufferInfo, WMTTextureInfo};

#[test]
fn bitfield_getters_read_their_bits() {
    let texture = WMTTextureInfo {
        pixel_format: 0,
        width: 0,
        height: 0,
        depth: 0,
        array_length: 0,
        bits: 0x0403_0201,
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    assert_eq!(
        (texture.r#type(), texture.mipmap_level_count(), texture.sample_count(), texture.usage()),
        (1, 2, 3, 4)
    );

    let icb = WMTIndirectCommandBufferInfo {
        r#type: 0,
        bits: 0b101 | 1 << 10 | 0x1f_ffff << 11,
        max_vertex_buffer_binding: 0,
        max_fragment_buffer_binding: 0,
        max_kernel_buffer_binding: 0,
        max_mesh_buffer_binding: 0,
        max_object_buffer_binding: 0,
        max_object_threadgroup_memory_binding: 0,
        max_kernel_threadgroup_memory_binding: 0,
        padding: 0,
        gpu_resource_id: 0,
    };
    assert_eq!(
        (icb.inherit_buffers(), icb.inherit_pso(), icb.inherit_cull_mode(), icb.support_ray_tracing()),
        (1, 0, 1, 0)
    );
    assert_eq!(icb.support_color_attachment_mapping(), 1);
}
