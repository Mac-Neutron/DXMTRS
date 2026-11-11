//! DXMT's core device (`dxmt_device.*`).
//!
//! Holds the Metal device, the command queue and the Metal shader version and object threadgroup
//! limit chosen from the host OS and GPU family.

use winemetal_sys::{
    MTLDevice_setShouldMaximizeConcurrentCompilation, MTLDevice_supportsFamily, Obj,
    WMTGetOSVersion,
    wire::{WMT_METAL_VERSION_MAX, WMT_METAL310, WMT_METAL320, WMTGPU_FAMILY_APPLE7},
};

use crate::queue::{Queue, QueueConfig};

pub mod adapter;

/// The core device (`dxmt::Device`).
pub struct Device {
    mtl: Obj,
    queue: Queue,
    cmd_library: Option<crate::library::Library>,
    metal_version: u32,
    max_object_threadgroups: u32,
}

impl Device {
    /// Creates a core device on `mtl` with `metal_version_option` (`dxmt.shaderMetalVersion`).
    ///
    /// # Panics
    ///
    /// When the command queue's worker threads cannot be spawned.
    #[must_use]
    pub fn new(mtl: Obj, metal_version_option: i32) -> Self {
        Self::with_config(mtl, metal_version_option, QueueConfig::default())
    }

    /// [`Device::new`] with a front end's own queue sizes (D3D9's memory profile, DXMTRS C4).
    ///
    /// # Panics
    ///
    /// When the command queue's worker threads cannot be spawned.
    #[must_use]
    pub fn with_config(mtl: Obj, metal_version_option: i32, config: QueueConfig) -> Self {
        let (mut major, mut minor, mut patch) = (0_u64, 0_u64, 0_u64);
        // SAFETY: all three pointers reference valid stack locals.
        unsafe { WMTGetOSVersion(&raw mut major, &raw mut minor, &raw mut patch) };
        // SAFETY: `mtl` holds a live Metal device handle.
        let supports_apple7 =
            unsafe { MTLDevice_supportsFamily(mtl.handle(), WMTGPU_FAMILY_APPLE7) };
        if !supports_apple7 {
            crate::log::warn("Experimental non-Apple GPU support");
        }
        let (metal_version, max_object_threadgroups, maximize_concurrent) =
            Self::select_caps(metal_version_option, major, minor, supports_apple7);
        if maximize_concurrent {
            // SAFETY: `mtl` holds a live Metal device handle.
            unsafe { MTLDevice_setShouldMaximizeConcurrentCompilation(mtl.handle(), true) };
        }
        let queue = Queue::new(&mtl, config).expect("command queue threads");
        let cmd_library = if mtl.handle() > 0x1000 {
            crate::library::Library::new(&mtl).ok()
        } else {
            None
        };
        Self {
            mtl,
            queue,
            cmd_library,
            metal_version,
            max_object_threadgroups,
        }
    }

    /// The Metal device.
    #[must_use]
    pub const fn device(&self) -> &Obj {
        &self.mtl
    }

    /// Raw handle to the internal `dxmt_command.metal` library, or `0` on unit-test devices.
    #[must_use]
    pub fn cmd_library_handle(&self) -> u64 {
        self.cmd_library
            .as_ref()
            .map_or(0, crate::library::Library::handle)
    }

    /// The command queue.
    #[must_use]
    pub const fn queue(&self) -> &Queue {
        &self.queue
    }

    /// Mutable access to the command queue.
    pub const fn queue_mut(&mut self) -> &mut Queue {
        &mut self.queue
    }

    /// The `WMTMetalVersion` shaders compile for.
    #[must_use]
    pub const fn metal_version(&self) -> u32 {
        self.metal_version
    }

    /// The object threadgroup limit per mesh dispatch.
    #[must_use]
    pub const fn max_object_threadgroups(&self) -> u32 {
        self.max_object_threadgroups
    }

    /// Selects `(metal_version, max_object_threadgroups, maximize_concurrent)` from host caps.
    #[must_use]
    pub const fn select_caps(
        metal_version_option: i32,
        macos_major: u64,
        macos_minor: u64,
        supports_apple7: bool,
    ) -> (u32, u32, bool) {
        let configured = if metal_version_option == WMT_METAL310.cast_signed() {
            WMT_METAL310
        } else if metal_version_option == WMT_METAL320.cast_signed() {
            WMT_METAL320
        } else {
            WMT_METAL_VERSION_MAX
        };
        let os_cap = if macos_major >= 15 {
            WMT_METAL320
        } else {
            WMT_METAL310
        };
        if supports_apple7 {
            let metal_version = if configured < os_cap {
                configured
            } else {
                os_cap
            };
            (metal_version, u32::MAX, true)
        } else {
            let maximize_concurrent = !(macos_major >= 16 && macos_major <= 26 && macos_minor < 2);
            (WMT_METAL310, 1024, maximize_concurrent)
        }
    }
}

#[cfg(test)]
mod tests;
