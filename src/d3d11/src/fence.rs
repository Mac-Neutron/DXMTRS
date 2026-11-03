//! D3D11 fence (`MTLD3D11Fence`, `d3d11_fence.hpp`, `d3d11_fence.cpp`).

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use std::sync::Arc;

use dxmt_core::fence::Fence as CoreFence;
use dxmt_util::private_data::PrivateData;
use winemetal_sys::{MTLSharedEvent_setWin32EventAtValue, MTLSharedEvent_signalValue, Obj};

use crate::{
    abi::{IID_ID3D11_DEVICE_CHILD, IID_ID3D11_FENCE, vtbl::ID3D11FenceVtbl},
    com::{
        ComObject, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, Hresult, IID_IUNKNOWN,
        S_OK,
    },
    device::Device,
};

/// `D3D11_FENCE_FLAG_NONE`.
pub const D3D11_FENCE_FLAG_NONE: u32 = 0x1;
/// `D3D11_FENCE_FLAG_SHARED`.
pub const D3D11_FENCE_FLAG_SHARED: u32 = 0x2;
/// `D3D11_FENCE_FLAG_SHARED_CROSS_ADAPTER`.
pub const D3D11_FENCE_FLAG_SHARED_CROSS_ADAPTER: u32 = 0x4;
/// `D3D11_FENCE_FLAG_NON_MONITORED`.
pub const D3D11_FENCE_FLAG_NON_MONITORED: u32 = 0x8;

const VALID_FENCE_FLAGS: u32 = D3D11_FENCE_FLAG_NONE
    | D3D11_FENCE_FLAG_SHARED
    | D3D11_FENCE_FLAG_SHARED_CROSS_ADAPTER
    | D3D11_FENCE_FLAG_NON_MONITORED;

static FENCE_VTBL: ID3D11FenceVtbl = ID3D11FenceVtbl {
    query_interface: fence_query_interface,
    add_ref: fence_add_ref,
    release: fence_release,
    get_device: fence_get_device,
    get_private_data: fence_get_private_data,
    set_private_data: fence_set_private_data,
    set_private_data_interface: fence_set_private_data_interface,
    create_shared_handle: fence_create_shared_handle,
    get_completed_value: fence_get_completed_value,
    set_event_on_completion: fence_set_event_on_completion,
};

/// `ID3D11Fence` implementation (`MTLD3D11FenceImpl`).
#[repr(C)]
pub struct Fence {
    vtbl: *const ID3D11FenceVtbl,
    refs: ComObject,
    device: *mut c_void,
    private_data: PrivateData,
    core: Option<Arc<CoreFence>>,
    signaled: AtomicU64,
    local_kmt: u32,
}

impl Fence {
    fn new_box(device: *mut c_void, initial_value: u64, local_kmt: u32) -> Box<Self> {
        // SAFETY: `device` is either null (in unit tests) or a valid COM device pointer.
        unsafe { crate::com::add_ref(device) };
        let core = Device::from_iface(device).map(|dev| {
            let mtl = dev.core().device();
            let fence = CoreFence::new(mtl, mtl);
            let shared = fence.shared_event();
            // SAFETY: `shared` is a live `MTLSharedEvent`.
            unsafe { MTLSharedEvent_signalValue(shared.handle(), initial_value) };
            fence
        });
        Box::new(Self {
            vtbl: &raw const FENCE_VTBL,
            refs: ComObject::with_initial_ref(),
            device,
            private_data: PrivateData::default(),
            core,
            signaled: AtomicU64::new(initial_value),
            local_kmt,
        })
    }

