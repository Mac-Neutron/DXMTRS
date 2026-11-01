//! `IMTLD3D12DeviceExt` and `MTLD3D12TemporalScalerImpl` (`d3d12_device.cpp:267-321, 470-615`).

use core::{ffi::c_void, sync::atomic::AtomicBool};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dxmt_core::{
    format::{
        MTL_DXGI_FORMAT_BC, MTL_DXGI_FORMAT_DEPTH_PLANER, MTL_DXGI_FORMAT_STENCIL_PLANER,
        MTL_DXGI_FORMAT_TYPELESS, is_integer, query_dxgi,
    },
    presenter::framegen::TemporalScaler,
    resource::{Texture, TextureAllocationFlags},
};
use dxmt_util::{com::ComObject, float_cast::u32_to_f32, private_data::PrivateData};
use winemetal_sys::MTLDevice_temporalScalerScaleRange;
use winemetal_wire::{
    WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
    WMT_PIXEL_FORMAT_INVALID, WMT_PIXEL_FORMAT_R32_FLOAT, WMT_PIXEL_FORMAT_R32_UINT,
    WMT_PIXEL_FORMAT_RG16_FLOAT, WMT_PIXEL_FORMAT_RG16_SINT, WMT_PIXEL_FORMAT_RG16_SNORM,
    WMT_PIXEL_FORMAT_RG16_UINT, WMT_PIXEL_FORMAT_RG16_UNORM, WMT_PIXEL_FORMAT_RG32_FLOAT,
    WMT_PIXEL_FORMAT_RG32_SINT, WMT_PIXEL_FORMAT_RG32_UINT, WMT_PIXEL_FORMAT_RGBA16_FLOAT,
    WMT_PIXEL_FORMAT_RGBA16_SINT, WMT_PIXEL_FORMAT_RGBA16_SNORM, WMT_PIXEL_FORMAT_RGBA16_UINT,
    WMT_PIXEL_FORMAT_RGBA16_UNORM, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
    WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_TEXTURE_FLAG_LOSSLESS, WMT_TEXTURE_TYPE2D,
    WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW, WMT_TEXTURE_USAGE_RENDER_TARGET,
    WMT_TEXTURE_USAGE_SHADER_READ, WMT_TEXTURE_USAGE_SHADER_WRITE, WMTFXTemporalScalerInfo,
    WMTTextureInfo,
};

use crate::{
    abi::{
        ext::{D3D12ScalerDesc, D3D12TemporalScalerVtbl, IID_D3D12_TEMPORAL_SCALER},
        vtbl::IUnknownVtbl,
    },
    com::{
        self, ComRefCounted, DXGI_ERROR_UNSUPPORTED, E_INVALIDARG, E_NOINTERFACE, E_OUTOFMEMORY,
        E_POINTER, Guid, Hresult, IID_IUNKNOWN, S_OK,
    },
    device::Device,
    slot,
};

const DXGI_FORMAT_UNKNOWN: u32 = 0;
const DXGI_FORMAT_R32G32B32A32_TYPELESS: u32 = 1;
const DXGI_FORMAT_R32G32B32A32_FLOAT: u32 = 2;
const DXGI_FORMAT_R16G16B16A16_TYPELESS: u32 = 9;
const DXGI_FORMAT_R16G16B16A16_FLOAT: u32 = 10;
const DXGI_FORMAT_R10G10B10A2_TYPELESS: u32 = 23;
const DXGI_FORMAT_R10G10B10A2_UNORM: u32 = 24;
const DXGI_FORMAT_R8G8B8A8_TYPELESS: u32 = 27;
const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
const DXGI_FORMAT_B8G8R8A8_UNORM: u32 = 87;
const DXGI_FORMAT_B8G8R8A8_TYPELESS: u32 = 90;

/// Which scratch texture a temporal upscale needs (`MTLD3D12TemporalScaler::Scratch`,
///
/// `d3d12_device.hpp:163`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scratch {
    Depth,
    Motion,
    Output,
}

/// `MTLD3D12TemporalScalerImpl` (`d3d12_device.hpp:156-165`, `d3d12_device.cpp:267-321`).
#[repr(C)]
pub struct TemporalScalerObject {
    pub vtbl: &'static D3D12TemporalScalerVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub desc: D3D12ScalerDesc,
    pub info: WMTFXTemporalScalerInfo,
    pub scaler: Arc<TemporalScaler>,
    pub fresh: AtomicBool,
    pub depth_scratch: Mutex<Option<Arc<Texture>>>,
    pub motion_scratch: Mutex<Option<Arc<Texture>>>,
    pub output_scratch: Mutex<Option<Arc<Texture>>>,
}

