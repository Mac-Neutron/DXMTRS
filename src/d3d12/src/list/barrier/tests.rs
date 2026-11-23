use core::{mem::ManuallyDrop, ptr};
use std::sync::Arc;

use dxmt_core::resource::{Buffer, Texture, TextureAllocationFlags};
use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::{
    Obj,
    wire::{WMT_TEXTURE_TYPE2D, WMTTextureInfo},
};

use super::*;
use crate::{
    abi::{
        barrier::{
            BarrierGroup, BarrierGroupBarriers, BarrierSubresourceRange,
            D3D12_BARRIER_ACCESS_COMMON, D3D12_BARRIER_ACCESS_RENDER_TARGET,
            D3D12_BARRIER_LAYOUT_COMMON, D3D12_BARRIER_LAYOUT_RENDER_TARGET,
            D3D12_BARRIER_SYNC_ALL, D3D12_BARRIER_SYNC_NONE, D3D12_BARRIER_TYPE_GLOBAL,
            D3D12_BARRIER_TYPE_TEXTURE, D3D12_RESOURCE_BARRIER_FLAG_BEGIN_ONLY,
            D3D12_RESOURCE_BARRIER_FLAG_END_ONLY, D3D12_RESOURCE_BARRIER_FLAG_NONE,
            D3D12_RESOURCE_BARRIER_TYPE_ALIASING, D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            D3D12_RESOURCE_BARRIER_TYPE_UAV, D3D12_RESOURCE_STATE_COMMON,
            D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS, GlobalBarrier, ResourceAliasingBarrier,
            ResourceBarrier, ResourceBarrierBody, ResourceTransitionBarrier, ResourceUavBarrier,
            TextureBarrier,
        },
        pipeline::SampleDesc,
        resource::{
            D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
            HeapProperties, ResourceDesc,
        },
        vtbl::ID3D12Resource2Vtbl,
    },
    encoder::BarrierOpacity,
    list::{DiscardedSubresource, List},
    resource::Resource,
};

fn test_device() -> Obj {
    // SAFETY: `tests.rs` provides fake `NSObject_retain` and `NSObject_release` for test binaries.
    unsafe { Obj::adopt(1) }.expect("nonzero fake handle")
}

fn dummy_resource() -> Box<Resource> {
    // SAFETY: `LIST_VTBL` is larger than `ID3D12Resource2Vtbl` and consists of non-null function
    // pointers; tests never dispatch through `Resource::vtbl`.
    let vtbl = unsafe { &*(&raw const crate::list::LIST_VTBL).cast::<ID3D12Resource2Vtbl>() };
    Box::new(Resource {
        vtbl,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        texture: None,
        buffer: None,
        placed_heap: ptr::null_mut(),
        desc: ResourceDesc {
            dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            alignment: 0,
            width: 64,
            height: 1,
            depth_or_array_size: 1,
            mip_levels: 1,
            format: 0,
            sample_desc: SampleDesc {
                count: 1,
                quality: 0,
            },
            layout: 1,
            flags: 0,
        },
        heap_props: HeapProperties {
            r#type: 1,
            cpu_page_property: 0,
            memory_pool_preference: 0,
            creation_node_mask: 1,
            visible_node_mask: 1,
        },
    })
}

fn backed_resource(dev: &Obj, flags: u32) -> Box<Resource> {
    let mut res = dummy_resource();
    res.desc.flags = flags;
    res.buffer = Some(Arc::new(Buffer::new(64, dev)));
    res
}

fn transition_barrier(
    flags: u32,
    resource: *mut c_void,
    state_before: u32,
    state_after: u32,
) -> ResourceBarrier {
    ResourceBarrier {
        r#type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        flags,
        barrier: ResourceBarrierBody {
            transition: ManuallyDrop::new(ResourceTransitionBarrier {
                resource,
                subresource: 0,
                state_before,
                state_after,
            }),
        },
    }
}