    /// Recovers `&Fence` from an `ID3D11Fence*` pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: `iface` is non-null and its first word holds the COM vtable pointer.
        let vtbl = unsafe { *iface.cast::<*const ID3D11FenceVtbl>() };
        if !ptr::eq(vtbl, &raw const FENCE_VTBL) {
            return None;
        }
        // SAFETY: `vtbl` matches `FENCE_VTBL`, so `iface` points to a live `Fence`.
        Some(unsafe { &*iface.cast::<Self>() })
    }

    /// Returns the underlying [`CoreFence`].
    ///
    /// # Panics
    ///
    /// When called on a test fence constructed without a live [`Device`].
    #[must_use]
    pub const fn core(&self) -> &Arc<CoreFence> {
        self.core
            .as_ref()
            .expect("fence created on a live device has a core fence")
    }

    /// Returns the `WMT::SharedEvent` handle (`MTLD3D11Fence::event`).
    #[must_use]
    pub fn event(&self) -> u64 {
        self.core.as_ref().map_or(0, |c| c.shared_event().handle())
    }

    /// Returns the fence's `MTLSharedEvent` if created on a live device.
    #[must_use]
    pub fn shared_event(&self) -> Option<Obj> {
        self.core.as_ref().map(|c| c.shared_event())
    }

    /// `ID3D11Fence::GetCompletedValue`.
    #[must_use]
    pub fn completed_value(&self) -> u64 {
        self.core.as_ref().map_or_else(
            || self.signaled.load(Ordering::Acquire),
            |c| c.completed_value(),
        )
    }

    /// Signals `value` on this fence (`MTLSharedEvent::signalValue`).
    pub fn signal(&self, value: u64) {
        if let Some(core) = &self.core {
            let shared = core.shared_event();
            // SAFETY: `shared` is a live `MTLSharedEvent`.
            unsafe { MTLSharedEvent_signalValue(shared.handle(), value) };
        }
        self.signaled.store(value, Ordering::Release);
    }

    /// `ID3D11Fence::SetEventOnCompletion`.
    pub fn set_event_on_completion(&self, value: u64, event: *mut c_void) -> Hresult {
        self.set_event_on_completion_impl(value, event)
    }

    fn set_event_on_completion_impl(&self, value: u64, event: *mut c_void) -> Hresult {
        if !event.is_null()
            && let (Some(core), Some(dev)) = (&self.core, Device::from_iface(self.device))
        {
            let shared = core.shared_event();
            let listener = dev.queue().shared_event_listener();
            // SAFETY: `shared` and `listener` are live handles; `event` is an opaque Win32 event.
            unsafe {
                MTLSharedEvent_setWin32EventAtValue(shared.handle(), listener, event, value);
            }
        }
        S_OK
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        // SAFETY: `self.device` was retained in `Fence::new_box`.
        unsafe { crate::com::release(self.device) };
    }
}

fn supports_iid(iid: &Guid) -> bool {
    *iid == IID_IUNKNOWN || *iid == IID_ID3D11_DEVICE_CHILD || *iid == IID_ID3D11_FENCE
}

fn create_impl(
    device: *mut c_void,
    initial_value: u64,
    flags: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null and writable by the caller.
        unsafe { *out = ptr::null_mut() };
    }
    if riid.is_null() || (flags & !VALID_FENCE_FLAGS) != 0 {
        return E_INVALIDARG;
    }
    let shared = (flags & (D3D11_FENCE_FLAG_SHARED | D3D11_FENCE_FLAG_SHARED_CROSS_ADAPTER)) != 0;
    if shared && Device::from_iface(device).is_none() {
        return E_FAIL;
    }
    // SAFETY: `riid` was checked non-null above.
    let iid = unsafe { *riid };
    if !supports_iid(&iid) {
        return E_NOINTERFACE;
    }
    if out.is_null() {
        return E_POINTER;
    }
    let fence = Box::into_raw(Fence::new_box(device, initial_value, 0));
    // SAFETY: `out` was checked non-null above.
    unsafe { *out = fence.cast::<c_void>() };
    S_OK
}

/// `ID3D11Device5::CreateFence`.
pub fn create(
    device: *mut c_void,
    initial_value: u64,
    flags: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    create_impl(device, initial_value, flags, riid, out)
}

/// Alias of [`create`] (`CreateFence`).
pub fn create_fence(
    device: *mut c_void,
    initial_value: u64,
    flags: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    create_impl(device, initial_value, flags, riid, out)
}

fn open_shared_impl(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = device;
    if !out.is_null() {
        // SAFETY: `out` is non-null and writable by the caller.
        unsafe { *out = ptr::null_mut() };
    }
    if (handle.addr() & 0xc000_0000) != 0 {
        crate::LOG.warn("OpenSharedFence: Invalid shared handle type");
        return E_INVALIDARG;
    }
    if handle.is_null() || riid.is_null() {
        if handle.is_null() && !riid.is_null() && !out.is_null() {
            crate::LOG.warn("OpenSharedFence: Failed to query resource: 0");
        }
        return E_INVALIDARG;
    }
    if out.is_null() {
        return E_POINTER;
    }
    crate::LOG.warn(&format!(
        "OpenSharedFence: Failed to query resource: {:x}",
        handle.addr()
    ));
    E_INVALIDARG
}

/// `ID3D11Device5::OpenSharedFence`.
pub fn open_shared(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    open_shared_impl(device, handle, riid, out)
}

/// Alias of [`open_shared`] (`OpenSharedFence`).
pub fn open_shared_fence(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    open_shared_impl(device, handle, riid, out)
}

