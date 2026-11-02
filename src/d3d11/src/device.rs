//! D3D11 device (`MTLD3D11Device`, `d3d11_device.hpp`, `d3d11_device.cpp`).

use core::{ffi::c_void, ptr, sync::atomic::AtomicU32};
use std::sync::LazyLock;

use dxmt_core::{device::Device as CoreDevice, queue::Queue};
use dxmt_util::{
    com::{
        ComObject, E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, Hresult, IID_IUNKNOWN, S_OK,
        add_ref, qi_log, release, write_out,
    },
    config::Config,
    private_data::PrivateData,
};
use winemetal_sys::{MTLDevice_supportsFamily, Obj};
use winemetal_wire::WMTGPU_FAMILY_APPLE7;

/// Process-wide configuration (`Config::getInstance`).
pub static CONFIG: LazyLock<Config> = LazyLock::new(|| Config::load(&crate::LOG));

pub use crate::multithread::DeviceMutex;
use crate::{
    LOG,
    abi::{
        IID_ID3D10_DEVICE, IID_ID3D10_DEVICE1, IID_ID3D11_DEVICE, IID_ID3D11_DEVICE_1,
        IID_ID3D11_DEVICE_2, IID_ID3D11_DEVICE_3, IID_ID3D11_DEVICE_4, IID_ID3D11_DEVICE_5,
        IID_ID3D11_MULTITHREAD, IID_IDXGI_DEVICE, IID_IDXGI_DEVICE_1, IID_IDXGI_DEVICE_2,
        IID_IDXGI_DEVICE_3, IID_IDXGI_OBJECT,
        ext::{
            Hwnd, IID_D3D11_CONTEXT_EXT, IID_D3D11_DEVICE_EXT, IID_IMTL_DXGI_DEVICE,
            IID_IMTL_SWAP_CHAIN_FACTORY, IMTLD3D11DeviceExtVtbl, IMTLDXGIAdapterVtbl,
            IMTLSwapChainFactoryVtbl, SwapChainDesc1, SwapChainFullscreenDesc,
        },
        vtbl::{ID3D11Device5Vtbl, IUnknownVtbl},
    },
    com::TearOff,
    context::{Context, Immediate},
    d3d10,
    inspection::Inspection,
    multithread::Multithread,
    pipeline::PipelineCache,
    state::cache::StateCaches,
};

pub mod dxgi;
pub mod impl_;
#[cfg(test)]
mod tests;

/// `DXGI_ERROR_INVALID_CALL` (`0x887a0001`).
pub const DXGI_ERROR_INVALID_CALL: Hresult = 0x887a_0001_u32.cast_signed();

/// Internal flag passed by `CreateD3D10Device` to select DirectX 10 behavior.
pub const D3D11_CREATE_DEVICE_D3D10_INTERNAL: u32 = 0x8000_0000;

/// Default feature level sequence when `pFeatureLevels` is null or empty.
pub const DEFAULT_FEATURE_LEVELS: [u32; 6] = [
    0xb000, // D3D_FEATURE_LEVEL_11_0
    0xa100, // D3D_FEATURE_LEVEL_10_1
    0xa000, // D3D_FEATURE_LEVEL_10_0
    0x9300, // D3D_FEATURE_LEVEL_9_3
    0x9200, // D3D_FEATURE_LEVEL_9_2
    0x9100, // D3D_FEATURE_LEVEL_9_1
];

const IID_ID3D11_DEBUG: Guid = Guid::new(
    0x79cf_2233,
    0x7536,
    0x4948,
    [0x9d, 0x36, 0x1e, 0x46, 0x92, 0xdc, 0x57, 0x60],
);
const IID_ID3D11_INFO_QUEUE: Guid = Guid::new(
    0x6543_dbb6,
    0x1b48,
    0x42f5,
    [0xab, 0x82, 0xe9, 0x7e, 0xc7, 0x43, 0x26, 0xf6],
);
const IID_RENDERDOC: Guid = Guid::new(
    0xa7aa_6116,
    0x9c8d,
    0x4bba,
    [0x90, 0x83, 0xb4, 0xd8, 0x16, 0xb7, 0x1b, 0x78],
);
const IID_PIX: Guid = Guid::new(
    0x9f25_1514,
    0x9d4d,
    0x4902,
    [0x9d, 0x60, 0x18, 0x98, 0x8a, 0xb7, 0xd4, 0xb5],
);
const IID_GPA: Guid = Guid::new(
    0xccff_ef16,
    0x7b69,
    0x468f,
    [0xbc, 0xe3, 0xcd, 0x95, 0x33, 0x69, 0xa3, 0x9a],
);

