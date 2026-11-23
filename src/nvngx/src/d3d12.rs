//! NGX's Direct3D 12 entry points, on DXMT's private interface to `MetalFX`'s temporal upscaler.
//!
//! A DLSS feature's upscaler is made at its first evaluation, from the formats of the textures the
//! game really passes (creation names only sizes and flags), and made again when they change.
//! NGX's conventions are `MetalFX`'s: motion vectors point from the current frame to the previous
//! one, in pixels after `MV.Scale`; the jitter is the sample's offset in input pixels. The first
//! evaluation logs what the game passed to the game's log (stderr), once, before any pixel is
//! trusted.

use core::ffi::{CStr, c_void};
use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use dxmt_util::{
    com::{Guid, Hresult, IUnknownVtbl},
    dxgi_consts::{
        DXGI_FORMAT_B8G8R8A8_TYPELESS, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8_TYPELESS,
        DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_R8G8B8A8_TYPELESS, DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R10G10B10A2_TYPELESS, DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16_TYPELESS,
        DXGI_FORMAT_R16_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R16G16B16A16_TYPELESS,
        DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_R32G32B32A32_TYPELESS, DXGI_FORMAT_UNKNOWN,
    },
    ext::{
        D3D12CommandListExt1Vtbl, D3D12CommandListExtVtbl, D3D12DenoisedScalerDesc,
        D3D12DenoisedUpscaleDesc, D3D12DeviceExt1Vtbl, D3D12DeviceExtVtbl, D3D12FrameGenUiDesc,
        D3D12ScalerDesc, D3D12UpscaleDesc, IID_D3D12_COMMAND_LIST_EXT, IID_D3D12_DEVICE_EXT,
        IID_IMTL_D3D12_COMMAND_LIST_EXT1, IID_IMTL_D3D12_DEVICE_EXT1,
    },
    sync::lock,
};

use crate::{
    com::{
        CommandList12Vtbl, IID_ID3D12_DEVICE, Resource12Vtbl, ResourceDesc12, query, release, vtbl,
    },
    ngx::{
        DLSSG_CAMERA_GUID, Dlss, DlssgCameraParams, FAIL, FEATURE_FRAME_GENERATION,
        FEATURE_NOT_SUPPORTED, FEATURE_RAY_RECONSTRUCTION, FEATURE_SUPERSAMPLING,
        FLAG_AUTO_EXPOSURE, FLAG_DEPTH_INVERTED, FLAG_MV_JITTERED, FLAG_MV_LOWRES,
        INVALID_PARAMETER, OK, PARAM_DENOISE_STRENGTH_MASK, PARAM_DIFFUSE_ALBEDO,
        PARAM_DLSSD_DENOISE_STRENGTH_MASK, PARAM_DLSSD_DIFFUSE_ALBEDO, PARAM_DLSSD_NORMALS,
        PARAM_DLSSD_ROUGHNESS, PARAM_DLSSD_SPECULAR_ALBEDO, PARAM_DLSSD_SPECULAR_HIT_DISTANCE,
        PARAM_DLSSD_TRANSPARENCY_OVERLAY, PARAM_DLSSD_VIEW_TO_CLIP_MATRIX,
        PARAM_DLSSD_WORLD_TO_VIEW_MATRIX, PARAM_DLSSG_BACKBUFFER_SUBRECT_BASE_X,
        PARAM_DLSSG_BACKBUFFER_SUBRECT_BASE_Y, PARAM_DLSSG_CAMERA_ASPECT, PARAM_DLSSG_CAMERA_FAR,
        PARAM_DLSSG_CAMERA_FOV, PARAM_DLSSG_CAMERA_NEAR, PARAM_DLSSG_CONTENT_HEIGHT,
        PARAM_DLSSG_CONTENT_WIDTH, PARAM_DLSSG_DEPTH_SUBRECT_BASE_X,
        PARAM_DLSSG_DEPTH_SUBRECT_BASE_Y, PARAM_DLSSG_HUDLESS, PARAM_DLSSG_JITTER_OFFSET_X,
        PARAM_DLSSG_JITTER_OFFSET_Y, PARAM_DLSSG_MVEC_SCALE_X, PARAM_DLSSG_MVEC_SCALE_Y,
        PARAM_DLSSG_MVEC_SUBRECT_BASE_X, PARAM_DLSSG_MVEC_SUBRECT_BASE_Y, PARAM_DLSSG_UI,
        PARAM_DLSSG_UI_COLOR_AND_ALPHA, PARAM_DLSSG_UI_COMPOSITED, PARAM_DLSSG_VIEW_TO_CLIP_MATRIX,
        PARAM_DLSSG_WORLD_TO_VIEW_MATRIX, PARAM_EXPOSURE_SCALE, PARAM_NORMALS,
        PARAM_REACTIVE_ACTIVE, PARAM_REACTIVE_BASE_X, PARAM_REACTIVE_BASE_Y, PARAM_REACTIVE_MASK,
        PARAM_ROUGHNESS, PARAM_SPECULAR_ALBEDO, PARAM_SPECULAR_HIT_DISTANCE,
        PARAM_TRANSPARENCY_OVERLAY, PARAM_VIEW_TO_CLIP_MATRIX, PARAM_WORLD_TO_VIEW_MATRIX, hex,
        note, pointer,
    },
    parameter::Parameters,
};

/// Whether the initialised D3D12 device supports `MetalFX`'s temporal denoised scaler.
static DENOISED_SCALER_SUPPORTED: AtomicBool = AtomicBool::new(false);

/// Answers whether the initialised D3D12 device supports `MetalFX`'s temporal denoised scaler.
#[must_use]
pub fn denoised_scaler_supported() -> bool {
    DENOISED_SCALER_SUPPORTED.load(Ordering::Relaxed)
}

/// Probes `device` for `IMTLD3D12DeviceExt1` temporal denoised scaler support.
///
/// Falls back to the last `Init` probe when `device` is null or does not expose
/// `IMTLD3D12DeviceExt1` (such as an `IDXGIAdapter`).
#[must_use]
pub fn query_denoised_supported(device: *mut c_void) -> bool {
    if let Some(supported) = probe_device_ext1(device) {
        DENOISED_SCALER_SUPPORTED.store(supported, Ordering::Relaxed);
        return supported;
    }
    denoised_scaler_supported()
}

/// `ID3D12Object` prefix of `ID3D12GraphicsCommandList` up to `SetPrivateData`.
#[repr(C)]
struct ObjectList12Vtbl {
    unknown: IUnknownVtbl,
    get_private_data: *const c_void,
    set_private_data:
        unsafe extern "system" fn(*mut c_void, *const Guid, u32, *const c_void) -> Hresult,
}

const _: () = {
    assert!(core::mem::offset_of!(ObjectList12Vtbl, set_private_data) == 4 * size_of::<usize>());
    assert!(size_of::<DlssgCameraParams>() == 16);
};

/// Writes `message` to the game's log (stderr), the first time `latch` is reached.
fn once(latch: &AtomicBool, message: std::fmt::Arguments) {
    if !latch.swap(true, Ordering::Relaxed) {
        note(message);
    }
}

/// Which D3D12 NGX feature a [`Feature`] handle represents.
#[derive(PartialEq, Eq)]
enum FeatureKind {
    SuperSampling,
    RayReconstruction,
    FrameGeneration,
}

/// A Direct3D 12 DLSS feature: the shared head first, whose `handle` the game holds the address of.
#[repr(C)]
struct Feature {
    dlss: Dlss,
    kind: FeatureKind,
    min_scale: f32,
    max_scale: f32,
    /// The device's `IMTLD3D12DeviceExt` (or `IMTLD3D12DeviceExt1`), released with the feature.
    ext: *mut c_void,
    scaler: Mutex<Scaler>,
    denoised_scaler: Mutex<DenoisedScaler>,
    camera_params_seen: AtomicBool,
    ui_params_seen: AtomicBool,
    unsupported_reactive_format: AtomicU32,
}

/// The upscaler made for the textures last evaluated, and what it was made for.
struct Scaler {
    scaler: *mut c_void,
    desc: D3D12ScalerDesc,
}

