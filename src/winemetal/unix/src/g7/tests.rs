use super::*;

use core::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use objc2_core_foundation::{CFRetained, CFRunLoop};

const LIMIT: Duration = Duration::from_secs(5);

/// Starts the listener `h` names on a new thread; the receiver hears when the start returns.
fn start_on_thread(h: u64) -> (thread::JoinHandle<()>, mpsc::Receiver<()>) {
    let (done, returned) = mpsc::channel();
    let t = thread::spawn(move || {
        // SAFETY: a listener the test made and has not destroyed before this start.
        unsafe { Listener::start(Listener::from_handle(h).unwrap()) };
        done.send(()).unwrap();
    });
    (t, returned)
}

/// The listener's first word: what `events.m` polls.
fn first_word(h: u64) -> usize {
    // SAFETY: the listener is alive and its first word an atomic pointer.
    let word = unsafe { &*core::ptr::with_exposed_provenance::<AtomicUsize>(u64_to_usize(h)) };
    word.load(Ordering::Acquire)
}

fn published(h: u64) -> usize {
    for _ in 0..500 {
        let w = first_word(h);
        if w != 0 {
            return w;
        }
        thread::sleep(Duration::from_millis(10));
    }
    0
}

#[test]
fn start_after_destroy_returns() {
    let h = Listener::create();
    // SAFETY: the handle's own destroy.
    unsafe { Listener::destroy(Listener::from_handle(h).unwrap()) };
    let (t, returned) = start_on_thread(h);
    assert!(returned.recv_timeout(LIMIT).is_ok(), "the start ran a loop nobody stops");
    t.join().unwrap();
}

#[test]
fn first_word_is_the_loop_and_a_second_start_returns() {
    let h = Listener::create();
    let (tx, rx) = mpsc::channel();
    let (done, returned) = mpsc::channel();
    let t = thread::spawn(move || {
        tx.send(CFRetained::as_ptr(&CFRunLoop::current().unwrap()).as_ptr().expose_provenance())
            .unwrap();
        // SAFETY: a listener the test made.
        unsafe { Listener::start(Listener::from_handle(h).unwrap()) };
        done.send(()).unwrap();
    });
    let run_loop = rx.recv_timeout(LIMIT).unwrap();
    assert_eq!(published(h), run_loop);
    // SAFETY: as above: the running listener's second start.
    unsafe { Listener::start(Listener::from_handle(h).unwrap()) };
    // SAFETY: the handle's own destroy.
    unsafe { Listener::destroy(Listener::from_handle(h).unwrap()) };
    assert!(returned.recv_timeout(LIMIT).is_ok(), "the destroy did not stop the loop");
    t.join().unwrap();
}

#[test]
fn a_stop_before_the_run_is_not_lost() {
    for _ in 0..100 {
        let h = Listener::create();
        let (t, returned) = start_on_thread(h);
        // SAFETY: the handle's own destroy.
        unsafe { Listener::destroy(Listener::from_handle(h).unwrap()) };
        assert!(returned.recv_timeout(LIMIT).is_ok(), "a start outlived its destroy");
        t.join().unwrap();
    }
}

#[test]
fn a_notification_before_the_start_sets_on_the_loop_thread() {
    let h = Listener::create();
    let listener = Listener::from_handle(h).unwrap();
    // SAFETY: the handle's reference keeps it alive.
    let owner = unsafe { Owner::new(listener) };
    let (tid, loop_tid) = mpsc::channel();
    let (done, returned) = mpsc::channel();
    let t = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        tid.send(thread::current().id()).unwrap();
        // SAFETY: a listener the test made.
        unsafe { Listener::start(Listener::from_handle(h).unwrap()) };
        done.send(()).unwrap();
    });
    let (set, set_on) = mpsc::channel();
    let destroyed = AtomicBool::new(false);
    owner.notify(move || set.send(thread::current().id()).unwrap(), || {
        destroyed.store(true, Ordering::Relaxed);
    });
    assert_eq!(set_on.recv_timeout(LIMIT).unwrap(), loop_tid.recv_timeout(LIMIT).unwrap());
    assert!(!destroyed.load(Ordering::Relaxed));
    drop(owner);
    // SAFETY: the handle's own destroy.
    unsafe { Listener::destroy(listener) };
    assert!(returned.recv_timeout(LIMIT).is_ok());
    t.join().unwrap();
}

#[test]
fn a_notification_after_the_destroy_never_sets() {
    let h = Listener::create();
    let listener = Listener::from_handle(h).unwrap();
    // SAFETY: the handle's reference keeps it alive.
    let owner = unsafe { Owner::new(listener) };
    // SAFETY: the handle's own destroy, then the start (returns at once).
    unsafe { Listener::destroy(listener) };
    // SAFETY: as above.
    unsafe { Listener::start(listener) };
    let set = Arc::new(AtomicBool::new(false));
    let set_seen = Arc::clone(&set);
    let mut destroyed = false;
    owner.notify(move || set_seen.store(true, Ordering::Relaxed), || destroyed = true);
    assert!(destroyed);
    assert!(!set.load(Ordering::Relaxed));
    drop(owner);
}

#[test]
fn a_nil_texture_describes_as_zeros() {
    let mut info = WMTTextureInfo {
        pixel_format: 80,
        width: 1,
        height: 1,
        depth: 1,
        array_length: 1,
        bits: u32::MAX,
        options: 1,
        flags: 1,
        mach_port: 7,
        gpu_resource_id: 9,
    };
    extract_texture_descriptor(None, &mut info);
    assert_eq!(
        (info.pixel_format, info.width, info.height, info.depth, info.array_length, info.bits),
        (0, 0, 0, 0, 0, 0)
    );
    assert_eq!((info.options, info.flags, info.mach_port, info.gpu_resource_id), (0, 0, 7, 9));
}

#[test]
fn texture_bits_cut_each_field_to_eight_bits() {
    let info = WMTTextureInfo {
        pixel_format: 0,
        width: 0,
        height: 0,
        depth: 0,
        array_length: 0,
        bits: texture_bits(2, 0x101, 0x1ff, 5),
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    assert_eq!(
        (info.r#type(), info.mipmap_level_count(), info.sample_count(), info.usage()),
        (2, 1, 0xff, 5)
    );
}
