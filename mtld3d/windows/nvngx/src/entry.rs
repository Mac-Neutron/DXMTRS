//! NGX's entry points both APIs share, and the capability map.
//!
//! The capability map is what NGX's SDK reads before it creates a feature: DLSS available, no
//! driver update needed, and the callback that picks a quality mode's render size.

use crate::float_cast::{f32_to_u32, u32_to_f32};
use crate::ngx::{
    FAIL, FEATURE_NOT_SUPPORTED, FEATURE_SUPERSAMPLING, FeatureDiscoveryInfo, FeatureRequirement, INVALID_PARAMETER,
    OK, SUPPORTED, UNSUPPORTED,
};
use crate::parameter::{Parameters, Value};
use core::ffi::c_void;
use std::sync::LazyLock;

/// `DLSSOptimalSettingsCallback`: a quality mode's render size, scale and dynamic range.
///
/// From `Width`, `Height` and `PerfQualityValue`, with DXMT's scales for each mode.
extern "C" fn optimal_settings(params: *mut Parameters) -> u32 {
    // SAFETY: NGX's SDK calls this with the capability map it got from us.
    let Some(params) = (unsafe { params.as_mut() }) else { return FAIL };
    let (Some(width), Some(height), Some(quality)) =
        (params.u32(c"Width"), params.u32(c"Height"), params.i32(c"PerfQualityValue"))
    else {
        return FAIL;
    };
    let scale: f32 = match quality {
        3 => 1.0 / 3.0,
        0 => 0.5,
        2 => 1.0 / 1.5,
        4 => 1.0 / 1.3,
        5 => 1.0,
        _ => 58.0 / 100.0,
    };
    params.set(c"Scale", Value::F32(scale));
    params.set(c"SuperSampling.ScaleFactor", Value::F32(scale));
    params.set(c"OutWidth", Value::U32(f32_to_u32((u32_to_f32(width) * scale).ceil())));
    params.set(c"OutHeight", Value::U32(f32_to_u32((u32_to_f32(height) * scale).ceil())));
    params.set(c"Sharpness", Value::F32(0.0));
    params.set(c"DLSS.Get.Dynamic.Min.Render.Width", Value::U32(width / 3));
    params.set(c"DLSS.Get.Dynamic.Min.Render.Height", Value::U32(height / 3));
    params.set(c"DLSS.Get.Dynamic.Max.Render.Width", Value::U32(width));
    params.set(c"DLSS.Get.Dynamic.Max.Render.Height", Value::U32(height));
    params.set(c"DLSSMode", Value::I32(1));
    for preset in [
        c"DLSS.Hint.Render.Preset.DLAA",
        c"DLSS.Hint.Render.Preset.UltraQuality",
        c"DLSS.Hint.Render.Preset.Quality",
        c"DLSS.Hint.Render.Preset.Balanced",
        c"DLSS.Hint.Render.Preset.Performance",
        c"DLSS.Hint.Render.Preset.UltraPerformance",
    ] {
        params.set(preset, Value::U32(0));
    }
    OK
}

/// `DLSSGetStatsCallback`: no memory of its own.
extern "C" fn stats(params: *mut Parameters) -> u32 {
    // SAFETY: as `optimal_settings`.
    if let Some(params) = unsafe { params.as_mut() } {
        params.set(c"SizeInBytes", Value::U64(0));
    }
    OK
}

/// Fills `params` with the capability map.
pub fn populate_capabilities(params: &mut Parameters) {
    params.set(c"SuperSampling.Available", Value::U32(1));
    params.set(c"#\x01", Value::U32(1));
    for name in [
        c"SuperSampling.MinDriverVersionMajor",
        c"SuperSampling.MinDriverVersionMinor",
        c"SuperSampling.NeedsUpdatedDriver",
        c"Snippet.OptLevel",
        c"Snippet.IsDevBranch",
        c"RTXValue",
    ] {
        params.set(name, Value::I32(0));
    }
    params.set(c"SuperSampling.FeatureInitResult", Value::U32(1));
    let callback: extern "C" fn(*mut Parameters) -> u32 = optimal_settings;
    params.set(c"DLSSOptimalSettingsCallback", Value::Ptr(callback as *mut c_void));
    let callback: extern "C" fn(*mut Parameters) -> u32 = stats;
    params.set(c"DLSSGetStatsCallback", Value::Ptr(callback as *mut c_void));
    for (name, value) in [
        (c"Sharpness", 0.0),
        (c"MV.Scale.X", 1.0),
        (c"MV.Scale.Y", 1.0),
        (c"MV.Offset.X", 0.0),
        (c"MV.Offset.Y", 0.0),
        (c"DLSS.Exposure.Scale", 1.0),
    ] {
        params.set(name, Value::F32(value));
    }
    params.set(c"PerfQualityValue", Value::I32(2));
    for name in [c"CreationNodeMask", c"VisibilityNodeMask", c"DLSS.Enable.Output.Subrects"] {
        params.set(name, Value::I32(1));
    }
}

/// A new capability map.
fn new_capabilities() -> *mut Parameters {
    let params = Parameters::allocate();
    // SAFETY: `allocate` returns a live map.
    populate_capabilities(unsafe { &mut *params });
    params
}