/// The denoised upscaler made for the textures last evaluated, and what it was made for.
struct DenoisedScaler {
    scaler: *mut c_void,
    desc: D3D12DenoisedScalerDesc,
}

impl Drop for Feature {
    fn drop(&mut self) {
        if let Ok(scaler) = self.scaler.get_mut() {
            // SAFETY: the feature holds one reference to each, taken when it made them.
            unsafe { release(scaler.scaler) };
        }
        if let Ok(denoised) = self.denoised_scaler.get_mut() {
            // SAFETY: the feature holds one reference to each, taken when it made them.
            unsafe { release(denoised.scaler) };
        }
        // SAFETY: as above.
        unsafe { release(self.ext) };
    }
}

/// Probes `device` for `IMTLD3D12DeviceExt1::supports_temporal_denoised_scaler`.
fn probe_device_ext1(device: *mut c_void) -> Option<bool> {
    if device.is_null() {
        return None;
    }
    // SAFETY: `device` is a live COM object.
    let ext1 = unsafe { query(device, &IID_IMTL_D3D12_DEVICE_EXT1) }?;
    // SAFETY: `ext1` is DXMT's `IMTLD3D12DeviceExt1`.
    let slots = unsafe { vtbl::<D3D12DeviceExt1Vtbl>(ext1) };
    // SAFETY: `ext1` is live and `supports_temporal_denoised_scaler` takes only `this`.
    let supported = unsafe { (slots.supports_temporal_denoised_scaler)(ext1) } != 0;
    // SAFETY: `query`'s new reference.
    unsafe { release(ext1) };
    Some(supported)
}

