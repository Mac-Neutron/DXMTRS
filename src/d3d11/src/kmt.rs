//! `D3DKMT` kernel-mode thunk wrappers and shared-resource layout (`util_d3dkmt.h`).

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use std::{
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use dxmt_util::{com::hresult::DXGI_ERROR_INVALID_CALL, ext::IID_IMTL_DXGI_DEVICE};

use crate::{
    abi::{
        records::{
            D3d11Texture1dDesc, D3d11Texture2dDesc1, D3d11Texture3dDesc1, SecurityAttributes,
        },
        vtbl::IDXGIDevice3Vtbl,
    },
    com::{E_FAIL, E_INVALIDARG, Hresult},
};

#[cfg(test)]
mod tests;

/// Length in bytes (including NUL terminator) of a shared-resource or keyed-mutex Mach port name.
pub const SHARED_NAME_LEN: usize = 54;

/// 54-byte NUL-terminated `"DXMT_shared_resource_%016llx%016llx"` name buffer.
pub type SharedName = [u8; SHARED_NAME_LEN];

/// Union of 1D, 2D and 3D texture descriptors inside [`SharedResourceData`].
#[repr(C)]
pub union SharedResourceDesc {
    pub desc1d: D3d11Texture1dDesc,
    pub desc2d: D3d11Texture2dDesc1,
    pub desc3d: D3d11Texture3dDesc1,
}

/// Private runtime data attached to a shared D3DKMT allocation (`d3d11_texture_device.cpp:346`).
#[repr(C)]
pub struct SharedResourceData {
    pub mach_port_name: SharedName,
    pub dimension: u32,
    pub desc: SharedResourceDesc,
    pub mutex_handle: u32,
}

impl Default for SharedResourceData {
    fn default() -> Self {
        Self {
            mach_port_name: [0; SHARED_NAME_LEN],
            dimension: 0,
            desc: SharedResourceDesc {
                desc2d: D3d11Texture2dDesc1::default(),
            },
            mutex_handle: 0,
        }
    }
}

/// `IMTLDXGIDevice` vtable prefix used to query `GetLocalD3DKMT`.
#[repr(C)]
struct MtlDxgiDeviceVtbl {
    base: IDXGIDevice3Vtbl,
    get_mtl_device: unsafe extern "system-unwind" fn(*mut c_void) -> u64,
    get_local_d3dkmt: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
}

fn device_local_kmt_inner(device: *mut c_void) -> u32 {
    if device.is_null() {
        return 0;
    }
    let mut dxgi_dev: *mut c_void = ptr::null_mut();
    let iid = IID_IMTL_DXGI_DEVICE;
    // SAFETY: `device` is a live COM interface pointer starting with `IUnknownVtbl`.
    let unk = unsafe { crate::com::vtbl::<crate::abi::vtbl::IUnknownVtbl>(device) };
    let qi = unk.query_interface;
    // SAFETY: `QueryInterface` is slot 0 of `IUnknownVtbl`.
    let hr = unsafe { qi(device, (&raw const iid).cast(), &raw mut dxgi_dev) };
    if hr < 0 || dxgi_dev.is_null() {
        return 0;
    }
    // SAFETY: `dxgi_dev` is a live `IMTLDXGIDevice` pointer.
    let dxgi_vtbl = unsafe { crate::com::vtbl::<MtlDxgiDeviceVtbl>(dxgi_dev) };
    let get_kmt = dxgi_vtbl.get_local_d3dkmt;
    let rel = dxgi_vtbl.base.release;
    // SAFETY: `get_local_d3dkmt` is slot 19 on `IMTLDXGIDevice`.
    let kmt = unsafe { get_kmt(dxgi_dev) };
    // SAFETY: balances `QueryInterface` above.
    unsafe { rel(dxgi_dev) };
    kmt
}

/// Queries `IMTLDXGIDevice::GetLocalD3DKMT` on `device`, returning `0` if unavailable.
#[must_use]
pub fn device_local_kmt(device: *mut c_void) -> u32 {
    device_local_kmt_inner(device)
}

static SHARED_NAME_SEQ: AtomicU64 = AtomicU64::new(1);

const fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Generates a unique 54-byte `"DXMT_shared_resource_..."` name (`MakeUniqueSharedName`).
#[must_use]
pub fn make_unique_shared_name() -> SharedName {
    const PREFIX: &[u8; 21] = b"DXMT_shared_resource_";
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let seq = SHARED_NAME_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = u64::from(process::id());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX));
    let stack = (&raw const seq) as usize as u64;
    let r1 = splitmix64(seq ^ pid.rotate_left(17) ^ nanos);
    let r2 = splitmix64(r1 ^ stack.rotate_left(23) ^ seq.wrapping_mul(0x9e37_79b9_7f4a_7c15));

    let mut out = [0u8; SHARED_NAME_LEN];
    let mut idx = 0usize;
    while idx < 21 {
        out[idx] = PREFIX[idx];
        idx += 1;
    }
    let mut i = 0usize;
    while i < 16 {
        let shift = (15 - i) * 4;
        out[21 + i] = HEX[((r1 >> shift) & 0xf) as usize];
        out[37 + i] = HEX[((r2 >> shift) & 0xf) as usize];
        i += 1;
    }
    out[53] = 0;
    out
}

