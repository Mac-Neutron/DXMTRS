//! The adapter, `IMTLDXGIAdapter`: one Metal device as DXGI describes it (`dxgi_adapter.cpp`).

use core::{
    ffi::c_void,
    mem::offset_of,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};

use dxmt_util::{
    com::{IUnknownVtbl, release as release_unknown, vtbl},
    luid_from_registry_id,
};
use winemetal_wire::{WMTUTF16_STRING_ENCODING, WMTUTF32_LITTLE_ENDIAN_STRING_ENCODING};

use crate::{
    LOG,
    abi::{
        AdapterDesc, AdapterDesc1, AdapterDesc2, AdapterDesc3, AdapterVtbl,
        DXGI_ADAPTER_FLAG3_NONE, DXGI_COMPUTE_PREEMPTION_DMA_BUFFER_BOUNDARY, DXGI_ERROR_NOT_FOUND,
        DXGI_ERROR_UNSUPPORTED, DXGI_GRAPHICS_PREEMPTION_DMA_BUFFER_BOUNDARY,
        DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL, E_INVALIDARG,
        E_NOTIMPL, Guid, Hresult, IID_ID3D10_DEVICE, IID_ID3D10_DEVICE1, IID_IDXGI_ADAPTER,
        IID_IDXGI_ADAPTER1, IID_IDXGI_ADAPTER2, IID_IDXGI_ADAPTER3, IID_IDXGI_ADAPTER4,
        IID_IDXGI_DEVICE, IID_IDXGI_OBJECT, IID_IMTL_DXGI_ADAPTER, IID_IUNKNOWN,
        IID_WINE_DXGI_ADAPTER, Luid, ObjectVtbl, QueryVideoMemoryInfo, S_OK, Wchar, transcode_into,
    },
    com::{self, Head},
    config::{CONFIG, Options},
    ffi, kmt, nvext, output,
    private_data::PrivateData,
    wsi,
};

/// A new adapter with one reference, on the Metal `device`.
///
/// It takes a reference to `device` and a public one to `factory`, reads the configuration's
/// `dxgi.*` options (the first adapter reads the configuration itself) and opens the device's
/// D3DKMT adapter, as the C++ constructor does, in that order.
///
/// # Safety
///
/// `device` is a live Metal device; `factory` is a live factory of this crate.
#[must_use]
pub unsafe fn create(device: u64, factory: *mut c_void) -> *mut c_void {
    let options = Options::from_config(&CONFIG);
    // SAFETY: the caller's: a live device; the adapter gives this reference back when it dies.
    unsafe { ffi::NSObject_retain(device) };
    // SAFETY: the caller's: a live factory, which starts with `IUnknown`'s slots.
    let slots = unsafe { vtbl::<IUnknownVtbl>(factory) };
    // SAFETY: as above; the adapter gives this reference back when it dies.
    unsafe { (slots.add_ref)(factory) };
    // SAFETY: the caller's: a live device.
    let registry_id = unsafe { ffi::MTLDevice_registryID(device) };
    let adapter = Adapter {
        head: Head::new(&ADAPTER_VTBL),
        private: PrivateData::default(),
        device,
        factory,
        kmt: kmt::open(luid(registry_id)),
        options,
        reserved: [AtomicU64::new(0), AtomicU64::new(0)],
    };
    Box::into_raw(Box::new(adapter)).cast()
}

/// Apple's PCI vendor ID, which a description reports.
const APPLE_VENDOR_ID: u32 = 0x106b;
/// NVIDIA's PCI vendor ID, which a description reports with DXMT's NVIDIA extension on.
const NVIDIA_VENDOR_ID: u32 = 0x10de;
/// The most video memory a 32-bit `SIZE_T` reports: 3 GiB, rather than the low 32 bits of more.
const VIDEO_MEMORY_CAP_32: u64 = 0xc000_0000;
/// `CheckInterfaceSupport`'s UMD version, all bits set: Metal has no Windows driver version.
const UMD_VERSION: i64 = -1;
/// The encoding a device name is asked in: the `WCHAR`s', UTF-32 natively, UTF-16 on PE.
const NAME_ENCODING: u64 = if size_of::<Wchar>() == 4 {
    WMTUTF32_LITTLE_ENDIAN_STRING_ENCODING
} else {
    WMTUTF16_STRING_ENCODING
};

