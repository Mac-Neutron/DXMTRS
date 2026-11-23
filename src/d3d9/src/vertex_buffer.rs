//! `IDirect3DVertexBuffer9` COM wrapper.
//!
//! Dynamic buffers are mapped directly through `CoreDevice` (`BufferMapMode::Direct`); all other
//! buffers use a CPU `PageBox` backing (`BufferMapMode::Staged`) whose dirty range is uploaded on
//! draw via [`VertexBufferInner::core_flush`].
//!
//! Layout follows the "state on Inner" pattern: the `#[repr(C)]` outer
//! struct only carries the vtable, refcount, and an opaque pointer to
//! the real state; everything else lives on `VertexBufferInner`.

use core::ffi::c_void;

use mtld3d_core::{
    buffer_backing::{BufferBacking, classify_backing, may_release_backing},
    buffer_rename::{BufferMapMode, classify_map_mode, may_trust_lock_bounds, records_dirty_range},
    dirty_range::DirtyRange,
    ids::BufferId,
    page_box::PageBox,
};
use mtld3d_shared::{InPtr, InPtrMut, OutPtr, ValueIn};
use mtld3d_types::{
    D3DFMT_VERTEXDATA, D3DLOCK_DISCARD, D3DLOCK_KNOWN_BITS, D3DLOCK_READONLY,
    D3DRTYPE_VERTEXBUFFER, D3DVERTEXBUFFER_DESC, Guid, IDirect3DVertexBuffer9Vtbl,
};

use super::{
    D3D_OK, D3DERR_INVALIDCALL, com_ref::ComUnknown, device::DeviceInner,
    private_data::PrivateDataStore,
};

static DIRECT3D_VERTEX_BUFFER9_VTBL: IDirect3DVertexBuffer9Vtbl = IDirect3DVertexBuffer9Vtbl {
    query_interface: vb_query_interface,
    add_ref: vb_add_ref,
    release: vb_release,
    get_device: vb_get_device,
    set_private_data: vb_set_private_data,
    get_private_data: vb_get_private_data,
    free_private_data: vb_free_private_data,
    set_priority: vb_set_priority,
    get_priority: vb_get_priority,
    pre_load: vb_pre_load,
    get_type: vb_get_type,
    lock: vb_lock,
    unlock: vb_unlock,
    get_desc: vb_get_desc,
};

#[repr(C)]
pub struct Direct3DVertexBuffer9 {
    vtbl: *const IDirect3DVertexBuffer9Vtbl,
    refcount: u32,
    /// Device-internal "bound slot" refcount, kept in sync by `CachedComPtr<_, Bound>`.
    ///
    /// The wrapper is destroyed only when both `refcount` and
    /// `private_refcount` reach zero.
    private_refcount: u32,
    inner: *mut VertexBufferInner,
}

pub struct VertexBufferInner {
    device_inner: *mut DeviceInner,
    buffer_id: BufferId,
    length: u32,
    usage: u32,
    fvf: u32,
    pool: u32,
    /// GUID-keyed application private data (`Set/Get/FreePrivateData`).
    ///
    /// Any stored `IUnknown` is released when this `VertexBufferInner` drops.
    private_data: PrivateDataStore,
    /// `Direct` (zero-copy `bytesNoCopy`) vs `Staged` (separate CPU staging + GPU device buffer).
    ///
    /// `Staged` does a dirty-range upload on Unlock. Decided once at
    /// creation from `usage`/`pool`; selects the `Lock`/`Unlock` path
    /// below. On a core device `Direct` is a buffer the core maps itself
    /// (DXMTRS C4c-2b) and every other is `Staged` (DXMTRS C4c-1).
    map_mode: BufferMapMode,
    /// `Staged` only: the byte range dirtied since the last upload.
    ///
    /// Accumulated across `Lock`s and flushed on `Unlock` (on a core
    /// device, by `core_flush` at the next draw, or at the `Unlock` of a
    /// re-created backing). Unused for `Direct`.
    dirty: DirtyRange,
    /// Backing storage for `Staged` buffers.
    ///
    /// Released for `Direct` and uploaded write-only buffers.
    backing: BufferBacking,
    /// Number of `Lock`s not yet ended by an `Unlock`.
    ///
    /// D3D9 lets a locked buffer be locked again, and every pointer handed
    /// out stays valid until the last `Unlock`, which is the one that
    /// publishes the writes. An `Unlock` with no lock out is logged once.
    lock_count: u32,
    /// References state blocks hold on this vertex buffer.
    ///
    /// A `D3DPOOL_DEFAULT` vertex buffer a state block keeps alive is a `Reset`
    /// blocker until this and the public refcount are both zero
    /// (`ComChild::state_block_refs_mut`).
    state_block_refs: u32,
    /// App-set resource priority, round-tripped by `GetPriority` / `SetPriority`.
    ///
    /// D3D9 honours priority for the pool a memory manager pages, which
    /// orders its evictions: `D3DPOOL_MANAGED` on a plain device and
    /// `D3DPOOL_DEFAULT` on an extended one. For every other pool both
    /// accessors are fixed at `0`. Metal has no eviction-order hint, so this
    /// is app-visible state only and never acted upon.
    priority: u32,
    core_created: bool,
}

