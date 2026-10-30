//! Displays: their IDs, colour gamut and EDR values, and the HDR setting kept for each.
//!
//! The gamut comes from `ColorSync` (raw FFI: no binding crate covers it), the EDR values from
//! the display's `NSScreen`, read on the calling thread ([`super::appkit`]).

use core::{ffi::c_void, ptr};
use std::sync::{Mutex, PoisonError};

use objc2::{
    ClassType,
    rc::{Retained, autoreleasepool},
    runtime::NSObjectProtocol,
};
use objc2_core_foundation::{CFData, CFRetained, CFString, CFType};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayMirrorsDisplay, CGGetOnlineDisplayList, CGMainDisplayID,
    kCGNullDirectDisplay,
};
use objc2_foundation::ns_string;
use objc2_quartz_core::CAMetalLayer;
use winemetal_wire::{
    UnixcallGenericObjPtrNoret, UnixcallGenericObjRet, UnixcallQueryDisplaySetting,
    UnixcallQueryDisplaySettingForLayer, UnixcallUpdateDisplaySetting, WMTDisplayDescription,
    WMTEDRValue, WMTHDRMetadata,
};

use super::appkit::{Screen, View};
use crate::{
    ffi::{AbortOnPanic, STATUS_SUCCESS, mem_mut, mem_ref, params},
    float_cast::{f64_to_f32, u64_to_u32},
    handle::obj,
};

// SAFETY: ColorSync's declarations. A profile is a CF object; the `Create` and `Copy` functions
// return +1 or null, which `Option<CFRetained<_>>` (a pointer, null as `None`) takes over.
unsafe extern "C" {
    safe fn ColorSyncProfileCreateWithDisplayID(display: u32) -> Option<CFRetained<CFType>>;
    safe fn ColorSyncProfileCreateWithName(name: &CFString) -> Option<CFRetained<CFType>>;
    safe fn ColorSyncProfileCopyTag(profile: &CFType, tag: &CFString)
    -> Option<CFRetained<CFData>>;
    safe static kColorSyncGenericRGBProfile: &'static CFString;
    safe static kColorSyncSigMediaWhitePointTag: &'static CFString;
    safe static kColorSyncSigRedColorantTag: &'static CFString;
    safe static kColorSyncSigGreenColorantTag: &'static CFString;
    safe static kColorSyncSigBlueColorantTag: &'static CFString;
}

/// An ICC `XYZ ` record's chromaticity: x and y of its big-endian s15.16 X, Y and Z.
///
/// `None` unless the record is 20 bytes with the `XYZ ` signature.
#[must_use]
pub fn xy(record: &[u8]) -> Option<[f32; 2]> {
    let ([sig, _, x, y, z], []) = record.as_chunks::<4>() else {
        return None;
    };
    if u32::from_ne_bytes(*sig) != 0x205a_5958 {
        return None;
    }
    let [x, y, z] = [x, y, z].map(|v| f64::from(i32::from_be_bytes(*v)) / 65536.0);
    Some([f64_to_f32(x / (x + y + z)), f64_to_f32(y / (x + y + z))])
}

/// Fills the white point and primaries from `profile`, in the C's order, up to a missing tag.
fn gamut(profile: &CFType, desc: &mut WMTDisplayDescription) -> bool {
    [
        (kColorSyncSigMediaWhitePointTag, &mut desc.white_points),
        (kColorSyncSigRedColorantTag, &mut desc.red_primaries),
        (kColorSyncSigGreenColorantTag, &mut desc.green_primaries),
        (kColorSyncSigBlueColorantTag, &mut desc.blue_primaries),
    ]
    .into_iter()
    .all(|(tag, out)| {
        let Some(v) = ColorSyncProfileCopyTag(profile, tag).and_then(|d| xy(&d.to_vec())) else {
            return false;
        };
        *out = v;
        true
    })
}

/// A screen's display ID: its `NSScreenNumber`.
fn screen_number(screen: &Screen) -> u32 {
    let description = screen.device_description();
    description
        .objectForKey(ns_string!("NSScreenNumber"))
        .map_or(0, |n| n.unsignedIntValue())
}

/// The screen a layer shows on: its delegate's window's, when the delegate is a view.
///
/// The C's sends: the delegate, its class, the window, then its screen twice.
fn layer_screen(layer: &CAMetalLayer) -> Option<Retained<Screen>> {
    let delegate = layer.delegate()?;
    if !delegate.isKindOfClass(View::class()) {
        return None;
    }
    // SAFETY: an `NSView`, as it just answered.
    let view = unsafe { Retained::cast_unchecked::<View>(delegate) };
    view.window()?;
    view.window()?.screen()?;
    view.window()?.screen()
}

