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
            D3D12_RESOURCE_STATE_SHADING_RATE_SOURCE, D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER, ResourceBarrier,
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

const fn read_only(state: u32) -> bool {
    state != D3D12_RESOURCE_STATE_COMMON && (state & !READ_ONLY_STATES) == 0
}

const fn write_ends(state: u32) -> bool {
    state == D3D12_RESOURCE_STATE_RENDER_TARGET
        || state == D3D12_RESOURCE_STATE_DEPTH_WRITE
        || state == D3D12_RESOURCE_STATE_COPY_DEST
        || state == D3D12_RESOURCE_STATE_RESOLVE_DEST
}

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

/// `ID3D12GraphicsCommandList::ResourceBarrier` (`d3d12_command_list.cpp:1352`).
///
/// # Safety
///
/// `this` and `barriers` are caller-supplied pointers.
pub unsafe fn resource_barrier(this: *mut c_void, num_barriers: u32, barriers: *const c_void) {
    crate::scope!("list.ResourceBarrier");
    crate::count!("#resource barriers", u64::from(num_barriers));
    if this.is_null() || (num_barriers > 0 && barriers.is_null()) {
        return;
    }
    // SAFETY: `this` is a non-null `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    let slice = if num_barriers == 0 {
        &[]
    } else {
        // SAFETY: `barriers` points to `num_barriers` contiguous `ResourceBarrier` entries.
        unsafe { slice::from_raw_parts(barriers.cast::<ResourceBarrier>(), num_barriers as usize) }
    };

    let mut join = false;
    let mut opaque: Option<BarrierOpacity> = None;
    list.barrier_scratch.clear();

    for b in slice {
        if join && opaque.is_some_and(|o| o >= BarrierOpacity::ALWAYS) {
            break;
        }
        if (b.flags & D3D12_RESOURCE_BARRIER_FLAG_BEGIN_ONLY) != 0 {
            continue;
        }
        let key = if b.r#type == D3D12_RESOURCE_BARRIER_TYPE_TRANSITION {
            // SAFETY: `b.r#type == D3D12_RESOURCE_BARRIER_TYPE_TRANSITION` selects `transition`.
            let tr = unsafe { &*b.barrier.transition };
            // SAFETY: `tr.resource` is null or a valid `Resource`.
            unsafe { resource_key(tr.resource.cast_const()) }
        } else {
            ptr::null()
        };
        let simultaneous = if key.is_null() || (b.flags & D3D12_RESOURCE_BARRIER_FLAG_END_ONLY) != 0
        {
            false
        } else {
            // SAFETY: `key` is non-null, so `b` is a transition barrier with a non-null `Resource`.
            let tr = unsafe { &*b.barrier.transition };
            // SAFETY: `tr.resource` is a non-null `Resource`.
            let res = unsafe { &*tr.resource.cast::<Resource>() };
            (res.desc_scoped().flags & D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS) != 0
        };
        if key.is_null() || (b.flags & D3D12_RESOURCE_BARRIER_FLAG_END_ONLY) != 0 || simultaneous {
            join = true;
            let activates = b.r#type == D3D12_RESOURCE_BARRIER_TYPE_ALIASING && {
                // SAFETY: `b.r#type == D3D12_RESOURCE_BARRIER_TYPE_ALIASING` selects `aliasing`.
                let al = unsafe { &*b.barrier.aliasing };
                !al.resource_after.is_null()
            };
            let why = if b.r#type == D3D12_RESOURCE_BARRIER_TYPE_UAV {
                BarrierOpacity::UavBarrier
            } else if activates {
                BarrierOpacity::Aliasing
            } else if (b.flags & D3D12_RESOURCE_BARRIER_FLAG_END_ONLY) != 0 {
                BarrierOpacity::SplitEnd
            } else if key.is_null() {
                BarrierOpacity::Unknown
            } else {
                BarrierOpacity::Simultaneous
            };
            opaque = Some(opaque.map_or(why, |prev| prev.max(why)));
            continue;
        }
        // SAFETY: `b.r#type == D3D12_RESOURCE_BARRIER_TYPE_TRANSITION` selects `transition`.
        let tr = unsafe { &*b.barrier.transition };
        let before = tr.state_before;
        let after = tr.state_after;
        if read_only(before) && read_only(after) {
            continue;
        }
        if ((before | after) & D3D12_RESOURCE_STATE_UNORDERED_ACCESS) != 0 {
            opaque = Some(opaque.map_or(BarrierOpacity::UavState, |prev| {
                prev.max(BarrierOpacity::UavState)
            }));
        }
        if before == D3D12_RESOURCE_STATE_COMMON || after == D3D12_RESOURCE_STATE_COMMON {
            opaque = Some(opaque.map_or(BarrierOpacity::Common, |prev| {
                prev.max(BarrierOpacity::Common)
            }));
        }
        if !write_ends(before) || after == D3D12_RESOURCE_STATE_COMMON {
            join = true;
            continue;
        }
        list.barrier_scratch.push(key);
    }

    let call = if list.allocator.is_null() {
        list.submission.barrier_count += 1;
        list.submission.barrier_count
    } else {
        // SAFETY: `list.allocator` is a valid `Allocator` while recording.
        let alloc = unsafe { &mut *list.allocator };
        alloc.barrier(join, &list.barrier_scratch);
        alloc.barrier_calls()
    };
    if let Some(why) = opaque {
        list.submission.barrier_opaque.push((call, why));
    }
    for b in slice {
        match b.r#type {
            D3D12_RESOURCE_BARRIER_TYPE_TRANSITION => {
                // SAFETY: `b.r#type == D3D12_RESOURCE_BARRIER_TYPE_TRANSITION` selects `transition`.
                let tr = unsafe { &*b.barrier.transition };
                // SAFETY: `tr.resource` is null or a valid `Resource`.
                let key = unsafe { resource_key(tr.resource.cast_const()) };
                list.submission.barrier_names.push((call, key));
            }
            D3D12_RESOURCE_BARRIER_TYPE_UAV => {
                // SAFETY: `b.r#type == D3D12_RESOURCE_BARRIER_TYPE_UAV` selects `uav`.
                let uav = unsafe { &*b.barrier.uav };
                // SAFETY: `uav.resource` is null or a valid `Resource`.
                let key = unsafe { resource_key(uav.resource.cast_const()) };
                list.submission.barrier_names.push((call, key));
            }
            _ => {
                // SAFETY: `b.r#type == D3D12_RESOURCE_BARRIER_TYPE_ALIASING` selects `aliasing`.
                let al = unsafe { &*b.barrier.aliasing };
                let before_key = if al.resource_before.is_null() {
                    ptr::null()
                } else {
                    // SAFETY: `al.resource_before` is a non-null `Resource`.
                    unsafe { resource_key(al.resource_before.cast_const()) }
                };
                let after_key = if al.resource_after.is_null() {
                    ptr::null()
                } else {
                    // SAFETY: `al.resource_after` is a non-null `Resource`.
                    unsafe { resource_key(al.resource_after.cast_const()) }
                };
                list.submission.barrier_names.push((call, before_key));
                list.submission.barrier_names.push((call, after_key));
            }
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
    // SAFETY: `this` is a live `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    let mut legacy = core::mem::take(&mut list.legacy_barriers);
    legacy.clear();
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
    // Keep the allocation for the next enhanced barrier on this list.
    list.legacy_barriers = legacy;
}

#[cfg(test)]
mod tests;
