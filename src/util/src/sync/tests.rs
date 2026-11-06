use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use super::{CpuFence, lock, read, write};

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

/// A mutex a panicking thread poisoned still locks, and the data is as the thread left it.
#[test]
fn lock_ignores_poison() {
    let mutex = Arc::new(std::sync::Mutex::new(2));
    let poisoner = Arc::clone(&mutex);
    let _ = thread::spawn(move || {
        let _guard = poisoner.lock().unwrap();
        panic!("poison the mutex");
    })
    .join();
    assert!(mutex.is_poisoned());
    assert_eq!(*lock(&mutex), 2);
}

/// The same for a reader-writer lock, read and write.
#[test]
fn read_and_write_ignore_poison() {
    let rwlock = Arc::new(std::sync::RwLock::new(2));
    let poisoner = Arc::clone(&rwlock);
    let _ = thread::spawn(move || {
        let _guard = poisoner.write().unwrap();
        panic!("poison the lock");
    })
    .join();
    assert!(rwlock.is_poisoned());
    assert_eq!(*read(&rwlock), 2);
    *write(&rwlock) = 3;
    assert_eq!(*read(&rwlock), 3);
}