type DropHook = Box<dyn FnOnce(&CoreDevice, &StateCaches) + Send>;

/// Root COM container implementing `IMTLDXGIDevice` (`IDXGIDevice3`) and embedding [`Device`].
#[repr(C)]
pub struct DxgiDevice {
    /// Primary `IMTLDXGIDevice` (`IDXGIDevice3`) vtable at offset 0.
    pub vtbl: *const dxgi::IMTLDXGIDeviceVtbl,
    /// Shared reference count for the device and all embedded tear-offs.
    pub refs: ComObject,
    /// Shared `PrivateData` storage for both `IDXGIDevice` and `ID3D11Device`.
    pub private_data: PrivateData,
    /// Retained parent `IMTLDXGIAdapter*` COM pointer.
    pub adapter: *mut c_void,
    /// Local D3DKMT device handle associated with the adapter.
    pub local_kmt: u32,
    /// Embedded `ID3D11Device5` implementation.
    pub d3d11: Device,
}

impl Drop for DxgiDevice {
    fn drop(&mut self) {
        if !self.adapter.is_null() {
            // SAFETY: `self.adapter` was retained with `add_ref` on creation.
            unsafe {
                release(self.adapter);
            }
            self.adapter = ptr::null_mut();
        }
    }
}

/// D3D11 device (`MTLD3D11DeviceImpl`).
#[repr(C)]
pub struct Device {
    /// Primary `ID3D11Device5` vtable at offset 0 of `Device`.
    pub vtbl: *const ID3D11Device5Vtbl,
    /// `IMTLD3D11DeviceExt` tear-off subobject.
    pub ext: TearOff<IMTLD3D11DeviceExtVtbl>,
    /// `IMTLSwapChainFactory` tear-off subobject.
    pub swap_chain_factory: TearOff<IMTLSwapChainFactoryVtbl>,
    /// `ID3D11Multithread` (`ID3D10Multithread`) tear-off subobject.
    pub multithread: Multithread,
    /// Back-pointer to the enclosing [`DxgiDevice`] container.
    pub container: *mut DxgiDevice,
    /// Toggleable recursive device lock.
    pub mutex: DeviceMutex,
    /// Shader extension UAV slot set via `IMTLD3D11DeviceExt::SetShaderExtensionSlot`.
    pub shader_extension_slot: AtomicU32,
    core: CoreDevice,
    feature_level: u32,
    flags: u32,
    direct_x_version: u32,
    inspection: Inspection,
    immediate: *mut Context<Immediate>,
    pipeline_cache: PipelineCache,
    caches: StateCaches,
    d3d10: Option<Box<d3d10::Device>>,
    d3d10_tear_off: TearOff<IUnknownVtbl>,
    on_drop_after_drain: Option<DropHook>,
}

