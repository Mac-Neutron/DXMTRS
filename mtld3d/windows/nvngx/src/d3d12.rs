//! NGX's Direct3D 12 entry points, on DXMT's private interface to `MetalFX`'s temporal upscaler.
//!
//! A DLSS feature's upscaler is made at its first evaluation, from the formats of the textures the
//! game really passes (creation names only sizes and flags), and made again when they change.
//! NGX's conventions are `MetalFX`'s: motion vectors point from the current frame to the previous
//! one, in pixels after `MV.Scale`; the jitter is the sample's offset in input pixels. The first
//! evaluation logs what the game passed to the game's log (stderr), once, before any pixel is
//! trusted.

use crate::com::{
    CommandList12Vtbl, CommandListExt12Vtbl, DeviceExt12Vtbl, IID_COMMAND_LIST_EXT_12, IID_DEVICE_EXT_12,
    IID_ID3D12_DEVICE, Resource12Vtbl, ResourceDesc12, ScalerDesc12, UpscaleDesc12, query, release, vtbl,
};
use crate::ngx::{
    Dlss, FAIL, FEATURE_NOT_SUPPORTED, FEATURE_SUPERSAMPLING, FLAG_AUTO_EXPOSURE, FLAG_DEPTH_INVERTED,
    FLAG_MV_JITTERED, FLAG_MV_LOWRES, INVALID_PARAMETER, OK, hex, note, pointer,
};
use crate::parameter::Parameters;
use core::ffi::{CStr, c_void};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

/// Writes `message` to the game's log (stderr), the first time `latch` is reached.
fn once(latch: &AtomicBool, message: std::fmt::Arguments) {
    if !latch.swap(true, Ordering::Relaxed) {
        note(message);
    }
}

/// A Direct3D 12 DLSS feature: the shared head first, whose `handle` the game holds the address of.
#[repr(C)]
struct Feature {
    dlss: Dlss,
    min_scale: f32,
    max_scale: f32,
    /// The device's `IMTLD3D12DeviceExt`, released with the feature.
    ext: *mut c_void,
    scaler: Mutex<Scaler>,
}

/// The upscaler made for the textures last evaluated, and what it was made for.
struct Scaler {
    scaler: *mut c_void,
    desc: ScalerDesc12,
}

impl Drop for Feature {
    fn drop(&mut self) {
        if let Ok(scaler) = self.scaler.get_mut() {
            // SAFETY: the feature holds one reference to each, taken when it made them.
            unsafe { release(scaler.scaler) };
        }
        // SAFETY: as above.
        unsafe { release(self.ext) };
    }
}

