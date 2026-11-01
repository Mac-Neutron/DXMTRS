use core::{
    cell::RefCell,
    ffi::c_void,
    sync::atomic::{AtomicU64, Ordering},
};
use std::{
    sync::{Condvar, LazyLock, Mutex, PoisonError},
    thread,
    time::Duration,
};

use rustc_hash::FxHashMap;
use winemetal_sys::Obj;

use super::{EventListener, Fence, Handle};

const DEVICE: u64 = 0x0d00;
const HELPER_QUEUE: u64 = 0x0e00;

struct ThreadState {
    next_shared: u64,
    next_gpu: u64,
    next_cmdbuf: u64,
    signaled: FxHashMap<u64, u64>,
    log: Vec<String>,
}

impl ThreadState {
    fn new() -> Self {
        Self {
            next_shared: 0x1001,
            next_gpu: 0x2001,
            next_cmdbuf: 0x3001,
            signaled: FxHashMap::default(),
            log: Vec::new(),
        }
    }
}

thread_local! {
    static STATE: RefCell<ThreadState> = RefCell::new(ThreadState::new());
}

#[derive(Default)]
struct ListenerRecord {
    started: bool,
    destroyed: bool,
    thread_name: String,
    events: Vec<&'static str>,
}

/// Per-listener state shared between the test thread and the `dxmt-event-list` thread.
///
/// A static: the winemetal listener C symbols (`SharedEventListener_*`) take only the listener's
/// `u64` handle across threads.
static LISTENERS: LazyLock<(Mutex<FxHashMap<u64, ListenerRecord>>, Condvar)> =
    LazyLock::new(|| (Mutex::new(FxHashMap::default()), Condvar::new()));

/// Monotonic handle source for `SharedEventListener_create`.
///
/// A static: gives each test's listener a distinct handle in `LISTENERS`.
static NEXT_LISTENER: AtomicU64 = AtomicU64::new(0x4001);

fn push_log(line: String) {
    STATE.with_borrow_mut(|state| state.log.push(line));
}

fn take_log() -> Vec<String> {
    STATE.with_borrow_mut(|state| core::mem::take(&mut state.log))
}

fn reset() {
    STATE.with_borrow_mut(|state| *state = ThreadState::new());
    crate::fakes::log_object_calls(|op, obj| {
        push_log(format!("{op} {obj:#x}"));
    });
}

fn fake_obj(handle: u64) -> Obj {
    // SAFETY: `handle` is a non-zero test token that `fakes::log_object_calls` tracks.
    unsafe { Obj::adopt(handle) }.expect("non-zero test handle")
}

#[unsafe(export_name = "MTLDevice_newSharedEvent")]
extern "C-unwind" fn fake_new_shared_event(device: u64) -> u64 {
    STATE.with_borrow_mut(|state| {
        let id = state.next_shared;
        state.next_shared += 1;
        state.signaled.insert(id, 0);
        state.log.push(format!(
            "MTLDevice_newSharedEvent device={device:#x} -> {id:#x}"
        ));
        id
    })
}

#[unsafe(export_name = "MTLDevice_newEvent")]
extern "C-unwind" fn fake_new_event(device: u64) -> u64 {
    STATE.with_borrow_mut(|state| {
        let id = state.next_gpu;
        state.next_gpu += 1;
        state
            .log
            .push(format!("MTLDevice_newEvent device={device:#x} -> {id:#x}"));
        id
    })
}

#[unsafe(export_name = "MTLSharedEvent_signaledValue")]
extern "C-unwind" fn fake_signaled_value(event: u64) -> u64 {
    STATE.with_borrow(|state| state.signaled.get(&event).copied().unwrap_or(0))
}

#[unsafe(export_name = "MTLSharedEvent_signalValue")]
extern "C-unwind" fn fake_signal_value(event: u64, value: u64) {
    STATE.with_borrow_mut(|state| {
        state.signaled.insert(event, value);
        state.log.push(format!(
            "MTLSharedEvent_signalValue event={event:#x} value={value}"
        ));
    });
}

#[unsafe(export_name = "MTLSharedEvent_waitUntilSignaledValue")]
extern "C-unwind" fn fake_wait_until_signaled_value(event: u64, value: u64, timeout: u64) -> bool {
    STATE.with_borrow_mut(|state| {
        let current = state.signaled.get(&event).copied().unwrap_or(0);
        state.log.push(format!(
            "MTLSharedEvent_waitUntilSignaledValue event={event:#x} value={value} timeout={timeout}"
        ));
        current >= value
    })
}

#[unsafe(export_name = "MTLCommandQueue_commandBuffer")]
extern "C-unwind" fn fake_command_buffer(queue: u64) -> u64 {
    STATE.with_borrow_mut(|state| {
        let id = state.next_cmdbuf;
        state.next_cmdbuf += 1;
        state.log.push(format!(
            "MTLCommandQueue_commandBuffer queue={queue:#x} -> {id:#x}"
        ));
        id
    })
}

