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
use objc2_core_foundation::{CFBoolean, CFData, CFDictionary, CFRetained, CFString, CFType};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayCopyAllDisplayModes, CGDisplayMirrorsDisplay, CGDisplayMode,
    CGGetOnlineDisplayList, CGMainDisplayID, kCGDisplayShowDuplicateLowResolutionModes,
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
    float_cast::{f64_to_f32, f64_to_u32_saturating, u64_to_u32},
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
    autoreleasepool(|_| {
        if let Some(screen) = layer_screen(layer) {
            value.maximum_edr_color_component_value = if layer.wantsExtendedDynamicRangeContent() {
                f64_to_f32(screen.max_edr())
            } else {
                1.0
            };
            value.maximum_potential_edr_color_component_value =
                f64_to_f32(screen.max_potential_edr());
        }
    });
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

/// The sizes of the screen under a window, which cap its drawable.
///
/// Published together by the main thread and read together when sizing the drawable, so the pair
/// always names one screen.
pub struct ScreenPixels {
    /// The panel's native pixel size, `None` = unknown.
    pub panel: Option<(u32, u32)>,
    /// The screen's frame in points, `None` = unknown.
    pub frame: Option<(u32, u32)>,
}

impl ScreenPixels {
    /// Nothing known yet: the drawable keeps the layer's own size.
    pub const UNKNOWN: Self = Self {
        panel: None,
        frame: None,
    };

    /// Fold both sizes into one word, 16 bits per dimension; a size that does not fit is unknown.
    #[must_use]
    pub fn pack(&self) -> u64 {
        let half = |size: Option<(u32, u32)>| {
            size.and_then(|(width, height)| {
                Some(
                    (u64::from(u16::try_from(width).ok()?) << 16)
                        | u64::from(u16::try_from(height).ok()?),
                )
            })
            .unwrap_or(0)
        };
        (half(self.panel) << 32) | half(self.frame)
    }

    /// Read back what [`Self::pack`] wrote.
    #[must_use]
    pub fn unpack(bits: u64) -> Self {
        let half = |half: u64| {
            let width = u32::try_from((half >> 16) & 0xffff).unwrap_or(0);
            let height = u32::try_from(half & 0xffff).unwrap_or(0);
            (width != 0 && height != 0).then_some((width, height))
        };
        Self {
            panel: half(bits >> 32),
            frame: half(bits & 0xffff_ffff),
        }
    }
}

/// A screen frame in points, in the pixels of a layer at `scale`; `None` when unknown or empty.
#[must_use]
pub fn backing_pixels(frame: Option<(u32, u32)>, scale: f64) -> Option<(u32, u32)> {
    let (width, height) = frame?;
    let pixels = (
        f64_to_u32_saturating((f64::from(width) * scale).round()),
        f64_to_u32_saturating((f64::from(height) * scale).round()),
    );
    (pixels.0 != 0 && pixels.1 != 0).then_some(pixels)
}

/// Folds `(panel, backing)` into one word (16 bits per dimension, panel high).
#[must_use]
pub fn pack_screen_sizes(panel: Option<(u32, u32)>, backing: Option<(u32, u32)>) -> u64 {
    let half = |size: Option<(u32, u32)>| {
        size.and_then(|(width, height)| {
            let width = u16::try_from(width).ok().filter(|&w| w != 0)?;
            let height = u16::try_from(height).ok().filter(|&h| h != 0)?;
            Some((u64::from(width) << 16) | u64::from(height))
        })
        .unwrap_or(0)
    };
    (half(panel) << 32) | half(backing)
}

/// Whether the display scales the backing store down to the panel, in either axis.
///
/// That is when the drawable can be capped below the layer's size and the compositor magnifies it
/// back, so the layer's magnification filter is linear then and nearest otherwise.
#[must_use]
pub const fn scaled_down_to_panel(backing: Option<(u32, u32)>, panel: Option<(u32, u32)>) -> bool {
    matches!(
        (backing, panel),
        (Some((backing_w, backing_h)), Some((panel_w, panel_h)))
            if backing_w > panel_w || backing_h > panel_h
    )
}

