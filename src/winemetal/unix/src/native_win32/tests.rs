use core::sync::atomic::AtomicU32;
use std::{
    thread,
    time::{Duration, Instant},
};

use super::*;

/// A handle's fields.
fn view<'a>(h: *mut c_void) -> &'a MnWin32Handle {
    assert!(!h.is_null());
    // SAFETY: a live handle the test made and holds while it reads it.
    unsafe { &*h.cast::<MnWin32Handle>() }
}

/// A handle's count, read while no other thread changes it.
fn count_of(h: *mut c_void) -> i32 {
    // SAFETY: no other thread of the test touches the handle during the read.
    unsafe { *view(h).count.get() }
}

fn refs_of(h: *mut c_void) -> i32 {
    view(h).refs.load(Ordering::Relaxed)
}

fn wait(h: *mut c_void, ms: u32) -> u32 {
    // SAFETY: a handle the test holds, or a value the call reads no handle from.
    unsafe { wmt_native_wait_for_single_object(h, ms) }
}

fn set(h: *mut c_void) -> i32 {
    // SAFETY: as in `wait`.
    unsafe { wmt_native_set_event(h) }
}

fn close(h: *mut c_void) -> i32 {
    // SAFETY: as in `wait`; the test gives its reference back.
    unsafe { wmt_native_close_handle(h) }
}

fn duplicate(h: *mut c_void, target: *mut *mut c_void, options: u32) -> i32 {
    // SAFETY: as in `wait`; `target` is null or a pointer to write.
    unsafe { wmt_native_duplicate_handle(h, target, options) }
}

fn release(h: *mut c_void, n: i32, previous: *mut i32) -> i32 {
    // SAFETY: as in `wait`; `previous` is null or an `i32` to write.
    unsafe { wmt_native_release_semaphore(h, n, previous) }
}

fn count(h: *mut c_void) -> u32 {
    // SAFETY: as in `wait`.
    unsafe { wmt_native_handle_count(h) }
}

/// Calls `f` on `h` from another thread, 20 ms from now.
fn later(h: *mut c_void, f: fn(*mut c_void)) -> thread::JoinHandle<()> {
    let addr = h.expose_provenance();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        f(ptr::with_exposed_provenance_mut(addr));
    })
}

/// A handle made as the header makes one: `calloc`, one reference, no descriptor, then `fill`.
fn by_hand(fill: impl FnOnce(&mut MnWin32Handle)) -> *mut c_void {
    let h = new_handle(MN_WIN32_FILE).unwrap();
    fill(h);
    ptr::from_mut(h).cast()
}

/// The calls and ends of a relay, its `ctx`.
fn counters<'a>(ctx: *mut c_void) -> &'a [AtomicU32; 2] {
    // SAFETY: the test's counters, which outlive the relay.
    unsafe { &*ctx.cast::<[AtomicU32; 2]>() }
}

extern "C-unwind" fn relay_call(ctx: *mut c_void) {
    counters(ctx)[0].fetch_add(1, Ordering::Relaxed);
}

extern "C-unwind" fn relay_end(ctx: *mut c_void) {
    counters(ctx)[1].fetch_add(1, Ordering::Relaxed);
}

/// The calling thread's id, cut to a `DWORD` as `GetCurrentThreadId` cuts it.
fn current_thread_id() -> u32 {
    let mut id = 0;
    // SAFETY: the calling thread (0) and an id to write.
    assert_eq!(unsafe { libc::pthread_threadid_np(0, &raw mut id) }, 0);
    crate::float_cast::u64_to_u32(id)
}

