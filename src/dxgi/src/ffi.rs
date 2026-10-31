//! What dxgi calls in winemetal: its devices, its displays and, natively, the window-system hooks.
//!
//! meson links winemetal. Declared without `cfg(windows)` (but for the two of one platform): a
//! host test binary still links, with `-undefined dynamic_lookup`, and a function that calls one
//! of these is out of the host tests' reach. A Metal object is a winemetal handle
//! (`obj_handle_t`, a `u64`).

unsafe extern "C" {
    pub fn NSObject_retain(object: u64);
    pub fn NSObject_release(object: u64);
    pub fn NSArray_object(array: u64, index: u64) -> u64;
    pub fn NSArray_count(array: u64) -> u64;
    pub fn WMTCopyAllDevices() -> u64;
    pub fn MTLDevice_recommendedMaxWorkingSetSize(device: u64) -> u64;
    pub fn MTLDevice_currentAllocatedSize(device: u64) -> u64;
    pub fn MTLDevice_name(device: u64) -> u64;
    /// `NSString`'s text in `encoding` (`WMTStringEncoding`, a 64-bit enum), into `buffer`.
    pub fn NSString_getCString(string: u64, buffer: *mut u8, max_length: u64, encoding: u64)
    -> u32;
    pub fn MTLDevice_hasUnifiedMemory(device: u64) -> bool;
    pub fn WMTGetPrimaryDisplayId() -> u32;
    pub fn WMTGetSecondaryDisplayId() -> u32;
    pub fn WMTGetDisplayDescription(
        display_id: u32,
        desc: *mut winemetal_wire::WMTDisplayDescription,
    );
    pub fn MTLDevice_registryID(device: u64) -> u64;
    #[cfg(windows)]
    pub fn WMTSetMetalShaderCachePath(path: *const core::ffi::c_char) -> bool;
    /// The hooks a VM host set, or null.
    #[cfg(not(windows))]
    pub fn WMTGetWsiHooks() -> *const crate::abi::WsiHooks;
}
