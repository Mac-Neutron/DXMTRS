//! A core device on `dxmt-fakes` for the recorder's tests, and what its queue's commits captured.

use dxmt_fakes as _;
use winemetal_sys::Obj;

use super::{CoreDevice, queue_profile};

/// A core device on the made-up handle `handle` with a 64x32 back buffer and a window.
pub fn core_on_fakes(handle: u64) -> CoreDevice {
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { Obj::adopt(handle) }.expect("a non-zero handle");
    let mut core = CoreDevice::new(mtl, queue_profile());
    core.attach(1, 64, 32);
    core
}

/// The index of the first captured call equal to `line`; panics with every call when none is.
pub fn at(calls: &[String], line: &str) -> usize {
    calls
        .iter()
        .position(|c| c == line)
        .unwrap_or_else(|| panic!("no {line:?} in {calls:#?}"))
}

/// Runs `f` on a thread of its own and fails the test when it has not returned within 10 s.
pub fn returns_within_10s(f: impl FnOnce() + Send + 'static) {
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        f();
        let _ = done.send(());
    });
    finished
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("returns within 10 s");
}