impl VertexBufferInner {
    pub const fn buffer_id(&self) -> BufferId {
        self.buffer_id
    }

    /// The buffer's length in bytes, as created.
    #[must_use]
    pub const fn length(&self) -> u32 {
        self.length
    }

    /// The buffer's CPU backing bytes.
    ///
    /// Used as the `ProcessVertices` source read: the source stream is a
    /// system-memory or default buffer whose current backing holds what the
    /// app wrote. Empty for a buffer that released its backing, which
    /// `may_release_backing` keeps out of this path and the caller checks.
    #[must_use]
    pub fn backing(&self) -> &[u8] {
        self.backing.as_slice()
    }

    /// Whether the buffer holds no CPU copy of its contents.
    ///
    /// True for a `D3DUSAGE_WRITEONLY` default-pool buffer whose
    /// upload has been queued: its bytes live on the GPU alone until the
    /// next `Lock` re-creates the backing. True from its creation on for a
    /// buffer a core device maps directly, which never has one (DXMTRS
    /// C4c-2b).
    #[must_use]
    pub const fn backing_is_released(&self) -> bool {
        self.backing.is_released()
    }

    /// Give a released buffer a backing again, zeroed.
    ///
    /// Every write path calls this before it touches the backing. The
    /// fresh pages hold no contents: only what is written into them from
    /// here on matches the device buffer, which is what
    /// `BackingState::Partial` records.
    fn restore_backing(&mut self) {
        if !self.backing.is_released() {
            return;
        }
        self.backing
            .restore(PageBox::new_zeroed(self.length as usize));
        mtld3d_shared::log_once_trace_by!(
            target: crate::LOG_TARGET,
            key: self.buffer_id.raw(),
            "vertex buffer {:#x}: backing re-created for a write",
            self.buffer_id.raw()
        );
    }

    /// The buffer's creation FVF.
    #[must_use]
    pub const fn fvf(&self) -> u32 {
        self.fvf
    }

