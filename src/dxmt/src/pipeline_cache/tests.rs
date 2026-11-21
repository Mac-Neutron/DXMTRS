use core::{
    cell::RefCell,
    sync::atomic::{AtomicUsize, Ordering},
};
use std::{
    sync::{Arc, OnceLock, mpsc},
    thread,
};

use winemetal_sys::Obj;

use super::{CompileState, Compiled, PsoMap};
use crate::scheduler::{CompileLane, CompileScheduler};

thread_local! {
    /// The `NSObject_*` calls this test thread made.
    static CALLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn fake_obj(handle: u64) -> Option<Obj> {
    // SAFETY: `handle` is a non-zero test token; the fakes only log its retain and release.
    unsafe { Obj::adopt(handle) }
}

fn idle(scheduler: &OnceLock<CompileScheduler>) {
    assert!(scheduler.get().expect("a scheduler").wait_idle(5_000));
}

#[test]
fn ready_waits_for_the_job_then_returns_its_handle() {
    let map = PsoMap::<u32, ()>::default();
    let scheduler = OnceLock::new();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let entry = Compiled::pending(Arc::new(()), move |()| {
        started_tx.send(()).expect("the test waits");
        release_rx.recv().expect("the test releases the compile");
        fake_obj(0x5001)
    });
    drop(map.insert_or_existing(1, entry, &scheduler));
    started_rx.recv().expect("the compile starts");

    let hit = map.get(&1).expect("the entry is in the map");
    assert_eq!(hit.handle.load(Ordering::Acquire), 0, "still compiling");
    let releaser = thread::spawn(move || release_tx.send(()).expect("the compile waits"));
    assert_eq!(hit.ready(), Some(0x5001));
    releaser.join().expect("the releaser");
    idle(&scheduler);
}

#[test]
fn a_failed_compile_is_cached_as_none() {
    let map = PsoMap::<u32, ()>::default();
    let scheduler = OnceLock::new();
    let compiles = Arc::new(AtomicUsize::new(0));
    let failing = |compiles: &Arc<AtomicUsize>| {
        let compiles = Arc::clone(compiles);
        Compiled::pending(Arc::new(()), move |()| {
            compiles.fetch_add(1, Ordering::SeqCst);
            None
        })
    };
    let first = map.insert_or_existing(1, failing(&compiles), &scheduler);
    assert_eq!(first.ready(), None);
    let again = map.insert_or_existing(1, failing(&compiles), &scheduler);
    assert!(Arc::ptr_eq(&first, &again));
    assert_eq!(map.get(&1).expect("cached").ready(), None);
    idle(&scheduler);
    assert_eq!(compiles.load(Ordering::SeqCst), 1);
}

#[test]
fn the_second_insert_gets_the_first_entry_and_its_job_never_runs() {
    let map = PsoMap::<u32, ()>::default();
    let scheduler = OnceLock::new();
    let counted = |handle: u64| {
        let compiles = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&compiles);
        let entry = Compiled::pending(Arc::new(()), move |()| {
            counter.fetch_add(1, Ordering::SeqCst);
            fake_obj(handle)
        });
        (entry, compiles)
    };
    let (winner, winner_compiles) = counted(0x5101);
    let (loser, loser_compiles) = counted(0x5102);

    let first = map.insert_or_existing(7, Arc::clone(&winner), &scheduler);
    let second = map.insert_or_existing(7, Arc::clone(&loser), &scheduler);
    assert!(Arc::ptr_eq(&first, &winner));
    assert!(
        Arc::ptr_eq(&second, &winner),
        "the second insert returns the first entry"
    );
    assert_eq!(second.ready(), Some(0x5101));
    idle(&scheduler);
    assert_eq!(winner_compiles.load(Ordering::SeqCst), 1);
    assert_eq!(
        loser_compiles.load(Ordering::SeqCst),
        0,
        "the loser's job never runs"
    );
    assert_eq!(map.len(), 1);
}

#[test]
fn ready_with_starts_no_scheduler() {
    let map = PsoMap::<u32, ()>::default();
    let scheduler = OnceLock::new();
    let entry = map.insert_or_existing(3, Compiled::ready_with(Arc::new(()), 7, None), &scheduler);
    assert_eq!(entry.ready(), Some(7));
    assert!(scheduler.get().is_none(), "no job, no scheduler");
}