/// The IIDs the adapter answers with itself.
const ANSWERED: [Guid; 8] = [
    IID_IUNKNOWN,
    IID_IDXGI_OBJECT,
    IID_IDXGI_ADAPTER,
    IID_IDXGI_ADAPTER1,
    IID_IDXGI_ADAPTER2,
    IID_IDXGI_ADAPTER3,
    IID_IDXGI_ADAPTER4,
    IID_IMTL_DXGI_ADAPTER,
];

/// The bytes `GetDesc` writes: `DXGI_ADAPTER_DESC3`'s up to the end of `AdapterLuid`.
///
/// The four descriptions share their fields' layout, so each older one is the bytes of
/// `DXGI_ADAPTER_DESC3` up to the end of its last field; its tail padding is left untouched, as the
/// C++'s field by field copy leaves it.
const DESC_LEN: usize = offset_of!(AdapterDesc, adapter_luid) + size_of::<Luid>();
/// The bytes `GetDesc1` writes: up to the end of `Flags`.
const DESC1_LEN: usize = offset_of!(AdapterDesc1, flags) + size_of::<u32>();
/// The bytes `GetDesc2` writes: up to the end of `ComputePreemptionGranularity`.
const DESC2_LEN: usize =
    offset_of!(AdapterDesc2, compute_preemption_granularity) + size_of::<u32>();
/// The bytes `GetDesc3` writes: as `GetDesc2`.
const DESC3_LEN: usize =
    offset_of!(AdapterDesc3, compute_preemption_granularity) + size_of::<u32>();

const _: () = assert!(
    offset_of!(AdapterDesc, adapter_luid) == offset_of!(AdapterDesc3, adapter_luid)
        && offset_of!(AdapterDesc1, adapter_luid) == offset_of!(AdapterDesc3, adapter_luid)
        && offset_of!(AdapterDesc2, adapter_luid) == offset_of!(AdapterDesc3, adapter_luid)
        && offset_of!(AdapterDesc1, flags) == offset_of!(AdapterDesc3, flags)
        && offset_of!(AdapterDesc2, flags) == offset_of!(AdapterDesc3, flags)
        && DESC2_LEN == DESC3_LEN
        && DESC3_LEN <= size_of::<AdapterDesc3>()
);

/// `IMTLDXGIAdapter`'s slots.
static ADAPTER_VTBL: AdapterVtbl = AdapterVtbl {
    object: ObjectVtbl {
        unknown: IUnknownVtbl {
            query_interface: adapter_query_interface,
            add_ref: adapter_add_ref,
            release: adapter_release,
        },
        set_private_data: adapter_set_private_data,
        set_private_data_interface: adapter_set_private_data_interface,
        get_private_data: adapter_get_private_data,
        get_parent: adapter_get_parent,
    },
    enum_outputs: adapter_enum_outputs,
    get_desc: adapter_get_desc,
    check_interface_support: adapter_check_interface_support,
    get_desc1: adapter_get_desc1,
    get_desc2: adapter_get_desc2,
    register_hardware_content_protection_teardown_status_event:
        adapter_register_hardware_content_protection_teardown_status_event,
    unregister_hardware_content_protection_teardown_status:
        adapter_unregister_hardware_content_protection_teardown_status,
    query_video_memory_info: adapter_query_video_memory_info,
    set_video_memory_reservation: adapter_set_video_memory_reservation,
    register_video_memory_budget_change_notification_event:
        adapter_register_video_memory_budget_change_notification_event,
    unregister_video_memory_budget_change_notification:
        adapter_unregister_video_memory_budget_change_notification,
    get_desc3: adapter_get_desc3,
    get_mtl_device: adapter_get_mtl_device,
    get_local_d3dkmt: adapter_get_local_d3dkmt,
};

