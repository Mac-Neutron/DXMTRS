use std::{collections::BTreeMap, sync::Arc};

use super::{insert_va, lookup_buffer_by_va, lookup_va, remove_va};
use crate::device::Device;

#[test]
fn va_lookup_takes_newest_alias() {
    let dev = Device::new_dummy();
    assert!(lookup_buffer_by_va(&dev, 0x1000).is_none());

    let mut map: BTreeMap<u64, Vec<Arc<u32>>> = BTreeMap::new();
    let old = Arc::new(1_u32);
    let newest = Arc::new(2_u32);
    let higher = Arc::new(3_u32);

    insert_va(&mut map, 0x1000, Arc::clone(&old));
    insert_va(&mut map, 0x1000, Arc::clone(&newest));
    insert_va(&mut map, 0x2000, Arc::clone(&higher));

    // Address before the first entry returns `None`.
    assert!(lookup_va(&map, 0x0fff).is_none());

    // Address inside `[0x1000, 0x2000)` returns the newest alias (`newest`) and byte offset.
    let (found, offset) = lookup_va(&map, 0x1040).expect("entry at 0x1000");
    assert!(Arc::ptr_eq(&found, &newest));
    assert_eq!(offset, 0x40);

    // Removing the newest alias leaves the older alias active at `0x1000`.
    remove_va(&mut map, 0x1000, &newest);
    let (found, offset) = lookup_va(&map, 0x1080).expect("old entry still at 0x1000");
    assert!(Arc::ptr_eq(&found, &old));
    assert_eq!(offset, 0x80);

    // Removing the last alias at `0x1000` removes the key so `0x1080` is no longer mapped.
    remove_va(&mut map, 0x1000, &old);
    assert!(lookup_va(&map, 0x1080).is_none());

    // `0x2000` remains intact.
    let (found, offset) = lookup_va(&map, 0x2010).expect("entry at 0x2000");
    assert!(Arc::ptr_eq(&found, &higher));
    assert_eq!(offset, 0x10);
}

#[test]
fn a_removal_leaves_the_set_before_its_handle_can_die() {
    use super::PendingResidency;
    let mut pending = PendingResidency::default();
    // An addition no flush has sent yet: dropping it is the whole removal.
    pending.add(0xa);
    assert!(!pending.remove(0xa));
    assert_eq!(pending.take_adds(), []);
    // One a flush sent: it leaves the Metal set now, since the caller may release the handle
    // before the next flush (a284b01 queued it there: a dead handle at removeAllocations).
    pending.add(0xb);
    assert_eq!(pending.take_adds(), [0xb]);
    assert!(pending.remove(0xb));
    assert_eq!(pending.take_adds(), []);
    // The removal still needs the next flush's commit.
    assert!(pending.dirty);
}