    /// Write processed vertices into the buffer at `offset`, clipped to its length.
    ///
    /// Nothing is written for an empty window. A `Direct` buffer writes through the core's lock of
    /// the window; a `Staged` buffer writes into its backing and dirties the range for `core_flush`
    /// (or uploads immediately when the backing was re-created after release).
    pub fn core_write_processed(
        &mut self,
        offset: usize,
        bytes: &[u8],
        core: &mut crate::rt::device::CoreDevice,
    ) {
        let length = self.length as usize;
        let end = offset.saturating_add(bytes.len()).min(length);
        if offset >= end {
            return;
        }
        let bytes = &bytes[..end - offset];
        let (min, max) = (
            u32::try_from(offset).expect("VB offset fits u32"),
            u32::try_from(end).expect("VB span fits u32"),
        );
        if matches!(self.map_mode, BufferMapMode::Direct) {
            let ptr = core.dynamic_lock(self.buffer_id, 0, self.usage, min, max - min);
            if ptr.is_null() {
                mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                    "d3d9 core: ProcessVertices' dynamic destination did not lock; nothing written");
                return;
            }
            // SAFETY: the core's lock answers `ptr` at byte `min` of its buffer's memory, which
            // holds the window (it ends inside the buffer); `bytes` is the caller's own memory.
            unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
            return;
        }
        self.restore_backing();
        let dst = self.backing.as_mut_slice();
        dst[offset..end].copy_from_slice(bytes);
        if self.backing.may_widen_upload() {
            self.dirty.conjoin(min, max - min, self.length);
        } else if core.buffer_upload(self.buffer_id, u64::from(min), bytes) {
            self.backing.note_upload(min, max);
        } else {
            mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                "d3d9 core: ProcessVertices' window into a re-created vertex buffer backing did not upload; the next draw uploads it");
            self.dirty.conjoin(min, max - min, self.length);
        }
    }

    pub const fn map_mode(&self) -> BufferMapMode {
        self.map_mode
    }

    /// Brings the buffer's core form up to its backing before a draw reads it.
    ///
    /// A `Staged` buffer missing from the core store is created at its backing's padded length and
    /// uploaded in full; otherwise any dirty span uploads. An accepted upload releases the backing
    /// when D3D9 promises no readback unless `ignore_lock_bounds` is set.
    #[must_use]
    pub fn core_flush(
        &mut self,
        core: &mut crate::rt::device::CoreDevice,
        ignore_lock_bounds: bool,
    ) -> bool {
        if matches!(self.map_mode, BufferMapMode::Direct) {
            return true;
        }
        if self.core_created && self.dirty.is_empty() {
            return true;
        }
        // A released backing's last upload carried every byte, and a lock re-creates the backing
        // before it records a range: nothing is owed.
        if self.backing.is_released() && self.dirty.is_empty() {
            return true;
        }
        let backing = self.backing.as_slice();
        let made = !self.core_created;
        if made && !self.backing.may_widen_upload() {
            // Unreachable: a backing is re-created only after an upload into the store, which
            // keeps the buffer until the D3D9 buffer is destroyed. Its zeros must not reach a new
            // core buffer.
            mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                "d3d9 core: a re-created vertex buffer backing has no core buffer; the draw is dropped");
            return false;
        }
        let (offset, end, bytes) = if made {
            let length = u64::try_from(backing.len());
            if !length.is_ok_and(|length| core.buffer_create(self.buffer_id, length)) {
                return false;
            }
            (0, self.length, Some(backing))
        } else {
            let Some((min, max)) = self.dirty.span() else {
                return true;
            };
            (min, max, backing.get(min as usize..max as usize))
        };
        if !bytes.is_some_and(|bytes| core.buffer_upload(self.buffer_id, u64::from(offset), bytes))
        {
            if made {
                core.forget_buffer(self.buffer_id);
            }
            return false;
        }
        if self.lock_count == 0 {
            self.dirty.clear();
        }
        self.core_created = true;
        self.backing.note_upload(offset, end);
        release_backing_after_upload(self, ignore_lock_bounds);
        true
    }
}

pub struct VertexBufferCreateInfo {
    pub device_inner: *mut DeviceInner,
    pub length: u32,
    pub usage: u32,
    pub fvf: u32,
    pub pool: u32,
}

impl Direct3DVertexBuffer9 {
    /// A buffer of `info.length` zeroed bytes, `None` when the process cannot allocate them.
    ///
    /// The CPU backing is the one allocation of a buffer create that may
    /// fail, which the create answers with `E_OUTOFMEMORY`.
    pub fn new(info: &VertexBufferCreateInfo) -> Option<Self> {
        let buffer_id = BufferId::new_unique();
        let class = classify_backing(info.usage, info.pool);
        let map_mode = classify_map_mode(info.usage, info.pool);
        // SAFETY: `info.device_inner` is null or the creating device, live, whose create entry
        // point uses no other reference to it until this returns.
        let core_mode =
            unsafe { core_map_mode(info.device_inner, map_mode, buffer_id, info.length) };
        let (map_mode, released) = core_mode.map_or((map_mode, false), |mode| {
            (mode, matches!(mode, BufferMapMode::Direct))
        });
        // A buffer the core maps directly keeps no backing. Any other is zeroed, not uninit: a
        // `Staged` buffer's first upload carries the whole staging region, so any byte the game
        // left alone has to be a defined value rather than heap residue. One `bzero` per create,
        // and renames keep using the recycle pool.
        let backing = if released {
            BufferBacking::released(info.length, class)
        } else {
            BufferBacking::new(
                PageBox::try_new_zeroed(info.length as usize)?,
                info.length,
                class,
            )
        };
        // A `Staged` buffer starts full-dirty, whatever
        // `buffer.ignoreLockBounds` says. Its device allocation is
        // `Private`, Metal does not zero it, and no blit command can fill
        // a buffer, so the opening upload is the only thing that can give
        // it defined contents. A fill made entirely through locks that
        // record no range (a `MANAGED` buffer's `READONLY` locks)
        // announces nothing, so that upload has to carry every byte or
        // the GPU reads an undefined buffer. It is also the one whole-buffer
        // range that is free: no draw has read the buffer yet, so it
        // cannot trip rename-at-overlap. `Direct` buffers share one
        // allocation with the GPU and never read `dirty`.
        // `texture_unlock_rect`'s `was_uploaded` gate is the same rule for
        // mips: a level filled only through READONLY locks still has to
        // reach the GPU once.
        let dirty = if matches!(map_mode, BufferMapMode::Staged) {
            DirtyRange::full(info.length)
        } else {
            DirtyRange::empty()
        };
        let inner = Box::into_raw(Box::new(VertexBufferInner {
            device_inner: info.device_inner,
            buffer_id,
            length: info.length,
            usage: info.usage,
            fvf: info.fvf,
            pool: info.pool,
            private_data: PrivateDataStore::default(),
            map_mode,
            dirty,
            backing,
            lock_count: 0,
            state_block_refs: 0,
            priority: 0,
            core_created: false,
        }));
        Some(Self {
            vtbl: &raw const DIRECT3D_VERTEX_BUFFER9_VTBL,
            refcount: 1,
            private_refcount: 0,
            inner,
        })
    }

