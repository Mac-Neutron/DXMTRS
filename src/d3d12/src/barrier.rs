//! Legacy resource barriers as enhanced ones: `LegacyBarriers`, `LegacyState`, `InitialState`.
//!
//! `ID3D12GraphicsCommandList7::Barrier` is translated into the legacy barriers that mean the
//! same to DXMT, which orders whole encoders: sync stages, layouts, subresource ranges and the
//! `DISCARD` flag change nothing here. The functions are the C++ header's
//! (`d3d12_barrier_legacy.hpp`) with the same answers for every input; the golden under
//! `barrier/golden` is that header's own output over the truth table.

use core::{ffi::c_void, mem::ManuallyDrop};

use crate::abi::barrier::{
    BarrierGroup, BufferBarrier, D3D12_BARRIER_ACCESS_CONSTANT_BUFFER,
    D3D12_BARRIER_ACCESS_COPY_DEST, D3D12_BARRIER_ACCESS_COPY_SOURCE,
    D3D12_BARRIER_ACCESS_DEPTH_STENCIL_READ, D3D12_BARRIER_ACCESS_DEPTH_STENCIL_WRITE,
    D3D12_BARRIER_ACCESS_INDEX_BUFFER, D3D12_BARRIER_ACCESS_INDIRECT_ARGUMENT,
    D3D12_BARRIER_ACCESS_NO_ACCESS, D3D12_BARRIER_ACCESS_RENDER_TARGET,
    D3D12_BARRIER_ACCESS_RESOLVE_DEST, D3D12_BARRIER_ACCESS_RESOLVE_SOURCE,
    D3D12_BARRIER_ACCESS_SHADER_RESOURCE, D3D12_BARRIER_ACCESS_SHADING_RATE_SOURCE,
    D3D12_BARRIER_ACCESS_STREAM_OUTPUT, D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
    D3D12_BARRIER_ACCESS_VERTEX_BUFFER, D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_COPY_DEST,
    D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_COPY_SOURCE,
    D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_GENERIC_READ,
    D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_SHADER_RESOURCE,
    D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_UNORDERED_ACCESS, D3D12_BARRIER_LAYOUT_COPY_DEST,
    D3D12_BARRIER_LAYOUT_COPY_SOURCE, D3D12_BARRIER_LAYOUT_DEPTH_STENCIL_READ,
    D3D12_BARRIER_LAYOUT_DEPTH_STENCIL_WRITE, D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_COPY_DEST,
    D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_COPY_SOURCE, D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_GENERIC_READ,
    D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_SHADER_RESOURCE,
    D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_UNORDERED_ACCESS, D3D12_BARRIER_LAYOUT_GENERIC_READ,
    D3D12_BARRIER_LAYOUT_RENDER_TARGET, D3D12_BARRIER_LAYOUT_RESOLVE_DEST,
    D3D12_BARRIER_LAYOUT_RESOLVE_SOURCE, D3D12_BARRIER_LAYOUT_SHADER_RESOURCE,
    D3D12_BARRIER_LAYOUT_SHADING_RATE_SOURCE, D3D12_BARRIER_LAYOUT_UNORDERED_ACCESS,
    D3D12_BARRIER_SYNC_NONE, D3D12_BARRIER_TYPE_BUFFER, D3D12_BARRIER_TYPE_GLOBAL,
    D3D12_BARRIER_TYPE_TEXTURE, D3D12_HEAP_TYPE_READBACK, D3D12_HEAP_TYPE_UPLOAD,
    D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, D3D12_RESOURCE_BARRIER_FLAG_NONE,
    D3D12_RESOURCE_BARRIER_TYPE_ALIASING, D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
    D3D12_RESOURCE_BARRIER_TYPE_UAV, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST,
    D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_DEPTH_READ,
    D3D12_RESOURCE_STATE_DEPTH_WRITE, D3D12_RESOURCE_STATE_GENERIC_READ,
    D3D12_RESOURCE_STATE_INDEX_BUFFER, D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
    D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
    D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_RESOLVE_DEST,
    D3D12_RESOURCE_STATE_RESOLVE_SOURCE, D3D12_RESOURCE_STATE_SHADING_RATE_SOURCE,
    D3D12_RESOURCE_STATE_STREAM_OUT, D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
    D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER, GlobalBarrier, ResourceAliasingBarrier,
    ResourceBarrier, ResourceBarrierBody, ResourceTransitionBarrier, ResourceUavBarrier,
    TextureBarrier,
};

