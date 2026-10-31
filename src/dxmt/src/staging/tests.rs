use super::*;

fn state() -> State {
    State {
        immediate: 0,
        pool: Vec::new(),
        fifo: VecDeque::new(),
        readable_after: 0,
        writable_after: 0,
    }
}

#[test]
fn staging_state_machine() {
    let mut s = state();
    assert!(s.try_map(0, true, true) == MapResult::Mappable);
    // A copy into it: reads and writes wait for it.
    s.use_copy_destination(5);
    assert!(s.try_map(3, true, false) == MapResult::WaitFor(5));
    assert!(s.try_map(3, false, true) == MapResult::WaitFor(5));
    assert!(s.try_map(5, true, true) == MapResult::Mappable);
    // A copy out of it: reads go ahead, a write renames once the last copy into it finished.
    s.use_copy_source(8);
    assert!(s.try_map(5, true, false) == MapResult::Mappable);
    assert!(s.try_map(5, false, true) == MapResult::Renamable);
    assert!(s.try_map(4, false, true) == MapResult::WaitFor(8));
    assert!(s.try_map(8, true, true) == MapResult::Mappable);
    // The sequences only move forward.
    s.use_copy_destination(2);
    assert!(s.try_map(3, true, false) == MapResult::WaitFor(5));
    assert!(s.try_map(5, true, false) == MapResult::Mappable);
    assert!(s.try_map(5, false, true) == MapResult::Renamable);
    // A rename starts the new allocation clean and queues the old one until `current_seq`.
    s.update_immediate_name(9, 1);
    assert!(s.try_map(0, true, true) == MapResult::Mappable);
    assert_eq!(s.immediate, 1);
    assert_eq!(s.free_id(8), None);
    assert_eq!(s.free_id(9), Some(0));
    assert_eq!(s.free_id(9), None);
}
