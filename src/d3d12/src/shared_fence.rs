//! Process-wide shared fence handle table (`d3d12_shared_fence.hpp`, `d3d12_device.cpp:1229-1262`).

#[cfg(windows)]
use core::mem;
use core::{ffi::c_void, ptr};
use std::sync::{LazyLock, Mutex, PoisonError};

use rustc_hash::FxHashMap;

use crate::{
    abi::{
        objects::{D3D12_FENCE_FLAG_SHARED, D3D12_FENCE_FLAG_SHARED_CROSS_ADAPTER},
        vtbl::{ID3D12Fence1Vtbl, IUnknownVtbl},
    },
    com::{self, E_INVALIDARG, E_NOTIMPL, E_OUTOFMEMORY, Hresult, IID_ID3D12_FENCE1, S_OK},
};

const DUPLICATE_SAME_ACCESS: u32 = 2;

#[cfg(windows)]
type NtQueryObjectFn =
    unsafe extern "system" fn(*mut c_void, i32, *mut c_void, u32, *mut u32) -> i32;

#[cfg(windows)]
unsafe extern "system" {
    fn GetModuleHandleA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn CreateEventW(
        attributes: *const c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn DuplicateHandle(
        source_process: *mut c_void,
        source_handle: *mut c_void,
        target_process: *mut c_void,
        target_handle: *mut *mut c_void,
        desired_access: u32,
        inherit_handle: i32,
        options: u32,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[cfg(windows)]
fn nt_query_object() -> Option<NtQueryObjectFn> {
    static QUERY: LazyLock<Option<NtQueryObjectFn>> = LazyLock::new(|| {
        // SAFETY: NUL-terminated literal; returns null if missing.
        let ntdll = unsafe { GetModuleHandleA(c"ntdll.dll".as_ptr().cast()) };
        // SAFETY: NUL-terminated literal; returns null on failure.
        let addr = unsafe { GetProcAddress(ntdll, c"NtQueryObject".as_ptr().cast()) };
        (!addr.is_null())
            // SAFETY: `ntdll.dll`'s `NtQueryObject` has `NtQueryObjectFn`'s signature.
            .then(|| unsafe { mem::transmute::<*mut c_void, NtQueryObjectFn>(addr) })
    });
    *QUERY
}

fn kernel_handle_count(handle: *mut c_void) -> u32 {
    if handle.is_null() {
        return 0;
    }
    #[cfg(windows)]
    {
        let Some(query) = nt_query_object() else {
            return 0;
        };
        let mut info = [0_u32; 14];
        // SAFETY: `info` is a 56-byte buffer for `OBJECT_BASIC_INFORMATION` (class 0).
        let status = unsafe {
            query(
                handle,
                0,
                info.as_mut_ptr().cast(),
                u32::try_from(mem::size_of_val(&info)).unwrap_or(56),
                ptr::null_mut(),
            )
        };
        if status == 0 { info[2] } else { 0 }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: `wmt_native_handle_count` safely inspects native handles and returns 0 for null.
        unsafe { winemetal_sys::wmt_native_handle_count(handle) }
    }
}

fn kernel_close_handle(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    #[cfg(windows)]
    {
        // SAFETY: `handle` is an owned Win32 kernel handle.
        unsafe {
            CloseHandle(handle);
        }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: `handle` is an owned native Win32 handle.
        unsafe {
            winemetal_sys::wmt_native_close_handle(handle);
        }
    }
}

fn create_and_dup_event() -> Option<(*mut c_void, *mut c_void)> {
    #[cfg(windows)]
    {
        // SAFETY: creates an unnamed manual-reset event.
        let h = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if h.is_null() {
            return None;
        }
        let mut ours: *mut c_void = ptr::null_mut();
        // SAFETY: duplicates `h` within the current process.
        let ok = unsafe {
            DuplicateHandle(
                winemetal_sys::CURRENT_PROCESS,
                h,
                winemetal_sys::CURRENT_PROCESS,
                &raw mut ours,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 || ours.is_null() {
            kernel_close_handle(h);
            return None;
        }
        Some((h, ours))
    }
    #[cfg(not(windows))]
    {
        // SAFETY: creates an unnamed manual-reset native event.
        let h = unsafe { winemetal_sys::wmt_native_create_event(1, 0) };
        if h.is_null() {
            return None;
        }
        let mut ours: *mut c_void = ptr::null_mut();
        // SAFETY: duplicates `h` within this process.
        let ok = unsafe {
            winemetal_sys::wmt_native_duplicate_handle(h, &raw mut ours, DUPLICATE_SAME_ACCESS)
        };
        if ok == 0 || ours.is_null() {
            kernel_close_handle(h);
            return None;
        }
        Some((h, ours))
    }
}

enum TableOp {
    Count,
    Close,
    ReleaseFence,
}

struct Entry {
    ours: usize,
    fence: usize,
}

struct SharedFenceTable {
    entries: FxHashMap<usize, Entry>,
}

impl SharedFenceTable {
    fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
        }
    }

    fn sweep(&mut self, ops: &mut dyn FnMut(usize, &TableOp) -> u32) {
        let mut dead = Vec::new();
        for (&client, entry) in &self.entries {
            if ops(entry.ours, &TableOp::Count) <= 1 {
                dead.push(client);
            }
        }
        for client in dead {
            if let Some(entry) = self.entries.remove(&client) {
                let _ = ops(entry.ours, &TableOp::Close);
                let _ = ops(entry.fence, &TableOp::ReleaseFence);
            }
        }
    }

    fn insert(
        &mut self,
        client_handle: usize,
        ours: usize,
        fence: usize,
        ops: &mut dyn FnMut(usize, &TableOp) -> u32,
    ) {
        self.sweep(ops);
        if let Some(prev) = self.entries.insert(client_handle, Entry { ours, fence }) {
            let _ = ops(prev.ours, &TableOp::Close);
            let _ = ops(prev.fence, &TableOp::ReleaseFence);
        }
    }

    fn get(
        &mut self,
        client_handle: usize,
        ops: &mut dyn FnMut(usize, &TableOp) -> u32,
    ) -> Option<usize> {
        self.sweep(ops);
        self.entries.get(&client_handle).map(|e| e.fence)
    }
}

fn real_ops(raw: usize, op: &TableOp) -> u32 {
    let ptr = ptr::with_exposed_provenance_mut::<c_void>(raw);
    match op {
        TableOp::Count => kernel_handle_count(ptr),
        TableOp::Close => {
            kernel_close_handle(ptr);
            0
        }
        TableOp::ReleaseFence => {
            // SAFETY: `ptr` is a COM `ID3D12Fence1` reference held by the table.
            unsafe {
                com::release(ptr);
            }
            0
        }
    }
}

fn global_table() -> &'static Mutex<SharedFenceTable> {
    static TABLE: LazyLock<Mutex<SharedFenceTable>> =
        LazyLock::new(|| Mutex::new(SharedFenceTable::new()));
    &TABLE
}

/// `ID3D12Device::CreateSharedHandle` (`d3d12_device.cpp:1229`).
///
/// # Safety
///
/// `this`, `object`, `attributes`, `name`, and `handle` are caller-supplied pointers.
pub unsafe fn create_shared_handle(
    _this: *mut c_void,
    object: *mut c_void,
    _attributes: *const c_void,
    _access: u32,
    name: *const c_void,
    handle: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateSharedHandle");
    if handle.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `handle` was just checked non-null.
    unsafe { *handle = ptr::null_mut() };
    if object.is_null() {
        return E_INVALIDARG;
    }
    if !name.is_null() {
        return E_NOTIMPL;
    }
    let mut fence_ptr: *mut c_void = ptr::null_mut();
    // SAFETY: `object` is a caller-supplied COM pointer whose first field is an `IUnknown` vtable.
    let vtbl_ptr = unsafe { *(object as *const *const IUnknownVtbl) };
    // SAFETY: `vtbl_ptr` points to the object's `IUnknownVtbl`.
    let vtbl = unsafe { &*vtbl_ptr };
    // SAFETY: queries `IID_ID3D12_FENCE1` on `object`.
    let hr = unsafe {
        (vtbl.query_interface)(
            object,
            ptr::from_ref(&IID_ID3D12_FENCE1).cast(),
            &raw mut fence_ptr,
        )
    };
    if hr < 0 || fence_ptr.is_null() {
        return E_NOTIMPL;
    }
    // SAFETY: `fence_ptr` is a valid `ID3D12Fence1` COM pointer returned by `QueryInterface`.
    let fence_vtbl_ptr = unsafe { *(fence_ptr as *const *const ID3D12Fence1Vtbl) };
    // SAFETY: `fence_vtbl_ptr` points to the fence's `ID3D12Fence1Vtbl`.
    let fence_vtbl = unsafe { &*fence_vtbl_ptr };
    // SAFETY: `get_creation_flags` is a method on `ID3D12Fence1`.
    let flags = unsafe { (fence_vtbl.get_creation_flags)(fence_ptr) };
    if flags & (D3D12_FENCE_FLAG_SHARED | D3D12_FENCE_FLAG_SHARED_CROSS_ADAPTER) == 0 {
        // SAFETY: drops the reference acquired from `QueryInterface`.
        unsafe { com::release(fence_ptr) };
        return E_INVALIDARG;
    }
    let Some((client_h, ours_h)) = create_and_dup_event() else {
        // SAFETY: drops the reference acquired from `QueryInterface`.
        unsafe { com::release(fence_ptr) };
        return E_OUTOFMEMORY;
    };
    let mut guard = global_table()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    guard.insert(
        client_h.expose_provenance(),
        ours_h.expose_provenance(),
        fence_ptr.expose_provenance(),
        &mut real_ops,
    );
    drop(guard);
    // SAFETY: `handle` is non-null.
    unsafe { *handle = client_h };
    S_OK
}

/// `ID3D12Device::OpenSharedHandle` (`d3d12_device.cpp:1244`).
///
/// # Safety
///
/// `this`, `nt_handle`, `riid`, and `obj` are caller-supplied pointers.
pub unsafe fn open_shared_handle(
    _this: *mut c_void,
    nt_handle: *mut c_void,
    riid: *const c_void,
    obj: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.OpenSharedHandle");
    if !obj.is_null() {
        // SAFETY: `obj` is non-null.
        unsafe { *obj = ptr::null_mut() };
    }
    if nt_handle.is_null() {
        return E_INVALIDARG;
    }
    let fence_raw = {
        let mut guard = global_table()
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        guard.get(nt_handle.expose_provenance(), &mut real_ops)
    };
    let Some(fence_raw) = fence_raw else {
        return E_INVALIDARG;
    };
    let fence_ptr = ptr::with_exposed_provenance_mut::<c_void>(fence_raw);
    // SAFETY: `fence_ptr` is a live `ID3D12Fence1` COM pointer held in the table.
    let vtbl_ptr = unsafe { *(fence_ptr as *const *const IUnknownVtbl) };
    // SAFETY: `vtbl_ptr` points to the fence's `IUnknownVtbl`.
    let vtbl = unsafe { &*vtbl_ptr };
    // SAFETY: forwards `QueryInterface` to the stored fence.
    unsafe { (vtbl.query_interface)(fence_ptr, riid, obj) }
}

/// `ID3D12Device::OpenSharedHandleByName` (`d3d12_device.cpp:1255`).
///
/// # Safety
///
/// `this`, `name`, and `nt_handle` are caller-supplied pointers.
pub unsafe fn open_shared_handle_by_name(
    this: *mut c_void,
    _name: *const c_void,
    _access: u32,
    _nt_handle: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.OpenSharedHandleByName");
    let _ = this.expose_provenance();
    E_NOTIMPL
}

#[cfg(test)]
mod tests;
