use super::BindingSet;

#[test]
fn bind_redundancy_and_dirty_tracking() {
    let mut set: BindingSet<u32, 14> = BindingSet::new();
    assert!(!set.any_bound());
    assert!(!set.any_dirty());

    assert!(set.bind(0, 42));
    assert!(set.test_bound(0));
    assert!(set.test_dirty(0));
    assert_eq!(set.get(0), Some(&42));

    set.clear_dirty();
    assert!(!set.any_dirty());

    // Redundant bind leaves dirty mask clear.
    assert!(!set.bind(0, 42));
    assert!(!set.test_dirty(0));

    // Replacement with new value marks slot dirty again.
    assert!(set.bind(0, 99));
    assert!(set.test_dirty(0));
    assert_eq!(set.get(0), Some(&99));

    // Unbind clears bound, dirty, and slot storage.
    assert!(set.unbind(0));
    assert!(!set.test_bound(0));
    assert!(!set.test_dirty(0));
    assert_eq!(set.get(0), None);
    assert!(!set.unbind(0));
}

#[test]
fn bind_with_handles_slot_127_and_clone() {
    let mut set: BindingSet<(usize, u32), 128> = BindingSet::default();
    let (replacement, entry) = set
        .bind_with(
            127,
            |e| e.0 == 0xdead_beef_usize,
            || (0xdead_beef_usize, 10),
        )
        .expect("slot 127 in bounds");
    assert!(!replacement);
    assert_eq!(entry.1, 10);
    assert!(set.test_bound(127));
    assert!(set.test_dirty(127));

    set.clear_dirty_slot(127);
    assert!(!set.test_dirty(127));

    let (replacement2, entry2) = set
        .bind_with(
            127,
            |e| e.0 == 0xdead_beef_usize,
            || (0xdead_beef_usize, 20),
        )
        .expect("slot 127 in bounds");
    assert!(replacement2);
    entry2.1 = 20;
    assert!(!set.test_dirty(127));

    let cloned = set.clone();
    assert_eq!(cloned.get(127), Some(&(0xdead_beef_usize, 20)));
    assert_eq!(cloned.bound_mask(), 1_u128 << 127);

    set.set_all_dirty();
    assert_eq!(set.dirty_mask(), u128::MAX);
    set.clear();
    assert!(!set.any_bound());
    assert!(!set.any_dirty());
}