#[unsafe(export_name = "MTLCommandBuffer_encodeSignalEvent")]
extern "C-unwind" fn fake_encode_signal_event(cmdbuf: u64, event: u64, value: u64) {
    push_log(format!(
        "MTLCommandBuffer_encodeSignalEvent cmdbuf={cmdbuf:#x} event={event:#x} value={value}"
    ));
}

#[unsafe(export_name = "MTLCommandBuffer_commit")]
extern "C-unwind" fn fake_commit(cmdbuf: u64) {
    push_log(format!("MTLCommandBuffer_commit cmdbuf={cmdbuf:#x}"));
}

#[unsafe(export_name = "MTLSharedEvent_setWin32EventAtValue")]
extern "C-unwind" fn fake_set_win32_event_at_value(
    event: u64,
    listener: u64,
    nt_event: *mut c_void,
    value: u64,
) {
    push_log(format!(
        "MTLSharedEvent_setWin32EventAtValue shared={event:#x} listener={listener:#x} event={:#x} value={value}",
        nt_event.addr()
    ));
}

#[unsafe(export_name = "MTLSharedEvent_setWin32EventAtValues")]
extern "C-unwind" fn fake_set_win32_event_at_values(
    listener: u64,
    events: *const u64,
    values: *const u64,
    count: u32,
    needed: u32,
    nt_event: *mut c_void,
) {
    let n = usize::try_from(count).expect("count fits in usize");
    // SAFETY: `set_event_on_values` passes `events` with `count` elements.
    let events_slice = unsafe { core::slice::from_raw_parts(events, n) };
    // SAFETY: `set_event_on_values` passes `values` with `count` elements.
    let values_slice = unsafe { core::slice::from_raw_parts(values, n) };
    let formatted_events: Vec<String> = events_slice.iter().map(|h| format!("{h:#x}")).collect();
    push_log(format!(
        "MTLSharedEvent_setWin32EventAtValues listener={listener:#x} events=[{}] values={values_slice:?} count={count} needed={needed} event={:#x}",
        formatted_events.join(", "),
        nt_event.addr()
    ));
}

#[unsafe(export_name = "SharedEventListener_create")]
extern "C-unwind" fn fake_listener_create() -> u64 {
    let id = NEXT_LISTENER.fetch_add(1, Ordering::Relaxed);
    let (lock, _) = &*LISTENERS;
    let mut map = lock.lock().unwrap_or_else(PoisonError::into_inner);
    map.insert(
        id,
        ListenerRecord {
            events: vec!["create"],
            ..ListenerRecord::default()
        },
    );
    id
}

#[unsafe(export_name = "SharedEventListener_start")]
extern "C-unwind" fn fake_listener_start(listener: u64) {
    let name = thread::current().name().unwrap_or("").to_owned();
    let (lock, cvar) = &*LISTENERS;
    let mut map = lock.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(rec) = map.get_mut(&listener) {
        rec.started = true;
        rec.thread_name = name;
        rec.events.push("start");
    }
    cvar.notify_all();
    while !map.get(&listener).is_some_and(|rec| rec.destroyed) {
        map = cvar.wait(map).unwrap_or_else(PoisonError::into_inner);
    }
    drop(map);
    // Pause briefly after destroy so a Drop implementation that forgets to wait for the thread
    // returns before "exit" is recorded.
    thread::sleep(Duration::from_millis(5));
    let mut map = lock.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(rec) = map.get_mut(&listener) {
        rec.events.push("exit");
    }
}

#[unsafe(export_name = "SharedEventListener_destroy")]
extern "C-unwind" fn fake_listener_destroy(listener: u64) {
    let (lock, cvar) = &*LISTENERS;
    let mut map = lock.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(rec) = map.get_mut(&listener) {
        rec.destroyed = true;
        rec.events.push("destroy");
    }
    drop(map);
    cvar.notify_all();
}

#[test]
fn lower_value_starts_a_generation() {
    reset();
    let device = fake_obj(DEVICE);
    let helper_queue = fake_obj(HELPER_QUEUE);
    let fence = Fence::new(&device, &helper_queue);

    let first = fence.current();
    assert_eq!(first.shared.handle(), 0x1001);
    assert_eq!(first.gpu.handle(), 0x2001);

    fence.signal(10);
    assert_eq!(fence.completed_value(), 10);

    // Asking for an equal or higher value keeps the current generation.
    let same_equal = fence.ask(10);
    assert_eq!(same_equal.shared.handle(), 0x1001);
    assert_eq!(same_equal.gpu.handle(), 0x2001);

    let same_higher = fence.ask(15);
    assert_eq!(same_higher.shared.handle(), 0x1001);
    assert_eq!(same_higher.gpu.handle(), 0x2001);

    drop(first);
    drop(same_equal);
    drop(same_higher);
    take_log();

    // Asking for a lower value replaces the generation with a fresh shared + GPU event pair and
    // releases the previous generation's events.
    let next = fence.ask(5);
    assert_eq!(next.shared.handle(), 0x1002);
    assert_eq!(next.gpu.handle(), 0x2002);
    assert_eq!(fence.completed_value(), 0);
    assert_eq!(
        take_log(),
        vec![
            "MTLDevice_newSharedEvent device=0xd00 -> 0x1002",
            "MTLDevice_newEvent device=0xd00 -> 0x2002",
            "NSObject_release 0x1001",
            "NSObject_release 0x2001",
            "NSObject_retain 0x1002",
            "NSObject_retain 0x2002",
            "NSObject_retain 0x1002",
            "NSObject_release 0x1002",
        ]
    );
}

