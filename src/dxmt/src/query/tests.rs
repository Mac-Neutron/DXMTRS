use core::sync::atomic::AtomicU64;
use std::sync::Arc;

use super::*;

#[test]
fn visibility_offsets_bump() {
    let mut o = VisibilityOffsets::new();
    // Outside a pass the read offset is the next result's.
    assert_eq!(o.next_read(), 0);
    o.begin_pass();
    // A pass starts with counting off: no record to turn it off again.
    assert_eq!(o.next_write(false), None);
    // A query is active: the draws count into result 0, said once.
    assert_eq!(o.next_write(true), Some(0));
    assert_eq!(o.next_write(true), None);
    // A read after draws wrote moves to the next result, once.
    assert_eq!(o.next_read(), 1);
    assert_eq!(o.next_read(), 1);
    assert_eq!(o.next_write(true), Some(1));
    o.end_pass();
    // The pass wrote result 1: the next pass starts at 2.
    assert_eq!(o.next_read(), 2);
    o.begin_pass();
    // Each pass says its offset again, and turns counting off when no query is active.
    assert_eq!(o.next_write(true), Some(2));
    assert_eq!(o.next_write(false), Some(NO_OFFSET));
    o.end_pass();
    // The chunk's results: 0 to 2. The next chunk starts at 0.
    assert_eq!(o.reset(), 3);
    assert_eq!(o.next_read(), 0);
    o.begin_pass();
    o.end_pass();
    // A pass without draws into a result writes none.
    assert_eq!(o.reset(), 0);
}

#[test]
fn readbacks_sum_per_query() {
    let mut q = Queries::new();
    let a = Arc::new(VisibilityQuery::new());
    let b = Arc::new(VisibilityQuery::new());
    let empty = Arc::new(VisibilityQuery::new());

    // Chunk 1: A begins and a pass draws into result 0.
    q.begin_pass();
    q.begin_visibility(1, Arc::clone(&a));
    assert_eq!(
        q.visibility_mode().map(|c| (c.mode, c.offset)),
        Some((2, 0))
    );
    // A query that ends where it began, with no draw between, is ready at once.
    q.begin_visibility(1, Arc::clone(&empty));
    q.end_visibility(1, &empty);
    assert_eq!(empty.value(), Some(0));
    q.end_pass();
    let (count, queries) = q.take_visibility(1).unwrap();
    assert_eq!((count, queries.len()), (1, 2));
    // A is still active, so it stays pending for the next chunk's readback.
    for query in &queries {
        query.issue(1, &[5]);
    }
    assert_eq!(a.value(), None);

    // Chunk 2: A goes on into result 0, B begins at result 1, A ends at 2, B at 3.
    q.begin_pass();
    assert!(q.visibility_active());
    assert_eq!(q.visibility_mode().map(|c| c.offset), Some(0));
    q.begin_visibility(2, Arc::clone(&b));
    assert_eq!(q.visibility_mode().map(|c| c.offset), Some(8));
    q.end_visibility(2, &a);
    assert_eq!(q.visibility_mode().map(|c| c.offset), Some(16));
    q.end_visibility(2, &b);
    assert!(!q.visibility_active());
    assert_eq!(
        q.visibility_mode().map(|c| (c.mode, c.offset)),
        Some((0, 0))
    );
    q.end_pass();
    let (count, queries) = q.take_visibility(2).unwrap();
    assert_eq!((count, queries.len()), (3, 2));
    for query in &queries {
        query.issue(2, &[1, 10, 100]);
    }
    assert_eq!(a.value(), Some(5 + 1 + 10));
    assert_eq!(b.value(), Some(10 + 100));
    // Both ended in chunk 2: nothing is pending, and a chunk without results has no readback.
    assert!(q.take_visibility(3).is_none());
}

#[test]
fn timestamps_never_move_back() {
    let mut t = Queries::new();
    let (x, y) = (
        Arc::new(TimestampQuery::new()),
        Arc::new(TimestampQuery::new()),
    );
    t.sample_timestamp(Arc::clone(&x));
    t.sample_timestamp(Arc::clone(&y));
    let taken = t.take_timestamps();
    assert_eq!(taken.len(), 2);
    assert!(t.take_timestamps().is_empty());
    assert_eq!(x.value(), None);
    // A command buffer may end before an earlier one: the later query keeps the latest time.
    let latest = AtomicU64::new(0);
    issue_timestamps(&taken, &latest, 50);
    assert_eq!((x.value(), y.value()), (Some(50), Some(50)));
    issue_timestamps(&taken[..1], &latest, 30);
    assert_eq!(x.value(), Some(50));
    issue_timestamps(&taken[1..], &latest, 70);
    assert_eq!(y.value(), Some(70));
}
