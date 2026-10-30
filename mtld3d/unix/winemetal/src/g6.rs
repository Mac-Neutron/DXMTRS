//! Metal layers and views, the developer HUD, displays and HDR.
//!
//! A layer's property and colour-space sets run on the main thread, as the C's did: at once when
//! the caller is there, else through the main queue while the caller waits. The display reads
//! (`display`) run on the calling thread.

use core::ffi::{CStr, c_void};
use core::mem::transmute;
use core::ptr;
#[cfg(feature = "native")]
use core::sync::atomic::{AtomicPtr, Ordering};

use dispatch2::DispatchQueue;
use objc2::runtime::{AnyClass, AnyObject, NSObject, ProtocolObject};
use objc2::{extern_class, extern_methods};
use objc2_core_foundation::{CFString, CGSize};
use objc2_core_graphics::{
    CGColorSpace, kCGColorSpaceExtendedLinearSRGB, kCGColorSpaceITUR_2020_sRGBGamma,
    kCGColorSpaceITUR_2100_PQ, kCGColorSpaceSRGB,
};
use objc2_foundation::{NSString, NSThread, ns_string};
use objc2_metal::MTLDevice;
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use winemetal_wire::{
    UnixcallCreateMetalViewFromHwnd, UnixcallGenericObjConstptrNoret, UnixcallGenericObjNoret,
    UnixcallGenericObjObjNoret, UnixcallGenericObjObjObjNoret, UnixcallGenericObjObjObjUint64Ret,
    UnixcallGenericObjObjRet, UnixcallGenericObjPtrNoret, UnixcallGenericObjRet,
    UnixcallGenericObjUint64Ret, UnixcallGenericObjUint64Uint64Ret, WMT_COLOR_SPACE_BT2020,
    WMT_COLOR_SPACE_HDR_PQ, WMT_COLOR_SPACE_HDR_SC_RGB, WMT_COLOR_SPACE_SRGB,
    WMT_COLOR_SPACE_SRGB_LINEAR, WMTLayerProps,
};

use crate::ffi::{AbortOnPanic, STATUS_SUCCESS, mem_mut, mem_ref, params};
use crate::float_cast::{u64_to_usize, usize_to_u32};
use crate::format::pixel_format;
use crate::handle::{lookup_class, obj, ret_passthrough};

pub mod appkit;
pub mod display;

extern_class!(
    /// `MTLHUDService`, re-declared for its private metric selectors.
    #[unsafe(super(NSObject))]
    #[name = "MTLHUDService"]
    struct HudService;
);

impl HudService {
    extern_methods!(
        /// `-insertMetric:after:name:unit:nameColor:valueColor:visualType:options:`.
        // allow: the C's private 8-argument selector in objc2's selector shape
        // (CONVENTIONS exception 4's class).
        #[allow(clippy::too_many_arguments)]
        #[unsafe(method(insertMetric:after:name:unit:nameColor:valueColor:visualType:options:))]
        #[unsafe(method_family = none)]
        fn insert_metric(
            &self,
            metric: Option<&AnyObject>,
            after: Option<&AnyObject>,
            name: &NSString,
            unit: &NSString,
            name_color: u32,
            value_color: u32,
            visual_type: u32,
            options: u64,
        ) -> bool;

        /// `-updateLabel:value:`.
        #[unsafe(method(updateLabel:value:))]
        #[unsafe(method_family = none)]
        fn update_label(&self, label: Option<&AnyObject>, value: Option<&AnyObject>);

        /// `-remove:`.
        #[unsafe(method(remove:))]
        #[unsafe(method_family = none)]
        fn remove(&self, label: Option<&AnyObject>);
    );
}

/// The HUD's class object, as `objc_lookUpClass` finds it.
trait HudClass {
    /// `+instance`, the shared HUD, as a raw pointer (+0, autoreleased).
    fn instance(&self) -> *mut AnyObject;
}

impl HudClass for AnyClass {
    extern_methods!(
        #[unsafe(method(instance))]
        #[unsafe(method_family = none)]
        fn instance(&self) -> *mut AnyObject;
    );
}

extern_class!(
    /// `CAMetalLayer`, re-declared for its +0 results as raw pointers.
    #[unsafe(super(NSObject))]
    #[name = "CAMetalLayer"]
    struct RawLayer;
);