/// Writes a new map (`capabilities`: filled with the capability map) to `out`.
fn give(out: *mut *mut Parameters, capabilities: bool) -> u32 {
    if out.is_null() {
        return INVALID_PARAMETER;
    }
    let params = if capabilities { new_capabilities() } else { Parameters::allocate() };
    // SAFETY: the game's out pointer, checked non-null.
    unsafe { out.write(params) };
    OK
}

/// Answers whether `info`'s feature is DLSS super resolution, and its requirements.
fn requirements(info: *const FeatureDiscoveryInfo, requirement: *mut FeatureRequirement, other: u32) -> u32 {
    // SAFETY: the game's pointers, each null or a live structure.
    let (Some(info), Some(requirement)) = (unsafe { info.as_ref() }, unsafe { requirement.as_mut() }) else {
        return INVALID_PARAMETER;
    };
    if info.feature_id == FEATURE_SUPERSAMPLING {
        requirement.feature_supported = SUPPORTED;
        requirement.min_hw_architecture = 0;
        let version = b"10.0.16299.0\0";
        requirement.min_os_version[..version.len()].copy_from_slice(version);
        return OK;
    }
    if other == FAIL {
        crate::log::warn(&format!("NVSDK_NGX_D3D11_GetFeatureRequirements: unsupported feature {}", info.feature_id));
    }
    requirement.feature_supported = UNSUPPORTED;
    other
}

/// The deprecated map NGX's SDK manages for Direct3D 12: one per process, never freed.
static SHARED_12: LazyLock<usize> = LazyLock::new(|| new_capabilities() as usize);

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_GetCapabilityParameters(out: *mut *mut Parameters) -> u32 {
    give(out, true)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D11_GetCapabilityParameters(out: *mut *mut Parameters) -> u32 {
    give(out, true)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_GetParameters(out: *mut *mut Parameters) -> u32 {
    if out.is_null() {
        return INVALID_PARAMETER;
    }
    // SAFETY: the game's out pointer, checked non-null.
    unsafe { out.write(*SHARED_12 as *mut Parameters) };
    OK
}

/// A new capability map each call, as DXMT's Direct3D 11 entry point gives it.
#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D11_GetParameters(out: *mut *mut Parameters) -> u32 {
    give(out, true)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_AllocateParameters(out: *mut *mut Parameters) -> u32 {
    give(out, false)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D11_AllocateParameters(out: *mut *mut Parameters) -> u32 {
    give(out, false)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_PopulateParameters_Impl(params: *mut Parameters) -> u32 {
    // SAFETY: the game's map, from AllocateParameters.
    let Some(params) = (unsafe { params.as_mut() }) else { return INVALID_PARAMETER };
    populate_capabilities(params);
    OK
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_DestroyParameters(params: *mut Parameters) -> u32 {
    Parameters::destroy(params);
    OK
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D11_DestroyParameters(params: *mut Parameters) -> u32 {
    Parameters::destroy(params);
    OK
}

/// No scratch memory: `MetalFX` holds its own.
#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_D3D12_GetScratchBufferSize(_: u32, _: *const Parameters, out: *mut usize) -> u32 {
    if !out.is_null() {
        // SAFETY: the game's out pointer, checked non-null.
        unsafe { out.write(0) };
    }
    OK
}

#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_D3D11_GetScratchBufferSize(feature: u32, params: *const Parameters, out: *mut usize) -> u32 {
    NVSDK_NGX_D3D12_GetScratchBufferSize(feature, params, out)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_GetFeatureRequirements(
    _: *mut c_void,
    info: *const FeatureDiscoveryInfo,
    requirement: *mut FeatureRequirement,
) -> u32 {
    requirements(info, requirement, FEATURE_NOT_SUPPORTED)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D11_GetFeatureRequirements(
    _: *mut c_void,
    info: *const FeatureDiscoveryInfo,
    requirement: *mut FeatureRequirement,
) -> u32 {
    requirements(info, requirement, FAIL)
}

#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_UpdateFeature(_: *const c_void, _: u32) -> u32 {
    OK
}

#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_D3D12_Shutdown() -> u32 {
    OK
}

#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_D3D12_Shutdown1(_: *mut c_void) -> u32 {
    OK
}

#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_D3D11_Shutdown() -> u32 {
    OK
}

#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_D3D11_Shutdown1(_: *mut c_void) -> u32 {
    OK
}

/// Direct3D 11's initialisations all succeed: the device is checked when a feature is made.
#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_D3D11_Init(_: u64, _: *const u16, _: *mut c_void, _: *const c_void, _: u32) -> u32 {
    OK
}

#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_D3D11_Init_Ext(_: u64, _: *const u16, _: *mut c_void, _: u32, _: *const c_void) -> u32 {
    OK
}

#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_D3D11_Init_ProjectID(
    _: *const u8,
    _: u32,
    _: *const u8,
    _: *const u16,
    _: *mut c_void,
    _: u32,
    _: *const c_void,
) -> u32 {
    OK
}

#[unsafe(no_mangle)]
pub const extern "system" fn NVSDK_NGX_D3D11_Init_with_ProjectID(
    _: *const u8,
    _: u32,
    _: *const u8,
    _: *const u16,
    _: *mut c_void,
    _: *const c_void,
    _: u32,
) -> u32 {
    OK
}