/// Creates a D3DKMT device handle from `adapter_kmt` (`D3DKMTCreateDevice`).
#[must_use]
pub fn create_device(adapter_kmt: u32) -> u32 {
    if adapter_kmt == 0 {
        return 0;
    }
    platform::create_device(adapter_kmt)
}

/// Destroys a D3DKMT device handle (`D3DKMTDestroyDevice`).
pub fn destroy_device(local_kmt: u32) {
    if local_kmt != 0 {
        platform::destroy_device(local_kmt);
    }
}

/// Destroys a D3DKMT resource allocation (`D3DKMTDestroyAllocation`).
pub fn destroy_allocation(kmt_device: u32, local_kmt: u32) {
    if local_kmt != 0 {
        platform::destroy_allocation(kmt_device, local_kmt);
    }
}

/// Creates a shared D3DKMT allocation (`D3DKMTCreateAllocation2`).
#[must_use]
pub fn create_shared_allocation(
    kmt_device: u32,
    runtime_data: &SharedResourceData,
    nt_security_sharing: bool,
) -> Option<(u32, u32)> {
    platform::create_shared_allocation(kmt_device, runtime_data, nt_security_sharing)
}

fn share_object_inner(
    local_kmt: u32,
    attributes: *const SecurityAttributes,
    access: u32,
    name: *const u16,
    out_nt_handle: *mut *mut c_void,
) -> Hresult {
    if out_nt_handle.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `out_nt_handle` was checked non-null.
    unsafe { *out_nt_handle = ptr::null_mut() };
    if local_kmt == 0 {
        return E_INVALIDARG;
    }
    platform::share_object(local_kmt, attributes, access, name, out_nt_handle)
}

/// Creates an NT shared handle for `local_kmt` (`D3DKMTShareObjects`).
pub fn share_object(
    local_kmt: u32,
    attributes: *const SecurityAttributes,
    access: u32,
    name: *const u16,
    out_nt_handle: *mut *mut c_void,
) -> Hresult {
    share_object_inner(local_kmt, attributes, access, name, out_nt_handle)
}

/// Queries and opens a legacy global shared handle (`D3DKMTQueryResourceInfo`).
#[must_use]
pub fn open_shared_from_global_handle(
    kmt_device: u32,
    global_handle: u32,
) -> Option<SharedResourceData> {
    platform::open_shared_from_global_handle(kmt_device, global_handle)
}

/// Queries and opens an NT shared handle (`D3DKMTOpenResourceFromNtHandle`).
#[must_use]
pub fn open_shared_from_nt_handle(
    kmt_device: u32,
    nt_handle: *mut c_void,
) -> Option<SharedResourceData> {
    platform::open_shared_from_nt_handle(kmt_device, nt_handle)
}

/// Opens an NT shared handle by object name (`D3DKMTOpenNtHandleFromName`).
#[must_use]
pub fn open_nt_handle_from_name(name: *const u16, desired_access: u32) -> *mut c_void {
    if name.is_null() {
        return ptr::null_mut();
    }
    platform::open_nt_handle_from_name(name, desired_access)
}

/// Closes a Win32 NT handle (`CloseHandle`).
pub fn close_nt_handle(handle: *mut c_void) {
    if !handle.is_null() {
        platform::close_nt_handle(handle);
    }
}

/// Creates a D3DKMT keyed mutex (`D3DKMTCreateKeyedMutex2`), returning `(global_kmt, local_kmt)`.
#[must_use]
pub fn create_keyed_mutex(mach_port_name: &SharedName) -> Option<(u32, u32)> {
    platform::create_keyed_mutex(mach_port_name)
}

/// Opens a D3DKMT keyed mutex (`D3DKMTOpenKeyedMutex2`), returning `(local_kmt, mach_port_name)`.
#[must_use]
pub fn open_keyed_mutex(global_kmt: u32) -> Option<(u32, SharedName)> {
    platform::open_keyed_mutex(global_kmt)
}

/// Destroys a D3DKMT keyed mutex (`D3DKMTDestroyKeyedMutex`).
pub fn destroy_keyed_mutex(local_kmt: u32) {
    if local_kmt != 0 {
        platform::destroy_keyed_mutex(local_kmt);
    }
}