impl ComRefCounted for TemporalScalerObject {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for TemporalScalerObject {
    fn drop(&mut self) {
        if !self.device.is_null() {
            // SAFETY: `self.device` is kept alive by a public reference acquired on creation.
            let _ = unsafe { com::release_com(self.device) };
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

unsafe fn scaler_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv: *mut *mut c_void,
) -> Hresult {
    if ppv.is_null() {
        return E_POINTER;
    }
    // SAFETY: `ppv` is non-null.
    unsafe { com::init_out_ptr(ppv) };
    // SAFETY: `riid` is null or points to a 16-byte `Guid`.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    if iid == IID_IUNKNOWN || iid == IID_D3D12_TEMPORAL_SCALER {
        // SAFETY: `this` is a live `TemporalScalerObject`.
        let _ = unsafe { com::add_ref_com(this.cast::<TemporalScalerObject>()) };
        // SAFETY: `ppv` is non-null.
        unsafe { ppv.write_unaligned(this) };
        return S_OK;
    }
    E_NOINTERFACE
}

static TEMPORAL_SCALER_VTBL: D3D12TemporalScalerVtbl = D3D12TemporalScalerVtbl {
    unknown: IUnknownVtbl {
        query_interface: slot!(
            (this: *mut c_void, riid: *const c_void, ppv_object: *mut *mut c_void) -> Hresult => {
                // SAFETY: caller forwards COM slot arguments.
                unsafe { scaler_query_interface(this, riid, ppv_object) }
            }
        ),
        add_ref: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is a live `TemporalScalerObject`.
            unsafe { com::add_ref_com(this.cast::<TemporalScalerObject>()) }
        }),
        release: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is a live `TemporalScalerObject`.
            unsafe { com::release_com(this.cast::<TemporalScalerObject>()) }
        }),
    },
};

impl TemporalScalerObject {
    /// Returns (creating on first use) the scratch texture for `which` (`GetScratch`,
    ///
    /// `d3d12_device.cpp:295`).
    #[must_use]
    pub fn get_scratch(&self, which: Scratch) -> Option<Arc<Texture>> {
        if self.device.is_null() {
            return None;
        }
        // SAFETY: `self.device` is kept alive for the lifetime of `self`.
        let dev = unsafe { &*self.device };
        let mtl = dev.mtl.as_ref()?;
        let output_size = which == Scratch::Output
            || (which == Scratch::Motion && self.info.output_resolution_motion_vectors != 0);
        let (width, height) = if output_size {
            (self.desc.output_width, self.desc.output_height)
        } else {
            (self.desc.input_width, self.desc.input_height)
        };
        let (slot, format) = match which {
            Scratch::Depth => (&self.depth_scratch, self.info.depth_format),
            Scratch::Motion => (&self.motion_scratch, self.info.motion_format),
            Scratch::Output => (&self.output_scratch, self.info.output_format),
        };
        let usage = WMT_TEXTURE_USAGE_SHADER_READ
            | WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW
            | if which == Scratch::Output {
                WMT_TEXTURE_USAGE_SHADER_WRITE | WMT_TEXTURE_USAGE_RENDER_TARGET
            } else {
                0
            };
        let mut guard = lock(slot);
        if let Some(tex) = guard.as_ref() {
            return Some(Arc::clone(tex));
        }
        let bits = (WMT_TEXTURE_TYPE2D & 0xff)
            | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
            | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
            | ((usage & 0xff) << WMTTextureInfo::USAGE_SHIFT);
        let lossless = std::env::var("DXMT_D3D12_COMPRESSION").as_deref() != Ok("0");
        let info = WMTTextureInfo {
            pixel_format: format,
            width,
            height,
            depth: 1,
            array_length: 1,
            bits,
            options: WMT_RESOURCE_STORAGE_MODE_PRIVATE
                | WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
            flags: if lossless {
                WMT_TEXTURE_FLAG_LOSSLESS
            } else {
                0
            },
            mach_port: 0,
            gpu_resource_id: 0,
        };
        let tex = Arc::new(Texture::new(info, mtl));
        let alloc = tex.allocate(TextureAllocationFlags::GPU_PRIVATE);
        let _ = tex.rename(alloc);
        *guard = Some(Arc::clone(&tex));
        drop(guard);
        Some(tex)
    }
}

