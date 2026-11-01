use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use super::{
    ComObject, E_NOINTERFACE, Guid, Hresult, IID_IUNKNOWN, IUnknownVtbl, Managed, S_OK, TearOff,
    iface_of, write_out,
};

#[repr(C)]
struct FakeDevice {
    vtbl: *const IUnknownVtbl,
    refs: AtomicU32,
}

impl FakeDevice {
    const fn new() -> Self {
        Self {
            vtbl: &raw const FAKE_DEVICE_VTBL,
            refs: AtomicU32::new(1),
        }
    }

    const fn as_iface(&self) -> *mut c_void {
        iface_of(&self.vtbl)
    }

    fn count(&self) -> u32 {
        self.refs.load(Ordering::SeqCst)
    }

    unsafe extern "system" fn query_interface(
        this: *mut c_void,
        riid: *const Guid,
        out: *mut *mut c_void,
    ) -> Hresult {
        // SAFETY: `riid` is a valid GUID pointer in tests.
        let Some(iid) = (unsafe { Guid::read(riid) }) else {
            return E_NOINTERFACE;
        };
        if iid == IID_IUNKNOWN {
            // SAFETY: `this` is a `FakeDevice`.
            unsafe { Self::add_ref(this) };
            // SAFETY: `out` is a valid output pointer in tests.
            unsafe { write_out(out, this) };
            return S_OK;
        }
        E_NOINTERFACE
    }

    unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
        let dev = container_of!(this, Self, vtbl);
        // SAFETY: `dev` is a live `FakeDevice`.
        let prev = unsafe { (*dev).refs.fetch_add(1, Ordering::SeqCst) };
        prev + 1
    }

    unsafe extern "system" fn release(this: *mut c_void) -> u32 {
        let dev = container_of!(this, Self, vtbl);
        // SAFETY: `dev` is a live `FakeDevice`.
        let prev = unsafe { (*dev).refs.fetch_sub(1, Ordering::SeqCst) };
        prev - 1
    }
}

static FAKE_DEVICE_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: FakeDevice::query_interface,
    add_ref: FakeDevice::add_ref,
    release: FakeDevice::release,
};

#[test]
fn com_object_deletes_at_private_zero() {
    let obj = ComObject::new();
    assert_eq!(obj.public_count(), 0);
    assert_eq!(obj.private_count(), 0);

    // First public ref takes one private ref.
    assert_eq!(obj.add_ref(), 1);
    assert_eq!(obj.public_count(), 1);
    assert_eq!(obj.private_count(), 1);

    // An internal bind holds an extra private ref; releasing the public ref drops public to 0
    // without freeing while the private ref remains.
    obj.add_ref_private();
    assert_eq!(obj.private_count(), 2);
    let (rem, free) = obj.release_and_free();
    assert_eq!(rem, 0);
    assert!(!free);
    assert_eq!(obj.private_count(), 1);

    // Releasing the last private ref signals deletion and sets the destroying guard bit.
    assert!(obj.release_private());
    assert_eq!(obj.private_count(), 0);
}

#[test]
fn managed_zero_is_valid_and_recycles() {
    let dev = FakeDevice::new();
    let managed = Managed::new();
    assert_eq!(managed.public_count(), 0);
    assert_eq!(managed.private_count(), 0);

    // Private 0 -> 1 -> 0 recycles without destroying, and the object can be reused from 0 again.
    managed.add_ref_private();
    assert_eq!(managed.private_count(), 1);
    assert!(managed.release_private());
    assert_eq!(managed.private_count(), 0);

    // Reuse from 0 via public AddRef -> Release.
    // SAFETY: `dev` is a live `FakeDevice`.
    let pub_after_add = unsafe { managed.add_ref(dev.as_iface()) };
    assert_eq!(pub_after_add, 1);
    assert_eq!(managed.private_count(), 1);

    // SAFETY: `dev` is a live `FakeDevice`.
    let (pub_after_rel, recycled) = unsafe { managed.release(dev.as_iface()) };
    assert_eq!(pub_after_rel, 0);
    assert!(recycled);
    assert_eq!(managed.public_count(), 0);
    assert_eq!(managed.private_count(), 0);
}

#[test]
fn first_public_ref_adds_device_ref() {
    let dev = FakeDevice::new();
    let managed = Managed::new();
    assert_eq!(dev.count(), 1);

    // 0 -> 1 public adds one ref to the parent device; 1 -> 2 does not.
    // SAFETY: `dev` is a live `FakeDevice`.
    assert_eq!(unsafe { managed.add_ref(dev.as_iface()) }, 1);
    assert_eq!(dev.count(), 2);
    // SAFETY: `dev` is a live `FakeDevice`.
    assert_eq!(unsafe { managed.add_ref(dev.as_iface()) }, 2);
    assert_eq!(dev.count(), 2);

    // Holding a private ref keeps `recycled == false` when public drops 1 -> 0, while still
    // releasing the parent device's ref on 1 -> 0 public.
    managed.add_ref_private();
    // SAFETY: `dev` is a live `FakeDevice`.
    assert_eq!(unsafe { managed.release(dev.as_iface()) }, (1, false));
    assert_eq!(dev.count(), 2);
    // SAFETY: `dev` is a live `FakeDevice`.
    assert_eq!(unsafe { managed.release(dev.as_iface()) }, (0, false));
    assert_eq!(dev.count(), 1);
    assert_eq!(managed.private_count(), 1);
    assert!(managed.release_private());
}

#[repr(C)]
struct TearOffVtbl {
    query_interface:
        unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> Hresult,
    add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
}

static TEAR_OFF_VTBL: TearOffVtbl = TearOffVtbl {
    query_interface: TearOff::<()>::query_interface,
    add_ref: TearOff::<()>::add_ref,
    release: TearOff::<()>::release,
};

#[test]
fn tear_off_forwards() {
    let dev = FakeDevice::new();
    let tear_off = TearOff::new(&TEAR_OFF_VTBL, dev.as_iface());
    let iface = tear_off.as_iface();

    // SAFETY: `iface` is a live `TearOff`.
    let r1 = unsafe { (TEAR_OFF_VTBL.add_ref)(iface) };
    assert_eq!(r1, 2);
    assert_eq!(dev.count(), 2);

    let iid = IID_IUNKNOWN;
    let mut out = ptr::null_mut();
    // SAFETY: `iface` is a live `TearOff`, `iid` and `out` are valid pointers.
    let hr =
        unsafe { (TEAR_OFF_VTBL.query_interface)(iface, (&raw const iid).cast(), &raw mut out) };
    assert_eq!(hr, S_OK);
    assert_eq!(out, dev.as_iface());
    assert_eq!(dev.count(), 3);

    // SAFETY: `iface` is a live `TearOff`.
    assert_eq!(unsafe { (TEAR_OFF_VTBL.release)(iface) }, 2);
    // SAFETY: `iface` is a live `TearOff`.
    assert_eq!(unsafe { (TEAR_OFF_VTBL.release)(iface) }, 1);
    assert_eq!(dev.count(), 1);
}