/// Acquires a D3DKMT keyed mutex (`D3DKMTAcquireKeyedMutex`), returning the fence value.
///
/// # Errors
///
/// Returns `WAIT_TIMEOUT`, `WAIT_ABANDONED` or `DXGI_ERROR_INVALID_CALL` on failure.
pub fn acquire_keyed_mutex(local_kmt: u32, key: u64, milliseconds: u32) -> Result<u64, Hresult> {
    platform::acquire_keyed_mutex(local_kmt, key, milliseconds)
}

/// Releases a D3DKMT keyed mutex (`D3DKMTReleaseKeyedMutex`).
///
/// # Errors
///
/// Returns `E_FAIL` or `DXGI_ERROR_INVALID_CALL` if the release thunk fails.
pub fn release_keyed_mutex(local_kmt: u32, key: u64, fence_value: u64) -> Result<(), Hresult> {
    platform::release_keyed_mutex(local_kmt, key, fence_value)
}

#[cfg(windows)]
mod platform {
    use core::{
        ffi::c_void,
        mem::size_of,
        ptr::{self, from_ref},
    };

    use super::{
        DXGI_ERROR_INVALID_CALL, E_FAIL, Hresult, SHARED_NAME_LEN, SecurityAttributes, SharedName,
        SharedResourceData,
    };
    use crate::com::S_OK;