    pub const fn vtbl(&self) -> &IDirect3DVertexBuffer9Vtbl {
        // SAFETY: `self.vtbl` is the `'static`
        // `DIRECT3D_VERTEX_BUFFER9_VTBL` installed at `Self::new`.
        unsafe { &*self.vtbl }
    }

    pub fn inner(&self) -> &VertexBufferInner {
        // SAFETY: `self.inner` was installed by `Self::new` as a
        // `Box::into_raw` and is dropped only in `vb_release` at refcount
        // zero, so it stays live for every live wrapper reference.
        unsafe { &*self.inner }
    }

    pub fn inner_mut(&mut self) -> &mut VertexBufferInner {
        // SAFETY: see [`Self::inner`] — same `Box::into_raw` lifetime
        // contract; `&mut self` guarantees exclusive access.
        unsafe { &mut *self.inner }
    }
}

/// The mode `CoreDevice` gives a new buffer `id` of `length` bytes classified as `mode`.
///
/// # Safety
///
/// `device` is null or a live device, no other reference to which is used during the call.
pub unsafe fn core_map_mode(
    device: *mut DeviceInner,
    mode: BufferMapMode,
    id: BufferId,
    length: u32,
) -> Option<BufferMapMode> {
    // SAFETY: null or live, and no other reference to it used meanwhile (the caller's contract).
    let core = unsafe { device.as_mut() }?.core_mut();
    let direct = matches!(mode, BufferMapMode::Direct) && core.dynamic_create(id, length);
    Some(if direct {
        BufferMapMode::Direct
    } else {
        BufferMapMode::Staged
    })
}

/// The pointer at `offset` that `CoreDevice::dynamic_lock` returns for a `Direct` buffer.
///
/// # Safety
///
/// `device` is null or a live device, no other reference to which is used during the call.
pub unsafe fn core_lock(
    device: *mut DeviceInner,
    mode: BufferMapMode,
    id: BufferId,
    usage: u32,
    offset: u32,
    size: u32,
    flags: u32,
) -> Option<*mut u8> {
    if !matches!(mode, BufferMapMode::Direct) {
        return None;
    }
    // SAFETY: null or live, and no other reference to it used meanwhile (the caller's contract).
    let core = unsafe { device.as_mut() }?.core_mut();
    Some(core.dynamic_lock(id, flags, usage, offset, size))
}

// ── IUnknown ──

#[inline]
fn vb_timer(this: *mut c_void) -> mtld3d_core::perf::ApiTimer {
    use mtld3d_core::perf::{ApiCategory, ApiTimer};
    // SAFETY: vtable thunk; `this` is *mut Direct3DVertexBuffer9 per ABI.
    let storage = (unsafe { InPtr::<Direct3DVertexBuffer9>::opt(this) }).and_then(|obj| {
        // SAFETY: the entry point holds the API lock and the device is live at timer entry.
        unsafe { crate::device::DeviceInner::perf_storage_of(obj.inner().device_inner) }
    });
    ApiTimer::start(storage, ApiCategory::VertexBuffer)
}

extern "system" fn vb_query_interface(
    this: *mut c_void,
    riid: *const Guid,
    ppv: *mut *mut c_void,
) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // SAFETY: vtable thunk; `this`, `riid` and `ppv` are the caller's per the
    // IUnknown::QueryInterface ABI.
    unsafe {
        crate::com_ref::com_query_interface(
            this,
            riid,
            ppv,
            &[
                mtld3d_types::IID_IUNKNOWN,
                mtld3d_types::IID_IDIRECT3DRESOURCE9,
                mtld3d_types::IID_IDIRECT3DVERTEXBUFFER9,
            ],
            vb_add_ref,
            "IDirect3DVertexBuffer9",
        )
    }
}

