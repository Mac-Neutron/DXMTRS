//! What dxgi calls in winemetal: its devices, its displays and, natively, the window-system hooks.
//!
//! winemetal-sys declares them, but for `WMTGetWsiHooks`, declared here with the hooks' layout
//! dxgi mirrors. meson links winemetal. A host test binary still links, with `-undefined
//! dynamic_lookup`, and a function that calls one of these is out of the host tests' reach. A
//! Metal object is a winemetal handle (`obj_handle_t`, a `u64`).

#[cfg(not(windows))]
unsafe extern "C" {
    /// The hooks a VM host set, or null.
    pub fn WMTGetWsiHooks() -> *const crate::abi::WsiHooks;
}

#[cfg(windows)]
pub use winemetal_sys::WMTSetMetalShaderCachePath;
pub use winemetal_sys::{
    MTLDevice_currentAllocatedSize, MTLDevice_hasUnifiedMemory, MTLDevice_name,
    MTLDevice_recommendedMaxWorkingSetSize, MTLDevice_registryID, NSArray_count, NSArray_object,
    NSObject_release, NSObject_retain, NSString_getCString, WMTCopyAllDevices,
    WMTGetDisplayDescription, WMTGetPrimaryDisplayId, WMTGetSecondaryDisplayId,
};
