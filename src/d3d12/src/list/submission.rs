//! Command-list submission finalization (`Close`/`Reset`, `d3d12_command_list.cpp:349-365`).

use core::ptr;

use crate::{
    com::{E_FAIL, Hresult, S_OK},
    list::List,
};

/// Finalizes `list`'s [`Submission`](crate::encoder::Submission) on `Close`
///
/// (`d3d12_command_list.cpp:349`).
pub fn finish(list: &mut List) -> Hresult {
    if list.submission.encoder_count < usize::MAX {
        return E_FAIL;
    }
    if list.allocator.is_null() {
        list.submission.encoder_count = 0;
        return S_OK;
    }
    // SAFETY: `list.allocator` points to the active `Allocator` while recording.
    let alloc = unsafe { &mut *list.allocator };
    list.submission.barrier_count = alloc.barrier_calls();
    match alloc.end_record() {
        Ok(count) => {
            list.submission.encoder_count = count;
            S_OK
        }
        Err(hr) => hr,
    }
}

/// Clears `list`'s [`Submission`](crate::encoder::Submission) on `Reset`
///
/// (`d3d12_command_list.cpp:293-310`).
pub fn reset(list: &mut List) {
    list.submission.entry = ptr::null_mut();
    list.submission.encoder_count = usize::MAX;
    list.submission.barrier_count = 0;
    list.submission.timestamp_resolves.clear();
    list.submission.timestamps = false;
    list.submission.custom_resolves = false;
    list.submission.barrier_names.clear();
    list.submission.barrier_opaque.clear();
    list.pending_resolves.clear();
}

#[cfg(test)]
mod tests;