extern "system" fn vb_add_ref(this: *mut c_void) -> u32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // SAFETY: IDirect3DVertexBuffer9 IUnknown AddRef thunk; the D3D9 ABI
    // guarantees `this` is the live wrapper for the call.
    unsafe { crate::com_ref::com_add_ref::<Direct3DVertexBuffer9>(this) }
}

extern "system" fn vb_release(this: *mut c_void) -> u32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // SAFETY: IDirect3DVertexBuffer9 IUnknown Release thunk; the D3D9 ABI
    // guarantees `this` is the live wrapper for the call.
    unsafe { crate::com_ref::com_release::<Direct3DVertexBuffer9>(this) }
}

/// Destroy a `Direct3DVertexBuffer9` wrapper.
///
/// Called once both `refcount` and `private_refcount` have reached zero.
/// Hands the current backing `PageBox` off to the device's retention
/// pipeline so any in-flight GPU reads see live memory until the matching
/// submit retires.
///
/// # Safety
///
/// `this` must point to a live `Direct3DVertexBuffer9` wrapper with both
/// counters at zero; caller must not access the wrapper afterwards.
unsafe fn finalize_vertex_buffer(this: *mut Direct3DVertexBuffer9) {
    // SAFETY: caller asserts wrapper still live; both counters at zero
    // means no other reference can be outstanding.
    let obj = unsafe { &*this };
    let inner_ptr = obj.inner;
    // Take ownership of the inner on the API thread; its state has
    // to survive transit into the encoder-thread retention closure.
    // SAFETY: both counters reached zero; `inner_ptr` is the original
    // `Box::into_raw(VertexBufferInner)` from `Self::new` and no
    // other reference can survive.
    let mut inner_box = unsafe { Box::from_raw(inner_ptr) };
    drop(inner_box.backing.release());
    let buffer_id = inner_box.buffer_id;
    let device_inner = inner_box.device_inner;
    if !device_inner.is_null() {
        // SAFETY: `device_inner` was stamped at `Self::new` from a
        // live `DeviceInner`; the device outlives all its child
        // resources per D3D9 lifetime rules.
        let dev = unsafe { &mut *device_inner };
        dev.core_mut().forget_buffer(buffer_id);
    }
    // SAFETY: both counters reached zero; `this` is the original
    // `Box::into_raw(Direct3DVertexBuffer9)` allocation.
    drop(unsafe { Box::from_raw(this) });
}

impl ComUnknown for Direct3DVertexBuffer9 {
    fn private_refcount_inc(&mut self) {
        self.private_refcount += 1;
    }
    unsafe fn private_refcount_dec_maybe_finalize(this: *mut Self) {
        // SAFETY: caller asserts `this` points to a live wrapper with
        // at least one private refcount outstanding.
        let obj = unsafe { &mut *this };
        obj.private_refcount -= 1;
        if obj.refcount == 0 && obj.private_refcount == 0 {
            // SAFETY: both counters reached zero — no other reference
            // can survive; finalize takes exclusive ownership.
            unsafe { finalize_vertex_buffer(this) };
        }
    }
}

// SAFETY: `refcount_mut`/`private_refcount` expose this wrapper's own counters;
// `finalize` frees it exactly once when both reach zero.
unsafe impl crate::com_ref::ComChild for Direct3DVertexBuffer9 {
    fn refcount_mut(&mut self) -> &mut u32 {
        &mut self.refcount
    }
    fn blocks_reset_while_referenced(&self) -> bool {
        self.inner().pool == mtld3d_types::D3DPOOL_DEFAULT
    }
    fn state_block_refs_mut(&mut self) -> Option<&mut u32> {
        Some(&mut self.inner_mut().state_block_refs)
    }
    fn private_refcount(&self) -> u32 {
        self.private_refcount
    }
    fn owning_device(&self) -> *mut c_void {
        crate::device::device_wrapper_from(self.inner().device_inner)
    }
    unsafe fn finalize(this: *mut Self) {
        // SAFETY: forwarded from the engine — both counters are zero.
        unsafe { finalize_vertex_buffer(this) };
    }
}

// ── IDirect3DResource9 ──

extern "system" fn vb_get_device(this: *mut c_void, device: *mut *mut c_void) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // SAFETY: vtable thunk; `this` is *mut Direct3DVertexBuffer9 per its ABI, and `device` is
    // the caller's out-param.
    unsafe { crate::com_ref::com_get_device::<Direct3DVertexBuffer9>(this, device) }
}

