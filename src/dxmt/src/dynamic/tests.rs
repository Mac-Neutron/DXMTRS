use super::*;

fn names(r: &Renames<u32>) -> Vec<(u32, u64)> {
    r.fifo.iter().map(|(a, at)| (**a, *at)).collect()
}

#[test]
fn dynamic_fifo_frees_in_order() {
    let mut r = Renames::new(Arc::new(0));
    for seq in 1..=3 {
        r.rename(seq, Arc::new(u32::try_from(seq).unwrap_or(0)), 0, false);
    }
    assert_eq!(names(&r), [(0, 1), (1, 2), (2, 3)]);
    assert_eq!(r.take_free(0), None);
    assert_eq!(r.take_free(2).as_deref(), Some(&0));
    assert_eq!(r.take_free(2).as_deref(), Some(&1));
    assert_eq!(r.take_free(2), None);
    assert_eq!(r.take_free(3).as_deref(), Some(&2));
    assert_eq!(*r.name, 3);
}

#[test]
fn command_list_names_come_back_through_recycle() {
    let mut r = Renames::new(Arc::new(0));
    let listed = Arc::new(1);
    // The list's allocation becomes the name; the old name is queued.
    r.rename(4, Arc::clone(&listed), 2, true);
    assert_eq!(names(&r), [(0, 4)]);
    assert_eq!(r.suballocation, 2);
    // Anything else it gives back is queued.
    r.recycle(6, Arc::new(7));
    assert_eq!(names(&r), [(0, 4), (7, 6)]);
    assert!(r.owned_by_command_list);
    // The list gives back the name: kept, not queued.
    r.recycle(5, Arc::clone(&listed));
    assert_eq!(names(&r), [(0, 4), (7, 6)]);
    assert!(!r.owned_by_command_list);
    // Renamed away from a list's allocation, the list still holds it: not queued.
    r.rename(7, Arc::new(8), 0, true);
    r.rename(8, Arc::new(9), 0, false);
    assert_eq!(names(&r), [(0, 4), (7, 6), (1, 7)]);
}

#[test]
fn pieces_as_cpp() {
    let mut r = Renames::new(Arc::new(0));
    // The name gets an id when first asked, and keeps it.
    assert_eq!(r.immediate_piece(), 1);
    assert_eq!(r.immediate_piece(), 1);
    let lent = Arc::new(5);
    assert_eq!(r.lend(Arc::clone(&lent)), 2);
    assert_eq!(r.lend(Arc::new(6)), 3);
    assert_eq!(r.take_lent(2).as_deref(), Some(&5));
    assert_eq!(r.take_lent(2), None);
    // A rename forgets the name's id; the next one asked for is new.
    r.rename(1, lent, 0, false);
    assert_eq!(r.immediate_piece(), 4);
    r.name_piece = 3;
    assert_eq!(r.immediate_piece(), 3);
}