/// An adapter: its head, then what the C++ class holds.
#[repr(C)]
struct Adapter {
    head: Head<AdapterVtbl>,
    private: PrivateData,
    /// The Metal device, retained.
    device: u64,
    /// The factory that made the adapter, a public reference.
    factory: *mut c_void,
    /// The D3DKMT adapter of the device's LUID, or 0.
    kmt: u32,
    options: Options,
    /// `SetVideoMemoryReservation`'s values, local then non-local.
    reserved: [AtomicU64; 2],
}

impl Adapter {
    /// What a description reads of the device, in the C++'s order.
    ///
    /// The name is read only when the description is not custom.
    fn facts(&self) -> DeviceFacts {
        let device = self.device;
        DeviceFacts {
            name: if self.options.device_desc.is_none() {
                device_name(device)
            } else {
                None
            },
            // SAFETY: the adapter's retained device.
            recommended_max: unsafe { ffi::MTLDevice_recommendedMaxWorkingSetSize(device) },
            // SAFETY: as above.
            unified: unsafe { ffi::MTLDevice_hasUnifiedMemory(device) },
            // SAFETY: as above.
            registry_id: unsafe { ffi::MTLDevice_registryID(device) },
        }
    }

    /// Writes the description's first `len` bytes to `out`.
    ///
    /// `Flags` is masked to `0b11` when `mask_flags` (`GetDesc1`, `GetDesc2`); a null `out` is
    /// `E_INVALIDARG`.
    ///
    /// # Safety
    ///
    /// `out` is null or addresses `len` writable bytes, with no alignment promised; `len` is at
    /// most `DESC3_LEN`.
    unsafe fn describe(&self, out: *mut u8, len: usize, mask_flags: bool) -> Hresult {
        if out.is_null() {
            return E_INVALIDARG;
        }
        let mut desc = desc3(&self.facts(), &self.options, nvext::enabled());
        if mask_flags {
            desc.flags &= 0b11;
        }
        // SAFETY: the caller's: `out` has room for `len` bytes, which a byte copy writes with no
        // alignment; `desc` holds at least `len` bytes, every field initialised.
        unsafe { ptr::copy_nonoverlapping(ptr::from_ref(&desc).cast::<u8>(), out, len) };
        S_OK
    }

    /// The reservation of segment group `group`: none for a group DXGI lacks.
    const fn reservation(&self, group: u32) -> Option<&AtomicU64> {
        let [local, non_local] = &self.reserved;
        match group {
            DXGI_MEMORY_SEGMENT_GROUP_LOCAL => Some(local),
            DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL => Some(non_local),
            _ => None,
        }
    }
}

impl Drop for Adapter {
    /// The C++ destructor's order: the D3DKMT adapter, the factory, the device.
    ///
    /// The private data goes after them, as the C++ base class's.
    fn drop(&mut self) {
        kmt::close(self.kmt);
        // SAFETY: the factory reference `create` took.
        unsafe { release_unknown(self.factory) };
        // SAFETY: the device reference `create` took.
        unsafe { ffi::NSObject_release(self.device) };
    }
}

/// What a description reads of the Metal device.
struct DeviceFacts {
    /// The device's name in `WCHAR`s, or none when it does not fit or was not asked for.
    name: Option<[Wchar; 128]>,
    /// `recommendedMaxWorkingSetSize`.
    recommended_max: u64,
    /// `hasUnifiedMemory`.
    unified: bool,
    /// `registryID`.
    registry_id: u64,
}