impl Device {
    /// Recovers `&Device` from an `ID3D11Device*` interface pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: Any non-null `ID3D11Device*` created by this crate points to `Device`.
        Some(unsafe { &*iface.cast::<Self>() })
    }

    /// Returns the underlying `dxmt_core::device::Device`.
    #[must_use]
    pub const fn core(&self) -> &CoreDevice {
        &self.core
    }

    /// Returns a mutable reference to the underlying `dxmt_core::device::Device`.
    pub const fn core_mut(&mut self) -> &mut CoreDevice {
        &mut self.core
    }

    /// Returns the command queue (`CommandQueue`).
    #[must_use]
    pub const fn queue(&self) -> &Queue {
        self.core.queue()
    }

    /// Returns the immediate context pointer (`GetImmediateContext`).
    #[must_use]
    pub const fn immediate(&self) -> *mut Context<Immediate> {
        self.immediate
    }

    /// Returns the device's pipeline cache (`GetPipelineCache`).
    #[must_use]
    pub const fn pipeline_cache(&self) -> &PipelineCache {
        &self.pipeline_cache
    }

    /// Returns the device's state-object caches.
    #[must_use]
    pub const fn caches(&self) -> &StateCaches {
        &self.caches
    }

    /// Returns `10` when created with `Flags & 0x8000_0000`, else `11` (`GetDirectXVersion`).
    #[must_use]
    pub const fn direct_x_version(&self) -> u32 {
        self.direct_x_version
    }

    /// Returns the embedded D3D10 device wrapper (`GetD3D10Device`).
    #[must_use]
    pub fn d3d10(&self) -> Option<&d3d10::Device> {
        self.d3d10.as_deref()
    }

    /// Returns the device feature level.
    #[must_use]
    pub const fn feature_level(&self) -> u32 {
        self.feature_level
    }

    /// Returns the raw creation flags (including internal flags).
    #[must_use]
    pub const fn raw_flags(&self) -> u32 {
        self.flags
    }

    /// Returns the `Inspection` capability helper.
    #[must_use]
    pub const fn inspection(&self) -> &Inspection {
        &self.inspection
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        if !self.immediate.is_null() {
            // SAFETY: `self.immediate` is a live `Context<Immediate>` owned by this device.
            unsafe {
                (*self.immediate).commit();
            }
            // SAFETY: `self.immediate` is a live `Context<Immediate>` owned by this device.
            unsafe {
                (*self.immediate).wait_until_gpu_idle();
            }
        }
        // Drain the command queue and shader scheduler BEFORE `caches` and `pipeline_cache` drop (P8).
        let seq = self.core.queue().current_seq();
        self.core.queue_mut().commit();
        self.core.queue().wait_cpu(seq);
        let _ = crate::scheduler::wait_idle(u32::MAX);
        if let Some(hook) = self.on_drop_after_drain.take() {
            hook(&self.core, &self.caches);
        }
    }
}

/// Computes the maximum supported feature level for device creation (`d3d11.cpp:59-80`).
#[must_use]
pub fn max_feature_level_from_config(cfg_value: &str, supports_apple7: bool) -> u32 {
    let default_max = if supports_apple7 { 0xb100 } else { 0xb000 };
    match cfg_value.trim() {
        "12_1" => 0xc100,
        "12_0" => 0xc000,
        "11_1" => 0xb100,
        "11_0" => 0xb000,
        "10_1" => 0xa100,
        "10_0" => 0xa000,
        "9_3" => 0x9300,
        "9_2" => 0x9200,
        "9_1" => 0x9100,
        _ => default_max,
    }
}

/// Selects the first valid feature level in `requested` that is `<= max_fl` (`d3d11.cpp:85-98`).
#[must_use]
pub fn select_feature_level(requested: &[u32], max_fl: u32) -> Option<u32> {
    requested
        .iter()
        .copied()
        .find(|&fl| is_known_feature_level(fl) && fl <= max_fl)
}

const fn is_known_feature_level(fl: u32) -> bool {
    matches!(
        fl,
        0x9100 | 0x9200 | 0x9300 | 0xa000 | 0xa100 | 0xb000 | 0xb100 | 0xc000 | 0xc100
    )
}

/// Queries whether the Metal device exposed by an `IMTLDXGIAdapter*` supports `WMTGPUFamilyApple7`.
///
/// # Safety
///
/// `adapter` must be a valid `IMTLDXGIAdapter*` COM pointer or null.
#[must_use]
pub unsafe fn adapter_supports_apple7(adapter: *mut c_void) -> bool {
    if adapter.is_null() {
        return false;
    }
    // SAFETY: Caller guarantees `adapter` is a valid `IMTLDXGIAdapter*`.
    let vtbl = unsafe { *adapter.cast::<*const IMTLDXGIAdapterVtbl>() };
    // SAFETY: `vtbl` is a valid `IMTLDXGIAdapterVtbl`.
    let get_mtl = unsafe { (*vtbl).get_mtl_device };
    // SAFETY: Calling `GetMTLDevice` on `adapter`.
    let mtl_dev = unsafe { get_mtl(adapter) };
    if mtl_dev == 0 {
        return false;
    }
    // SAFETY: `mtl_dev` is a valid `WMTDevice` handle.
    unsafe { MTLDevice_supportsFamily(mtl_dev, WMTGPU_FAMILY_APPLE7) }
}