#[test]
fn barrier_classification_as_cpp() {
    let dev = test_device();
    let mut res_a = backed_resource(&dev, 0);
    let mut res_b = backed_resource(&dev, 0);
    let mut res_sim = backed_resource(&dev, D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS);
    let mut res_unbacked = dummy_resource();

    let ptr_a = (&raw mut *res_a).cast::<c_void>();
    let ptr_b = (&raw mut *res_b).cast::<c_void>();
    let ptr_sim = (&raw mut *res_sim).cast::<c_void>();
    let ptr_unbacked = (&raw mut *res_unbacked).cast::<c_void>();
    let key_a = Arc::as_ptr(res_a.buffer.as_ref().expect("backed")).cast::<c_void>();
    let key_b = Arc::as_ptr(res_b.buffer.as_ref().expect("backed")).cast::<c_void>();

    // 1. BEGIN_ONLY skips opacity/join classification, still increments call count and names resource.
    let mut list = List::new_closed(ptr::null_mut());
    let b_begin = transition_barrier(
        D3D12_RESOURCE_BARRIER_FLAG_BEGIN_ONLY,
        ptr_a,
        D3D12_RESOURCE_STATE_RENDER_TARGET,
        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
    );
    // SAFETY: `list` and `b_begin` are valid for the call.
    unsafe { resource_barrier((&raw mut *list).cast(), 1, (&raw const b_begin).cast()) };
    assert_eq!(list.submission.barrier_opaque.len(), 0);
    assert_eq!(list.submission.barrier_names, vec![(1, key_a)]);
    assert_eq!(list.submission.barrier_count, 1);

    // 2. END_ONLY records SplitEnd and triggers a barrier.
    let b_end = transition_barrier(
        D3D12_RESOURCE_BARRIER_FLAG_END_ONLY,
        ptr_a,
        D3D12_RESOURCE_STATE_RENDER_TARGET,
        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
    );
    // SAFETY: `list` and `b_end` are valid for the call.
    unsafe { resource_barrier((&raw mut *list).cast(), 1, (&raw const b_end).cast()) };
    assert_eq!(
        list.submission.barrier_opaque,
        vec![(2, BarrierOpacity::SplitEnd)]
    );
    assert_eq!(list.submission.barrier_count, 2);

    // 3. UAV barrier records UavBarrier and resource key in barrier_names.
    let b_uav = ResourceBarrier {
        r#type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
        flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        barrier: ResourceBarrierBody {
            uav: ManuallyDrop::new(ResourceUavBarrier { resource: ptr_a }),
        },
    };
    // SAFETY: `list` and `b_uav` are valid for the call.
    unsafe { resource_barrier((&raw mut *list).cast(), 1, (&raw const b_uav).cast()) };
    assert_eq!(
        list.submission.barrier_opaque[1],
        (3, BarrierOpacity::UavBarrier)
    );
    assert_eq!(list.submission.barrier_names[2], (3, key_a));
    assert_eq!(list.submission.barrier_count, 3);

    // 4. Aliasing with resource_after records Aliasing; without resource_after records Unknown.
    let b_alias_some = ResourceBarrier {
        r#type: D3D12_RESOURCE_BARRIER_TYPE_ALIASING,
        flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        barrier: ResourceBarrierBody {
            aliasing: ManuallyDrop::new(ResourceAliasingBarrier {
                resource_before: ptr_b,
                resource_after: ptr_a,
            }),
        },
    };
    let b_alias_null = ResourceBarrier {
        r#type: D3D12_RESOURCE_BARRIER_TYPE_ALIASING,
        flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        barrier: ResourceBarrierBody {
            aliasing: ManuallyDrop::new(ResourceAliasingBarrier {
                resource_before: ptr_a,
                resource_after: ptr::null_mut(),
            }),
        },
    };
    // SAFETY: `list` and `b_alias_some` are valid.
    unsafe { resource_barrier((&raw mut *list).cast(), 1, (&raw const b_alias_some).cast()) };
    // SAFETY: `list` and `b_alias_null` are valid.
    unsafe { resource_barrier((&raw mut *list).cast(), 1, (&raw const b_alias_null).cast()) };
    assert_eq!(
        list.submission.barrier_opaque[2],
        (4, BarrierOpacity::Aliasing)
    );
    assert_eq!(
        list.submission.barrier_opaque[3],
        (5, BarrierOpacity::Unknown)
    );
    let _ = key_b;

    // 5. Transitions: each call records its single strongest opacity (`Common` > `Simultaneous` > `Unknown` > `UavState`).
    let transitions = [
        transition_barrier(
            D3D12_RESOURCE_BARRIER_FLAG_NONE,
            ptr_unbacked,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
        ),
        transition_barrier(
            D3D12_RESOURCE_BARRIER_FLAG_NONE,
            ptr_sim,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
        ),
        transition_barrier(
            D3D12_RESOURCE_BARRIER_FLAG_NONE,
            ptr_b,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            D3D12_RESOURCE_STATE_COPY_SOURCE,
        ),
        transition_barrier(
            D3D12_RESOURCE_BARRIER_FLAG_NONE,
            ptr_b,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
        ),
        transition_barrier(
            D3D12_RESOURCE_BARRIER_FLAG_NONE,
            ptr_b,
            D3D12_RESOURCE_STATE_COMMON,
            D3D12_RESOURCE_STATE_COPY_DEST,
        ),
    ];
    for t in &transitions {
        // SAFETY: `list` and `t` are valid.
        unsafe {
            resource_barrier((&raw mut *list).cast(), 1, ptr::from_ref(t).cast());
        }
    }
    assert_eq!(
        &list.submission.barrier_opaque[4..],
        &[
            (6, BarrierOpacity::Unknown),
            (7, BarrierOpacity::Simultaneous),
            (9, BarrierOpacity::UavState),
            (10, BarrierOpacity::Common),
        ]
    );
    assert_eq!(list.submission.barrier_count, 10);

    // 6. Pure write-ends transition (RENDER_TARGET -> PIXEL_SHADER_RESOURCE) on non-simultaneous
    // resource increments barrier count without adding an opaque entry.
    let b_write_ends = transition_barrier(
        D3D12_RESOURCE_BARRIER_FLAG_NONE,
        ptr_a,
        D3D12_RESOURCE_STATE_RENDER_TARGET,
        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
    );
    // SAFETY: `list` and `b_write_ends` are valid.
    unsafe { resource_barrier((&raw mut *list).cast(), 1, (&raw const b_write_ends).cast()) };
    assert_eq!(list.submission.barrier_opaque.len(), 8);
    assert_eq!(list.submission.barrier_count, 11);
}

