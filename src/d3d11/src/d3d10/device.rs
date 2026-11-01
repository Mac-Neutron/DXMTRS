//! `ID3D10Device1` and `ID3D10Multithread` vtables (`d3d10_device.cpp`).

use core::ffi::c_void;

/// D3D10 device wrapper (`MTLD3D10Device`).
#[repr(C)]
pub struct Device {
    pub d3d11: *mut c_void,
    pub context: *mut c_void,
}

impl Device {
    /// Creates a new `MTLD3D10Device` wrapping `d3d11` and `context`.
    #[must_use]
    pub fn new(d3d11: *mut c_void, context: *mut c_void) -> Box<Self> {
        Box::new(Self { d3d11, context })
    }
}
