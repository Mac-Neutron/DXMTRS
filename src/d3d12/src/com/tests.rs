use core::{
    ffi::c_void,
    mem::offset_of,
    ptr::{self, NonNull},
    sync::atomic::{AtomicUsize, Ordering},
};

use super::{
    ComObject, ComRefCounted, DeviceChild, E_NOINTERFACE, E_POINTER, Guid, Hresult,
    IID_ID3D12_DEVICE, IID_IUNKNOWN, PrivateHold, S_OK, add_ref_com, cast_com, from_companion,
    init_out_ptr, release_com, to_companion, write_out,
};
use crate::abi::{
    ext::{D3D12DeviceExtVtbl, D3D12ScalerDesc, IID_D3D12_DEVICE_EXT},
    vtbl::IUnknownVtbl,
};

#[repr(C)]
struct FakeDevice {
    vtbl: &'static IUnknownVtbl,
    ext_vtbl: &'static D3D12DeviceExtVtbl,
    com: ComObject,
    drops: &'static AtomicUsize,
}

impl ComRefCounted for FakeDevice {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for FakeDevice {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

const EXT_OFFSET: usize = offset_of!(FakeDevice, ext_vtbl);

unsafe fn fake_qi(this: *mut FakeDevice, riid: *const c_void, out: *mut *mut c_void) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `out` was just checked non-null.
    unsafe { init_out_ptr(out) };
    // SAFETY: `riid` points to a caller-supplied `Guid`.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_NOINTERFACE;
    };
    if iid == IID_IUNKNOWN || iid == IID_ID3D12_DEVICE {
        // SAFETY: `this` is a live `FakeDevice`.
        unsafe { add_ref_com(this) };
        // SAFETY: `out` is non-null.
        unsafe { write_out(out, this.cast::<c_void>()) };
        return S_OK;
    }
    if iid == IID_D3D12_DEVICE_EXT {
        // SAFETY: `this` is a live `FakeDevice`.
        unsafe { add_ref_com(this) };
        // SAFETY: `EXT_OFFSET` is `offset_of!(FakeDevice, ext_vtbl)`.
        let ext_ptr = unsafe { to_companion(this, EXT_OFFSET) };
        // SAFETY: `out` is non-null.
        unsafe { write_out(out, ext_ptr) };
        return S_OK;
    }
    E_NOINTERFACE
}

static FAKE_DEVICE_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: slot!((this: *mut c_void, riid: *const c_void, out: *mut *mut c_void) -> i32 => {
        // SAFETY: `this` is a `FakeDevice` primary pointer.
        unsafe { fake_qi(this.cast::<FakeDevice>(), riid, out) }
    }),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a `FakeDevice` primary pointer.
        unsafe { add_ref_com(this.cast::<FakeDevice>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a `FakeDevice` primary pointer.
        unsafe { release_com(this.cast::<FakeDevice>()) }
    }),
};

static FAKE_DEVICE_EXT_VTBL: D3D12DeviceExtVtbl = D3D12DeviceExtVtbl {
    unknown: IUnknownVtbl {
        query_interface: slot!((this: *mut c_void, riid: *const c_void, out: *mut *mut c_void) -> i32 => {
            // SAFETY: `this` points to `FakeDevice::ext_vtbl`.
            let dev = unsafe { from_companion::<FakeDevice>(this, EXT_OFFSET) };
            // SAFETY: `dev` is the enclosing `FakeDevice`.
            unsafe { fake_qi(dev, riid, out) }
        }),
        add_ref: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` points to `FakeDevice::ext_vtbl`.
            let dev = unsafe { from_companion::<FakeDevice>(this, EXT_OFFSET) };
            // SAFETY: `dev` is the enclosing `FakeDevice`.
            unsafe { add_ref_com(dev) }
        }),
        release: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` points to `FakeDevice::ext_vtbl`.
            let dev = unsafe { from_companion::<FakeDevice>(this, EXT_OFFSET) };
            // SAFETY: `dev` is the enclosing `FakeDevice`.
            unsafe { release_com(dev) }
        }),
    },
    get_temporal_scaler_scale_range: slot!((_this: *mut c_void, _min: *mut f32, _max: *mut f32) -> Hresult => S_OK),
    create_temporal_scaler: slot!((_this: *mut c_void, _desc: *const D3D12ScalerDesc, _out: *mut *mut c_void) -> Hresult => S_OK),
};