#[test]
fn clear_releases_the_psos() {
    crate::fakes::log_object_calls(|op, obj| {
        CALLS.with_borrow_mut(|calls| calls.push(format!("{op} {obj:#x}")));
    });
    let map = PsoMap::<u32, ()>::default();
    let scheduler = OnceLock::new();
    let entry = Compiled::ready_with(Arc::new(()), 0x5201, fake_obj(0x5201));
    drop(map.insert_or_existing(5, entry, &scheduler));
    assert!(!map.is_empty());
    assert!(
        CALLS.with_borrow(Vec::is_empty),
        "nothing released before clear"
    );

    map.clear();
    assert!(map.is_empty());
    assert_eq!(CALLS.with_borrow(Clone::clone), ["NSObject_release 0x5201"]);
}

#[test]
fn insert_ready_keeps_the_first_entry() {
    let map = PsoMap::<u32, ()>::default();
    let first = map.insert_ready(7, Compiled::ready_with(Arc::new(()), 0x7001, None));
    let second = map.insert_ready(7, Compiled::ready_with(Arc::new(()), 0x7002, None));
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(second.ready(), Some(0x7001));
    assert_eq!(map.len(), 1);
}

/// `insert_ready` refuses a pending entry.
///
/// Its job only a scheduler runs: inserted with none, its `ready` would wait forever.
#[test]
#[should_panic(expected = "insert_ready: a pending entry")]
fn insert_ready_refuses_a_pending_entry() {
    let map = PsoMap::<u32, ()>::default();
    drop(map.insert_ready(1, Compiled::pending(Arc::new(()), |()| None)));
}

#[test]
fn try_ready_reports_pending_ready_and_failed() {
    let ready_entry = Compiled::ready_with(Arc::new(()), 0x8001, fake_obj(0x8001));
    assert_eq!(ready_entry.try_ready(), CompileState::Ready(0x8001));

    let failed_entry = Compiled::ready_with(Arc::new(()), 0, None);
    assert_eq!(failed_entry.try_ready(), CompileState::Failed);

    let map = PsoMap::<u32, ()>::default();
    let scheduler = OnceLock::new();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let pending = Compiled::pending(Arc::new(()), move |()| {
        started_tx.send(()).expect("signal start");
        release_rx.recv().expect("release");
        None
    });
    let inserted = map.insert_or_existing_lane(
        9,
        pending,
        &scheduler,
        || CompileScheduler::with_bounds(1, 1),
        CompileLane::Pipeline,
    );
    assert!(inserted.ticket().is_some());
    started_rx.recv().expect("started");
    assert_eq!(inserted.try_ready(), CompileState::Pending);
    release_tx.send(()).expect("release");
    assert_eq!(inserted.ready(), None);
    assert_eq!(inserted.try_ready(), CompileState::Failed);
    idle(&scheduler);
}

#[test]
fn ready_or_steal_runs_unstarted_entry_on_caller_and_ticket_is_visible() {
    let map = PsoMap::<u32, ()>::default();
    let scheduler = OnceLock::new();
    let (blocker_started_tx, blocker_started_rx) = mpsc::channel();
    let (blocker_release_tx, blocker_release_rx) = mpsc::channel::<()>();
    let blocker = Compiled::pending(Arc::new(()), move |()| {
        blocker_started_tx.send(()).expect("blocker start");
        blocker_release_rx.recv().expect("blocker release");
        fake_obj(0x9001)
    });
    drop(map.insert_or_existing_lane(
        1,
        blocker,
        &scheduler,
        || CompileScheduler::with_bounds(1, 1),
        CompileLane::Normal,
    ));
    blocker_started_rx.recv().expect("blocker running");

    let target = Compiled::pending(Arc::new(()), |()| fake_obj(0x9002));
    let inserted = map.insert_or_existing_lane(
        2,
        target,
        &scheduler,
        || CompileScheduler::with_bounds(1, 1),
        CompileLane::Normal,
    );
    assert!(inserted.ticket().is_some());
    assert_eq!(
        map.get(&2).and_then(|e| e.ticket()),
        inserted.ticket(),
        "ticket must be visible immediately after map insertion"
    );

    // Worker is blocked on key 1, so caller must steal and compile key 2 itself.
    assert_eq!(inserted.ready_or_steal(scheduler.get()), Some(0x9002));
    assert_eq!(inserted.try_ready(), CompileState::Ready(0x9002));

    blocker_release_tx.send(()).expect("release blocker");
    idle(&scheduler);
}
