//! NGX's Direct3D 11 feature entry points, on DXMT's private `IMTLD3D11ContextExt`.
//!
//! The upscale is the immediate context's: DXMT's context makes `MetalFX`'s temporal upscaler
//! for the textures each evaluation passes.

use crate::com::{
    ContextExt11Vtbl, FEATURE_METALFX_TEMPORAL_SCALER, IID_CONTEXT_EXT_11, IID_CONTEXT_EXT1_11, UpscaleDesc11, query,
    release, vtbl,
};
use crate::ngx::{
    Dlss, FAIL, FEATURE_NOT_SUPPORTED, FEATURE_SUPERSAMPLING, FLAG_AUTO_EXPOSURE, FLAG_DEPTH_INVERTED, FLAG_MV_LOWRES,
    INVALID_PARAMETER, OK,
};
use crate::parameter::Parameters;
use core::ffi::{CStr, c_void};

/// Releases a COM reference when it goes out of scope.
struct Held(*mut c_void);

impl Drop for Held {
    fn drop(&mut self) {
        // SAFETY: the reference `query` gave.
        unsafe { release(self.0) };
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D11_CreateFeature(
    context: *mut c_void,
    feature: u32,
    params: *mut Parameters,
    out_handle: *mut *mut u32,
) -> u32 {
    // SAFETY: the game's map, from AllocateParameters, or null.
    let (Some(params), false) = (unsafe { params.as_ref() }, out_handle.is_null()) else { return INVALID_PARAMETER };
    // SAFETY: the game's immediate context, a live COM object.
    let Some(ext) = (unsafe { query(context, &IID_CONTEXT_EXT1_11) }) else { return INVALID_PARAMETER };
    let ext = Held(ext);
    if feature != FEATURE_SUPERSAMPLING {
        crate::log::err(&format!("NVSDK_NGX_D3D11_CreateFeature: feature {feature} not handled"));
        return FAIL;
    }
    let mut supported = 0_i32;
    // SAFETY: `ext` is DXMT's `IMTLD3D11ContextExt1`.
    let slots = unsafe { vtbl::<ContextExt11Vtbl>(ext.0) };
    // SAFETY: the answer is a BOOL, written to the local.
    let hr = unsafe {
        (slots.check_feature_support)(ext.0, FEATURE_METALFX_TEMPORAL_SCALER, (&raw mut supported).cast(), 4)
    };
    if hr < 0 || supported == 0 {
        return FEATURE_NOT_SUPPORTED;
    }
    let (Some(width), Some(height), Some(target_width), Some(target_height)) =
        (params.u32(c"Width"), params.u32(c"Height"), params.u32(c"OutWidth"), params.u32(c"OutHeight"))
    else {
        return INVALID_PARAMETER;
    };
    let dlss = Box::new(Dlss {
        handle: 0,
        feature,
        api: 11,
        width,
        height,
        target_width,
        target_height,
        quality: params.i32(c"PerfQualityValue").unwrap_or(0),
        flag: params.i32(c"DLSS.Feature.Create.Flags").unwrap_or(0),
        enable_output_subrects: params.i32(c"DLSS.Enable.Output.Subrects").unwrap_or(0),
    });
    // SAFETY: the game's out pointer, checked non-null; the handle is the head's first field.
    unsafe { out_handle.write(Box::into_raw(dlss).cast::<u32>()) };
    OK
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D11_ReleaseFeature(handle: *mut u32) -> u32 {
    if handle.is_null() {
        return OK;
    }
    // SAFETY: a handle CreateFeature gave: the address of a feature's head.
    let head = unsafe { &*handle.cast::<Dlss>() };
    if head.api == 11 && head.feature == FEATURE_SUPERSAMPLING {
        // SAFETY: a Direct3D 11 feature, boxed by CreateFeature; ReleaseFeature is its last use.
        drop(unsafe { Box::from_raw(handle.cast::<Dlss>()) });
    }
    OK
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D11_EvaluateFeature(
    context: *mut c_void,
    handle: *const u32,
    params: *mut Parameters,
    _: *mut c_void,
) -> u32 {
    // SAFETY: the game's map, or null.
    let (Some(params), false) = (unsafe { params.as_ref() }, handle.is_null()) else { return INVALID_PARAMETER };
    // SAFETY: a handle CreateFeature gave: its head says what it is.
    let dlss = unsafe { &*handle.cast::<Dlss>() };
    if dlss.feature != FEATURE_SUPERSAMPLING {
        return FAIL;
    }
    // SAFETY: the game's immediate context, a live COM object.
    let Some(ext) = (unsafe { query(context, &IID_CONTEXT_EXT_11) }) else { return INVALID_PARAMETER };
    let ext = Held(ext);
    let texture = |name: &CStr| params.d3d11(name);
    let (Some(color), Some(output), Some(depth), Some(motion)) =
        (texture(c"Color"), texture(c"Output"), texture(c"Depth"), texture(c"MotionVectors"))
    else {
        return INVALID_PARAMETER;
    };
    let (Some(width), Some(height)) =
        (params.u32(c"DLSS.Render.Subrect.Dimensions.Width"), params.u32(c"DLSS.Render.Subrect.Dimensions.Height"))
    else {
        return INVALID_PARAMETER;
    };
    let flag = dlss.flag.cast_unsigned();
    let float = |name: &CStr, fallback: f32| params.f32(name).unwrap_or(fallback);
    let desc = UpscaleDesc11 {
        input_content_width: width,
        input_content_height: height,
        auto_exposure: i32::from(flag & FLAG_AUTO_EXPOSURE != 0),
        in_reset: params.i32(c"Reset").unwrap_or(0),
        depth_reversed: i32::from(flag & FLAG_DEPTH_INVERTED != 0),
        motion_vector_in_display_res: i32::from(flag & FLAG_MV_LOWRES == 0),
        color,
        depth,
        motion_vector: motion,
        output,
        motion_vector_scale: [float(c"MV.Scale.X", 1.0), float(c"MV.Scale.Y", 1.0)],
        pre_exposure: float(c"DLSS.Pre.Exposure", 1.0),
        exposure_texture: texture(c"ExposureTexture").unwrap_or(core::ptr::null_mut()),
        jitter_offset: [float(c"Jitter.Offset.X", 0.0), float(c"Jitter.Offset.Y", 0.0)],
    };
    // SAFETY: `ext` is DXMT's `IMTLD3D11ContextExt`.
    let slots = unsafe { vtbl::<ContextExt11Vtbl>(ext.0) };
    // SAFETY: the description and its textures outlive the call.
    unsafe { (slots.temporal_upscale)(ext.0, &raw const desc) };
    OK
}