/// Initialisation succeeds on a device with `MetalFX`'s temporal upscaler.
fn init_with(device: *mut c_void) -> u32 {
    /// A per-call-site log-once latch.
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if device.is_null() {
        return INVALID_PARAMETER;
    }
    // SAFETY: the game's device, a live COM object.
    let Some(ext) = (unsafe { query(device, &IID_DEVICE_EXT_12) }) else {
        once(&LOGGED, format_args!("NGX initialised on a device without MetalFX's temporal upscaler: DLSS unavailable"));
        return FEATURE_NOT_SUPPORTED;
    };
    // SAFETY: `query`'s new reference.
    unsafe { release(ext) };
    OK
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_Init(_: u64, _: *const u16, device: *mut c_void, _: *const c_void, _: u32) -> u32 {
    init_with(device)
}

#[unsafe(no_mangle)]
pub extern "system" fn NVSDK_NGX_D3D12_Init_Ext(_: u64, _: *const u16, device: *mut c_void, _: u32, _: *const c_void) -> u32 {
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
    /// A per-call-site log-once latch.
    static LOGGED: AtomicBool = AtomicBool::new(false);
    // SAFETY: the game's map, from AllocateParameters, or null.
    let Some(params) = (unsafe { params.as_ref() }) else { return INVALID_PARAMETER };
    if out_handle.is_null() {
        return INVALID_PARAMETER;
    }
    if feature != FEATURE_SUPERSAMPLING {
        once(&LOGGED, format_args!("CreateFeature: feature {feature} isn't DLSS super resolution: refused"));
        return FEATURE_NOT_SUPPORTED;
    }
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
    // SAFETY: GetDevice's new reference, released once queried.
    let ext = unsafe { query(device, &IID_DEVICE_EXT_12) };
    // SAFETY: as above.
    unsafe { release(device) };
    let Some(ext) = ext else { return FEATURE_NOT_SUPPORTED };
    let mut made = Box::new(Feature {
        dlss: Dlss { handle: NEXT_ID.fetch_add(1, Ordering::Relaxed), feature, api: 12, ..Dlss::default() },
        min_scale: 1.0,
        max_scale: 3.0,
        ext,
        scaler: Mutex::new(Scaler { scaler: core::ptr::null_mut(), desc: ScalerDesc12::default() }),
    });
    // SAFETY: `ext` is DXMT's `IMTLD3D12DeviceExt`.
    let slots = unsafe { vtbl::<DeviceExt12Vtbl>(ext) };
    let made_ref = &mut *made;
    // SAFETY: the outputs are the feature's fields.
    unsafe { (slots.get_temporal_scaler_scale_range)(ext, &raw mut made_ref.min_scale, &raw mut made_ref.max_scale) };
    let d = &mut made.dlss;
    let (Some(width), Some(height), Some(target_width), Some(target_height)) =
        (params.u32(c"Width"), params.u32(c"Height"), params.u32(c"OutWidth"), params.u32(c"OutHeight"))
    else {
        return INVALID_PARAMETER;
    };
    if width == 0 || height == 0 || target_width == 0 || target_height == 0 {
        return INVALID_PARAMETER;
    }
    (d.width, d.height, d.target_width, d.target_height) = (width, height, target_width, target_height);
    d.quality = params.i32(c"PerfQualityValue").unwrap_or(0);
    d.flag = params.i32(c"DLSS.Feature.Create.Flags").unwrap_or(0);
    d.enable_output_subrects = params.i32(c"DLSS.Enable.Output.Subrects").unwrap_or(0);
    let sx = crate::float_cast::u32_to_f32(target_width) / crate::float_cast::u32_to_f32(width);
    let sy = crate::float_cast::u32_to_f32(target_height) / crate::float_cast::u32_to_f32(height);
    if sx > made.max_scale + 0.01 || sy > made.max_scale + 0.01 || sx < 0.99 || sy < 0.99 {
        note(format_args!(
            "CreateFeature: {width}x{height} to {target_width}x{target_height} is outside MetalFX's {:.2}x-{:.2}x",
            made.min_scale, made.max_scale
        ));
        return INVALID_PARAMETER;
    }
    note(format_args!(
        "DLSS {width}x{height} to {target_width}x{target_height} (quality {}, flags {}) answered by MetalFX",
        made.dlss.quality,
        hex(made.dlss.flag.cast_unsigned())
    ));
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
    if head.feature == FEATURE_SUPERSAMPLING && head.api == 12 {
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
    let Some(params) = (unsafe { params.as_ref() }) else { return INVALID_PARAMETER };
    if handle.is_null() {
        return INVALID_PARAMETER;
    }
    // SAFETY: as ReleaseFeature's.
    let head = unsafe { &*handle.cast::<Dlss>() };
    if head.feature != FEATURE_SUPERSAMPLING || head.api != 12 {
        return FEATURE_NOT_SUPPORTED;
    }
    // SAFETY: a Direct3D 12 feature, live until ReleaseFeature.
    let feature = unsafe { &*handle.cast::<c_void>().cast::<Feature>() };
    let Ok(mut scaler) = feature.scaler.lock() else { return FAIL };
    evaluate(feature, &mut scaler, list, params)
}

/// An `ID3D12Resource` parameter, whichever of NGX's setters the game used for it.
fn resource(params: &Parameters, name: &CStr) -> *mut c_void {
    params.d3d12(name).filter(|p| !p.is_null()).or_else(|| params.ptr(name)).unwrap_or(core::ptr::null_mut())
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

/// One evaluation: the upscaler for these textures, then DXMT's temporal upscale on `list`.
fn evaluate(feature: &Feature, scaler: &mut Scaler, list: *mut c_void, params: &Parameters) -> u32 {
    /// Per-call-site log-once latches.
    static LOGGED_INPUTS: AtomicBool = AtomicBool::new(false);
    static LOGGED_MISSING: AtomicBool = AtomicBool::new(false);
    static LOGGED_SMALL: AtomicBool = AtomicBool::new(false);
    static LOGGED_SCALER: AtomicBool = AtomicBool::new(false);
    static LOGGED_FOREIGN: AtomicBool = AtomicBool::new(false);
    static LOGGED_REFUSED: AtomicBool = AtomicBool::new(false);
    let dlss = &feature.dlss;
    let flag = dlss.flag.cast_unsigned();
    let (color, output) = (resource(params, c"Color"), resource(params, c"Output"));
    let (depth, motion) = (resource(params, c"Depth"), resource(params, c"MotionVectors"));
    let exposure =
        if flag & FLAG_AUTO_EXPOSURE == 0 { resource(params, c"ExposureTexture") } else { core::ptr::null_mut() };
    if list.is_null() || color.is_null() || output.is_null() || depth.is_null() || motion.is_null() {
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
    let (cx, cy) = (base(c"DLSS.Input.Color.Subrect.Base.X"), base(c"DLSS.Input.Color.Subrect.Base.Y"));
    let (dx, dy) = (base(c"DLSS.Input.Depth.Subrect.Base.X"), base(c"DLSS.Input.Depth.Subrect.Base.Y"));
    let (mx, my) = (base(c"DLSS.Input.MV.SubrectBase.X"), base(c"DLSS.Input.MV.SubrectBase.Y"));
    let (ox, oy) = (base(c"DLSS.Output.Subrect.Base.X"), base(c"DLSS.Output.Subrect.Base.Y"));
    let mut iw = params.u32(c"DLSS.Render.Subrect.Dimensions.Width").unwrap_or(dlss.width);
    let mut ih = params.u32(c"DLSS.Render.Subrect.Dimensions.Height").unwrap_or(dlss.height);
    if iw == 0 || ih == 0 {
        (iw, ih) = (dlss.width, dlss.height);
    }
    let (cd, dd, md, od) = (desc(color), desc(depth), desc(motion), desc(output));
    let ((cw, ch), (dw, dh), (mut mw, mut mh), (ow, oh)) =
        (extent(&cd, cx, cy), extent(&dd, dx, dy), extent(&md, mx, my), extent(&od, ox, oy));
    let float = |name: &CStr, fallback: f32| params.f32(name).unwrap_or(fallback);
    let (jitter_x, jitter_y) = (float(c"Jitter.Offset.X", 0.0), float(c"Jitter.Offset.Y", 0.0));
    let (scale_x, scale_y) = (float(c"MV.Scale.X", 1.0), float(c"MV.Scale.Y", 1.0));
    let pre_exposure = float(c"DLSS.Pre.Exposure", 1.0);
    let reset = params.i32(c"Reset").unwrap_or(0);
    if !LOGGED_INPUTS.swap(true, Ordering::Relaxed) {
        note(format_args!(
            "flags {} quality {}; render {iw}x{ih} of {}x{}, output {}x{}; formats colour {} depth {} \
             motion {} output {}; sizes colour {}x{} depth {}x{} motion {}x{} output {}x{}; bases colour {cx},{cy} \
             depth {dx},{dy} motion {mx},{my} output {ox},{oy}; exposure texture {}; jitter {jitter_x:.6},{jitter_y:.6}; \
             motion scale {scale_x:.6},{scale_y:.6}; pre-exposure {pre_exposure:.6}; reset {reset}",
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
    let mut want = ScalerDesc12 {
        output_width: dlss.target_width,
        output_height: dlss.target_height,
        color_format: cd.format,
        depth_format: dd.format,
        motion_format: md.format,
        output_format: od.format,
        auto_exposure: i32::from(flag & FLAG_AUTO_EXPOSURE != 0),
        output_resolution_motion_vectors: i32::from(flag & FLAG_MV_LOWRES == 0),
        jittered_motion_vectors: i32::from(flag & FLAG_MV_JITTERED != 0),
        ..ScalerDesc12::default()
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
        (mw, mh) = if mw < want.output_width || mh < want.output_height { (0, 0) } else { (u32::MAX, u32::MAX) };
    }
    let tw = cw.min(dw).min(mw).min(want.output_width);
    let th = ch.min(dh).min(mh).min(want.output_height);
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
        let slots = unsafe { vtbl::<DeviceExt12Vtbl>(ext) };
        // SAFETY: `want` outlives the call; the new upscaler is the feature's reference.
        let hr = unsafe { (slots.create_temporal_scaler)(ext, &raw const want, &raw mut scaler.scaler) };
        if hr < 0 {
            scaler.scaler = core::ptr::null_mut();
            once(
                &LOGGED_SCALER,
                format_args!(
                    "EvaluateFeature: no MetalFX upscaler for {tw}x{th} to {}x{}, formats colour {} depth {} motion {} \
                     output {}: {}",
                    want.output_width,
                    want.output_height,
                    want.color_format,
                    want.depth_format,
                    want.motion_format,
                    want.output_format,
                    hex(hr.cast_unsigned())
                ),
            );
            return FAIL;
        }
        scaler.desc = want;
    }
    // SAFETY: the game's command list, a live COM object.
    let Some(list_ext) = (unsafe { query(list, &IID_COMMAND_LIST_EXT_12) }) else {
        once(&LOGGED_FOREIGN, format_args!("EvaluateFeature: command list {} isn't DXMT's", pointer(list)));
        return INVALID_PARAMETER;
    };
    let d = UpscaleDesc12 {
        color,
        depth,
        motion_vector: motion,
        exposure,
        reactive_mask: core::ptr::null_mut(),
        output,
        input_content_width: iw,
        input_content_height: ih,
        color_offset: [cx, cy],
        depth_offset: [dx, dy],
        motion_offset: [mx, my],
        reactive_offset: [0, 0],
        output_offset: [ox, oy],
        jitter_offset: [jitter_x, jitter_y],
        motion_vector_scale: [scale_x, scale_y],
        pre_exposure: if pre_exposure > 0.0 { pre_exposure } else { 1.0 },
        reset: i32::from(reset != 0),
        depth_reversed: i32::from(flag & FLAG_DEPTH_INVERTED != 0),
    };
    // SAFETY: `list_ext` is DXMT's `IMTLD3D12CommandListExt`.
    let slots = unsafe { vtbl::<CommandListExt12Vtbl>(list_ext) };
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