impl RawLayer {
    extern_methods!(
        /// `-nextDrawable`, autoreleased.
        #[unsafe(method(nextDrawable))]
        #[unsafe(method_family = none)]
        fn next_drawable(&self) -> *mut AnyObject;

        /// `-device`, the layer's.
        #[unsafe(method(device))]
        #[unsafe(method_family = none)]
        fn raw_device(&self) -> *mut AnyObject;
    );
}

/// `CAMetalDrawable`'s texture as a raw pointer (+0, the drawable's).
trait RawDrawable {
    /// `-texture`.
    fn raw_texture(&self) -> *mut AnyObject;
}

impl RawDrawable for ProtocolObject<dyn CAMetalDrawable> {
    extern_methods!(
        #[unsafe(method(texture))]
        #[unsafe(method_family = none)]
        fn raw_texture(&self) -> *mut AnyObject;
    );
}

/// Runs `work` on the main thread and waits for it (the C's `execute_on_main`).
///
/// At once when the caller is there, else through the main queue.
fn on_main(work: impl FnOnce() + Send) {
    if NSThread::isMainThread_class() {
        work();
    } else {
        DispatchQueue::main().exec_sync(work);
    }
}

/// Slot 62, `_DeveloperHUDProperties_instance`: `MTLHUDService`'s instance, 0 without the class.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_ret`.
#[unsafe(export_name = "_DeveloperHUDProperties_instance")]
pub unsafe extern "C-unwind" fn hud_instance(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjRet>(args) };
    p.ret = lookup_class(c"MTLHUDService").map_or(0, |c| ret_passthrough(c.instance()));
    STATUS_SUCCESS
}

/// Slot 63, `_DeveloperHUDProperties_addLabel`: a metric after another, in white, as text.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_obj_obj_uint64_ret`, its handles live objects
/// or 0.
#[unsafe(export_name = "_DeveloperHUDProperties_addLabel")]
pub unsafe extern "C-unwind" fn hud_add_label(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjObjUint64Ret>(args) };
    // SAFETY: as above.
    let hud = unsafe { obj::<HudService>(p.handle) };
    // SAFETY: as above.
    let metric = unsafe { obj::<AnyObject>(p.arg0) };
    // SAFETY: as above.
    let after = unsafe { obj::<AnyObject>(p.arg1) };
    let (name, unit) = (ns_string!(""), ns_string!(""));
    let added = hud.is_some_and(|h| {
        h.insert_metric(metric, after, name, unit, u32::MAX, u32::MAX, 1, 0)
    });
    p.ret = added.into();
    STATUS_SUCCESS
}

/// Slot 64, `_DeveloperHUDProperties_updateLabel`.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_obj_obj_noret`, its handles live objects or 0.
#[unsafe(export_name = "_DeveloperHUDProperties_updateLabel")]
pub unsafe extern "C-unwind" fn hud_update_label(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjObjNoret>(args) };
    // SAFETY: as above.
    let hud = unsafe { obj::<HudService>(p.handle) };
    // SAFETY: as above.
    let label = unsafe { obj::<AnyObject>(p.arg0) };
    // SAFETY: as above.
    let value = unsafe { obj::<AnyObject>(p.arg1) };
    if let Some(hud) = hud {
        hud.update_label(label, value);
    }
    STATUS_SUCCESS
}

/// Slot 65, `_DeveloperHUDProperties_remove`.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_obj_noret`, its handles live objects or 0.
#[unsafe(export_name = "_DeveloperHUDProperties_remove")]
pub unsafe extern "C-unwind" fn hud_remove(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjNoret>(args) };
    // SAFETY: as above.
    let hud = unsafe { obj::<HudService>(p.handle) };
    // SAFETY: as above.
    let label = unsafe { obj::<AnyObject>(p.arg) };
    if let Some(hud) = hud {
        hud.remove(label);
    }
    STATUS_SUCCESS
}

/// Slot 66, `_MetalDrawable_texture`: the drawable's texture, +0.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_obj_ret`, its handle a live object or 0.
#[unsafe(export_name = "_MetalDrawable_texture")]
pub unsafe extern "C-unwind" fn drawable_texture(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: as above.
    let drawable = unsafe { obj::<ProtocolObject<dyn CAMetalDrawable>>(p.handle) };
    p.ret = drawable.map_or(0, |d| ret_passthrough(d.raw_texture()));
    STATUS_SUCCESS
}

