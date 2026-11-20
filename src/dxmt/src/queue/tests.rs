use core::cell::Cell;
use std::{
    collections::VecDeque,
    env,
    os::unix::process::ExitStatusExt,
    process::Command,
    sync::{
        Arc, Condvar, LazyLock, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use rustc_hash::FxHashMap;
use winemetal_sys::{
    Obj,
    wire::{
        WMT_COMMAND_BUFFER_STATUS_COMPLETED, WMT_COMMAND_BUFFER_STATUS_ERROR,
        WMT_COMMAND_BUFFER_STATUS_NOT_ENQUEUED, WMT_COMMAND_BUFFER_STATUS_SCHEDULED,
        WMTSamplerInfo,
    },
};

use super::{Queue, QueueConfig, Ring};

const ENCODE_PANIC_ENV: &str = "DXMT_CORE_TEST_ENCODE_PANIC";

enum Script {
    ScheduledThenError,
    UnexpectedStatus(u64),
    ReturnNil,
}

enum CmdbufState {
    Scheduled { waited: bool },
    Unexpected(u64),
}

#[derive(Default)]
struct QueueFakeState {
    queues: FxHashMap<u64, u64>,
    cmdbufs: FxHashMap<u64, u64>,
    armed: FxHashMap<u64, VecDeque<Script>>,
    cmdbuf_states: FxHashMap<u64, CmdbufState>,
    error_calls: FxHashMap<u64, u32>,
}

static NEXT_QUEUE: AtomicU64 = AtomicU64::new(0x7100);
static FAKE_STATE: LazyLock<Mutex<QueueFakeState>> =
    LazyLock::new(|| Mutex::new(QueueFakeState::default()));

thread_local! {
    static ERROR_HOOK: Cell<Option<crate::fakes::CommandBufferHook>> = const { Cell::new(None) };
    static OBJECT_LOG: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn init_queue_fakes() {
    crate::fakes::register_command_buffer_handler(on_command_buffer);
}

fn arm_script(device: u64, script: Script) {
    let mut state = FAKE_STATE.lock().unwrap_or_else(PoisonError::into_inner);
    state.armed.entry(device).or_default().push_back(script);
}

fn error_calls(device: u64) -> u32 {
    let state = FAKE_STATE.lock().unwrap_or_else(PoisonError::into_inner);
    state.error_calls.get(&device).copied().unwrap_or(0)
}

fn on_command_buffer(queue: u64, cmdbuf: u64) -> u64 {
    let mut state = FAKE_STATE.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(device) = state.queues.get(&queue).copied() else {
        return cmdbuf;
    };
    if let Some(scripts) = state.armed.get_mut(&device)
        && matches!(scripts.front(), Some(Script::ReturnNil))
    {
        scripts.pop_front();
        return 0;
    }
    state.cmdbufs.insert(cmdbuf, device);
    cmdbuf
}

#[unsafe(export_name = "MTLDevice_newCommandQueue")]
extern "C-unwind" fn fake_new_command_queue(device: u64, _max_cmdbufs: u64) -> u64 {
    let queue = NEXT_QUEUE.fetch_add(1, Ordering::Relaxed);
    let mut state = FAKE_STATE.lock().unwrap_or_else(PoisonError::into_inner);
    state.queues.insert(queue, device);
    queue
}

#[unsafe(export_name = "MTLDevice_newSamplerState")]
extern "C-unwind" fn fake_new_sampler_state(_device: u64, info: *mut WMTSamplerInfo) -> u64 {
    // SAFETY: `Encoder::new` passes a valid local `WMTSamplerInfo`.
    unsafe { (*info).gpu_resource_id = 0x7200 };
    0x7201
}

#[unsafe(export_name = "MTLCommandBuffer_status")]
extern "C-unwind" fn fake_cmdbuf_status(cmdbuf: u64) -> u64 {
    let mut state = FAKE_STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(cmdbuf_state) = state.cmdbuf_states.get(&cmdbuf) {
        return match cmdbuf_state {
            CmdbufState::Scheduled { waited: false } => WMT_COMMAND_BUFFER_STATUS_SCHEDULED,
            CmdbufState::Scheduled { waited: true } => WMT_COMMAND_BUFFER_STATUS_ERROR,
            CmdbufState::Unexpected(status) => *status,
        };
    }
    let Some(device) = state.cmdbufs.get(&cmdbuf).copied() else {
        return WMT_COMMAND_BUFFER_STATUS_COMPLETED;
    };
    let next = state.armed.get_mut(&device).and_then(|scripts| {
        if matches!(scripts.front(), Some(Script::ReturnNil) | None) {
            None
        } else {
            scripts.pop_front()
        }
    });
    match next {
        Some(Script::ScheduledThenError) => {
            state
                .cmdbuf_states
                .insert(cmdbuf, CmdbufState::Scheduled { waited: false });
            WMT_COMMAND_BUFFER_STATUS_SCHEDULED
        }
        Some(Script::UnexpectedStatus(status)) => {
            state
                .cmdbuf_states
                .insert(cmdbuf, CmdbufState::Unexpected(status));
            status
        }
        Some(Script::ReturnNil) | None => WMT_COMMAND_BUFFER_STATUS_COMPLETED,
    }
}

#[unsafe(export_name = "MTLCommandBuffer_waitUntilCompleted")]
extern "C-unwind" fn fake_cmdbuf_wait(cmdbuf: u64) {
    let mut state = FAKE_STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(CmdbufState::Scheduled { waited }) = state.cmdbuf_states.get_mut(&cmdbuf) {
        *waited = true;
    }
}

#[unsafe(export_name = "MTLCommandBuffer_error")]
extern "C-unwind" fn fake_cmdbuf_error(cmdbuf: u64) -> u64 {
    if let Some(hook) = ERROR_HOOK.get() {
        return hook(cmdbuf);
    }
    let mut state = FAKE_STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(device) = state.cmdbufs.get(&cmdbuf).copied() {
        *state.error_calls.entry(device).or_insert(0) += 1;
    }
    0
}

#[unsafe(export_name = "MTLCommandBuffer_encodeWaitForEvent")]
extern "C-unwind" fn fake_cmdbuf_wait_event(_cmdbuf: u64, _event: u64, _value: u64) {}

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

/// A chunk retired before its commit returns never wraps `in_flight` below zero.
///
/// Wrapped, the recorder would wait on `u64::MAX` for a finish that never comes (`d3d11_draw
/// perf`'s stalls). Two chunks make the commit wait at one in flight, so every commit races its
/// own chunk's finish.
#[test]
fn a_chunk_finished_before_its_commit_returns_never_stalls_the_ring() {
    const COMMITS: u64 = 200_000;
    let ring = Arc::new(Ring::new(2));
    let encode = {
        let ring = Arc::clone(&ring);
        thread::spawn(move || {
            let mut seq = 1;
            while ring.wait_committed(seq) {
                ring.encoded();
                seq += 1;
            }
        })
    };
    let finish = {
        let ring = Arc::clone(&ring);
        thread::spawn(move || {
            let mut seq = 1;
            while ring.wait_encoded(seq) {
                ring.finish(seq);
                seq += 1;
            }
        })
    };
    let recorder = {
        let ring = Arc::clone(&ring);
        thread::spawn(move || {
            for _ in 0..COMMITS {
                ring.commit();
            }
        })
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while !recorder.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let stalled = !recorder.is_finished();
    let in_flight = ring.in_flight.load(Ordering::Acquire);
    // Unblock a stalled recorder (it waits on in_flight) so the threads can be joined.
    ring.in_flight.store(0, Ordering::Release);
    super::sync::wake_all(&ring.in_flight);
    recorder.join().unwrap();
    ring.stop();
    encode.join().unwrap();
    finish.join().unwrap();
    assert!(
        !stalled,
        "the recorder stalled with in_flight = {in_flight:#x}"
    );
}

#[test]
fn encode_thread_panic_aborts() {
    if env::var_os(ENCODE_PANIC_ENV).is_some() {
        // SAFETY: fake non-zero handle for the test binary's `NSObject_retain`/`NSObject_release`.
        let device = unsafe { Obj::adopt(0xD5) }.expect("non-zero handle");
        let mut queue = Queue::new(&device, QueueConfig::default()).expect("queue threads");
        let started = Arc::new((Mutex::new(false), Condvar::new()));
        let started_clone = Arc::clone(&started);
        queue.chunk().emit(move |_| {
            let (lock, cvar) = &*started_clone;
            *lock.lock().expect("mutex") = true;
            cvar.notify_one();
            panic!("intentional encode thread panic");
        });
        queue.commit();
        let (lock, cvar) = &*started;
        let mut guard = lock.lock().expect("mutex");
        while !*guard {
            guard = cvar.wait(guard).expect("condvar");
        }
        drop(guard);
        thread::sleep(Duration::from_millis(100));
        return;
    }

    let exe = env::current_exe().expect("test binary path");
    let status = Command::new(exe)
        .env(ENCODE_PANIC_ENV, "1")
        .arg("--exact")
        .arg("queue::tests::encode_thread_panic_aborts")
        .arg("--nocapture")
        .status()
        .expect("child test process");
    assert_eq!(status.signal(), Some(libc::SIGABRT));
}

#[test]
fn command_buffer_error_latches_has_error_without_stalling() {
    const DEVICE: u64 = 0xD6;
    init_queue_fakes();
    // SAFETY: fake non-zero handle for the test binary's `NSObject_retain`/`NSObject_release`.
    let device = unsafe { Obj::adopt(DEVICE) }.expect("non-zero handle");
    let mut queue = Queue::new(&device, QueueConfig::default()).expect("queue threads");
    assert!(!queue.has_error());

    let seq1 = queue.current_seq();
    queue.commit();
    queue.wait_cpu(seq1);
    assert!(!queue.has_error());
    assert_eq!(error_calls(DEVICE), 0);

    arm_script(DEVICE, Script::ScheduledThenError);
    let seq2 = queue.current_seq();
    queue.chunk().signal_frame_latency(1);
    queue.commit();
    queue.wait_cpu(seq2);
    queue.set_max_latency(0);
    queue.present_boundary(true);
    assert!(queue.has_error());
    assert_eq!(error_calls(DEVICE), 1);

    arm_script(DEVICE, Script::ScheduledThenError);
    let seq3 = queue.current_seq();
    queue.commit();
    queue.wait_cpu(seq3);
    assert!(queue.has_error());
    assert_eq!(error_calls(DEVICE), 1);

    let seq4 = queue.current_seq();
    queue.commit();
    queue.wait_cpu(seq4);
    assert!(queue.has_error());
    assert_eq!(error_calls(DEVICE), 1);
}

#[test]
fn nil_and_unexpected_status_command_buffers_latch_has_error() {
    const NIL_DEVICE: u64 = 0xD7;
    const UNEXPECTED_DEVICE: u64 = 0xD8;
    init_queue_fakes();

    // SAFETY: fake non-zero handle for the test binary's `NSObject_retain`/`NSObject_release`.
    let nil_device = unsafe { Obj::adopt(NIL_DEVICE) }.expect("non-zero handle");
    let mut nil_queue = Queue::new(&nil_device, QueueConfig::default()).expect("queue threads");
    arm_script(NIL_DEVICE, Script::ReturnNil);
    let seq1 = nil_queue.current_seq();
    nil_queue.chunk().signal_frame_latency(1);
    nil_queue.commit();
    nil_queue.wait_cpu(seq1);
    nil_queue.set_max_latency(0);
    nil_queue.present_boundary(true);
    assert!(nil_queue.has_error());
    assert_eq!(error_calls(NIL_DEVICE), 0);

    // SAFETY: fake non-zero handle for the test binary's `NSObject_retain`/`NSObject_release`.
    let unexpected_device = unsafe { Obj::adopt(UNEXPECTED_DEVICE) }.expect("non-zero handle");
    let mut unexpected_queue =
        Queue::new(&unexpected_device, QueueConfig::default()).expect("queue threads");
    arm_script(
        UNEXPECTED_DEVICE,
        Script::UnexpectedStatus(WMT_COMMAND_BUFFER_STATUS_NOT_ENQUEUED),
    );
    let seq2 = unexpected_queue.current_seq();
    unexpected_queue.chunk().signal_frame_latency(1);
    unexpected_queue.commit();
    unexpected_queue.wait_cpu(seq2);
    unexpected_queue.set_max_latency(0);
    unexpected_queue.present_boundary(true);
    assert!(unexpected_queue.has_error());
    assert_eq!(error_calls(UNEXPECTED_DEVICE), 0);
}

#[test]
fn log_command_buffer_error_formats_nil_and_retained_error_on_calling_thread() {
    fn record_object(op: &'static str, obj: u64) {
        OBJECT_LOG.with_borrow_mut(|log| log.push(format!("{op} {obj:#x}")));
    }
    fn error_for_cmdbuf(cmdbuf: u64) -> u64 {
        match cmdbuf {
            1 => 0,
            2 => 0x9001,
            _ => 0x9002,
        }
    }
    fn description_for_err(err: u64) -> Option<&'static str> {
        (err == 0x9002).then_some("GPU Hang")
    }

    let _ = crate::log::take_captured();
    OBJECT_LOG.with_borrow_mut(Vec::clear);
    crate::fakes::on_autorelease_pool(None);
    crate::fakes::log_object_calls(record_object);
    crate::fakes::on_description(Some(description_for_err));
    ERROR_HOOK.set(Some(error_for_cmdbuf));

    super::log_command_buffer_error(1, 41);
    super::log_command_buffer_error(2, 42);
    super::log_command_buffer_error(3, 43);

    ERROR_HOOK.set(None);
    crate::fakes::on_description(None);
    crate::fakes::clear_object_calls_hook();

    assert_eq!(
        crate::log::take_captured(),
        vec![
            (
                4,
                "dxmt queue: Metal command buffer 41 completed with error status".to_owned()
            ),
            (
                4,
                "dxmt queue: Metal command buffer 42 completed with error status".to_owned()
            ),
            (
                4,
                "dxmt queue: Metal command buffer 43 completed with error: GPU Hang".to_owned()
            ),
        ]
    );
    assert_eq!(
        OBJECT_LOG.take(),
        vec![
            "NSObject_release 0x730e",
            "NSObject_retain 0x9001",
            "NSObject_release 0x730e",
            "NSObject_release 0x9001",
            "NSObject_release 0x730e",
            "NSObject_retain 0x9002",
            "NSObject_release 0x730e",
            "NSObject_release 0x9002",
            "NSObject_release 0x730e",
        ]
    );
}
