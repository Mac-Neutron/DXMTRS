use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use super::Ring;

/// The 32nd commit waits while 31 chunks are in flight, and goes on once the oldest finishes.
#[test]
fn ring_blocks_at_31_in_flight() {
    let ring = Arc::new(Ring::new(32));
    for _ in 0..31 {
        ring.commit();
    }
    assert_eq!(ring.current(), 32);
    let committed = Arc::new(AtomicBool::new(false));
    let worker = {
        let (ring, committed) = (Arc::clone(&ring), Arc::clone(&committed));
        thread::spawn(move || {
            ring.commit();
            committed.store(true, Ordering::SeqCst);
        })
    };
    thread::sleep(Duration::from_millis(100));
    assert!(!committed.load(Ordering::SeqCst), "31 chunks are in flight");
    assert_eq!(ring.current(), 33, "the chunk is handed on before the wait");
    ring.finish(1);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !committed.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        committed.load(Ordering::SeqCst),
        "chunk 1 finished: there is room again"
    );
    worker.join().unwrap();
    assert_eq!(ring.coherent(), 1);
}
