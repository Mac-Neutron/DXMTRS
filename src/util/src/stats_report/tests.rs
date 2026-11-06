//! `stats.txt`'s renderer, as D3D9 feeds it: a counters-only window.
//!
//! dxmt-core's `stats/tests.rs` pin the rest of the format through `super::*`.

use super::*;

#[test]
fn a_counters_only_window() {
    let snapshot = Snapshot {
        ticks: 0,
        threads: Vec::new(),
        counters: [("draws_total".to_owned(), 120)].into(),
    };
    assert_eq!(
        report(&snapshot, 2.0, 120, "every 2 s", [1, 2, 3]),
        b"# 01:02:03 2.0 s, 120 frames (60.0 fps), every 2 s\ncounters:\n  draws_total 120 (1.0 per frame)\n  encoder boundaries 0, 0 with no barrier\n"
    );
}