/// The accesses that have a legacy resource state, each with it, in the order the C++ walks them.
const STATES: [(u32, u32); 14] = [
    (
        D3D12_BARRIER_ACCESS_VERTEX_BUFFER | D3D12_BARRIER_ACCESS_CONSTANT_BUFFER,
        D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER,
    ),
    (
        D3D12_BARRIER_ACCESS_INDEX_BUFFER,
        D3D12_RESOURCE_STATE_INDEX_BUFFER,
    ),
    (
        D3D12_BARRIER_ACCESS_RENDER_TARGET,
        D3D12_RESOURCE_STATE_RENDER_TARGET,
    ),
    (
        D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
        D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
    ),
    (
        D3D12_BARRIER_ACCESS_DEPTH_STENCIL_WRITE,
        D3D12_RESOURCE_STATE_DEPTH_WRITE,
    ),
    (
        D3D12_BARRIER_ACCESS_DEPTH_STENCIL_READ,
        D3D12_RESOURCE_STATE_DEPTH_READ,
    ),
    (
        D3D12_BARRIER_ACCESS_SHADER_RESOURCE,
        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
    ),
    (
        D3D12_BARRIER_ACCESS_STREAM_OUTPUT,
        D3D12_RESOURCE_STATE_STREAM_OUT,
    ),
    (
        D3D12_BARRIER_ACCESS_INDIRECT_ARGUMENT,
        D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
    ),
    (
        D3D12_BARRIER_ACCESS_COPY_DEST,
        D3D12_RESOURCE_STATE_COPY_DEST,
    ),
    (
        D3D12_BARRIER_ACCESS_COPY_SOURCE,
        D3D12_RESOURCE_STATE_COPY_SOURCE,
    ),
    (
        D3D12_BARRIER_ACCESS_RESOLVE_DEST,
        D3D12_RESOURCE_STATE_RESOLVE_DEST,
    ),
    (
        D3D12_BARRIER_ACCESS_RESOLVE_SOURCE,
        D3D12_RESOURCE_STATE_RESOLVE_SOURCE,
    ),
    (
        D3D12_BARRIER_ACCESS_SHADING_RATE_SOURCE,
        D3D12_RESOURCE_STATE_SHADING_RATE_SOURCE,
    ),
];

/// The accesses that only read: a barrier between two of them orders nothing.
const READS: u32 = D3D12_BARRIER_ACCESS_VERTEX_BUFFER
    | D3D12_BARRIER_ACCESS_CONSTANT_BUFFER
    | D3D12_BARRIER_ACCESS_INDEX_BUFFER
    | D3D12_BARRIER_ACCESS_DEPTH_STENCIL_READ
    | D3D12_BARRIER_ACCESS_SHADER_RESOURCE
    | D3D12_BARRIER_ACCESS_INDIRECT_ARGUMENT
    | D3D12_BARRIER_ACCESS_COPY_SOURCE
    | D3D12_BARRIER_ACCESS_RESOLVE_SOURCE
    | D3D12_BARRIER_ACCESS_SHADING_RATE_SOURCE;

/// The legacy state of an access and whether every bit of it has one.
///
/// An access with no legacy state (acceleration structures, video, sampler feedback, an unknown
/// bit, `NO_ACCESS`) leaves its bits out of the state and answers `false`; `COMMON` (0) is
/// `COMMON`.
#[must_use]
pub const fn legacy_state(access: u32) -> (u32, bool) {
    let mut left = access;
    let mut state = 0;
    let mut row = 0;
    while row < STATES.len() {
        let (bits, legacy) = STATES[row];
        if left & bits != 0 {
            state |= legacy;
            left &= !bits;
        }
        row += 1;
    }
    (state, left == 0)
}

/// Whether an access only reads.
#[must_use]
pub const fn read_only_access(access: u32) -> bool {
    access != 0 && access & !READS == 0
}

/// A barrier naming any resource, which `ResourceBarrier` joins (opacity "unknown").
#[must_use]
pub const fn any_resource_barrier() -> ResourceBarrier {
    ResourceBarrier {
        r#type: D3D12_RESOURCE_BARRIER_TYPE_ALIASING,
        flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        barrier: ResourceBarrierBody {
            aliasing: ManuallyDrop::new(ResourceAliasingBarrier {
                resource_before: core::ptr::null_mut(),
                resource_after: core::ptr::null_mut(),
            }),
        },
    }
}

