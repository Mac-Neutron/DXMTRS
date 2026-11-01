//! `ResourceBarrier` and enhanced `Barrier` classification (`d3d12_command_list.cpp:1221, 2423`).

use core::{ffi::c_void, ptr, slice};
use std::sync::Arc;

use crate::{
    abi::{
        barrier::{
            BarrierGroup, D3D12_RESOURCE_BARRIER_FLAG_BEGIN_ONLY,
            D3D12_RESOURCE_BARRIER_FLAG_END_ONLY, D3D12_RESOURCE_BARRIER_TYPE_ALIASING,
            D3D12_RESOURCE_BARRIER_TYPE_TRANSITION, D3D12_RESOURCE_BARRIER_TYPE_UAV,
            D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST,
            D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_DEPTH_READ,
            D3D12_RESOURCE_STATE_DEPTH_WRITE, D3D12_RESOURCE_STATE_INDEX_BUFFER,
            D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_RESOLVE_DEST, D3D12_RESOURCE_STATE_RESOLVE_SOURCE,
            D3D12_RESOURCE_STATE_SHADING_RATE_SOURCE, D3D12_RESOURCE_STATE_STREAM_OUT,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER,
            ResourceBarrier,
        },
        resource::D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
    },
    encoder::BarrierOpacity,
    list::List,
    resource::Resource,
};

const READ_ONLY_STATES: u32 = D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER
    | D3D12_RESOURCE_STATE_INDEX_BUFFER
    | D3D12_RESOURCE_STATE_DEPTH_READ
    | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE
    | D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE
    | D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT
    | D3D12_RESOURCE_STATE_COPY_SOURCE
    | D3D12_RESOURCE_STATE_RESOLVE_SOURCE
    | D3D12_RESOURCE_STATE_SHADING_RATE_SOURCE;

const WRITE_ONLY_STATES: u32 = D3D12_RESOURCE_STATE_RENDER_TARGET
    | D3D12_RESOURCE_STATE_DEPTH_WRITE
    | D3D12_RESOURCE_STATE_STREAM_OUT
    | D3D12_RESOURCE_STATE_COPY_DEST
    | D3D12_RESOURCE_STATE_RESOLVE_DEST;

/// `ResourceKey` (`d3d12_command_list.cpp:138-146`).
///
/// # Safety
///
/// If non-null, `raw` must point to a valid [`Resource`].
unsafe fn resource_key(raw: *const c_void) -> *const c_void {
    if raw.is_null() {
        return ptr::null();
    }
    // SAFETY: `raw` is non-null and points to a valid `Resource`.
    let r = unsafe { &*raw.cast::<Resource>() };
    if let Some(tex) = r.texture.as_ref() {
        return Arc::as_ptr(tex).cast::<c_void>();
    }
    if let Some(buf) = r.buffer.as_ref() {
        return Arc::as_ptr(buf).cast::<c_void>();
    }
    ptr::null()
}

