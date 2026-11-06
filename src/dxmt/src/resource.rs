//! Buffers, textures, their allocations and their views.
//!
//! A [`Buffer`] or a [`Texture`] describes a resource; an allocation is the Metal object behind it,
//! replaced (renamed) as the front end discards the contents, and kept alive by the command
//! chunks that use it, which [`BufferAllocation::check_retained`] lets record once. A view is a
//! key: a [`TextureViewKey`] is its descriptor's own encoding and a [`BufferViewKey`] its
//! format's, so dimensions and formats decode from the key without a lock, and each allocation
//! makes the Metal view of a key the first time it is asked for.
//!
//! Behaviour is the C++ `Buffer`'s and `Texture`'s. Written ahead for the switches of
//! `d3d11.dll` and `d3d12.dll`, which are its first callers.

use core::{
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use std::alloc::{self, Layout};

use dxmt_util::sync::lock;
use winemetal_sys::{
    Obj,
    wire::{
        WMT_RESOURCE_CPU_CACHE_MODE_WRITE_COMBINED, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
        WMT_RESOURCE_STORAGE_MODE_MANAGED, WMT_RESOURCE_STORAGE_MODE_MEMORYLESS,
        WMT_RESOURCE_STORAGE_MODE_PRIVATE,
    },
};

mod buffer;
pub mod dims;
mod texture;
pub mod texture_info;
pub mod view;

/// The page suballocations share and placed memory aligns to: meson's `DXMT_PAGE_SIZE`.
const PAGE_SIZE: u64 = 4096;

pub use buffer::{
    Buffer, BufferAllocation, BufferAllocationFlags, BufferView, BufferViewDescriptor,
    BufferViewKey, External,
};
pub use texture::{
    RenamableTexturePool, Texture, TextureAllocation, TextureAllocationFlags, TextureView,
    TextureViewDescriptor, TextureViewKey,
};

/// The last command sequence that retained an allocation.
#[derive(Default)]
struct Retained(AtomicU64);

impl Retained {
    /// Records `seq`; true when it was already the last one, so the chunk holds it already.
    ///
    /// One recording thread calls it, as the C++'s `checkRetained`; the swap only keeps a reader
    /// on another thread from racing it.
    fn check(&self, seq: u64) -> bool {
        self.0.swap(seq, Ordering::Relaxed) == seq
    }

    fn last(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Pages an allocation owns and frees when it drops.
enum Placed {
    /// The global allocator's, aligned to [`PAGE_SIZE`].
    Heap { addr: usize, layout: Layout },
    /// The VM guest's `WMTMemoryAlloc`, which the host can map.
    #[cfg(feature = "guest-vm")]
    Guest(usize),
}

impl Placed {
    /// Pages a buffer placed in CPU memory wraps: the guest's shared memory in a VM guest build.
    fn pages(length: u64) -> Option<Self> {
        #[cfg(feature = "guest-vm")]
        {
            // SAFETY: no precondition; the memory is freed once, by this value's drop.
            let p = unsafe { winemetal_sys::WMTMemoryAlloc(length) };
            (!p.is_null()).then(|| Self::Guest(p.expose_provenance()))
        }
        #[cfg(not(feature = "guest-vm"))]
        Self::heap(length)
    }

    /// Page-aligned memory from the global allocator; `None` when it has none to give.
    fn heap(length: u64) -> Option<Self> {
        let size = usize::try_from(length).ok().filter(|&s| s != 0)?;
        let layout = Layout::from_size_align(size, usize::try_from(PAGE_SIZE).ok()?).ok()?;
        // SAFETY: the layout's size is not zero (filtered above).
        let p = unsafe { alloc::alloc(layout) };
        (!p.is_null()).then(|| Self::Heap {
            addr: p.expose_provenance(),
            layout,
        })
    }

    const fn addr(&self) -> usize {
        match *self {
            Self::Heap { addr, .. } => addr,
            #[cfg(feature = "guest-vm")]
            Self::Guest(addr) => addr,
        }
    }
}

impl Drop for Placed {
    fn drop(&mut self) {
        match *self {
            Self::Heap { addr, layout } => {
                // SAFETY: `alloc::alloc` returned `addr` with this layout, freed only here.
                unsafe { alloc::dealloc(ptr::with_exposed_provenance_mut(addr), layout) };
            }
            #[cfg(feature = "guest-vm")]
            Self::Guest(addr) => {
                // SAFETY: `WMTMemoryAlloc` returned `addr`, freed only here.
                unsafe { winemetal_sys::WMTMemoryFree(ptr::with_exposed_provenance_mut(addr)) };
            }
        }
    }
}

/// A Metal view an allocation made, by its key.
struct CachedView<K> {
    key: K,
    texture: Option<Obj>,
    gpu_resource_id: u64,
}

/// `WMTResourceOptions` for an allocation's flags, whose storage bits buffers and textures share.
const STORAGE_CPU_WRITE_COMBINED: u8 = 1;
const STORAGE_PRIVATE: u8 = 1 << 1;
const STORAGE_MANAGED: u8 = 1 << 2;
const STORAGE_MEMORYLESS: u8 = 1 << 3;

const fn storage_options(flags: u8) -> u64 {
    let mut options = WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED;
    if flags & STORAGE_CPU_WRITE_COMBINED != 0 {
        options |= WMT_RESOURCE_CPU_CACHE_MODE_WRITE_COMBINED;
    }
    if flags & STORAGE_MEMORYLESS != 0 {
        options |= WMT_RESOURCE_STORAGE_MODE_MEMORYLESS;
    } else if flags & STORAGE_PRIVATE != 0 {
        options |= WMT_RESOURCE_STORAGE_MODE_PRIVATE;
    }
    if flags & STORAGE_MANAGED != 0 {
        options |= WMT_RESOURCE_STORAGE_MODE_MANAGED;
    }
    options
}

/// A reference of its own to the object `obj` holds one to.
fn another_ref(obj: &Obj) -> Obj {
    // SAFETY: `obj` holds a reference, so its handle is a live object's.
    let new = unsafe { Obj::retain(obj.handle()) };
    new.unwrap_or_else(|| unreachable!("an Obj's handle is never null"))
}

/// The handle `obj` holds, or the null handle.
fn handle(obj: Option<&Obj>) -> u64 {
    obj.map_or(0, Obj::handle)
}

#[cfg(test)]
mod tests;