/// Casts a non-null `IMTLD3D12TemporalScaler` pointer if its vtable matches
///
/// [`TemporalScalerObject`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer (a valid vtable pointer at
/// offset 0).
#[must_use]
pub unsafe fn cast_temporal_scaler<'a>(raw: *mut c_void) -> Option<&'a TemporalScalerObject> {
    // SAFETY: `raw` is null or a valid COM object pointer per caller contract.
    unsafe {
        com::cast_com::<TemporalScalerObject, D3D12TemporalScalerVtbl>(raw, &TEMPORAL_SCALER_VTBL)
    }
}

/// `IMTLD3D12DeviceExt::GetTemporalScalerScaleRange` (`d3d12_device.cpp:471`).
///
/// # Safety
///
/// `this`, `min_scale`, and `max_scale` are caller-supplied pointers.
pub unsafe fn get_temporal_scaler_scale_range(
    this: *mut c_void,
    min_scale: *mut f32,
    max_scale: *mut f32,
) -> Hresult {
    if this.is_null() || min_scale.is_null() || max_scale.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is the companion pointer at `Device::EXT_OFFSET` inside a live `Device`.
    let dev_ptr = unsafe { com::from_companion::<Device>(this, Device::EXT_OFFSET) };
    // SAFETY: `dev_ptr` points to the enclosing `Device`.
    let dev = unsafe { &*dev_ptr };
    let Some(mtl) = dev.mtl.as_ref() else {
        return E_INVALIDARG;
    };
    let h = mtl.handle();
    // SAFETY: `h` is a live `MTLDevice` handle and `min_scale`/`max_scale` are non-null.
    unsafe { MTLDevice_temporalScalerScaleRange(h, min_scale, max_scale) };
    S_OK
}

const fn info_eq(a: &WMTFXTemporalScalerInfo, b: &WMTFXTemporalScalerInfo) -> bool {
    a.color_format == b.color_format
        && a.output_format == b.output_format
        && a.depth_format == b.depth_format
        && a.motion_format == b.motion_format
        && a.input_width == b.input_width
        && a.input_height == b.input_height
        && a.output_width == b.output_width
        && a.output_height == b.output_height
        && a.input_content_min_scale.to_bits() == b.input_content_min_scale.to_bits()
        && a.input_content_max_scale.to_bits() == b.input_content_max_scale.to_bits()
        && a.auto_exposure == b.auto_exposure
        && a.input_content_properties_enabled == b.input_content_properties_enabled
        && a.requires_synchronous_initialization == b.requires_synchronous_initialization
        && a.reactive_mask_enabled == b.reactive_mask_enabled
        && a.reactive_mask_format == b.reactive_mask_format
        && a.output_resolution_motion_vectors == b.output_resolution_motion_vectors
        && a.jittered_motion_vectors == b.jittered_motion_vectors
}

const fn typed_dxgi_format(format: u32) -> u32 {
    match format {
        DXGI_FORMAT_R16G16B16A16_TYPELESS => DXGI_FORMAT_R16G16B16A16_FLOAT,
        DXGI_FORMAT_R32G32B32A32_TYPELESS => DXGI_FORMAT_R32G32B32A32_FLOAT,
        DXGI_FORMAT_R10G10B10A2_TYPELESS => DXGI_FORMAT_R10G10B10A2_UNORM,
        DXGI_FORMAT_R8G8B8A8_TYPELESS => DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_B8G8R8A8_TYPELESS => DXGI_FORMAT_B8G8R8A8_UNORM,
        other => other,
    }
}

fn colour_pixel_format(dxgi_format: u32) -> u32 {
    let (hr, f) = query_dxgi(dxgi_format);
    if hr < 0 {
        return WMT_PIXEL_FORMAT_INVALID;
    }
    let disallowed = MTL_DXGI_FORMAT_BC
        | MTL_DXGI_FORMAT_DEPTH_PLANER
        | MTL_DXGI_FORMAT_STENCIL_PLANER
        | MTL_DXGI_FORMAT_TYPELESS;
    if f.pixel_format != WMT_PIXEL_FORMAT_INVALID
        && !is_integer(f.pixel_format)
        && (f.flag & disallowed) == 0
    {
        f.pixel_format
    } else {
        WMT_PIXEL_FORMAT_INVALID
    }
}