/// Slot 67, `_MetalLayer_nextDrawable`: the layer's next drawable, +0 (autoreleased); may block.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_obj_ret`, its handle a live object or 0.
#[unsafe(export_name = "_MetalLayer_nextDrawable")]
pub unsafe extern "C-unwind" fn layer_next_drawable(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: as above.
    let layer = unsafe { obj::<RawLayer>(p.handle) };
    p.ret = layer.map_or(0, |l| ret_passthrough(l.next_drawable()));
    STATUS_SUCCESS
}

/// Slot 70, `_MetalLayer_setProps`: the layer's properties, set on the main thread.
///
/// A null props pointer sends nothing (the C read through it).
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_constptr_noret`: a live layer or 0, and null or a
/// `WMTLayerProps` whose device is a live device or 0.
#[unsafe(export_name = "_MetalLayer_setProps")]
pub unsafe extern "C-unwind" fn layer_set_props(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjConstptrNoret>(args) };
    // SAFETY: as above.
    let Some(&props) = (unsafe { mem_ref::<WMTLayerProps>(p.arg) }) else {
        return STATUS_SUCCESS;
    };
    let layer = p.handle;
    on_main(move || {
        // SAFETY: the caller's layer, live while it waits.
        let Some(layer) = (unsafe { obj::<CAMetalLayer>(layer) }) else {
            return;
        };
        // SAFETY: the caller's contract.
        let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(props.device) };
        layer.setDevice(device);
        layer.setOpaque(props.opaque != 0);
        layer.setFramebufferOnly(props.framebuffer_only != 0);
        layer.setContentsScale(props.contents_scale);
        layer.setDisplaySyncEnabled(props.display_sync_enabled != 0);
        layer.setDrawableSize(CGSize::new(props.drawable_width, props.drawable_height));
        layer.setPixelFormat(pixel_format(props.pixel_format));
    });
    STATUS_SUCCESS
}

/// Slot 71, `_MetalLayer_getProps`: the layer's properties (zeros for no layer), its device +0.
///
/// A null props pointer writes nothing (the C wrote through it).
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_ptr_noret`: a live layer or 0, and null or a
/// `WMTLayerProps`.
#[unsafe(export_name = "_MetalLayer_getProps")]
pub unsafe extern "C-unwind" fn layer_get_props(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjPtrNoret>(args) };
    // SAFETY: as above.
    let Some(props) = (unsafe { mem_mut::<WMTLayerProps>(p.arg) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: as above.
    let layer = unsafe { obj::<CAMetalLayer>(p.handle) };
    // SAFETY: as above: the same layer under the declaration of its raw device getter.
    let raw = unsafe { obj::<RawLayer>(p.handle) };
    props.device = raw.map_or(0, |l| ret_passthrough(l.raw_device()));
    props.opaque = layer.is_some_and(|l| l.isOpaque()).into();
    props.framebuffer_only = layer.is_some_and(CAMetalLayer::framebufferOnly).into();
    props.contents_scale = layer.map_or(0.0, |l| l.contentsScale());
    props.display_sync_enabled = layer.is_some_and(CAMetalLayer::displaySyncEnabled).into();
    props.drawable_height = layer.map_or(0.0, |l| l.drawableSize().height);
    props.drawable_width = layer.map_or(0.0, |l| l.drawableSize().width);
    props.pixel_format = layer.map_or(0, |l| usize_to_u32(l.pixelFormat().0));
    STATUS_SUCCESS
}

/// winemac's private data of a window: its view is the fourth field (offset 24).
#[repr(C)]
struct WinData {
    _hwnd: *mut c_void,
    _cocoa_window: *mut c_void,
    _cocoa_view: *mut c_void,
    client_cocoa_view: *mut c_void,
}

type GetWinData = unsafe extern "C-unwind" fn(hwnd: *mut c_void) -> *mut WinData;
type ReleaseWinData = unsafe extern "C-unwind" fn(data: *mut WinData);
type CreateMetalView =
    unsafe extern "C-unwind" fn(view: *mut c_void, device: *mut c_void) -> *mut c_void;
type GetMetalLayer = unsafe extern "C-unwind" fn(view: *mut c_void) -> *mut c_void;
type ReleaseMetalView = unsafe extern "C-unwind" fn(view: *mut c_void);

/// winemac's `macdrv_functions`, to the entries winemetal calls (1, 2, 6, 7 and 8).
#[repr(C)]
struct MacdrvFunctions {
    _init_display_devices: *const c_void,
    get_win_data: Option<GetWinData>,
    release_win_data: Option<ReleaseWinData>,
    _window_and_device: [*const c_void; 3],
    view_create_metal_view: Option<CreateMetalView>,
    view_get_metal_layer: Option<GetMetalLayer>,
    view_release_metal_view: Option<ReleaseMetalView>,
}

/// A symbol any loaded image exports, null when none does.
fn symbol(name: &CStr) -> *mut c_void {
    // SAFETY: a NUL-terminated name.
    unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) }
}

