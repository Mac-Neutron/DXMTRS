//! `IDirect3DQuery9` implementation backed by [`crate::rt::device::CoreDevice`].
//!
//! - `EVENT`: `Issue(D3DISSUE_END)` stamps the chunk being recorded, and `GetData` reports
//!   completion once that chunk retires (committing an open chunk on poll), unless
//!   `query.eventImmediate` answers every poll as completed.
//! - `OCCLUSION`: `Issue(BEGIN/END)` drives a `crate::rt::device::Occlusion` bracket that the
//!   query owns and ends at final `Release`. An `END` with no `BEGIN` answers `0`, and a pending
//!   `GetData(D3DGETDATA_FLUSH)` waits unless `query.flushImmediate` is enabled.

use core::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};

use mtld3d_shared::InPtr;
use mtld3d_types::{
    D3DGETDATA_FLUSH, D3DISSUE_BEGIN, D3DISSUE_END, D3DQUERYTYPE_EVENT, D3DQUERYTYPE_OCCLUSION,
    Guid, IDirect3DQuery9Vtbl,
};

use super::{D3D_OK, D3DERR_INVALIDCALL, LOG_TARGET, S_FALSE, device::DeviceInner};
use crate::rt::device::{Occlusion, OcclusionAnswer};

pub static DIRECT3D_QUERY9_VTBL: IDirect3DQuery9Vtbl = IDirect3DQuery9Vtbl {
    query_interface: query_query_interface,
    add_ref: query_add_ref,
    release: query_release,
    get_device: query_get_device,
    get_type: query_get_type,
    get_data_size: query_get_data_size,
    issue: query_issue,
    get_data: query_get_data,
};

#[repr(C)]
pub struct Direct3DQuery9 {
    vtbl: *const IDirect3DQuery9Vtbl,
    refcount: u32,
    inner: *mut QueryInner,
}

impl Direct3DQuery9 {
    pub fn new(device_inner: *mut DeviceInner, query_type: u32, data_size: u32) -> Self {
        let inner = Box::into_raw(Box::new(QueryInner {
            device_inner,
            query_type,
            data_size,
            end_seq: AtomicU64::new(0),
            occlusion: core::cell::RefCell::default(),
        }));
        Self {
            vtbl: &raw const DIRECT3D_QUERY9_VTBL,
            refcount: 1,
            inner,
        }
    }

    fn inner(&self) -> &QueryInner {
        // SAFETY: `self.inner` was installed by `Self::new` as a
        // `Box::into_raw` and is dropped only in `query_release` at
        // refcount zero, so it stays live for every live wrapper
        // reference.
        unsafe { &*self.inner }
    }
}

/// Returns the byte count `GetDataSize` reports.
///
/// `None` doubles as the flag telling the caller whether the query type is
/// supported.
pub const fn data_size_for(query_type: u32) -> Option<u32> {
    match query_type {
        // BOOL (EVENT) / DWORD pixel count (OCCLUSION): both are u32-sized.
        D3DQUERYTYPE_EVENT | D3DQUERYTYPE_OCCLUSION => Some(4),
        _ => None,
    }
}

struct QueryInner {
    device_inner: *mut DeviceInner,
    query_type: u32,
    /// Number of bytes `GetData` should write when `data != null`.
    data_size: u32,
    /// Chunk stamped when `Issue(D3DISSUE_END)` ran on an EVENT query; zero before any `END`.
    end_seq: AtomicU64,
    /// OCCLUSION bracket owned by this query on the core runtime device.
    occlusion: core::cell::RefCell<Occlusion>,
}

/// The poison a query never issued answers: every byte `0xdd`.
///
/// The runtime's uninitialised result.
const NEVER_ISSUED: u64 = 0xdddd_dddd_dddd_dddd;

/// Write an EVENT's BOOL: TRUE exactly when `status` is `D3D_OK`.
///
/// A short read takes the low bytes of it rather than nothing: the runtime fills what the caller
/// asked for.
unsafe fn write_signalled(data: *mut c_void, size: u32, status: i32) {
    let signalled = u32::from(status == D3D_OK).to_le_bytes();
    let n = (size as usize).min(signalled.len());
    // SAFETY: the caller guarantees `data` is non-null with at least `size` writable bytes, and
    // `n <= size`.
    unsafe { core::ptr::copy_nonoverlapping(signalled.as_ptr(), data.cast::<u8>(), n) };
}

/// Write the low `min(size, 8)` bytes of a 64-bit occlusion result.
///
/// D3D9 advertises `GetDataSize == 4` (DWORD) but the runtime backs
/// occlusion with a UINT64 and honors partial/oversized reads, so the
/// width is the caller's `size` capped at 8, not the advertised DWORD.
unsafe fn write_occlusion(data: *mut c_void, size: u32, value: u64) {
    let n = (size as usize).min(8);
    let bytes = value.to_le_bytes();
    // SAFETY: caller guarantees `data` is non-null with >= `size` writable
    // bytes and `size >= 1`; `n <= size`.
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast::<u8>(), n) };
}