/// Allocates and initializes a [`DxgiDevice`] with an existing [`CoreDevice`].
#[must_use]
fn create_with_core(
    adapter: *mut c_void,
    local_kmt: u32,
    core: CoreDevice,
    feature_level: u32,
    flags: u32,
) -> *mut DxgiDevice {
    if !adapter.is_null() {
        // SAFETY: `adapter` is a live COM pointer.
        unsafe {
            add_ref(adapter);
        }
    }
    let mtl_handle = core.device().handle();
    let direct_x_version = if (flags & D3D11_CREATE_DEVICE_D3D10_INTERNAL) != 0 {
        10
    } else {
        11
    };
    let box_dev = Box::new(DxgiDevice {
        vtbl: &raw const dxgi::VTBL,
        refs: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        adapter,
        local_kmt,
        d3d11: Device {
            vtbl: &raw const impl_::VTBL,
            ext: TearOff::new(&EXT_VTBL, ptr::null_mut()),
            swap_chain_factory: TearOff::new(&SWAP_CHAIN_FACTORY_VTBL, ptr::null_mut()),
            multithread: Multithread::new(),
            container: ptr::null_mut(),
            mutex: DeviceMutex::new(),
            shader_extension_slot: AtomicU32::new(u32::MAX),
            core,
            feature_level,
            flags,
            direct_x_version,
            inspection: Inspection::new(mtl_handle),
            immediate: ptr::null_mut(),
            pipeline_cache: PipelineCache::default(),
            caches: StateCaches,
            d3d10: None,
            d3d10_tear_off: TearOff::new(&D3D10_TEAR_OFF_VTBL, ptr::null_mut()),
            on_drop_after_drain: None,
        },
    });
    let raw = Box::into_raw(box_dev);
    let dxgi_iface = raw.cast::<c_void>();
    // SAFETY: `raw` was just allocated by `Box::into_raw` and is uniquely owned here.
    let dev = unsafe { &mut (*raw).d3d11 };
    dev.container = raw;
    dev.ext.outer = dxgi_iface;
    dev.swap_chain_factory.outer = dxgi_iface;
    dev.d3d10_tear_off.outer = dxgi_iface;
    let mutex_ptr = &raw const dev.mutex;
    dev.multithread.init(dxgi_iface, mutex_ptr);

    let d3d11_iface = ptr::from_mut(dev).cast::<c_void>();
    dev.d3d10 = Some(d3d10::Device::new(d3d11_iface, dev.immediate.cast()));
    raw
}

static D3D10_TEAR_OFF_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: TearOff::<()>::query_interface,
    add_ref: TearOff::<()>::add_ref,
    release: TearOff::<()>::release,
};

/// Creates a new `IMTLDXGIDevice` instance bound to `adapter` (`dxmt::CreateD3D11Device`).
///
/// # Safety
///
/// `adapter` must be a valid `IMTLDXGIAdapter*` COM pointer and `out` a writable pointer.
#[must_use]
pub unsafe fn create_d3d11_device(
    adapter: *mut c_void,
    feature_level: u32,
    flags: u32,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() || adapter.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `adapter` is a valid `IMTLDXGIAdapter*` COM pointer.
    let vtbl = unsafe { *adapter.cast::<*const IMTLDXGIAdapterVtbl>() };
    // SAFETY: `vtbl` points to a valid `IMTLDXGIAdapterVtbl`.
    let get_mtl = unsafe { (*vtbl).get_mtl_device };
    // SAFETY: `vtbl` points to a valid `IMTLDXGIAdapterVtbl`.
    let get_kmt = unsafe { (*vtbl).get_local_d3dkmt };
    // SAFETY: Calling `GetMTLDevice` on `adapter`.
    let mtl_handle = unsafe { get_mtl(adapter) };
    // SAFETY: Calling `GetLocalD3DKMT` on `adapter`.
    let adapter_kmt = unsafe { get_kmt(adapter) };

    // Retain the adapter's Metal device handle for `CoreDevice`.
    // SAFETY: `mtl_handle` is a live `WMTDevice` handle owned by `adapter`.
    let Some(mtl_obj) = (unsafe { Obj::retain(mtl_handle) }) else {
        return E_INVALIDARG;
    };
    let metal_version_opt = CONFIG.get_i32("dxmt.shaderMetalVersion", 0);
    let core = CoreDevice::new(mtl_obj, metal_version_opt);
    let raw = create_with_core(adapter, adapter_kmt, core, feature_level, flags);
    // SAFETY: `out` was checked non-null above.
    unsafe {
        write_out(out, raw.cast::<c_void>());
    }
    S_OK
}