    const OBJ_CASE_INSENSITIVE: u32 = 0x0000_0040;
    const STATUS_TIMEOUT: i32 = 0x0000_0102;
    const STATUS_ABANDONED_WAIT_0: i32 = 0x0000_0080;
    const WAIT_TIMEOUT_HR: Hresult = 0x0000_0102;
    const WAIT_ABANDONED_HR: Hresult = 0x0000_0080;

    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *mut u16,
    }

    #[repr(C)]
    struct ObjectAttributes {
        length: u32,
        root_directory: *mut c_void,
        object_name: *mut UnicodeString,
        attributes: u32,
        security_descriptor: *mut c_void,
        security_quality_of_service: *mut c_void,
    }

    #[repr(C)]
    struct D3dkmtCreateDevice {
        adapter: usize,
        flags: u32,
        device: u32,
        command_buffer: *mut c_void,
        command_buffer_size: u32,
        allocation_list: *mut c_void,
        allocation_list_size: u32,
        patch_location_list: *mut c_void,
        patch_location_list_size: u32,
    }

    #[repr(C)]
    struct D3dkmtDestroyDevice {
        device: u32,
    }

    #[repr(C)]
    struct D3dkmtDestroySynchronizationObject {
        sync_object: u32,
    }

    #[repr(C)]
    struct D3dkmtDestroyAllocation {
        device: u32,
        resource: u32,
        allocation_list: *const u32,
        allocation_count: u32,
    }

    #[repr(C)]
    struct D3dkmtCreateStandardAllocation {
        r#type: u32,
        existing_heap_size: usize,
        flags: u32,
    }

    #[repr(C)]
    struct D3dddiAllocationInfo {
        allocation: u32,
        sys_mem: *const c_void,
        private_driver_data: *mut c_void,
        private_driver_data_size: u32,
        vid_pn_source_id: u32,
        flags: u32,
    }

    #[repr(C)]
    struct D3dddiAllocationInfo2 {
        allocation: u32,
        sys_mem: *const c_void,
        private_driver_data: *mut c_void,
        private_driver_data_size: u32,
        vid_pn_source_id: u32,
        flags: u32,
        gpu_virtual_address: u64,
        priority: usize,
        reserved: [usize; 5],
    }

    #[repr(C)]
    struct D3dkmtCreateAllocation {
        device: u32,
        resource: u32,
        global_share: u32,
        private_runtime_data: *const c_void,
        private_runtime_data_size: u32,
        private_driver_data: *const c_void,
        private_driver_data_size: u32,
        num_allocations: u32,
        allocation_info2: *mut D3dddiAllocationInfo2,
        flags: u32,
        private_runtime_resource_handle: *mut c_void,
    }

    #[repr(C)]
    struct D3dkmtQueryResourceInfo {
        device: u32,
        global_share: u32,
        private_runtime_data: *mut c_void,
        private_runtime_data_size: u32,
        total_private_driver_data_size: u32,
        resource_private_driver_data_size: u32,
        num_allocations: u32,
    }

    #[repr(C)]
    struct D3dkmtQueryResourceInfoFromNtHandle {
        device: u32,
        nt_handle: *mut c_void,
        private_runtime_data: *mut c_void,
        private_runtime_data_size: u32,
        total_private_driver_data_size: u32,
        resource_private_driver_data_size: u32,
        num_allocations: u32,
    }

    #[repr(C)]
    struct D3dddiOpenAllocationInfo2 {
        allocation: u32,
        private_driver_data: *const c_void,
        private_driver_data_size: u32,
        gpu_virtual_address: u64,
        reserved: [usize; 6],
    }

    impl Default for D3dddiOpenAllocationInfo2 {
        fn default() -> Self {
            Self {
                allocation: 0,
                private_driver_data: ptr::null(),
                private_driver_data_size: 0,
                gpu_virtual_address: 0,
                reserved: [0; 6],
            }
        }
    }

    #[repr(C)]
    struct D3dkmtOpenResource {
        device: u32,
        global_share: u32,
        num_allocations: u32,
        open_allocation_info2: *mut D3dddiOpenAllocationInfo2,
        private_runtime_data: *mut c_void,
        private_runtime_data_size: u32,
        resource_private_driver_data: *mut c_void,
        resource_private_driver_data_size: u32,
        total_private_driver_data_buffer: *mut c_void,
        total_private_driver_data_buffer_size: u32,
        resource: u32,
    }

    #[repr(C)]
    struct D3dkmtOpenResourceFromNtHandle {
        device: u32,
        nt_handle: *mut c_void,
        num_allocations: u32,
        open_allocation_info2: *mut D3dddiOpenAllocationInfo2,
        private_runtime_data_size: u32,
        private_runtime_data: *mut c_void,
        resource_private_driver_data_size: u32,
        resource_private_driver_data: *mut c_void,
        total_private_driver_data_buffer_size: u32,
        total_private_driver_data_buffer: *mut c_void,
        resource: u32,
        keyed_mutex: u32,
        keyed_mutex_private_runtime_data: *mut c_void,
        keyed_mutex_private_runtime_data_size: u32,
        sync_object: u32,
    }

    #[repr(C)]
    struct D3dkmtOpenNtHandleFromName {
        desired_access: u32,
        obj_attrib: *mut ObjectAttributes,
        nt_handle: *mut c_void,
    }

    #[repr(C)]
    struct D3dkmtDestroyKeyedMutex {
        keyed_mutex: u32,
    }

    #[repr(C)]
    struct D3dkmtCreateKeyedMutex2 {
        initial_value: u64,
        shared_handle: u32,
        keyed_mutex: u32,
        private_runtime_data: *mut c_void,
        private_runtime_data_size: u32,
        flags: u32,
    }

    #[repr(C)]
    struct D3dkmtOpenKeyedMutex2 {
        shared_handle: u32,
        keyed_mutex: u32,
        private_runtime_data: *mut c_void,
        private_runtime_data_size: u32,
    }

    #[repr(C)]
    struct D3dkmtAcquireKeyedMutex {
        keyed_mutex: u32,
        key: u64,
        timeout: *mut i64,
        fence_value: u64,
    }

    #[repr(C)]
    struct D3dkmtReleaseKeyedMutex {
        keyed_mutex: u32,
        key: u64,
        fence_value: u64,
    }

    unsafe extern "system" {
        fn D3DKMTCreateDevice(desc: *mut D3dkmtCreateDevice) -> i32;
        fn D3DKMTDestroyDevice(desc: *const D3dkmtDestroyDevice) -> i32;
        fn D3DKMTDestroySynchronizationObject(
            desc: *const D3dkmtDestroySynchronizationObject,
        ) -> i32;
        fn D3DKMTDestroyAllocation(desc: *const D3dkmtDestroyAllocation) -> i32;
        fn D3DKMTCreateAllocation2(desc: *mut D3dkmtCreateAllocation) -> i32;
        fn D3DKMTShareObjects(
            count: u32,
            handles: *const u32,
            attr: *mut ObjectAttributes,
            access: u32,
            handle: *mut *mut c_void,
        ) -> i32;
        fn D3DKMTQueryResourceInfo(desc: *mut D3dkmtQueryResourceInfo) -> i32;
        fn D3DKMTQueryResourceInfoFromNtHandle(
            desc: *mut D3dkmtQueryResourceInfoFromNtHandle,
        ) -> i32;
        fn D3DKMTOpenResource2(desc: *mut D3dkmtOpenResource) -> i32;
        fn D3DKMTOpenResourceFromNtHandle(desc: *mut D3dkmtOpenResourceFromNtHandle) -> i32;
        fn D3DKMTOpenNtHandleFromName(desc: *mut D3dkmtOpenNtHandleFromName) -> i32;
        fn D3DKMTDestroyKeyedMutex(desc: *const D3dkmtDestroyKeyedMutex) -> i32;
        fn D3DKMTCreateKeyedMutex2(desc: *mut D3dkmtCreateKeyedMutex2) -> i32;
        fn D3DKMTOpenKeyedMutex2(desc: *mut D3dkmtOpenKeyedMutex2) -> i32;
        fn D3DKMTAcquireKeyedMutex(desc: *mut D3dkmtAcquireKeyedMutex) -> i32;
        fn D3DKMTReleaseKeyedMutex(desc: *mut D3dkmtReleaseKeyedMutex) -> i32;
        fn GetCurrentProcessId() -> u32;
        fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    fn format_session_object_name(name: *const u16, buf: &mut [u16; 260]) -> Option<usize> {
        let mut session = 0u32;
        // SAFETY: `GetCurrentProcessId` and `ProcessIdToSessionId` are Win32 process queries.
        let pid = unsafe { GetCurrentProcessId() };
        // SAFETY: `session` is a valid out pointer.
        unsafe { ProcessIdToSessionId(pid, &raw mut session) };
        let prefix = format!("\\Sessions\\{session}\\BaseNamedObjects\\");
        let mut len = 0usize;
        for u in prefix.encode_utf16() {
            if len >= buf.len() {
                return None;
            }
            buf[len] = u;
            len += 1;
        }
        let mut i = 0usize;
        loop {
            // SAFETY: caller passes a NUL-terminated UTF-16 `LPCWSTR`.
            let ch_ptr = unsafe { name.add(i) };
            // SAFETY: `ch_ptr` addresses a valid `u16` code unit in `name`.
            let ch = unsafe { *ch_ptr };
            if len >= buf.len() {
                return None;
            }
            buf[len] = ch;
            if ch == 0 {
                break;
            }
            len += 1;
            i += 1;
        }
        Some(len)
    }

    pub fn create_device(adapter_kmt: u32) -> u32 {
        let mut create = D3dkmtCreateDevice {
            adapter: adapter_kmt as usize,
            flags: 0,
            device: 0,
            command_buffer: ptr::null_mut(),
            command_buffer_size: 0,
            allocation_list: ptr::null_mut(),
            allocation_list_size: 0,
            patch_location_list: ptr::null_mut(),
            patch_location_list_size: 0,
        };
        // SAFETY: `create` is a valid local `D3DKMT_CREATEDEVICE`.
        if unsafe { D3DKMTCreateDevice(&raw mut create) } != 0 {
            return 0;
        }
        create.device
    }

    pub fn destroy_device(local_kmt: u32) {
        let destroy = D3dkmtDestroyDevice { device: local_kmt };
        // SAFETY: `destroy` is a valid local `D3DKMT_DESTROYDEVICE`.
        unsafe { D3DKMTDestroyDevice(&raw const destroy) };
    }

    pub fn destroy_allocation(kmt_device: u32, local_kmt: u32) {
        let desc = D3dkmtDestroyAllocation {
            device: kmt_device,
            resource: local_kmt,
            allocation_list: ptr::null(),
            allocation_count: 0,
        };
        // SAFETY: `desc` is a valid `D3DKMT_DESTROYALLOCATION`.
        unsafe { D3DKMTDestroyAllocation(&raw const desc) };
    }

    pub fn create_shared_allocation(
        kmt_device: u32,
        runtime_data: &SharedResourceData,
        nt_security_sharing: bool,
    ) -> Option<(u32, u32)> {
        if (kmt_device & 0xc000_0000) == 0 {
            return None;
        }
        let system_mem = D3dddiAllocationInfo {
            allocation: 0,
            sys_mem: ptr::null(),
            private_driver_data: ptr::null_mut(),
            private_driver_data_size: 0,
            vid_pn_source_id: 0,
            flags: 0,
        };
        let mut alloc_info = D3dddiAllocationInfo2 {
            allocation: 0,
            sys_mem: (&raw const system_mem).cast(),
            private_driver_data: ptr::null_mut(),
            private_driver_data_size: 0,
            vid_pn_source_id: 0,
            flags: 0,
            gpu_virtual_address: 0,
            priority: 0,
            reserved: [0; 5],
        };
        let standard_alloc = D3dkmtCreateStandardAllocation {
            r#type: 1, // D3DKMT_STANDARDALLOCATIONTYPE_EXISTINGHEAP
            existing_heap_size: 0,
            flags: 0,
        };
        // CreateResource (bit 0) | CreateShared (bit 1) | ExistingSysMem (bit 5)
        // | NtSecuritySharing (bit 6) | StandardAllocation (bit 16).
        let flags = 0b11u32 | (1u32 << 5) | (u32::from(nt_security_sharing) << 6) | (1u32 << 16);
        let mut create = D3dkmtCreateAllocation {
            device: kmt_device,
            resource: 0,
            global_share: 0,
            private_runtime_data: from_ref(runtime_data).cast(),
            private_runtime_data_size: u32::try_from(size_of::<SharedResourceData>()).unwrap_or(0),
            private_driver_data: (&raw const standard_alloc).cast(),
            private_driver_data_size: 0,
            num_allocations: 1,
            allocation_info2: &raw mut alloc_info,
            flags,
            private_runtime_resource_handle: ptr::null_mut(),
        };
        // SAFETY: `create`, `alloc_info`, `standard_alloc`, and `system_mem` are valid local structs.
        let status = unsafe { D3DKMTCreateAllocation2(&raw mut create) };
        (status == 0).then_some((create.resource, create.global_share))
    }

    pub fn share_object(
        local_kmt: u32,
        attributes: *const SecurityAttributes,
        access: u32,
        name: *const u16,
        out_nt_handle: *mut *mut c_void,
    ) -> Hresult {
        let mut name_buf = [0u16; 260];
        let mut unicode = UnicodeString {
            length: 0,
            maximum_length: 0,
            buffer: ptr::null_mut(),
        };
        let mut attr = ObjectAttributes {
            length: u32::try_from(size_of::<ObjectAttributes>()).unwrap_or(0),
            root_directory: ptr::null_mut(),
            object_name: ptr::null_mut(),
            attributes: 0,
            security_descriptor: attributes.cast_mut().cast(),
            security_quality_of_service: ptr::null_mut(),
        };
        if !name.is_null() {
            let Some(char_len) = format_session_object_name(name, &mut name_buf) else {
                return E_FAIL;
            };
            let byte_len = u16::try_from(char_len * size_of::<u16>()).unwrap_or(0);
            unicode.length = byte_len;
            unicode.maximum_length = byte_len.wrapping_add(2);
            unicode.buffer = name_buf.as_mut_ptr();
            attr.object_name = &raw mut unicode;
            attr.attributes = OBJ_CASE_INSENSITIVE;
        }
        // SAFETY: `local_kmt`, `attr`, and `out_nt_handle` are valid pointers.
        let status = unsafe {
            D3DKMTShareObjects(
                1,
                &raw const local_kmt,
                &raw mut attr,
                access,
                out_nt_handle,
            )
        };
        if status == 0 { S_OK } else { E_FAIL }
    }

    pub fn open_shared_from_global_handle(
        kmt_device: u32,
        global_handle: u32,
    ) -> Option<SharedResourceData> {
        let expected_size = u32::try_from(size_of::<SharedResourceData>()).unwrap_or(0);
        let mut runtime_data = SharedResourceData::default();
        let mut query = D3dkmtQueryResourceInfo {
            device: kmt_device,
            global_share: global_handle,
            private_runtime_data: (&raw mut runtime_data).cast(),
            private_runtime_data_size: expected_size,
            total_private_driver_data_size: 0,
            resource_private_driver_data_size: 0,
            num_allocations: 0,
        };
        // SAFETY: `query` is a valid local `D3DKMT_QUERYRESOURCEINFO`.
        if unsafe { D3DKMTQueryResourceInfo(&raw mut query) } != 0 {
            return None;
        }
        if query.private_runtime_data_size != expected_size {
            return None;
        }
        let mut alloc = D3dddiOpenAllocationInfo2::default();
        let mut open = D3dkmtOpenResource {
            device: kmt_device,
            global_share: global_handle,
            num_allocations: 1,
            open_allocation_info2: &raw mut alloc,
            private_runtime_data: (&raw mut runtime_data).cast(),
            private_runtime_data_size: query.private_runtime_data_size,
            resource_private_driver_data: ptr::null_mut(),
            resource_private_driver_data_size: 0,
            total_private_driver_data_buffer: ptr::null_mut(),
            total_private_driver_data_buffer_size: 0,
            resource: 0,
        };
        // SAFETY: `open` and its buffers are valid local allocations.
        let status = unsafe { D3DKMTOpenResource2(&raw mut open) };
        if status != 0 {
            return None;
        }
        destroy_allocation(kmt_device, open.resource);
        Some(runtime_data)
    }

    pub fn open_shared_from_nt_handle(
        kmt_device: u32,
        nt_handle: *mut c_void,
    ) -> Option<SharedResourceData> {
        let expected_size = u32::try_from(size_of::<SharedResourceData>()).unwrap_or(0);
        let mut runtime_data = SharedResourceData::default();
        let mut query = D3dkmtQueryResourceInfoFromNtHandle {
            device: kmt_device,
            nt_handle,
            private_runtime_data: (&raw mut runtime_data).cast(),
            private_runtime_data_size: expected_size,
            total_private_driver_data_size: 0,
            resource_private_driver_data_size: 0,
            num_allocations: 0,
        };
        // SAFETY: `query` is a valid local `D3DKMT_QUERYRESOURCEINFOFROMNTHANDLE`.
        if unsafe { D3DKMTQueryResourceInfoFromNtHandle(&raw mut query) } != 0 {
            return None;
        }
        if query.private_runtime_data_size != expected_size {
            return None;
        }
        let mut alloc = D3dddiOpenAllocationInfo2::default();
        let mut dummy = 0u8;
        let mut open = D3dkmtOpenResourceFromNtHandle {
            device: kmt_device,
            nt_handle,
            num_allocations: 1,
            open_allocation_info2: &raw mut alloc,
            private_runtime_data_size: query.private_runtime_data_size,
            private_runtime_data: (&raw mut runtime_data).cast(),
            resource_private_driver_data_size: 0,
            resource_private_driver_data: ptr::null_mut(),
            total_private_driver_data_buffer_size: 0,
            total_private_driver_data_buffer: (&raw mut dummy).cast(),
            resource: 0,
            keyed_mutex: 0,
            keyed_mutex_private_runtime_data: ptr::null_mut(),
            keyed_mutex_private_runtime_data_size: 0,
            sync_object: 0,
        };
        // SAFETY: `open` and its buffers are valid local allocations.
        let status = unsafe { D3DKMTOpenResourceFromNtHandle(&raw mut open) };
        if status != 0 {
            return None;
        }
        destroy_allocation(kmt_device, open.resource);
        if open.sync_object != 0 {
            let destroy_sync = D3dkmtDestroySynchronizationObject {
                sync_object: open.sync_object,
            };
            // SAFETY: `destroy_sync` is a valid local `D3DKMT_DESTROYSYNCHRONIZATIONOBJECT`.
            unsafe { D3DKMTDestroySynchronizationObject(&raw const destroy_sync) };
        }
        if open.keyed_mutex != 0 {
            destroy_keyed_mutex(open.keyed_mutex);
        }
        Some(runtime_data)
    }

    pub fn open_nt_handle_from_name(name: *const u16, desired_access: u32) -> *mut c_void {
        let mut name_buf = [0u16; 260];
        let Some(char_len) = format_session_object_name(name, &mut name_buf) else {
            return ptr::null_mut();
        };
        let byte_len = u16::try_from(char_len * size_of::<u16>()).unwrap_or(0);
        let mut unicode = UnicodeString {
            length: byte_len,
            maximum_length: byte_len.wrapping_add(2),
            buffer: name_buf.as_mut_ptr(),
        };
        let mut attr = ObjectAttributes {
            length: u32::try_from(size_of::<ObjectAttributes>()).unwrap_or(0),
            root_directory: ptr::null_mut(),
            object_name: &raw mut unicode,
            attributes: OBJ_CASE_INSENSITIVE,
            security_descriptor: ptr::null_mut(),
            security_quality_of_service: ptr::null_mut(),
        };
        let mut open = D3dkmtOpenNtHandleFromName {
            desired_access,
            obj_attrib: &raw mut attr,
            nt_handle: ptr::null_mut(),
        };
        // SAFETY: `open`, `attr`, and `unicode` are valid local structs.
        if unsafe { D3DKMTOpenNtHandleFromName(&raw mut open) } != 0 {
            return ptr::null_mut();
        }
        open.nt_handle
    }

    pub fn close_nt_handle(handle: *mut c_void) {
        // SAFETY: `handle` is a Win32 NT handle returned by `D3DKMTOpenNtHandleFromName`.
        unsafe { CloseHandle(handle) };
    }

    pub fn create_keyed_mutex(mach_port_name: &SharedName) -> Option<(u32, u32)> {
        let mut name_copy = *mach_port_name;
        let mut create = D3dkmtCreateKeyedMutex2 {
            initial_value: 0,
            shared_handle: 0,
            keyed_mutex: 0,
            private_runtime_data: name_copy.as_mut_ptr().cast(),
            private_runtime_data_size: u32::try_from(SHARED_NAME_LEN).unwrap_or(0),
            flags: 0,
        };
        // SAFETY: `create` and `name_copy` are valid local buffers.
        if unsafe { D3DKMTCreateKeyedMutex2(&raw mut create) } != 0 {
            return None;
        }
        Some((create.shared_handle, create.keyed_mutex))
    }

    pub fn open_keyed_mutex(global_kmt: u32) -> Option<(u32, SharedName)> {
        let mut name = [0u8; SHARED_NAME_LEN];
        let mut open = D3dkmtOpenKeyedMutex2 {
            shared_handle: global_kmt,
            keyed_mutex: 0,
            private_runtime_data: name.as_mut_ptr().cast(),
            private_runtime_data_size: u32::try_from(SHARED_NAME_LEN).unwrap_or(0),
        };
        // SAFETY: `open` and `name` are valid local buffers.
        if unsafe { D3DKMTOpenKeyedMutex2(&raw mut open) } != 0 {
            return None;
        }
        Some((open.keyed_mutex, name))
    }

    pub fn destroy_keyed_mutex(local_kmt: u32) {
        let destroy = D3dkmtDestroyKeyedMutex {
            keyed_mutex: local_kmt,
        };
        // SAFETY: `destroy` is a valid local `D3DKMT_DESTROYKEYEDMUTEX`.
        unsafe { D3DKMTDestroyKeyedMutex(&raw const destroy) };
    }

    pub fn acquire_keyed_mutex(
        local_kmt: u32,
        key: u64,
        milliseconds: u32,
    ) -> Result<u64, Hresult> {
        let _ = DXGI_ERROR_INVALID_CALL;
        let mut timeout = i64::from(milliseconds).wrapping_mul(-10_000);
        let mut acquire = D3dkmtAcquireKeyedMutex {
            keyed_mutex: local_kmt,
            key,
            timeout: &raw mut timeout,
            fence_value: 0,
        };
        // SAFETY: `acquire` and `timeout` are valid local structs.
        let status = unsafe { D3DKMTAcquireKeyedMutex(&raw mut acquire) };
        if status == STATUS_TIMEOUT {
            return Err(WAIT_TIMEOUT_HR);
        }
        if status == STATUS_ABANDONED_WAIT_0 {
            return Err(WAIT_ABANDONED_HR);
        }
        if status != 0 {
            return Err(E_FAIL);
        }
        Ok(acquire.fence_value)
    }

    pub fn release_keyed_mutex(local_kmt: u32, key: u64, fence_value: u64) -> Result<(), Hresult> {
        let mut release = D3dkmtReleaseKeyedMutex {
            keyed_mutex: local_kmt,
            key,
            fence_value,
        };
        // SAFETY: `release` is a valid local `D3DKMT_RELEASEKEYEDMUTEX`.
        let status = unsafe { D3DKMTReleaseKeyedMutex(&raw mut release) };
        if status == 0 { Ok(()) } else { Err(E_FAIL) }
    }
}

