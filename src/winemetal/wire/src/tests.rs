use crate::{
    MemPtr, UnixcallMtldrawableSetpresentedevent, UnixcallWmtAttachMetalView,
    UnixcallWmtViewSyncAndPoll, WMTCursorOverlayParams, WMTIndirectCommandBufferInfo,
    WMTTextureInfo, WMTViewAttachDesc, WMTViewPollOut, slot,
};

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
        (
            texture.r#type(),
            texture.mipmap_level_count(),
            texture.sample_count(),
            texture.usage()
        ),
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
        (
            icb.inherit_buffers(),
            icb.inherit_pso(),
            icb.inherit_cull_mode(),
            icb.support_ray_tracing()
        ),
        (1, 0, 1, 0)
    );
    assert_eq!(icb.support_color_attachment_mapping(), 1);
}

/// `MemPtr` round-trips a reachable address and answers null for the null pointer.
#[test]
fn mem_ptr_round_trips_addresses() {
    let value = 7_u32;
    let p = core::ptr::from_ref(&value);
    assert_eq!(MemPtr::from_ptr(p).as_ptr::<u32>().cast_const(), p);
    assert_eq!(MemPtr::from_addr(p.addr()).as_ptr::<u32>().cast_const(), p);
    assert!(MemPtr::NULL.as_ptr::<u8>().is_null());
    assert!(
        MemPtr::from_ptr(core::ptr::null::<u8>())
            .as_ptr::<u8>()
            .is_null()
    );
}

#[test]
fn pull_on_present_wire_sizes_and_slots() {
    assert_eq!(size_of::<WMTViewAttachDesc>(), 16);
    assert_eq!(size_of::<WMTViewPollOut>(), 40);
    assert_eq!(size_of::<WMTCursorOverlayParams>(), 64);
    assert_eq!(size_of::<UnixcallWmtAttachMetalView>(), 48);
    assert_eq!(size_of::<UnixcallWmtViewSyncAndPoll>(), 32);
    assert_eq!(size_of::<UnixcallMtldrawableSetpresentedevent>(), 40);
    assert_eq!(slot::WMT_DECLARE_LATENCY_CRITICAL_ACTIVITY, 158);
    assert_eq!(slot::WMT_ATTACH_METAL_VIEW, 159);
    assert_eq!(slot::WMT_RETIRE_METAL_VIEW, 160);
    assert_eq!(slot::WMT_VIEW_SYNC_AND_POLL, 161);
    assert_eq!(slot::WMT_PRESENT_CLIENT_SURFACE, 162);
    assert_eq!(slot::MTL_DRAWABLE_SET_PRESENTED_EVENT, 163);
    assert_eq!(slot::WMT_SET_CURSOR_OVERLAY, 164);
}