/// `IMTLD3D12DeviceExt::CreateTemporalScaler` (`d3d12_device.cpp:479`).
///
/// # Safety
///
/// `this`, `desc`, and `scaler` are caller-supplied pointers.
pub unsafe fn create_temporal_scaler(
    this: *mut c_void,
    desc: *const D3D12ScalerDesc,
    scaler: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateTemporalScaler");
    if this.is_null() || desc.is_null() || scaler.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `scaler` is non-null.
    unsafe { com::init_out_ptr(scaler) };
    // SAFETY: `this` is the companion pointer at `Device::EXT_OFFSET` inside a live `Device`.
    let dev_ptr = unsafe { com::from_companion::<Device>(this, Device::EXT_OFFSET) };
    // SAFETY: `dev_ptr` points to the enclosing `Device`.
    let dev = unsafe { &*dev_ptr };
    if !dev.temporal_supported {
        return DXGI_ERROR_UNSUPPORTED;
    }
    let Some(mtl) = dev.mtl.as_ref() else {
        return DXGI_ERROR_UNSUPPORTED;
    };
    // SAFETY: `desc` is non-null.
    let d = unsafe { desc.read_unaligned() };

    let mut lo = 0.0f32;
    let mut hi = 0.0f32;
    let h = mtl.handle();
    // SAFETY: `h` is a live `MTLDevice` handle and `lo`/`hi` are valid out-pointers.
    unsafe { MTLDevice_temporalScalerScaleRange(h, &raw mut lo, &raw mut hi) };

    let min = if d.input_content_min_scale == 0.0 {
        lo
    } else {
        d.input_content_min_scale.clamp(lo, hi)
    };
    let max = if d.input_content_max_scale == 0.0 {
        hi
    } else {
        d.input_content_max_scale.clamp(lo, hi)
    };

    if d.input_width == 0
        || d.input_height == 0
        || d.output_width == 0
        || d.output_height == 0
        || min.partial_cmp(&max).is_none_or(core::cmp::Ordering::is_gt)
    {
        return E_INVALIDARG;
    }
    let sx = u32_to_f32(d.output_width) / u32_to_f32(d.input_width);
    let sy = u32_to_f32(d.output_height) / u32_to_f32(d.input_height);
    if sx < lo || sy < lo || sx > max || sy > max {
        return E_INVALIDARG;
    }

    let (hr_d, depth_f) = query_dxgi(d.depth_format);
    let depth = if hr_d < 0 {
        WMT_PIXEL_FORMAT_INVALID
    } else {
        match depth_f.pixel_format {
            WMT_PIXEL_FORMAT_DEPTH32_FLOAT
            | WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8
            | WMT_PIXEL_FORMAT_R32_FLOAT
            | WMT_PIXEL_FORMAT_R32_UINT => WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
            _ => WMT_PIXEL_FORMAT_INVALID,
        }
    };

    let (hr_m, motion_f) = query_dxgi(d.motion_format);
    let motion = if hr_m < 0 {
        WMT_PIXEL_FORMAT_INVALID
    } else {
        match motion_f.pixel_format {
            WMT_PIXEL_FORMAT_RG16_UINT
            | WMT_PIXEL_FORMAT_RG16_FLOAT
            | WMT_PIXEL_FORMAT_RG16_SINT
            | WMT_PIXEL_FORMAT_RG16_SNORM
            | WMT_PIXEL_FORMAT_RG16_UNORM => WMT_PIXEL_FORMAT_RG16_FLOAT,
            WMT_PIXEL_FORMAT_RG32_UINT
            | WMT_PIXEL_FORMAT_RG32_FLOAT
            | WMT_PIXEL_FORMAT_RG32_SINT => WMT_PIXEL_FORMAT_RG32_FLOAT,
            WMT_PIXEL_FORMAT_RGBA16_SINT
            | WMT_PIXEL_FORMAT_RGBA16_SNORM
            | WMT_PIXEL_FORMAT_RGBA16_UINT
            | WMT_PIXEL_FORMAT_RGBA16_UNORM
            | WMT_PIXEL_FORMAT_RGBA16_FLOAT => WMT_PIXEL_FORMAT_RGBA16_FLOAT,
            _ => WMT_PIXEL_FORMAT_INVALID,
        }
    };

    let color_format = colour_pixel_format(typed_dxgi_format(d.color_format));
    let output_format = colour_pixel_format(typed_dxgi_format(d.output_format));
    let reactive_mask_enabled = u8::from(d.reactive_mask_format != DXGI_FORMAT_UNKNOWN);
    let reactive_mask_format = if reactive_mask_enabled != 0 {
        colour_pixel_format(d.reactive_mask_format)
    } else {
        WMT_PIXEL_FORMAT_INVALID
    };

    if color_format == WMT_PIXEL_FORMAT_INVALID
        || output_format == WMT_PIXEL_FORMAT_INVALID
        || depth == WMT_PIXEL_FORMAT_INVALID
        || motion == WMT_PIXEL_FORMAT_INVALID
        || (reactive_mask_enabled != 0 && reactive_mask_format == WMT_PIXEL_FORMAT_INVALID)
    {
        return E_INVALIDARG;
    }

    let info = WMTFXTemporalScalerInfo {
        color_format,
        output_format,
        depth_format: depth,
        motion_format: motion,
        input_width: d.input_width,
        input_height: d.input_height,
        output_width: d.output_width,
        output_height: d.output_height,
        input_content_min_scale: min,
        input_content_max_scale: max,
        auto_exposure: u8::from(d.auto_exposure != 0),
        input_content_properties_enabled: 1,
        requires_synchronous_initialization: 0,
        reactive_mask_enabled,
        reactive_mask_format,
        output_resolution_motion_vectors: u8::from(d.output_resolution_motion_vectors != 0),
        jittered_motion_vectors: u8::from(d.jittered_motion_vectors != 0),
    };

    let Some(fx) = acquire_temporal_scaler(dev, &info) else {
        crate::err!(
            "CreateTemporalScaler: MetalFX made no upscaler for {}x{} to {}x{}",
            d.input_width,
            d.input_height,
            d.output_width,
            d.output_height
        );
        return E_OUTOFMEMORY;
    };

    // SAFETY: `dev_ptr` is a live `Device`; retains a public reference for `TemporalScalerObject`.
    let _ = unsafe { com::add_ref_com(dev_ptr) };

    let obj = Box::new(TemporalScalerObject {
        vtbl: &TEMPORAL_SCALER_VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: dev_ptr,
        desc: d,
        info,
        scaler: fx,
        fresh: AtomicBool::new(true),
        depth_scratch: Mutex::new(None),
        motion_scratch: Mutex::new(None),
        output_scratch: Mutex::new(None),
    });
    // SAFETY: `scaler` was validated non-null above.
    unsafe { scaler.write_unaligned(Box::into_raw(obj).cast::<c_void>()) };
    S_OK
}