#[cfg(not(windows))]
mod platform {
    use core::{ffi::c_void, sync::atomic::Ordering};

    use super::{
        DXGI_ERROR_INVALID_CALL, E_FAIL, Hresult, SHARED_NAME_SEQ, SecurityAttributes, SharedName,
        SharedResourceData,
    };

    fn touch() {
        let _ = SHARED_NAME_SEQ.load(Ordering::Relaxed);
    }

    pub fn create_device(adapter_kmt: u32) -> u32 {
        touch();
        let _ = adapter_kmt;
        0
    }

    pub fn destroy_device(local_kmt: u32) {
        touch();
        let _ = local_kmt;
    }

    pub fn destroy_allocation(kmt_device: u32, local_kmt: u32) {
        touch();
        let _ = (kmt_device, local_kmt);
    }

    pub fn create_shared_allocation(
        kmt_device: u32,
        runtime_data: &SharedResourceData,
        nt_security_sharing: bool,
    ) -> Option<(u32, u32)> {
        touch();
        let _ = (kmt_device, runtime_data, nt_security_sharing);
        None
    }

    pub fn share_object(
        local_kmt: u32,
        attributes: *const SecurityAttributes,
        access: u32,
        name: *const u16,
        out_nt_handle: *mut *mut c_void,
    ) -> Hresult {
        touch();
        let _ = (local_kmt, attributes, access, name, out_nt_handle);
        E_FAIL
    }

