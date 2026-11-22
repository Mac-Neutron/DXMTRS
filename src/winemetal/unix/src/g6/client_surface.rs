//! Wine client-surface lookup, hook bypass, and `WineContentView` `hidden` KVO watch.
//!
//! `WMTAttachMetalView` converts `WineMetalLayer` back to plain `CAMetalLayer` when win32u
//! provides client-surface calls and `WineContentView` carries a client-surface ivar, then
//! watches `hidden` on the parent client view so slot 161 latches `CLIENT_SURFACE_DIRTY`.

use core::{
    ffi::{CStr, c_void},
    mem::transmute,
};
use std::sync::Arc;

use objc2::{
    ClassType, Encode, MainThreadMarker, MainThreadOnly, define_class, extern_methods,
    rc::{Retained, autoreleasepool},
    runtime::{AnyClass, AnyObject, Ivar, NSObject},
};
use objc2_app_kit::NSView;
use objc2_foundation::{
    NSKeyValueObservingOptions, NSObjectNSKeyValueObserverRegistration, NSString,
};
use objc2_quartz_core::CAMetalLayer;

use super::attachment::{self, Attachment};
use crate::ffi::AbortOnPanic;

const HOOK_LAYER_CLASS: &CStr = c"WineMetalLayer";
const CLIENT_VIEW_CLASS: &CStr = c"WineContentView";
const CLIENT_SURFACE_IVARS: [&CStr; 2] = [c"d3dmetal_client_surface", c"dxmt_client_surface"];
const HIDDEN_KEY: &str = "hidden";

type SurfaceFn = unsafe extern "C" fn(*mut c_void);

/// win32u's client-surface reference and present functions, when exported by Wine.
pub struct SurfaceCalls {
    add_ref: SurfaceFn,
    present: SurfaceFn,
    release: SurfaceFn,
}

impl SurfaceCalls {
    /// Load win32u's client-surface exports from the process symbol table.
    #[must_use]
    pub fn load() -> Option<Self> {
        Some(Self {
            add_ref: surface_fn(c"client_surface_add_ref")?,
            present: surface_fn(c"client_surface_present")?,
            release: surface_fn(c"client_surface_release")?,
        })
    }

    /// Take a reference on `surface`; `0` returns `0`.
    #[must_use]
    pub fn retain(&self, surface: usize) -> usize {
        if surface != 0 {
            // SAFETY: `surface` is a live client surface read under `get_win_data` or retained by
            // a live attachment record.
            unsafe { (self.add_ref)(surface as *mut c_void) };
        }
        surface
    }

    /// Present `surface` as its window's active client view; `0` is a no-op.
    pub fn present(&self, surface: usize) {
        if surface != 0 {
            // SAFETY: caller holds a reference on `surface`; win32u checks window attachment
            // under its surface lock.
            unsafe { (self.present)(surface as *mut c_void) };
        }
    }

    /// Release a reference taken by [`Self::retain`]; `0` is a no-op.
    pub fn release(&self, surface: usize) {
        if surface != 0 {
            // SAFETY: caller hands back one reference taken by `add_ref`.
            unsafe { (self.release)(surface as *mut c_void) };
        }
    }
}

fn surface_fn(name: &CStr) -> Option<SurfaceFn> {
    // SAFETY: NUL-terminated static symbol name passed to `dlsym`.
    let sym = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
    if sym.is_null() {
        return None;
    }
    // SAFETY: non-null win32u client-surface export matching `unsafe extern "C" fn(*mut c_void)`.
    Some(unsafe { transmute::<*mut c_void, SurfaceFn>(sym) })
}

/// Reset a `WineMetalLayer` subclass back to plain `CAMetalLayer` on the main thread when safe.
pub fn bypass_present_hook(layer: *mut c_void, can_present: bool, _mtm: MainThreadMarker) {
    let Some(hook) = AnyClass::get(HOOK_LAYER_CLASS) else {
        return;
    };
    if !can_present
        || AnyClass::get(CLIENT_VIEW_CLASS)
            .and_then(surface_ivar)
            .is_none()
    {
        return;
    }
    // SAFETY: `layer` comes from a live metal view; retaining keeps it valid through `object_setClass`.
    let Some(layer) = (unsafe { Retained::retain(layer.cast::<CAMetalLayer>()) }) else {
        return;
    };
    let object: &AnyObject = layer.as_ref();
    if object.class() != hook {
        return;
    }
    // SAFETY: `WineMetalLayer` extends `CAMetalLayer` with no extra ivars; switching class to
    // `CAMetalLayer` bypasses its off-main `nextDrawable` override safely.
    unsafe {
        objc2::ffi::object_setClass(
            core::ptr::from_ref(object).cast_mut(),
            CAMetalLayer::class(),
        );
    }
}

