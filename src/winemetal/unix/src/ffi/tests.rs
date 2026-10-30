use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

use super::*;

/// Set in the child [`panic_under_guard_aborts`] runs: the child panics under the guard.
const CHILD: &str = "WINEMETAL_TEST_GUARDED_PANIC";

fn guarded_panic() {
    let _guard = AbortOnPanic;
    panic!("a Rust panic under AbortOnPanic");
}

#[test]
fn panic_under_guard_aborts() {
    if std::env::var_os(CHILD).is_some() {
        guarded_panic();
        return;
    }
    let status = Command::new(std::env::current_exe().expect("the test binary's path"))
        .args(["--exact", "ffi::tests::panic_under_guard_aborts", "--test-threads=1"])
        .env(CHILD, "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("the test binary runs again");
    assert_eq!(status.signal(), Some(libc::SIGABRT), "{status}");
}

#[test]
fn mem_slice_of_null() {
    let null = MemPtr { ptr: 0 };
    // SAFETY: a null pointer is never read.
    assert_eq!(unsafe { mem_slice::<u32>(null, 0) }, Some(&[][..]));
    // SAFETY: as above.
    assert_eq!(unsafe { mem_slice::<u32>(null, 3) }, None);
}