    pub fn open_shared_from_global_handle(
        kmt_device: u32,
        global_handle: u32,
    ) -> Option<SharedResourceData> {
        touch();
        let _ = (kmt_device, global_handle);
        None
    }

    pub fn open_shared_from_nt_handle(
        kmt_device: u32,
        nt_handle: *mut c_void,
    ) -> Option<SharedResourceData> {
        touch();
        let _ = (kmt_device, nt_handle);
        None
    }

    pub fn open_nt_handle_from_name(name: *const u16, desired_access: u32) -> *mut c_void {
        touch();
        let _ = (name, desired_access);
        core::ptr::null_mut()
    }

    pub fn close_nt_handle(handle: *mut c_void) {
        touch();
        let _ = handle;
    }

    pub fn create_keyed_mutex(mach_port_name: &SharedName) -> Option<(u32, u32)> {
        touch();
        let _ = mach_port_name;
        None
    }

    pub fn open_keyed_mutex(global_kmt: u32) -> Option<(u32, SharedName)> {
        touch();
        let _ = global_kmt;
        None
    }

    pub fn destroy_keyed_mutex(local_kmt: u32) {
        touch();
        let _ = local_kmt;
    }

    pub fn acquire_keyed_mutex(
        local_kmt: u32,
        key: u64,
        milliseconds: u32,
    ) -> Result<u64, Hresult> {
        touch();
        let _ = (local_kmt, key, milliseconds);
        Err(DXGI_ERROR_INVALID_CALL)
    }

    pub fn release_keyed_mutex(local_kmt: u32, key: u64, fence_value: u64) -> Result<(), Hresult> {
        touch();
        let _ = (local_kmt, key, fence_value);
        Err(DXGI_ERROR_INVALID_CALL)
    }
}
