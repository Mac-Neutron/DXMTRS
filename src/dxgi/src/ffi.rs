//! What dxgi calls in winemetal: its devices and its displays.
//!
//! winemetal-sys declares them, and meson links winemetal. A host test binary still links, with
//! `-undefined dynamic_lookup`, and a function that calls one of these is out of the host tests'
//! reach. A Metal object is a winemetal handle (`obj_handle_t`, a `u64`).

#[cfg(windows)]
pub use winemetal_sys::WMTSetMetalShaderCachePath;
pub use winemetal_sys::{
    MTLDevice_currentAllocatedSize, MTLDevice_hasUnifiedMemory, MTLDevice_name,
    MTLDevice_recommendedMaxWorkingSetSize, MTLDevice_registryID, NSArray_count, NSArray_object,
    NSObject_release, NSObject_retain, NSString_getCString, WMTCopyAllDevices,
    WMTGetDisplayDescription, WMTGetPrimaryDisplayId, WMTGetSecondaryDisplayId,
};