#[test]
fn an_auto_reset_event_wakes_an_infinite_wait_and_is_taken() {
    let e = wmt_native_create_event(0, 0);
    let o = view(e);
    assert!(o.kind == MN_WIN32_EVENT && refs_of(e) == 1 && o.max == 1 && count_of(e) == 0);
    let t = later(e, |e| assert_eq!(set(e), 1));
    assert_eq!(wait(e, INFINITE), WAIT_OBJECT_0);
    t.join().unwrap();
    assert_eq!(wait(e, 0), WAIT_TIMEOUT);
    let initial = wmt_native_create_event(0, 7);
    assert_eq!(count_of(initial), 1);
    assert_eq!(wait(initial, 0), WAIT_OBJECT_0);
    assert_eq!(wait(initial, 0), WAIT_TIMEOUT);
    assert_eq!(close(e), 1);
    assert_eq!(close(initial), 1);
}

#[test]
fn a_manual_reset_event_stays_set() {
    let m = wmt_native_create_event(1, 0);
    let t = later(m, |m| assert_eq!(set(m), 1));
    assert_eq!(wait(m, INFINITE), WAIT_OBJECT_0);
    t.join().unwrap();
    assert_eq!(wait(m, 0), WAIT_OBJECT_0);
    assert_eq!(wait(m, 0), WAIT_OBJECT_0);
    assert_eq!(count_of(m), 1);
    assert_eq!(close(m), 1);
}

#[test]
fn a_timed_wait_on_an_unset_event_times_out_at_its_deadline() {
    let e = wmt_native_create_event(0, 0);
    let start = Instant::now();
    assert_eq!(wait(e, 50), WAIT_TIMEOUT);
    assert!(start.elapsed() >= Duration::from_millis(40));
    assert_eq!(close(e), 1);
}

#[test]
fn a_semaphore_counts_to_its_maximum() {
    let s = wmt_native_create_semaphore(1, 2);
    assert!(view(s).kind == MN_WIN32_SEMAPHORE && view(s).max == 2);
    assert_eq!(wait(s, 0), WAIT_OBJECT_0);
    assert_eq!(wait(s, 0), WAIT_TIMEOUT);
    let mut prev = -1;
    assert_eq!(release(s, 1, &raw mut prev), 1);
    assert_eq!(prev, 0);
    assert_eq!(release(s, 1, &raw mut prev), 1);
    assert_eq!(prev, 1);
    prev = -5;
    assert_eq!(release(s, 1, &raw mut prev), 0);
    assert_eq!(release(s, 0, &raw mut prev), 0);
    assert_eq!(release(s, -1, &raw mut prev), 0);
    assert_eq!(prev, -5);
    assert_eq!(wait(s, 0), WAIT_OBJECT_0);
    assert_eq!(release(s, 1, ptr::null_mut()), 1);
    assert_eq!(count_of(s), 2);
    assert_eq!(close(s), 1);

    let empty = wmt_native_create_semaphore(0, 1);
    assert_eq!(wait(empty, 0), WAIT_TIMEOUT);
    let t = later(empty, |s| assert_eq!(release(s, 1, ptr::null_mut()), 1));
    assert_eq!(wait(empty, 2000), WAIT_OBJECT_0);
    t.join().unwrap();
    assert_eq!(wait(empty, 0), WAIT_TIMEOUT);
    assert_eq!(close(empty), 1);
}

#[test]
fn semaphore_limits_and_a_release_on_an_event_are_refused() {
    assert!(wmt_native_create_semaphore(0, 0).is_null());
    assert!(wmt_native_create_semaphore(-1, 1).is_null());
    assert!(wmt_native_create_semaphore(2, 1).is_null());
    assert!(wmt_native_create_semaphore(0, -1).is_null());
    let e = wmt_native_create_event(0, 0);
    let mut prev = -5;
    assert_eq!(release(e, 1, &raw mut prev), 0);
    assert_eq!(prev, -5);
    assert_eq!(close(e), 1);
}