#[inline]
fn query_timer(this: *mut c_void) -> mtld3d_core::perf::ApiTimer {
    use mtld3d_core::perf::{ApiCategory, ApiTimer};
    // SAFETY: vtable thunk; `this` is *mut Direct3DQuery9 per IDirect3DQuery9 ABI.
    let storage = (unsafe { InPtr::<Direct3DQuery9>::opt(this) }).and_then(|obj| {
        // SAFETY: the entry point holds the API lock and the device is live at timer entry.
        unsafe { crate::device::DeviceInner::perf_storage_of(obj.inner().device_inner) }
    });
    ApiTimer::start(storage, ApiCategory::Query)
}

extern "system" fn query_query_interface(
    this: *mut c_void,
    riid: *const Guid,
    ppv: *mut *mut c_void,
) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DQuery9>(this);
    let _timer = query_timer(this);
    // SAFETY: vtable thunk; `this`, `riid` and `ppv` are the caller's per the
    // IUnknown::QueryInterface ABI.
    unsafe {
        crate::com_ref::com_query_interface(
            this,
            riid,
            ppv,
            &[
                mtld3d_types::IID_IUNKNOWN,
                mtld3d_types::IID_IDIRECT3DQUERY9,
            ],
            query_add_ref,
            "IDirect3DQuery9",
        )
    }
}

extern "system" fn query_add_ref(this: *mut c_void) -> u32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DQuery9>(this);
    let _timer = query_timer(this);
    // SAFETY: IDirect3DQuery9 IUnknown AddRef thunk; the D3D9 ABI guarantees
    // `this` is the live wrapper for the call.
    unsafe { crate::com_ref::com_add_ref::<Direct3DQuery9>(this) }
}

extern "system" fn query_release(this: *mut c_void) -> u32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DQuery9>(this);
    let _timer = query_timer(this);
    // SAFETY: IDirect3DQuery9 IUnknown Release thunk; the D3D9 ABI guarantees
    // `this` is the live wrapper for the call.
    unsafe { crate::com_ref::com_release::<Direct3DQuery9>(this) }
}

/// Destroy a `Direct3DQuery9` wrapper once its refcount has reached zero.
///
/// # Safety
///
/// `this` must point to a live `Direct3DQuery9` wrapper at refcount zero;
/// caller must not access the wrapper afterwards.
unsafe fn finalize_query(this: *mut Direct3DQuery9) {
    // SAFETY: refcount reached zero; `(*this).inner` is the original
    // `Box::into_raw(QueryInner)` from `Self::new` and no other reference
    // can survive a zero refcount.
    let inner = unsafe { (*this).inner };
    // SAFETY: as above; the inner is live until it is dropped below.
    let inner_ref = unsafe { &*inner };
    if inner_ref.query_type == D3DQUERYTYPE_OCCLUSION {
        // SAFETY: a query's final `Release` finalizes it before giving back its device reference,
        // so `device_inner` is live, and the release holds the device's API lock.
        let dev = unsafe { &mut *inner_ref.device_inner };
        dev.core_mut()
            .occlusion_release(&mut inner_ref.occlusion.borrow_mut());
    }
    // SAFETY: sole owner of the inner allocation.
    drop(unsafe { Box::from_raw(inner) });
    // SAFETY: refcount reached zero; `this` is the original
    // `Box::into_raw(Direct3DQuery9)` allocation.
    drop(unsafe { Box::from_raw(this) });
}

// SAFETY: `refcount_mut` exposes this wrapper's own counter; `finalize` frees
// it exactly once at refcount zero. Queries have no bound-slot (private)
// refcount; they forward one device reference for their public lifetime.
unsafe impl crate::com_ref::ComChild for Direct3DQuery9 {
    fn refcount_mut(&mut self) -> &mut u32 {
        &mut self.refcount
    }
    fn owning_device(&self) -> *mut c_void {
        crate::device::device_wrapper_from(self.inner().device_inner)
    }
    unsafe fn finalize(this: *mut Self) {
        // SAFETY: forwarded from the engine; refcount is zero.
        unsafe { finalize_query(this) };
    }
}

extern "system" fn query_get_device(this: *mut c_void, device: *mut *mut c_void) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DQuery9>(this);
    let _timer = query_timer(this);
    // SAFETY: vtable thunk; `this` is *mut Direct3DQuery9 per its ABI, and `device` is
    // the caller's out-param.
    unsafe { crate::com_ref::com_get_device::<Direct3DQuery9>(this, device) }
}