extern "system" fn vb_set_private_data(
    this: *mut c_void,
    guid: *const Guid,
    data: *const c_void,
    size: u32,
    flags: u32,
) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // SAFETY: vtable in-param; `guid` is *const Guid per IDirect3DResource9 ABI; alignment is not
    // assumed.
    let Some(guid) = (unsafe { ValueIn::<Guid>::read_opt(guid.cast()) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DVertexBuffer9 per ABI.
    let Some(mut obj) = (unsafe { InPtrMut::<Direct3DVertexBuffer9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let store = &mut obj.inner_mut().private_data;
    // SAFETY: `data`/`size`/`flags` are the caller-supplied payload per the
    // D3D9 ABI; `set` validates them.
    unsafe { store.set(&guid, data, size, flags) }
}

extern "system" fn vb_get_private_data(
    this: *mut c_void,
    guid: *const Guid,
    data: *mut c_void,
    size: *mut u32,
) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // SAFETY: vtable in-param; `guid` is *const Guid per IDirect3DResource9 ABI; alignment is not
    // assumed.
    let Some(guid) = (unsafe { ValueIn::<Guid>::read_opt(guid.cast()) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DVertexBuffer9 per ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DVertexBuffer9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: `data`/`size` are the caller-owned out buffer + size slot per
    // the D3D9 ABI; the store validates the size before any copy.
    unsafe { obj.inner().private_data.get(&guid, data, size) }
}

extern "system" fn vb_free_private_data(this: *mut c_void, guid: *const Guid) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // SAFETY: vtable in-param; `guid` is *const Guid per IDirect3DResource9 ABI; alignment is not
    // assumed.
    let Some(guid) = (unsafe { ValueIn::<Guid>::read_opt(guid.cast()) }) else {
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable thunk; `this` is *mut Direct3DVertexBuffer9 per ABI.
    let Some(mut obj) = (unsafe { InPtrMut::<Direct3DVertexBuffer9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    obj.inner_mut().private_data.free(&guid)
}

// Priority is honoured for the pool a device's memory manager pages:
// `D3DPOOL_MANAGED` on a plain device, `D3DPOOL_DEFAULT` on an extended one.
// For every other pool both accessors are fixed at `0`. Metal has no
// eviction-order hint, so the value is stored and round-tripped but never
// acted upon.
extern "system" fn vb_set_priority(this: *mut c_void, priority: u32) -> u32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // SAFETY: vtable thunk; `this` is *mut Direct3DVertexBuffer9 per ABI.
    let Some(mut obj) = (unsafe { InPtrMut::<Direct3DVertexBuffer9>::opt(this) }) else {
        return 0;
    };
    let inner = obj.inner_mut();
    let extended = inner.pool == mtld3d_types::D3DPOOL_DEFAULT
        // SAFETY: a default-pool buffer forwards a reference to its device,
        // so `device_inner` is null or that live device.
        && unsafe { crate::device::device_inner_is_extended(inner.device_inner as u64) };
    if !mtld3d_core::pool::priority_settable(inner.pool, extended) {
        return 0;
    }
    core::mem::replace(&mut inner.priority, priority)
}

extern "system" fn vb_get_priority(this: *mut c_void) -> u32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // SAFETY: vtable thunk; `this` is *mut Direct3DVertexBuffer9 per ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DVertexBuffer9>::opt(this) }) else {
        return 0;
    };
    obj.inner().priority
}

extern "system" fn vb_pre_load(this: *mut c_void) {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // See IDirect3DTexture9::PreLoad — Metal has no resident-set hint.
    mtld3d_shared::log_once_info!(
        target: crate::LOG_TARGET,
        "IDirect3DVertexBuffer9::PreLoad: no Metal analog, no-op"
    );
}

extern "system" fn vb_get_type(this: *mut c_void) -> u32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    D3DRTYPE_VERTEXBUFFER
}

// ── IDirect3DVertexBuffer9 ──

extern "system" fn vb_lock(
    this: *mut c_void,
    offset_to_lock: u32,
    size_to_lock: u32,
    pp_data: *mut *mut c_void,
    flags: u32,
) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    if pp_data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DVertexBuffer9 per ABI.
    let Some(mut obj) = (unsafe { InPtrMut::<Direct3DVertexBuffer9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let inner = obj.inner_mut();
    if offset_to_lock > inner.length {
        // SAFETY: `pp_data` is non-null (checked above) and per the D3D9 ABI points to a writable
        // `*mut c_void` slot owned by the caller; alignment is not assumed.
        unsafe { OutPtr::write_opt(pp_data, core::ptr::null_mut()) };
        return D3DERR_INVALIDCALL;
    }
    if size_to_lock != 0 && offset_to_lock.saturating_add(size_to_lock) > inner.length {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "vb_lock: clamping out-of-range range (off={offset_to_lock}, size={size_to_lock}, len={})",
            inner.length
        );
    }
    // SAFETY: the buffer's device, null or live (it outlives its buffers); this entry point
    // uses no other reference to it during the call.
    let direct = unsafe {
        core_lock(
            inner.device_inner,
            inner.map_mode,
            inner.buffer_id,
            inner.usage,
            offset_to_lock,
            size_to_lock,
            flags,
        )
    };
    if let Some(ptr) = direct {
        // SAFETY: `pp_data` is non-null (checked above), the caller's writable slot.
        return unsafe { answer_lock(inner, flags, ptr, pp_data) };
    }

    if inner.backing.is_released() {
        if flags & D3DLOCK_READONLY != 0 {
            mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                "vb_lock: D3DLOCK_READONLY on a D3DUSAGE_WRITEONLY buffer whose backing was released; the mapped bytes read as zero");
        }
        inner.restore_backing();
    }

    if matches!(inner.map_mode, BufferMapMode::Staged) && records_dirty_range(flags, inner.pool) {
        if flags & D3DLOCK_DISCARD != 0
            && matches!(
                classify_map_mode(inner.usage, inner.pool),
                BufferMapMode::Staged
            )
        {
            mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                "vb_lock: D3DLOCK_DISCARD on a non-DYNAMIC (Staged) buffer: treating as a normal dirtied-range upload");
        }
        let trusted = may_trust_lock_bounds(
            flags,
            inner.usage,
            inner.pool,
            size_to_lock,
            ignore_lock_bounds(inner),
        );
        let trusted = trusted
            && (matches!(
                classify_map_mode(inner.usage, inner.pool),
                BufferMapMode::Staged
            ) || inner.device_inner.is_null());
        let discard = flags & D3DLOCK_DISCARD != 0;
        let widen = !trusted && (inner.backing.may_widen_upload() || discard);
        if !trusted && !widen {
            mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                "vb_lock: keeping the announced lock window on a re-created backing; a widened upload would overwrite device bytes the buffer no longer holds");
        }
        let (dirty_offset, dirty_size) = if widen {
            (0, 0)
        } else {
            (offset_to_lock, size_to_lock)
        };
        inner.dirty.conjoin(dirty_offset, dirty_size, inner.length);
    }

    // `offset_to_lock <= inner.length` is checked above and the backing is
    // allocated for `inner.length` bytes, so the offset lands inside it.
    let ptr = inner
        .backing
        .write_ptr_at(offset_to_lock as usize)
        .unwrap_or_else(core::ptr::null_mut);
    // SAFETY: `pp_data` is non-null (checked above), the caller's writable slot.
    unsafe { answer_lock(inner, flags, ptr, pp_data) }
}

