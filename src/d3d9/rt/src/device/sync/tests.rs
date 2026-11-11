use crate::device::fixture::{core_on_fakes, returns_within_10s};

/// `drain` on a chunk nothing was recorded into returns (it would hang waiting on that chunk).
#[test]
fn drain_on_a_clean_chunk_returns() {
    returns_within_10s(|| {
        let mut core = core_on_fakes(0xa8);
        core.drain();
        core.drain();
    });
}
