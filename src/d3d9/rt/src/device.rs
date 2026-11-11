//! The core device under D3D9: its Metal device and its queue profile.
//!
//! The Metal device is winemetal's (`WMTCopyAllDevices`), the one whose registry ID the adapter
//! `GetDeviceInfo` reported has; the queue takes D3D9's memory profile ([`queue_profile`]).

use dxmt_core::{device::Device, queue::QueueConfig};
use winemetal_sys::{
    MTLDevice_registryID, NSArray_count, NSArray_object, NSObject_release, Obj, WMTCopyAllDevices,
};

/// The `log` target of the core runtime's lines.
pub const LOG_TARGET: &str = "mtld3d::d3d9::core";

/// A D3D9 device's core: the Metal device and the core's command queue.
pub struct CoreDevice {
    device: Device,
}

impl CoreDevice {
    /// A core device on `mtl` whose queue has `profile`'s sizes; logs the device line once.
    ///
    /// # Panics
    ///
    /// When the queue's threads cannot be spawned.
    #[must_use]
    pub fn new(mtl: Obj, profile: QueueConfig) -> Self {
        let chunks = profile.chunks;
        let device = Device::with_config(mtl, 0, profile);
        // A perf build compiles out what is below a warning; the gates read this line there too.
        let level = if cfg!(feature = "perf-build") {
            log::Level::Warn
        } else {
            log::Level::Info
        };
        log::log!(target: LOG_TARGET, level, "d3d9 core: device on dxmt-core ({chunks} chunks)");
        if device.cmd_library_handle() == 0 && device.device().handle() > 0x1000 {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: no command library (dxmt_command.metallib), so nothing presents"
            );
        }
        Self { device }
    }

    /// The core device.
    #[must_use]
    pub const fn device(&self) -> &Device {
        &self.device
    }

    /// The core device, mutably: its queue records.
    pub const fn device_mut(&mut self) -> &mut Device {
        &mut self.device
    }
}

/// D3D9's queue sizes in this process: [`queue_profile_for`] its pointer width.
#[must_use]
pub fn queue_profile() -> QueueConfig {
    queue_profile_for(usize::BITS)
}

/// D3D9's queue sizes for a process of `pointer_bits`.
///
/// D3D11's, whose 32 chunks also cover D3D9's frame latency of up to 30; a 32-bit process's
/// staging and argument rings take 1 MiB blocks, which it places in its own address space.
#[must_use]
pub fn queue_profile_for(pointer_bits: u32) -> QueueConfig {
    let mut profile = QueueConfig::default();
    if pointer_bits <= 32 {
        profile.staging_block = 0x10_0000;
        profile.gpu_heap = 0x10_0000;
    }
    profile
}

/// winemetal's device whose registry ID is `registry_id`; the first device when none is.
///
/// `None` when winemetal lists no device. A fallback logs a warning: the adapter `GetDeviceInfo`
/// described is then not the one the core renders on.
#[must_use]
pub fn select_device(registry_id: u64) -> Option<Obj> {
    // SAFETY: winemetal's call; the array it answers is this function's to release.
    let devices = unsafe { WMTCopyAllDevices() };
    if devices == 0 {
        return None;
    }
    // SAFETY: a live array of devices.
    let count = unsafe { NSArray_count(devices) };
    // SAFETY: (each) an index below the array's count.
    let at = |index: u64| unsafe { NSArray_object(devices, index) };
    let found = (0..count)
        .map(at)
        // SAFETY: a device of the live array.
        .find(|&device| unsafe { MTLDevice_registryID(device) } == registry_id);
    let picked = found.or_else(|| {
        (count > 0).then(|| {
            log::warn!(
                target: LOG_TARGET,
                "d3d9 core: no Metal device has registry ID {registry_id:#x}; the first one renders"
            );
            at(0)
        })
    });
    // SAFETY: the array's element, retained before the array (which holds it) is released.
    let device = picked.and_then(|handle| unsafe { Obj::retain(handle) });
    // SAFETY: the reference `WMTCopyAllDevices` gave.
    unsafe { NSObject_release(devices) };
    device
}

#[cfg(test)]
mod tests;