#[test]
fn device_child_and_private_hold_track_refs() {
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    DROPS.store(0, Ordering::SeqCst);

    let dev = Box::into_raw(Box::new(FakeDevice {
        vtbl: &FAKE_DEVICE_VTBL,
        ext_vtbl: &FAKE_DEVICE_EXT_VTBL,
        com: ComObject::new(),
        drops: &DROPS,
    }));
    // SAFETY: `dev` was just allocated.
    let mut pub_ref = unsafe { add_ref_com(dev) };
    assert_eq!(pub_ref, 1);

    let dev_nn = NonNull::new(dev).expect("allocated Box::into_raw is non-null");
    // SAFETY: `dev` is non-null and live.
    let child = unsafe { DeviceChild::new(dev_nn) };
    // `DeviceChild` takes a public reference (`device_->AddRef()`).
    // SAFETY: `dev` is live.
    assert_eq!(unsafe { (*dev).com.public_count() }, 2);

    // SAFETY: `dev` is non-null and live.
    let priv_hold = unsafe { PrivateHold::new(dev_nn) };
    // SAFETY: `dev` is live.
    assert_eq!(unsafe { (*dev).com.private_count() }, 2);

    // `cast_com` matches `FAKE_DEVICE_VTBL` and rejects a foreign vtable.
    // SAFETY: `dev` is a live COM object.
    assert!(unsafe {
        cast_com::<FakeDevice, IUnknownVtbl>(dev.cast(), &FAKE_DEVICE_VTBL).is_some()
    });
    // SAFETY: `dev` is a live COM object.
    assert!(unsafe {
        cast_com::<FakeDevice, D3D12DeviceExtVtbl>(dev.cast(), &FAKE_DEVICE_EXT_VTBL).is_none()
    });

    // `GetDevice` on `child` returns a new public reference via `QueryInterface`.
    let iid_dev = IID_ID3D12_DEVICE;
    let mut queried: *mut c_void = ptr::null_mut();
    // SAFETY: `iid_dev` and `queried` are valid stack variables.
    let hr = unsafe { child.get_device((&raw const iid_dev).cast::<c_void>(), &raw mut queried) };
    assert_eq!(hr, S_OK);
    assert_eq!(queried, dev.cast::<c_void>());
    // SAFETY: `queried` holds 1 public reference.
    assert_eq!(unsafe { release_com(queried.cast::<FakeDevice>()) }, 2);

    // Drop `child` (public count 2 -> 1) and release the initial public ref (1 -> 0); `priv_hold`
    // keeps `dev` alive until dropped.
    drop(child);
    // SAFETY: `dev` is live and owns 1 public ref.
    pub_ref = unsafe { release_com(dev) };
    assert_eq!(pub_ref, 0);
    assert_eq!(DROPS.load(Ordering::SeqCst), 0);

    drop(priv_hold);
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
}

#[test]
fn companion_vtbl_round_trips_to_self() {
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    DROPS.store(0, Ordering::SeqCst);

    let dev = Box::into_raw(Box::new(FakeDevice {
        vtbl: &FAKE_DEVICE_VTBL,
        ext_vtbl: &FAKE_DEVICE_EXT_VTBL,
        com: ComObject::new(),
        drops: &DROPS,
    }));
    // SAFETY: `dev` was just allocated.
    assert_eq!(unsafe { add_ref_com(dev) }, 1);

    let iid_ext = IID_D3D12_DEVICE_EXT;
    let mut ext_ptr: *mut c_void = ptr::null_mut();
    // SAFETY: querying `IID_D3D12_DEVICE_EXT` on `dev`.
    let hr = unsafe {
        (FAKE_DEVICE_VTBL.query_interface)(
            dev.cast::<c_void>(),
            (&raw const iid_ext).cast::<c_void>(),
            &raw mut ext_ptr,
        )
    };
    assert_eq!(hr, S_OK);
    assert_eq!(ext_ptr.addr(), dev.addr() + EXT_OFFSET);

    let iid_unk = IID_IUNKNOWN;
    let mut unk_from_ext: *mut c_void = ptr::null_mut();
    // SAFETY: querying `IID_IUNKNOWN` through the companion vtable must return the primary pointer
    // (COM identity law).
    let hr = unsafe {
        (FAKE_DEVICE_EXT_VTBL.unknown.query_interface)(
            ext_ptr,
            (&raw const iid_unk).cast::<c_void>(),
            &raw mut unk_from_ext,
        )
    };
    assert_eq!(hr, S_OK);
    assert_eq!(unk_from_ext, dev.cast::<c_void>());

    // Release all three references (initial, `ext_ptr`, `unk_from_ext`).
    // SAFETY: `unk_from_ext` is a valid primary pointer.
    assert_eq!(unsafe { (FAKE_DEVICE_VTBL.release)(unk_from_ext) }, 2);
    // SAFETY: `ext_ptr` is a valid companion pointer.
    let ext_rel = unsafe { (FAKE_DEVICE_EXT_VTBL.unknown.release)(ext_ptr) };
    assert_eq!(ext_rel, 1);
    // SAFETY: `dev` is a valid primary pointer.
    assert_eq!(unsafe { release_com(dev) }, 0);
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
}

#[cfg(target_os = "macos")]
#[test]
fn slot_and_thread_panics_abort() {
    use std::{
        os::unix::process::ExitStatusExt,
        process::{Command, Stdio},
    };

    const CHILD: &str = "DXMT_D3D12_TEST_GUARDED_PANIC";

    if let Ok(mode) = std::env::var(CHILD) {
        if mode == "slot" {
            let f: unsafe extern "system-unwind" fn(*mut c_void) -> u32 = slot!("device.GetNodeCount", (_this: *mut c_void) -> u32 => {
                panic!("a Rust panic inside a COM slot");
            });
            // SAFETY: null is unused by the panicking slot body.
            unsafe { f(ptr::null_mut()) };
        } else if mode == "thread" {
            let handle = super::spawn_thread("d3d12-test-panic", || {
                panic!("a Rust panic inside a D3D12 worker thread");
            });
            let _ = handle.join();
        }
        return;
    }

    for mode in ["slot", "thread"] {
        let status = Command::new(std::env::current_exe().expect("the test binary's path"))
            .args([
                "--exact",
                "com::tests::slot_and_thread_panics_abort",
                "--test-threads=1",
            ])
            .env(CHILD, mode)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("the test binary runs again");
        assert_eq!(status.signal(), Some(libc::SIGABRT), "{mode}: {status}");
    }
}