/// Slot 94, `_WMTGetPrimaryDisplayId`: `CGMainDisplayID`.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_ret`.
#[unsafe(export_name = "_WMTGetPrimaryDisplayId")]
pub unsafe extern "C-unwind" fn primary_display_id(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjRet>(args) };
    p.ret = CGMainDisplayID().into();
    STATUS_SUCCESS
}

/// Slot 95, `_WMTGetSecondaryDisplayId`: the first online display neither main nor a mirror.
///
/// 0 when there is none.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_ret`.
#[unsafe(export_name = "_WMTGetSecondaryDisplayId")]
pub unsafe extern "C-unwind" fn secondary_display_id(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjRet>(args) };
    p.ret = kCGNullDirectDisplay.into();
    let mut count = 0;
    // SAFETY: a count, no list.
    unsafe { CGGetOnlineDisplayList(0, ptr::null_mut(), &raw mut count) };
    if count == 0 {
        return STATUS_SUCCESS;
    }
    let main = CGMainDisplayID();
    let mut displays: Vec<CGDirectDisplayID> = vec![0; count as usize];
    // SAFETY: room for `count` displays.
    unsafe { CGGetOnlineDisplayList(count, displays.as_mut_ptr(), &raw mut count) };
    displays.truncate(count as usize);
    if let Some(&id) = displays
        .iter()
        .find(|&&id| id != main && CGDisplayMirrorsDisplay(id) == kCGNullDirectDisplay)
    {
        p.ret = id.into();
    }
    STATUS_SUCCESS
}

/// Slot 96, `_WMTGetDisplayDescription`: a display's gamut and EDR values.
///
/// The gamut is the display's `ColorSync` profile's, else the generic RGB profile's; the EDR values
/// are its screen's, else 1, 0 (reference) and 1. A null description pointer writes nothing (the C
/// wrote through it).
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_ptr_noret`: the display ID in its handle, and
/// null or a `WMTDisplayDescription`.
#[unsafe(export_name = "_WMTGetDisplayDescription")]
pub unsafe extern "C-unwind" fn display_description(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjPtrNoret>(args) };
    let id = u64_to_u32(p.handle);
    // SAFETY: as above.
    let Some(desc) = (unsafe { mem_mut::<WMTDisplayDescription>(p.arg) }) else {
        return STATUS_SUCCESS;
    };
    let profile = ColorSyncProfileCreateWithDisplayID(id);
    if !profile
        .as_deref()
        .is_some_and(|profile| gamut(profile, desc))
        && let Some(fallback) = ColorSyncProfileCreateWithName(kColorSyncGenericRGBProfile)
    {
        gamut(&fallback, desc);
    }
    autoreleasepool(|_| {
        let screen = Screen::screens().iter().find(|s| screen_number(s) == id);
        if let Some(screen) = screen {
            desc.maximum_edr_color_component_value = f64_to_f32(screen.max_edr());
            desc.maximum_reference_edr_color_component_value =
                f64_to_f32(screen.max_reference_edr());
            desc.maximum_potential_edr_color_component_value =
                f64_to_f32(screen.max_potential_edr());
        } else {
            desc.maximum_edr_color_component_value = 1.0;
            desc.maximum_reference_edr_color_component_value = 0.0;
            desc.maximum_potential_edr_color_component_value = 1.0;
        }
    });
    STATUS_SUCCESS
}

/// Slot 97, `_MetalLayer_getEDRValue`: the EDR values of the screen a layer shows on.
///
/// 1 and 1 for a layer on no screen; the maximum is 1 unless the layer wants EDR content. A null
/// value pointer writes nothing (the C wrote through it).
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_ptr_noret`: a live layer or 0, and null or a
/// `WMTEDRValue`.
#[unsafe(export_name = "_MetalLayer_getEDRValue")]
pub unsafe extern "C-unwind" fn layer_edr_value(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjPtrNoret>(args) };
    // SAFETY: as above.
    let Some(value) = (unsafe { mem_mut::<WMTEDRValue>(p.arg) }) else {
        return STATUS_SUCCESS;
    };
    value.maximum_edr_color_component_value = 1.0;
    value.maximum_potential_edr_color_component_value = 1.0;
    // SAFETY: as above.
    let Some(layer) = (unsafe { obj::<CAMetalLayer>(p.handle) }) else {
        return STATUS_SUCCESS;
    };
    let Some(screen) = layer_screen(layer) else {
        return STATUS_SUCCESS;
    };
    value.maximum_edr_color_component_value = if layer.wantsExtendedDynamicRangeContent() {
        f64_to_f32(screen.max_edr())
    } else {
        1.0
    };
    value.maximum_potential_edr_color_component_value = f64_to_f32(screen.max_potential_edr());
    STATUS_SUCCESS
}