#[test]
fn an_events_duplicate_is_another_reference_and_close_source_gives_it_back() {
    let e = wmt_native_create_event(1, 0);
    let mut d = ptr::null_mut();
    assert_eq!(duplicate(e, &raw mut d, 0), 1);
    assert_eq!(d, e);
    assert_eq!(count(e), 2);
    assert_eq!(close(d), 1);
    assert_eq!(refs_of(e), 1);
    let mut moved = ptr::null_mut();
    assert_eq!(duplicate(e, &raw mut moved, DUPLICATE_CLOSE_SOURCE), 1);
    assert_eq!(moved, e);
    assert_eq!(refs_of(e), 1);
    assert_eq!(close(moved), 1);
}

#[test]
fn a_duplicate_without_a_target_a_source_or_of_a_relay_is_refused() {
    let e = wmt_native_create_event(0, 0);
    let relay = by_hand(|o| {
        o.kind = MN_WIN32_RELAY;
        o.relay = Some(relay_call);
    });
    let untouched = ptr::without_provenance_mut(0x11);
    let mut d = untouched;
    assert_eq!(duplicate(e, ptr::null_mut(), 0), 0);
    assert_eq!(duplicate(relay, &raw mut d, 0), 0);
    assert_eq!(d, untouched);
    assert_eq!(refs_of(e), 1);
    assert_eq!(refs_of(relay), 1);
    assert_eq!(close(e), 1);
    assert_eq!(close(relay), 1);
}

#[test]
fn a_files_duplicate_owns_a_descriptor_of_its_own() {
    let mut fds = [0; 2];
    // SAFETY: two descriptors to write.
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    // SAFETY: the pipe's read end: a read answers EAGAIN, not a wait, while a writer is open.
    let r = unsafe { libc::fcntl(fds[0], libc::F_SETFL, libc::O_NONBLOCK) };
    assert_eq!(r, 0);
    let h = by_hand(|o| o.fd = fds[1]);
    let mut d = ptr::null_mut();
    assert_eq!(duplicate(h, &raw mut d, 0), 1);
    assert!(!d.is_null() && d != h);
    let fd = view(d).fd;
    assert!(view(d).kind == MN_WIN32_FILE && refs_of(d) == 1 && refs_of(h) == 1);
    assert!(fd >= 0 && fd != fds[1]);
    assert_eq!(wait(d, 0), WAIT_FAILED);
    assert_eq!(close(h), 1);
    // SAFETY: the duplicate's descriptor, open until its handle closes, and two bytes to send.
    assert_eq!(unsafe { libc::write(fd, b"ab".as_ptr().cast(), 2) }, 2);
    assert_eq!(close(d), 1);
    let mut buf = [0_u8; 4];
    // SAFETY: the pipe's read end and a buffer of 4 bytes.
    assert_eq!(unsafe { libc::read(fds[0], buf.as_mut_ptr().cast(), 4) }, 2);
    assert_eq!(&buf[..2], b"ab");
    // SAFETY: as above. 0, the end of the pipe: both write ends are closed.
    assert_eq!(unsafe { libc::read(fds[0], buf.as_mut_ptr().cast(), 4) }, 0);
    // SAFETY: the read end, which the test owns.
    unsafe { libc::close(fds[0]) };
}

#[test]
fn a_relays_set_calls_its_function_and_its_last_drop_its_end_once() {
    let calls = [AtomicU32::new(0), AtomicU32::new(0)];
    let ctx = ptr::from_ref(&calls).cast_mut().cast::<c_void>();
    let r = by_hand(|o| {
        o.kind = MN_WIN32_RELAY;
        o.relay = Some(relay_call);
        o.relay_end = Some(relay_end);
        o.relay_ctx = ctx;
    });
    assert_eq!(set(r), 1);
    assert_eq!(calls[0].load(Ordering::Relaxed), 1);
    assert_eq!(wait(r, 0), WAIT_FAILED);
    // SAFETY: a live handle.
    unsafe { wmt_native_ref(r) };
    assert_eq!(count(r), 2);
    assert_eq!(close(r), 1);
    assert_eq!(calls[1].load(Ordering::Relaxed), 0);
    // SAFETY: a live handle, whose last reference the test gives back.
    assert_eq!(unsafe { wmt_native_unref(r) }, 1);
    assert_eq!(calls[0].load(Ordering::Relaxed), 1);
    assert_eq!(calls[1].load(Ordering::Relaxed), 1);
    let plain = by_hand(|o| {
        o.kind = MN_WIN32_RELAY;
        o.relay = Some(relay_call);
    });
    assert_eq!(close(plain), 1);
}