#[test]
fn barrier_purges_and_clears_discarded_resources() {
    let dev = test_device();
    let info = WMTTextureInfo {
        pixel_format: 70,
        width: 32,
        height: 32,
        depth: 1,
        array_length: 1,
        bits: (WMT_TEXTURE_TYPE2D & 0xff) | (1 << 8) | (1 << 16),
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let tex_a = Arc::new(Texture::new(info, &dev));
    let alloc_a = tex_a.allocate(TextureAllocationFlags::SHARED);
    let _ = tex_a.rename(alloc_a);
    let tex_b = Arc::new(Texture::new(info, &dev));
    let alloc_b = tex_b.allocate(TextureAllocationFlags::SHARED);
    let _ = tex_b.rename(alloc_b);

    let key_a = Arc::as_ptr(&tex_a).cast::<c_void>();
    let key_b = Arc::as_ptr(&tex_b).cast::<c_void>();

    let mut res_a = dummy_resource();
    res_a.texture = Some(tex_a);
    let mut res_b = dummy_resource();
    res_b.texture = Some(tex_b);
    let ptr_a = (&raw mut *res_a).cast::<c_void>();
    let ptr_b = (&raw mut *res_b).cast::<c_void>();

    let mut list = List::new_closed(ptr::null_mut());
    list.discarded_resources = vec![
        DiscardedSubresource {
            tex_ptr: key_a,
            mip: 0,
            slice: 0,
            plane: 0,
            all: true,
        },
        DiscardedSubresource {
            tex_ptr: key_b,
            mip: 0,
            slice: 0,
            plane: 0,
            all: true,
        },
    ];

    // Transition on `res_a` purges `key_a` only, leaving `key_b`.
    let b_tr_a = transition_barrier(
        D3D12_RESOURCE_BARRIER_FLAG_NONE,
        ptr_a,
        D3D12_RESOURCE_STATE_COMMON,
        D3D12_RESOURCE_STATE_RENDER_TARGET,
    );
    // SAFETY: `list` and `b_tr_a` are valid.
    unsafe { resource_barrier((&raw mut *list).cast(), 1, (&raw const b_tr_a).cast()) };
    assert_eq!(list.discarded_resources.len(), 1);
    assert_eq!(list.discarded_resources[0].tex_ptr, key_b);

    // UAV barrier clears all remaining discard entries.
    let b_uav = ResourceBarrier {
        r#type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
        flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        barrier: ResourceBarrierBody {
            uav: ManuallyDrop::new(ResourceUavBarrier { resource: ptr_b }),
        },
    };
    // SAFETY: `list` and `b_uav` are valid.
    unsafe { resource_barrier((&raw mut *list).cast(), 1, (&raw const b_uav).cast()) };
    assert!(list.discarded_resources.is_empty());

    // Aliasing barrier clears all discard entries.
    list.discarded_resources.push(DiscardedSubresource {
        tex_ptr: key_a,
        mip: 0,
        slice: 0,
        plane: 0,
        all: true,
    });
    let b_alias = ResourceBarrier {
        r#type: D3D12_RESOURCE_BARRIER_TYPE_ALIASING,
        flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        barrier: ResourceBarrierBody {
            aliasing: ManuallyDrop::new(ResourceAliasingBarrier {
                resource_before: ptr_a,
                resource_after: ptr_b,
            }),
        },
    };
    // SAFETY: `list` and `b_alias` are valid.
    unsafe { resource_barrier((&raw mut *list).cast(), 1, (&raw const b_alias).cast()) };
    assert!(list.discarded_resources.is_empty());

    // Enhanced `Barrier` with `SYNC_NONE` texture barrier purges matching `key_a` only.
    list.discarded_resources = vec![
        DiscardedSubresource {
            tex_ptr: key_a,
            mip: 0,
            slice: 0,
            plane: 0,
            all: true,
        },
        DiscardedSubresource {
            tex_ptr: key_b,
            mip: 0,
            slice: 0,
            plane: 0,
            all: true,
        },
    ];
    let tex_bar = TextureBarrier {
        sync_before: D3D12_BARRIER_SYNC_NONE,
        sync_after: D3D12_BARRIER_SYNC_ALL,
        access_before: D3D12_BARRIER_ACCESS_COMMON,
        access_after: D3D12_BARRIER_ACCESS_RENDER_TARGET,
        layout_before: D3D12_BARRIER_LAYOUT_COMMON,
        layout_after: D3D12_BARRIER_LAYOUT_RENDER_TARGET,
        resource: ptr_a,
        subresources: BarrierSubresourceRange {
            index_or_first_mip_level: 0,
            num_mip_levels: 1,
            first_array_slice: 0,
            num_array_slices: 1,
            first_plane: 0,
            num_planes: 1,
        },
        flags: 0,
    };
    let tex_group = BarrierGroup {
        r#type: D3D12_BARRIER_TYPE_TEXTURE,
        num_barriers: 1,
        barriers: BarrierGroupBarriers {
            texture_barriers: &raw const tex_bar,
        },
    };
    // SAFETY: `list` and `tex_group` are valid.
    unsafe { barrier((&raw mut *list).cast(), 1, (&raw const tex_group).cast()) };
    assert_eq!(list.discarded_resources.len(), 1);
    assert_eq!(list.discarded_resources[0].tex_ptr, key_b);

    // Enhanced `Barrier` global barrier clears all remaining discard entries.
    let global_bar = GlobalBarrier {
        sync_before: D3D12_BARRIER_SYNC_NONE,
        sync_after: D3D12_BARRIER_SYNC_ALL,
        access_before: D3D12_BARRIER_ACCESS_COMMON,
        access_after: D3D12_BARRIER_ACCESS_COMMON,
    };
    let global_group = BarrierGroup {
        r#type: D3D12_BARRIER_TYPE_GLOBAL,
        num_barriers: 1,
        barriers: BarrierGroupBarriers {
            global_barriers: &raw const global_bar,
        },
    };
    // SAFETY: `list` and `global_group` are valid.
    unsafe { barrier((&raw mut *list).cast(), 1, (&raw const global_group).cast()) };
    assert!(list.discarded_resources.is_empty());
}