/// A display's HDR setting: version 0 is unset, each update counts one more.
struct DisplaySetting {
    version: u64,
    colorspace: u64,
    hdr_metadata: WMTHDRMetadata,
}

/// Runs `f` on the setting of `display`: the main display's, or the one every other ID shares.
fn with_setting<R>(display: u32, f: impl FnOnce(&mut DisplaySetting) -> R) -> R {
    const UNSET: DisplaySetting = DisplaySetting {
        version: 0,
        colorspace: 0,
        hdr_metadata: WMTHDRMetadata {
            red_primary: [0; 2],
            green_primary: [0; 2],
            blue_primary: [0; 2],
            white_point: [0; 2],
            max_mastering_luminance: 0,
            min_mastering_luminance: 0,
            max_content_light_level: 0,
            max_frame_average_light_level: 0,
        },
    };
    /// The settings of the other displays and of the main one.
    ///
    /// The resource is process-wide: the displays of the machine, whichever device presents.
    static SETTINGS: Mutex<[DisplaySetting; 2]> = Mutex::new([UNSET, UNSET]);
    let mut settings = SETTINGS.lock().unwrap_or_else(PoisonError::into_inner);
    let [other, main] = &mut *settings;
    f(if display == CGMainDisplayID() {
        main
    } else {
        other
    })
}

/// Slot 99, `_WMTQueryDisplaySetting`: a display's HDR setting, `ret` 1 when one is set.
///
/// A null metadata pointer with a setting writes `ret` 0 only (the C wrote through it).
///
/// # Safety
///
/// `args` is the caller's `unixcall_query_display_setting`, its metadata null or a
/// `WMTHDRMetadata`.
#[unsafe(export_name = "_WMTQueryDisplaySetting")]
pub unsafe extern "C-unwind" fn query_display_setting(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallQueryDisplaySetting>(args) };
    // SAFETY: as above.
    let value = unsafe { mem_mut::<WMTHDRMetadata>(p.hdr_metadata) };
    p.ret = 0;
    with_setting(u64_to_u32(p.display_id), |s| {
        if s.version == 0 {
            return;
        }
        let Some(value) = value else {
            return;
        };
        *value = s.hdr_metadata;
        p.colorspace = s.colorspace;
        p.ret = 1;
    });
    STATUS_SUCCESS
}

/// Slot 100, `_WMTUpdateDisplaySetting`: sets a display's HDR setting; no metadata unsets it.
///
/// # Safety
///
/// `args` is the caller's `unixcall_update_display_setting`, its metadata null or a
/// `WMTHDRMetadata`.
#[unsafe(export_name = "_WMTUpdateDisplaySetting")]
pub unsafe extern "C-unwind" fn update_display_setting(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallUpdateDisplaySetting>(args) };
    // SAFETY: as above.
    let value = unsafe { mem_ref::<WMTHDRMetadata>(p.hdr_metadata) };
    with_setting(u64_to_u32(p.display_id), |s| {
        if let Some(value) = value {
            s.hdr_metadata = *value;
            s.colorspace = p.colorspace;
            s.version = s.version.wrapping_add(1);
        } else {
            s.version = 0;
        }
    });
    STATUS_SUCCESS
}

/// Slot 101, `_WMTQueryDisplaySettingForLayer`: the setting and EDR values of a layer's screen.
///
/// Version 0, nothing else written, for a layer on no screen; runs on every present. A null
/// metadata pointer writes the version 0 only (the C wrote through it).
///
/// # Safety
///
/// `args` is the caller's `unixcall_query_display_setting_for_layer`: a live layer or 0, and null
/// or a `WMTHDRMetadata`.
#[unsafe(export_name = "_WMTQueryDisplaySettingForLayer")]
pub unsafe extern "C-unwind" fn query_display_setting_for_layer(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallQueryDisplaySettingForLayer>(args) };
    // SAFETY: as above.
    let out = unsafe { mem_mut::<WMTHDRMetadata>(p.hdr_metadata) };
    // SAFETY: as above.
    let layer = unsafe { obj::<CAMetalLayer>(p.layer) };
    autoreleasepool(|_| {
        p.version = 0;
        let Some(layer) = layer else {
            return;
        };
        let Some(screen) = layer_screen(layer) else {
            return;
        };
        let id = screen_number(&screen);
        let Some(out) = out else {
            return;
        };
        with_setting(id, |s| {
            *out = s.hdr_metadata;
            p.version = s.version;
            p.colorspace = s.colorspace;
        });
        p.edr_value.maximum_edr_color_component_value = if layer.wantsExtendedDynamicRangeContent()
        {
            f64_to_f32(screen.max_edr())
        } else {
            1.0
        };
        p.edr_value.maximum_potential_edr_color_component_value =
            f64_to_f32(screen.max_potential_edr());
    });
    STATUS_SUCCESS
}
