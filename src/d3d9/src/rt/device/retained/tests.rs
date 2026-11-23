use mtld3d_core::ids::BufferId;

use super::*;
use crate::rt::device::fixture::core_on_fakes;

/// Chunks settle oldest first, up to the coherent one, and an add at one chunk joins its entry.
///
/// Would catch a settle that drops a chunk past `coherent` or keeps the coherent one, and a sum
/// out of step with the entries.
#[test]
fn the_ledger_settles_oldest_first() {
    let mut ledger = Retained::default();
    ledger.add(1, 10, 0);
    ledger.add(2, 20, 0);
    ledger.add(2, 5, 0);
    ledger.add(3, 30, 0);
    assert_eq!((ledger.bytes(), ledger.oldest()), (65, Some(1)));
    ledger.settle(1);
    assert_eq!((ledger.bytes(), ledger.oldest()), (55, Some(2)));
    ledger.settle(2);
    assert_eq!((ledger.bytes(), ledger.oldest()), (30, Some(3)));
}

/// An add settles first, so the ledger holds no chunk the GPU has retired.
///
/// Would catch an add that leaves chunk 1 in after chunk 2 is coherent.
#[test]
fn an_add_settles_first() {
    let mut ledger = Retained::default();
    ledger.add(1, 10, 0);
    ledger.add(3, 7, 2);
    assert_eq!((ledger.bytes(), ledger.oldest()), (7, Some(3)));
}

/// An add of no bytes makes no entry: the hold never waits on a chunk that holds nothing.
#[test]
fn an_add_of_no_bytes_makes_no_entry() {
    let mut ledger = Retained::default();
    ledger.add(4, 0, 0);
    assert_eq!((ledger.bytes(), ledger.oldest()), (0, None));
}

/// Settling an empty ledger, or past every chunk, twice, leaves it empty (no underflow).
#[test]
fn settling_past_everything_leaves_nothing() {
    let mut ledger = Retained::default();
    ledger.settle(9);
    ledger.add(5, 3, 0);
    ledger.settle(u64::MAX);
    ledger.settle(u64::MAX);
    assert_eq!((ledger.bytes(), ledger.oldest()), (0, None));
}

/// Uploads past a small cap in the first, uncommitted chunk commit it and wait for it.
///
/// The second upload finds the first's 128 bytes at the 64-byte cap: the chunk being recorded is
/// the oldest, so it is committed (a commit no present asked for) and waited for, and the second
/// upload's bytes are all the ledger holds after it. On the fakes a committed chunk retires at
/// once, so this shows the commit and the coherent sequence, not a wait for a committed chunk.
/// Would catch an upload that does not hold, or adds no bytes.
#[test]
fn uploads_past_the_cap_commit_the_oldest_chunk() {
    const DEVICE: u64 = 0x3e0;
    let mut core = core_on_fakes(DEVICE);
    core.set_retention_cap(64);
    let id = BufferId::from_raw(1);
    assert!(core.buffer_create(id, 256));
    let held = core.device().queue().current_seq();
    assert!(core.buffer_upload(id, 0, &[1; 128]));
    assert_eq!(core.device().queue().current_seq(), held, "under the cap");
    let _ = dxmt_fakes::take_calls(DEVICE);
    assert!(core.buffer_upload(id, 0, &[2; 128]));
    assert_eq!(core.device().queue().current_seq(), held + 1);
    assert!(core.device().queue().coherent_seq() >= held);
    assert_eq!(core.retained.bytes(), 128);
    let calls = dxmt_fakes::take_calls(DEVICE);
    assert!(
        calls.iter().any(|c| c == "MTLCommandBuffer_commit"),
        "{calls:#?}"
    );
    core.drain();
}

/// Under a cap of 0 the same uploads commit nothing, and the ledger still counts them.
#[test]
fn a_cap_of_0_never_commits() {
    const DEVICE: u64 = 0x3e1;
    let mut core = core_on_fakes(DEVICE);
    let id = BufferId::from_raw(1);
    assert!(core.buffer_create(id, 256));
    let held = core.device().queue().current_seq();
    for byte in [1, 2] {
        assert!(core.buffer_upload(id, 0, &[byte; 128]));
    }
    assert_eq!(core.device().queue().current_seq(), held);
    assert_eq!(core.retained.bytes(), 256);
    core.drain();
    let _ = dxmt_fakes::take_calls(DEVICE);
}
