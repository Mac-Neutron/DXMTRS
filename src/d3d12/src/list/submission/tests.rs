use core::ptr;

use super::*;
use crate::{
    com::{E_FAIL, S_OK},
    encoder::{BarrierOpacity, TimestampResolve},
    list::List,
};

#[test]
fn fields_as_cpp() {
    let mut list = List::new_closed(ptr::null_mut());

    // Reset initializes the recording submission state (`encoder_count = usize::MAX`).
    reset(&mut list);
    assert!(list.submission.entry.is_null());
    assert_eq!(list.submission.encoder_count, usize::MAX);
    assert_eq!(list.submission.barrier_count, 0);
    assert!(!list.submission.timestamps);
    assert!(!list.submission.custom_resolves);
    assert_eq!(list.submission.timestamp_resolves.len(), 0);
    assert_eq!(list.submission.barrier_names.len(), 0);
    assert_eq!(list.submission.barrier_opaque.len(), 0);

    // Populate fields as command recording does.
    list.submission.timestamps = true;
    list.submission.custom_resolves = true;
    list.submission.timestamp_resolves.push(TimestampResolve {
        samples: 1,
        start: 0,
        count: 2,
        dst: ptr::null_mut(),
        aliases: vec![(0, 1)],
        values: ptr::null(),
    });
    list.submission.barrier_names.push((0, ptr::null()));
    list.submission
        .barrier_opaque
        .push((0, BarrierOpacity::UavBarrier));

    // First `finish` succeeds and marks the submission closed; a second `finish` returns `E_FAIL`.
    assert_eq!(finish(&mut list), S_OK);
    assert!(list.submission.encoder_count < usize::MAX);
    assert_eq!(finish(&mut list), E_FAIL);

    // `reset` clears all submission vectors and flags and re-opens recording.
    reset(&mut list);
    assert!(list.submission.entry.is_null());
    assert_eq!(list.submission.encoder_count, usize::MAX);
    assert_eq!(list.submission.barrier_count, 0);
    assert!(!list.submission.timestamps);
    assert!(!list.submission.custom_resolves);
    assert_eq!(list.submission.timestamp_resolves.len(), 0);
    assert_eq!(list.submission.barrier_names.len(), 0);
    assert_eq!(list.submission.barrier_opaque.len(), 0);
}