#[test]
fn an_event_held_past_its_close_is_set_then_freed_by_the_hold() {
    let e = wmt_native_create_event(0, 0);
    // SAFETY: a live handle.
    unsafe { wmt_native_ref(e) };
    assert_eq!(close(e), 1);
    assert_eq!(count(e), 1);
    assert_eq!(set(e), 1);
    assert_eq!(wait(e, 0), WAIT_OBJECT_0);
    // SAFETY: a live handle, whose last reference the test gives back.
    assert_eq!(unsafe { wmt_native_unref(e) }, 1);
}

#[test]
fn this_thread_opens_by_its_id_and_nothing_waits_on_it() {
    let tid = current_thread_id();
    let t = wmt_native_open_thread(tid);
    assert!(view(t).kind == MN_WIN32_THREAD && view(t).tid == u64::from(tid) && refs_of(t) == 1);
    assert_eq!(wait(t, 0), WAIT_FAILED);
    assert_eq!(set(t), 0);
    let mut d = ptr::null_mut();
    assert_eq!(duplicate(t, &raw mut d, 0), 1);
    assert_eq!(d, t);
    assert_eq!(refs_of(t), 2);
    assert_eq!(close(d), 1);
    assert_eq!(close(t), 1);
    assert!(wmt_native_open_thread(0).is_null());
    assert!(wmt_native_open_thread(0x7fff_fff1).is_null());
}

#[test]
fn what_is_no_handle_gets_each_calls_refusal() {
    let mut stack = 0_i32;
    let mut prev = -5;
    let untouched = ptr::without_provenance_mut(0x11);
    let mut d = untouched;
    for h in [
        ptr::null_mut(),
        ptr::without_provenance_mut(usize::MAX),
        ptr::without_provenance_mut(usize::MAX - 1),
        (&raw mut stack).cast::<c_void>(),
    ] {
        // SAFETY: a value the call reads no handle from.
        unsafe { wmt_native_ref(h) };
        assert_eq!(set(h), 0);
        // SAFETY: as above.
        assert_eq!(unsafe { wmt_native_unref(h) }, 0);
        assert_eq!(count(h), 0);
        assert_eq!(wait(h, 0), WAIT_FAILED);
        assert_eq!(release(h, 1, &raw mut prev), 0);
        assert_eq!(close(h), i32::from(is_pseudo(h)));
        assert_eq!(duplicate(h, &raw mut d, 0), 0);
    }
    assert_eq!(prev, -5);
    assert_eq!(d, untouched);
    assert_eq!(stack, 0);
}

#[test]
fn the_count_is_the_reference_count() {
    let e = wmt_native_create_event(0, 0);
    assert_eq!(count(e), 1);
    // SAFETY: a live handle.
    unsafe { wmt_native_ref(e) };
    assert!(count(e) == 2 && refs_of(e) == 2);
    // SAFETY: a live handle, which keeps a reference.
    assert_eq!(unsafe { wmt_native_unref(e) }, 1);
    assert_eq!(count(e), 1);
    assert_eq!(close(e), 1);
}

#[test]
fn the_not_implemented_flag_is_the_calling_threads_and_read_once() {
    assert!(!mtld3d_not_implemented());
    wmt_native_not_implemented();
    assert!(!thread::spawn(|| mtld3d_not_implemented()).join().unwrap());
    assert!(MTLD3DNotImplemented());
    assert!(!MTLD3DNotImplemented());
}
