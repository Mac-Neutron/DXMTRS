//! `ID3D12Fence1` (`d3d12_fence.cpp`): GPU/CPU timeline fences over [`dxmt_core::fence::Fence`].

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::fence::Fence;
use dxmt_util::{
    com::ComObject,
    private_data::{self, HasPrivateData, PrivateData},
};

use crate::{
    abi::{
        objects::D3D12_MULTIPLE_FENCE_WAIT_FLAG_ALL,
        vtbl::{ID3D12Fence1Vtbl, IUnknownVtbl},
    },
    com::{
        self, ComRefCounted, E_INVALIDARG, E_OUTOFMEMORY, Hresult, IID_ID3D12_DEVICE_CHILD,
        IID_ID3D12_FENCE, IID_ID3D12_FENCE1, IID_ID3D12_OBJECT, IID_ID3D12_PAGEABLE, IID_IUNKNOWN,
        S_FALSE, S_OK,
    },
    device::Device,
};

const INFINITE: u32 = 0xFFFF_FFFF;

#[cfg(windows)]
unsafe extern "system" {
    fn SetEvent(event: *mut c_void) -> i32;
    fn CreateEventW(
        attributes: *const c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[cfg(not(windows))]
unsafe extern "C-unwind" {
    fn wmt_native_set_event(handle: *mut c_void) -> i32;
}

fn set_win32_event(event: *mut c_void) {
    #[cfg(windows)]
    {
        // SAFETY: `event` is a caller-supplied non-null Win32 event handle.
        unsafe {
            SetEvent(event);
        }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: `event` is a caller-supplied non-null native Win32 event handle.
        unsafe {
            wmt_native_set_event(event);
        }
    }
}

fn create_temp_event() -> *mut c_void {
    #[cfg(windows)]
    {
        // SAFETY: creates an unnamed, auto-reset, initially non-signaled Win32 event.
        unsafe { CreateEventW(ptr::null(), 0, 0, ptr::null()) }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: creates an unnamed, auto-reset, initially non-signaled native event.
        unsafe { winemetal_sys::wmt_native_create_event(0, 0) }
    }
}

fn wait_and_close_temp_event(event: *mut c_void) {
    #[cfg(windows)]
    {
        // SAFETY: `event` was created by `create_temp_event`.
        unsafe {
            WaitForSingleObject(event, INFINITE);
        }
        // SAFETY: `event` is no longer needed after `WaitForSingleObject` returns.
        unsafe {
            CloseHandle(event);
        }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: `event` was created by `create_temp_event`.
        unsafe {
            winemetal_sys::wmt_native_wait_for_single_object(event, INFINITE);
        }
        // SAFETY: `event` is no longer needed after `wmt_native_wait_for_single_object` returns.
        unsafe {
            winemetal_sys::wmt_native_close_handle(event);
        }
    }
}

/// `MTLD3D12FenceImpl` (`d3d12_fence.cpp:27`).
#[repr(C)]
pub struct FenceObject {
    pub vtbl: &'static ID3D12Fence1Vtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub fence: Arc<Fence>,
    pub flags: u32,
}

impl ComRefCounted for FenceObject {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for FenceObject {
    fn drop(&mut self) {
        if !self.device.is_null() {
            // SAFETY: `self.device` was retained by `add_ref` when `FenceObject` was constructed.
            unsafe { com::release_com(self.device) };
        }
    }
}

impl HasPrivateData for FenceObject {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `FenceObject`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

static FENCE_VTBL: ID3D12Fence1Vtbl = ID3D12Fence1Vtbl {
    query_interface: fence_query_interface,
    add_ref: fence_add_ref,
    release: fence_release,
    get_private_data: private_data::get_private_data::<FenceObject>,
    set_private_data: private_data::set_private_data::<FenceObject>,
    set_private_data_interface: private_data::set_private_data_interface::<FenceObject>,
    set_name: private_data::set_name::<FenceObject>,
    get_device: fence_get_device,
    get_completed_value: fence_get_completed_value,
    set_event_on_completion: fence_set_event_on_completion,
    signal: fence_signal,
    get_creation_flags: fence_get_creation_flags,
};

unsafe extern "system-unwind" fn fence_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::scope!("fence.QueryInterface");
    if this.is_null() {
        // SAFETY: the out pointer is null or writable.
        return unsafe { com::qi_refuse(out, E_INVALIDARG) };
    }
    let qi = com::Qi {
        null_riid: E_INVALIDARG,
        silent: &[],
        log: &crate::LOG,
        miss: com::QiMiss::Quiet,
    };
    let add_ref = || {
        // SAFETY: `this` is a live `FenceObject`.
        unsafe { com::add_ref_com(this.cast::<FenceObject>()) };
    };
    // SAFETY: the application's `riid` and out pointer, null or valid; `this` is a live `FenceObject`.
    unsafe {
        com::qi_table(
            riid,
            out,
            &[(
                &[
                    IID_IUNKNOWN,
                    IID_ID3D12_OBJECT,
                    IID_ID3D12_DEVICE_CHILD,
                    IID_ID3D12_PAGEABLE,
                    IID_ID3D12_FENCE,
                    IID_ID3D12_FENCE1,
                ],
                this,
            )],
            add_ref,
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn fence_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `FenceObject` pointer.
    unsafe { com::add_ref_com(this.cast::<FenceObject>()) }
}

unsafe extern "system-unwind" fn fence_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `FenceObject` pointer.
    unsafe { com::release_com(this.cast::<FenceObject>()) }
}

unsafe extern "system-unwind" fn fence_get_device(
    this: *mut c_void,
    riid: *const c_void,
    device: *mut *mut c_void,
) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `FenceObject`.
    let dev = unsafe { (*this.cast::<FenceObject>()).device }.cast::<c_void>();
    if dev.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `dev` is a live `Device` whose vtable starts with `IUnknownVtbl`.
    let unk = unsafe { com::vtbl::<IUnknownVtbl>(dev) };
    // SAFETY: `dev` is the owning `Device` and `riid`/`device` are caller pointers.
    unsafe { (unk.query_interface)(dev, riid, device) }
}

unsafe extern "system-unwind" fn fence_get_completed_value(this: *mut c_void) -> u64 {
    crate::scope!("fence.GetCompletedValue");
    if this.is_null() {
        return 0;
    }
    // SAFETY: `this` is a live `FenceObject`.
    let obj = unsafe { &*this.cast::<FenceObject>() };
    obj.fence.completed_value()
}

unsafe extern "system-unwind" fn fence_set_event_on_completion(
    this: *mut c_void,
    value: u64,
    event: *mut c_void,
) -> Hresult {
    crate::scope!("fence.SetEventOnCompletion");
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `FenceObject`.
    let obj = unsafe { &*this.cast::<FenceObject>() };
    // SAFETY: `this` is a live `FenceObject`.
    let completed = unsafe { fence_get_completed_value(this) };
    complete_or_wait(
        completed,
        value,
        event,
        set_win32_event,
        |target| {
            let _ = obj.fence.wait(target, u64::MAX);
        },
        |ev, target| {
            if !obj.device.is_null() {
                // SAFETY: `obj.device` is a live `Device`.
                if let Some(listener) = unsafe { (*obj.device).events.as_ref() } {
                    listener.set_event_on_value(&obj.fence, ev, target);
                }
            }
        },
    )
}

unsafe extern "system-unwind" fn fence_signal(this: *mut c_void, value: u64) -> Hresult {
    crate::scope!("fence.Signal");
    crate::count!("#fence signals forwarded to the GPU event", 1);
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `FenceObject`.
    let obj = unsafe { &*this.cast::<FenceObject>() };
    obj.fence.signal(value);
    S_OK
}

unsafe extern "system-unwind" fn fence_get_creation_flags(this: *mut c_void) -> u32 {
    crate::scope!("fence.GetCreationFlags");
    if this.is_null() {
        return 0;
    }
    // SAFETY: `this` is a live `FenceObject`.
    unsafe { (*this.cast::<FenceObject>()).flags }
}

/// Casts a non-null `ID3D12Fence` pointer if its vtable matches [`FenceObject`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_fence<'a>(raw: *mut c_void) -> Option<&'a FenceObject> {
    if raw.is_null() {
        return None;
    }
    // SAFETY: `raw` is non-null and points to a COM object whose first field is its vtable pointer.
    let vtbl = unsafe { *(raw as *const *const ID3D12Fence1Vtbl) };
    if !ptr::eq(vtbl, &raw const FENCE_VTBL) {
        return None;
    }
    // SAFETY: the vtable check above confirms `raw` is a `FenceObject`.
    Some(unsafe { &*raw.cast::<FenceObject>() })
}

/// Creates a [`FenceObject`] wrapping an existing core [`Fence`] (`OpenSharedHandle`).
///
/// # Safety
///
/// `device`, `riid`, and `out` are caller-supplied COM pointers.
pub unsafe fn create_with_fence(
    device: *mut Device,
    fence: Arc<Fence>,
    flags: u32,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe { *out = ptr::null_mut() };
    }
    if device.is_null() {
        return E_INVALIDARG;
    }
    if out.is_null() {
        return S_FALSE;
    }
    // SAFETY: `device` is a live `Device` whose COM refcount is incremented for the child object.
    unsafe { (*device).com.add_ref() };
    let obj = Box::into_raw(Box::new(FenceObject {
        vtbl: &FENCE_VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device,
        fence,
        flags,
    }));
    // SAFETY: `obj` was just allocated with refcount 1; `fence_query_interface` adds a ref on success.
    let hr = unsafe { fence_query_interface(obj.cast(), riid, out) };
    // SAFETY: release the initial reference from `Box::into_raw`.
    unsafe { fence_release(obj.cast()) };
    hr
}

/// `ID3D12Device::CreateFence` (`d3d12_device.cpp:1277`).
///
/// # Safety
///
/// `this`, `riid`, and `fence` are caller-supplied COM pointers.
pub unsafe fn create_fence(
    this: *mut c_void,
    initial_value: u64,
    flags: u32,
    riid: *const c_void,
    fence: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateFence");
    if !fence.is_null() {
        // SAFETY: `fence` is non-null.
        unsafe { *fence = ptr::null_mut() };
    }
    if this.is_null() {
        return E_INVALIDARG;
    }
    let dev = this.cast::<Device>();
    // SAFETY: `dev` is a valid `Device`.
    let dev_ref = unsafe { &*dev };
    let (Some(mtl), Some(helper)) = (dev_ref.mtl.as_ref(), dev_ref.fence_helper.as_ref()) else {
        return E_INVALIDARG;
    };
    let core_fence = Fence::new(mtl, helper);
    core_fence.signal(initial_value);
    // SAFETY: `dev` is a valid `Device` and `riid`/`fence` are caller pointers.
    unsafe { create_with_fence(dev, core_fence, flags, riid, fence) }
}

/// `ID3D12Device1::SetEventOnMultipleFenceCompletion` (`d3d12_device.cpp:1487`).
///
/// # Safety
///
/// `this`, `fences`, `values`, and `event` are caller-supplied pointers.
pub unsafe fn set_event_on_multiple_fence_completion(
    this: *mut c_void,
    fences: *const *mut c_void,
    values: *const u64,
    num_fences: u32,
    flags: u32,
    event: *mut c_void,
) -> Hresult {
    crate::scope!("device.SetEventOnMultipleFenceCompletion");
    if num_fences == 0 {
        return S_OK;
    }
    if this.is_null() || fences.is_null() || values.is_null() {
        return E_INVALIDARG;
    }
    let count = num_fences as usize;
    // SAFETY: `fences` and `values` are non-null and hold `num_fences` elements per the D3D12 contract.
    let raw_fences = unsafe { core::slice::from_raw_parts(fences, count) };
    // SAFETY: `values` is non-null and holds `num_fences` elements.
    let target_values = unsafe { core::slice::from_raw_parts(values, count) };
    let mut core_fences: Vec<&Fence> = Vec::with_capacity(count);
    for &raw in raw_fences {
        // SAFETY: each element of `raw_fences` is a caller-supplied `ID3D12Fence` pointer.
        let Some(fence_obj) = (unsafe { cast_fence(raw) }) else {
            return E_INVALIDARG;
        };
        core_fences.push(&fence_obj.fence);
    }
    let dev = this.cast::<Device>();
    // SAFETY: `dev` is a valid `Device`.
    let Some(listener) = (unsafe { (*dev).events.as_ref() }) else {
        return E_INVALIDARG;
    };
    let all = flags == D3D12_MULTIPLE_FENCE_WAIT_FLAG_ALL;
    let wait_event = if event.is_null() {
        let tmp = create_temp_event();
        if tmp.is_null() {
            return E_OUTOFMEMORY;
        }
        tmp
    } else {
        event
    };
    listener.set_event_on_values(&core_fences, target_values, all, wait_event);
    if event.is_null() {
        wait_and_close_temp_event(wait_event);
    }
    S_OK
}

fn complete_or_wait<S, W, R>(
    completed_value: u64,
    value: u64,
    event: *mut c_void,
    set_event_now: S,
    wait_blocking: W,
    register_listener: R,
) -> Hresult
where
    S: FnOnce(*mut c_void),
    W: FnOnce(u64),
    R: FnOnce(*mut c_void, u64),
{
    if completed_value >= value {
        if !event.is_null() {
            set_event_now(event);
        }
        return S_OK;
    }
    if event.is_null() {
        wait_blocking(value);
        return S_OK;
    }
    register_listener(event, value);
    S_OK
}

#[cfg(test)]
mod tests;
