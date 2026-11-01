//! DXMT's core device (`dxmt_device.*`).
//!
//! Holds the Metal device, the command queue and the Metal shader version and object threadgroup
//! limit chosen from the host OS and GPU family.

use winemetal_sys::{Obj, wire::WMT_METAL310};

use crate::queue::{Queue, QueueConfig};

/// The core device (`dxmt::Device`).
pub struct Device {
    mtl: Obj,
    queue: Queue,
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
        let _ = metal_version_option;
        let queue = Queue::new(&mtl, QueueConfig::default()).expect("command queue threads");
        Self {
            mtl,
            queue,
            metal_version: WMT_METAL310,
            max_object_threadgroups: u32::MAX,
        }
    }

    /// The Metal device.
    #[must_use]
    pub const fn device(&self) -> &Obj {
        &self.mtl
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
}
