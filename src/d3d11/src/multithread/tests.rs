//! Unit tests for `DeviceMutex` and `Multithread`.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use super::{DeviceMutex, Multithread};

#[test]
fn off_until_protected() {
    let m = Arc::new(DeviceMutex::new());
    assert!(!m.get_protected());

    // When unprotected, `lock` is a no-op and another thread can also `lock` freely.
    m.lock();
    let m2 = Arc::clone(&m);
    let ran = thread::spawn(move || {
        m2.lock();
        m2.unlock();
        true
    })
    .join()
    .unwrap();
    assert!(ran);
    m.unlock();

    // Toggling via `Multithread` COM vtable reports the old state and updates `get_protected`.
    let mut mt = Multithread::new();
    mt.init(core::ptr::null_mut(), Arc::as_ptr(&m));
    let iface = mt.as_iface();
    // SAFETY: `iface` is a valid `Multithread` tear-off pointer.
    let vtbl_ptr = unsafe { *iface.cast::<*const crate::abi::vtbl::ID3D11MultithreadVtbl>() };
    // SAFETY: `vtbl_ptr` points to the static `ID3D11MultithreadVtbl`.
    let vtbl = unsafe { &*vtbl_ptr };
    // SAFETY: Calling `get_multithread_protected` on valid `iface`.
    assert_eq!(unsafe { (vtbl.get_multithread_protected)(iface) }, 0);
    // SAFETY: Calling `set_multithread_protected` on valid `iface`.
    assert_eq!(unsafe { (vtbl.set_multithread_protected)(iface, 1) }, 0);
    assert!(m.get_protected());
    // SAFETY: Calling `get_multithread_protected` on valid `iface`.
    assert_eq!(unsafe { (vtbl.get_multithread_protected)(iface) }, 1);
    // SAFETY: Calling `set_multithread_protected` on valid `iface`.
    assert_eq!(unsafe { (vtbl.set_multithread_protected)(iface, 0) }, 1);
    assert!(!m.get_protected());
}

#[test]
fn recursive_for_the_owner() {
    let m = Arc::new(DeviceMutex::new());
    assert!(!m.set_protected(true));

    // Acquire 3 nested locks on the main thread.
    m.lock();
    m.lock();
    let guard = m.lock_guard();

    let entered = Arc::new(AtomicBool::new(false));
    let entered2 = Arc::clone(&entered);
    let m2 = Arc::clone(&m);
    let handle = thread::spawn(move || {
        m2.lock();
        entered2.store(true, Ordering::SeqCst);
        m2.unlock();
    });

    // While depth > 0 on the main thread, the worker thread cannot enter.
    thread::sleep(Duration::from_millis(10));
    assert!(!entered.load(Ordering::SeqCst));

    drop(guard);
    m.unlock();
    thread::sleep(Duration::from_millis(10));
    assert!(!entered.load(Ordering::SeqCst));

    // Final unlock releases ownership so the worker thread can enter.
    m.unlock();
    handle.join().unwrap();
    assert!(entered.load(Ordering::SeqCst));
}

#[test]
fn unprotect_resets_owner() {
    let m = Arc::new(DeviceMutex::new());
    m.set_protected(true);
    m.lock();
    m.lock();

    // Disabling protection clears owner and depth so re-enabling allows another thread to lock.
    assert!(m.set_protected(false));
    assert!(!m.set_protected(true));

    let m2 = Arc::clone(&m);
    let ok = thread::spawn(move || {
        m2.lock();
        m2.unlock();
        true
    })
    .join()
    .unwrap();
    assert!(ok);
}