/// Acquires or creates a `MetalFX` temporal scaler matching `info` (`d3d12_device.cpp:593`).
#[must_use]
pub fn acquire_temporal_scaler(
    device: &Device,
    info: &WMTFXTemporalScalerInfo,
) -> Option<Arc<TemporalScaler>> {
    let mut pool = lock(&device.upscalers);
    let metal = if let Some(pos) = pool
        .iter()
        .position(|(k, v)| info_eq(k, info) && Arc::strong_count(v) == 1)
    {
        let (_, obj) = pool.remove(pos);
        obj
    } else {
        let mtl = device.mtl.as_ref()?;
        let h = mtl.handle();
        // SAFETY: `h` is a live `MTLDevice` handle.
        let ts = unsafe { TemporalScaler::new(h, *info) };
        ts.scaler().is_some().then(|| Arc::new(ts))?
    };
    pool.push((*info, Arc::clone(&metal)));
    while pool
        .iter()
        .filter(|(_, s)| Arc::strong_count(s) == 1)
        .count()
        > 4
    {
        if let Some(pos) = pool.iter().position(|(_, s)| Arc::strong_count(s) == 1) {
            let _ = pool.remove(pos);
        }
    }
    drop(pool);
    Some(metal)
}

/// Returns a `MetalFX` temporal scaler to the device's pool (`d3d12_device.cpp:609`).
pub fn release_temporal_scaler(
    device: &Device,
    info: WMTFXTemporalScalerInfo,
    scaler: Arc<TemporalScaler>,
) {
    let mut pool = lock(&device.upscalers);
    pool.push((info, scaler));
}