/// winemac's table of functions, `None` where it publishes none (native: its loose symbols).
fn macdrv_functions() -> Option<&'static MacdrvFunctions> {
    // SAFETY: winemac's table, which lives as long as the process.
    unsafe { symbol(c"macdrv_functions").cast::<MacdrvFunctions>().as_ref() }
}

/// Slot 72, `_CreateMetalViewFromHWND`: a Metal view (+1, winemac's) in the window's view.
///
/// Through winemac's `macdrv_functions`, else its loose symbols; with no window data, or no view
/// in it, no view (`ret_view` 0, `ret_layer` untouched).
///
/// # Safety
///
/// `args` is the caller's `unixcall_create_metal_view_from_hwnd`, its device a live device or 0.
#[unsafe(export_name = "_CreateMetalViewFromHWND")]
pub unsafe extern "C-unwind" fn create_metal_view(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallCreateMetalViewFromHwnd>(args) };
    let fns = macdrv_functions().map_or_else(
        || {
            // SAFETY: winemac's function of this signature, or null.
            let get = unsafe { transmute::<*mut c_void, Option<GetWinData>>(symbol(c"get_win_data")) };
            // SAFETY: as above.
            let release =
                unsafe { transmute::<*mut c_void, Option<ReleaseWinData>>(symbol(c"release_win_data")) };
            let create = symbol(c"macdrv_view_create_metal_view");
            // SAFETY: as above.
            let create = unsafe { transmute::<*mut c_void, Option<CreateMetalView>>(create) };
            let layer_of = symbol(c"macdrv_view_get_metal_layer");
            // SAFETY: as above.
            let layer_of = unsafe { transmute::<*mut c_void, Option<GetMetalLayer>>(layer_of) };
            (get, release, create, layer_of)
        },
        |t| (t.get_win_data, t.release_win_data, t.view_create_metal_view, t.view_get_metal_layer),
    );
    let (Some(get), Some(release), Some(create), Some(layer_of)) = fns else {
        return STATUS_SUCCESS;
    };
    // An HWND is a handle value, not an address.
    let hwnd = ptr::without_provenance_mut(u64_to_usize(p.hwnd));
    // SAFETY: winemac's function, on any HWND.
    let data = unsafe { get(hwnd) };
    // SAFETY: null or winemac's data for the window, live until it is released.
    let client = unsafe { data.as_ref() }.map_or(ptr::null_mut(), |d| d.client_cocoa_view);
    let view = if client.is_null() {
        ptr::null_mut()
    } else {
        let device = ptr::with_exposed_provenance_mut(u64_to_usize(p.device));
        // SAFETY: the window's view and the caller's device or null.
        unsafe { create(client, device) }
    };
    p.ret_view = ret_passthrough(view.cast());
    if !view.is_null() {
        // SAFETY: the view winemac just made.
        p.ret_layer = ret_passthrough(unsafe { layer_of(view) }.cast());
    }
    if !data.is_null() {
        // SAFETY: the data `get` handed out.
        unsafe { release(data) };
    }
    STATUS_SUCCESS
}

/// Slot 73, `_ReleaseMetalView`: winemac releases a view slot 72 made.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_noret`, its handle a view of slot 72 or 0.
#[unsafe(export_name = "_ReleaseMetalView")]
pub unsafe extern "C-unwind" fn release_metal_view(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjNoret>(args) };
    let release = macdrv_functions().map_or_else(
        || {
            let release = symbol(c"macdrv_view_release_metal_view");
            // SAFETY: winemac's function of this signature, or null.
            unsafe { transmute::<*mut c_void, Option<ReleaseMetalView>>(release) }
        },
        |t| t.view_release_metal_view,
    );
    if let Some(release) = release {
        // SAFETY: the caller's view or null, as winemac takes it.
        unsafe { release(ptr::with_exposed_provenance_mut(u64_to_usize(p.handle))) };
    }
    STATUS_SUCCESS
}