/// `GetDesc3`'s description of a device with `facts`, under `options`.
///
/// `nvext` is DXMT's NVIDIA extension, on or off. The description is the custom one, its first
/// 127 units, or the device's name; the vendor the custom one, else NVIDIA's with the extension,
/// else Apple's; the device the custom one or 0.
fn desc3(facts: &DeviceFacts, options: &Options, nvext: bool) -> AdapterDesc3 {
    let mut description: [Wchar; 128] = [0; 128];
    if let Some(custom) = &options.device_desc {
        let (text, _) = description.split_at_mut(127);
        transcode_into(custom, text);
    } else if let Some(name) = facts.name {
        description = name;
    }
    let vendor = if nvext {
        NVIDIA_VENDOR_ID
    } else {
        APPLE_VENDOR_ID
    };
    let video = video_memory(facts.recommended_max, facts.unified, usize::BITS);
    AdapterDesc3 {
        description,
        vendor_id: options.vendor_id.map_or(vendor, u32::from),
        device_id: options.device_id.map_or(0, u32::from),
        sub_sys_id: 0,
        revision: 0,
        dedicated_video_memory: usize::try_from(video).unwrap_or(usize::MAX),
        dedicated_system_memory: 0,
        shared_system_memory: 0,
        adapter_luid: luid(facts.registry_id),
        flags: DXGI_ADAPTER_FLAG3_NONE,
        graphics_preemption_granularity: DXGI_GRAPHICS_PREEMPTION_DMA_BUFFER_BOUNDARY,
        compute_preemption_granularity: DXGI_COMPUTE_PREEMPTION_DMA_BUFFER_BOUNDARY,
    }
}

/// The video memory a description reports for a `SIZE_T` of `size_t_bits`.
///
/// The working set, halved with unified memory, then at most 3 GiB in a 32-bit build.
const fn video_memory(recommended_max: u64, unified: bool, size_t_bits: u32) -> u64 {
    let video = if unified {
        recommended_max / 2
    } else {
        recommended_max
    };
    if size_t_bits < 64 && video > VIDEO_MEMORY_CAP_32 {
        VIDEO_MEMORY_CAP_32
    } else {
        video
    }
}

/// The LUID of the device with `registry_id`.
const fn luid(registry_id: u64) -> Luid {
    let [l0, l1, l2, l3, h0, h1, h2, h3] = luid_from_registry_id(registry_id);
    Luid {
        low_part: u32::from_ne_bytes([l0, l1, l2, l3]),
        high_part: i32::from_ne_bytes([h0, h1, h2, h3]),
    }
}

/// `device`'s name in `WCHAR`s, or none when it and its NUL do not fit 128 units.
fn device_name(device: u64) -> Option<[Wchar; 128]> {
    let mut name: [Wchar; 128] = [0; 128];
    // SAFETY: a live device; its name comes back +0 and lives as long as the device.
    let string = unsafe { ffi::MTLDevice_name(device) };
    // SAFETY: `name` has room for the bytes passed; winemetal writes no more.
    let fits = unsafe {
        ffi::NSString_getCString(
            string,
            name.as_mut_ptr().cast(),
            size_of::<[Wchar; 128]>() as u64,
            NAME_ENCODING,
        )
    };
    (fits != 0).then_some(name)
}

/// The adapter behind a slot's `this`.
///
/// # Safety
///
/// `this` is a live adapter, the object the slot's vtable was found on.
const unsafe fn adapter_of<'a>(this: *mut c_void) -> &'a Adapter {
    // SAFETY: the caller's.
    unsafe { &*this.cast::<Adapter>() }
}

extern "system" fn adapter_query_interface(
    this: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: a live adapter, whose `AddRef` is `adapter_add_ref`; `riid` and `out` are the
    // application's, each null or valid, with no alignment promised.
    unsafe {
        com::query_interface(
            this,
            riid,
            out,
            &ANSWERED,
            &[IID_WINE_DXGI_ADAPTER],
            (&IID_IDXGI_ADAPTER, "DXGIAdapter"),
            adapter_add_ref,
        )
    }
}

extern "system" fn adapter_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: a live adapter, from `Box::into_raw`.
    unsafe { com::add_ref::<Adapter>(this) }
}

extern "system" fn adapter_release(this: *mut c_void) -> u32 {
    // SAFETY: a live adapter, from `Box::into_raw`.
    unsafe { com::release::<Adapter>(this) }
}