/// Read the client surface pointer stored on `client_view`. **Main thread only.**
#[must_use]
pub fn client_view_surface(client_view: *mut c_void, _mtm: MainThreadMarker) -> usize {
    let Some(host) = AnyClass::get(CLIENT_VIEW_CLASS) else {
        return 0;
    };
    // SAFETY: `client_view` is read from a locked `WinData` record and retained for this call.
    let Some(client_view) = (unsafe { Retained::retain(client_view.cast::<AnyObject>()) }) else {
        return 0;
    };
    surface_field(&client_view, host)
}

fn surface_field(object: &AnyObject, host: &'static AnyClass) -> usize {
    let mut class = Some(object.class());
    while let Some(current) = class {
        if current == host {
            break;
        }
        class = current.superclass();
    }
    if class.is_none() {
        return 0;
    }
    let Some(ivar) = surface_ivar(host) else {
        return 0;
    };
    // SAFETY: `object` inherits from `WineContentView` and `ivar` has pointer encoding.
    let surface = unsafe { *ivar.load::<*mut c_void>(object) };
    surface as usize
}

fn surface_ivar(host: &'static AnyClass) -> Option<&'static Ivar> {
    let ivar = CLIENT_SURFACE_IVARS
        .iter()
        .find_map(|name| host.instance_variable(name))?;
    let encoding = ivar.type_encoding().to_str().unwrap_or_default();
    if !<*mut c_void>::ENCODING.equivalent_to_str(encoding) {
        return None;
    }
    Some(ivar)
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "WmtClientViewObserver"]
    struct ClientViewObserver;

    impl ClientViewObserver {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe_value(
            &self,
            _key_path: Option<&NSString>,
            object: Option<&AnyObject>,
            _change: Option<&AnyObject>,
            context: *mut c_void,
        ) {
            autoreleasepool(|_| {
                let _guard = AbortOnPanic;
                client_view_changed(object, context.addr());
            });
        }
    }
);

impl ClientViewObserver {
    extern_methods!(
        #[unsafe(method(new))]
        #[unsafe(method_family = new)]
        fn new(mtm: MainThreadMarker) -> Retained<Self>;
    );
}

struct ClientViewWatch {
    client_view: Retained<NSView>,
    observer: Retained<ClientViewObserver>,
}

/// Start observing `hidden` on `view`'s parent client view. **Main thread only.**
pub fn watch(att: &Arc<Attachment>, view: &NSView, mtm: MainThreadMarker) {
    // SAFETY: called on main thread while `view` is retained and attached.
    let Some(client_view) = (unsafe { view.superview() }) else {
        return;
    };
    let observer = ClientViewObserver::new(mtm);
    let key = NSString::from_str(HIDDEN_KEY);
    // SAFETY: `observer` is kept alive alongside `client_view` in `ClientViewWatch` until
    // `remove` unregisters it; `context` is an integer lookup key never dereferenced.
    unsafe {
        client_view.addObserver_forKeyPath_options_context(
            &observer,
            &key,
            NSKeyValueObservingOptions::empty(),
            core::ptr::without_provenance_mut(att.view()),
        );
    }
    let hidden_at_install = client_view.isHidden();
    let watch = Box::new(ClientViewWatch {
        client_view,
        observer,
    });
    let prev = att.replace_client_view_watch(Box::into_raw(watch) as usize);
    remove(prev, mtm);
    if hidden_at_install {
        let _ = attachment::request_client_surface(att.view());
    }
}

/// Schedule main-thread removal of `att`'s client-view KVO watch without blocking.
pub fn unwatch(att: &Attachment) {
    let address = att.take_client_view_watch();
    if address == 0 {
        return;
    }
    super::window::run_on_main_thread_async(move || {
        if let Some(mtm) = MainThreadMarker::new() {
            remove(address, mtm);
        }
    });
}

fn remove(address: usize, _mtm: MainThreadMarker) {
    if address == 0 {
        return;
    }
    // SAFETY: `address` came from `Box::into_raw` in `watch` and was claimed by one atomic swap.
    let watch = unsafe { Box::from_raw(address as *mut ClientViewWatch) };
    let key = NSString::from_str(HIDDEN_KEY);
    // SAFETY: `observer` was registered on `client_view` for `HIDDEN_KEY` and kept alive by `watch`.
    unsafe {
        watch
            .client_view
            .removeObserver_forKeyPath(&watch.observer, &key);
    }
}

fn client_view_changed(object: Option<&AnyObject>, view: usize) {
    if MainThreadMarker::new().is_none() {
        let _ = attachment::request_client_surface(view);
        return;
    }
    let Some(client_view) = object.and_then(AnyObject::downcast_ref::<NSView>) else {
        return;
    };
    if client_view.isHidden() {
        let _ = attachment::request_client_surface(view);
    }
}