/// The end of a `vb_lock` that has its pointer: the flags' check, then `ptr`, null when none maps.
///
/// # Safety
///
/// `pp_data` is non-null and, per the D3D9 ABI, points to a writable `*mut c_void` slot owned
/// by the caller.
unsafe fn answer_lock(
    inner: &mut VertexBufferInner,
    flags: u32,
    ptr: *mut u8,
    pp_data: *mut *mut c_void,
) -> i32 {
    let unknown = flags & !D3DLOCK_KNOWN_BITS;
    if unknown != 0 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "vb_lock: unrecognised D3DLOCK bits {unknown:#x} ignored");
    }
    if ptr.is_null() {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "vb_lock: no backing to map");
        // SAFETY: `pp_data` is the caller's writable slot (this function's contract); alignment
        // is not assumed.
        unsafe { OutPtr::write_opt(pp_data, core::ptr::null_mut()) };
        return D3DERR_INVALIDCALL;
    }
    // Counted only once the pointer exists: a refused Lock maps nothing for
    // an Unlock to end.
    inner.lock_count = inner.lock_count.saturating_add(1);
    // SAFETY: `pp_data` is the caller's writable slot (this function's contract); alignment is
    // not assumed.
    unsafe { OutPtr::write_opt(pp_data, ptr.cast::<c_void>()) };
    D3D_OK
}