/// A UAV barrier of `resource` (any resource when it is null).
#[must_use]
pub const fn uav_barrier(resource: *mut c_void) -> ResourceBarrier {
    ResourceBarrier {
        r#type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
        flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        barrier: ResourceBarrierBody {
            uav: ManuallyDrop::new(ResourceUavBarrier { resource }),
        },
    }
}

/// One texture or buffer barrier's legacy barrier.
///
/// Nothing, a UAV barrier, any resource, or a transition of the resource.
fn legacy_resource_barrier(
    sync_before: u32,
    before: u32,
    after: u32,
    resource: *mut c_void,
    out: &mut Vec<ResourceBarrier>,
) {
    if sync_before == D3D12_BARRIER_SYNC_NONE {
        return; // nothing earlier to wait on
    }
    if before == D3D12_BARRIER_ACCESS_NO_ACCESS {
        out.push(any_resource_barrier()); // still a scope to wait for: an execution dependency
        return;
    }
    let (state_before, known_before) = legacy_state(before);
    // No access after: whatever wrote it still has to end, as when it leaves for COMMON.
    let (state_after, known_after) = if after == D3D12_BARRIER_ACCESS_NO_ACCESS {
        (D3D12_RESOURCE_STATE_COMMON, true)
    } else {
        legacy_state(after)
    };
    if !known_before || !known_after {
        out.push(any_resource_barrier());
    } else if before == D3D12_BARRIER_ACCESS_UNORDERED_ACCESS
        && after == D3D12_BARRIER_ACCESS_UNORDERED_ACCESS
    {
        out.push(uav_barrier(resource));
    } else {
        out.push(ResourceBarrier {
            r#type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            barrier: ResourceBarrierBody {
                transition: ManuallyDrop::new(ResourceTransitionBarrier {
                    resource,
                    subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    state_before,
                    state_after,
                }),
            },
        });
    }
}

/// One global barrier's: nothing, a UAV barrier of no resource, or any resource.
fn legacy_global_barrier(barrier: &GlobalBarrier, out: &mut Vec<ResourceBarrier>) {
    let reads = read_only_access(barrier.access_before);
    if barrier.sync_before == D3D12_BARRIER_SYNC_NONE
        || (reads
            && (read_only_access(barrier.access_after)
                || barrier.access_after == D3D12_BARRIER_ACCESS_NO_ACCESS))
    {
        return; // nothing earlier to wait on, or reads then reads (or nothing)
    }
    // UAV writes before, or reads before UAV writes (a write after read, as a legacy move into
    // UNORDERED_ACCESS orders): a UAV barrier; any other write on either side: any resource.
    let uav = barrier.access_before == D3D12_BARRIER_ACCESS_UNORDERED_ACCESS
        || (reads && barrier.access_after == D3D12_BARRIER_ACCESS_UNORDERED_ACCESS);
    out.push(if uav {
        uav_barrier(core::ptr::null_mut())
    } else {
        any_resource_barrier()
    });
}

/// The `index`th element of the application's array of `T`, copied out.
///
/// # Safety
///
/// `array` points to at least `index + 1` readable `T`s; they need no alignment.
const unsafe fn element<T>(array: *const T, index: usize) -> T {
    // SAFETY: the caller's array holds `index + 1` readable `T`s, and `read_unaligned` takes any
    // alignment.
    unsafe { array.wrapping_add(index).read_unaligned() }
}