/// Shared `QueryInterface` implementation for [`DxgiDevice`], [`Device`], and all device tear-offs.
///
/// # Safety
///
/// `raw` must be a valid `DxgiDevice` pointer; `riid` and `out` must obey `QueryInterface` rules.
pub unsafe fn container_query_interface(
    raw: *mut DxgiDevice,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `out` is non-null.
    unsafe {
        write_out(out, ptr::null_mut());
    }
    // SAFETY: `riid` is a `GUID` pointer passed by the COM caller.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return DXGI_ERROR_INVALID_CALL;
    };

    if iid == IID_IUNKNOWN
        || iid == IID_IDXGI_OBJECT
        || iid == IID_IDXGI_DEVICE
        || iid == IID_IDXGI_DEVICE_1
        || iid == IID_IDXGI_DEVICE_2
        || iid == IID_IDXGI_DEVICE_3
        || iid == IID_IMTL_DXGI_DEVICE
    {
        // SAFETY: `raw` is a valid `DxgiDevice` pointer.
        unsafe { (*raw).refs.add_ref() };
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, raw.cast::<c_void>());
        }
        return S_OK;
    }

    if iid == IID_ID3D11_DEVICE
        || iid == IID_ID3D11_DEVICE_1
        || iid == IID_ID3D11_DEVICE_2
        || iid == IID_ID3D11_DEVICE_3
        || iid == IID_ID3D11_DEVICE_4
        || iid == IID_ID3D11_DEVICE_5
    {
        // SAFETY: `raw` is a valid `DxgiDevice` pointer.
        unsafe { (*raw).refs.add_ref() };
        // SAFETY: Projecting the `d3d11` field of `raw`.
        let iface = unsafe { (&raw mut (*raw).d3d11).cast::<c_void>() };
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, iface);
        }
        return S_OK;
    }

    if iid == IID_ID3D10_DEVICE || iid == IID_ID3D10_DEVICE1 {
        // SAFETY: `raw` is a valid `DxgiDevice` pointer.
        unsafe { (*raw).refs.add_ref() };
        // SAFETY: Projecting `d3d10_tear_off` on `raw`.
        let iface = unsafe { (*raw).d3d11.d3d10_tear_off.as_iface() };
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, iface);
        }
        return S_OK;
    }

    if iid == IID_IMTL_SWAP_CHAIN_FACTORY {
        // SAFETY: `raw` is a valid `DxgiDevice` pointer.
        unsafe { (*raw).refs.add_ref() };
        // SAFETY: Projecting `swap_chain_factory` on `raw`.
        let iface = unsafe { (*raw).d3d11.swap_chain_factory.as_iface() };
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, iface);
        }
        return S_OK;
    }

    if iid == IID_D3D11_DEVICE_EXT {
        // SAFETY: `raw` is a valid `DxgiDevice` pointer.
        unsafe { (*raw).refs.add_ref() };
        // SAFETY: Projecting `ext` on `raw`.
        let iface = unsafe { (*raw).d3d11.ext.as_iface() };
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, iface);
        }
        return S_OK;
    }

    if iid == IID_ID3D11_MULTITHREAD {
        // SAFETY: `raw` is a valid `DxgiDevice` pointer.
        unsafe { (*raw).refs.add_ref() };
        // SAFETY: Projecting `multithread` on `raw`.
        let iface = unsafe { (*raw).d3d11.multithread.as_iface() };
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, iface);
        }
        return S_OK;
    }

    if iid == IID_ID3D11_DEBUG
        || iid == IID_ID3D11_INFO_QUEUE
        || iid == IID_D3D11_CONTEXT_EXT
        || iid == IID_RENDERDOC
        || iid == IID_PIX
        || iid == IID_GPA
    {
        return E_NOINTERFACE;
    }

    if qi_log(&IID_IMTL_DXGI_DEVICE, &iid) {
        LOG.warn(&format!(
            "D3D11DXGIDevice::QueryInterface: Unknown interface query {iid}"
        ));
    }
    E_NOINTERFACE
}

