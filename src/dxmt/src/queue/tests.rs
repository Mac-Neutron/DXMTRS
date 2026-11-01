use std::{
    env,
    os::unix::process::ExitStatusExt,
    process::Command,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use winemetal_sys::{
    Obj,
    wire::{WMT_COMMAND_BUFFER_STATUS_COMPLETED, WMTSamplerInfo},
};

use super::{Queue, QueueConfig, Ring};

const ENCODE_PANIC_ENV: &str = "DXMT_CORE_TEST_ENCODE_PANIC";

#[unsafe(export_name = "MTLDevice_newCommandQueue")]
extern "C-unwind" fn fake_new_command_queue(_device: u64, _max_cmdbufs: u64) -> u64 {
    0x7100
}

#[unsafe(export_name = "MTLDevice_newSamplerState")]
extern "C-unwind" fn fake_new_sampler_state(_device: u64, info: *mut WMTSamplerInfo) -> u64 {
    // SAFETY: `Encoder::new` passes a valid local `WMTSamplerInfo`.
    unsafe { (*info).gpu_resource_id = 0x7200 };
    0x7201
}

#[unsafe(export_name = "MTLDevice_newFence")]
extern "C-unwind" fn fake_new_fence(_device: u64) -> u64 {
    0x7300
}

#[unsafe(export_name = "MTLCommandBuffer_status")]
extern "C-unwind" fn fake_cmdbuf_status(_cmdbuf: u64) -> u64 {
    WMT_COMMAND_BUFFER_STATUS_COMPLETED
}

#[unsafe(export_name = "MTLCommandBuffer_waitUntilCompleted")]
extern "C-unwind" fn fake_cmdbuf_wait(_cmdbuf: u64) {}

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