/// A `Barrier` call's groups as the barriers of one `ResourceBarrier` call.
///
/// `out` is emptied first. A null `groups`, and a null array of a global, texture or buffer
/// group, add nothing; a group of another type adds one any-resource barrier per barrier it
/// counts.
///
/// # Safety
///
/// When `groups` is not null it points to `group_count` readable groups, and each of those whose
/// type is global, texture or buffer points, when its array is not null, to `num_barriers`
/// readable barriers of that type. Neither needs alignment: an application's pointer promises
/// none.
pub unsafe fn legacy_barriers(
    group_count: u32,
    groups: *const BarrierGroup,
    out: &mut Vec<ResourceBarrier>,
) {
    out.clear();
    if groups.is_null() {
        return;
    }
    for g in 0..group_count as usize {
        // SAFETY: `groups` holds `group_count` readable groups.
        let group = unsafe { element(groups, g) };
        let count = group.num_barriers as usize;
        match group.r#type {
            D3D12_BARRIER_TYPE_GLOBAL => {
                // SAFETY: the group's type names its array; every arm of the union is a pointer.
                let array = unsafe { group.barriers.global_barriers };
                if !array.is_null() {
                    for i in 0..count {
                        // SAFETY: a global group's array holds `num_barriers` readable barriers.
                        let b: GlobalBarrier = unsafe { element(array, i) };
                        legacy_global_barrier(&b, out);
                    }
                }
            }
            D3D12_BARRIER_TYPE_TEXTURE => {
                // SAFETY: the group's type names its array; every arm of the union is a pointer.
                let array = unsafe { group.barriers.texture_barriers };
                if !array.is_null() {
                    for i in 0..count {
                        // SAFETY: a texture group's array holds `num_barriers` readable barriers.
                        let b: TextureBarrier = unsafe { element(array, i) };
                        let (sync, before, after) =
                            (b.sync_before, b.access_before, b.access_after);
                        legacy_resource_barrier(sync, before, after, b.resource, out);
                    }
                }
            }
            D3D12_BARRIER_TYPE_BUFFER => {
                // SAFETY: the group's type names its array; every arm of the union is a pointer.
                let array = unsafe { group.barriers.buffer_barriers };
                if !array.is_null() {
                    for i in 0..count {
                        // SAFETY: a buffer group's array holds `num_barriers` readable barriers.
                        let b: BufferBarrier = unsafe { element(array, i) };
                        let (sync, before, after) =
                            (b.sync_before, b.access_before, b.access_after);
                        legacy_resource_barrier(sync, before, after, b.resource, out);
                    }
                }
            }
            _ => out.extend((0..count).map(|_| any_resource_barrier())),
        }
    }
}

/// The state a resource's creation takes, from its initial layout and heap type.
///
/// `ID3D12Device10`'s layout, with an upload heap's `GENERIC_READ` and a readback heap's
/// `COPY_DEST`.
#[must_use]
pub const fn initial_state(layout: u32, heap_type: u32) -> u32 {
    if heap_type == D3D12_HEAP_TYPE_UPLOAD {
        return D3D12_RESOURCE_STATE_GENERIC_READ;
    }
    if heap_type == D3D12_HEAP_TYPE_READBACK {
        return D3D12_RESOURCE_STATE_COPY_DEST;
    }
    match layout {
        D3D12_BARRIER_LAYOUT_GENERIC_READ
        | D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_GENERIC_READ
        | D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_GENERIC_READ => D3D12_RESOURCE_STATE_GENERIC_READ,
        D3D12_BARRIER_LAYOUT_RENDER_TARGET => D3D12_RESOURCE_STATE_RENDER_TARGET,
        D3D12_BARRIER_LAYOUT_UNORDERED_ACCESS
        | D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_UNORDERED_ACCESS
        | D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_UNORDERED_ACCESS => {
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS
        }
        D3D12_BARRIER_LAYOUT_DEPTH_STENCIL_WRITE => D3D12_RESOURCE_STATE_DEPTH_WRITE,
        D3D12_BARRIER_LAYOUT_DEPTH_STENCIL_READ => D3D12_RESOURCE_STATE_DEPTH_READ,
        D3D12_BARRIER_LAYOUT_SHADER_RESOURCE
        | D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_SHADER_RESOURCE
        | D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_SHADER_RESOURCE => {
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE
                | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE
        }
        D3D12_BARRIER_LAYOUT_COPY_SOURCE
        | D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_COPY_SOURCE
        | D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_COPY_SOURCE => D3D12_RESOURCE_STATE_COPY_SOURCE,
        D3D12_BARRIER_LAYOUT_COPY_DEST
        | D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_COPY_DEST
        | D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_COPY_DEST => D3D12_RESOURCE_STATE_COPY_DEST,
        D3D12_BARRIER_LAYOUT_RESOLVE_SOURCE => D3D12_RESOURCE_STATE_RESOLVE_SOURCE,
        D3D12_BARRIER_LAYOUT_RESOLVE_DEST => D3D12_RESOURCE_STATE_RESOLVE_DEST,
        D3D12_BARRIER_LAYOUT_SHADING_RATE_SOURCE => D3D12_RESOURCE_STATE_SHADING_RATE_SOURCE,
        _ => D3D12_RESOURCE_STATE_COMMON, // UNDEFINED, COMMON (PRESENT), the queues' COMMON, video
    }
}

#[cfg(test)]
mod tests;