#[test]
fn signal_forwards_nonzero_only() {
    reset();
    let device = fake_obj(DEVICE);
    let helper_queue = fake_obj(HELPER_QUEUE);
    let fence = Fence::new(&device, &helper_queue);
    take_log();

    fence.signal(0);
    assert_eq!(
        take_log(),
        vec![
            "NSObject_retain 0x1001",
            "NSObject_retain 0x2001",
            "MTLSharedEvent_signalValue event=0x1001 value=0",
            "NSObject_release 0x1001",
            "NSObject_release 0x2001",
        ]
    );

    fence.signal(7);
    assert_eq!(
        take_log(),
        vec![
            "NSObject_retain 0x1001",
            "NSObject_retain 0x2001",
            "MTLSharedEvent_signalValue event=0x1001 value=7",
            "MTLCommandQueue_commandBuffer queue=0xe00 -> 0x3001",
            "MTLCommandBuffer_encodeSignalEvent cmdbuf=0x3001 event=0x2001 value=7",
            "MTLCommandBuffer_commit cmdbuf=0x3001",
            "NSObject_release 0x1001",
            "NSObject_release 0x2001",
        ]
    );
    assert!(fence.wait(7, 100));
    assert!(!fence.wait(8, 100));
}

#[test]
fn listener_registers_through_slot_104() {
    reset();
    let device = fake_obj(DEVICE);
    let helper_queue = fake_obj(HELPER_QUEUE);
    let fence = Fence::new(&device, &helper_queue);
    let listener = EventListener::new();
    let listener_id = listener.handle();
    take_log();

    let win32_event: Handle = core::ptr::without_provenance_mut(0xdead_beef);
    listener.set_event_on_value(&fence, win32_event, 42);

    let expected_slot_104 = format!(
        "MTLSharedEvent_setWin32EventAtValue shared=0x1001 listener={listener_id:#x} event=0xdeadbeef value=42"
    );
    assert_eq!(
        take_log(),
        vec![
            "NSObject_retain 0x1001",
            expected_slot_104.as_str(),
            "NSObject_release 0x1001",
        ]
    );
}

#[test]
fn multiple_fences_through_slot_146() {
    reset();
    let device = fake_obj(DEVICE);
    let helper_queue = fake_obj(HELPER_QUEUE);
    let f0 = Fence::new(&device, &helper_queue);
    let f1 = Fence::new(&device, &helper_queue);
    let f2 = Fence::new(&device, &helper_queue);
    let listener = EventListener::new();
    let listener_id = listener.handle();
    take_log();

    let win32_event: Handle = core::ptr::without_provenance_mut(0xcafe_babe);
    listener.set_event_on_values(&[&f0, &f1, &f2], &[10, 20, 30], true, win32_event);

    // Every fence's shared event is retained before slot 146 and released only after it returns.
    let expected_all = format!(
        "MTLSharedEvent_setWin32EventAtValues listener={listener_id:#x} events=[0x1001, 0x1002, 0x1003] values=[10, 20, 30] count=3 needed=3 event=0xcafebabe"
    );
    assert_eq!(
        take_log(),
        vec![
            "NSObject_retain 0x1001",
            "NSObject_retain 0x1002",
            "NSObject_retain 0x1003",
            expected_all.as_str(),
            "NSObject_release 0x1001",
            "NSObject_release 0x1002",
            "NSObject_release 0x1003",
        ]
    );

    listener.set_event_on_values(&[&f0, &f1, &f2], &[11, 22, 33], false, win32_event);
    let expected_any = format!(
        "MTLSharedEvent_setWin32EventAtValues listener={listener_id:#x} events=[0x1001, 0x1002, 0x1003] values=[11, 22, 33] count=3 needed=1 event=0xcafebabe"
    );
    assert_eq!(
        take_log(),
        vec![
            "NSObject_retain 0x1001",
            "NSObject_retain 0x1002",
            "NSObject_retain 0x1003",
            expected_any.as_str(),
            "NSObject_release 0x1001",
            "NSObject_release 0x1002",
            "NSObject_release 0x1003",
        ]
    );
}

#[test]
fn listener_drop_destroys_then_joins() {
    let listener = EventListener::new();
    let id = listener.handle();

    let (lock, cvar) = &*LISTENERS;
    let mut map = lock.lock().unwrap_or_else(PoisonError::into_inner);
    while !map.get(&id).is_some_and(|rec| rec.started) {
        map = cvar.wait(map).unwrap_or_else(PoisonError::into_inner);
    }
    assert_eq!(map[&id].thread_name, "dxmt-event-list");
    drop(map);

    drop(listener);

    let map = lock.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(map[&id].events, vec!["create", "start", "destroy", "exit"]);
}
