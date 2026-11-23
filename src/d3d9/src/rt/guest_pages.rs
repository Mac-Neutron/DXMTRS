//! Page boxes from the VM guest's winemetal (`WMTMemoryAlloc`) under the core runtime.
//!
//! mtld3d's runtime takes them from `mtld3d.so` (`AllocPages`); the core runtime has no Unix side
//! of its own, and the guest's winemetal answers 16 KiB-aligned pages the host can map (below
//! 2 GiB in a 32-bit process).

use core::ptr::NonNull;

use winemetal_sys::{WMTMemoryAlloc, WMTMemoryFree};

/// `len` bytes of pages, or `None` when winemetal has none.
#[must_use]
pub fn alloc(len: usize) -> Option<NonNull<u8>> {
    // SAFETY: winemetal's allocation; any length is a valid request.
    NonNull::new(unsafe { WMTMemoryAlloc(len as u64) }.cast::<u8>())
}

/// Gives back pages [`alloc`] answered.
pub fn free(ptr: NonNull<u8>, _len: usize) {
    // SAFETY: `ptr` came from `WMTMemoryAlloc` and is freed once.
    unsafe { WMTMemoryFree(ptr.as_ptr().cast()) };
}
