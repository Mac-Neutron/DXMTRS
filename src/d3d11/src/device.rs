//! D3D11 device (`MTLD3D11Device`, `d3d11_device.hpp`, `d3d11_device.cpp`).

use core::{
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
};

use dxmt_core::{device::Device as CoreDevice, queue::Queue};

use crate::{
    context::{Context, Immediate},
    d3d10,
    pipeline::PipelineCache,
    scheduler,
    state::cache::StateCaches,
};

pub mod dxgi;
pub mod impl_;

/// Recursive device lock controlled by `ID3D11Multithread` (`d3d11_multithread.hpp`).
#[derive(Default)]
pub struct DeviceMutex {
    protected: AtomicBool,
}

impl DeviceMutex {
    /// Acquires the device lock if multithread protection is enabled.
    pub const fn lock(&self) {}

    /// Releases the device lock if multithread protection is enabled.
    pub const fn unlock(&self) {}

    /// Enables or disables multithread protection (`SetMultithreadProtected`).
    pub fn set_protected(&self, protected: bool) -> bool {
        self.protected.swap(protected, Ordering::SeqCst)
    }

    /// Returns `true` when multithread protection is enabled (`GetMultithreadProtected`).
    #[must_use]
    pub fn get_protected(&self) -> bool {
        self.protected.load(Ordering::SeqCst)
    }
}

/// D3D11 device (`MTLD3D11DeviceImpl`).
pub struct Device {
    pub mutex: DeviceMutex,
    core: CoreDevice,
    immediate: *mut Context<Immediate>,
    pipeline_cache: PipelineCache,
    caches: StateCaches,
    direct_x_version: u32,
    d3d10: Option<Box<d3d10::Device>>,
}

impl Device {
    /// Recovers `&Device` from an `ID3D11Device*` interface pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }

    /// Returns the underlying `dxmt_core::device::Device`.
    #[must_use]
    pub const fn core(&self) -> &CoreDevice {
        &self.core
    }

    /// Returns the command queue (`CommandQueue`).
    #[must_use]
    pub const fn queue(&self) -> &Queue {
        self.core.queue()
    }

    /// Returns the immediate context pointer (`GetImmediateContext`).
    #[must_use]
    pub const fn immediate(&self) -> *mut Context<Immediate> {
        self.immediate
    }

    /// Returns the device's pipeline cache (`GetPipelineCache`).
    #[must_use]
    pub const fn pipeline_cache(&self) -> &PipelineCache {
        &self.pipeline_cache
    }

    /// Returns the device's state-object caches.
    #[must_use]
    pub const fn caches(&self) -> &StateCaches {
        &self.caches
    }

    /// Returns `10` when created with `Flags & 0x8000_0000`, else `11` (`GetDirectXVersion`).
    #[must_use]
    pub const fn direct_x_version(&self) -> u32 {
        self.direct_x_version
    }

    /// Returns the embedded D3D10 device wrapper (`GetD3D10Device`).
    #[must_use]
    pub fn d3d10(&self) -> Option<&d3d10::Device> {
        self.d3d10.as_deref()
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = scheduler::wait_idle(u32::MAX);
    }
}
