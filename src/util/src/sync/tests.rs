use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use super::CpuFence;

/// Polls `flag` for up to ten seconds: whether it turned true.
fn turns_true(flag: &AtomicBool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !flag.load(Ordering::SeqCst) {
        if Instant::now() > deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(1));
    }
    true
}

/// A thread waiting for 2 sleeps through a signal of 1 and wakes on 2.
#[test]
fn wait_wakes_on_another_thread() {
    let fence = Arc::new(CpuFence::new(0));
    let reached = Arc::new(AtomicBool::new(false));
    let waiter = {
        let (fence, reached) = (Arc::clone(&fence), Arc::clone(&reached));
        thread::spawn(move || {
            fence.wait(2);
            reached.store(true, Ordering::SeqCst);
        })
    };
    thread::sleep(Duration::from_millis(50));
    fence.signal(1);
    thread::sleep(Duration::from_millis(50));
    assert!(
        !reached.load(Ordering::SeqCst),
        "1 is below the value waited for"
    );
    assert_eq!(fence.signaled(), 1);
    fence.signal(2);
    assert!(turns_true(&reached), "the signal of 2 wakes the waiter");
    waiter.join().unwrap();
    fence.signal(1);
    assert_eq!(fence.signaled(), 2, "a fence never moves back");
}