/// `ID3D12GraphicsCommandList::ResourceBarrier` (`d3d12_command_list.cpp:1221`).
///
/// # Safety
///
/// `this` and `barriers` are caller-supplied pointers.
pub unsafe fn resource_barrier(this: *mut c_void, num_barriers: u32, barriers: *const c_void) {
    crate::scope!("list.ResourceBarrier");
    crate::count!("#resource barriers", u64::from(num_barriers));
    if this.is_null() || num_barriers == 0 || barriers.is_null() {
        return;
    }
    // SAFETY: `this` is a non-null `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    let barrier_idx = if list.allocator.is_null() {
        list.submission.barrier_count
    } else {
        // SAFETY: `list.allocator` is a valid `Allocator` while recording.
        unsafe { (*list.allocator).barrier_calls() }
    };
    // SAFETY: `barriers` points to `num_barriers` contiguous `ResourceBarrier` entries.
    let slice =
        unsafe { slice::from_raw_parts(barriers.cast::<ResourceBarrier>(), num_barriers as usize) };

    let mut join = false;
    let mut transitioned: Vec<*const c_void> = Vec::new();

    for b in slice {
        if (b.flags & D3D12_RESOURCE_BARRIER_FLAG_BEGIN_ONLY) != 0 {
            continue;
        }
        if (b.flags & D3D12_RESOURCE_BARRIER_FLAG_END_ONLY) != 0 {
            list.submission
                .barrier_opaque
                .push((barrier_idx, BarrierOpacity::SplitEnd));
            join = true;
            continue;
        }
        match b.r#type {
            D3D12_RESOURCE_BARRIER_TYPE_UAV => {
                // SAFETY: `b.r#type == D3D12_RESOURCE_BARRIER_TYPE_UAV` selects the `uav` union arm.
                let uav = unsafe { &*b.barrier.uav };
                if !uav.resource.is_null() {
                    list.submission
                        .barrier_names
                        .push((barrier_idx, uav.resource.cast_const()));
                }
                list.submission
                    .barrier_opaque
                    .push((barrier_idx, BarrierOpacity::UavBarrier));
                join = true;
            }
            D3D12_RESOURCE_BARRIER_TYPE_ALIASING => {
                // SAFETY: `b.r#type == D3D12_RESOURCE_BARRIER_TYPE_ALIASING` selects `aliasing`.
                let aliasing = unsafe { &*b.barrier.aliasing };
                if !aliasing.resource_before.is_null() {
                    list.submission
                        .barrier_names
                        .push((barrier_idx, aliasing.resource_before.cast_const()));
                }
                if !aliasing.resource_after.is_null() {
                    list.submission
                        .barrier_names
                        .push((barrier_idx, aliasing.resource_after.cast_const()));
                }
                let opacity = if aliasing.resource_after.is_null() {
                    BarrierOpacity::Unknown
                } else {
                    BarrierOpacity::Aliasing
                };
                list.submission.barrier_opaque.push((barrier_idx, opacity));
                join = true;
            }
            D3D12_RESOURCE_BARRIER_TYPE_TRANSITION => {
                // SAFETY: `b.r#type == D3D12_RESOURCE_BARRIER_TYPE_TRANSITION` selects `transition`.
                let tr = unsafe { &*b.barrier.transition };
                if tr.state_before == tr.state_after {
                    continue;
                }
                if !tr.resource.is_null() {
                    list.submission
                        .barrier_names
                        .push((barrier_idx, tr.resource.cast_const()));
                }
                // SAFETY: `tr.resource` is null or a valid `Resource`.
                let key = unsafe { resource_key(tr.resource.cast_const()) };
                if key.is_null() {
                    list.submission
                        .barrier_opaque
                        .push((barrier_idx, BarrierOpacity::Unknown));
                    join = true;
                    continue;
                }
                // SAFETY: `key` is non-null, so `tr.resource` is a non-null `Resource`.
                let res = unsafe { &*tr.resource.cast::<Resource>() };
                if (res.desc.flags & D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS) != 0 {
                    list.submission
                        .barrier_opaque
                        .push((barrier_idx, BarrierOpacity::Simultaneous));
                    join = true;
                    continue;
                }
                let before_read = tr.state_before != D3D12_RESOURCE_STATE_COMMON
                    && (tr.state_before & !READ_ONLY_STATES) == 0;
                let after_read = tr.state_after != D3D12_RESOURCE_STATE_COMMON
                    && (tr.state_after & !READ_ONLY_STATES) == 0;
                if before_read && after_read {
                    continue;
                }
                let before_write = tr.state_before != D3D12_RESOURCE_STATE_COMMON
                    && (tr.state_before & !WRITE_ONLY_STATES) == 0;
                let after_write = tr.state_after != D3D12_RESOURCE_STATE_COMMON
                    && (tr.state_after & !WRITE_ONLY_STATES) == 0;
                if ((tr.state_before | tr.state_after) & D3D12_RESOURCE_STATE_UNORDERED_ACCESS) != 0
                {
                    list.submission
                        .barrier_opaque
                        .push((barrier_idx, BarrierOpacity::UavState));
                }
                if tr.state_before == D3D12_RESOURCE_STATE_COMMON
                    || tr.state_after == D3D12_RESOURCE_STATE_COMMON
                {
                    list.submission
                        .barrier_opaque
                        .push((barrier_idx, BarrierOpacity::Common));
                }
                if before_write && (after_read || after_write) {
                    transitioned.push(key);
                } else {
                    join = true;
                }
            }
            _ => {}
        }
    }

    if !transitioned.is_empty() || join {
        if list.allocator.is_null() {
            list.submission.barrier_count += 1;
        } else {
            // SAFETY: `list.allocator` is a valid `Allocator` while recording.
            let alloc = unsafe { &mut *list.allocator };
            alloc.barrier(join, &transitioned);
        }
    }
}

/// `ID3D12GraphicsCommandList7::Barrier` (`d3d12_command_list.cpp:2423`).
///
/// # Safety
///
/// `this` and `barrier_groups` are caller-supplied pointers.
pub unsafe fn barrier(this: *mut c_void, num_barrier_groups: u32, barrier_groups: *const c_void) {
    crate::scope!("list.Barrier");
    if this.is_null() || num_barrier_groups == 0 || barrier_groups.is_null() {
        return;
    }
    let mut legacy = Vec::new();
    // SAFETY: `barrier_groups` points to `num_barrier_groups` `BarrierGroup` entries whose nested
    // barrier arrays are valid for the duration of the call.
    unsafe {
        crate::barrier::legacy_barriers(
            num_barrier_groups,
            barrier_groups.cast::<BarrierGroup>(),
            &mut legacy,
        );
    }
    for b in &legacy {
        // SAFETY: `this` is valid and `b` is a valid `ResourceBarrier`.
        unsafe { resource_barrier(this, 1, (&raw const *b).cast()) };
    }
}

#[cfg(test)]
mod tests;