unsafe extern "system-unwind" fn fence_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> Hresult {
    if ppv_object.is_null() {
        return E_POINTER;
    }
    // SAFETY: `ppv_object` is non-null and writable.
    unsafe { *ppv_object = ptr::null_mut() };
    if riid.is_null() {
        return E_NOINTERFACE;
    }
    // SAFETY: `riid` is non-null and points to a `Guid`.
    let iid = unsafe { *riid.cast::<Guid>() };
    if supports_iid(&iid) {
        // SAFETY: `this` is a live `Fence`.
        let _ = unsafe { (*this.cast::<Fence>()).refs.add_ref() };
        // SAFETY: `ppv_object` is non-null.
        unsafe { *ppv_object = this };
        return S_OK;
    }
    if crate::com::qi_log(&crate::abi::IID_ID3D11_QUERY, &iid) {
        crate::LOG.warn(&format!("D3D11Fence: Unknown interface query {iid}"));
    }
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn fence_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Fence`.
    unsafe { (*this.cast::<Fence>()).refs.add_ref() }
}

unsafe extern "system-unwind" fn fence_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Fence`.
    let (rem, free) = unsafe { (*this.cast::<Fence>()).refs.release_and_free() };
    if free {
        // SAFETY: the last reference was released; `this` was allocated via `Box::into_raw`.
        drop(unsafe { Box::from_raw(this.cast::<Fence>()) });
    }
    rem
}

unsafe extern "system-unwind" fn fence_get_device(this: *mut c_void, pp_device: *mut *mut c_void) {
    if pp_device.is_null() {
        return;
    }
    // SAFETY: `this` is a live `Fence`.
    let dev = unsafe { (*this.cast::<Fence>()).device };
    // SAFETY: `dev` is null or a valid COM pointer.
    unsafe { crate::com::add_ref(dev) };
    // SAFETY: `pp_device` is non-null.
    unsafe { *pp_device = dev };
}

unsafe extern "system-unwind" fn fence_get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    p_data_size: *mut u32,
    p_data: *mut c_void,
) -> Hresult {
    if guid.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `guid` is non-null and points to a `Guid`.
    let g = unsafe { *guid.cast::<Guid>() };
    // SAFETY: `this` is a live `Fence`.
    let pd = unsafe { &(*this.cast::<Fence>()).private_data };
    // SAFETY: `p_data_size` and `p_data` are forwarded from the COM caller.
    unsafe { pd.get_data(g, p_data_size, p_data) }
}

unsafe extern "system-unwind" fn fence_set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    data_size: u32,
    p_data: *const c_void,
) -> Hresult {
    if guid.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `guid` is non-null and points to a `Guid`.
    let g = unsafe { *guid.cast::<Guid>() };
    // SAFETY: `this` is a live `Fence`.
    let pd = unsafe { &(*this.cast::<Fence>()).private_data };
    // SAFETY: `p_data` and `data_size` are forwarded from the COM caller.
    unsafe { pd.set_data(g, data_size, p_data) }
}

unsafe extern "system-unwind" fn fence_set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    p_data: *const c_void,
) -> Hresult {
    if guid.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `guid` is non-null and points to a `Guid`.
    let g = unsafe { *guid.cast::<Guid>() };
    // SAFETY: `this` is a live `Fence`.
    let pd = unsafe { &(*this.cast::<Fence>()).private_data };
    // SAFETY: `p_data` is forwarded from the COM caller.
    unsafe { pd.set_interface(g, p_data.cast_mut()) }
}

unsafe extern "system-unwind" fn fence_create_shared_handle(
    this: *mut c_void,
    _attributes: *const c_void,
    _access: u32,
    _name: *const c_void,
    p_handle: *mut *mut c_void,
) -> Hresult {
    if !p_handle.is_null() {
        // SAFETY: `p_handle` is non-null.
        unsafe { *p_handle = ptr::null_mut() };
    }
    // SAFETY: `this` is a live `Fence`.
    let local_kmt = unsafe { (*this.cast::<Fence>()).local_kmt };
    if local_kmt == 0 {
        return E_INVALIDARG;
    }
    E_FAIL
}

unsafe extern "system-unwind" fn fence_get_completed_value(this: *mut c_void) -> u64 {
    // SAFETY: `this` is a live `Fence`.
    unsafe { (*this.cast::<Fence>()).completed_value() }
}

unsafe extern "system-unwind" fn fence_set_event_on_completion(
    this: *mut c_void,
    value: u64,
    h_event: *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a live `Fence`.
    unsafe { (*this.cast::<Fence>()).set_event_on_completion(value, h_event) }
}