extern "system" fn query_get_type(this: *mut c_void) -> u32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DQuery9>(this);
    let _timer = query_timer(this);
    // SAFETY: vtable thunk; `this` is *mut Direct3DQuery9 per IDirect3DQuery9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DQuery9>::opt(this) }) else {
        return 0;
    };
    obj.inner().query_type
}

extern "system" fn query_get_data_size(this: *mut c_void) -> u32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DQuery9>(this);
    let _timer = query_timer(this);
    // SAFETY: vtable thunk; `this` is *mut Direct3DQuery9 per IDirect3DQuery9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DQuery9>::opt(this) }) else {
        return 0;
    };
    obj.inner().data_size
}

extern "system" fn query_issue(this: *mut c_void, flags: u32) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DQuery9>(this);
    let _timer = query_timer(this);
    let unknown = flags & !(D3DISSUE_BEGIN | D3DISSUE_END);
    if unknown != 0 {
        mtld3d_shared::log_once_warn_by!(
            target: LOG_TARGET,
            key: u64::from(flags),
            "Query::Issue(flags={flags:#x}) has unknown bits {unknown:#x}: accepting as no-op",
        );
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3DQuery9 per IDirect3DQuery9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DQuery9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let inner = obj.inner();
    // SAFETY: the query retains its owning device throughout this call.
    let dev = unsafe { &mut *inner.device_inner };
    if let Err(hr) = dev.encoder_status() {
        return hr;
    }
    let rt = dev.core_mut();
    if inner.query_type == D3DQUERYTYPE_EVENT {
        if flags & D3DISSUE_END != 0 {
            inner.end_seq.store(rt.event_stamp(), Ordering::Release);
        }
    } else if inner.query_type == D3DQUERYTYPE_OCCLUSION {
        let mut occlusion = inner.occlusion.borrow_mut();
        if flags & D3DISSUE_BEGIN != 0 {
            rt.occlusion_begin(&mut occlusion);
        }
        if flags & D3DISSUE_END != 0 {
            rt.occlusion_end(&mut occlusion);
        }
    }
    D3D_OK
}

extern "system" fn query_get_data(
    this: *mut c_void,
    data: *mut c_void,
    size: u32,
    flags: u32,
) -> i32 {
    let _api = crate::com_ref::com_api_lock::<Direct3DQuery9>(this);
    let _timer = query_timer(this);
    // SAFETY: vtable thunk; `this` is *mut Direct3DQuery9 per IDirect3DQuery9 ABI.
    let Some(obj) = (unsafe { InPtr::<Direct3DQuery9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let inner = obj.inner();
    // SAFETY: the query's device reference keeps its owning DeviceInner alive.
    let dev = unsafe { &mut *inner.device_inner };
    if let Err(hr) = dev.encoder_status() {
        return hr;
    }
    let config = dev.config();
    let (event_immediate, flush_immediate) =
        (config.query_event_immediate, config.query_flush_immediate);
    let rt = dev.core_mut();
    let has_output = !data.is_null() && size != 0;
    if inner.query_type == D3DQUERYTYPE_EVENT {
        let done = event_immediate || rt.event_done(inner.end_seq.load(Ordering::Acquire));
        if let Err(hr) = dev.encoder_status() {
            return hr;
        }
        let status = if done { D3D_OK } else { S_FALSE };
        if has_output {
            // SAFETY: `has_output` guarantees `data` is non-null with at least `size` writable
            // bytes.
            unsafe { write_signalled(data, size, status) };
        }
        return status;
    }
    if inner.query_type == D3DQUERYTYPE_OCCLUSION {
        let flush = flags & D3DGETDATA_FLUSH != 0;
        let answer = rt.occlusion_poll(&inner.occlusion.borrow(), flush && !flush_immediate);
        if let Err(hr) = dev.encoder_status() {
            return hr;
        }
        let value = match answer {
            OcclusionAnswer::Never => NEVER_ISSUED,
            OcclusionAnswer::Count(count) => count,
            OcclusionAnswer::Pending if flush && flush_immediate => u64::from(u32::MAX),
            OcclusionAnswer::Pending => return S_FALSE,
        };
        if has_output {
            // SAFETY: `has_output` guarantees `data` is non-null with at least `size` writable
            // bytes, `size >= 1`.
            unsafe { write_occlusion(data, size, value) };
        }
        return D3D_OK;
    }
    if has_output {
        let wanted = inner.data_size.min(size) as usize;
        // SAFETY: `data` is non-null and per the D3D9 ABI points to a buffer of at least `size`
        // bytes; `wanted = min(data_size, size)` stays within that buffer.
        unsafe { core::ptr::write_bytes(data.cast::<u8>(), 0, wanted) };
        mtld3d_shared::log_once_warn_by!(
            target: LOG_TARGET,
            key: u64::from(inner.query_type),
            "stub IDirect3DQuery9::GetData for unknown query_type={} -> reporting zeros",
            inner.query_type,
        );
    }
    D3D_OK
}