extern "system" fn vb_unlock(this: *mut c_void) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    // SAFETY: vtable thunk; `this` is *mut Direct3DVertexBuffer9 per ABI.
    let Some(mut obj) = (unsafe { InPtrMut::<Direct3DVertexBuffer9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let inner = obj.inner_mut();
    if inner.lock_count == 0 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "vb_unlock: Unlock without matching Lock → S_OK");
    } else {
        inner.lock_count -= 1;
        if inner.lock_count != 0 {
            return D3D_OK;
        }
    }
    if !inner.device_inner.is_null()
        && matches!(inner.map_mode, BufferMapMode::Staged)
        && inner.dirty.span().is_some()
    {
        // SAFETY: the device outlives its buffers (D3D9 lifetime rules, as the finalizer relies
        // on); nothing else of it is borrowed in this entry point.
        let dev = unsafe { &mut *inner.device_inner };
        if let Err(hr) = dev.encoder_status() {
            return hr;
        }
        if !inner.backing.may_widen_upload() {
            let ignore = dev.config().buffer_ignore_lock_bounds;
            if !inner.core_flush(dev.core_mut(), ignore) {
                mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
                    "d3d9 core: a re-created vertex buffer's window did not upload at its Unlock; the next draw uploads it");
            }
        }
    }
    D3D_OK
}

/// `buffer.ignoreLockBounds` of the device that owns this buffer.
///
/// A buffer with no device behind it (the pointer is stamped at creation and
/// is null only for a buffer that outlived a teardown) answers the default.
fn ignore_lock_bounds(inner: &VertexBufferInner) -> bool {
    if inner.device_inner.is_null() {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "vb: no device behind the buffer; buffer.ignoreLockBounds reads as false");
        return false;
    }
    // SAFETY: `inner.device_inner` was stamped at `Self::new` from a live
    // `DeviceInner`; the device outlives all its child resources per D3D9
    // lifetime rules.
    let dev = unsafe { &*inner.device_inner };
    dev.config().buffer_ignore_lock_bounds
}

/// Release the CPU backing of a buffer whose upload just carried every byte.
///
/// The transient the upload owns holds the bytes until the GPU has them, so
/// the buffer's own copy is dead weight from here: `D3DUSAGE_WRITEONLY` in
/// `D3DPOOL_DEFAULT` is the one class D3D9 promises no readback, and inside a
/// 32-bit title that copy competes with the title's own address space. A
/// later `Lock` re-creates the backing and uploads only what it announces.
///
/// The trade is the one the texture-staging release already accepts: the GPU
/// holds the only copy, so nothing can re-upload these buffers if the Metal
/// device is recreated under them. `ignore_lock_bounds` is the device's
/// `buffer.ignoreLockBounds`, which the caller reads: a core device's draw
/// holds the device borrowed while it uploads (DXMTRS C4c-2b).
fn release_backing_after_upload(inner: &mut VertexBufferInner, ignore_lock_bounds: bool) {
    // `buffer.ignoreLockBounds` says this title writes outside the windows it
    // announces, and the only way to carry those writes is to upload the whole
    // buffer out of the CPU copy. Releasing the copy would leave that upload
    // nothing true to carry, so the knob keeps it.
    if inner.lock_count != 0
        || ignore_lock_bounds
        || !may_release_backing(inner.usage, inner.pool)
        || !inner.backing.may_widen_upload()
    {
        return;
    }
    // The release is this class's ordinary path and logs nothing. What would
    // be a bug is a read of the bytes afterwards, and the one read a lock
    // declares, `D3DLOCK_READONLY`, warns in `vb_lock`.
    drop(inner.backing.release());
}

extern "system" fn vb_get_desc(this: *mut c_void, desc: *mut D3DVERTEXBUFFER_DESC) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DVertexBuffer9>(this);
    let _timer = vb_timer(this);
    if desc.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DVertexBuffer9 per ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DVertexBuffer9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let inner = obj.inner();
    // SAFETY: `desc` is non-null (checked above) and per the D3D9 ABI
    // points to a writable `D3DVERTEXBUFFER_DESC` slot owned by the
    // caller; alignment is not assumed.
    unsafe {
        OutPtr::write_opt(
            desc,
            D3DVERTEXBUFFER_DESC {
                format: D3DFMT_VERTEXDATA,
                resource_type: D3DRTYPE_VERTEXBUFFER,
                usage: inner.usage,
                pool: inner.pool,
                size: inner.length,
                fvf: inner.fvf,
            },
        );
    }
    D3D_OK
}
