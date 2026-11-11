use core::cell::Cell;

use winemetal_sys::{
    Obj,
    wire::{WMT_METAL310, WMT_METAL320},
};

use super::Device;

thread_local! {
    static OS_VERSION: Cell<(u64, u64, u64)> = const { Cell::new((15, 0, 0)) };
    static MAXIMIZE_CONCURRENT: Cell<Option<bool>> = const { Cell::new(None) };
}

#[unsafe(export_name = "WMTGetOSVersion")]
extern "C-unwind" fn fake_get_os_version(
    major_out: *mut u64,
    minor_out: *mut u64,
    patch_out: *mut u64,
) {
    let (major, minor, patch) = OS_VERSION.get();
    // SAFETY: `Device::new` passes a valid local `u64` pointer for `major_out`.
    unsafe { *major_out = major };
    // SAFETY: `Device::new` passes a valid local `u64` pointer for `minor_out`.
    unsafe { *minor_out = minor };
    // SAFETY: `Device::new` passes a valid local `u64` pointer for `patch_out`.
    unsafe { *patch_out = patch };
}

#[unsafe(export_name = "MTLDevice_setShouldMaximizeConcurrentCompilation")]
extern "C-unwind" fn fake_set_maximize_concurrent(_device: u64, value: bool) {
    MAXIMIZE_CONCURRENT.set(Some(value));
}

#[test]
fn metal_version_as_cpp() {
    // Apple7+ on macOS 15+: defaults to Metal 3.2, respects explicit 310/320, ignores unknown.
    assert_eq!(
        Device::select_caps(0, 15, 0, true),
        (WMT_METAL320, u32::MAX, true)
    );
    assert_eq!(
        Device::select_caps(310, 15, 0, true),
        (WMT_METAL310, u32::MAX, true)
    );
    assert_eq!(
        Device::select_caps(320, 15, 0, true),
        (WMT_METAL320, u32::MAX, true)
    );
    assert_eq!(
        Device::select_caps(999, 15, 0, true),
        (WMT_METAL320, u32::MAX, true)
    );

    // Apple7+ on macOS 14: OS cap clamps Metal 3.2 down to Metal 3.1.
    assert_eq!(
        Device::select_caps(0, 14, 5, true),
        (WMT_METAL310, u32::MAX, true)
    );
    assert_eq!(
        Device::select_caps(320, 14, 5, true),
        (WMT_METAL310, u32::MAX, true)
    );

    // Non-Apple7: always Metal 3.1, max_object_threadgroups = 1024; concurrent compilation
    // disabled on macOS 16..=26 with minor < 2, enabled otherwise.
    assert_eq!(
        Device::select_caps(320, 15, 4, false),
        (WMT_METAL310, 1024, true)
    );
    assert_eq!(
        Device::select_caps(320, 16, 1, false),
        (WMT_METAL310, 1024, false)
    );
    assert_eq!(
        Device::select_caps(320, 26, 1, false),
        (WMT_METAL310, 1024, false)
    );
    assert_eq!(
        Device::select_caps(320, 26, 2, false),
        (WMT_METAL310, 1024, true)
    );

    // End-to-end `Device::new` (in tests `MTLDevice_supportsFamily` is false on this thread):
    // macOS 15.0 enables concurrent compilation; macOS 26.1 skips it.
    OS_VERSION.set((15, 0, 0));
    MAXIMIZE_CONCURRENT.set(None);
    // SAFETY: fake non-zero handle for the test binary's `NSObject_retain`/`NSObject_release`.
    let mtl = unsafe { Obj::adopt(0xD1) }.expect("non-zero handle");
    let dev = Device::new(mtl, 320);
    assert_eq!(dev.metal_version(), WMT_METAL310);
    assert_eq!(dev.max_object_threadgroups(), 1024);
    assert_eq!(MAXIMIZE_CONCURRENT.get(), Some(true));

    OS_VERSION.set((26, 1, 0));
    MAXIMIZE_CONCURRENT.set(None);
    // SAFETY: fake non-zero handle for the test binary's `NSObject_retain`/`NSObject_release`.
    let mtl = unsafe { Obj::adopt(0xD2) }.expect("non-zero handle");
    let dev = Device::new(mtl, 320);
    assert_eq!(dev.metal_version(), WMT_METAL310);
    assert_eq!(dev.max_object_threadgroups(), 1024);
    assert_eq!(MAXIMIZE_CONCURRENT.get(), None);
}

/// A front end's own queue sizes reach the device's queue and its rings (D3D9's, DXMTRS C4).
#[test]
fn with_config_reaches_the_queue() {
    use crate::queue::QueueConfig;
    // SAFETY: fake non-zero handle for the test binary's `NSObject_retain`/`NSObject_release`.
    let mtl = unsafe { Obj::adopt(0xD3) }.expect("non-zero handle");
    let config = QueueConfig {
        chunks: 4,
        staging_block: 0x10_0000,
        ring_lifetime: 30,
        ..QueueConfig::default()
    };
    let dev = Device::with_config(mtl, 320, config);
    let got = dev.queue().config();
    assert_eq!(
        (got.chunks, got.staging_block, got.ring_lifetime),
        (4, 0x10_0000, 30)
    );
    let staging = dev.queue().staging_ring_config();
    assert_eq!((staging.block, staging.lifetime), (0x10_0000, 30));
}
