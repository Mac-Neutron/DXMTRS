//! `MTLD3D12Device` (`d3d12_device.{hpp,cpp}`): the D3D12 device struct and shared state.

pub mod factory;
pub mod features;
pub mod null;
pub mod residency;
pub mod temporal;
pub mod vtbl;

use core::{ffi::c_void, mem::offset_of, ptr};
use std::sync::{Arc, Mutex, OnceLock};

use dxmt_core::{fence::EventListener, library::Library, presenter::framegen::TemporalScaler};
use dxmt_util::{com::ComObject, private_data::PrivateData};
use rustc_hash::FxHashMap;
use winemetal_sys::Obj;
use winemetal_wire::WMTFXTemporalScalerInfo;

use crate::{
    abi::{ext::D3D12DeviceExtVtbl, vtbl::ID3D12Device10Vtbl},
    com::ComRefCounted,
};

/// `MTLD3D12Device` (`d3d12_device.cpp:341-370`).
///
/// Primary vtable pointer (`ID3D12Device10`) sits at offset 0; the companion `IMTLD3D12DeviceExt`
/// vtable pointer sits at [`Device::EXT_OFFSET`].
#[repr(C)]
pub struct Device {
    pub vtbl: &'static ID3D12Device10Vtbl,
    pub ext_vtbl: &'static D3D12DeviceExtVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub adapter: *mut c_void,
    pub mtl: Option<Obj>,
    pub min_feature_level: u32,
    pub advertise_numa: bool,
    pub temporal_supported: bool,
    pub residency: residency::Residency,
    pub null: null::NullResources,
    pub library: Option<Library>,
    pub formats: FxHashMap<u32, u32>,
    pub fence_helper: Option<Obj>,
    pub default_depth_stencil_state: Option<Obj>,
    pub depth_stencil_states: Mutex<FxHashMap<[u8; 16], Obj>>,
    pub events: Option<EventListener>,
    pub upscalers: Mutex<Vec<(WMTFXTemporalScalerInfo, Arc<TemporalScaler>)>>,
    pub timestamp_frequency: OnceLock<u64>,
}

// SAFETY: `adapter` is a thread-safe COM `IMTLDXGIAdapter` pointer owned for the lifetime of `Device`.
unsafe impl Send for Device {}
// SAFETY: all interior mutation on `Device` goes through atomics, `Mutex`, or `OnceLock`.
unsafe impl Sync for Device {}

impl ComRefCounted for Device {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Device {
    /// Byte offset of `ext_vtbl` inside `Device` (`IMTLD3D12DeviceExt` companion pointer).
    pub const EXT_OFFSET: usize = offset_of!(Self, ext_vtbl);

    /// Allocates an uninitialized device header for stub and vtable tests.
    #[must_use]
    pub fn new_dummy() -> Box<Self> {
        Box::new(Self {
            vtbl: &vtbl::DEVICE_VTBL,
            ext_vtbl: &vtbl::DEVICE_EXT_VTBL,
            com: ComObject::with_initial_ref(),
            private_data: PrivateData::default(),
            adapter: ptr::null_mut(),
            mtl: None,
            min_feature_level: crate::D3D_FEATURE_LEVEL_11_0,
            advertise_numa: false,
            temporal_supported: false,
            residency: residency::Residency::new(),
            null: null::NullResources::new(),
            library: None,
            formats: FxHashMap::default(),
            fence_helper: None,
            default_depth_stencil_state: None,
            depth_stencil_states: Mutex::new(FxHashMap::default()),
            events: None,
            upscalers: Mutex::new(Vec::new()),
            timestamp_frequency: OnceLock::new(),
        })
    }

    /// The 256 KiB device-wide atomic lock table GPU virtual address (`d3d12_device.cpp:1981`).
    #[must_use]
    pub fn lock_table(&self) -> u64 {
        null::atomic64_locks(self)
    }

    /// Format capability bits (`dxmt_core::format::CAP_*`) for `format` (`WMTPixelFormat`).
    #[must_use]
    pub fn format_capability(&self, format: u32) -> u32 {
        self.formats.get(&format).copied().unwrap_or(0)
    }
}