/// Initialisation succeeds on a device with `MetalFX`'s temporal upscaler.
fn init_with(device: *mut c_void) -> u32 {
    /// A per-call-site log-once latch.
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if device.is_null() {
        return INVALID_PARAMETER;
    }
    // SAFETY: the game's device, a live COM object.
    let Some(ext) = (unsafe { query(device, &IID_D3D12_DEVICE_EXT) }) else {
        DENOISED_SCALER_SUPPORTED.store(false, Ordering::Relaxed);
        once(
            &LOGGED,
            format_args!(
                "NGX initialised on a device without MetalFX's temporal upscaler: DLSS unavailable"
            ),
        );
        return FEATURE_NOT_SUPPORTED;
    };
    // SAFETY: `query`'s new reference.
    unsafe { release(ext) };
    let denoised = probe_device_ext1(device).unwrap_or(false);
    DENOISED_SCALER_SUPPORTED.store(denoised, Ordering::Relaxed);
    OK
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_Init(
    _: u64,
    _: *const u16,
    device: *mut c_void,
    _: *const c_void,
    _: u32,
) -> u32 {
    init_with(device)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_Init_Ext(
    _: u64,
    _: *const u16,
    device: *mut c_void,
    _: u32,
    _: *const c_void,
) -> u32 {
    init_with(device)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_Init_ProjectID(
    _: *const u8,
    _: u32,
    _: *const u8,
    _: *const u16,
    device: *mut c_void,
    _: u32,
    _: *const c_void,
) -> u32 {
    init_with(device)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_Init_with_ProjectID(
    _: *const u8,
    _: u32,
    _: *const u8,
    _: *const u16,
    device: *mut c_void,
    _: *const c_void,
    _: u32,
) -> u32 {
    init_with(device)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_CreateFeature(
    list: *mut c_void,
    feature: u32,
    params: *mut Parameters,
    out_handle: *mut *mut u32,
) -> u32 {
    /// Feature ids, which outlive every feature: a released one's id is not handed out again.
    static NEXT_ID: AtomicU32 = AtomicU32::new(1);
    /// Per-call-site log-once latches.
    static LOGGED_UNSUPPORTED: AtomicBool = AtomicBool::new(false);
    static LOGGED_FG_DISABLED: AtomicBool = AtomicBool::new(false);
    // SAFETY: the game's map, from AllocateParameters, or null.
    let Some(params) = (unsafe { params.as_ref() }) else {
        return INVALID_PARAMETER;
    };
    if out_handle.is_null() {
        return INVALID_PARAMETER;
    }
    let kind = match feature {
        FEATURE_SUPERSAMPLING => FeatureKind::SuperSampling,
        FEATURE_RAY_RECONSTRUCTION => FeatureKind::RayReconstruction,
        FEATURE_FRAME_GENERATION => {
            if !crate::entry::framegen_enabled() {
                once(
                    &LOGGED_FG_DISABLED,
                    format_args!(
                        "CreateFeature: frame generation requested without MACNEUTRON_FRAMEGEN: refused"
                    ),
                );
                return FEATURE_NOT_SUPPORTED;
            }
            FeatureKind::FrameGeneration
        }
        _ => {
            once(
                &LOGGED_UNSUPPORTED,
                format_args!("CreateFeature: feature {feature} isn't supported: refused"),
            );
            return FEATURE_NOT_SUPPORTED;
        }
    };
    if list.is_null() {
        return INVALID_PARAMETER;
    }
    let mut device = core::ptr::null_mut();
    // SAFETY: the game's command list, a live COM object.
    let slots = unsafe { vtbl::<CommandList12Vtbl>(list) };
    // SAFETY: `device` is a local the call writes.
    if unsafe { (slots.get_device)(list, &raw const IID_ID3D12_DEVICE, &raw mut device) } < 0 {
        return INVALID_PARAMETER;
    }
    let ext = if kind == FeatureKind::RayReconstruction {
        // SAFETY: `GetDevice`'s new reference, released once queried.
        let ext1 = unsafe { query(device, &IID_IMTL_D3D12_DEVICE_EXT1) };
        // SAFETY: as above.
        unsafe { release(device) };
        let Some(ext1) = ext1 else {
            return FEATURE_NOT_SUPPORTED;
        };
        // SAFETY: `ext1` is DXMT's `IMTLD3D12DeviceExt1`.
        let slots = unsafe { vtbl::<D3D12DeviceExt1Vtbl>(ext1) };
        // SAFETY: `ext1` is live and `supports_temporal_denoised_scaler` takes only `this`.
        let supported = unsafe { (slots.supports_temporal_denoised_scaler)(ext1) } != 0;
        if !supported {
            // SAFETY: `query`'s new reference.
            unsafe { release(ext1) };
            return FEATURE_NOT_SUPPORTED;
        }
        ext1
    } else {
        // SAFETY: `GetDevice`'s new reference, released once queried.
        let ext = unsafe { query(device, &IID_D3D12_DEVICE_EXT) };
        // SAFETY: as above.
        unsafe { release(device) };
        let Some(ext) = ext else {
            return FEATURE_NOT_SUPPORTED;
        };
        ext
    };
    let mut made = Box::new(Feature {
        dlss: Dlss {
            handle: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            feature,
            api: 12,
            ..Dlss::default()
        },
        kind,
        min_scale: 1.0,
        max_scale: 3.0,
        ext,
        scaler: Mutex::new(Scaler {
            scaler: core::ptr::null_mut(),
            desc: D3D12ScalerDesc::default(),
        }),
        denoised_scaler: Mutex::new(DenoisedScaler {
            scaler: core::ptr::null_mut(),
            desc: zero_denoised_scaler_desc(),
        }),
        camera_params_seen: AtomicBool::new(false),
        ui_params_seen: AtomicBool::new(false),
        unsupported_reactive_format: AtomicU32::new(DXGI_FORMAT_UNKNOWN),
    });
    // SAFETY: `ext` starts with DXMT's `IMTLD3D12DeviceExt`.
    let slots = unsafe { vtbl::<D3D12DeviceExtVtbl>(ext) };
    let made_ref = &mut *made;
    // SAFETY: the outputs are the feature's fields.
    unsafe {
        (slots.get_temporal_scaler_scale_range)(
            ext,
            &raw mut made_ref.min_scale,
            &raw mut made_ref.max_scale,
        )
    };
    let d = &mut made.dlss;
    d.quality = params.i32(c"PerfQualityValue").unwrap_or(0);
    d.flag = params.i32(c"DLSS.Feature.Create.Flags").unwrap_or(0);
    d.enable_output_subrects = params.i32(c"DLSS.Enable.Output.Subrects").unwrap_or(0);
    if made.kind == FeatureKind::FrameGeneration {
        d.width = params
            .u32(c"Width")
            .or_else(|| params.u32(PARAM_DLSSG_CONTENT_WIDTH))
            .unwrap_or(0);
        d.height = params
            .u32(c"Height")
            .or_else(|| params.u32(PARAM_DLSSG_CONTENT_HEIGHT))
            .unwrap_or(0);
        d.target_width = params.u32(c"OutWidth").unwrap_or(d.width);
        d.target_height = params.u32(c"OutHeight").unwrap_or(d.height);
        note(format_args!(
            "DLSSG frame generation handle {} created",
            d.handle
        ));
    } else {
        let (Some(width), Some(height), Some(target_width), Some(target_height)) = (
            params.u32(c"Width"),
            params.u32(c"Height"),
            params.u32(c"OutWidth"),
            params.u32(c"OutHeight"),
        ) else {
            return INVALID_PARAMETER;
        };
        if width == 0 || height == 0 || target_width == 0 || target_height == 0 {
            return INVALID_PARAMETER;
        }
        (d.width, d.height, d.target_width, d.target_height) =
            (width, height, target_width, target_height);
        let sx = dxmt_util::float_cast::u32_to_f32(target_width)
            / dxmt_util::float_cast::u32_to_f32(width);
        let sy = dxmt_util::float_cast::u32_to_f32(target_height)
            / dxmt_util::float_cast::u32_to_f32(height);
        if made.kind == FeatureKind::SuperSampling
            && (sx > made.max_scale + 0.01 || sy > made.max_scale + 0.01 || sx < 0.99 || sy < 0.99)
        {
            note(format_args!(
                "CreateFeature: {width}x{height} to {target_width}x{target_height} is outside MetalFX's {:.2}x-{:.2}x",
                made.min_scale, made.max_scale
            ));
            return INVALID_PARAMETER;
        }
        if made.kind == FeatureKind::RayReconstruction && (sx < 0.99 || sy < 0.99) {
            note(format_args!(
                "CreateFeature: DLSSD {width}x{height} to {target_width}x{target_height} downscale unsupported"
            ));
            return INVALID_PARAMETER;
        }
        let label = if made.kind == FeatureKind::RayReconstruction {
            "DLSSD"
        } else {
            "DLSS"
        };
        note(format_args!(
            "{label} {width}x{height} to {target_width}x{target_height} (quality {}, flags {}) answered by MetalFX",
            made.dlss.quality,
            hex(made.dlss.flag.cast_unsigned())
        ));
    }
    // SAFETY: the game's out pointer, checked non-null; the handle is the feature's first field.
    unsafe { out_handle.write(Box::into_raw(made).cast::<u32>()) };
    OK
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_ReleaseFeature(handle: *mut u32) -> u32 {
    if handle.is_null() {
        return OK;
    }
    // SAFETY: a handle CreateFeature gave: the address of a feature's head, whose fields say whose it is.
    let head = unsafe { &*handle.cast::<Dlss>() };
    if head.api == 12
        && matches!(
            head.feature,
            FEATURE_SUPERSAMPLING | FEATURE_FRAME_GENERATION | FEATURE_RAY_RECONSTRUCTION
        )
    {
        // SAFETY: a Direct3D 12 feature, boxed by CreateFeature; ReleaseFeature is its last use.
        drop(unsafe { Box::from_raw(handle.cast::<c_void>().cast::<Feature>()) });
    }
    OK
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_EvaluateFeature(
    list: *mut c_void,
    handle: *const u32,
    params: *mut Parameters,
    _: *mut c_void,
) -> u32 {
    // SAFETY: the game's map, or null.
    let Some(params) = (unsafe { params.as_ref() }) else {
        return INVALID_PARAMETER;
    };
    if handle.is_null() {
        return INVALID_PARAMETER;
    }
    // SAFETY: as ReleaseFeature's.
    let head = unsafe { &*handle.cast::<Dlss>() };
    if head.api != 12
        || !matches!(
            head.feature,
            FEATURE_SUPERSAMPLING | FEATURE_FRAME_GENERATION | FEATURE_RAY_RECONSTRUCTION
        )
    {
        return FEATURE_NOT_SUPPORTED;
    }
    // SAFETY: a Direct3D 12 feature, live until ReleaseFeature.
    let feature = unsafe { &*handle.cast::<c_void>().cast::<Feature>() };
    match feature.kind {
        FeatureKind::SuperSampling => {
            let mut scaler = lock(&feature.scaler);
            evaluate(feature, &mut scaler, list, params)
        }
        FeatureKind::RayReconstruction => {
            let mut denoised = lock(&feature.denoised_scaler);
            evaluate_denoised(feature, &mut denoised, list, params)
        }
        FeatureKind::FrameGeneration => evaluate_frame_gen(feature, list, params),
    }
}

/// An `ID3D12Resource` parameter, whichever of NGX's setters the game used for it.
fn resource(params: &Parameters, name: &CStr) -> *mut c_void {
    params
        .d3d12(name)
        .filter(|p| !p.is_null())
        .or_else(|| params.ptr(name))
        .unwrap_or(core::ptr::null_mut())
}

/// First non-null `ID3D12Resource` parameter among `names`.
fn resource_any(params: &Parameters, names: &[&CStr]) -> *mut c_void {
    for name in names {
        let ptr = resource(params, name);
        if !ptr.is_null() {
            return ptr;
        }
    }
    core::ptr::null_mut()
}

/// First present `f32` parameter among `names`, or `fallback`.
fn float_any(params: &Parameters, names: &[&CStr], fallback: f32) -> f32 {
    for name in names {
        if let Some(value) = params.f32(name) {
            return value;
        }
    }
    fallback
}

/// Reads a 4x4 matrix pointer (`[f32; 16]`) from the first non-null pointer among `names`.
fn extract_mat4x4(params: &Parameters, names: &[&CStr]) -> Option<[f32; 16]> {
    for name in names {
        let Some(ptr) = params.ptr(name).filter(|p| !p.is_null()) else {
            continue;
        };
        // SAFETY: NGX matrix parameters point to 16 caller-provided `f32` values (unaligned allowed).
        let mat = unsafe { ptr.cast::<[f32; 16]>().read_unaligned() };
        if mat.iter().all(|v| v.is_finite()) {
            return Some(mat);
        }
    }
    None
}

/// Extracts `WorldToViewMatrix` and `ViewToClipMatrix` if both are present and finite.
fn extract_matrices(
    params: &Parameters,
    w2v_names: &[&CStr],
    v2c_names: &[&CStr],
) -> (i32, [f32; 16], [f32; 16]) {
    if let (Some(w2v), Some(v2c)) = (
        extract_mat4x4(params, w2v_names),
        extract_mat4x4(params, v2c_names),
    ) {
        (1, w2v, v2c)
    } else {
        (0, [0.0; 16], [0.0; 16])
    }
}

/// Zero-initialized `D3D12DenoisedScalerDesc`.
const fn zero_denoised_scaler_desc() -> D3D12DenoisedScalerDesc {
    D3D12DenoisedScalerDesc {
        input_width: 0,
        input_height: 0,
        output_width: 0,
        output_height: 0,
        color_format: 0,
        depth_format: 0,
        motion_format: 0,
        diffuse_albedo_format: 0,
        specular_albedo_format: 0,
        normal_format: 0,
        roughness_format: 0,
        specular_hit_distance_format: 0,
        denoise_strength_mask_format: 0,
        transparency_overlay_format: 0,
        reactive_mask_format: 0,
        output_format: 0,
        synchronous_init: 0,
        auto_exposure: 0,
        reactive_mask_enabled: 0,
        specular_hit_distance_enabled: 0,
        denoise_strength_mask_enabled: 0,
        transparency_overlay_enabled: 0,
    }
}

/// Field-wise equality for `D3D12DenoisedScalerDesc`.
const fn denoised_scaler_desc_eq(a: &D3D12DenoisedScalerDesc, b: &D3D12DenoisedScalerDesc) -> bool {
    a.input_width == b.input_width
        && a.input_height == b.input_height
        && a.output_width == b.output_width
        && a.output_height == b.output_height
        && a.color_format == b.color_format
        && a.depth_format == b.depth_format
        && a.motion_format == b.motion_format
        && a.diffuse_albedo_format == b.diffuse_albedo_format
        && a.specular_albedo_format == b.specular_albedo_format
        && a.normal_format == b.normal_format
        && a.roughness_format == b.roughness_format
        && a.specular_hit_distance_format == b.specular_hit_distance_format
        && a.denoise_strength_mask_format == b.denoise_strength_mask_format
        && a.transparency_overlay_format == b.transparency_overlay_format
        && a.reactive_mask_format == b.reactive_mask_format
        && a.output_format == b.output_format
        && a.synchronous_init == b.synchronous_init
        && a.auto_exposure == b.auto_exposure
        && a.reactive_mask_enabled == b.reactive_mask_enabled
        && a.specular_hit_distance_enabled == b.specular_hit_distance_enabled
        && a.denoise_strength_mask_enabled == b.denoise_strength_mask_enabled
        && a.transparency_overlay_enabled == b.transparency_overlay_enabled
}

/// Whether `params` carries any `DLSSG.*` UI, subrect, content, or camera matrix parameters.
fn has_frame_gen_ui_params(params: &Parameters) -> bool {
    let has_tex = !resource_any(
        params,
        &[
            PARAM_DLSSG_UI,
            PARAM_DLSSG_UI_COLOR_AND_ALPHA,
            PARAM_DLSSG_HUDLESS,
        ],
    )
    .is_null();
    if has_tex {
        return true;
    }
    let has_scalar = [
        PARAM_DLSSG_UI_COMPOSITED,
        PARAM_DLSSG_CONTENT_WIDTH,
        PARAM_DLSSG_CONTENT_HEIGHT,
        PARAM_DLSSG_DEPTH_SUBRECT_BASE_X,
        PARAM_DLSSG_DEPTH_SUBRECT_BASE_Y,
        PARAM_DLSSG_MVEC_SUBRECT_BASE_X,
        PARAM_DLSSG_MVEC_SUBRECT_BASE_Y,
        PARAM_DLSSG_BACKBUFFER_SUBRECT_BASE_X,
        PARAM_DLSSG_BACKBUFFER_SUBRECT_BASE_Y,
    ]
    .iter()
    .any(|name| params.i32(name).is_some() || params.u32(name).is_some());
    if has_scalar {
        return true;
    }
    let (has_matrices, _, _) = extract_matrices(
        params,
        &[PARAM_DLSSG_WORLD_TO_VIEW_MATRIX, PARAM_WORLD_TO_VIEW_MATRIX],
        &[PARAM_DLSSG_VIEW_TO_CLIP_MATRIX, PARAM_VIEW_TO_CLIP_MATRIX],
    );
    has_matrices != 0
}

/// Builds a `D3D12FrameGenUiDesc` from NGX `DLSSG.*` and camera matrix parameters.
fn build_frame_gen_ui_desc(
    params: &Parameters,
    default_width: u32,
    default_height: u32,
) -> D3D12FrameGenUiDesc {
    let ui_texture = resource_any(
        params,
        &[
            PARAM_DLSSG_UI,
            PARAM_DLSSG_UI_COLOR_AND_ALPHA,
            PARAM_DLSSG_HUDLESS,
        ],
    );
    let ui_composited = i32::from(params.i32(PARAM_DLSSG_UI_COMPOSITED).unwrap_or(0) != 0);
    let content_width = params
        .u32(PARAM_DLSSG_CONTENT_WIDTH)
        .filter(|&v| v > 0)
        .unwrap_or(default_width);
    let content_height = params
        .u32(PARAM_DLSSG_CONTENT_HEIGHT)
        .filter(|&v| v > 0)
        .unwrap_or(default_height);
    let depth_offset = [
        params.u32(PARAM_DLSSG_DEPTH_SUBRECT_BASE_X).unwrap_or(0),
        params.u32(PARAM_DLSSG_DEPTH_SUBRECT_BASE_Y).unwrap_or(0),
    ];
    let motion_offset = [
        params.u32(PARAM_DLSSG_MVEC_SUBRECT_BASE_X).unwrap_or(0),
        params.u32(PARAM_DLSSG_MVEC_SUBRECT_BASE_Y).unwrap_or(0),
    ];
    let output_offset = [
        params
            .u32(PARAM_DLSSG_BACKBUFFER_SUBRECT_BASE_X)
            .unwrap_or(0),
        params
            .u32(PARAM_DLSSG_BACKBUFFER_SUBRECT_BASE_Y)
            .unwrap_or(0),
    ];
    let (has_matrices, world_to_view_matrix, view_to_clip_matrix) = extract_matrices(
        params,
        &[PARAM_DLSSG_WORLD_TO_VIEW_MATRIX, PARAM_WORLD_TO_VIEW_MATRIX],
        &[PARAM_DLSSG_VIEW_TO_CLIP_MATRIX, PARAM_VIEW_TO_CLIP_MATRIX],
    );
    D3D12FrameGenUiDesc {
        ui_texture,
        ui_composited,
        content_width,
        content_height,
        depth_offset,
        motion_offset,
        output_offset,
        has_matrices,
        world_to_view_matrix,
        view_to_clip_matrix,
    }
}

/// Forwards `D3D12FrameGenUiDesc` onto `list` via `IMTLD3D12CommandListExt1::set_frame_gen_ui`.
fn apply_frame_gen_ui(
    feature: &Feature,
    list: *mut c_void,
    params: &Parameters,
    has_ui: bool,
) -> Option<Hresult> {
    feature.ui_params_seen.store(has_ui, Ordering::Relaxed);
    let ui_desc = build_frame_gen_ui_desc(params, feature.dlss.width, feature.dlss.height);
    // SAFETY: `list` is a live `ID3D12GraphicsCommandList` COM object.
    let list_ext1 = unsafe { query(list, &IID_IMTL_D3D12_COMMAND_LIST_EXT1) }?;
    // SAFETY: `list_ext1` is DXMT's `IMTLD3D12CommandListExt1`.
    let slots = unsafe { vtbl::<D3D12CommandListExt1Vtbl>(list_ext1) };
    // SAFETY: `list_ext1` is live and `ui_desc` outlives the call.
    let hr = unsafe { (slots.set_frame_gen_ui)(list_ext1, &raw const ui_desc) };
    // SAFETY: `query`'s new reference.
    unsafe { release(list_ext1) };
    Some(hr)
}

/// `resource`'s description.
fn desc(resource: *mut c_void) -> ResourceDesc12 {
    let mut desc = ResourceDesc12::default();
    // SAFETY: a live `ID3D12Resource`.
    let slots = unsafe { vtbl::<Resource12Vtbl>(resource) };
    // SAFETY: GetDesc writes its description through the pointer to the local.
    unsafe { (slots.get_desc)(resource, &raw mut desc) };
    desc
}

/// The width and height of a resource past (x, y): 0 when (x, y) is outside it.
fn extent(desc: &ResourceDesc12, x: u32, y: u32) -> (u32, u32) {
    let width = u32::try_from(desc.width).unwrap_or(u32::MAX);
    (width.saturating_sub(x), desc.height.saturating_sub(y))
}

/// Whether reactive mask forwarding is enabled (`MACNEUTRON_DLSS_REACTIVE != "0"`).
fn reactive_mask_enabled() -> bool {
    static ENABLED: LazyLock<bool> =
        LazyLock::new(|| std::env::var("MACNEUTRON_DLSS_REACTIVE").ok().as_deref() != Some("0"));
    *ENABLED
}

/// Resolves typeless DXGI formats used for reactive masks into concrete typed formats.
const fn resolve_reactive_format(format: u32) -> u32 {
    match format {
        DXGI_FORMAT_R8_TYPELESS => DXGI_FORMAT_R8_UNORM,
        DXGI_FORMAT_R16_TYPELESS => DXGI_FORMAT_R16_UNORM,
        DXGI_FORMAT_R8G8B8A8_TYPELESS => DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_B8G8R8A8_TYPELESS => DXGI_FORMAT_B8G8R8A8_UNORM,
        DXGI_FORMAT_R10G10B10A2_TYPELESS => DXGI_FORMAT_R10G10B10A2_UNORM,
        DXGI_FORMAT_R16G16B16A16_TYPELESS => DXGI_FORMAT_R16G16B16A16_FLOAT,
        DXGI_FORMAT_R32G32B32A32_TYPELESS => DXGI_FORMAT_R32G32B32A32_FLOAT,
        other => other,
    }
}

/// Computes pre-exposure from `DLSS.Pre.Exposure` and `DLSS.Exposure.Scale`.
fn effective_pre_exposure(params: &Parameters) -> f32 {
    let positive = |name: &CStr| params.f32(name).filter(|v| v.is_finite() && *v > 0.0);
    let pre = positive(c"DLSS.Pre.Exposure").unwrap_or(1.0);
    let scale = positive(PARAM_EXPOSURE_SCALE).unwrap_or(1.0);
    let combined = pre * scale;
    if combined.is_finite() && combined > 0.0 {
        combined
    } else {
        1.0
    }
}

/// Extracts valid positive finite `DLSSG.Camera*` parameters, returning `None` if all absent.
fn extract_camera_params(params: &Parameters) -> Option<DlssgCameraParams> {
    let pos = |primary: &CStr, fallback: &CStr| {
        params
            .f32(primary)
            .or_else(|| params.f32(fallback))
            .filter(|v| v.is_finite() && *v > 0.0)
    };
    let near_plane = pos(PARAM_DLSSG_CAMERA_NEAR, c"CameraNear");
    let far_plane = pos(PARAM_DLSSG_CAMERA_FAR, c"CameraFar");
    let field_of_view = pos(PARAM_DLSSG_CAMERA_FOV, c"CameraFOV");
    let aspect_ratio = pos(PARAM_DLSSG_CAMERA_ASPECT, c"CameraAspectRatio");
    if near_plane.is_none()
        && far_plane.is_none()
        && field_of_view.is_none()
        && aspect_ratio.is_none()
    {
        return None;
    }
    Some(DlssgCameraParams {
        near_plane: near_plane.unwrap_or(0.0),
        far_plane: far_plane.unwrap_or(0.0),
        field_of_view: field_of_view.unwrap_or(0.0),
        aspect_ratio: aspect_ratio.unwrap_or(0.0),
    })
}

/// Forwards `DLSSG.Camera*` parameters to `d3d12` via `ID3D12Object::SetPrivateData` on `list`.
fn forward_camera_params(feature: &Feature, list: *mut c_void, params: &Parameters) {
    let guid = DLSSG_CAMERA_GUID;
    if let Some(cam) = extract_camera_params(params) {
        feature.camera_params_seen.store(true, Ordering::Relaxed);
        // SAFETY: `list` is a live `ID3D12GraphicsCommandList` COM object.
        let list_slots = unsafe { vtbl::<ObjectList12Vtbl>(list) };
        let cam_size = u32::try_from(size_of::<DlssgCameraParams>()).unwrap_or(16);
        // SAFETY: `guid` and `cam` are valid for the duration of `SetPrivateData`.
        let _ = unsafe {
            (list_slots.set_private_data)(list, &raw const guid, cam_size, (&raw const cam).cast())
        };
    } else if feature.camera_params_seen.load(Ordering::Relaxed) {
        // SAFETY: `list` is a live `ID3D12GraphicsCommandList` COM object.
        let list_slots = unsafe { vtbl::<ObjectList12Vtbl>(list) };
        // SAFETY: null data erases any prior camera private data on `list`.
        let _ =
            unsafe { (list_slots.set_private_data)(list, &raw const guid, 0, core::ptr::null()) };
    }
}

/// One evaluation: the upscaler for these textures, then DXMT's temporal upscale on `list`.
fn evaluate(feature: &Feature, scaler: &mut Scaler, list: *mut c_void, params: &Parameters) -> u32 {
    /// Per-call-site log-once latches.
    static LOGGED_INPUTS: AtomicBool = AtomicBool::new(false);
    static LOGGED_MISSING: AtomicBool = AtomicBool::new(false);
    static LOGGED_SMALL: AtomicBool = AtomicBool::new(false);
    static LOGGED_SCALER: AtomicBool = AtomicBool::new(false);
    static LOGGED_REACTIVE_DROP: AtomicBool = AtomicBool::new(false);
    static LOGGED_FOREIGN: AtomicBool = AtomicBool::new(false);
    static LOGGED_REFUSED: AtomicBool = AtomicBool::new(false);
    let dlss = &feature.dlss;
    let flag = dlss.flag.cast_unsigned();
    let (color, output) = (resource(params, c"Color"), resource(params, c"Output"));
    let (depth, motion) = (
        resource(params, c"Depth"),
        resource(params, c"MotionVectors"),
    );
    let exposure = if flag & FLAG_AUTO_EXPOSURE == 0 {
        resource(params, c"ExposureTexture")
    } else {
        core::ptr::null_mut()
    };
    if list.is_null() || color.is_null() || output.is_null() || depth.is_null() || motion.is_null()
    {
        once(
            &LOGGED_MISSING,
            format_args!(
                "EvaluateFeature without a texture it needs: list {} colour {} output {} depth {} motion {}",
                pointer(list),
                pointer(color),
                pointer(output),
                pointer(depth),
                pointer(motion)
            ),
        );
        return INVALID_PARAMETER;
    }
    let base = |name: &CStr| params.u32(name).unwrap_or(0);
    let (cx, cy) = (
        base(c"DLSS.Input.Color.Subrect.Base.X"),
        base(c"DLSS.Input.Color.Subrect.Base.Y"),
    );
    let (dx, dy) = (
        base(c"DLSS.Input.Depth.Subrect.Base.X"),
        base(c"DLSS.Input.Depth.Subrect.Base.Y"),
    );
    let (mx, my) = (
        base(c"DLSS.Input.MV.SubrectBase.X"),
        base(c"DLSS.Input.MV.SubrectBase.Y"),
    );
    let (ox, oy) = (
        base(c"DLSS.Output.Subrect.Base.X"),
        base(c"DLSS.Output.Subrect.Base.Y"),
    );
    let mut iw = params
        .u32(c"DLSS.Render.Subrect.Dimensions.Width")
        .unwrap_or(dlss.width);
    let mut ih = params
        .u32(c"DLSS.Render.Subrect.Dimensions.Height")
        .unwrap_or(dlss.height);
    if iw == 0 || ih == 0 {
        (iw, ih) = (dlss.width, dlss.height);
    }
    let reactive_active = params.i32(PARAM_REACTIVE_ACTIVE).unwrap_or(0) != 0;
    let mut reactive_mask = if reactive_active && reactive_mask_enabled() {
        resource(params, PARAM_REACTIVE_MASK)
    } else {
        core::ptr::null_mut()
    };
    let (mut rx, mut ry) = if reactive_mask.is_null() {
        (0, 0)
    } else {
        (base(PARAM_REACTIVE_BASE_X), base(PARAM_REACTIVE_BASE_Y))
    };
    let reactive_desc = if reactive_mask.is_null() {
        None
    } else {
        Some(desc(reactive_mask))
    };
    let mut reactive_format = reactive_desc
        .as_ref()
        .map_or(DXGI_FORMAT_UNKNOWN, |rd| resolve_reactive_format(rd.format));
    if reactive_format == DXGI_FORMAT_UNKNOWN
        || reactive_format == feature.unsupported_reactive_format.load(Ordering::Relaxed)
    {
        reactive_mask = core::ptr::null_mut();
        (rx, ry) = (0, 0);
        reactive_format = DXGI_FORMAT_UNKNOWN;
    }
    let (rw, rh) = if reactive_mask.is_null() {
        (u32::MAX, u32::MAX)
    } else {
        reactive_desc
            .as_ref()
            .map_or((u32::MAX, u32::MAX), |rd| extent(rd, rx, ry))
    };
    let (cd, dd, md, od) = (desc(color), desc(depth), desc(motion), desc(output));
    let ((cw, ch), (dw, dh), (mut mw, mut mh), (ow, oh)) = (
        extent(&cd, cx, cy),
        extent(&dd, dx, dy),
        extent(&md, mx, my),
        extent(&od, ox, oy),
    );
    let (jitter_x, jitter_y) = (
        float_any(
            params,
            &[c"Jitter.Offset.X", PARAM_DLSSG_JITTER_OFFSET_X],
            0.0,
        ),
        float_any(
            params,
            &[c"Jitter.Offset.Y", PARAM_DLSSG_JITTER_OFFSET_Y],
            0.0,
        ),
    );
    let (scale_x, scale_y) = (
        float_any(params, &[c"MV.Scale.X", PARAM_DLSSG_MVEC_SCALE_X], 1.0),
        float_any(params, &[c"MV.Scale.Y", PARAM_DLSSG_MVEC_SCALE_Y], 1.0),
    );
    let pre_exposure = effective_pre_exposure(params);
    let reset = params.i32(c"Reset").unwrap_or(0);
    if !LOGGED_INPUTS.swap(true, Ordering::Relaxed) {
        note(format_args!(
            "flags {} quality {}; render {iw}x{ih} of {}x{}, output {}x{}; formats colour {} depth {} \
             motion {} output {} reactive {}; sizes colour {}x{} depth {}x{} motion {}x{} output {}x{}; \
             bases colour {cx},{cy} depth {dx},{dy} motion {mx},{my} reactive {rx},{ry} output {ox},{oy}; \
             exposure texture {}; jitter {jitter_x:.6},{jitter_y:.6}; motion scale {scale_x:.6},{scale_y:.6}; \
             pre-exposure {pre_exposure:.6}; reset {reset}",
            hex(flag),
            dlss.quality,
            dlss.width,
            dlss.height,
            dlss.target_width,
            dlss.target_height,
            cd.format,
            dd.format,
            md.format,
            od.format,
            reactive_format,
            cd.width,
            cd.height,
            dd.width,
            dd.height,
            md.width,
            md.height,
            od.width,
            od.height,
            pointer(exposure)
        ));
    }

    // The upscaler for these textures: the one there is if they hold the region it reads, else one
    // reading as much of them as they share (no more than the output), so that a game's dynamic
    // resolution moves only the content size.
    let mut want = D3D12ScalerDesc {
        output_width: dlss.target_width,
        output_height: dlss.target_height,
        color_format: cd.format,
        depth_format: dd.format,
        motion_format: md.format,
        output_format: od.format,
        reactive_mask_format: reactive_format,
        auto_exposure: i32::from(flag & FLAG_AUTO_EXPOSURE != 0),
        output_resolution_motion_vectors: i32::from(flag & FLAG_MV_LOWRES == 0),
        jittered_motion_vectors: i32::from(flag & FLAG_MV_JITTERED != 0),
        ..D3D12ScalerDesc::default()
    };
    if ow < want.output_width || oh < want.output_height {
        once(
            &LOGGED_SMALL,
            format_args!(
                "EvaluateFeature: output {}x{} at {ox},{oy} holds no {}x{}",
                od.width, od.height, want.output_width, want.output_height
            ),
        );
        return INVALID_PARAMETER;
    }
    if want.output_resolution_motion_vectors != 0 {
        // An output-size texture, which holds every input pixel's motion.
        (mw, mh) = if mw < want.output_width || mh < want.output_height {
            (0, 0)
        } else {
            (u32::MAX, u32::MAX)
        };
    }
    let tw = cw.min(dw).min(mw).min(rw).min(want.output_width);
    let th = ch.min(dh).min(mh).min(rh).min(want.output_height);
    if tw < iw || th < ih {
        once(
            &LOGGED_SMALL,
            format_args!(
                "EvaluateFeature: render {iw}x{ih} at bases ({cx},{cy}) ({dx},{dy}) ({mx},{my}) isn't in its textures"
            ),
        );
        return INVALID_PARAMETER;
    }
    (want.input_width, want.input_height) = (scaler.desc.input_width, scaler.desc.input_height);
    if scaler.scaler.is_null()
        || want != scaler.desc
        || tw < want.input_width
        || th < want.input_height
        || iw > want.input_width
        || ih > want.input_height
    {
        // SAFETY: the feature's reference to its last upscaler, if any.
        unsafe { release(scaler.scaler) };
        scaler.scaler = core::ptr::null_mut();
        (want.input_width, want.input_height) = (tw, th);
        let ext = feature.ext;
        // SAFETY: `ext` is DXMT's `IMTLD3D12DeviceExt`.
        let slots = unsafe { vtbl::<D3D12DeviceExtVtbl>(ext) };
        // SAFETY: `want` outlives the call; the new upscaler is the feature's reference.
        let mut hr =
            unsafe { (slots.create_temporal_scaler)(ext, &raw const want, &raw mut scaler.scaler) };
        if hr < 0 && want.reactive_mask_format != DXGI_FORMAT_UNKNOWN {
            let dropped = want.reactive_mask_format;
            feature
                .unsupported_reactive_format
                .store(dropped, Ordering::Relaxed);
            once(
                &LOGGED_REACTIVE_DROP,
                format_args!(
                    "EvaluateFeature: reactive mask format {dropped} unsupported by MetalFX, disabling reactive mask"
                ),
            );
            want.reactive_mask_format = DXGI_FORMAT_UNKNOWN;
            reactive_mask = core::ptr::null_mut();
            (rx, ry) = (0, 0);
            // SAFETY: `want` outlives the call; retrying without reactive mask.
            hr = unsafe {
                (slots.create_temporal_scaler)(ext, &raw const want, &raw mut scaler.scaler)
            };
        }
        if hr < 0 {
            scaler.scaler = core::ptr::null_mut();
            once(
                &LOGGED_SCALER,
                format_args!(
                    "EvaluateFeature: no MetalFX upscaler for {tw}x{th} to {}x{}, formats colour {} depth {} motion {} \
                     output {} reactive {}: {}",
                    want.output_width,
                    want.output_height,
                    want.color_format,
                    want.depth_format,
                    want.motion_format,
                    want.output_format,
                    want.reactive_mask_format,
                    hex(hr.cast_unsigned())
                ),
            );
            return FAIL;
        }
        scaler.desc = want;
    }
    // SAFETY: the game's command list, a live COM object.
    let Some(list_ext) = (unsafe { query(list, &IID_D3D12_COMMAND_LIST_EXT) }) else {
        once(
            &LOGGED_FOREIGN,
            format_args!(
                "EvaluateFeature: command list {} isn't DXMT's",
                pointer(list)
            ),
        );
        return INVALID_PARAMETER;
    };
    forward_camera_params(feature, list, params);
    let has_ui = has_frame_gen_ui_params(params);
    if has_ui || feature.ui_params_seen.load(Ordering::Relaxed) {
        let _ = apply_frame_gen_ui(feature, list, params, has_ui);
    }
    let d = D3D12UpscaleDesc {
        color,
        depth,
        motion_vector: motion,
        exposure,
        reactive_mask,
        output,
        input_content_width: iw,
        input_content_height: ih,
        color_offset: [cx, cy],
        depth_offset: [dx, dy],
        motion_offset: [mx, my],
        reactive_offset: [rx, ry],
        output_offset: [ox, oy],
        jitter_offset: [jitter_x, jitter_y],
        motion_vector_scale: [scale_x, scale_y],
        pre_exposure,
        reset: i32::from(reset != 0),
        depth_reversed: i32::from(flag & FLAG_DEPTH_INVERTED != 0),
    };
    // SAFETY: `list_ext` is DXMT's `IMTLD3D12CommandListExt`.
    let slots = unsafe { vtbl::<D3D12CommandListExtVtbl>(list_ext) };
    // SAFETY: `scaler` is the feature's live upscaler; `d` and its textures outlive the call.
    let hr = unsafe { (slots.temporal_upscale)(list_ext, scaler.scaler, &raw const d) };
    // SAFETY: `query`'s new reference.
    unsafe { release(list_ext) };
    if hr < 0 {
        once(
            &LOGGED_REFUSED,
            format_args!(
                "EvaluateFeature: DXMT refused the upscale ({iw}x{ih} to {}x{}): {}",
                dlss.target_width,
                dlss.target_height,
                hex(hr.cast_unsigned())
            ),
        );
        return FAIL;
    }
    OK
}

/// Evaluates `FEATURE_FRAME_GENERATION`: forwards UI/HUD-less, subrect, matrix, and camera data.
fn evaluate_frame_gen(feature: &Feature, list: *mut c_void, params: &Parameters) -> u32 {
    static LOGGED_FOREIGN: AtomicBool = AtomicBool::new(false);
    static LOGGED_REFUSED: AtomicBool = AtomicBool::new(false);
    if list.is_null() {
        return INVALID_PARAMETER;
    }
    forward_camera_params(feature, list, params);
    let has_ui = has_frame_gen_ui_params(params);
    if let Some(hr) = apply_frame_gen_ui(feature, list, params, has_ui) {
        if hr < 0 {
            once(
                &LOGGED_REFUSED,
                format_args!(
                    "EvaluateFeature(FrameGeneration): set_frame_gen_ui failed: {}",
                    hex(hr.cast_unsigned())
                ),
            );
            return FAIL;
        }
        return OK;
    }
    // SAFETY: `list` is a live COM command list.
    let Some(list_ext) = (unsafe { query(list, &IID_D3D12_COMMAND_LIST_EXT) }) else {
        once(
            &LOGGED_FOREIGN,
            format_args!(
                "EvaluateFeature(FrameGeneration): command list {} isn't DXMT's",
                pointer(list)
            ),
        );
        return INVALID_PARAMETER;
    };
    // SAFETY: `query`'s new reference.
    unsafe { release(list_ext) };
    OK
}

/// Evaluates `FEATURE_RAY_RECONSTRUCTION` through `IMTLD3D12DeviceExt1` and command list Ext1.
fn evaluate_denoised(
    feature: &Feature,
    denoised: &mut DenoisedScaler,
    list: *mut c_void,
    params: &Parameters,
) -> u32 {
    static LOGGED_INPUTS: AtomicBool = AtomicBool::new(false);
    static LOGGED_MISSING: AtomicBool = AtomicBool::new(false);
    static LOGGED_SMALL: AtomicBool = AtomicBool::new(false);
    static LOGGED_SCALER: AtomicBool = AtomicBool::new(false);
    static LOGGED_REACTIVE_DROP: AtomicBool = AtomicBool::new(false);
    static LOGGED_FOREIGN: AtomicBool = AtomicBool::new(false);
    static LOGGED_REFUSED: AtomicBool = AtomicBool::new(false);

    let dlss = &feature.dlss;
    let flag = dlss.flag.cast_unsigned();
    let color = resource(params, c"Color");
    let output = resource(params, c"Output");
    let depth = resource(params, c"Depth");
    let motion = resource(params, c"MotionVectors");
    let diffuse_albedo = resource_any(params, &[PARAM_DIFFUSE_ALBEDO, PARAM_DLSSD_DIFFUSE_ALBEDO]);
    let specular_albedo = resource_any(
        params,
        &[PARAM_SPECULAR_ALBEDO, PARAM_DLSSD_SPECULAR_ALBEDO],
    );
    let normal = resource_any(params, &[PARAM_NORMALS, PARAM_DLSSD_NORMALS]);
    let roughness = resource_any(params, &[PARAM_ROUGHNESS, PARAM_DLSSD_ROUGHNESS]);
    if list.is_null()
        || color.is_null()
        || output.is_null()
        || depth.is_null()
        || motion.is_null()
        || diffuse_albedo.is_null()
        || specular_albedo.is_null()
        || normal.is_null()
        || roughness.is_null()
    {
        once(
            &LOGGED_MISSING,
            format_args!(
                "EvaluateFeature(DLSSD) missing required texture: list {} colour {} output {} depth {} \
                 motion {} diffuse {} specular {} normal {} roughness {}",
                pointer(list),
                pointer(color),
                pointer(output),
                pointer(depth),
                pointer(motion),
                pointer(diffuse_albedo),
                pointer(specular_albedo),
                pointer(normal),
                pointer(roughness),
            ),
        );
        return INVALID_PARAMETER;
    }

    let specular_hit_distance = resource_any(
        params,
        &[
            PARAM_SPECULAR_HIT_DISTANCE,
            PARAM_DLSSD_SPECULAR_HIT_DISTANCE,
        ],
    );
    let denoise_strength_mask = resource_any(
        params,
        &[
            PARAM_DENOISE_STRENGTH_MASK,
            PARAM_DLSSD_DENOISE_STRENGTH_MASK,
        ],
    );
    let transparency_overlay = resource_any(
        params,
        &[PARAM_TRANSPARENCY_OVERLAY, PARAM_DLSSD_TRANSPARENCY_OVERLAY],
    );
    let exposure = if flag & FLAG_AUTO_EXPOSURE == 0 {
        resource(params, c"ExposureTexture")
    } else {
        core::ptr::null_mut()
    };

    let mut iw = params
        .u32(c"DLSS.Render.Subrect.Dimensions.Width")
        .unwrap_or(dlss.width);
    let mut ih = params
        .u32(c"DLSS.Render.Subrect.Dimensions.Height")
        .unwrap_or(dlss.height);
    if iw == 0 || ih == 0 {
        (iw, ih) = (dlss.width, dlss.height);
    }

    let reactive_active = params.i32(PARAM_REACTIVE_ACTIVE).unwrap_or(0) != 0;
    let mut reactive_mask = if reactive_active && reactive_mask_enabled() {
        resource(params, PARAM_REACTIVE_MASK)
    } else {
        core::ptr::null_mut()
    };
    let reactive_desc = (!reactive_mask.is_null()).then(|| desc(reactive_mask));
    let mut reactive_format = reactive_desc
        .as_ref()
        .map_or(DXGI_FORMAT_UNKNOWN, |rd| resolve_reactive_format(rd.format));
    if reactive_format == DXGI_FORMAT_UNKNOWN
        || reactive_format == feature.unsupported_reactive_format.load(Ordering::Relaxed)
    {
        reactive_mask = core::ptr::null_mut();
        reactive_format = DXGI_FORMAT_UNKNOWN;
    }
    let (rw, rh) = if reactive_mask.is_null() {
        (u32::MAX, u32::MAX)
    } else {
        reactive_desc
            .as_ref()
            .map_or((u32::MAX, u32::MAX), |rd| extent(rd, 0, 0))
    };

    let color_desc = desc(color);
    let depth_desc = desc(depth);
    let motion_desc = desc(motion);
    let output_desc = desc(output);
    let diffuse_desc = desc(diffuse_albedo);
    let specular_desc = desc(specular_albedo);
    let normal_desc = desc(normal);
    let rough_desc = desc(roughness);
    let hit_desc = (!specular_hit_distance.is_null()).then(|| desc(specular_hit_distance));
    let denoise_desc = (!denoise_strength_mask.is_null()).then(|| desc(denoise_strength_mask));
    let overlay_desc = (!transparency_overlay.is_null()).then(|| desc(transparency_overlay));

    let (cw, ch) = extent(&color_desc, 0, 0);
    let (dw, dh) = extent(&depth_desc, 0, 0);
    let (mut mw, mut mh) = extent(&motion_desc, 0, 0);
    let (ow, oh) = extent(&output_desc, 0, 0);
    let (daw, dah) = extent(&diffuse_desc, 0, 0);
    let (saw, sah) = extent(&specular_desc, 0, 0);
    let (nw, nh) = extent(&normal_desc, 0, 0);
    let (rgh_w, rgh_h) = extent(&rough_desc, 0, 0);

    let (jitter_x, jitter_y) = (
        float_any(
            params,
            &[c"Jitter.Offset.X", PARAM_DLSSG_JITTER_OFFSET_X],
            0.0,
        ),
        float_any(
            params,
            &[c"Jitter.Offset.Y", PARAM_DLSSG_JITTER_OFFSET_Y],
            0.0,
        ),
    );
    let (scale_x, scale_y) = (
        float_any(params, &[c"MV.Scale.X", PARAM_DLSSG_MVEC_SCALE_X], 1.0),
        float_any(params, &[c"MV.Scale.Y", PARAM_DLSSG_MVEC_SCALE_Y], 1.0),
    );
    let pre_exposure = effective_pre_exposure(params);
    let reset = params.i32(c"Reset").unwrap_or(0);
    let (has_matrices, world_to_view_matrix, view_to_clip_matrix) = extract_matrices(
        params,
        &[PARAM_WORLD_TO_VIEW_MATRIX, PARAM_DLSSD_WORLD_TO_VIEW_MATRIX],
        &[PARAM_VIEW_TO_CLIP_MATRIX, PARAM_DLSSD_VIEW_TO_CLIP_MATRIX],
    );

    if !LOGGED_INPUTS.swap(true, Ordering::Relaxed) {
        note(format_args!(
            "DLSSD flags {} quality {}; render {iw}x{ih} of {}x{}, output {}x{}; formats colour {} \
             depth {} motion {} diffuse {} specular {} normal {} roughness {} output {} reactive {}; \
             matrices {}; jitter {jitter_x:.6},{jitter_y:.6}; motion scale {scale_x:.6},{scale_y:.6}; \
             pre-exposure {pre_exposure:.6}; reset {reset}",
            hex(flag),
            dlss.quality,
            dlss.width,
            dlss.height,
            dlss.target_width,
            dlss.target_height,
            color_desc.format,
            depth_desc.format,
            motion_desc.format,
            diffuse_desc.format,
            specular_desc.format,
            normal_desc.format,
            rough_desc.format,
            output_desc.format,
            reactive_format,
            has_matrices,
        ));
    }

    if ow < dlss.target_width || oh < dlss.target_height {
        once(
            &LOGGED_SMALL,
            format_args!(
                "EvaluateFeature(DLSSD): output {}x{} holds no {}x{}",
                output_desc.width, output_desc.height, dlss.target_width, dlss.target_height
            ),
        );
        return INVALID_PARAMETER;
    }
    if flag & FLAG_MV_LOWRES == 0 {
        (mw, mh) = if mw < dlss.target_width || mh < dlss.target_height {
            (0, 0)
        } else {
            (u32::MAX, u32::MAX)
        };
    }
    let tw = cw
        .min(dw)
        .min(mw)
        .min(daw)
        .min(saw)
        .min(nw)
        .min(rgh_w)
        .min(rw)
        .min(dlss.target_width);
    let th = ch
        .min(dh)
        .min(mh)
        .min(dah)
        .min(sah)
        .min(nh)
        .min(rgh_h)
        .min(rh)
        .min(dlss.target_height);
    if tw < iw || th < ih {
        once(
            &LOGGED_SMALL,
            format_args!(
                "EvaluateFeature(DLSSD): render {iw}x{ih} exceeds input textures ({tw}x{th})"
            ),
        );
        return INVALID_PARAMETER;
    }

    let mut want = D3D12DenoisedScalerDesc {
        input_width: denoised.desc.input_width,
        input_height: denoised.desc.input_height,
        output_width: dlss.target_width,
        output_height: dlss.target_height,
        color_format: color_desc.format,
        depth_format: depth_desc.format,
        motion_format: motion_desc.format,
        diffuse_albedo_format: diffuse_desc.format,
        specular_albedo_format: specular_desc.format,
        normal_format: normal_desc.format,
        roughness_format: rough_desc.format,
        specular_hit_distance_format: hit_desc.as_ref().map_or(DXGI_FORMAT_UNKNOWN, |d| d.format),
        denoise_strength_mask_format: denoise_desc
            .as_ref()
            .map_or(DXGI_FORMAT_UNKNOWN, |d| d.format),
        transparency_overlay_format: overlay_desc
            .as_ref()
            .map_or(DXGI_FORMAT_UNKNOWN, |d| d.format),
        reactive_mask_format: reactive_format,
        output_format: output_desc.format,
        synchronous_init: 0,
        auto_exposure: i32::from(flag & FLAG_AUTO_EXPOSURE != 0),
        reactive_mask_enabled: i32::from(!reactive_mask.is_null()),
        specular_hit_distance_enabled: i32::from(!specular_hit_distance.is_null()),
        denoise_strength_mask_enabled: i32::from(!denoise_strength_mask.is_null()),
        transparency_overlay_enabled: i32::from(!transparency_overlay.is_null()),
    };

    if denoised.scaler.is_null()
        || !denoised_scaler_desc_eq(&want, &denoised.desc)
        || tw < want.input_width
        || th < want.input_height
        || iw > want.input_width
        || ih > want.input_height
    {
        // SAFETY: the feature's reference to its last denoised scaler, if any.
        unsafe { release(denoised.scaler) };
        denoised.scaler = core::ptr::null_mut();
        (want.input_width, want.input_height) = (tw, th);
        let ext1 = feature.ext;
        // SAFETY: `ext1` is DXMT's `IMTLD3D12DeviceExt1` for `RayReconstruction`.
        let slots = unsafe { vtbl::<D3D12DeviceExt1Vtbl>(ext1) };
        // SAFETY: `want` outlives the call; the new scaler is the feature's reference.
        let mut hr = unsafe {
            (slots.create_temporal_denoised_scaler)(ext1, &raw const want, &raw mut denoised.scaler)
        };
        if hr < 0 && want.reactive_mask_format != DXGI_FORMAT_UNKNOWN {
            let dropped = want.reactive_mask_format;
            feature
                .unsupported_reactive_format
                .store(dropped, Ordering::Relaxed);
            once(
                &LOGGED_REACTIVE_DROP,
                format_args!(
                    "EvaluateFeature(DLSSD): reactive mask format {dropped} unsupported, disabling reactive mask"
                ),
            );
            want.reactive_mask_format = DXGI_FORMAT_UNKNOWN;
            want.reactive_mask_enabled = 0;
            reactive_mask = core::ptr::null_mut();
            // SAFETY: `want` outlives the call; retrying without reactive mask.
            hr = unsafe {
                (slots.create_temporal_denoised_scaler)(
                    ext1,
                    &raw const want,
                    &raw mut denoised.scaler,
                )
            };
        }
        if hr < 0 {
            denoised.scaler = core::ptr::null_mut();
            once(
                &LOGGED_SCALER,
                format_args!(
                    "EvaluateFeature(DLSSD): no MetalFX denoised scaler for {tw}x{th} to {}x{}: {}",
                    want.output_width,
                    want.output_height,
                    hex(hr.cast_unsigned())
                ),
            );
            return FAIL;
        }
        denoised.desc = want;
    }

    // SAFETY: `list` is a live `ID3D12GraphicsCommandList` COM object.
    let Some(list_ext1) = (unsafe { query(list, &IID_IMTL_D3D12_COMMAND_LIST_EXT1) }) else {
        once(
            &LOGGED_FOREIGN,
            format_args!(
                "EvaluateFeature(DLSSD): command list {} lacks IMTLD3D12CommandListExt1",
                pointer(list)
            ),
        );
        return FEATURE_NOT_SUPPORTED;
    };
    forward_camera_params(feature, list, params);
    let desc = D3D12DenoisedUpscaleDesc {
        color,
        output,
        depth,
        motion_vector: motion,
        diffuse_albedo,
        specular_albedo,
        normal,
        roughness,
        specular_hit_distance,
        denoise_strength_mask,
        transparency_overlay,
        exposure,
        reactive_mask,
        input_content_width: iw,
        input_content_height: ih,
        jitter_offset: [jitter_x, jitter_y],
        motion_vector_scale: [scale_x, scale_y],
        pre_exposure,
        reset: i32::from(reset != 0),
        depth_reversed: i32::from(flag & FLAG_DEPTH_INVERTED != 0),
        has_matrices,
        world_to_view_matrix,
        view_to_clip_matrix,
    };
    // SAFETY: `list_ext1` is DXMT's `IMTLD3D12CommandListExt1`.
    let slots = unsafe { vtbl::<D3D12CommandListExt1Vtbl>(list_ext1) };
    // SAFETY: `denoised.scaler` is live and `desc` plus its textures outlive the call.
    let hr =
        unsafe { (slots.temporal_denoised_upscale)(list_ext1, denoised.scaler, &raw const desc) };
    // SAFETY: `query`'s new reference.
    unsafe { release(list_ext1) };
    if hr < 0 {
        once(
            &LOGGED_REFUSED,
            format_args!(
                "EvaluateFeature(DLSSD): DXMT refused denoised upscale ({iw}x{ih} to {}x{}): {}",
                dlss.target_width,
                dlss.target_height,
                hex(hr.cast_unsigned())
            ),
        );
        return FAIL;
    }
    OK
}

#[cfg(test)]
mod tests;