/// The drawable size for a layer whose own size is `natural`, capped at the panel's pixels.
///
/// In a scaled display mode the screen's backing store has more pixels than the panel, and the
/// display scales every composited frame down to it. The cap scales `natural` per axis by panel
/// over backing, so a layer covering the whole screen comes out at exactly the panel size, and
/// the compositor takes the rest of the way. It applies only when it shrinks the drawable and the
/// source still fits in the capped size. Any unknown input leaves `natural`, and nothing is ever
/// enlarged.
#[must_use]
pub fn panel_drawable_size(
    natural: (u32, u32),
    backing: Option<(u32, u32)>,
    panel: Option<(u32, u32)>,
    source: Option<(u32, u32)>,
) -> (u32, u32) {
    let (Some((backing_w, backing_h)), Some((panel_w, panel_h)), Some((source_w, source_h))) =
        (backing, panel, source)
    else {
        return natural;
    };
    if backing_w == 0 || backing_h == 0 {
        return natural;
    }
    let scale = |size: u32, num: u32, den: u32| {
        let scaled =
            (u128::from(size) * u128::from(num) * 2 + u128::from(den)) / (u128::from(den) * 2);
        u32::try_from(scaled.min(u128::from(size)))
            .unwrap_or(size)
            .max(1)
    };
    let (natural_w, natural_h) = natural;
    let capped_w = scale(natural_w, panel_w, backing_w).min(natural_w);
    let capped_h = scale(natural_h, panel_h, backing_h).min(natural_h);
    let shrinks = natural_w > capped_w || natural_h > capped_h;
    let source_fits = source_w <= capped_w && source_h <= capped_h;
    if shrinks && source_fits {
        (capped_w, capped_h)
    } else {
        natural
    }
}

/// The native pixel size of `display`, or `(0, 0)` when unavailable.
///
/// A display flags one native mode per refresh rate, and with the duplicate low-resolution modes
/// listed also the scaled modes whose pixels are the panel's, so the largest flagged pixel size
/// is taken. Pure CoreGraphics, safe to call on any thread.
#[must_use]
pub fn native_panel_pixels(display: u32) -> (u32, u32) {
    native_panel_pixels_opt(display).unwrap_or((0, 0))
}

fn native_panel_pixels_opt(display: u32) -> Option<(u32, u32)> {
    /// `kDisplayModeNativeFlag` (`IOKit` `IOGraphicsTypes.h`), which no `objc2` crate binds.
    const DISPLAY_MODE_NATIVE_FLAG: u32 = 0x0200_0000;

    // SAFETY: `kCGDisplayShowDuplicateLowResolutionModes` is a static CoreGraphics string key.
    let key = unsafe { kCGDisplayShowDuplicateLowResolutionModes };
    let options = CFDictionary::<CFString, CFBoolean>::from_slices(&[key], &[CFBoolean::new(true)]);
    // SAFETY: `options` maps `key` to `CFBoolean`, as CoreGraphics documents for display modes.
    let modes = unsafe { CGDisplayCopyAllDisplayModes(display, Some(options.as_ref())) }?;
    // SAFETY: CoreGraphics documents every element of the returned array as a `CGDisplayMode`.
    let modes = unsafe { modes.cast_unchecked::<CGDisplayMode>() };
    modes
        .iter()
        .filter(|mode| CGDisplayMode::io_flags(Some(mode)) & DISPLAY_MODE_NATIVE_FLAG != 0)
        .filter_map(|mode| {
            let width = u32::try_from(CGDisplayMode::pixel_width(Some(&mode))).ok()?;
            let height = u32::try_from(CGDisplayMode::pixel_height(Some(&mode))).ok()?;
            (width != 0 && height != 0).then_some((width, height))
        })
        .max_by_key(|&(width, height)| u64::from(width) * u64::from(height))
}