/// The name of the CoreGraphics colour space a `WMTColorSpace` stands for, `None` for none.
fn colorspace_name(colorspace: u64) -> Option<&'static CFString> {
    match colorspace {
        // SAFETY: CoreGraphics' constant.
        WMT_COLOR_SPACE_SRGB => Some(unsafe { kCGColorSpaceSRGB }),
        WMT_COLOR_SPACE_SRGB_LINEAR | WMT_COLOR_SPACE_HDR_SC_RGB => {
            // SAFETY: as above.
            Some(unsafe { kCGColorSpaceExtendedLinearSRGB })
        }
        // SAFETY: as above.
        WMT_COLOR_SPACE_BT2020 => Some(unsafe { kCGColorSpaceITUR_2020_sRGBGamma }),
        // SAFETY: as above.
        WMT_COLOR_SPACE_HDR_PQ => Some(unsafe { kCGColorSpaceITUR_2100_PQ }),
        _ => None,
    }
}

/// The colour space a `WMTColorSpace` stands for, `None` for none or when CoreGraphics makes none.
fn colorspace(colorspace: u64) -> Option<objc2_core_foundation::CFRetained<CGColorSpace>> {
    CGColorSpace::with_name(Some(colorspace_name(colorspace)?))
}

/// Slot 92, `_CGColorSpace_checkColorSpaceSupported`: whether CoreGraphics makes the colour space.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_uint64_ret`, the colour space in its handle.
#[unsafe(export_name = "_CGColorSpace_checkColorSpaceSupported")]
pub unsafe extern "C-unwind" fn colorspace_supported(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Ret>(args) };
    p.ret = 0;
    p.ret = colorspace(p.handle).is_some().into();
    STATUS_SUCCESS
}

/// Slot 93, `_MetalLayer_setColorSpace`: the layer's colour space and EDR wish, set on main.
///
/// An HDR colour space (`WMT_COLORSPACE_IS_HDR`) wants extended dynamic range content.
///
/// # Safety
///
/// `args` is the caller's `unixcall_generic_obj_uint64_uint64_ret`, its handle a live layer or 0.
#[unsafe(export_name = "_MetalLayer_setColorSpace")]
pub unsafe extern "C-unwind" fn layer_set_colorspace(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Uint64Ret>(args) };
    let cs = p.arg;
    p.ret = 0;
    let Some(space) = colorspace(cs) else {
        return STATUS_SUCCESS;
    };
    let layer = p.handle;
    let space_addr = ptr::from_ref::<CGColorSpace>(&space).expose_provenance();
    on_main(move || {
        // SAFETY: the caller's layer, live while it waits.
        let Some(layer) = (unsafe { obj::<CAMetalLayer>(layer) }) else {
            return;
        };
        // SAFETY: `space`, which the caller holds until this returns.
        let space = unsafe { &*ptr::with_exposed_provenance::<CGColorSpace>(space_addr) };
        layer.setColorspace(Some(space));
        layer.setWantsExtendedDynamicRangeContent(cs & 0b100 != 0);
    });
    p.ret = 1;
    STATUS_SUCCESS
}

/// `WMTSetWsiHooks`'s hooks: the native host's, set once before any device (native only).
#[cfg(feature = "native")]
static WSI_HOOKS: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

/// `WMTSetWsiHooks`: keeps the native host's window-system hooks for its later reads.
#[cfg(feature = "native")]
#[unsafe(export_name = "WMTSetWsiHooks")]
pub extern "C-unwind" fn set_wsi_hooks(hooks: *const c_void) {
    let _guard = AbortOnPanic;
    WSI_HOOKS.store(hooks.cast_mut(), Ordering::Release);
}

/// `WMTGetWsiHooks`: the hooks `WMTSetWsiHooks` kept, null for none.
#[cfg(feature = "native")]
#[unsafe(export_name = "WMTGetWsiHooks")]
pub extern "C-unwind" fn wsi_hooks() -> *const c_void {
    let _guard = AbortOnPanic;
    WSI_HOOKS.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests;