/// Increments the shared container reference count.
///
/// # Safety
///
/// `raw` must be a valid `DxgiDevice` pointer.
pub unsafe fn container_add_ref(raw: *mut DxgiDevice) -> u32 {
    // SAFETY: `raw` is a valid `DxgiDevice` pointer.
    unsafe { (*raw).refs.add_ref() }
}

/// Decrements the shared container reference count and frees the [`DxgiDevice`] when it reaches 0.
///
/// # Safety
///
/// `raw` must be a valid `DxgiDevice` pointer.
pub unsafe fn container_release(raw: *mut DxgiDevice) -> u32 {
    // SAFETY: `raw` is a valid `DxgiDevice` pointer.
    let (rc, free) = unsafe { (*raw).refs.release_and_free() };
    if free {
        // SAFETY: `raw` was allocated by `Box::into_raw` in `create_with_core` and refcount hit 0.
        drop(unsafe { Box::from_raw(raw) });
    }
    rc
}

static EXT_VTBL: IMTLD3D11DeviceExtVtbl = IMTLD3D11DeviceExtVtbl {
    unknown: IUnknownVtbl {
        query_interface: TearOff::<()>::query_interface,
        add_ref: TearOff::<()>::add_ref,
        release: TearOff::<()>::release,
    },
    set_shader_extension_slot,
};

unsafe extern "system-unwind" fn set_shader_extension_slot(this: *mut c_void, uav_slot: u32) {
    // SAFETY: `this` is `&mut device.ext`, whose `outer` is the enclosing `DxgiDevice*`.
    let outer = unsafe { (*this.cast::<TearOff<IMTLD3D11DeviceExtVtbl>>()).outer };
    if !outer.is_null() {
        // SAFETY: `outer` is a valid `DxgiDevice` pointer.
        let dev = unsafe { &(*outer.cast::<DxgiDevice>()).d3d11 };
        dev.shader_extension_slot
            .store(uav_slot, core::sync::atomic::Ordering::Relaxed);
    }
}

static SWAP_CHAIN_FACTORY_VTBL: IMTLSwapChainFactoryVtbl = IMTLSwapChainFactoryVtbl {
    unknown: IUnknownVtbl {
        query_interface: TearOff::<()>::query_interface,
        add_ref: TearOff::<()>::add_ref,
        release: TearOff::<()>::release,
    },
    create_swap_chain: factory_create_swap_chain,
};

unsafe extern "system-unwind" fn factory_create_swap_chain(
    this: *mut c_void,
    factory: *mut c_void,
    hwnd: Hwnd,
    desc: *const SwapChainDesc1,
    fullscreen_desc: *const SwapChainFullscreenDesc,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is `&mut device.swap_chain_factory`, whose `outer` is `DxgiDevice*`.
    let outer = unsafe { (*this.cast::<TearOff<IMTLSwapChainFactoryVtbl>>()).outer };
    if outer.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `outer` is a valid `DxgiDevice` pointer.
    let d3d11_ptr = unsafe { (&raw mut (*outer.cast::<DxgiDevice>()).d3d11).cast::<c_void>() };
    crate::swapchain::create_swap_chain(
        d3d11_ptr,
        factory,
        hwnd,
        desc.cast(),
        fullscreen_desc.cast(),
        out,
    )
}
