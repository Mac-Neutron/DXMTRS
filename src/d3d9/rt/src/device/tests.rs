use dxmt_fakes as _;
use winemetal_sys::Obj;

use super::{CoreDevice, queue_profile, queue_profile_for, select_device};

/// A 32-bit process's rings place 1 MiB blocks; a 64-bit one keeps D3D11's 4 MiB.
#[test]
fn queue_profile_places_1mib_blocks_on_32_bit() {
    let narrow = queue_profile_for(32);
    assert_eq!(narrow.staging_block, 0x10_0000);
    assert_eq!(narrow.gpu_heap, 0x10_0000);
    assert_eq!(narrow.chunks, 32);
    let wide = queue_profile_for(64);
    assert_eq!(wide.staging_block, 0x40_0000);
    assert_eq!(wide.gpu_heap, 0x40_0000);
    assert_eq!(queue_profile().staging_block, wide.staging_block);
}

/// The profile a core device is made with reaches its queue's rings.
#[test]
fn core_device_comes_up_on_the_fakes() {
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { Obj::adopt(0xd9) }.expect("a non-zero handle");
    let core = CoreDevice::new(mtl, queue_profile_for(32));
    let queue = core.device().queue();
    assert_eq!(queue.staging_ring_config().block, 0x10_0000);
    assert_eq!(queue.config().gpu_heap, 0x10_0000);
    assert_eq!(queue.config().chunks, 32);
    assert_eq!(core.device().device().handle(), 0xd9);
}

/// The device whose registry ID `GetDeviceInfo` reported; the first one when none has it.
#[test]
fn select_device_takes_the_reported_registry_id() {
    let second = dxmt_fakes::FAKE_DEVICES[1];
    let picked = select_device(dxmt_fakes::fake_registry_id(second)).expect("a device");
    assert_eq!(picked.handle(), second);
    let fallback = select_device(0x5eed).expect("a device");
    assert_eq!(fallback.handle(), dxmt_fakes::FAKE_DEVICES[0]);
}