extern "system" fn adapter_set_private_data(
    this: *mut c_void,
    guid: *const Guid,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: the application's GUID, null or readable.
    let Some(guid) = (unsafe { Guid::read(guid) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: a live adapter.
    let adapter = unsafe { adapter_of(this) };
    // SAFETY: the application's `data`, null or `size` readable bytes.
    unsafe { adapter.private.set_data(guid, size, data) }
}

extern "system" fn adapter_set_private_data_interface(
    this: *mut c_void,
    guid: *const Guid,
    unknown: *mut c_void,
) -> Hresult {
    // SAFETY: the application's GUID, null or readable.
    let Some(guid) = (unsafe { Guid::read(guid) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: a live adapter.
    let adapter = unsafe { adapter_of(this) };
    // SAFETY: the application's interface, null or a live COM object.
    unsafe { adapter.private.set_interface(guid, unknown) }
}

extern "system" fn adapter_get_private_data(
    this: *mut c_void,
    guid: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: the application's GUID, null or readable.
    let Some(guid) = (unsafe { Guid::read(guid) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: a live adapter.
    let adapter = unsafe { adapter_of(this) };
    // SAFETY: the application's `size` and `data`, each null or valid for the call, with no
    // alignment promised.
    unsafe { adapter.private.get_data(guid, size, data) }
}

/// `GetParent`: the factory's `QueryInterface`, which answers a null `riid` itself.
extern "system" fn adapter_get_parent(
    this: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: a live adapter.
    let factory = unsafe { adapter_of(this) }.factory;
    // SAFETY: the adapter's factory is live while it holds its reference.
    let slots = unsafe { vtbl::<IUnknownVtbl>(factory) };
    // SAFETY: as above; `riid` and `out` go to it as the application passed them.
    unsafe { (slots.query_interface)(factory, riid, out) }
}

extern "system" fn adapter_enum_outputs(
    this: *mut c_void,
    output: u32,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: the application's out slot, non-null; the write assumes no alignment.
    unsafe { com::write_out(out, ptr::null_mut()) };
    let Some(monitor) = wsi::enum_monitors(output) else {
        return DXGI_ERROR_NOT_FOUND;
    };
    // SAFETY: a live adapter.
    let force_sdr = unsafe { adapter_of(this) }.options.force_sdr;
    // SAFETY: `this` is a live adapter of this crate.
    let made = unsafe { output::create(this, monitor, force_sdr) };
    // SAFETY: as above.
    unsafe { com::write_out(out, made) };
    S_OK
}

extern "system" fn adapter_get_desc(this: *mut c_void, desc: *mut AdapterDesc) -> Hresult {
    // SAFETY: a live adapter.
    let adapter = unsafe { adapter_of(this) };
    // SAFETY: the application's description, null or a writable `DXGI_ADAPTER_DESC`.
    unsafe { adapter.describe(desc.cast::<u8>(), DESC_LEN, false) }
}

extern "system" fn adapter_get_desc1(this: *mut c_void, desc: *mut AdapterDesc1) -> Hresult {
    // SAFETY: a live adapter.
    let adapter = unsafe { adapter_of(this) };
    // SAFETY: the application's description, null or a writable `DXGI_ADAPTER_DESC1`.
    unsafe { adapter.describe(desc.cast::<u8>(), DESC1_LEN, true) }
}

extern "system" fn adapter_get_desc2(this: *mut c_void, desc: *mut AdapterDesc2) -> Hresult {
    // SAFETY: a live adapter.
    let adapter = unsafe { adapter_of(this) };
    // SAFETY: the application's description, null or a writable `DXGI_ADAPTER_DESC2`.
    unsafe { adapter.describe(desc.cast::<u8>(), DESC2_LEN, true) }
}

extern "system" fn adapter_get_desc3(this: *mut c_void, desc: *mut AdapterDesc3) -> Hresult {
    // SAFETY: a live adapter.
    let adapter = unsafe { adapter_of(this) };
    // SAFETY: the application's description, null or a writable `DXGI_ADAPTER_DESC3`.
    unsafe { adapter.describe(desc.cast::<u8>(), DESC3_LEN, false) }
}

/// `CheckInterfaceSupport`: the D3D10 device interfaces, with an unknown UMD version.
///
/// Anything else is `DXGI_ERROR_UNSUPPORTED`, logged with its IID; a null `guid` is
/// `E_INVALIDARG`.
extern "system" fn adapter_check_interface_support(
    _this: *mut c_void,
    guid: *const Guid,
    umd_version: *mut i64,
) -> Hresult {
    // SAFETY: the application's GUID, null or readable.
    let Some(guid) = (unsafe { Guid::read(guid) }) else {
        return E_INVALIDARG;
    };
    if [IID_IDXGI_DEVICE, IID_ID3D10_DEVICE, IID_ID3D10_DEVICE1].contains(&guid) {
        if !umd_version.is_null() {
            // SAFETY: the application's version slot, non-null; the write assumes no alignment.
            unsafe { umd_version.write_unaligned(UMD_VERSION) };
        }
        return S_OK;
    }
    LOG.err("DXGI: CheckInterfaceSupport: Unsupported interface");
    LOG.err(&guid.to_string());
    DXGI_ERROR_UNSUPPORTED
}

/// Slot 12: not implemented (the C++ asserts and returns nothing).
extern "system" fn adapter_register_hardware_content_protection_teardown_status_event(
    _this: *mut c_void,
    _event: *mut c_void,
    _cookie: *mut u32,
) -> Hresult {
    LOG.err("Not implemented");
    E_NOTIMPL
}

/// Slot 13: does nothing (the C++ asserts in debug builds only).
const extern "system" fn adapter_unregister_hardware_content_protection_teardown_status(
    _this: *mut c_void,
    _cookie: u32,
) {
}

/// `QueryVideoMemoryInfo`: the working-set size, the allocated size and the group's reservation.
extern "system" fn adapter_query_video_memory_info(
    this: *mut c_void,
    node: u32,
    group: u32,
    info: *mut QueryVideoMemoryInfo,
) -> Hresult {
    if node > 0 || info.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: a live adapter.
    let adapter = unsafe { adapter_of(this) };
    let Some(reservation) = adapter.reservation(group) else {
        return E_INVALIDARG;
    };
    let answer = QueryVideoMemoryInfo {
        // SAFETY: the adapter's retained device.
        budget: unsafe { ffi::MTLDevice_recommendedMaxWorkingSetSize(adapter.device) },
        // SAFETY: as above.
        current_usage: unsafe { ffi::MTLDevice_currentAllocatedSize(adapter.device) },
        available_for_reservation: 0,
        current_reservation: reservation.load(Ordering::Relaxed),
    };
    // SAFETY: the application's struct, non-null; the write assumes no alignment.
    unsafe { info.write_unaligned(answer) };
    S_OK
}

extern "system" fn adapter_set_video_memory_reservation(
    this: *mut c_void,
    node: u32,
    group: u32,
    reservation: u64,
) -> Hresult {
    if node > 0 {
        return E_INVALIDARG;
    }
    // SAFETY: a live adapter.
    let Some(slot) = unsafe { adapter_of(this) }.reservation(group) else {
        return E_INVALIDARG;
    };
    slot.store(reservation, Ordering::Relaxed);
    S_OK
}

/// Slot 16: a cookie of 0; a null cookie is `E_INVALIDARG` (the C++ writes through it).
const extern "system" fn adapter_register_video_memory_budget_change_notification_event(
    _this: *mut c_void,
    _event: *mut c_void,
    cookie: *mut u32,
) -> Hresult {
    if cookie.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: the application's cookie slot, non-null; the write assumes no alignment.
    unsafe { cookie.write_unaligned(0) };
    S_OK
}

/// Slot 17: does nothing.
const extern "system" fn adapter_unregister_video_memory_budget_change_notification(
    _this: *mut c_void,
    _cookie: u32,
) {
}

const extern "system" fn adapter_get_mtl_device(this: *mut c_void) -> u64 {
    // SAFETY: a live adapter.
    unsafe { adapter_of(this) }.device
}

const extern "system" fn adapter_get_local_d3dkmt(this: *mut c_void) -> u32 {
    // SAFETY: a live adapter.
    unsafe { adapter_of(this) }.kmt
}

#[cfg(test)]
mod tests;
