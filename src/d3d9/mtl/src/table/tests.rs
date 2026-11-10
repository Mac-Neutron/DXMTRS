//! Which of winemetal's tables each slot goes through.

use super::takes_table64;
use crate::{SLOTS, wmt};

/// Every slot the facade calls, with the guest's memory slots.
fn slots() -> impl Iterator<Item = u32> {
    SLOTS
        .iter()
        .map(|&(slot, _)| slot)
        .chain([wmt::MEMORY_ALLOC, wmt::MEMORY_FREE])
}

const fn is_sm50(slot: u32) -> bool {
    matches!(slot, wmt::SM50_INITIALIZE..=wmt::SM50_FREE_ERROR)
}

/// A marked 64-bit build calls SM50 through the 64-bit table, every other slot as before.
///
/// It lays out SM50's structs 64-bit; the `WoW64` table converts them from 32-bit ones.
#[test]
fn a_wide_wow64_process_takes_the_64bit_table_for_sm50_alone() {
    for slot in slots() {
        assert_eq!(
            takes_table64(slot, true, true),
            is_sm50(slot),
            "slot {slot}"
        );
    }
}

/// A process not marked, or a 32-bit build (whose structs are 32-bit), keeps today's table.
#[test]
fn an_unmarked_process_or_a_narrow_build_keeps_its_table() {
    for slot in slots() {
        assert!(takes_table64(slot, false, true), "slot {slot}");
        assert!(takes_table64(slot, false, false), "slot {slot}");
        assert!(!takes_table64(slot, true, false), "slot {slot}");
    }
}
