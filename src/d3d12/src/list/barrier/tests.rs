use core::{mem::ManuallyDrop, ptr};
use std::sync::Arc;

use dxmt_core::resource::Buffer;
use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::Obj;

use super::*;
use crate::{
    abi::{
        barrier::{
            D3D12_RESOURCE_BARRIER_FLAG_BEGIN_ONLY, D3D12_RESOURCE_BARRIER_FLAG_END_ONLY,
            D3D12_RESOURCE_BARRIER_FLAG_NONE, D3D12_RESOURCE_BARRIER_TYPE_ALIASING,
            D3D12_RESOURCE_BARRIER_TYPE_TRANSITION, D3D12_RESOURCE_BARRIER_TYPE_UAV,
            D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST,
            D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            ResourceAliasingBarrier, ResourceBarrier, ResourceBarrierBody,
            ResourceTransitionBarrier, ResourceUavBarrier,
        },
        pipeline::SampleDesc,
        resource::{
            D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
            HeapProperties, ResourceDesc,
        },
        vtbl::ID3D12Resource2Vtbl,
    },
    encoder::BarrierOpacity,
    list::List,
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

    // 1. BEGIN_ONLY is ignored completely.
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
    assert_eq!(list.submission.barrier_names.len(), 0);
    assert_eq!(list.submission.barrier_count, 0);

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
        vec![(0, BarrierOpacity::SplitEnd)]
    );
    assert_eq!(list.submission.barrier_count, 1);

    // 3. UAV barrier records UavBarrier and resource pointer in barrier_names.
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
        (1, BarrierOpacity::UavBarrier)
    );
    assert_eq!(list.submission.barrier_names, vec![(1, ptr_a.cast_const())]);
    assert_eq!(list.submission.barrier_count, 2);

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
        (2, BarrierOpacity::Aliasing)
    );
    assert_eq!(
        list.submission.barrier_opaque[3],
        (3, BarrierOpacity::Unknown)
    );

    // 5. Transitions: unbacked -> Unknown, simultaneous -> Simultaneous, read->read ignored,
    // UAV state -> UavState, COMMON -> Common.
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
    // SAFETY: `list` and `transitions` are valid.
    unsafe {
        resource_barrier(
            (&raw mut *list).cast(),
            u32::try_from(transitions.len()).unwrap_or(0),
            transitions.as_ptr().cast(),
        );
    }
    assert_eq!(
        &list.submission.barrier_opaque[4..],
        &[
            (4, BarrierOpacity::Unknown),
            (4, BarrierOpacity::Simultaneous),
            (4, BarrierOpacity::UavState),
            (4, BarrierOpacity::Common),
        ]
    );
    assert_eq!(list.submission.barrier_count, 5);

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
    assert_eq!(list.submission.barrier_count, 6);
}
